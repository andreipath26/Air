//! Verified-solution distillation into a compact local AIRW delta overlay.
//!
//! Callers must provide compiler and test evidence from air-core before promotion.
//! The archive stores only verified examples; writes rebuild the canonical archive
//! atomically, which also compacts duplicates and stale entries.
use air_weights::{overlay::{build_delta_archive, decode_delta_archive, DeltaTemplate}, ArchiveError, ArchiveReader};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{error::Error, fmt, fs, io, path::{Path, PathBuf}};

pub const HDC_BITS: usize = 10_000;
pub const HDC_WORDS: usize = (HDC_BITS + 63) / 64;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Verification {
    pub compiler_clean: bool,
    pub tests_passed: bool,
}
impl Verification {
    pub fn is_verified(self) -> bool { self.compiler_clean && self.tests_passed }
}

pub type SymbolicTemplate = DeltaTemplate;

#[derive(Debug)]
pub enum DistillationError {
    Io(io::Error),
    Archive(ArchiveError),
    Json(serde_json::Error),
    Unverified,
    CriticRejected,
    InvalidLanguage,
}
impl fmt::Display for DistillationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(e) => write!(f, "overlay I/O error: {e}"),
            Self::Archive(e) => write!(f, "overlay archive error: {e}"),
            Self::Json(e) => write!(f, "overlay JSON error: {e}"),
            Self::Unverified => f.write_str("refusing to distill an unverified solution"),
            Self::CriticRejected => f.write_str("refusing to distill without matching sandboxed adversarial proof"),
            Self::InvalidLanguage => f.write_str("language must be rust or python"),
        }
    }
}
impl Error for DistillationError {}
impl From<io::Error> for DistillationError { fn from(e: io::Error) -> Self { Self::Io(e) } }
impl From<ArchiveError> for DistillationError { fn from(e: ArchiveError) -> Self { Self::Archive(e) } }
impl From<serde_json::Error> for DistillationError { fn from(e: serde_json::Error) -> Self { Self::Json(e) } }

/// Deterministically maps prompt text into a 10,000-bit binary HDC signature.
pub fn hdc_signature(text: &str) -> Vec<u64> {
    let digest = Sha256::digest(text.as_bytes());
    let mut seed = [0u8; 8];
    seed.copy_from_slice(&digest[..8]);
    let mut state = u64::from_le_bytes(seed);
    let mut words = Vec::with_capacity(HDC_WORDS);
    for _ in 0..HDC_WORDS {
        state = state.wrapping_add(0x9e3779b97f4a7c15);
        let mut z = state;
        z = (z ^ (z >> 30)).wrapping_mul(0xbf58476d1ce4e5b9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94d049bb133111eb);
        words.push(z ^ (z >> 31));
    }
    words[HDC_WORDS - 1] &= (1u64 << (HDC_BITS % 64)) - 1;
    words
}

pub fn hamming_distance(left: &[u64], right: &[u64]) -> Option<u32> {
    if left.len() != HDC_WORDS || right.len() != HDC_WORDS { return None; }
    Some(left.iter().zip(right).map(|(a, b)| (a ^ b).count_ones()).sum())
}

#[derive(Debug, Clone)]
pub struct OverlayStore { path: PathBuf }
impl OverlayStore {
    pub fn new(path: impl Into<PathBuf>) -> Self { Self { path: path.into() } }
    pub fn path(&self) -> &Path { &self.path }

    /// Promote only a solution that passed both compiler verification and tests.
    /// The canonical rewrite is an append plus compaction, preventing unbounded
    /// duplicate growth on low-memory laptops.
    pub(crate) fn append_verified(
        &self,
        language: &str,
        prompt: &str,
        template: &str,
        verification: Verification,
    ) -> Result<usize, DistillationError> {
        if !verification.is_verified() { return Err(DistillationError::Unverified); }
        if language != "rust" && language != "python" { return Err(DistillationError::InvalidLanguage); }
        let mut entries = self.read_entries()?;
        let mut hash = Sha256::new();
        hash.update(language.as_bytes()); hash.update([0]); hash.update(prompt.as_bytes()); hash.update([0]);
        hash.update(template.as_bytes());
        let fingerprint = format!("{:x}", hash.finalize());
        if !entries.iter().any(|entry| entry.fingerprint == fingerprint) {
            entries.push(SymbolicTemplate {
                language: language.to_owned(),
                prompt: prompt.to_owned(),
                template: template.to_owned(),
                signature: hdc_signature(prompt),
                fingerprint,
            });
        }
        self.write_compacted(entries)
    }

    /// Public promotion path requiring compiler/tests and matching sandboxed critic evidence.
    pub fn append_critically_verified(
        &self,
        language: &str,
        prompt: &str,
        template: &str,
        verification: Verification,
        proof: &crate::adversarial::CriticProof,
    ) -> Result<usize, DistillationError> {
        if !verification.is_verified() { return Err(DistillationError::Unverified); }
        if !proof.matches(language, prompt, template) {
            return Err(DistillationError::CriticRejected);
        }
        self.append_verified(language, prompt, template, verification)
    }

    /// Rebuilds the archive in deterministic fingerprint order and removes duplicates.
    pub fn compact(&self) -> Result<usize, DistillationError> {
        let entries = self.read_entries()?;
        self.write_compacted(entries)
    }

    /// Finds the nearest stored prompt signature within the caller's distance bound.
    pub fn lookup(&self, prompt: &str, max_distance: u32) -> Result<Option<SymbolicTemplate>, DistillationError> {
        let query = hdc_signature(prompt);
        let entries = self.read_entries()?;
        Ok(entries.into_iter().filter_map(|entry| {
            let distance = hamming_distance(&query, &entry.signature)?;
            (distance <= max_distance).then_some((distance, entry))
        }).min_by_key(|(distance, _)| *distance).map(|(_, entry)| entry))
    }

    fn read_entries(&self) -> Result<Vec<SymbolicTemplate>, DistillationError> {
        if !self.path.exists() { return Ok(Vec::new()); }
        let archive = ArchiveReader::open(&self.path)?;
        Ok(decode_delta_archive(&archive)?)
    }

    fn write_compacted(&self, mut entries: Vec<SymbolicTemplate>) -> Result<usize, DistillationError> {
        entries.sort_by(|a, b| a.fingerprint.cmp(&b.fingerprint));
        entries.dedup_by(|a, b| a.fingerprint == b.fingerprint);
        let archive = build_delta_archive(&entries)?;
        if let Some(parent) = self.path.parent() { fs::create_dir_all(parent)?; }
        let temp = self.path.with_extension(format!("airw.tmp-{}", std::process::id()));
        fs::write(&temp, archive)?;
        fs::rename(temp, &self.path)?;
        Ok(entries.len())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};
    static NEXT: AtomicUsize = AtomicUsize::new(0);
    fn path() -> PathBuf {
        std::env::temp_dir().join(format!("air-overlay-{}-{}.airw", std::process::id(), NEXT.fetch_add(1, Ordering::Relaxed)))
    }

    #[test]
    fn signature_is_deterministic_and_exactly_ten_thousand_bits() {
        let a = hdc_signature("borrow a value safely");
        assert_eq!(a, hdc_signature("borrow a value safely"));
        assert_eq!(a.len(), 157);
        assert_eq!(a[156] >> 16, 0);
        assert_ne!(hdc_signature("different"), a);
        assert_eq!(hamming_distance(&hdc_signature("x"), &hdc_signature("x")), Some(0));
    }

    #[test]
    fn rejects_unverified_and_deduplicates_verified_promotions() {
        let file = path();
        let store = OverlayStore::new(&file);
        assert!(matches!(store.append_verified("rust", "p", "code", Verification { compiler_clean: true, tests_passed: false }), Err(DistillationError::Unverified)));
        assert_eq!(store.append_verified("rust", "p", "code", Verification { compiler_clean: true, tests_passed: true }).unwrap(), 1);
        assert_eq!(store.append_verified("rust", "p", "code", Verification { compiler_clean: true, tests_passed: true }).unwrap(), 1);
        let hit = store.lookup("p", 0).unwrap().unwrap();
        assert_eq!(hit.template, "code");
        assert_eq!(store.compact().unwrap(), 1);
        fs::remove_file(file).unwrap();
    }

    #[test]
    fn archive_is_mmap_readable_and_signature_has_expected_distance() {
        let file = path();
        let store = OverlayStore::new(&file);
        store.append_verified("python", "sort values", "sorted(values)", Verification { compiler_clean: true, tests_passed: true }).unwrap();
        let reader = ArchiveReader::open(&file).unwrap();
        assert!(reader.is_memory_mapped());
        assert!(reader.section("templates").is_some());
        assert_eq!(hamming_distance(&hdc_signature("sort values"), &hdc_signature("sort values")), Some(0));
        fs::remove_file(file).unwrap();
    }
}
