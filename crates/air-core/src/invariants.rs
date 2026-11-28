//! Conservative, dependency-light source checks for candidate Rust patches.
//!
//! These are static heuristics, not formal verification. The report can identify
//! common panic and overflow hazards, but cannot prove memory safety, semantic
//! correctness, liveness, or absence of all panics. Compiler and test evidence
//! must still be produced by the caller in an enforced sandbox.
use sha2::{Digest, Sha256};
use tree_sitter::{Node, Parser};
use std::fmt;

pub const MAX_CANDIDATE_BYTES: usize = 256 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CandidateVerification {
    pub compiler_passed: bool,
    pub unit_tests_passed: bool,
    /// Caller attests that compiler and tests ran inside the configured sandbox.
    /// This value is not independently verified by this static checker.
    pub sandbox_enforced: bool,
}
impl CandidateVerification {
    pub fn complete(self) -> bool {
        self.compiler_passed && self.unit_tests_passed && self.sandbox_enforced
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum FindingKind {
    SyntaxError,
    UncheckedUnwrap,
    UncheckedExpect,
    PanicMacro,
    UncheckedArithmetic,
    DynamicBorrowRisk,
    GuardAcrossAwaitRisk,
    UnsafeBlock,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InvariantFinding {
    pub kind: FindingKind,
    pub start_byte: usize,
    pub end_byte: usize,
    pub explanation: &'static str,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InvariantReport {
    candidate_sha256: String,
    findings: Vec<InvariantFinding>,
}
impl InvariantReport {
    pub fn candidate_sha256(&self) -> &str { &self.candidate_sha256 }
    pub fn findings(&self) -> &[InvariantFinding] { &self.findings }
    pub fn is_clear(&self) -> bool { self.findings.is_empty() }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PromotionProof {
    candidate_sha256: String,
    invariant_report: InvariantReport,
    verification: CandidateVerification,
}
impl PromotionProof {
    pub fn candidate_sha256(&self) -> &str { &self.candidate_sha256 }
    pub fn invariant_report(&self) -> &InvariantReport { &self.invariant_report }
    pub fn verification(&self) -> CandidateVerification { self.verification }
}

/// Adapter boundary for archive or rule stores. Implementations must accept
/// only the proof returned by gate_candidate, bind it to the exact candidate
/// bytes, and perform their own atomic write/rollback.
pub trait InvariantGatedPromotion {
    type Error: fmt::Display;
    fn promote(
        &self,
        language: &str,
        prompt: &str,
        candidate: &str,
        proof: &PromotionProof,
    ) -> Result<usize, Self::Error>;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GateError {
    CandidateTooLarge,
    ParserSetup(String),
    ParseCancelled,
    VerificationIncomplete,
    InvariantFindings(InvariantReport),
}
impl fmt::Display for GateError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::CandidateTooLarge => write!(f, "candidate exceeds {MAX_CANDIDATE_BYTES} bytes"),
            Self::ParserSetup(error) => write!(f, "Rust parser setup failed: {error}"),
            Self::ParseCancelled => f.write_str("Rust parser cancelled"),
            Self::VerificationIncomplete => f.write_str("sandboxed compiler and unit-test evidence is required"),
            Self::InvariantFindings(report) => write!(f, "candidate has {} blocking static invariant finding(s)", report.findings().len()),
        }
    }
}
impl std::error::Error for GateError {}

/// Parse a Rust candidate and report conservative high-risk constructs.
///
/// Raw arithmetic operators are blocked by default because tree-sitter cannot
/// infer integer types or prove guarding conditions. The caller should replace
/// them with checked/saturating operations or route the candidate for review.
pub fn inspect_rust_candidate(source: &str) -> Result<InvariantReport, GateError> {
    if source.len() > MAX_CANDIDATE_BYTES { return Err(GateError::CandidateTooLarge); }
    let mut parser = Parser::new();
    let language = tree_sitter_rust::LANGUAGE;
    parser.set_language(&language.into())
        .map_err(|error| GateError::ParserSetup(error.to_string()))?;
    let tree = parser.parse(source.as_bytes(), None).ok_or(GateError::ParseCancelled)?;
    let mut findings = Vec::new();
    walk(tree.root_node(), source.as_bytes(), &mut findings);
    if source.contains("MutexGuard") && source.contains(".await") {
        findings.push(InvariantFinding {
            kind: FindingKind::GuardAcrossAwaitRisk,
            start_byte: 0,
            end_byte: source.len(),
            explanation: "MutexGuard and await coexist; static scan cannot prove the guard is dropped before suspension",
        });
    }
    findings.sort_by_key(|finding| (finding.start_byte, finding.end_byte, finding.kind));
    findings.dedup();
    let digest = Sha256::digest(source.as_bytes());
    Ok(InvariantReport {
        candidate_sha256: format!("{digest:x}"),
        findings,
    })
}

/// Return promotion evidence only if compiler, unit tests, sandbox attestation,
/// and the static invariant scan all pass for these exact candidate bytes.
pub fn gate_candidate(
    source: &str,
    verification: CandidateVerification,
) -> Result<PromotionProof, GateError> {
    let report = inspect_rust_candidate(source)?;
    if !verification.complete() { return Err(GateError::VerificationIncomplete); }
    if !report.is_clear() { return Err(GateError::InvariantFindings(report)); }
    Ok(PromotionProof {
        candidate_sha256: report.candidate_sha256().to_owned(),
        invariant_report: report,
        verification,
    })
}

fn node_text<'a>(node: Node<'_>, source: &'a [u8]) -> &'a str {
    source.get(node.byte_range())
        .and_then(|bytes| std::str::from_utf8(bytes).ok())
        .unwrap_or("")
}

fn add_finding(
    findings: &mut Vec<InvariantFinding>,
    kind: FindingKind,
    node: Node<'_>,
    explanation: &'static str,
) {
    findings.push(InvariantFinding {
        kind,
        start_byte: node.start_byte(),
        end_byte: node.end_byte(),
        explanation,
    });
}

fn walk(node: Node<'_>, source: &[u8], findings: &mut Vec<InvariantFinding>) {
    if node.is_error() || node.is_missing() {
        add_finding(findings, FindingKind::SyntaxError, node, "Rust syntax tree contains an error or missing token");
    }
    match node.kind() {
        "call_expression" => {
            if let Some(function) = node.child_by_field_name("function") {
                let name = node_text(function, source).trim();
                let method = name.rsplit('.').next().unwrap_or(name);
                if method == "unwrap" && name.contains('.') {
                    add_finding(findings, FindingKind::UncheckedUnwrap, node, "unchecked unwrap may panic");
                } else if method == "expect" && name.contains('.') {
                    add_finding(findings, FindingKind::UncheckedExpect, node, "expect may panic on an unhandled error");
                }
                if (method == "borrow" || method == "borrow_mut") && name.contains(".") {
                    add_finding(findings, FindingKind::DynamicBorrowRisk, node, "RefCell dynamic borrow can panic when borrow rules are violated");
                }
            }
        }
        "macro_invocation" => {
            let text = node_text(node, source);
            let macro_name = text.split('!').next().unwrap_or("").trim();
            let macro_tail = macro_name.rsplit("::").next().unwrap_or(macro_name);
            if matches!(macro_tail, "panic" | "todo" | "unimplemented" | "unreachable" | "assert" | "assert_eq" | "assert_ne") {
                add_finding(findings, FindingKind::PanicMacro, node, "macro can panic or terminate execution");
            }
        }
        "binary_expression" | "compound_assignment_expr" => {
            let mut operator = node.child_by_field_name("operator").map(|child| node_text(child, source)).unwrap_or("");
            if operator.is_empty() {
                for index in 0..node.child_count() {
                    if let Some(child) = node.child(index) {
                        let token = node_text(child, source).trim();
                        if matches!(token, "+" | "-" | "*" | "/" | "%" | "<<" | ">>" | "+=" | "-=" | "*=" | "/=" | "%=" | "<<=" | ">>=") {
                            operator = token;
                            break;
                        }
                    }
                }
            }
            if matches!(operator.trim(), "+" | "-" | "*" | "/" | "%" | "<<" | ">>" | "+=" | "-=" | "*=" | "/=" | "%=" | "<<=" | ">>=") {
                add_finding(findings, FindingKind::UncheckedArithmetic, node, "raw arithmetic may overflow, divide by zero, or shift out of range");
            }
        }
        "unsafe_block" | "unsafe_expression" => {
            add_finding(findings, FindingKind::UnsafeBlock, node, "unsafe code requires a separate human-reviewed safety contract");
        }
        _ => {}
    }
    for index in 0..node.child_count() {
        if let Some(child) = node.child(index) {
            walk(child, source, findings);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_unwrap_expect_and_panic_paths() {
        let source = r#"
            fn brittle(value: Option<u32>) -> u32 {
                let x = value.unwrap();
                let y = Some(x).expect("present");
                if y == 0 { panic!("zero"); }
                todo!()
            }
        "#;
        let report = inspect_rust_candidate(source).unwrap();
        let kinds: Vec<_> = report.findings().iter().map(|finding| finding.kind).collect();
        assert!(kinds.contains(&FindingKind::UncheckedUnwrap));
        assert!(kinds.contains(&FindingKind::UncheckedExpect));
        assert!(kinds.iter().filter(|kind| **kind == FindingKind::PanicMacro).count() >= 2);
    }

    #[test]
    fn detects_raw_integer_boundaries_and_unsafe_sections() {
        let source = "pub fn add(a: u32, b: u32) -> u32 { unsafe { a + b } }";
        let report = inspect_rust_candidate(source).unwrap();
        assert!(report.findings().iter().any(|finding| finding.kind == FindingKind::UncheckedArithmetic));
        assert!(report.findings().iter().any(|finding| finding.kind == FindingKind::UnsafeBlock));
    }

    #[test]
    fn safe_checked_operations_pass_static_scan() {
        let source = r#"
            pub fn add(a: u32, b: u32) -> Option<u32> {
                a.checked_add(b)
            }
            pub fn read(value: Option<u32>) -> Result<u32, &'static str> {
                value.ok_or("missing")
            }
        "#;
        assert!(inspect_rust_candidate(source).unwrap().is_clear());
    }

    #[test]
    fn detects_dynamic_borrow_and_guard_await_risks() {
        let source = r#"
            use std::cell::RefCell;
            use std::sync::MutexGuard;
            async fn update(cell: &RefCell<u32>, guard: MutexGuard<'_, u32>) {
                let _value = cell.borrow_mut();
                let _still_locked = guard;
                std::future::ready(()).await;
            }
        "#;
        let report = inspect_rust_candidate(source).unwrap();
        assert!(report.findings().iter().any(|finding| finding.kind == FindingKind::DynamicBorrowRisk));
        assert!(report.findings().iter().any(|finding| finding.kind == FindingKind::GuardAcrossAwaitRisk));
    }

    #[test]
    fn promotion_requires_both_tests_and_sandboxed_verification() {
        let safe = "pub fn add(a: u32, b: u32) -> Option<u32> { a.checked_add(b) }";
        let good = CandidateVerification { compiler_passed: true, unit_tests_passed: true, sandbox_enforced: true };
        let proof = gate_candidate(safe, good).unwrap();
        assert_eq!(proof.candidate_sha256(), inspect_rust_candidate(safe).unwrap().candidate_sha256());
        assert!(gate_candidate(safe, CandidateVerification { sandbox_enforced: false, ..good }).is_err());
        assert!(gate_candidate(safe, CandidateVerification { unit_tests_passed: false, ..good }).is_err());
        assert!(matches!(gate_candidate("fn bad() { todo!() }", good), Err(GateError::InvariantFindings(_))));
    }

    #[test]
    fn reports_malformed_source_and_bounds() {
        let report = inspect_rust_candidate("fn incomplete( {").unwrap();
        assert!(report.findings().iter().any(|finding| finding.kind == FindingKind::SyntaxError));
        assert!(matches!(inspect_rust_candidate(&" ".repeat(MAX_CANDIDATE_BYTES + 1)), Err(GateError::CandidateTooLarge)));
    }
}
