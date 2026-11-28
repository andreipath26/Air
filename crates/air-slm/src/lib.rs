//! Resource-bounded local SLM inference and escalation for AIR.
pub mod runner;
pub mod routing;
pub use runner::{run_verified, Completion, CompletionBackend, DeterministicMock, GenerationOptions, LlamaCppRunner, ModelFamily, ResourceLimits, ThinkingMode, VerifiedCompletion};
pub use routing::{route_fuzzy, route_prompt, EscalationReason, FuzzyRoute, RouteDecision};
