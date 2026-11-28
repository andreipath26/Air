//! Hard-negative prototype inhibition and deterministic AIRW storage.
//!
//! A failed symbolic verification can be recorded as a 10,000-bit HDC
//! prototype. Nearby future signatures bypass symbolic acceptance and request
//! SLM escalation. Prototypes live in a typed AIRW section; legacy archives
//! remain readable because this uses the v1 extensible section manifest.

use crate::{simd::hamming_distance, HyperVector, HYPERVECTOR_BITS, HYPERVECTOR_WORDS};
use air_weights::{ArchiveBuilder, ArchiveError, ArchiveReader};
use std::{fs, path::Path};

const SECTION_NAME: &str = "inhibitory-prototypes";
const SECTION_KIND: &str = "inhibitory-hdc-v1";
const PAYLOAD_MAGIC: [u8; 4] = *b"IHDP";
const PAYLOAD_VERSION: u32 = 1;
const PAYLOAD_HEADER_BYTES: usize = 16;
pub const MAX_INHIBITORY_PROTOTYPES: usize = 4096;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum InhibitionReason {
    NoSymbolicCandidate,
    FailedVerificationPrototype,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum InhibitoryDecision {
    AllowSymbolic,
    EscalateToSlm { reason: InhibitionReason },
}

/// Bounded, deterministic set of failed-verification signatures.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct InhibitoryMemory {
    max_hamming_distance: u32,
    prototypes: Vec<HyperVector>,
}

impl InhibitoryMemory {
    /// Creates an empty memory. The inhibition radius cannot exceed the
    /// logical vector dimensions.
    pub fn new(max_hamming_distance: u32) -> Result<Self, ArchiveError> {
        if max_hamming_distance as usize > HYPERVECTOR_BITS {
            return Err(ArchiveError::Invalid("inhibitory distance threshold"));
        }
        Ok(Self { max_hamming_distance, prototypes: Vec::new() })
    }

    pub fn max_hamming_distance(&self) -> u32 { self.max_hamming_distance }
    pub fn prototypes(&self) -> &[HyperVector] { &self.prototypes }
    pub fn len(&self) -> usize { self.prototypes.len() }
    pub fn is_empty(&self) -> bool { self.prototypes.is_empty() }

    /// Add the signature of a symbolic candidate that failed compiler/test
    /// verification. Duplicate failures are collapsed.
    pub fn record_symbolic_verification_failure(
        &mut self,
        signature: HyperVector,
    ) -> Result<bool, ArchiveError> {
        if signature.0[HYPERVECTOR_WORDS - 1] >> (HYPERVECTOR_BITS % 64) != 0 {
            return Err(ArchiveError::Invalid("inhibitory signature padding"));
        }
        if self.prototypes.contains(&signature) {
            return Ok(false);
        }
        if self.prototypes.len() >= MAX_INHIBITORY_PROTOTYPES {
            return Err(ArchiveError::Limit("inhibitory prototype count"));
        }
        self.prototypes.push(signature);
        Ok(true)
    }

    pub fn inhibits(&self, query: &HyperVector) -> bool {
        self.prototypes.iter().any(|prototype| {
            hamming_distance(prototype, query) <= self.max_hamming_distance
        })
    }

    /// Gate a proposed symbolic hit. A prior hard-negative match always
    /// escalates; non-hits also escalate through the ordinary SLM path.
    pub fn gate(&self, query: &HyperVector, symbolic_hit: bool) -> InhibitoryDecision {
        if self.inhibits(query) {
            InhibitoryDecision::EscalateToSlm {
                reason: InhibitionReason::FailedVerificationPrototype,
            }
        } else if symbolic_hit {
            InhibitoryDecision::AllowSymbolic
        } else {
            InhibitoryDecision::EscalateToSlm {
                reason: InhibitionReason::NoSymbolicCandidate,
            }
        }
    }

    /// Serialize this memory as a typed AIRW section. Prototypes are sorted
    /// and deduplicated so equivalent logical memories yield identical bytes.
    pub fn to_airw(&self) -> Result<Vec<u8>, ArchiveError> {
        let mut prototypes: Vec<[u64; HYPERVECTOR_WORDS]> =
            self.prototypes.iter().map(|prototype| prototype.0).collect();
        prototypes.sort_unstable();
        prototypes.dedup();

        let body_len = prototypes.len()
            .checked_mul(HYPERVECTOR_WORDS * 8)
            .ok_or(ArchiveError::Limit("inhibitory payload size"))?;
        let total_len = PAYLOAD_HEADER_BYTES.checked_add(body_len)
            .ok_or(ArchiveError::Limit("inhibitory payload size"))?;
        let mut payload = Vec::with_capacity(total_len);
        payload.extend_from_slice(&PAYLOAD_MAGIC);
        payload.extend_from_slice(&PAYLOAD_VERSION.to_le_bytes());
        payload.extend_from_slice(&self.max_hamming_distance.to_le_bytes());
        payload.extend_from_slice(&(prototypes.len() as u32).to_le_bytes());
        for prototype in prototypes {
            for word in prototype {
                payload.extend_from_slice(&word.to_le_bytes());
            }
        }

        let mut builder = ArchiveBuilder::new("air-inhibitory-prototypes");
        builder.add_language("rust")?;
        builder.add_section(SECTION_NAME, SECTION_KIND, &payload)?;
        builder.finish()
    }

    pub fn from_archive(archive: &ArchiveReader) -> Result<Self, ArchiveError> {
        let descriptor = archive.manifest().sections.iter()
            .find(|descriptor| descriptor.name == SECTION_NAME)
            .ok_or(ArchiveError::Invalid("inhibitory section missing"))?;
        if descriptor.kind != SECTION_KIND {
            return Err(ArchiveError::Invalid("inhibitory section kind"));
        }
        let payload = archive.section(SECTION_NAME)
            .ok_or(ArchiveError::Invalid("inhibitory section missing"))?;
        Self::decode_payload(payload)
    }

    fn decode_payload(payload: &[u8]) -> Result<Self, ArchiveError> {
        if payload.len() < PAYLOAD_HEADER_BYTES || payload[..4] != PAYLOAD_MAGIC {
            return Err(ArchiveError::Invalid("inhibitory payload header"));
        }
        let version = u32::from_le_bytes(payload[4..8].try_into().unwrap());
        if version != PAYLOAD_VERSION {
            return Err(ArchiveError::Invalid("inhibitory payload version"));
        }
        let threshold = u32::from_le_bytes(payload[8..12].try_into().unwrap());
        let count_bytes = payload.get(12..16).ok_or(ArchiveError::Invalid("inhibitory payload header"))?;
        let count = u32::from_le_bytes(count_bytes.try_into().unwrap()) as usize;
        if threshold as usize > HYPERVECTOR_BITS {
            return Err(ArchiveError::Invalid("inhibitory distance threshold"));
        }
        if count > MAX_INHIBITORY_PROTOTYPES {
            return Err(ArchiveError::Limit("inhibitory prototype count"));
        }
        let header_bytes = 16;
        let prototype_bytes = HYPERVECTOR_WORDS * 8;
        let expected = count.checked_mul(prototype_bytes)
            .and_then(|len| len.checked_add(header_bytes))
            .ok_or(ArchiveError::Limit("inhibitory payload size"))?;
        if payload.len() != expected {
            return Err(ArchiveError::Invalid("inhibitory payload length"));
        }
        let mut memory = Self::new(threshold)?;
        let mut offset = header_bytes;
        for _ in 0..count {
            let mut words = [0u64; HYPERVECTOR_WORDS];
            for word in &mut words {
                *word = u64::from_le_bytes(payload[offset..offset + 8].try_into().unwrap());
                offset += 8;
            }
            let signature = HyperVector(words);
            memory.record_symbolic_verification_failure(signature)?;
        }
        Ok(memory)
    }
}

/// Mapped archive owner plus its validated prototype index.
pub struct MappedInhibitoryMemory {
    archive: ArchiveReader,
    memory: InhibitoryMemory,
}

impl MappedInhibitoryMemory {
    pub fn open(path: impl AsRef<Path>) -> Result<Self, ArchiveError> {
        let archive = ArchiveReader::open(path)?;
        let memory = InhibitoryMemory::from_archive(&archive)?;
        Ok(Self { archive, memory })
    }

    pub fn is_memory_mapped(&self) -> bool { self.archive.is_memory_mapped() }
    pub fn memory(&self) -> &InhibitoryMemory { &self.memory }
}

/// Write a deterministic inhibitory AIRW archive to a local file.
pub fn write_airw(path: impl AsRef<Path>, memory: &InhibitoryMemory) -> Result<(), ArchiveError> {
    fs::write(path, memory.to_airw()?).map_err(ArchiveError::Io)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};

    #[test]
    fn failed_symbolic_signature_inhibits_nearby_hit_and_escalates() {
        let mut memory = InhibitoryMemory::new(1).unwrap();
        let failed = HyperVector::from_seed(0xabc);
        assert!(memory.record_symbolic_verification_failure(failed).unwrap());
        assert!(!memory.record_symbolic_verification_failure(failed).unwrap());

        let mut near = failed;
        near.set_bit(3, !near.bit(3));
        assert_eq!(
            memory.gate(&near, true),
            InhibitoryDecision::EscalateToSlm {
                reason: InhibitionReason::FailedVerificationPrototype,
            }
        );

        let far = HyperVector::from_seed(0xdef);
        assert_eq!(memory.gate(&far, true), InhibitoryDecision::AllowSymbolic);
        assert_eq!(
            memory.gate(&far, false),
            InhibitoryDecision::EscalateToSlm {
                reason: InhibitionReason::NoSymbolicCandidate,
            }
        );
    }

    #[test]
    fn airw_encoding_is_deterministic_and_round_trips() {
        let mut left = InhibitoryMemory::new(8).unwrap();
        let a = HyperVector::from_seed(12);
        let b = HyperVector::from_seed(42);
        left.record_symbolic_verification_failure(a).unwrap();
        left.record_symbolic_verification_failure(b).unwrap();

        let mut right = InhibitoryMemory::new(8).unwrap();
        right.record_symbolic_verification_failure(b).unwrap();
        right.record_symbolic_verification_failure(a).unwrap();
        assert_eq!(left.to_airw().unwrap(), right.to_airw().unwrap());

        let bytes = left.to_airw().unwrap();
        let archive = ArchiveReader::from_bytes(bytes).unwrap();
        assert_eq!(InhibitoryMemory::from_archive(&archive).unwrap(), left);
    }

    #[test]
    fn inhibitory_airw_opens_through_mmap() {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let path = std::env::temp_dir().join(format!(
            "air-inhibitory-{}-{}.airw",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        let mut memory = InhibitoryMemory::new(4).unwrap();
        let failed = HyperVector::from_seed(77);
        memory.record_symbolic_verification_failure(failed).unwrap();
        write_airw(&path, &memory).unwrap();
        let mapped = MappedInhibitoryMemory::open(&path).unwrap();
        assert!(mapped.is_memory_mapped());
        assert_eq!(mapped.memory().gate(&failed, true), InhibitoryDecision::EscalateToSlm {
            reason: InhibitionReason::FailedVerificationPrototype,
        });
        drop(mapped);
        fs::remove_file(path).unwrap();
    }

    #[test]
    fn corrupt_payload_is_rejected() {
        assert!(InhibitoryMemory::decode_payload(b"bad").is_err());
        let mut payload = Vec::new();
        payload.extend_from_slice(&PAYLOAD_MAGIC);
        payload.extend_from_slice(&PAYLOAD_VERSION.to_le_bytes());
        payload.extend_from_slice(&0u32.to_le_bytes());
        payload.extend_from_slice(&((MAX_INHIBITORY_PROTOTYPES + 1) as u32).to_le_bytes());
        assert!(InhibitoryMemory::decode_payload(&payload).is_err());
    }
}
