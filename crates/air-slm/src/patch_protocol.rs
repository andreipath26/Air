//! Search/replace patch protocol for constrained local SLM outputs.
use air_core::patch::{parse_search_replace, SearchReplaceBlock};

pub const SEARCH_REPLACE_GBNF: &str = r#"root ::= "<<<<<<< SEARCH\\n" body "=======\\n" body ">>>>>>> REPLACE\\n"
body ::= line+
line ::= [^\\n]+ "\\n"
"#;

pub fn patch_prompt(task: &str, source: &str) -> String {
    serde_json::json!({
        "instruction": "Make the smallest code change. Return one or more SEARCH / REPLACE blocks only; do not return a whole file, Markdown fences, or explanation.",
        "task": task,
        "source": source,
        "format": "<<<<<<< SEARCH\\n<exact old text>\\n=======\\n<replacement text>\\n>>>>>>> REPLACE",
    }).to_string()
}

pub fn filter_search_replace(text: &str) -> Result<Vec<SearchReplaceBlock>, String> {
    let trimmed = text.trim();
    if trimmed.contains("\\x60\\x60\\x60") { return Err("patch output cannot contain Markdown fences".into()); }
    let blocks = parse_search_replace(trimmed).map_err(|error| error.to_string())?;
    if blocks.is_empty() { return Err("patch output is empty".into()); }
    Ok(blocks)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exposes_compact_grammar_and_prompt_protocol() {
        assert!(SEARCH_REPLACE_GBNF.contains("<<<<<<< SEARCH"));
        let prompt = patch_prompt("change a return value", "fn f() -> i32 { 1 }");
        assert!(prompt.contains("SEARCH / REPLACE"));
        assert!(prompt.contains("do not return a whole file"));
    }

    #[test]
    fn filters_invalid_search_replace_output() {
        assert!(filter_search_replace("Here is the fix").is_err());
        assert!(filter_search_replace("\\x60\\x60\\x60rust\\nfn f() {}\\n\\x60\\x60\\x60").is_err());
        assert!(filter_search_replace("<<<<<<< SEARCH\\nx\\n=======\\ny\\n>>>>>>> REPLACE").is_ok());
    }
}
