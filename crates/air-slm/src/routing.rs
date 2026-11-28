//! Deterministic low-cost routing and explicit escalation triggers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FuzzyRoute { RustOwnership, RustLifetime, TraitGeneric, AsyncConcurrency, ErrorHandling, MacroIterator, Unknown }
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EscalationReason { ExplicitTrigger, LowConfidence }
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RouteDecision { NeuroSymbolicOnly, EscalateToSlm { reason: EscalationReason } }

/// Leading "deep" word or "!" requests always escalate; whitespace is ignored.
pub fn has_explicit_trigger(prompt: &str) -> bool {
    let trimmed = prompt.trim_start();
    trimmed.starts_with('!') || trimmed.split_whitespace().next().is_some_and(|w| w.eq_ignore_ascii_case("deep"))
}
pub fn route_prompt(prompt: &str, confidence: f32, threshold: f32) -> RouteDecision {
    if has_explicit_trigger(prompt) {
        return RouteDecision::EscalateToSlm { reason: EscalationReason::ExplicitTrigger };
    }
    if !confidence.is_finite() || !threshold.is_finite() || !(0.0..=1.0).contains(&confidence) || !(0.0..=1.0).contains(&threshold) || confidence < threshold {
        RouteDecision::EscalateToSlm { reason: EscalationReason::LowConfidence }
    } else { RouteDecision::NeuroSymbolicOnly }
}
/// Six-family fuzzy classifier used by the coding gauntlet.
pub fn route_fuzzy(input: &str) -> FuzzyRoute {
    let s = input.to_ascii_lowercase();
    let families: [(FuzzyRoute, &[&str]); 6] = [
        (FuzzyRoute::RustOwnership, &["ownership", "borrow checker", "borrowing", "borrow", "moved value", "move semantics", "mutable borrow", "without moving"]),
        (FuzzyRoute::RustLifetime, &["lifetime", "lifetimes", "outlives", "dangling reference", "elided lifetime", "borrowed value", "return a borrowed"]),
        (FuzzyRoute::TraitGeneric, &["trait", "generic", "generics", "type parameter", "trait bound", "impl trait", "where clause"]),
        (FuzzyRoute::AsyncConcurrency, &["async", "await", "concurrency", "concurrent", "thread", "mutex", "race condition", "spawn"]),
        (FuzzyRoute::ErrorHandling, &["error handling", "error propagation", "error", "result", "option", "anyhow", "thiserror", "parse error", "?"]),
        (FuzzyRoute::MacroIterator, &["macro", "macro_rules", "iterator", "iterators", "into_iter", "from_fn", "fused iterator", "edge cases", "collect"]),
    ];
    let mut best = (FuzzyRoute::Unknown, 0usize, 0usize);
    for (family, terms) in families {
        let mut score = 0;
        let mut hits = 0;
        for term in terms {
            if s.contains(term) { score += if term.contains(' ') || term.len() > 9 { 3 } else { 1 }; hits += 1; }
        }
        if score > best.1 || (score == best.1 && hits > best.2) { best = (family, score, hits); }
        else if score > 0 && score == best.1 && hits == best.2 { return FuzzyRoute::Unknown; }
    }
    if best.1 == 0 { FuzzyRoute::Unknown } else { best.0 }
}
#[cfg(test)]
mod tests {
 use super::*;
 #[test] fn triggers_and_confidence() {
  assert_eq!(route_prompt(" deep please",1.0,0.3),RouteDecision::EscalateToSlm{reason:EscalationReason::ExplicitTrigger});
  assert_eq!(route_prompt("!please",1.0,0.3),RouteDecision::EscalateToSlm{reason:EscalationReason::ExplicitTrigger});
  assert_eq!(route_prompt("q",0.2,0.3),RouteDecision::EscalateToSlm{reason:EscalationReason::LowConfidence});
  assert_eq!(route_prompt("q",0.8,0.3),RouteDecision::NeuroSymbolicOnly);
  assert_eq!(route_fuzzy("borrow a Rust String without moving it"),FuzzyRoute::RustOwnership);
  assert_eq!(route_fuzzy("add a lifetime to return a borrowed value"),FuzzyRoute::RustLifetime);
  assert_eq!(route_fuzzy("implement a generic trait for a type"),FuzzyRoute::TraitGeneric);
  assert_eq!(route_fuzzy("write an async function and spawn concurrent tasks"),FuzzyRoute::AsyncConcurrency);
  assert_eq!(route_fuzzy("propagate a parse error with Result and ?"),FuzzyRoute::ErrorHandling);
  assert_eq!(route_fuzzy("write a macro and handle iterator edge cases"),FuzzyRoute::MacroIterator);
 }
}
