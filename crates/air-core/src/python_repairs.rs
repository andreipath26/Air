//! Small deterministic Python repair and intent-routing candidates.
//! This is a bounded lexical layer, not a Python parser or general natural-language model.
//! Callers provide a source line/span from their own diagnostics/parser and must verify
//! every proposed edit with a real compiler, AST checker, or task test oracle.

use crate::coding::OracleDecision;
use std::collections::BTreeSet;

pub const MAX_PROMPT_BYTES: usize = 512;
pub const MAX_SOURCE_BYTES: usize = 1024 * 1024;
pub const MAX_SOURCE_LINES: usize = 20_000;
pub const MIN_ROUTE_SCORE: u16 = 600;
pub const MIN_ROUTE_MARGIN: u16 = 200;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PythonRepairKind {
    NarrowBareExcept,
    FreshMutableDefault,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RouteAbstainReason {
    Empty,
    TooLong,
    NoMatch,
    Ambiguous,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PythonRoute {
    Matched { kind: PythonRepairKind, score_per_mille: u16 },
    Abstained { reason: RouteAbstainReason },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PythonRepairTarget {
    BareExcept { line_index: usize },
    MutableDefault { function_line: usize, parameter: String },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RepairAbstainReason {
    NoOracle,
    InputTooLarge,
    UnsupportedSourceShape,
    OracleRejected,
    OracleWithheld,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PythonRepairOutcome {
    Verified { kind: PythonRepairKind, source: String },
    Abstained { reason: RepairAbstainReason },
}

/// The caller owns Python parsing/execution. A candidate is returned to the
/// runtime only if this oracle accepts the exact transformed source.
pub trait PythonRepairOracle {
    fn verify(
        &self,
        kind: PythonRepairKind,
        before: &str,
        after: &str,
    ) -> OracleDecision;
}

const IGNORE: &[&str] = &[
    "a", "an", "change", "code", "fix", "for", "from", "in", "please",
    "python", "repair", "the", "to",
];
const BARE_EXCEPT_ALIASES: &[&[&str]] = &[
    &["bare", "except"],
    &["narrow", "bare", "except"],
    &["broad", "except"],
    &["catch", "all", "exceptions"],
    &["except", "handler"],
];
const MUTABLE_DEFAULT_ALIASES: &[&[&str]] = &[
    &["mutable", "default", "list"],
    &["mutable", "list", "default"],
    &["list", "default", "argument"],
    &["mutable", "default", "dict"],
    &["mutable", "default", "dictionary"],
    &["default", "argument"],
];

/// Routes only a small curated vocabulary using token-set similarity.
/// It does not infer arbitrary programming requests.
pub fn route_prompt(prompt: &str) -> PythonRoute {
    if prompt.is_empty() { return PythonRoute::Abstained { reason: RouteAbstainReason::Empty }; }
    if prompt.len() > MAX_PROMPT_BYTES {
        return PythonRoute::Abstained { reason: RouteAbstainReason::TooLong };
    }
    let tokens = normalize_tokens(prompt);
    if tokens.is_empty() { return PythonRoute::Abstained { reason: RouteAbstainReason::NoMatch }; }

    let bare = best_alias_score(&tokens, BARE_EXCEPT_ALIASES);
    let mutable = best_alias_score(&tokens, MUTABLE_DEFAULT_ALIASES);
    let (kind, best, other) = if bare > mutable {
        (PythonRepairKind::NarrowBareExcept, bare, mutable)
    } else if mutable > bare {
        (PythonRepairKind::FreshMutableDefault, mutable, bare)
    } else {
        return PythonRoute::Abstained { reason: RouteAbstainReason::Ambiguous };
    };
    if best < MIN_ROUTE_SCORE { return PythonRoute::Abstained { reason: RouteAbstainReason::NoMatch }; }
    if best.saturating_sub(other) < MIN_ROUTE_MARGIN {
        return PythonRoute::Abstained { reason: RouteAbstainReason::Ambiguous };
    }
    PythonRoute::Matched { kind, score_per_mille: best }
}

fn normalize_tokens(input: &str) -> BTreeSet<String> {
    let lower = input.to_ascii_lowercase();
    let tokens: BTreeSet<String> = lower
        .split(|ch: char| !ch.is_ascii_alphanumeric() && ch != '_')
        .filter(|word| !word.is_empty() && !IGNORE.contains(word))
        .map(str::to_owned)
        .collect();
    tokens
}

fn best_alias_score(tokens: &BTreeSet<String>, aliases: &[&[&str]]) -> u16 {
    aliases.iter().map(|alias| {
        let alias_set: BTreeSet<&str> = alias.iter().copied().collect();
        let intersection = alias_set.iter().filter(|item| tokens.contains(**item)).count();
        let union = alias_set.len() + tokens.len() - intersection;
        if union == 0 { 0 } else { ((intersection * 1000) / union) as u16 }
    }).max().unwrap_or(0)
}

/// Builds a patch candidate for exactly the supported shapes, asks the caller's
/// oracle to validate it, and exposes the changed source only after acceptance.
pub fn repair(
    source: &str,
    target: PythonRepairTarget,
    oracle: Option<&dyn PythonRepairOracle>,
) -> PythonRepairOutcome {
    let Some(oracle) = oracle else {
        return PythonRepairOutcome::Abstained { reason: RepairAbstainReason::NoOracle };
    };
    if source.len() > MAX_SOURCE_BYTES || line_count(source) > MAX_SOURCE_LINES {
        return PythonRepairOutcome::Abstained { reason: RepairAbstainReason::InputTooLarge };
    }
    let (kind, candidate) = match propose(source, target) {
        Some(candidate) => candidate,
        None => return PythonRepairOutcome::Abstained {
            reason: RepairAbstainReason::UnsupportedSourceShape,
        },
    };
    match oracle.verify(kind, source, &candidate) {
        OracleDecision::Accept => PythonRepairOutcome::Verified { kind, source: candidate },
        OracleDecision::Reject => PythonRepairOutcome::Abstained {
            reason: RepairAbstainReason::OracleRejected,
        },
        OracleDecision::Abstain => PythonRepairOutcome::Abstained {
            reason: RepairAbstainReason::OracleWithheld,
        },
    }
}

fn line_count(source: &str) -> usize {
    source.bytes().filter(|byte| *byte == b'\n').count()
        + usize::from(!source.is_empty() && !source.ends_with('\n'))
}

fn line_bounds(source: &str, wanted: usize) -> Option<(usize, usize, usize)> {
    let mut start = 0;
    let mut line = 0;
    for (index, byte) in source.bytes().enumerate() {
        if byte == b'\n' {
            if line == wanted {
                let mut content_end = index;
                if content_end > start && source.as_bytes()[content_end - 1] == b'\r' {
                    content_end -= 1;
                }
                return Some((start, content_end, index + 1));
            }
            line += 1;
            start = index + 1;
        }
    }
    if line == wanted && start <= source.len() {
        let mut end = source.len();
        if end > start && source.as_bytes()[end - 1] == b'\r' { end -= 1; }
        return Some((start, end, source.len()));
    }
    None
}

fn propose(
    source: &str,
    target: PythonRepairTarget,
) -> Option<(PythonRepairKind, String)> {
    match target {
        PythonRepairTarget::BareExcept { line_index } => {
            let (start, end, _) = line_bounds(source, line_index)?;
            let line = &source[start..end];
            let indent_len = line.len() - line.trim_start_matches([' ', '\t']).len();
            let statement = &line[indent_len..];
            let tail = statement.strip_prefix("except:")?;
            let tail = tail.trim_start();
            if !tail.is_empty() && !tail.starts_with('#') { return None; }
            let mut changed = source.to_owned();
            changed.replace_range(start + indent_len..start + indent_len + "except:".len(), "except Exception:");
            Some((PythonRepairKind::NarrowBareExcept, changed))
        }
        PythonRepairTarget::MutableDefault { function_line, parameter } => {
            propose_mutable_default(source, function_line, &parameter)
                .map(|changed| (PythonRepairKind::FreshMutableDefault, changed))
        }
    }
}

fn propose_mutable_default(source: &str, function_line: usize, parameter: &str) -> Option<String> {
    if !is_identifier(parameter) { return None; }
    let (line_start, line_end, line_full_end) = line_bounds(source, function_line)?;
    if line_full_end == line_end { return None; } // signature needs a line ending and a body
    let line = &source[line_start..line_end];
    let signature_start = line.len() - line.trim_start_matches([' ', '\t']).len();
    let signature = &line[signature_start..];
    let open = signature.find('(')?;
    if !signature.starts_with("def ") { return None; }
    let function_name = signature[4..open].trim();
    if !is_identifier(function_name) { return None; }
    let close = matching_paren(signature, open)?;
    if signature[close + 1..].trim() != ":" { return None; }

    let args_start = open + 1;
    let args_end = close;
    let ranges = split_top_level(&signature[args_start..args_end], b',')?;
    let mut matching_default: Option<(usize, usize)> = None;
    let mut mutable_count = 0;
    for (arg_start, arg_end) in ranges {
        let arg = &signature[args_start + arg_start..args_start + arg_end];
        let trimmed = arg.trim();
        if trimmed.is_empty() { continue; }
        let eq = top_level_byte(arg, b'=')?;
        let Some(eq) = eq else { continue; };
        let left = arg[..eq].trim();
        let default = arg[eq + 1..].trim();
        if default == "[]" || default == "{}" {
            mutable_count += 1;
            if left == parameter {
                let default_offset = arg[eq + 1..].len() - arg[eq + 1..].trim_start().len();
                let value_start = args_start + arg_start + eq + 1 + default_offset;
                let value_end = value_start + default.len();
                matching_default = Some((value_start, value_end));
            }
        }
    }
    if mutable_count != 1 { return None; }
    let (default_start, default_end) = matching_default?;

    let def_indent = &line[..signature_start];
    let mut body_indent: Option<&str> = None;
    let mut first_statement: Option<&str> = None;
    let mut body_line = function_line + 1;
    while let Some((start, end, _)) = line_bounds(source, body_line) {
        let candidate = &source[start..end];
        let trim = candidate.trim_start_matches([' ', '\t']);
        if !trim.is_empty() {
            if !trim.starts_with('#') {
                first_statement = Some(trim);
            }
            let indent_len = candidate.len() - trim.len();
            let indent = &candidate[..indent_len];
            if !indent.starts_with(def_indent) || indent.len() <= def_indent.len() {
                return None;
            }
            body_indent = Some(indent);
            break;
        }
        body_line += 1;
    }
    let body_indent = body_indent?;
    let statement = first_statement?;
    // Inserting before a docstring would change Python's function docstring semantics.
    if statement.starts_with('"') || statement.starts_with('\'') { return None; }
    let unit = &body_indent[def_indent.len()..];
    if unit.bytes().any(|b| b != b' ' && b != b'\t') { return None; }

    let mut changed = source.to_owned();
    let absolute_default_start = line_start + signature_start + default_start;
    let absolute_default_end = line_start + signature_start + default_end;
    let old_len = absolute_default_end - absolute_default_start;
    changed.replace_range(absolute_default_start..absolute_default_end, "None");
    let insertion = line_full_end + ("None".len() - old_len);
    let newline = if source.as_bytes().get(line_end) == Some(&b'\r') { "\r\n" } else { "\n" };
    let literal = if source[line_start + signature_start + default_start..line_start + signature_start + default_end] == *"[]" {
        "[]"
    } else {
        "{}"
    };
    let guard = format!(
        "{body_indent}if {parameter} is None:{newline}{body_indent}{unit}{parameter} = {literal}{newline}"
    );
    changed.insert_str(insertion, &guard);
    Some(changed)
}

fn is_identifier(value: &str) -> bool {
    let mut chars = value.chars();
    let Some(first) = chars.next() else { return false; };
    (first == '_' || first.is_ascii_alphabetic())
        && chars.all(|ch| ch == '_' || ch.is_ascii_alphanumeric())
}

fn matching_paren(input: &str, open: usize) -> Option<usize> {
    let bytes = input.as_bytes();
    let mut depth = 0_i32;
    let mut quote = 0_u8;
    let mut escaped = false;
    for (index, byte) in bytes.iter().copied().enumerate().skip(open) {
        if quote != 0 {
            if escaped { escaped = false; }
            else if byte == b'\\' { escaped = true; }
            else if byte == quote { quote = 0; }
            continue;
        }
        if byte == b'\'' || byte == b'"' { quote = byte; continue; }
        if byte == b'(' { depth += 1; }
        else if byte == b')' {
            depth -= 1;
            if depth == 0 { return Some(index); }
        }
    }
    None
}

fn split_top_level(input: &str, delimiter: u8) -> Option<Vec<(usize, usize)>> {
    let bytes = input.as_bytes();
    let mut parts = Vec::new();
    let mut start = 0;
    let mut stack: Vec<u8> = Vec::new();
    let mut quote = 0_u8;
    let mut escaped = false;
    for (index, byte) in bytes.iter().copied().enumerate() {
        if quote != 0 {
            if escaped { escaped = false; }
            else if byte == b'\\' { escaped = true; }
            else if byte == quote { quote = 0; }
            continue;
        }
        if byte == b'\'' || byte == b'"' { quote = byte; continue; }
        match byte {
            b'(' | b'[' | b'{' => stack.push(byte),
            b')' => if stack.pop()? != b'(' { return None; },
            b']' => if stack.pop()? != b'[' { return None; },
            b'}' => if stack.pop()? != b'{' { return None; },
            _ => {}
        }
        if stack.is_empty() && byte == delimiter {
            parts.push((start, index));
            start = index + 1;
        }
    }
    if quote != 0 || !stack.is_empty() { return None; }
    parts.push((start, input.len()));
    Some(parts)
}

fn top_level_byte(input: &str, needle: u8) -> Option<Option<usize>> {
    split_top_level(input, needle).map(|parts| {
        if parts.len() > 1 { Some(parts[0].1) } else { None }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    struct ExactPostImage(&'static str);
    impl PythonRepairOracle for ExactPostImage {
        fn verify(&self, _: PythonRepairKind, _: &str, after: &str) -> OracleDecision {
            if after == self.0 { OracleDecision::Accept } else { OracleDecision::Reject }
        }
    }

    #[test]
    fn curated_aliases_route_python_repair_intents_only() {
        assert_eq!(
            route_prompt("Please narrow the bare-except clause"),
            PythonRoute::Matched { kind: PythonRepairKind::NarrowBareExcept, score_per_mille: 750 }
        );
        assert!(matches!(
            route_prompt("Make the mutable default list fresh"),
            PythonRoute::Matched { kind: PythonRepairKind::FreshMutableDefault, .. }
        ));
        assert_eq!(
            route_prompt("fix a Python bug"),
            PythonRoute::Abstained { reason: RouteAbstainReason::NoMatch }
        );
        assert_eq!(
            route_prompt(""),
            PythonRoute::Abstained { reason: RouteAbstainReason::Empty }
        );
    }

    #[test]
    fn bare_except_is_changed_only_after_oracle_acceptance() {
        let before = "try:\n    work()\nexcept:\n    recover()\n";
        let after = "try:\n    work()\nexcept Exception:\n    recover()\n";
        let result = repair(
            before,
            PythonRepairTarget::BareExcept { line_index: 2 },
            Some(&ExactPostImage(after)),
        );
        assert_eq!(result, PythonRepairOutcome::Verified {
            kind: PythonRepairKind::NarrowBareExcept,
            source: after.to_string(),
        });
    }

    #[test]
    fn mutable_list_default_is_rewritten_with_a_fresh_per_call_value() {
        let before = "def append_item(items=[]):\n    return items\n";
        let after = "def append_item(items=None):\n    if items is None:\n        items = []\n    return items\n";
        let result = repair(
            before,
            PythonRepairTarget::MutableDefault { function_line: 0, parameter: "items".into() },
            Some(&ExactPostImage(after)),
        );
        assert_eq!(result, PythonRepairOutcome::Verified {
            kind: PythonRepairKind::FreshMutableDefault,
            source: after.to_string(),
        });
    }

    #[test]
    fn mutable_dict_default_keeps_signature_and_body_context() {
        let before = "class Cache:\n    def lookup(self, entries={}):\n        return entries\n";
        let after = "class Cache:\n    def lookup(self, entries=None):\n        if entries is None:\n            entries = {}\n        return entries\n";
        let result = repair(
            before,
            PythonRepairTarget::MutableDefault { function_line: 1, parameter: "entries".into() },
            Some(&ExactPostImage(after)),
        );
        assert_eq!(result, PythonRepairOutcome::Verified {
            kind: PythonRepairKind::FreshMutableDefault,
            source: after.to_string(),
        });
    }

    #[test]
    fn caller_oracle_rejection_and_missing_oracle_abstain() {
        let before = "try:\n    work()\nexcept:\n    recover()\n";
        assert_eq!(
            repair(before, PythonRepairTarget::BareExcept { line_index: 2 }, None),
            PythonRepairOutcome::Abstained { reason: RepairAbstainReason::NoOracle }
        );
        assert_eq!(
            repair(before, PythonRepairTarget::BareExcept { line_index: 2 }, Some(&ExactPostImage("different"))),
            PythonRepairOutcome::Abstained { reason: RepairAbstainReason::OracleRejected }
        );
    }

    #[test]
    fn unsupported_or_ambiguous_shapes_abstain() {
        let annotated = "def append_item(items: list = []):\n    return items\n";
        let two_mutable = "def f(items=[], cache={}):\n    return items\n";
        let docstring = "def f(items=[]):\n    \"\"\"docstring\"\"\"\n    return items\n";
        for source in [annotated, two_mutable, docstring] {
            assert_eq!(
                repair(
                    source,
                    PythonRepairTarget::MutableDefault { function_line: 0, parameter: "items".into() },
                    Some(&ExactPostImage("unused")),
                ),
                PythonRepairOutcome::Abstained { reason: RepairAbstainReason::UnsupportedSourceShape }
            );
        }
    }
}
