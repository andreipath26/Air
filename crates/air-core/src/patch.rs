//! Transactional SEARCH / REPLACE patches for bounded code edits.
use std::{error::Error, fmt};

pub const MAX_PATCH_BYTES: usize = 64 * 1024;
pub const MAX_PATCH_BLOCKS: usize = 64;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SearchReplaceBlock {
    pub search: String,
    pub replace: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PatchError {
    TooLarge,
    TooManyBlocks,
    InvalidFormat(&'static str),
    SearchNotFound,
    AmbiguousSearch,
}
impl fmt::Display for PatchError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::TooLarge => f.write_str("patch exceeds byte limit"),
            Self::TooManyBlocks => f.write_str("patch exceeds block limit"),
            Self::InvalidFormat(reason) => write!(f, "invalid SEARCH / REPLACE patch: {reason}"),
            Self::SearchNotFound => f.write_str("SEARCH block did not match source"),
            Self::AmbiguousSearch => f.write_str("SEARCH block matched more than once"),
        }
    }
}
impl Error for PatchError {}

/// Parse one or more exact delimiter blocks.
pub fn parse_search_replace(input: &str) -> Result<Vec<SearchReplaceBlock>, PatchError> {
    if input.len() > MAX_PATCH_BYTES { return Err(PatchError::TooLarge); }
    let lines: Vec<&str> = input.lines().collect();
    let mut blocks = Vec::new();
    let mut cursor = 0usize;
    while cursor < lines.len() {
        while cursor < lines.len() && lines[cursor].trim().is_empty() { cursor += 1; }
        if cursor == lines.len() { break; }
        if lines[cursor].trim_end_matches('\r') != "<<<<<<< SEARCH" {
            return Err(PatchError::InvalidFormat("expected <<<<<<< SEARCH"));
        }
        cursor += 1;
        let search_start = cursor;
        while cursor < lines.len() && lines[cursor].trim_end_matches('\r') != "=======" { cursor += 1; }
        if cursor == lines.len() { return Err(PatchError::InvalidFormat("missing =======")); }
        let search = lines[search_start..cursor].join("\n");
        cursor += 1;
        let replace_start = cursor;
        while cursor < lines.len() && lines[cursor].trim_end_matches('\r') != ">>>>>>> REPLACE" { cursor += 1; }
        if cursor == lines.len() { return Err(PatchError::InvalidFormat("missing >>>>>>> REPLACE")); }
        let replace = lines[replace_start..cursor].join("\n");
        cursor += 1;
        if search.is_empty() { return Err(PatchError::InvalidFormat("empty SEARCH block")); }
        blocks.push(SearchReplaceBlock { search, replace });
        if blocks.len() > MAX_PATCH_BLOCKS { return Err(PatchError::TooManyBlocks); }
    }
    if blocks.is_empty() { return Err(PatchError::InvalidFormat("no patch blocks")); }
    Ok(blocks)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PatchApplication {
    Applied(String),
    Rejected { original: String, error: PatchError },
}
impl PatchApplication {
    pub fn source(&self) -> &str {
        match self { Self::Applied(source) => source, Self::Rejected { original, .. } => original }
    }
    pub fn was_applied(&self) -> bool { matches!(self, Self::Applied(_)) }
}

/// Apply blocks transactionally. Matching is exact first, then whitespace
/// tolerant at line boundaries. Every SEARCH region must be unique; failure
/// returns the untouched original, even if earlier blocks matched.
pub fn apply_search_replace(source: &str, input: &str) -> PatchApplication {
    match parse_search_replace(input).and_then(|blocks| apply_blocks(source, &blocks)) {
        Ok(updated) => PatchApplication::Applied(updated),
        Err(error) => PatchApplication::Rejected { original: source.to_owned(), error },
    }
}

pub fn apply_blocks(source: &str, blocks: &[SearchReplaceBlock]) -> Result<String, PatchError> {
    let mut candidate = source.to_owned();
    for block in blocks {
        let range = locate_unique(&candidate, &block.search)?;
        let mut replacement = block.replace.clone();
        if candidate[range.clone()].ends_with('\n') && !replacement.ends_with('\n') {
            replacement.push('\n');
        }
        candidate.replace_range(range, &replacement);
    }
    Ok(candidate)
}

fn locate_unique(source: &str, search: &str) -> Result<std::ops::Range<usize>, PatchError> {
    let mut exact = source.match_indices(search);
    if let Some((start, _)) = exact.next() {
        if exact.next().is_some() { return Err(PatchError::AmbiguousSearch); }
        return Ok(start..start + search.len());
    }
    locate_whitespace_tolerant(source, search)
}

fn locate_whitespace_tolerant(source: &str, search: &str) -> Result<std::ops::Range<usize>, PatchError> {
    let wanted: Vec<&str> = search.lines().map(str::trim).collect();
    if wanted.is_empty() || wanted.iter().all(|line| line.is_empty()) {
        return Err(PatchError::InvalidFormat("SEARCH block contains no code"));
    }
    let source_lines: Vec<(usize, usize, &str)> = source.split_inclusive('\n').scan(0usize, |offset, line| {
        let start = *offset;
        *offset += line.len();
        let without_newline = line.strip_suffix('\n').unwrap_or(line);
        let text = without_newline.strip_suffix('\r').unwrap_or(without_newline);
        Some((start, *offset, text))
    }).collect();
    let mut found = None;
    for start in 0..source_lines.len() {
        let end = start + wanted.len();
        if end > source_lines.len() { break; }
        if (0..wanted.len()).all(|index| source_lines[start + index].2.trim() == wanted[index]) {
            if found.is_some() { return Err(PatchError::AmbiguousSearch); }
            found = Some(source_lines[start].0..source_lines[end - 1].1);
        }
    }
    found.ok_or(PatchError::SearchNotFound)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Instant;

    #[test]
    fn applies_exact_search_replace_block() {
        let source = "fn value() {\n    1\n}\n";
        let patch = "<<<<<<< SEARCH\n    1\n=======\n    2\n>>>>>>> REPLACE";
        assert_eq!(apply_search_replace(source, patch).source(), "fn value() {\n    2\n}\n");
    }

    #[test]
    fn whitespace_tolerant_match_preserves_source_outside_region() {
        let source = "fn value() {\n\tlet  x=1;   \n    x\n}\n";
        let patch = "<<<<<<< SEARCH\n    let x = 1;\n    x\n=======\n    let x = 2;\n    x\n>>>>>>> REPLACE";
        assert_eq!(apply_search_replace(source, patch).source(), "fn value() {\n    let x = 2;\n    x\n}\n");
    }

    #[test]
    fn failed_later_block_rolls_back_all_earlier_changes() {
        let source = "first\nsecond\n";
        let patch = "<<<<<<< SEARCH\nfirst\n=======\nchanged\n>>>>>>> REPLACE\n<<<<<<< SEARCH\nmissing\n=======\nnew\n>>>>>>> REPLACE";
        let result = apply_search_replace(source, patch);
        assert!(!result.was_applied());
        assert_eq!(result.source(), source);
        assert_eq!(result, PatchApplication::Rejected { original: source.into(), error: PatchError::SearchNotFound });
    }

    #[test]
    fn duplicate_matches_are_rejected_without_partial_write() {
        let source = "x\nx\n";
        let result = apply_search_replace(source, "<<<<<<< SEARCH\nx\n=======\ny\n>>>>>>> REPLACE");
        assert!(matches!(result, PatchApplication::Rejected { error: PatchError::AmbiguousSearch, .. }));
        assert_eq!(result.source(), source);
    }

    #[test]
    fn multi_hundred_line_rust_patch_latency_is_reported() {
        let mut source = String::from("pub fn generated() {\n");
        for index in 0..600 { source.push_str(&format!("    let v{index} = {index};\n")); }
        source.push_str("    let _ = v599;\n}\n");
        let patch = "<<<<<<< SEARCH\n    let v599 = 599;\n=======\n    let v599 = 600;\n>>>>>>> REPLACE";
        let started = Instant::now();
        let result = apply_search_replace(&source, patch);
        let elapsed = started.elapsed();
        assert!(result.was_applied());
        assert!(result.source().contains("let v599 = 600;"));
        eprintln!("SEARCH_REPLACE_BENCH lines=603 elapsed_us={}", elapsed.as_micros());
    }
}
