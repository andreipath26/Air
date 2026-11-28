//! Bounded seam between typed coding intent, AIRW candidate artifacts, and an
//! external verification oracle. This module does not parse natural language,
//! generate patches, invoke compilers, or implement language-specific rules.

use air_weights::ArchiveReader;
use std::collections::BTreeSet;

pub const MAX_RULES: usize = 4096;
pub const MAX_CONCEPTS: usize = 128;
pub const MAX_CANDIDATES: usize = 128;
const MAX_ID_BYTES: usize = 128;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ConceptKind {
    Intent,
    Operation,
    DataStructure,
    Diagnostic,
    Constraint,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct TypedConcept {
    pub kind: ConceptKind,
    pub value: String,
}
impl TypedConcept {
    pub fn new(kind: ConceptKind, value: impl Into<String>) -> Result<Self, CodingError> {
        let value = value.into();
        if !valid_token(&value) { return Err(CodingError::InvalidInput("concept value")); }
        Ok(Self { kind, value })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CodingRequest {
    pub language: String,
    concepts: BTreeSet<TypedConcept>,
}
impl CodingRequest {
    pub fn new(language: impl Into<String>, concepts: Vec<TypedConcept>) -> Result<Self, CodingError> {
        let language = language.into();
        if !valid_token(&language) { return Err(CodingError::InvalidInput("language")); }
        if concepts.len() > MAX_CONCEPTS { return Err(CodingError::Limit("concept count")); }
        Ok(Self { language, concepts: concepts.into_iter().collect() })
    }
    pub fn concepts(&self) -> impl Iterator<Item = &TypedConcept> {
        self.concepts.iter()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReductionRule {
    pub id: String,
    pub language: String,
    pub required: BTreeSet<TypedConcept>,
    pub priority: i32,
    pub artifact_section: String,
}
impl ReductionRule {
    pub fn new(
        id: impl Into<String>,
        language: impl Into<String>,
        required: Vec<TypedConcept>,
        priority: i32,
        artifact_section: impl Into<String>,
    ) -> Result<Self, CodingError> {
        let id = id.into();
        let language = language.into();
        let artifact_section = artifact_section.into();
        if !valid_token(&id) || !valid_token(&language) || !valid_token(&artifact_section) {
            return Err(CodingError::InvalidInput("rule identifier"));
        }
        if required.len() > MAX_CONCEPTS { return Err(CodingError::Limit("rule concept count")); }
        Ok(Self {
            id,
            language,
            required: required.into_iter().collect(),
            priority,
            artifact_section,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RankedCandidate {
    pub id: String,
    pub score: i32,
    pub artifact_section: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OracleDecision {
    Accept,
    Reject,
    Abstain,
}

/// Implement this with a real caller-owned verifier, such as a compiler/test
/// harness. AIR core does not claim to run compilers or inspect ASTs here.
pub trait VerificationOracle {
    fn verify(
        &self,
        request: &CodingRequest,
        candidate: &RankedCandidate,
        artifact: &[u8],
    ) -> OracleDecision;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AbstainReason {
    NoOracle,
    LanguageUnavailable,
    NoMatchingCandidate,
    ArtifactUnavailable,
    OracleWithheld,
    CandidatesRejected,
}

#[derive(Debug, PartialEq, Eq)]
pub enum Resolution<'a> {
    Verified {
        candidate: RankedCandidate,
        artifact: &'a [u8],
        considered: usize,
    },
    Abstained {
        reason: AbstainReason,
        considered: usize,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CodingError {
    InvalidInput(&'static str),
    Limit(&'static str),
    DuplicateRule,
}
impl std::fmt::Display for CodingError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidInput(s) => write!(f, "invalid coding input: {s}"),
            Self::Limit(s) => write!(f, "coding input limit exceeded: {s}"),
            Self::DuplicateRule => f.write_str("duplicate reduction rule id"),
        }
    }
}
impl std::error::Error for CodingError {}

fn valid_token(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= MAX_ID_BYTES
        && value.bytes().all(|b| {
            b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.' | b':')
        })
}

#[derive(Debug, Clone, Default)]
pub struct CodingReducer {
    rules: Vec<ReductionRule>,
}
impl CodingReducer {
    pub fn new(rules: Vec<ReductionRule>) -> Result<Self, CodingError> {
        if rules.len() > MAX_RULES { return Err(CodingError::Limit("rule count")); }
        let mut ids = BTreeSet::new();
        for rule in &rules {
            if !ids.insert(rule.id.as_str()) { return Err(CodingError::DuplicateRule); }
        }
        Ok(Self { rules })
    }

    /// Returns deterministic top candidates for exact typed-concept coverage.
    /// Scores are integer priorities plus the number of matched required concepts.
    pub fn rank(&self, request: &CodingRequest, limit: usize) -> Vec<RankedCandidate> {
        let limit = limit.min(MAX_CANDIDATES);
        if limit == 0 { return Vec::new(); }
        let mut ranked: Vec<_> = self.rules.iter()
            .filter(|rule| {
                rule.language == request.language
                    && rule.required.iter().all(|concept| request.concepts.contains(concept))
            })
            .map(|rule| RankedCandidate {
                id: rule.id.clone(),
                score: rule.priority.saturating_add(rule.required.len() as i32),
                artifact_section: rule.artifact_section.clone(),
            })
            .collect();
        ranked.sort_by(|a, b| b.score.cmp(&a.score).then_with(|| a.id.cmp(&b.id)));
        ranked.truncate(limit);
        ranked
    }

    /// Evaluates candidates in rank order. Missing oracle, unavailable language,
    /// missing artifacts, or oracle uncertainty produce explicit abstention.
    pub fn resolve<'a>(
        &self,
        request: &CodingRequest,
        archive: &'a ArchiveReader,
        oracle: Option<&dyn VerificationOracle>,
        limit: usize,
    ) -> Resolution<'a> {
        let Some(oracle) = oracle else {
            return Resolution::Abstained { reason: AbstainReason::NoOracle, considered: 0 };
        };
        if !archive.manifest().languages.iter().any(|lang| lang == &request.language) {
            return Resolution::Abstained { reason: AbstainReason::LanguageUnavailable, considered: 0 };
        }
        let candidates = self.rank(request, limit);
        if candidates.is_empty() {
            return Resolution::Abstained { reason: AbstainReason::NoMatchingCandidate, considered: 0 };
        }
        let mut considered = 0;
        let mut missing = 0;
        for candidate in candidates {
            let Some(artifact) = archive.section(&candidate.artifact_section) else {
                missing += 1;
                continue;
            };
            considered += 1;
            match oracle.verify(request, &candidate, artifact) {
                OracleDecision::Accept => {
                    return Resolution::Verified { candidate, artifact, considered };
                }
                OracleDecision::Reject => {}
                OracleDecision::Abstain => {
                    return Resolution::Abstained { reason: AbstainReason::OracleWithheld, considered };
                }
            }
        }
        let reason = if considered == 0 && missing > 0 {
            AbstainReason::ArtifactUnavailable
        } else {
            AbstainReason::CandidatesRejected
        };
        Resolution::Abstained { reason, considered }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use air_weights::{ArchiveBuilder, ArchiveReader};

    struct MatchArtifact(&'static [u8]);
    impl VerificationOracle for MatchArtifact {
        fn verify(&self, _: &CodingRequest, _: &RankedCandidate, artifact: &[u8]) -> OracleDecision {
            if artifact == self.0 { OracleDecision::Accept } else { OracleDecision::Reject }
        }
    }
    struct Withhold;
    impl VerificationOracle for Withhold {
        fn verify(&self, _: &CodingRequest, _: &RankedCandidate, _: &[u8]) -> OracleDecision {
            OracleDecision::Abstain
        }
    }

    fn concept(kind: ConceptKind, value: &str) -> TypedConcept {
        TypedConcept::new(kind, value).unwrap()
    }
    fn archive() -> ArchiveReader {
        let mut builder = ArchiveBuilder::new("coding-test");
        builder.add_language("rust").unwrap();
        builder.add_language("python").unwrap();
        builder.add_section("candidate.high", "rule", b"high").unwrap();
        builder.add_section("candidate.low", "rule", b"low").unwrap();
        builder.finish().and_then(ArchiveReader::from_bytes).unwrap()
    }
    fn request() -> CodingRequest {
        CodingRequest::new("rust", vec![
            concept(ConceptKind::Diagnostic, "err:e0277"),
            concept(ConceptKind::Intent, "repair"),
        ]).unwrap()
    }
    fn reducer() -> CodingReducer {
        CodingReducer::new(vec![
            ReductionRule::new("high", "rust",
                vec![concept(ConceptKind::Diagnostic, "err:e0277")], 10, "candidate.high").unwrap(),
            ReductionRule::new("low", "rust",
                vec![concept(ConceptKind::Diagnostic, "err:e0277"),
                     concept(ConceptKind::Intent, "repair")], 0, "candidate.low").unwrap(),
            ReductionRule::new("wrong-language", "python", vec![], 100, "candidate.high").unwrap(),
        ]).unwrap()
    }

    #[test]
    fn ranks_by_integer_priority_and_specificity_with_stable_ties() {
        let ranked = reducer().rank(&request(), 10);
        assert_eq!(ranked.iter().map(|c| c.id.as_str()).collect::<Vec<_>>(), vec!["high", "low"]);
        assert!(ranked[0].score > ranked[1].score);
    }
    #[test]
    fn caller_oracle_rejects_top_candidate_then_accepts_next_borrowed_section() {
        let r = reducer();
        let a = archive();
        let result = r.resolve(&request(), &a, Some(&MatchArtifact(b"low")), 8);
        match result {
            Resolution::Verified { candidate, artifact, considered } => {
                assert_eq!(candidate.id, "low");
                assert_eq!(artifact, b"low");
                assert_eq!(considered, 2);
            }
            other => panic!("unexpected result: {other:?}"),
        }
    }
    #[test]
    fn missing_oracle_abstains_by_default() {
        let result = reducer().resolve(&request(), &archive(), None, 8);
        assert_eq!(result, Resolution::Abstained {
            reason: AbstainReason::NoOracle, considered: 0
        });
    }
    #[test]
    fn oracle_uncertainty_abstains_without_falling_through() {
        let result = reducer().resolve(&request(), &archive(), Some(&Withhold), 8);
        assert_eq!(result, Resolution::Abstained {
            reason: AbstainReason::OracleWithheld, considered: 1
        });
    }
    #[test]
    fn unsupported_language_and_unmatched_intent_abstain() {
        let r = reducer();
        let a = archive();
        let python = CodingRequest::new("go", vec![]).unwrap();
        assert_eq!(r.resolve(&python, &a, Some(&Withhold), 8), Resolution::Abstained {
            reason: AbstainReason::LanguageUnavailable, considered: 0
        });
        let missing = CodingRequest::new("rust", vec![concept(ConceptKind::Operation, "new")]).unwrap();
        assert_eq!(r.resolve(&missing, &a, Some(&Withhold), 8), Resolution::Abstained {
            reason: AbstainReason::NoMatchingCandidate, considered: 0
        });
    }
    #[test]
    fn missing_section_and_rejected_candidates_abstain() {
        let missing = CodingReducer::new(vec![
            ReductionRule::new("dangling", "rust", vec![], 1, "absent.section").unwrap()
        ]).unwrap();
        assert_eq!(missing.resolve(&request(), &archive(), Some(&Withhold), 1), Resolution::Abstained {
            reason: AbstainReason::ArtifactUnavailable, considered: 0
        });
        assert_eq!(reducer().resolve(&request(), &archive(), Some(&MatchArtifact(b"never")), 8),
            Resolution::Abstained { reason: AbstainReason::CandidatesRejected, considered: 2 });
    }
}
