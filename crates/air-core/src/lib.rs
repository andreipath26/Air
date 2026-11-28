//! Research prototype; no general intelligence implementation yet.
pub mod store;
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Representation { Fact, Concept, Procedure, Pattern, StateMachine, Neural }
#[derive(Debug, Clone, Copy)]
pub struct Budget { pub max_bytes: u64, pub max_steps: u64 }
impl Budget { pub fn permits(&self, bytes: u64, steps: u64) -> bool { bytes <= self.max_bytes && steps <= self.max_steps } }
#[cfg(test)]
mod tests { use super::*; #[test] fn budget_boundaries() { let b = Budget { max_bytes: 1024, max_steps: 10 }; assert!(b.permits(1024,10)); assert!(!b.permits(1025,10)); assert!(!b.permits(1024,11)); } }

pub mod indexed;
pub mod router;
pub mod curate;

pub mod model;
pub mod reasoning;

pub mod cognition;
pub mod skills;
pub mod associative;
pub mod evidence;
pub mod language;

pub mod policy;
pub mod engine;
pub mod synthesis;
pub mod deduction;

pub mod consolidation;
pub mod causal;
pub mod probability;
pub mod packages;

pub mod plan;
pub mod temporal;
pub mod compact;
pub mod verification;
pub mod evaluation;

pub mod dataflow;
pub mod prediction;
pub mod weightless;
pub mod runtime;

pub mod learned_router;
pub mod revision;
pub mod diagnostics;

pub mod amt;
pub mod checkpoint;

pub mod amt_knowledge;
pub mod amt_classification;

pub mod intelligence_package;

pub mod elicitation;

pub mod lexical;
pub mod classifier_package;
pub mod representation_selection;

pub mod discovery;
pub mod prefetch;

pub mod factual;

pub mod coding;
pub mod python_repairs;

pub mod distillation;
pub mod ast_index;

pub mod compiler_repair;

pub mod patch;
pub mod incremental_syntax;
pub mod sandbox;
pub mod dependency_slice;
