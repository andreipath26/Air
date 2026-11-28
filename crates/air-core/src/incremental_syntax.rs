//! Incremental Rust/Python syntax trees using tree-sitter InputEdit.
use std::time::{Duration, Instant};
use tree_sitter::{InputEdit, Language, Parser, Point, Tree};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SyntaxLanguage { Rust, Python }
impl SyntaxLanguage {
    fn grammar(self) -> Language {
        match self {
            Self::Rust => tree_sitter_rust::LANGUAGE.into(),
            Self::Python => tree_sitter_python::LANGUAGE.into(),
        }
    }
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SyntaxError {
    InvalidRange,
    NonCharacterBoundary,
    ParserSetup(String),
    ParseCancelled,
}
impl std::fmt::Display for SyntaxError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidRange => f.write_str("edit range is invalid"),
            Self::NonCharacterBoundary => f.write_str("edit range is not on UTF-8 boundaries"),
            Self::ParserSetup(error) => write!(f, "tree-sitter parser setup failed: {error}"),
            Self::ParseCancelled => f.write_str("tree-sitter parse was cancelled"),
        }
    }
}
impl std::error::Error for SyntaxError {}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EditMetrics { pub incremental_parse: Duration }

pub struct IncrementalSyntaxDocument {
    language: SyntaxLanguage,
    parser: Parser,
    source: String,
    tree: Tree,
}
impl IncrementalSyntaxDocument {
    pub fn new(language: SyntaxLanguage, source: impl Into<String>) -> Result<Self, SyntaxError> {
        let source = source.into();
        let mut parser = Parser::new();
        let grammar = language.grammar();
        parser.set_language(&grammar).map_err(|error| SyntaxError::ParserSetup(error.to_string()))?;
        let tree = parser.parse(source.as_bytes(), None).ok_or(SyntaxError::ParseCancelled)?;
        Ok(Self { language, parser, source, tree })
    }

    pub fn source(&self) -> &str { &self.source }
    pub fn tree(&self) -> &Tree { &self.tree }

    /// Apply a byte-range replacement and incrementally reparse using tree.edit.
    pub fn apply_edit(&mut self, start_byte: usize, old_end_byte: usize, replacement: &str) -> Result<EditMetrics, SyntaxError> {
        if start_byte > old_end_byte || old_end_byte > self.source.len() { return Err(SyntaxError::InvalidRange); }
        if !self.source.is_char_boundary(start_byte) || !self.source.is_char_boundary(old_end_byte) {
            return Err(SyntaxError::NonCharacterBoundary);
        }
        let start_position = point_at(&self.source, start_byte);
        let old_end_position = point_at(&self.source, old_end_byte);
        let mut updated_source = self.source.clone();
        updated_source.replace_range(start_byte..old_end_byte, replacement);
        let new_end_byte = start_byte + replacement.len();
        let new_end_position = point_at(&updated_source, new_end_byte);

        let edit = InputEdit {
            start_byte,
            old_end_byte,
            new_end_byte,
            start_position,
            old_end_position,
            new_end_position,
        };
        let mut edited_tree = self.tree.clone();
        edited_tree.edit(&edit);
        let started = Instant::now();
        let updated_tree = self.parser.parse(updated_source.as_bytes(), Some(&edited_tree))
            .ok_or(SyntaxError::ParseCancelled)?;
        let elapsed = started.elapsed();
        self.source = updated_source;
        self.tree = updated_tree;
        Ok(EditMetrics { incremental_parse: elapsed })
    }
    pub fn language(&self) -> SyntaxLanguage { self.language }
}

fn point_at(source: &str, byte: usize) -> Point {
    let prefix = &source[..byte];
    let row = prefix.bytes().filter(|byte| *byte == b'\n').count();
    let column = prefix.rsplit('\n').next().unwrap_or("").len();
    Point { row, column }
}

pub fn full_parse_duration(language: SyntaxLanguage, source: &str) -> Result<Duration, SyntaxError> {
    let mut parser = Parser::new();
    let grammar = language.grammar();
    parser.set_language(&grammar).map_err(|error| SyntaxError::ParserSetup(error.to_string()))?;
    let started = Instant::now();
    parser.parse(source.as_bytes(), None).ok_or(SyntaxError::ParseCancelled)?;
    Ok(started.elapsed())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rust_and_python_incremental_edits_match_fresh_parses() {
        let mut rust = IncrementalSyntaxDocument::new(SyntaxLanguage::Rust, "pub fn answer() -> i32 {\n    41\n}\n").unwrap();
        let at = rust.source().find("41").unwrap();
        rust.apply_edit(at, at + 2, "42").unwrap();
        let fresh = IncrementalSyntaxDocument::new(SyntaxLanguage::Rust, rust.source()).unwrap();
        assert_eq!(rust.tree().root_node().to_sexp(), fresh.tree().root_node().to_sexp());

        let mut python = IncrementalSyntaxDocument::new(SyntaxLanguage::Python, "def answer():\n    return 41\n").unwrap();
        let at = python.source().find("41").unwrap();
        python.apply_edit(at, at + 2, "42").unwrap();
        let fresh = IncrementalSyntaxDocument::new(SyntaxLanguage::Python, python.source()).unwrap();
        assert_eq!(python.tree().root_node().to_sexp(), fresh.tree().root_node().to_sexp());
    }

    #[test]
    fn rejects_invalid_or_non_utf8_edits() {
        let mut doc = IncrementalSyntaxDocument::new(SyntaxLanguage::Rust, "fn f() { let x = \"é\"; }").unwrap();
        assert_eq!(doc.apply_edit(2, 1, ""), Err(SyntaxError::InvalidRange));
        let at = doc.source().find('é').unwrap();
        assert_eq!(doc.apply_edit(at + 1, at + 2, ""), Err(SyntaxError::NonCharacterBoundary));
    }

    #[test]
    fn reports_incremental_latency_on_a_large_rust_file() {
        let mut source = String::from("pub fn generated() {\n");
        for index in 0..600 { source.push_str(&format!("    let v{index} = {index};\n")); }
        source.push_str("    let _ = v599;\n}\n");
        let mut doc = IncrementalSyntaxDocument::new(SyntaxLanguage::Rust, source.clone()).unwrap();
        let at = source.find("v599 = 599").unwrap() + "v599 = ".len();
        let incremental = doc.apply_edit(at, at + 3, "600").unwrap();
        let full = full_parse_duration(SyntaxLanguage::Rust, doc.source()).unwrap();
        eprintln!("TREE_SITTER_BENCH lines=603 incremental_us={} full_parse_us={}", incremental.incremental_parse.as_micros(), full.as_micros());
        assert_eq!(doc.tree().root_node().to_sexp(), IncrementalSyntaxDocument::new(SyntaxLanguage::Rust, doc.source()).unwrap().tree().root_node().to_sexp());
    }
}
