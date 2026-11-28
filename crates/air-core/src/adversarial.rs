//! Deterministic adversarial probes for verified solution promotion.
//!
//! This module generates boundary-focused probe cases from a Rust function
//! signature. Production callers must use a runner backed by air-core's
//! unprivileged process sandbox; the trait is an integration seam, not itself
//! a security boundary.

use std::{error::Error, fmt};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProbeKind {
    EmptyInput,
    ZeroBoundary,
    MaxBoundary,
    BorrowContention,
    ResultErrorPath,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExpectedOutcome {
    Completes,
    ReturnsError,
    CompileRejected,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AdversarialProbe {
    pub name: String,
    pub kind: ProbeKind,
    /// Rust test or compile-fail source to run with the candidate source.
    pub source: String,
    pub expected: ExpectedOutcome,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CriticExecution {
    pub sandboxed: bool,
    pub primary_passed: bool,
    pub adversarial_passed: usize,
    pub adversarial_total: usize,
}

#[derive(Debug)]
pub enum CriticError {
    UnsupportedLanguage,
    InvalidSignature,
    NoApplicableProbes,
    Runner(String),
    Rejected,
    Distillation(crate::distillation::DistillationError),
}
impl fmt::Display for CriticError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnsupportedLanguage => f.write_str("adversarial synthesis currently supports Rust"),
            Self::InvalidSignature => f.write_str("could not parse a supported Rust function signature"),
            Self::NoApplicableProbes => f.write_str("signature produced no applicable adversarial probes"),
            Self::Runner(error) => write!(f, "sandboxed critic runner failed: {error}"),
            Self::Rejected => f.write_str("primary or adversarial verification failed or was not sandboxed"),
            Self::Distillation(error) => write!(f, "distillation failed: {error}"),
        }
    }
}
impl Error for CriticError {}
impl From<crate::distillation::DistillationError> for CriticError {
    fn from(value: crate::distillation::DistillationError) -> Self { Self::Distillation(value) }
}

/// Implemented by a runner that compiles and executes all supplied probe code
/// inside an OS sandbox. A runner must not set sandboxed=true unless all
/// primary and adversarial checks actually ran under that sandbox.
pub trait SandboxedAdversarialRunner {
    type Error: fmt::Display;
    fn run_primary_and_adversarial(
        &self,
        language: &str,
        candidate: &str,
        primary_tests: &str,
        probes: &[AdversarialProbe],
    ) -> Result<CriticExecution, Self::Error>;
}

/// Opaque evidence bound to the exact candidate and prompt. Only a successful
/// run through run_adversarial_critic can construct this proof.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CriticProof {
    fingerprint: String,
    cases_passed: usize,
    sandboxed: bool,
}
impl CriticProof {
    pub fn cases_passed(&self) -> usize { self.cases_passed }
    pub fn is_sandboxed(&self) -> bool { self.sandboxed }

    pub(crate) fn matches(&self, language: &str, prompt: &str, candidate: &str) -> bool {
        self.sandboxed && self.fingerprint == candidate_fingerprint(language, prompt, candidate)
    }
}

fn candidate_fingerprint(language: &str, prompt: &str, candidate: &str) -> String {
    use sha2::{Digest, Sha256};
    let mut hash = Sha256::new();
    hash.update(language.as_bytes());
    hash.update([0]);
    hash.update(prompt.as_bytes());
    hash.update([0]);
    hash.update(candidate.as_bytes());
    format!("{:x}", hash.finalize())
}

#[derive(Debug)]
struct FunctionSignature {
    name: String,
    params: Vec<String>,
    returns: String,
}

fn matching_delimiter(text: &str, open_at: usize, open: char, close: char) -> Option<usize> {
    let mut depth = 0usize;
    for (offset, ch) in text[open_at..].char_indices() {
        if ch == open { depth += 1; }
        if ch == close {
            depth = depth.checked_sub(1)?;
            if depth == 0 { return Some(open_at + offset); }
        }
    }
    None
}

fn split_top_level(text: &str) -> Vec<&str> {
    let mut parts = Vec::new();
    let mut start = 0usize;
    let mut angles = 0i32;
    let mut parens = 0i32;
    let mut brackets = 0i32;
    for (index, ch) in text.char_indices() {
        match ch {
            '<' => angles += 1,
            '>' => angles = (angles - 1).max(0),
            '(' => parens += 1,
            ')' => parens = (parens - 1).max(0),
            '[' => brackets += 1,
            ']' => brackets = (brackets - 1).max(0),
            ',' if angles == 0 && parens == 0 && brackets == 0 => {
                let item = text[start..index].trim();
                if !item.is_empty() { parts.push(item); }
                start = index + 1;
            }
            _ => {}
        }
    }
    let final_item = text[start..].trim();
    if !final_item.is_empty() { parts.push(final_item); }
    parts
}

fn parse_signature(signature: &str) -> Option<FunctionSignature> {
    let fn_at = signature.find("fn ")? + 3;
    let rest = signature[fn_at..].trim_start();
    let name_len = rest.find(|ch: char| !(ch.is_ascii_alphanumeric() || ch == '_')).unwrap_or(rest.len());
    if name_len == 0 { return None; }
    let name = rest[..name_len].to_owned();
    let paren_rel = rest.find('(')?;
    let paren_at = fn_at + signature[fn_at..].len() - rest.len() + paren_rel;
    let close = matching_delimiter(signature, paren_at, '(', ')')?;
    let args = &signature[paren_at + 1..close];
    let params = split_top_level(args).into_iter().map(|param| {
        param.split_once(':').map(|(_, ty)| ty.trim().to_owned())
    }).collect::<Option<Vec<_>>>()?;
    let after = signature[close + 1..].trim_start();
    let returns = after.strip_prefix("->").map(str::trim).unwrap_or("");
    Some(FunctionSignature { name, params, returns: returns.trim_end_matches('{').trim().to_owned() })
}

fn canonical_type(ty: &str) -> String {
    let trimmed = ty.trim();
    if let Some(rest) = trimmed.strip_prefix("&'") {
        if let Some(space) = rest.find(char::is_whitespace) {
            return format!("&{}", rest[space..].trim().replace(' ', ""));
        }
    }
    trimmed.replace(' ', "")
}

fn identifier_type(ty: &str) -> Option<&str> {
    let compact = canonical_type(ty);
    let name = compact.rsplit("::").next().unwrap_or(&compact);
    ["u8", "u16", "u32", "u64", "u128", "usize", "i8", "i16", "i32", "i64", "i128", "isize"]
        .iter().copied().find(|primitive| name == *primitive)
}

fn value_for_type(ty: &str, edge: &str) -> Option<String> {
    let compact = canonical_type(ty);
    if compact == "&str" { return Some(if edge == "empty" { r#""""#.into() } else { r#""air""#.into() }); }
    if compact == "String" { return Some(if edge == "empty" { "String::new()".into() } else { r#"String::from("air")"#.into() }); }
    if compact.starts_with("&[") { return Some("&[]".into()); }
    if compact.starts_with("&mut[") { return Some("&mut []".into()); }
    if compact.starts_with("Vec<") { return Some("Vec::new()".into()); }
    if compact.starts_with("Option<") { return Some("None".into()); }
    if compact == "bool" { return Some("false".into()); }
    if let Some(integer) = identifier_type(&compact) {
        return Some(if edge == "max" { format!("{integer}::MAX") } else { "0".into() });
    }
    if compact == "&i32" { return Some("&0".into()); }
    if compact == "&u32" { return Some("&0".into()); }
    if compact == "&muti32" { return Some("&mut 0".into()); }
    if compact == "&mutu32" { return Some("&mut 0".into()); }
    None
}

fn test_source(function: &FunctionSignature, args: &[String], assert_error: bool) -> String {
    let invocation = format!("{}({})", function.name, args.join(", "));
    if assert_error {
        format!("assert!({invocation}.is_err(), "empty input should exercise the error path");")
    } else {
        format!(
            "assert!(std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {{ let _ = {invocation}; }})).is_ok(), "adversarial input panicked");"
        )
    }
}

/// Generate deterministic probes from a Rust free-function signature.
/// For Result-returning parse/decode/read-style functions, empty string input
/// also becomes an expected error-path case. Unknown parameter types are not
/// guessed or instantiated.
pub fn synthesize_signature_probes(
    language: &str,
    signature: &str,
) -> Result<Vec<AdversarialProbe>, CriticError> {
    if language != "rust" { return Err(CriticError::UnsupportedLanguage); }
    let function = parse_signature(signature).ok_or(CriticError::InvalidSignature)?;
    let mut baseline = Vec::with_capacity(function.params.len());
    for ty in &function.params {
        baseline.push(value_for_type(ty, "normal").ok_or(CriticError::InvalidSignature)?);
    }

    let mut probes = Vec::new();
    for (index, ty) in function.params.iter().enumerate() {
        let compact = canonical_type(ty);
        let mut args = baseline.clone();
        if compact.starts_with("&[") || compact.starts_with("&mut[") || compact.starts_with("Vec<") {
            args[index] = value_for_type(ty, "empty").ok_or(CriticError::InvalidSignature)?;
            probes.push(AdversarialProbe {
                name: format!("empty_input_param_{index}"),
                kind: ProbeKind::EmptyInput,
                source: test_source(&function, &args, false),
                expected: ExpectedOutcome::Completes,
            });
            let element_type = if compact.starts_with("&[") {
                compact.strip_prefix("&[").and_then(|value| value.strip_suffix(']'))
            } else if compact.starts_with("Vec<") {
                compact.strip_prefix("Vec<").and_then(|value| value.strip_suffix('>'))
            } else { None };
            if let Some(integer) = element_type.and_then(identifier_type) {
                args[index] = if compact.starts_with("Vec<") {
                    format!("vec![0{integer}]")
                } else { format!("&[0{integer}]") };
                probes.push(AdversarialProbe {
                    name: format!("zero_boundary_element_{index}"),
                    kind: ProbeKind::ZeroBoundary,
                    source: test_source(&function, &args, false),
                    expected: ExpectedOutcome::Completes,
                });
                args[index] = if compact.starts_with("Vec<") {
                    format!("vec![{integer}::MAX]")
                } else { format!("&[{integer}::MAX]") };
                probes.push(AdversarialProbe {
                    name: format!("max_boundary_element_{index}"),
                    kind: ProbeKind::MaxBoundary,
                    source: test_source(&function, &args, false),
                    expected: ExpectedOutcome::Completes,
                });
            }
        } else if compact == "&str" || compact == "String" {
            args[index] = value_for_type(ty, "empty").ok_or(CriticError::InvalidSignature)?;
            probes.push(AdversarialProbe {
                name: format!("empty_string_param_{index}"),
                kind: ProbeKind::EmptyInput,
                source: test_source(&function, &args, false),
                expected: ExpectedOutcome::Completes,
            });
            let lower_name = function.name.to_ascii_lowercase();
            if function.returns.starts_with("Result<")
                && ["parse", "decode", "read", "load"].iter().any(|key| lower_name.contains(key))
            {
                probes.push(AdversarialProbe {
                    name: format!("empty_string_error_param_{index}"),
                    kind: ProbeKind::ResultErrorPath,
                    source: test_source(&function, &args, true),
                    expected: ExpectedOutcome::ReturnsError,
                });
            }
        } else if identifier_type(&compact).is_some() {
            args[index] = value_for_type(ty, "zero").ok_or(CriticError::InvalidSignature)?;
            probes.push(AdversarialProbe {
                name: format!("zero_boundary_param_{index}"),
                kind: ProbeKind::ZeroBoundary,
                source: test_source(&function, &args, false),
                expected: ExpectedOutcome::Completes,
            });
            args[index] = value_for_type(ty, "max").ok_or(CriticError::InvalidSignature)?;
            probes.push(AdversarialProbe {
                name: format!("max_boundary_param_{index}"),
                kind: ProbeKind::MaxBoundary,
                source: test_source(&function, &args, false),
                expected: ExpectedOutcome::Completes,
            });
        } else if compact == "bool" || compact.starts_with("Option<") {
            args[index] = value_for_type(ty, "empty").ok_or(CriticError::InvalidSignature)?;
            probes.push(AdversarialProbe {
                name: format!("empty_or_none_param_{index}"),
                kind: ProbeKind::EmptyInput,
                source: test_source(&function, &args, false),
                expected: ExpectedOutcome::Completes,
            });
        }

        if compact.starts_with('&') {
            probes.push(AdversarialProbe {
                name: format!("borrowed_input_scope_param_{index}"),
                kind: ProbeKind::BorrowContention,
                source: format!(
                    "{{ let _owned = String::from("air"); let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {{ let _ = {}({}); }})); }}",
                    function.name,
                    function.params.iter().enumerate().map(|(i, ty)| {
                        if i == index {
                            if canonical_type(ty) == "&str" { ""air"".to_owned() }
                            else if canonical_type(ty) == "&[u8]" { "&[0u8]".to_owned() }
                            else if canonical_type(ty) == "&[i32]" { "&[0i32]".to_owned() }
                            else if canonical_type(ty) == "&i32" { "&0i32".to_owned() }
                            else if canonical_type(ty) == "&u32" { "&0u32".to_owned() }
                            else if canonical_type(ty) == "&muti32" { "&mut 0i32".to_owned() }
                            else if canonical_type(ty) == "&mutu32" { "&mut 0u32".to_owned() }
                            else { baseline[i].clone() }
                        } else { baseline[i].clone() }
                    }).collect::<Vec<_>>().join(", ")
                ),
                expected: ExpectedOutcome::Completes,
            });
        }
    }

    if probes.is_empty() { return Err(CriticError::NoApplicableProbes); }
    Ok(probes)
}


/// Run the primary prompt tests and generated probes. A successful proof is
/// bound to exactly these bytes so it cannot authorize another candidate.
pub fn run_adversarial_critic<R: SandboxedAdversarialRunner>(
    runner: &R,
    language: &str,
    prompt: &str,
    signature: &str,
    candidate: &str,
    primary_tests: &str,
) -> Result<CriticProof, CriticError> {
    let probes = synthesize_signature_probes(language, signature)?;
    let execution = runner.run_primary_and_adversarial(language, candidate, primary_tests, &probes)
        .map_err(|error| CriticError::Runner(error.to_string()))?;
    if !execution.sandboxed || !execution.primary_passed
        || execution.adversarial_total != probes.len()
        || execution.adversarial_passed != probes.len()
    {
        return Err(CriticError::Rejected);
    }
    Ok(CriticProof {
        fingerprint: candidate_fingerprint(language, prompt, candidate),
        cases_passed: execution.adversarial_passed,
        sandboxed: true,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    struct MockRunner { fail_probe: Option<String> }
    impl SandboxedAdversarialRunner for MockRunner {
        type Error = &'static str;
        fn run_primary_and_adversarial(
            &self,
            _language: &str,
            candidate: &str,
            primary_tests: &str,
            probes: &[AdversarialProbe],
        ) -> Result<CriticExecution, Self::Error> {
            let failed = self.fail_probe.as_deref()
                .filter(|_| candidate.contains("brittle") || primary_tests.contains("fail"));
            Ok(CriticExecution {
                sandboxed: true,
                primary_passed: failed.is_none(),
                adversarial_passed: probes.len() - usize::from(failed.is_some()),
                adversarial_total: probes.len(),
            })
        }
    }

    #[test]
    fn signature_synthesizes_empty_numeric_borrow_and_error_probes() {
        let probes = synthesize_signature_probes(
            "rust",
            "fn parse_count(input: &str, limit: usize, values: &[u32]) -> Result<u32, ParseError>",
        ).unwrap();
        assert!(probes.iter().any(|p| p.kind == ProbeKind::EmptyInput));
        assert!(probes.iter().any(|p| p.kind == ProbeKind::ZeroBoundary));
        assert!(probes.iter().any(|p| p.kind == ProbeKind::MaxBoundary));
        assert!(probes.iter().any(|p| p.kind == ProbeKind::BorrowContention));
        assert!(probes.iter().any(|p| p.kind == ProbeKind::ResultErrorPath));
    }

    #[test]
    fn rejects_overfitted_solution_when_adversarial_probe_fails() {
        let runner = MockRunner { fail_probe: Some("overfit".into()) };
        assert!(matches!(
            run_adversarial_critic(&runner, "rust", "parse a bounded count", "fn parse(input: &str) -> Result<u32, E>", "fn parse(input: &str) -> Result<u32, E> { /* brittle */ }", "#[test] fn primary() {}"),
            Err(CriticError::Rejected)
        ));
    }

    #[test]
    fn robust_solution_gets_candidate_bound_sandboxed_proof() {
        let runner = MockRunner { fail_probe: None };
        let prompt = "parse a bounded count";
        let candidate = "fn parse(input: &str) -> Result<u32, E> { input.parse().map_err(|_| E) }";
        let proof = run_adversarial_critic(
            &runner, "rust", prompt, "fn parse(input: &str) -> Result<u32, E>", candidate, "#[test] fn primary() {}"
        ).unwrap();
        assert!(proof.is_sandboxed());
        assert!(proof.cases_passed() >= 2);
        assert!(proof.matches("rust", prompt, candidate));
        assert!(!proof.matches("rust", prompt, "fn parse(input: &str) -> Result<u32, E> { /* brittle */ }"));
        let path = std::env::temp_dir().join(format!("air-critic-{}.airw", std::process::id()));
        let _ = std::fs::remove_file(&path);
        let store = crate::distillation::OverlayStore::new(&path);
        let verified = crate::distillation::Verification { compiler_clean: true, tests_passed: true };
        assert_eq!(store.append_critically_verified("rust", prompt, candidate, verified, &proof).unwrap(), 1);
        assert!(matches!(
            store.append_critically_verified("rust", prompt, "fn parse(input: &str) -> Result<u32, E> { /* different */ }", verified, &proof),
            Err(crate::distillation::DistillationError::CriticRejected)
        ));
        std::fs::remove_file(path).unwrap();
    }

    #[test]
    fn refuses_runner_that_did_not_sandbox() {
        struct NotSandboxed;
        impl SandboxedAdversarialRunner for NotSandboxed {
            type Error = &'static str;
            fn run_primary_and_adversarial(&self, _l: &str, _c: &str, _p: &str, probes: &[AdversarialProbe]) -> Result<CriticExecution, Self::Error> {
                Ok(CriticExecution { sandboxed: false, primary_passed: true, adversarial_passed: probes.len(), adversarial_total: probes.len() })
            }
        }
        assert!(matches!(
            run_adversarial_critic(&NotSandboxed, "rust", "prompt", "fn f(x: u32) -> u32", "fn f(x:u32)->u32{x}", "test"),
            Err(CriticError::Rejected)
        ));
    }
}
