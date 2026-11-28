//! Versioned AIRW archive container. File-backed readers own a read-only mmap;
//! byte-buffer readers own their Vec. Validated section access borrows the backing storage.

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use memmap2::{Mmap, MmapOptions};

use std::{collections::BTreeSet, error::Error, fmt, fs::File, io, path::Path};

pub mod dependency;
pub use dependency::{
    DependencyEdge, DependencyEdgeKind, DependencyGraph, DEPENDENCY_GRAPH_KIND,
    DEPENDENCY_GRAPH_SECTION,
};

pub const MAGIC: [u8; 4] = *b"AIRW";
pub const FORMAT_VERSION: u16 = 1;
/// Known v1 feature bit: one or more sections use BITCOS sparse ternary encoding.
pub const AIRW_FLAG_BITCOS_TERNARY: u16 = 0x0001;
const KNOWN_AIRW_FLAGS: u16 = AIRW_FLAG_BITCOS_TERNARY;
pub const HEADER_LEN: usize = 52;
pub const MAX_MANIFEST_BYTES: usize = 1024 * 1024;
pub const MAX_PAYLOAD_BYTES: usize = 512 * 1024 * 1024;
pub const MAX_SECTIONS: usize = 4096;
pub const MAX_LANGUAGES: usize = 64;

#[derive(Debug)]
pub enum ArchiveError {
    Io(io::Error),
    Json(serde_json::Error),
    Invalid(&'static str),
    Limit(&'static str),
    UnsupportedVersion(u16),
    ChecksumMismatch,
}
impl fmt::Display for ArchiveError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(e) => write!(f, "archive I/O error: {e}"),
            Self::Json(e) => write!(f, "archive manifest error: {e}"),
            Self::Invalid(s) => write!(f, "invalid archive: {s}"),
            Self::Limit(s) => write!(f, "archive limit exceeded: {s}"),
            Self::UnsupportedVersion(v) => write!(f, "unsupported AIRW version: {v}"),
            Self::ChecksumMismatch => f.write_str("archive checksum mismatch"),
        }
    }
}
impl Error for ArchiveError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self { Self::Io(e) => Some(e), Self::Json(e) => Some(e), _ => None }
    }
}
impl From<io::Error> for ArchiveError {
    fn from(e: io::Error) -> Self { Self::Io(e) }
}
impl From<serde_json::Error> for ArchiveError {
    fn from(e: serde_json::Error) -> Self { Self::Json(e) }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct SectionDescriptor {
    pub name: String,
    pub kind: String,
    pub offset: u64,
    pub length: u64,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Manifest {
    pub package: String,
    pub languages: Vec<String>,
    pub sections: Vec<SectionDescriptor>,
}
fn token(s: &str) -> bool {
    !s.is_empty() && s.len() <= 128
        && s.bytes().all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.' | b':'))
}
impl Manifest {
    fn validate(&self, payload_len: usize) -> Result<(), ArchiveError> {
        if !token(&self.package) { return Err(ArchiveError::Invalid("package name")); }
        if self.languages.is_empty() || self.languages.len() > MAX_LANGUAGES {
            return Err(ArchiveError::Limit("language count"));
        }
        let mut langs = BTreeSet::new();
        for lang in &self.languages {
            if !token(lang) || !langs.insert(lang) {
                return Err(ArchiveError::Invalid("language identifier"));
            }
        }
        if self.sections.is_empty() || self.sections.len() > MAX_SECTIONS {
            return Err(ArchiveError::Limit("section count"));
        }
        let mut names = BTreeSet::new();
        let mut ranges = Vec::with_capacity(self.sections.len());
        for section in &self.sections {
            if !token(&section.name) || !names.insert(&section.name) {
                return Err(ArchiveError::Invalid("section name"));
            }
            if !token(&section.kind) { return Err(ArchiveError::Invalid("section kind")); }
            if section.length == 0 { return Err(ArchiveError::Invalid("empty section")); }
            let end = section.offset.checked_add(section.length)
                .ok_or(ArchiveError::Invalid("section range overflow"))?;
            if end > payload_len as u64 { return Err(ArchiveError::Invalid("section outside payload")); }
            ranges.push((section.offset, end));
        }
        ranges.sort_unstable();
        if ranges.windows(2).any(|p| p[0].1 > p[1].0) {
            return Err(ArchiveError::Invalid("overlapping sections"));
        }
        Ok(())
    }
}

#[derive(Debug, Default)]
pub struct ArchiveBuilder {
    package: String,
    languages: Vec<String>,
    sections: Vec<SectionDescriptor>,
    payload: Vec<u8>,
    header_flags: u16,
}
impl ArchiveBuilder {
    pub fn new(package: impl Into<String>) -> Self {
        Self { package: package.into(), ..Self::default() }
    }
    /// Sets known archive feature flags. A zero flag remains the legacy v1 encoding.
    pub fn set_header_flags(&mut self, flags: u16) -> Result<(), ArchiveError> {
        if flags & !KNOWN_AIRW_FLAGS != 0 { return Err(ArchiveError::Invalid("unknown header flags")); }
        self.header_flags = flags;
        Ok(())
    }
    pub fn add_language(&mut self, language: impl Into<String>) -> Result<(), ArchiveError> {
        let language = language.into();
        if !token(&language) || self.languages.contains(&language) {
            return Err(ArchiveError::Invalid("language identifier"));
        }
        if self.languages.len() >= MAX_LANGUAGES { return Err(ArchiveError::Limit("language count")); }
        self.languages.push(language);
        Ok(())
    }
    pub fn add_section(&mut self, name: impl Into<String>, kind: impl Into<String>, data: &[u8])
        -> Result<(), ArchiveError>
    {
        let name = name.into();
        let kind = kind.into();
        if !token(&name) || self.sections.iter().any(|s| s.name == name) {
            return Err(ArchiveError::Invalid("section name"));
        }
        if !token(&kind) { return Err(ArchiveError::Invalid("section kind")); }
        if data.is_empty() { return Err(ArchiveError::Invalid("empty section")); }
        if self.sections.len() >= MAX_SECTIONS { return Err(ArchiveError::Limit("section count")); }
        let end = self.payload.len().checked_add(data.len())
            .ok_or(ArchiveError::Limit("payload size"))?;
        if end > MAX_PAYLOAD_BYTES { return Err(ArchiveError::Limit("payload size")); }
        self.sections.push(SectionDescriptor {
            name, kind, offset: self.payload.len() as u64, length: data.len() as u64
        });
        self.payload.extend_from_slice(data);
        Ok(())
    }
    pub fn finish(self) -> Result<Vec<u8>, ArchiveError> {
        let manifest = Manifest { package: self.package, languages: self.languages, sections: self.sections };
        manifest.validate(self.payload.len())?;
        encode(&manifest, &self.payload, self.header_flags)
    }
}
fn encode(manifest: &Manifest, payload: &[u8], header_flags: u16) -> Result<Vec<u8>, ArchiveError> {
    if payload.len() > MAX_PAYLOAD_BYTES { return Err(ArchiveError::Limit("payload size")); }
    manifest.validate(payload.len())?;
    let meta = serde_json::to_vec(manifest)?;
    if meta.len() > MAX_MANIFEST_BYTES { return Err(ArchiveError::Limit("manifest size")); }
    let meta_len = u32::try_from(meta.len()).map_err(|_| ArchiveError::Limit("manifest size"))?;
    let payload_len = u64::try_from(payload.len()).map_err(|_| ArchiveError::Limit("payload size"))?;
    let mut hash = Sha256::new();
    hash.update(&meta); hash.update(payload);
    let digest = hash.finalize();
    let total = HEADER_LEN.checked_add(meta.len()).and_then(|n| n.checked_add(payload.len()))
        .ok_or(ArchiveError::Limit("archive size"))?;
    let mut out = Vec::with_capacity(total);
    out.extend_from_slice(&MAGIC);
    out.extend_from_slice(&FORMAT_VERSION.to_le_bytes());
    out.extend_from_slice(&header_flags.to_le_bytes());
    out.extend_from_slice(&meta_len.to_le_bytes());
    out.extend_from_slice(&payload_len.to_le_bytes());
    out.extend_from_slice(&digest);
    out.extend_from_slice(&meta);
    out.extend_from_slice(payload);
    Ok(out)
}

enum ArchiveStorage {
    Owned(Vec<u8>),
    Mapped(Mmap),
}
impl ArchiveStorage {
    fn as_slice(&self) -> &[u8] {
        match self { Self::Owned(bytes) => bytes, Self::Mapped(map) => map }
    }
}

pub struct ArchiveReader {
    storage: ArchiveStorage,
    manifest: Manifest,
    payload_start: usize,
    payload_len: usize,
    header_flags: u16,
}
impl ArchiveReader {
    /// Opens an archive using a read-only memory map.
    ///
    /// The caller must ensure the file is not modified or truncated for the reader's lifetime.
    ///
    /// Safety contract: the mapped file must not be modified or truncated by
    /// another process while this reader exists. The mapping is owned by this
    /// object and remains valid for the reader's lifetime.
    pub fn open(path: impl AsRef<Path>) -> Result<Self, ArchiveError> {
        let file = File::open(path)?;
        let length = file.metadata()?.len();
        let max_len = (HEADER_LEN + MAX_MANIFEST_BYTES + MAX_PAYLOAD_BYTES) as u64;
        if length > max_len { return Err(ArchiveError::Limit("archive size")); }
        if length < HEADER_LEN as u64 { return Err(ArchiveError::Invalid("truncated header")); }

        // SAFETY: the map is read-only and owned by ArchiveReader. Callers must
        // uphold the documented no-mutation/no-truncation requirement.
        let map = unsafe { MmapOptions::new().len(length as usize).map(&file)? };
        Self::from_storage(ArchiveStorage::Mapped(map))
    }

    pub fn from_bytes(bytes: Vec<u8>) -> Result<Self, ArchiveError> {
        Self::from_storage(ArchiveStorage::Owned(bytes))
    }

    fn from_storage(storage: ArchiveStorage) -> Result<Self, ArchiveError> {
        let bytes = storage.as_slice();
        if bytes.len() < HEADER_LEN { return Err(ArchiveError::Invalid("truncated header")); }
        if bytes[0..4] != MAGIC { return Err(ArchiveError::Invalid("bad magic")); }
        let version = u16::from_le_bytes([bytes[4], bytes[5]]);
        if version != FORMAT_VERSION { return Err(ArchiveError::UnsupportedVersion(version)); }
        let header_flags = u16::from_le_bytes([bytes[6], bytes[7]]);
        if header_flags & !KNOWN_AIRW_FLAGS != 0 {
            return Err(ArchiveError::Invalid("unknown header flags"));
        }
        let meta_len = u32::from_le_bytes(bytes[8..12].try_into().unwrap()) as usize;
        let payload_u64 = u64::from_le_bytes(bytes[12..20].try_into().unwrap());
        let payload_len = usize::try_from(payload_u64).map_err(|_| ArchiveError::Limit("payload size"))?;
        if meta_len > MAX_MANIFEST_BYTES { return Err(ArchiveError::Limit("manifest size")); }
        if payload_len > MAX_PAYLOAD_BYTES { return Err(ArchiveError::Limit("payload size")); }
        let payload_start = HEADER_LEN.checked_add(meta_len)
            .ok_or(ArchiveError::Invalid("archive length overflow"))?;
        let expected = payload_start.checked_add(payload_len)
            .ok_or(ArchiveError::Invalid("archive length overflow"))?;
        if bytes.len() != expected { return Err(ArchiveError::Invalid("archive length mismatch")); }
        let meta = &bytes[HEADER_LEN..payload_start];
        let payload = &bytes[payload_start..expected];
        let mut hash = Sha256::new();
        hash.update(meta); hash.update(payload);
        if hash.finalize().as_slice() != &bytes[20..52] {
            return Err(ArchiveError::ChecksumMismatch);
        }
        let manifest: Manifest = serde_json::from_slice(meta)?;
        manifest.validate(payload_len)?;
        Ok(Self { storage, manifest, payload_start, payload_len, header_flags })
    }

    pub fn manifest(&self) -> &Manifest { &self.manifest }
    pub fn payload_len(&self) -> usize { self.payload_len }
    /// Returns the validated feature flags from the AIRW header.
    pub fn header_flags(&self) -> u16 { self.header_flags }
    /// Compatibility alias emphasizing that v1 flag bits select section encoding.
    pub fn encoding_flag(&self) -> u16 { self.header_flags }
    pub fn is_memory_mapped(&self) -> bool { matches!(&self.storage, ArchiveStorage::Mapped(_)) }
    pub fn section(&self, name: &str) -> Option<&[u8]> {
        let d = self.manifest.sections.iter().find(|s| s.name == name)?;
        let start = self.payload_start + d.offset as usize;
        Some(&self.storage.as_slice()[start..start + d.length as usize])
    }
    pub fn section_names(&self) -> impl Iterator<Item = &str> {
        self.manifest.sections.iter().map(|s| s.name.as_str())
    }
}


/// Helpers for creating deterministic AIRW fixtures at test time. No binary
/// fixtures need to be checked into the repository.
pub mod testing {
    use super::{ArchiveBuilder, ArchiveError, MAX_PAYLOAD_BYTES, MAX_SECTIONS};

    /// Builds a canonical archive from named sections. Inputs are sorted before
    /// serialization so identical logical inputs yield identical bytes.
    pub fn build_deterministic_archive(
        package: &str,
        languages: &[&str],
        sections: &[(&str, &str, &[u8])],
    ) -> Result<Vec<u8>, ArchiveError> {
        let mut sorted_languages = languages.to_vec();
        sorted_languages.sort_unstable();
        let mut sorted_sections = sections.to_vec();
        sorted_sections.sort_unstable_by(|a, b| a.0.cmp(b.0));

        let mut builder = ArchiveBuilder::new(package);
        for language in sorted_languages {
            builder.add_language(language)?;
        }
        for (name, kind, data) in sorted_sections {
            builder.add_section(name, kind, data)?;
        }
        builder.finish()
    }

    /// Generates deterministic pseudo-random test weights in a valid AIRW
    /// archive. The seed and section sizes fully determine the output. The
    /// archive can be opened with ArchiveReader::open to exercise mmap.
    pub fn deterministic_test_weights(
        seed: u64,
        section_count: usize,
        bytes_per_section: usize,
    ) -> Result<Vec<u8>, ArchiveError> {
        if section_count == 0 || bytes_per_section == 0 {
            return Err(ArchiveError::Invalid("empty test weights"));
        }
        if section_count > MAX_SECTIONS {
            return Err(ArchiveError::Limit("section count"));
        }
        let payload_len = section_count.checked_mul(bytes_per_section)
            .ok_or(ArchiveError::Limit("payload size"))?;
        if payload_len > MAX_PAYLOAD_BYTES {
            return Err(ArchiveError::Limit("payload size"));
        }

        fn next(state: &mut u64) -> u64 {
            *state = state.wrapping_add(0x9e3779b97f4a7c15);
            let mut z = *state;
            z = (z ^ (z >> 30)).wrapping_mul(0xbf58476d1ce4e5b9);
            z = (z ^ (z >> 27)).wrapping_mul(0x94d049bb133111eb);
            z ^ (z >> 31)
        }

        let mut state = seed;
        let mut builder = ArchiveBuilder::new("air-test-weights");
        builder.add_language("rust")?;
        for index in 0..section_count {
            let mut data = Vec::with_capacity(bytes_per_section);
            while data.len() < bytes_per_section {
                data.extend_from_slice(&next(&mut state).to_le_bytes());
            }
            data.truncate(bytes_per_section);
            let name = format!("weight.{index:04}");
            builder.add_section(name, "test-weight", &data)?;
        }
        builder.finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn fixture() -> Vec<u8> {
        let mut b = ArchiveBuilder::new("demo");
        b.add_language("rust").unwrap();
        b.add_language("python").unwrap();
        b.add_section("rust.e0382", "ast-rule", b"rule1").unwrap();
        b.add_section("python.context-manager", "ast-rule", b"rule2").unwrap();
        b.finish().unwrap()
    }
    #[test]
    fn round_trip_sections_and_manifest() {
        let r = ArchiveReader::from_bytes(fixture()).unwrap();
        assert_eq!(r.manifest().package, "demo");
        assert_eq!(r.manifest().languages, vec!["rust".to_string(), "python".to_string()]);
        assert_eq!(r.section("rust.e0382"), Some(&b"rule1"[..]));
        assert_eq!(r.section("python.context-manager"), Some(&b"rule2"[..]));
        assert_eq!(r.section("missing"), None);
        assert_eq!(r.section_names().count(), 2);
    }
    #[test]
    fn rejects_truncation_trailing_bytes_and_bad_magic() {
        let mut bytes = fixture(); bytes.pop();
        assert!(matches!(ArchiveReader::from_bytes(bytes), Err(ArchiveError::Invalid("archive length mismatch"))));
        let mut bytes = fixture(); bytes.push(0);
        assert!(matches!(ArchiveReader::from_bytes(bytes), Err(ArchiveError::Invalid("archive length mismatch"))));
        let mut bytes = fixture(); bytes[0] = b'X';
        assert!(matches!(ArchiveReader::from_bytes(bytes), Err(ArchiveError::Invalid("bad magic"))));
    }
    #[test]
    fn file_open_uses_mapping_and_reads_validated_sections() {
        use std::sync::atomic::{AtomicU64, Ordering};
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let path = std::env::temp_dir().join(format!(
            "airw-test-{}-{}.airw",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::write(&path, fixture()).unwrap();
        let reader = ArchiveReader::open(&path).unwrap();
        assert!(reader.is_memory_mapped());
        assert_eq!(reader.section("rust.e0382"), Some(&b"rule1"[..]));
        assert_eq!(reader.manifest().languages, vec!["rust".to_string(), "python".to_string()]);
        drop(reader);
        std::fs::remove_file(path).unwrap();
    }
    #[test]
    fn rejects_checksum_tampering() {
        let mut bytes = fixture(); *bytes.last_mut().unwrap() ^= 1;
        assert!(matches!(ArchiveReader::from_bytes(bytes), Err(ArchiveError::ChecksumMismatch)));
    }
    #[test]
    fn rejects_unsupported_version_and_flags() {
        let mut bytes = fixture(); bytes[4..6].copy_from_slice(&2u16.to_le_bytes());
        assert!(matches!(ArchiveReader::from_bytes(bytes), Err(ArchiveError::UnsupportedVersion(2))));
        let mut bytes = fixture(); bytes[6..8].copy_from_slice(&0x8000u16.to_le_bytes());
        assert!(matches!(ArchiveReader::from_bytes(bytes), Err(ArchiveError::Invalid("unknown header flags"))));
        let mut bytes = fixture(); bytes[6..8].copy_from_slice(&AIRW_FLAG_BITCOS_TERNARY.to_le_bytes());
        assert_eq!(ArchiveReader::from_bytes(bytes).unwrap().header_flags(), AIRW_FLAG_BITCOS_TERNARY);
    }
    #[test]
    fn rejects_duplicate_and_empty_sections() {
        let mut b = ArchiveBuilder::new("demo"); b.add_language("rust").unwrap();
        b.add_section("main", "ast-rule", b"one").unwrap();
        assert!(matches!(b.add_section("main", "ast-rule", b"two"), Err(ArchiveError::Invalid("section name"))));
        assert!(matches!(b.add_section("empty", "ast-rule", b""), Err(ArchiveError::Invalid("empty section"))));
    }
    #[test]
    fn validates_section_ranges_and_overlap() {
        let m = Manifest {
            package:"demo".into(), languages:vec!["rust".into()],
            sections:vec![
                SectionDescriptor{name:"one".into(),kind:"rule".into(),offset:0,length:4},
                SectionDescriptor{name:"two".into(),kind:"rule".into(),offset:3,length:4},
            ],
        };
        assert!(matches!(m.validate(8),Err(ArchiveError::Invalid("overlapping sections"))));
        assert!(matches!(m.validate(6),Err(ArchiveError::Invalid("section outside payload"))));
    }
    #[test]
    fn canonical_fixture_builder_sorts_languages_and_sections() {
        let forward = testing::build_deterministic_archive(
            "canonical",
            &["rust", "python"],
            &[("zeta", "rule", b"z".as_slice()), ("alpha", "rule", b"a".as_slice())],
        ).unwrap();
        let reversed = testing::build_deterministic_archive(
            "canonical",
            &["python", "rust"],
            &[("alpha", "rule", b"a".as_slice()), ("zeta", "rule", b"z".as_slice())],
        ).unwrap();
        assert_eq!(forward, reversed);
        let reader = ArchiveReader::from_bytes(forward).unwrap();
        assert_eq!(reader.section("alpha"), Some(&b"a"[..]));
        assert_eq!(reader.section("zeta"), Some(&b"z"[..]));
    }

    #[test]
    fn generated_fixture_opens_through_memory_map() {
        use std::sync::atomic::{AtomicU64, Ordering};
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let path = std::env::temp_dir().join(format!(
            "airw-generated-{}-{}.airw",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::write(&path, testing::deterministic_test_weights(7, 4, 32).unwrap()).unwrap();
        let reader = ArchiveReader::open(&path).unwrap();
        assert!(reader.is_memory_mapped());
        assert_eq!(reader.section("weight.0003").unwrap().len(), 32);
        drop(reader);
        std::fs::remove_file(path).unwrap();
    }

    #[test]
    fn deterministic_weight_seed_changes_payload_and_rejects_invalid_sizes() {
        let a = testing::deterministic_test_weights(7, 2, 16).unwrap();
        let b = testing::deterministic_test_weights(8, 2, 16).unwrap();
        assert_ne!(a, b);
        assert!(matches!(
            testing::deterministic_test_weights(7, 0, 16),
            Err(ArchiveError::Invalid("empty test weights"))
        ));
        assert!(testing::deterministic_test_weights(7, 1, MAX_PAYLOAD_BYTES + 1).is_err());
    }

}


pub mod compaction;
pub use compaction::{compact_archives, CompactionError, CompactionPolicy, CompactionResult, CompactionStats, HitCountedRuleRecord, HDC_PROTOTYPE_KIND, HIT_COUNTED_RULE_KIND};

pub mod overlay;
