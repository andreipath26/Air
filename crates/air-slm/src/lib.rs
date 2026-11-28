//! Resource-bounded local SLM inference and escalation for AIR.
pub mod runner;
pub mod routing;
pub use runner::{route_and_run_verified,run_verified,Completion,CompletionBackend,DeterministicMock,GenerationOptions,LlamaCppRunner,ModelFamily,ResourceLimits,RoutedAnswer,ThinkingMode,VerifiedCompletion};
pub use routing::{route_fuzzy,route_prompt,EscalationReason,FuzzyRoute,RouteDecision};
