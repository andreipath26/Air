//! Confidence and ambiguity gate for prototype routing.
//!
//! The gate consumes the ternary router's integer scores. It does not generate
//! code: when the two best routes are too close, it emits a deterministic
//! multiple-choice clarification instead of choosing blindly.
use crate::ternary_snn::RouteScore;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct AmbiguityThresholds {
    /// The winner must reach at least this raw ternary score.
    pub minimum_top_score: i32,
    /// Minimum absolute gap between the top two raw scores.
    pub minimum_margin: i64,
    /// Maximum normalized binary entropy for the top-two distribution.
    /// Values range from 0 (certain) through 1 (an exact tie).
    pub maximum_top_two_entropy: f64,
    /// Positive temperature controlling how score gaps map to probabilities.
    pub temperature: f64,
}

impl Default for AmbiguityThresholds {
    fn default() -> Self {
        Self {
            minimum_top_score: 1,
            minimum_margin: 8,
            maximum_top_two_entropy: 0.65,
            temperature: 8.0,
        }
    }
}

impl AmbiguityThresholds {
    fn validate(self) -> Result<Self, AmbiguityError> {
        if self.minimum_margin < 0
            || !self.maximum_top_two_entropy.is_finite()
            || !(0.0..=1.0).contains(&self.maximum_top_two_entropy)
            || !self.temperature.is_finite()
            || self.temperature <= 0.0
        {
            return Err(AmbiguityError::InvalidThresholds);
        }
        Ok(self)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RoutingAction {
    /// Use the unambiguous AIR route.
    Select { output: usize },
    /// Ask the user to choose between the leading interpretations.
    Clarify { questions: Vec<String> },
    /// An explicit leading "deep" or "!" request bypasses clarification.
    EscalateExplicit,
}

#[derive(Clone, Debug, PartialEq)]
pub struct AmbiguityAssessment {
    pub action: RoutingAction,
    pub top_output: Option<usize>,
    pub top_score: Option<i32>,
    pub margin: i64,
    pub normalized_top_two_entropy: f64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AmbiguityError {
    InvalidThresholds,
}

/// Assess a scored route set using the top-two score margin and normalized
/// entropy. `labels` maps output IDs to human-readable route names.
///
/// An explicit `deep` word or `!` prefix is returned as escalation before any
/// clarification logic. Scores are sorted deterministically by score descending
/// and then output ID ascending.
pub fn assess_route(
    prompt: &str,
    scores: &[RouteScore],
    labels: &[&str],
    thresholds: AmbiguityThresholds,
) -> Result<AmbiguityAssessment, AmbiguityError> {
    let thresholds = thresholds.validate()?;
    if has_explicit_trigger(prompt) {
        return Ok(AmbiguityAssessment {
            action: RoutingAction::EscalateExplicit,
            top_output: None,
            top_score: None,
            margin: 0,
            normalized_top_two_entropy: 0.0,
        });
    }

    let mut ranked = scores.to_vec();
    ranked.sort_by(|a, b| b.score.cmp(&a.score).then_with(|| a.output.cmp(&b.output)));
    let Some(top) = ranked.first().copied() else {
        return Ok(AmbiguityAssessment {
            action: RoutingAction::Clarify {
                questions: vec![generic_question()],
            },
            top_output: None,
            top_score: None,
            margin: 0,
            normalized_top_two_entropy: 0.0,
        });
    };
    let Some(second) = ranked.get(1).copied() else {
        return Ok(AmbiguityAssessment {
            action: RoutingAction::Clarify {
                questions: vec![generic_question()],
            },
            top_output: Some(top.output),
            top_score: Some(top.score),
            margin: 0,
            normalized_top_two_entropy: 0.0,
        });
    };

    let margin = i64::from(top.score) - i64::from(second.score);
    let entropy = normalized_pair_entropy(top.score, second.score, thresholds.temperature);
    let ambiguous = top.score < thresholds.minimum_top_score
        || margin < thresholds.minimum_margin
        || entropy > thresholds.maximum_top_two_entropy;
    let action = if ambiguous {
        RoutingAction::Clarify {
            questions: vec![route_question(
                label_for(labels, top.output),
                label_for(labels, second.output),
            )],
        }
    } else {
        RoutingAction::Select { output: top.output }
    };

    Ok(AmbiguityAssessment {
        action,
        top_output: Some(top.output),
        top_score: Some(top.score),
        margin,
        normalized_top_two_entropy: entropy,
    })
}

/// Normalized Shannon entropy of the two highest scores under a stable
/// temperature-scaled softmax. Exact ties yield 1; a dominant top score tends
/// to 0. Only two scores are processed, so this stays bounded and inexpensive.
pub fn normalized_pair_entropy(top_score: i32, second_score: i32, temperature: f64) -> f64 {
    if !temperature.is_finite() || temperature <= 0.0 {
        return f64::NAN;
    }
    let gap = (i64::from(top_score) - i64::from(second_score)).max(0) as f64;
    let second_weight = (-gap / temperature).exp();
    let denominator = 1.0 + second_weight;
    let p_top = 1.0 / denominator;
    let p_second = second_weight / denominator;
    let entropy = -(p_top * p_top.ln()
        + if p_second == 0.0 { 0.0 } else { p_second * p_second.ln() });
    (entropy / std::f64::consts::LN_2).clamp(0.0, 1.0)
}

/// The explicit escalation convention matches air-slm: only a leading
/// `deep` token or `!` is special; embedded occurrences are ordinary text.
pub fn has_explicit_trigger(prompt: &str) -> bool {
    let trimmed = prompt.trim_start();
    trimmed.starts_with('!')
        || trimmed
            .split_whitespace()
            .next()
            .is_some_and(|word| word.eq_ignore_ascii_case("deep"))
}

fn label_for(labels: &[&str], output: usize) -> String {
    let raw = labels.get(output).copied().unwrap_or_default();
    let cleaned: String = raw
        .chars()
        .filter(|ch| *ch != '\n' && *ch != '\r' && !ch.is_control())
        .take(80)
        .collect();
    if cleaned.trim().is_empty() {
        format!("route {output}")
    } else {
        cleaned
    }
}

fn route_question(first: String, second: String) -> String {
    format!(
        "Which interpretation should I use? A) {first}; B) {second}."
    )
}

fn generic_question() -> String {
    "What should I do? A) produce a code change; B) explain the issue first.".to_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn score(output: usize, score: i32) -> RouteScore {
        RouteScore {
            output,
            score,
            fired: score > 0,
        }
    }

    fn thresholds() -> AmbiguityThresholds {
        AmbiguityThresholds {
            minimum_top_score: 10,
            minimum_margin: 20,
            maximum_top_two_entropy: 0.7,
            temperature: 10.0,
        }
    }

    #[test]
    fn equal_top_scores_have_maximum_entropy_and_zero_margin() {
        assert_eq!(normalized_pair_entropy(50, 50, 10.0), 1.0);
        let assessment = assess_route(
            "fix this",
            &[score(2, 50), score(1, 50)],
            &["ownership", "lifetimes", "traits"],
            thresholds(),
        )
        .unwrap();
        assert_eq!(assessment.margin, 0);
        assert_eq!(assessment.normalized_top_two_entropy, 1.0);
        assert_eq!(
            assessment.action,
            RoutingAction::Clarify {
                questions: vec!["Which interpretation should I use? A) lifetimes; B) traits.".into()]
            }
        );
    }

    #[test]
    fn a_large_score_gap_has_low_entropy() {
        assert!(normalized_pair_entropy(100, 0, 10.0) < 0.01);
        let assessment = assess_route(
            "repair ownership",
            &[score(0, 100), score(1, 0)],
            &["ownership", "async"],
            thresholds(),
        )
        .unwrap();
        assert_eq!(assessment.margin, 100);
        assert!(assessment.normalized_top_two_entropy < 0.01);
        assert_eq!(assessment.action, RoutingAction::Select { output: 0 });
    }

    #[test]
    fn low_margin_and_low_confidence_trigger_clarification() {
        let result = assess_route(
            "fix it",
            &[score(0, 15), score(1, 10)],
            &["borrow", "lifetime"],
            thresholds(),
        )
        .unwrap();
        assert!(matches!(result.action, RoutingAction::Clarify { .. }));

        let result = assess_route(
            "fix it",
            &[score(0, 5), score(1, -40)],
            &["borrow", "lifetime"],
            thresholds(),
        )
        .unwrap();
        assert!(matches!(result.action, RoutingAction::Clarify { .. }));
    }

    #[test]
    fn explicit_deep_and_bang_escalation_bypass_clarification() {
        for prompt in ["deep fix this", " DEEP fix this", "!fix this", "  ! fix this"] {
            let result = assess_route(
                prompt,
                &[score(0, 50), score(1, 50)],
                &["borrow", "lifetime"],
                thresholds(),
            )
            .unwrap();
            assert_eq!(result.action, RoutingAction::EscalateExplicit);
        }
        assert!(!has_explicit_trigger("please go deep"));
        assert!(!has_explicit_trigger("exclamation ! embedded"));
    }

    #[test]
    fn clarification_is_deterministic_targeted_and_multiple_choice() {
        let scores = [score(0, 100), score(1, 99)];
        let a = assess_route(
            "repair this Rust code",
            &scores,
            &["ownership and borrowing", "async concurrency"],
            thresholds(),
        )
        .unwrap();
        let b = assess_route(
            "repair this Rust code",
            &scores,
            &["ownership and borrowing", "async concurrency"],
            thresholds(),
        )
        .unwrap();
        assert_eq!(a, b);
        let RoutingAction::Clarify { questions } = a.action else {
            panic!("the close scores should require clarification");
        };
        assert_eq!(questions.len(), 1);
        assert!(questions[0].contains("A) ownership and borrowing"));
        assert!(questions[0].contains("B) async concurrency"));
    }

    #[test]
    fn empty_or_single_candidate_asks_a_bounded_generic_question() {
        for scores in [vec![], vec![score(4, 100)]] {
            let result = assess_route("do something", &scores, &[], thresholds()).unwrap();
            let RoutingAction::Clarify { questions } = result.action else {
                panic!("insufficient alternatives must not silently select");
            };
            assert_eq!(questions.len(), 1);
            assert!(questions[0].contains("A)"));
            assert!(questions[0].contains("B)"));
        }
    }

    #[test]
    fn invalid_thresholds_are_rejected() {
        let invalid = AmbiguityThresholds {
            maximum_top_two_entropy: f64::NAN,
            ..thresholds()
        };
        assert_eq!(
            assess_route("x", &[score(0, 1), score(1, 0)], &[], invalid),
            Err(AmbiguityError::InvalidThresholds)
        );
    }
}
