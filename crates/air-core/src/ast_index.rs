//! Lightweight incremental Rust/Python signature index stored in a memory-mappable AIRW archive.
use air_weights::{ArchiveBuilder, ArchiveError, ArchiveReader};
use serde::{Deserialize, Serialize};
use std::{collections::BTreeMap, error::Error, fmt, fs, io, path::{Path, PathBuf}};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AstSymbol {
    pub language: String,
    pub file: String,
    pub kind: String,
    pub name: String,
    /// Source signature only; function bodies are deliberately excluded.
    pub signature: String,
}

#[derive(Debug)]
pub enum AstIndexError {
    Io(io::Error),
    Archive(ArchiveError),
    Json(serde_json::Error),
    InvalidLanguage,
}
impl fmt::Display for AstIndexError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(e) => write!(f, "AST index I/O error: {e}"),
            Self::Archive(e) => write!(f, "AST index archive error: {e}"),
            Self::Json(e) => write!(f, "AST index JSON error: {e}"),
            Self::InvalidLanguage => f.write_str("language must be rust or python"),
        }
    }
}
impl Error for AstIndexError {}
impl From<io::Error> for AstIndexError { fn from(e: io::Error) -> Self { Self::Io(e) } }
impl From<ArchiveError> for AstIndexError { fn from(e: ArchiveError) -> Self { Self::Archive(e) } }
impl From<serde_json::Error> for AstIndexError { fn from(e: serde_json::Error) -> Self { Self::Json(e) } }

fn identifier_after<'a>(line: &'a str, keyword: &str) -> Option<&'a str> {
    let rest = line.strip_prefix(keyword)?.trim_start();
    let end = rest.find(|c: char| !(c.is_ascii_alphanumeric() || c == '_')).unwrap_or(rest.len());
    (end > 0).then_some(&rest[..end])
}

/// Extracts declaration signatures without parsing or retaining function bodies.
/// This intentionally small scanner is deterministic and useful for prompt context.
pub fn extract_signatures(language: &str, file: &str, source: &str) -> Result<Vec<AstSymbol>, AstIndexError> {
    if language != "rust" && language != "python" { return Err(AstIndexError::InvalidLanguage); }
    let mut symbols = Vec::new();
    for raw in source.lines() {
        let line = raw.trim();
        if line.is_empty() || line.starts_with("//") || line.starts_with('#') { continue; }
        let rust_line = line.strip_prefix("pub ").unwrap_or(line);
        let rust_line = rust_line.strip_prefix("async ").unwrap_or(rust_line);
        let candidate = if language == "rust" {
            [("fn ", "function"), ("struct ", "struct"), ("enum ", "enum"), ("trait ", "trait")]
                .iter().find_map(|(keyword, kind)| identifier_after(rust_line, keyword).map(|name| (*kind, name)))
                .map(|(kind, name)| (kind.to_owned(), name.to_owned(), line.trim_end_matches('{').trim().to_owned()))
        } else {
            [("def ", "function"), ("async def ", "function"), ("class ", "class")]
                .iter().find_map(|(keyword, kind)| identifier_after(line, keyword).map(|name| (*kind, name)))
                .map(|(kind, name)| (kind.to_owned(), name.to_owned(), line.trim_end_matches(':').trim().to_owned()))
        };
        if let Some((kind, name, signature)) = candidate {
            symbols.push(AstSymbol { language: language.to_owned(), file: file.to_owned(), kind, name, signature });
        }
    }
    symbols.sort_by(|a, b| (&a.file, &a.name, &a.kind).cmp(&(&b.file, &b.name, &b.kind)));
    symbols.dedup_by(|a, b| a.file == b.file && a.name == b.name && a.kind == b.kind);
    Ok(symbols)
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct AstSymbolIndex {
    files: BTreeMap<String, Vec<AstSymbol>>,
}
impl AstSymbolIndex {
    /// Incrementally replaces one file's indexed declarations.
    pub fn update_file(&mut self, language: &str, file: &str, source: &str) -> Result<usize, AstIndexError> {
        let symbols = extract_signatures(language, file, source)?;
        let count = symbols.len();
        if count == 0 { self.files.remove(file); } else { self.files.insert(file.to_owned(), symbols); }
        Ok(count)
    }
    pub fn remove_file(&mut self, file: &str) -> bool { self.files.remove(file).is_some() }
    pub fn symbols(&self) -> impl Iterator<Item = &AstSymbol> { self.files.values().flatten() }

    /// Returns declaration signatures whose identifiers appear in the prompt,
    /// bounded to keep added context small for an SLM KV cache.
    pub fn matching_context(&self, prompt: &str, limit: usize) -> Vec<String> {
        let terms: Vec<String> = prompt.split(|c: char| !(c.is_ascii_alphanumeric() || c == '_'))
            .filter(|part| part.len() >= 2).map(str::to_ascii_lowercase).collect();
        let mut ranked: Vec<(usize, &AstSymbol)> = self.symbols().filter_map(|symbol| {
            let haystack = format!("{} {}", symbol.name, symbol.signature).to_ascii_lowercase();
            let score = terms.iter().filter(|term| haystack.contains(term.as_str())).count();
            (score > 0).then_some((score, symbol))
        }).collect();
        ranked.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| a.1.file.cmp(&b.1.file)).then_with(|| a.1.name.cmp(&b.1.name)));
        ranked.into_iter().take(limit).map(|(_, symbol)| symbol.signature.clone()).collect()
    }

    pub fn write_airw(&self, path: impl AsRef<Path>) -> Result<(), AstIndexError> {
        let data = serde_json::to_vec(self)?;
        let mut builder = ArchiveBuilder::new("air-ast-index");
        builder.add_language("rust")?;
        builder.add_language("python")?;
        builder.add_section("symbols", "ast-symbol-index", &data)?;
        let archive = builder.finish()?;
        let path = path.as_ref();
        if let Some(parent) = path.parent() { fs::create_dir_all(parent)?; }
        let temp = path.with_extension(format!("airw.tmp-{}", std::process::id()));
        fs::write(&temp, archive)?;
        fs::rename(temp, path)?;
        Ok(())
    }

    pub fn open_mapped(path: impl AsRef<Path>) -> Result<MappedAstSymbolIndex, AstIndexError> {
        MappedAstSymbolIndex::open(path)
    }
}

/// Retains the read-only mmap for the archive's lifetime while exposing owned symbols.
pub struct MappedAstSymbolIndex {
    archive: ArchiveReader,
    index: AstSymbolIndex,
}
impl MappedAstSymbolIndex {
    pub fn open(path: impl AsRef<Path>) -> Result<Self, AstIndexError> {
        let archive = ArchiveReader::open(path)?;
        let bytes = archive.section("symbols").ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "symbols section missing"))?;
        let index = serde_json::from_slice(bytes)?;
        Ok(Self { archive, index })
    }
    pub fn is_memory_mapped(&self) -> bool { self.archive.is_memory_mapped() }
    pub fn matching_context(&self, prompt: &str, limit: usize) -> Vec<String> { self.index.matching_context(prompt, limit) }
    pub fn symbols(&self) -> impl Iterator<Item = &AstSymbol> { self.index.symbols() }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{fs, sync::atomic::{AtomicUsize, Ordering}};
    static NEXT: AtomicUsize = AtomicUsize::new(0);
    fn path() -> PathBuf { std::env::temp_dir().join(format!("air-ast-{}-{}.airw", std::process::id(), NEXT.fetch_add(1, Ordering::Relaxed))) }

    #[test]
    fn extracts_rust_and_python_public_signatures() {
        let rust = extract_signatures("rust", "src/lib.rs", "pub struct Store {\npub trait Lookup {\nasync fn find(&self, key: &str) -> Option<Value> {\nenum State {").unwrap();
        assert_eq!(rust.len(), 4);
        assert!(rust.iter().any(|s| s.kind == "trait" && s.name == "Lookup"));
        let python = extract_signatures("python", "app.py", "class Store:\ndef find(key: str) -> str:\n    return key").unwrap();
        assert_eq!(python.len(), 2);
        assert!(python.iter().any(|s| s.name == "find"));
    }

    #[test]
    fn updates_incrementally_and_ranks_matching_context() {
        let mut index = AstSymbolIndex::default();
        index.update_file("rust", "a.rs", "pub fn lookup(key: &str) -> bool {\n}").unwrap();
        index.update_file("rust", "b.rs", "pub fn store(value: i32) {").unwrap();
        assert_eq!(index.matching_context("please lookup this key", 1).len(), 1);
        assert_eq!(index.update_file("rust", "a.rs", "// deleted declarations").unwrap(), 0);
        assert_eq!(index.symbols().count(), 1);
    }

    #[test]
    fn index_is_persisted_in_a_mmap_airw_section() {
        let file = path();
        let mut index = AstSymbolIndex::default();
        index.update_file("python", "app.py", "def parse(source):").unwrap();
        index.write_airw(&file).unwrap();
        let mapped = AstSymbolIndex::open_mapped(&file).unwrap();
        assert!(mapped.is_memory_mapped());
        assert_eq!(mapped.symbols().count(), 1);
        assert_eq!(mapped.matching_context("parse source", 3), vec!["def parse(source)".to_string()]);
        drop(mapped);
        fs::remove_file(file).unwrap();
    }
}
