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
