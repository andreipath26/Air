//! Explicit, offline AIRW compaction.
//!
//! Prototype sections use kind "hdc-10000-le" and contain 157 little-endian
//! u64 words (the top 48 padding bits must be zero). Rule sections may opt in
//! to pruning with kind "hit-counted-rule-v1" and JSON HitCountedRuleRecord
//! metadata. Unknown section kinds are copied byte-for-byte.

use crate::{ArchiveBuilder, ArchiveError, ArchiveReader, MAX_PAYLOAD_BYTES};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{collections::{BTreeMap, BTreeSet}, error::Error, fmt};

pub const HDC_PROTOTYPE_KIND: &str = "hdc-10000-le";
pub const HIT_COUNTED_RULE_KIND: &str = "hit-counted-rule-v1";
const HDC_BITS: usize = 10_000;
const HDC_WORDS: usize = 157;
const HDC_BYTES: usize = HDC_WORDS * 8;
const HDC_LAST_WORD_MASK: u64 = 0xffff;
const MAX_INPUT_ARCHIVES: usize = 64;
const MAX_COMPACTION_INPUT_BYTES: usize = MAX_PAYLOAD_BYTES;
const MAX_SECTIONS_PER_COMPACTION: usize = 16_384;

/// Usage data attached to an opt-in hit-counted rule section.
///
/// Compaction only drops a rule when both pruning options are supplied and the
/// rule has fewer than the hit threshold and was last used strictly before the
/// explicit cutoff. No clock is read and absent metadata is never guessed.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct HitCountedRuleRecord {
    pub hit_count: u64,
    pub last_hit_unix_ms: u64,
    pub rule_bytes: Vec<u8>,
}

impl HitCountedRuleRecord {
    pub fn encode(&self) -> Result<Vec<u8>, serde_json::Error> {
        serde_json::to_vec(self)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CompactionPolicy {
    /// Valid AIRW package token to write into the compacted archive.
    pub package: String,
    /// Bundle same-name prototypes when similarity to the first sorted member
    /// of their group is at least this many basis points (0..=10,000).
    pub minimum_prototype_similarity_basis_points: u16,
    /// Optional minimum hit count. Must be paired with an explicit cutoff to
    /// enable rule pruning.
    pub minimum_rule_hits: Option<u64>,
    /// Optional caller-supplied Unix-millisecond cutoff. Must be paired with
    /// minimum_rule_hits to enable rule pruning.
    pub prune_before_unix_ms: Option<u64>,
}

impl Default for CompactionPolicy {
    fn default() -> Self {
        Self {
            package: "air.compacted".into(),
            minimum_prototype_similarity_basis_points: 9_800,
            minimum_rule_hits: None,
            prune_before_unix_ms: None,
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct CompactionStats {
    pub input_archives: usize,
    pub input_sections: usize,
    pub output_sections: usize,
    pub bundled_prototypes: usize,
    pub pruned_rules: usize,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CompactionResult {
    pub archive: Vec<u8>,
    pub stats: CompactionStats,
}

#[derive(Debug)]
pub enum CompactionError {
    Archive(ArchiveError),
    InvalidPolicy(&'static str),
    InputCount,
    InputBytes,
    SectionCount,
    EmptyOutput,
    InvalidPrototype,
    InvalidRuleMetadata,
}

impl fmt::Display for CompactionError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Archive(e) => write!(f, "AIRW compaction archive error: {e}"),
            Self::InvalidPolicy(s) => write!(f, "invalid compaction policy: {s}"),
            Self::InputCount => f.write_str("AIRW compaction input count exceeds limit"),
            Self::InputBytes => f.write_str("AIRW compaction input bytes exceed limit"),
            Self::SectionCount => f.write_str("AIRW compaction section count exceeds limit"),
            Self::EmptyOutput => f.write_str("AIRW compaction would produce an empty archive"),
            Self::InvalidPrototype => f.write_str("invalid 10,000-bit prototype encoding"),
            Self::InvalidRuleMetadata => f.write_str("invalid hit-counted rule metadata"),
        }
    }
}
impl Error for CompactionError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self { Self::Archive(e) => Some(e), _ => None }
    }
}
impl From<ArchiveError> for CompactionError {
    fn from(value: ArchiveError) -> Self { Self::Archive(value) }
}

#[derive(Clone)]
struct InputArchive<'a> {
    digest: String,
    bytes: &'a [u8],
}

#[derive(Clone)]
struct OutputSection {
    name: String,
    kind: String,
    bytes: Vec<u8>,
}

#[derive(Clone)]
struct PackedPrototype([u64; HDC_WORDS]);

impl PackedPrototype {
    fn decode(bytes: &[u8]) -> Result<Self, CompactionError> {
        if bytes.len() != HDC_BYTES { return Err(CompactionError::InvalidPrototype); }
        let mut words = [0u64; HDC_WORDS];
        for (word, chunk) in words.iter_mut().zip(bytes.chunks_exact(8)) {
            let raw: [u8; 8] = chunk.try_into().map_err(|_| CompactionError::InvalidPrototype)?;
            *word = u64::from_le_bytes(raw);
        }
        if words[HDC_WORDS - 1] & !HDC_LAST_WORD_MASK != 0 {
            return Err(CompactionError::InvalidPrototype);
        }
        Ok(Self(words))
    }

    fn encode(&self) -> Vec<u8> {
        let mut bytes = Vec::with_capacity(HDC_BYTES);
        for word in self.0 { bytes.extend_from_slice(&word.to_le_bytes()); }
        bytes
    }

    fn similarity_basis_points(&self, other: &Self) -> u16 {
        let mut distance = 0u32;
        for i in 0..HDC_WORDS {
            distance += (self.0[i] ^ other.0[i]).count_ones();
        }
        // Padding is validated on decode and masked on bundle, so this is the
        // Hamming similarity on exactly 10,000 logical bits.
        (HDC_BITS as u32 - distance) as u16
    }

    fn bundle(vectors: &[Self]) -> Result<Self, CompactionError> {
        if vectors.is_empty() { return Err(CompactionError::InvalidPrototype); }
        let mut counts = vec![0u32; HDC_BITS];
        for vector in vectors {
            for (word_index, word) in vector.0.iter().enumerate() {
                let mut bits = *word;
                while bits != 0 {
                    let bit = bits.trailing_zeros() as usize;
                    let logical = word_index * 64 + bit;
                    if logical < HDC_BITS {
                        counts[logical] = counts[logical].checked_add(1)
                            .ok_or(CompactionError::InvalidPrototype)?;
                    }
                    bits &= bits - 1;
                }
            }
        }
        let mut output = Self([0u64; HDC_WORDS]);
        for (index, count) in counts.into_iter().enumerate() {
            if u64::from(count) * 2 > vectors.len() as u64 {
                output.0[index / 64] |= 1u64 << (index % 64);
            }
        }
        output.0[HDC_WORDS - 1] &= HDC_LAST_WORD_MASK;
        Ok(output)
    }
}

struct PrototypeGroup {
    output_name: String,
    kind: String,
    anchor: PackedPrototype,
    members: Vec<PackedPrototype>,
}

fn digest_hex(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    let mut out = String::with_capacity(64);
    for byte in digest { out.push_str(&format!("{byte:02x}")); }
    out
}

fn allocate_name(base: &str, source_digest: &str, ordinal: usize, used: &mut BTreeSet<String>)
    -> Result<String, CompactionError>
{
    if base.is_empty() || base.len() > 128
        || !base.bytes().all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.' | b':'))
    {
        return Err(CompactionError::InvalidPolicy("invalid section name"));
    }
    if !used.contains(base) {
        used.insert(base.to_owned());
        return Ok(base.to_owned());
    }
    for collision in 0usize.. {
        let suffix = format!(".m{}.{}.{}", &source_digest[..12], ordinal, collision);
        let base_len = 128usize.saturating_sub(suffix.len());
        if base_len == 0 { return Err(CompactionError::InvalidPolicy("section name namespace exhausted")); }
        let candidate = format!("{}{}", &base[..base.len().min(base_len)], suffix);
        if used.insert(candidate.clone()) { return Ok(candidate); }
    }
    unreachable!()
}

/// Compact one or more valid AIRW archives into a deterministic archive.
///
/// This is an explicit offline operation: it starts no worker, reads no clock,
/// writes no files, and assumes no idle state. The caller chooses when to call
/// it and supplies the rule-pruning cutoff. Unknown sections are preserved
/// byte-for-byte. The output payload is repacked contiguously through
/// `ArchiveBuilder`. Only sections explicitly typed as the two formats documented
/// above receive special treatment. Prototype compatibility is intentionally
/// tied to the `hdc-10000-le` encoding documented at the top of this module.
pub fn compact_archives(
    archives: &[&[u8]],
    policy: &CompactionPolicy,
) -> Result<CompactionResult, CompactionError> {
    if archives.is_empty() { return Err(CompactionError::InputCount); }
    if archives.len() > MAX_INPUT_ARCHIVES { return Err(CompactionError::InputCount); }
    if policy.package.is_empty() || policy.package.len() > 128
        || !policy.package.bytes().all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.' | b':'))
    {
        return Err(CompactionError::InvalidPolicy("invalid package"));
    }
    if policy.minimum_prototype_similarity_basis_points > 10_000 {
        return Err(CompactionError::InvalidPolicy("prototype similarity must be in 0..=10000"));
    }
    let input_bytes = archives.iter().try_fold(0usize, |total, bytes| total.checked_add(bytes.len()))
        .ok_or(CompactionError::InputBytes)?;
    if input_bytes > MAX_COMPACTION_INPUT_BYTES { return Err(CompactionError::InputBytes); }

    let mut ordered: Vec<InputArchive<'_>> = archives.iter()
        .map(|bytes| InputArchive { digest: digest_hex(bytes), bytes })
        .collect();
    ordered.sort_by(|a, b| a.digest.cmp(&b.digest).then_with(|| a.bytes.len().cmp(&b.bytes.len())));

    let mut languages = BTreeSet::new();
    let mut used_names = BTreeSet::new();
    let mut sections: Vec<OutputSection> = Vec::new();
    let mut prototype_groups: BTreeMap<(String, String), Vec<PrototypeGroup>> = BTreeMap::new();
    let mut header_flags = 0u16;
    let mut stats = CompactionStats { input_archives: ordered.len(), ..CompactionStats::default() };
    let mut section_ordinal = 0usize;

    for source in &ordered {
        let reader = ArchiveReader::from_bytes(source.bytes.to_vec())?;
        header_flags |= reader.header_flags();
        for language in &reader.manifest().languages { languages.insert(language.clone()); }

        let mut descriptors = reader.manifest().sections.clone();
        descriptors.sort_by(|a, b| a.name.cmp(&b.name).then_with(|| a.kind.cmp(&b.kind)));
        stats.input_sections = stats.input_sections.checked_add(descriptors.len())
            .ok_or(CompactionError::SectionCount)?;
        if stats.input_sections > MAX_SECTIONS_PER_COMPACTION { return Err(CompactionError::SectionCount); }

        for descriptor in descriptors {
            section_ordinal = section_ordinal.checked_add(1).ok_or(CompactionError::SectionCount)?;
            let data = reader.section(&descriptor.name)
                .ok_or(CompactionError::Archive(ArchiveError::Invalid("section unavailable")))?
                .to_vec();

            if descriptor.kind == HIT_COUNTED_RULE_KIND {
                let usage: HitCountedRuleRecord = serde_json::from_slice(&data)
                    .map_err(|_| CompactionError::InvalidRuleMetadata)?;
                let should_prune = match (policy.minimum_rule_hits, policy.prune_before_unix_ms) {
                    (Some(min_hits), Some(cutoff)) => usage.hit_count < min_hits && usage.last_hit_unix_ms < cutoff,
                    _ => false,
                };
                if should_prune {
                    stats.pruned_rules += 1;
                    continue;
                }
            }

            if descriptor.kind == HDC_PROTOTYPE_KIND {
                let vector = PackedPrototype::decode(&data)?;
                let key = (descriptor.name.clone(), descriptor.kind.clone());
                let groups = prototype_groups.entry(key).or_default();
                if let Some(group) = groups.iter_mut().find(|group|
                    group.anchor.similarity_basis_points(&vector)
                        >= policy.minimum_prototype_similarity_basis_points
                ) {
                    group.members.push(vector);
                    stats.bundled_prototypes += 1;
                } else {
                    let output_name = allocate_name(
                        &descriptor.name, &source.digest, section_ordinal, &mut used_names
                    )?;
                    groups.push(PrototypeGroup {
                        output_name,
                        kind: descriptor.kind,
                        anchor: vector.clone(),
                        members: vec![vector],
                    });
                }
                continue;
            }

            let name = allocate_name(
                &descriptor.name, &source.digest, section_ordinal, &mut used_names
            )?;
            sections.push(OutputSection { name, kind: descriptor.kind, bytes: data });
        }
    }

    for (_, groups) in prototype_groups {
        for group in groups {
            let bundled = PackedPrototype::bundle(&group.members)?;
            sections.push(OutputSection {
                name: group.output_name,
                kind: group.kind,
                bytes: bundled.encode(),
            });
        }
    }
    if sections.is_empty() { return Err(CompactionError::EmptyOutput); }
    sections.sort_by(|a, b| a.name.cmp(&b.name).then_with(|| a.kind.cmp(&b.kind)));

    let mut builder = ArchiveBuilder::new(policy.package.clone());
    for language in languages { builder.add_language(language)?; }
    builder.set_header_flags(header_flags)?;
    for section in &sections {
        builder.add_section(section.name.clone(), section.kind.clone(), &section.bytes)?;
    }
    let archive = builder.finish()?;
    stats.output_sections = sections.len();
    Ok(CompactionResult { archive, stats })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ArchiveBuilder;

    fn make_archive(package: &str, sections: &[(&str, &str, &[u8])]) -> Vec<u8> {
        let mut builder = ArchiveBuilder::new(package);
        builder.add_language("rust").unwrap();
        for (name, kind, bytes) in sections {
            builder.add_section(*name, *kind, bytes).unwrap();
        }
        builder.finish().unwrap()
    }

    fn prototype(bits: &[usize]) -> Vec<u8> {
        let mut words = [0u64; HDC_WORDS];
        for bit in bits {
            words[*bit / 64] |= 1u64 << (*bit % 64);
        }
        PackedPrototype(words).encode()
    }

    fn usage(hit_count: u64, last_hit_unix_ms: u64) -> Vec<u8> {
        HitCountedRuleRecord { hit_count, last_hit_unix_ms, rule_bytes: vec![1, 2, 3] }
            .encode().unwrap()
    }

    #[test]
    fn merge_is_deterministic_across_input_order_and_packs_contiguously() {
        let a = make_archive("delta.a", &[("rule", "ast-rule", b"a")]);
        let b = make_archive("delta.b", &[("rule", "ast-rule", b"b"), ("other", "ast-rule", b"c")]);
        let first = compact_archives(&[a.as_slice(), b.as_slice()], &CompactionPolicy::default()).unwrap();
        let second = compact_archives(&[b.as_slice(), a.as_slice()], &CompactionPolicy::default()).unwrap();
        assert_eq!(first.archive, second.archive);
        let reader = ArchiveReader::from_bytes(first.archive.clone()).unwrap();
        assert_eq!(reader.section_names().count(), 3);
        assert!(reader.section("rule").is_some());
        assert_eq!(reader.manifest().sections.first().unwrap().offset, 0);
        assert!(reader.manifest().sections.windows(2).all(|pair|
            pair[0].offset + pair[0].length == pair[1].offset
        ));
    }

    #[test]
    fn similar_ten_thousand_bit_prototypes_majority_bundle_and_remain_similar() {
        let base = prototype(&[1, 7, 63, 100, 9999]);
        let near = prototype(&[1, 7, 63, 101, 9999]);
        let a = make_archive("delta.a", &[("rust.proto.ownership", HDC_PROTOTYPE_KIND, &base)]);
        let b = make_archive("delta.b", &[("rust.proto.ownership", HDC_PROTOTYPE_KIND, &base)]);
        let c = make_archive("delta.c", &[("rust.proto.ownership", HDC_PROTOTYPE_KIND, &near)]);
        let policy = CompactionPolicy { minimum_prototype_similarity_basis_points: 9_900, ..Default::default() };
        let result = compact_archives(&[a.as_slice(), b.as_slice(), c.as_slice()], &policy).unwrap();
        assert_eq!(result.stats.bundled_prototypes, 2);
        assert_eq!(result.stats.output_sections, 1);
        let reader = ArchiveReader::from_bytes(result.archive).unwrap();
        let bundled = PackedPrototype::decode(reader.section("rust.proto.ownership").unwrap()).unwrap();
        let original = PackedPrototype::decode(&base).unwrap();
        assert!(original.similarity_basis_points(&bundled) >= 9_900);
    }

    #[test]
    fn dissimilar_same_name_prototypes_are_preserved_separately() {
        let a = prototype(&[0, 1, 2, 3]);
        let mut b = vec![0u8; HDC_BYTES];
        for bit in (0..1_000).step_by(2) {
            b[bit / 8] |= 1 << (bit % 8);
        }
        let left = make_archive("left", &[("route", HDC_PROTOTYPE_KIND, &a)]);
        let right = make_archive("right", &[("route", HDC_PROTOTYPE_KIND, &b)]);
        let policy = CompactionPolicy { minimum_prototype_similarity_basis_points: 9_900, ..Default::default() };
        let result = compact_archives(&[left.as_slice(), right.as_slice()], &policy).unwrap();
        assert_eq!(result.stats.output_sections, 2);
        let reader = ArchiveReader::from_bytes(result.archive).unwrap();
        assert_eq!(reader.section_names().count(), 2);
    }

    #[test]
    fn prunes_only_low_hit_rules_older_than_explicit_cutoff() {
        let stale_low = usage(2, 99);
        let fresh_low = usage(2, 100);
        let stale_high = usage(5, 99);
        let archive = make_archive("delta", &[
            ("stale-low", HIT_COUNTED_RULE_KIND, &stale_low),
            ("fresh-low", HIT_COUNTED_RULE_KIND, &fresh_low),
            ("stale-high", HIT_COUNTED_RULE_KIND, &stale_high),
        ]);
        let policy = CompactionPolicy {
            minimum_rule_hits: Some(3),
            prune_before_unix_ms: Some(100),
            ..Default::default()
        };
        let result = compact_archives(&[archive.as_slice()], &policy).unwrap();
        assert_eq!(result.stats.pruned_rules, 1);
        let reader = ArchiveReader::from_bytes(result.archive).unwrap();
        assert!(reader.section("stale-low").is_none());
        assert!(reader.section("fresh-low").is_some());
        assert!(reader.section("stale-high").is_some());
    }

    #[test]
    fn missing_cutoff_never_prunes_and_corrupt_metadata_fails_closed() {
        let old_low = usage(1, 1);
        let archive = make_archive("delta", &[("rule", HIT_COUNTED_RULE_KIND, &old_low)]);
        let policy = CompactionPolicy { minimum_rule_hits: Some(10), ..Default::default() };
        let result = compact_archives(&[archive.as_slice()], &policy).unwrap();
        assert_eq!(result.stats.pruned_rules, 0);

        let corrupt = make_archive("bad", &[("rule", HIT_COUNTED_RULE_KIND, b"{")]);
        assert!(matches!(
            compact_archives(&[corrupt.as_slice()], &CompactionPolicy::default()),
            Err(CompactionError::InvalidRuleMetadata)
        ));
    }

    #[test]
    fn rejects_bad_prototype_padding_and_corrupt_archives() {
        let mut malformed = vec![0u8; HDC_BYTES];
        malformed[HDC_BYTES - 1] = 0x80;
        let archive = make_archive("bad", &[("proto", HDC_PROTOTYPE_KIND, &malformed)]);
        assert!(matches!(
            compact_archives(&[archive.as_slice()], &CompactionPolicy::default()),
            Err(CompactionError::InvalidPrototype)
        ));
        assert!(matches!(
            compact_archives(&[b"not an archive"], &CompactionPolicy::default()),
            Err(CompactionError::Archive(_))
        ));
    }

    #[test]
    fn explicit_policy_bounds_and_empty_inputs_are_rejected() {
        let archive = make_archive("delta", &[("rule", "ast-rule", b"x")]);
        let policy = CompactionPolicy { minimum_prototype_similarity_basis_points: 10_001, ..Default::default() };
        assert!(matches!(
            compact_archives(&[archive.as_slice()], &policy),
            Err(CompactionError::InvalidPolicy(_))
        ));
        assert!(matches!(compact_archives(&[], &CompactionPolicy::default()), Err(CompactionError::InputCount)));
    }
}
