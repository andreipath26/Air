//! Resource-bounded local SLM inference and escalation for AIR.
pub mod grammar;
pub mod runner;
pub mod patch_protocol;
pub mod repair_adapter;
pub mod routing;
pub use grammar::OutputGrammar;
pub use runner::{route_and_run_verified,run_verified,Completion,CompletionBackend,DeterministicMock,GenerationOptions,LlamaCppRunner,ModelFamily,ResourceLimits,RoutedAnswer,SpeculativeConfig,ThinkingMode,VerifiedCompletion};
pub use routing::{route_fuzzy,route_prompt,EscalationReason,FuzzyRoute,RouteDecision};
