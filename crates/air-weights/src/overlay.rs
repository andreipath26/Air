//! AIRW delta overlay builder used by online solution distillation.
use super::{ArchiveBuilder, ArchiveError, ArchiveReader};
use serde::{Deserialize, Serialize};
use std::{fs, path::Path};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DeltaTemplate {
    pub language: String,
    pub prompt: String,
    pub template: String,
    pub signature: Vec<u64>,
    pub fingerprint: String,
}
fn validate(entries: &[DeltaTemplate]) -> Result<(), ArchiveError> {
    for e in entries {
        if e.language != "rust" && e.language != "python" { return Err(ArchiveError::Invalid("delta language")); }
        if e.prompt.is_empty() || e.template.is_empty() || e.fingerprint.is_empty() { return Err(ArchiveError::Invalid("empty delta template field")); }
        if e.signature.len() != 157 || e.signature[156] >> 16 != 0 { return Err(ArchiveError::Invalid("delta HDC signature")); }
    }
    Ok(())
}
pub fn build_delta_archive(entries: &[DeltaTemplate]) -> Result<Vec<u8>, ArchiveError> {
    validate(entries)?;
    let mut sorted = entries.to_vec();
    sorted.sort_by(|a,b| a.fingerprint.cmp(&b.fingerprint));
    sorted.dedup_by(|a,b| a.fingerprint == b.fingerprint);
    let data = serde_json::to_vec(&sorted)?;
    let mut b = ArchiveBuilder::new("air-delta-overlay");
    b.add_language("rust")?;
    b.add_language("python")?;
    b.add_section("templates", "symbolic-templates", &data)?;
    b.finish()
}
pub fn decode_delta_archive(archive: &ArchiveReader) -> Result<Vec<DeltaTemplate>, ArchiveError> {
    let data = archive.section("templates").ok_or(ArchiveError::Invalid("delta templates section missing"))?;
    let entries: Vec<DeltaTemplate> = serde_json::from_slice(data)?;
    validate(&entries)?;
    Ok(entries)
}
pub struct MappedDeltaOverlay { archive: ArchiveReader, entries: Vec<DeltaTemplate> }
impl MappedDeltaOverlay {
    pub fn open(path: impl AsRef<Path>) -> Result<Self, ArchiveError> {
        let archive = ArchiveReader::open(path)?;
        let entries = decode_delta_archive(&archive)?;
        Ok(Self { archive, entries })
    }
    pub fn is_memory_mapped(&self) -> bool { self.archive.is_memory_mapped() }
    pub fn entries(&self) -> &[DeltaTemplate] { &self.entries }
}
#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};
    static NEXT: AtomicUsize = AtomicUsize::new(0);
    fn sample(name: &str) -> DeltaTemplate {
        let mut signature = vec![0; 157]; signature[156] = 1;
        DeltaTemplate { language:"rust".into(), prompt:name.into(), template:format!("fn {name}() {{}}"), signature, fingerprint:name.into() }
    }
    #[test]
    fn deterministic_and_deduplicated() {
        let a=sample("a"); let b=sample("b");
        let one=build_delta_archive(&[b.clone(),a.clone(),a.clone()]).unwrap();
        let two=build_delta_archive(&[a,b]).unwrap();
        assert_eq!(one,two);
        let reader=ArchiveReader::from_bytes(one).unwrap();
        assert_eq!(decode_delta_archive(&reader).unwrap().len(),2);
    }
    #[test]
    fn mmap_round_trip_and_signature_validation() {
        let file=std::env::temp_dir().join(format!("air-delta-{}-{}.airw",std::process::id(),NEXT.fetch_add(1,Ordering::Relaxed)));
        fs::write(&file,build_delta_archive(&[sample("mapped")]).unwrap()).unwrap();
        let mapped=MappedDeltaOverlay::open(&file).unwrap();
        assert!(mapped.is_memory_mapped());
        assert_eq!(mapped.entries().len(),1);
        drop(mapped); fs::remove_file(file).unwrap();
        let mut bad=sample("bad"); bad.signature[156]=1<<16;
        assert!(matches!(build_delta_archive(&[bad]),Err(ArchiveError::Invalid("delta HDC signature"))));
    }
}
