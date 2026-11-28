//! Bounded compiler-in-the-loop repair primitives.
//!
//! This module parses rustc and Cargo JSON diagnostics without invoking a
//! compiler. Callers may feed it output from `rustc --error-format=json` or
//! `cargo check --message-format=json`. Deterministic fixes are restricted
//! to unambiguous local edits or rustc's MachineApplicable suggestions.

use serde_json::Value;
use std::ops::Range;

pub const MAX_DIAGNOSTIC_LINE_BYTES: usize = 1_048_576;
pub const MAX_DIAGNOSTICS: usize = 256;
pub const MAX_TARGETED_REPAIR_TURNS: usize = 2;
pub const MAX_REPAIR_CONTEXT_BYTES: usize = 16_384;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceSpan {
    pub file_name: String,
    pub line_start: usize,
    pub column_start: usize,
    pub line_end: usize,
    pub column_end: usize,
    pub is_primary: bool,
    pub label: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SuggestedEdit {
    pub span: SourceSpan,
    pub replacement: String,
    pub applicability: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompilerDiagnostic {
    pub code: Option<String>,
    pub message: String,
    pub level: String,
    pub spans: Vec<SourceSpan>,
    /// Only rustc suggestions explicitly marked MachineApplicable are kept.
    pub machine_applicable_edits: Vec<SuggestedEdit>,
}

impl CompilerDiagnostic {
    pub fn primary_span(&self) -> Option<&SourceSpan> {
        self.spans.iter().find(|span| span.is_primary).or_else(|| self.spans.first())
    }

    pub fn is_structural_borrow_or_lifetime_error(&self) -> bool {
        match self.code.as_deref() {
            Some("E0382" | "E0502" | "E0596" | "E0106" | "E0261" | "E0495" | "E0597" | "E0621") => true,
            _ => self.message.to_ascii_lowercase().contains("lifetime") && self.level == "error",
        }
    }

    pub fn summary_json(&self) -> String {
        // This deliberately excludes the entire input source and arbitrary
        // compiler payload fields when creating a targeted repair request.
        serde_json::json!({
            "code": self.code,
            "message": self.message,
            "level": self.level,
            "primary_span": self.primary_span().map(|span| serde_json::json!({
                "file_name": span.file_name,
                "line_start": span.line_start,
                "column_start": span.column_start,
                "line_end": span.line_end,
                "column_end": span.column_end,
                "label": span.label,
            }))
        }).to_string()
    }
}

/// Parse rustc JSON lines and Cargo `compiler-message` JSON lines. Non-error
/// Cargo events are ignored. Malformed or oversized lines are skipped safely.
pub fn parse_json_diagnostics(input: &str) -> Vec<CompilerDiagnostic> {
    let mut diagnostics = Vec::new();
    for line in input.lines().take(MAX_DIAGNOSTICS.saturating_mul(8)) {
        if line.len() > MAX_DIAGNOSTIC_LINE_BYTES || diagnostics.len() >= MAX_DIAGNOSTICS {
            continue;
        }
        let Ok(root) = serde_json::from_str::<Value>(line) else { continue };
        let message = if root.get("reason").and_then(Value::as_str) == Some("compiler-message") {
            let Some(message) = root.get("message") else { continue };
            message
        } else {
            &root
        };
        let Some(text) = message.get("message").and_then(Value::as_str) else { continue };
        let level = message.get("level").and_then(Value::as_str).unwrap_or("unknown").to_owned();
        let code = message.get("code")
            .and_then(|value| value.get("code"))
            .and_then(Value::as_str)
            .map(str::to_owned);
        let mut spans = Vec::new();
        parse_spans(message.get("spans"), &mut spans, false);
        let mut edits = Vec::new();
        collect_suggestions(message, &mut edits);
        diagnostics.push(CompilerDiagnostic {
            code,
            message: text.to_owned(),
            level,
            spans,
            machine_applicable_edits: edits,
        });
    }
    diagnostics
}

fn parse_spans(value: Option<&Value>, output: &mut Vec<SourceSpan>, suggestions_only: bool) {
    let Some(items) = value.and_then(Value::as_array) else { return };
    for item in items {
        let Some(file_name) = item.get("file_name").and_then(Value::as_str) else { continue };
        let span = SourceSpan {
            file_name: file_name.to_owned(),
            line_start: item.get("line_start").and_then(Value::as_u64).unwrap_or(0) as usize,
            column_start: item.get("column_start").and_then(Value::as_u64).unwrap_or(0) as usize,
            line_end: item.get("line_end").and_then(Value::as_u64).unwrap_or(0) as usize,
            column_end: item.get("column_end").and_then(Value::as_u64).unwrap_or(0) as usize,
            is_primary: item.get("is_primary").and_then(Value::as_bool).unwrap_or(false),
            label: item.get("label").and_then(Value::as_str).map(str::to_owned),
        };
        if !suggestions_only { output.push(span); }
    }
}

fn collect_suggestions(value: &Value, output: &mut Vec<SuggestedEdit>) {
    if let Some(children) = value.get("children").and_then(Value::as_array) {
        for child in children {
            let applicability = child.get("children")
                .and_then(Value::as_array)
                .into_iter().flatten()
                .filter_map(|_| None::<String>)
                .next();
            let _ = applicability;
            let child_applicability = child.get("spans").and_then(Value::as_array)
                .and_then(|spans| spans.iter().find_map(|span| span.get("suggested_replacement")
                    .and_then(Value::as_str).map(|_| span.get("suggestion_applicability")
                        .and_then(Value::as_str).unwrap_or("Unspecified"))))
                .unwrap_or("Unspecified");
            if child_applicability == "MachineApplicable" {
                if let Some(items) = child.get("spans").and_then(Value::as_array) {
                    for item in items {
                        let Some(replacement) = item.get("suggested_replacement").and_then(Value::as_str) else { continue };
                        let mut one = Vec::new();
                        parse_spans(Some(&Value::Array(vec![item.clone()])), &mut one, true);
                        if let Some(span) = one.pop() {
                            output.push(SuggestedEdit {
                                span,
                                replacement: replacement.to_owned(),
                                applicability: child_applicability.to_owned(),
                            });
                        }
                    }
                }
            }
            collect_suggestions(child, output);
        }
    }
}

/// Convert rustc's 1-based line/byte-column coordinates into a source offset.
/// Returns None if a span falls outside the supplied source snippet.
fn source_offset(source: &str, line: usize, column: usize) -> Option<usize> {
    if line == 0 || column == 0 { return None; }
    let mut offset = 0usize;
    for (index, text) in source.split_inclusive('\n').enumerate() {
        if index + 1 == line {
            let byte_column = column - 1;
            if byte_column > text.trim_end_matches('\n').len() { return None; }
            let at = offset.checked_add(byte_column)?;
            if !source.is_char_boundary(at) { return None; }
            return Some(at);
        }
        offset = offset.checked_add(text.len())?;
    }
    if line == source.lines().count() + 1 && column == 1 { Some(source.len()) } else { None }
}

fn edit_range(source: &str, span: &SourceSpan) -> Option<Range<usize>> {
    let start = source_offset(source, span.line_start, span.column_start)?;
    let end = source_offset(source, span.line_end, span.column_end)?;
    (start <= end).then_some(start..end)
}

/// Apply rustc edits only when every edit is MachineApplicable, in-bounds,
/// single-file and non-overlapping. Edits are applied from right to left.
pub fn apply_machine_applicable_edits(source: &str, diagnostic: &CompilerDiagnostic) -> Option<String> {
    if diagnostic.machine_applicable_edits.is_empty() { return None; }
    let first_file = &diagnostic.machine_applicable_edits.first()?.span.file_name;
    let mut edits: Vec<_> = diagnostic.machine_applicable_edits.iter().map(|edit| {
        if edit.applicability != "MachineApplicable" || &edit.span.file_name != first_file { return None; }
        Some((edit_range(source, &edit.span)?, edit.replacement.as_str()))
    }).collect::<Option<_>>()?;
    edits.sort_by_key(|(range, _)| range.start);
    if edits.windows(2).any(|pair| pair[0].0.end > pair[1].0.start) { return None; }
    let mut patched = source.to_owned();
    for (range, replacement) in edits.into_iter().rev() {
        patched.replace_range(range, replacement);
    }
    (patched != source).then_some(patched)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SymbolicPatch {
    pub source: String,
    pub rule: &'static str,
}

/// Conservative Tier 1 patches. Rustc's machine-applicable suggestions take
/// precedence. Heuristic edits are limited to a uniquely identified missing
/// `mut` binding or a small allowlist of standard-library imports.
pub fn tier1_symbolic_patch(source: &str, diagnostic: &CompilerDiagnostic) -> Option<SymbolicPatch> {
    if let Some(source) = apply_machine_applicable_edits(source, diagnostic) {
        return Some(SymbolicPatch { source, rule: "rustc-machine-applicable" });
    }

    if diagnostic.code.as_deref() == Some("E0596")
        && diagnostic.message.contains("as mutable")
    {
        if let Some(name) = first_backticked(&diagnostic.message) {
            let needle = format!("let {name}");
            let mut_positions: Vec<_> = source.match_indices(&needle)
                .filter(|(index, _)| {
                    let after = source.get(index + needle.len()..).and_then(|tail| tail.chars().next());
                    after.map(|ch| !(ch.is_ascii_alphanumeric() || ch == '_')).unwrap_or(true)
                })
                .collect();
            if mut_positions.len() == 1 {
                let (index, _) = mut_positions[0];
                let insert_at = index + "let ".len();
                let mut patched = source.to_owned();
                patched.insert_str(insert_at, "mut ");
                return Some(SymbolicPatch { source: patched, rule: "insert-mut-binding" });
            }
        }
    }

    if matches!(diagnostic.code.as_deref(), Some("E0433" | "E0425")) {
        if let Some(name) = first_backticked(&diagnostic.message) {
            const STD_MODULES: &[&str] = &[
                "cmp", "collections", "env", "fs", "io", "path", "process",
                "sync", "thread", "time",
            ];
            if STD_MODULES.contains(&name.as_str()) {
                let import = format!("use std::{name};\n");
                let existing = format!("use std::{name}");
                if !source.contains(&existing) {
                    return Some(SymbolicPatch {
                        source: format!("{import}{source}"),
                        rule: "add-std-module-import",
                    });
                }
            }
        }
    }
    None
}

fn first_backticked(message: &str) -> Option<String> {
    let start = message.find('`')? + 1;
    let rest = &message[start..];
    let end = rest.find('`')?;
    let name = &rest[..end];
    if !name.is_empty() && name.bytes().all(|byte| byte.is_ascii_alphanumeric() || byte == b'_') {
        Some(name.to_owned())
    } else {
        None
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TargetedRepairRequest {
    /// Only the primary diagnostic's minimal line span, bounded to 16 KiB.
    pub code_span: String,
    pub diagnostic_json: String,
    pub line_start: usize,
    pub line_end: usize,
}

pub trait TargetedRepairBackend {
    type Error: std::fmt::Display;
    /// Returns replacement text for the supplied span, not a whole-file rewrite.
    fn repair_span(&self, request: &TargetedRepairRequest) -> Result<String, Self::Error>;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RepairResult {
    Tier1 { source: String, rule: &'static str },
    Tier2 { source: String, turns: usize },
    Unchanged,
}

fn targeted_request(source: &str, diagnostic: &CompilerDiagnostic) -> Option<(TargetedRepairRequest, Range<usize>)> {
    let span = diagnostic.primary_span()?;
    if span.line_start == 0 || span.line_end < span.line_start { return None; }
    let lines: Vec<_> = source.split_inclusive('\n').collect();
    let start = span.line_start.saturating_sub(1).min(lines.len());
    let end = span.line_end.min(lines.len());
    let context = lines.get(start..end)?.concat();
    if context.is_empty() || context.len() > MAX_REPAIR_CONTEXT_BYTES { return None; }
    let byte_start: usize = lines.iter().take(start).map(|line| line.len()).sum();
    let byte_end = byte_start + context.len();
    Some((TargetedRepairRequest {
        code_span: context,
        diagnostic_json: diagnostic.summary_json(),
        line_start: span.line_start,
        line_end: span.line_end,
    }, byte_start..byte_end))
}

/// Run the deterministic tier first. If it cannot repair a borrow/lifetime
/// diagnostic, request a minimal targeted replacement for at most two turns.
pub fn repair_diagnostic<B: TargetedRepairBackend>(
    source: &str,
    diagnostic: &CompilerDiagnostic,
    backend: Option<&B>,
) -> RepairResult {
    if let Some(patch) = tier1_symbolic_patch(source, diagnostic) {
        return RepairResult::Tier1 { source: patch.source, rule: patch.rule };
    }
    if !diagnostic.is_structural_borrow_or_lifetime_error() { return RepairResult::Unchanged; }
    let Some(backend) = backend else { return RepairResult::Unchanged };
    let Some((mut request, range)) = targeted_request(source, diagnostic) else { return RepairResult::Unchanged };
    let mut whole = source.to_owned();
    let mut turns = 0;
    while turns < MAX_TARGETED_REPAIR_TURNS {
        turns += 1;
        let Ok(replacement) = backend.repair_span(&request) else { break };
        if replacement.is_empty() || replacement.len() > MAX_REPAIR_CONTEXT_BYTES { break; }
        whole.replace_range(range.clone(), &replacement);
        request.code_span = replacement;
        if request.code_span.len() > MAX_REPAIR_CONTEXT_BYTES { break; }
        // This is a targeted proposal loop; the caller must re-run rustc and
        // air-core verification before accepting the result.
        if turns == MAX_TARGETED_REPAIR_TURNS { break; }
    }
    if turns > 0 && whole != source { RepairResult::Tier2 { source: whole, turns } } else { RepairResult::Unchanged }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn diagnostic(code: &str, message: &str, spans: Value) -> CompilerDiagnostic {
        let json = serde_json::json!({
            "message": message,
            "level": "error",
            "code": {"code": code},
            "spans": spans,
        }).to_string();
        parse_json_diagnostics(&json).remove(0)
    }

    #[test]
    fn parses_rustc_and_cargo_compiler_messages() {
        let inner = serde_json::json!({
            "message":"borrow of moved value: `value`",
            "level":"error",
            "code":{"code":"E0382"},
            "spans":[{"file_name":"src/main.rs","line_start":2,"column_start":5,"line_end":2,"column_end":10,"is_primary":true}]
        });
        let cargo = serde_json::json!({"reason":"compiler-message","message":inner});
        let parsed = parse_json_diagnostics(&cargo.to_string());
        assert_eq!(parsed.len(), 1);
        assert_eq!(parsed[0].code.as_deref(), Some("E0382"));
        assert_eq!(parsed[0].primary_span().unwrap().line_start, 2);
        assert!(parsed[0].is_structural_borrow_or_lifetime_error());
    }

    #[test]
    fn tier1_adds_mut_to_unambiguous_binding() {
        let source = "fn f() {\n    let x = String::new();\n    x.push('a');\n}";
        let diag = diagnostic("E0596", "cannot borrow `x` as mutable, as it is not declared as mutable", Value::Array(vec![]));
        let patched = tier1_symbolic_patch(source, &diag).unwrap();
        assert_eq!(patched.rule, "insert-mut-binding");
        assert!(patched.source.contains("let mut x"));
    }

    #[test]
    fn tier1_imports_only_allowlisted_std_modules() {
        let source = "fn f() { let _ = fs::read("x"); }";
        let diag = diagnostic("E0433", "failed to resolve: use of undeclared crate or module `fs`", Value::Array(vec![]));
        let patched = tier1_symbolic_patch(source, &diag).unwrap();
        assert!(patched.source.starts_with("use std::fs;\n"));
        let unknown = diagnostic("E0433", "failed to resolve: use of undeclared crate or module `dangerous`", Value::Array(vec![]));
        assert!(tier1_symbolic_patch(source, &unknown).is_none());
    }

    #[test]
    fn machine_applicable_suggestion_adds_clone() {
        let spans = serde_json::json!([{
            "file_name":"src/main.rs","line_start":1,"column_start":9,"line_end":1,"column_end":10,
            "is_primary":true,"suggested_replacement":"x.clone()","suggestion_applicability":"MachineApplicable"
        }]);
        let diag = diagnostic("E0382", "use of moved value", spans);
        assert_eq!(apply_machine_applicable_edits("let y = x;", &diag).as_deref(), Some("let y = x.clone();"));
    }

    struct TwoTurn;
    impl TargetedRepairBackend for TwoTurn {
        type Error = &'static str;
        fn repair_span(&self, request: &TargetedRepairRequest) -> Result<String, Self::Error> {
            assert!(request.code_span.len() < MAX_REPAIR_CONTEXT_BYTES);
            assert!(request.diagnostic_json.contains("E0502"));
            Ok(request.code_span.replace("borrow()", "borrow_mut()"))
        }
    }

    #[test]
    fn tier2_escalation_is_bounded_to_two_turns_and_span() {
        let source = "fn f() {\n    borrow();\n}\n";
        let spans = serde_json::json!([{
            "file_name":"src/lib.rs","line_start":2,"column_start":5,"line_end":2,"column_end":13,"is_primary":true
        }]);
        let diag = diagnostic("E0502", "cannot borrow as mutable", spans);
        let result = repair_diagnostic(source, &diag, Some(&TwoTurn));
        match result {
            RepairResult::Tier2 { source, turns } => {
                assert!(turns <= MAX_TARGETED_REPAIR_TURNS);
                assert!(turns > 0);
                assert!(source.contains("borrow_mut()"));
            }
            other => panic!("expected targeted repair, got {other:?}"),
        }
    }

    #[test]
    fn malformed_and_oversized_json_are_ignored() {
        let oversized = format!("{{\"message\":\"{}\"}}", "x".repeat(MAX_DIAGNOSTIC_LINE_BYTES));
        assert!(parse_json_diagnostics(&oversized).is_empty());
        assert!(parse_json_diagnostics("not json").is_empty());
    }
}
