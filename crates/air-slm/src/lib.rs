//! Resource-bounded local SLM inference and escalation for AIR.
pub mod grammar;
pub mod runner;
pub mod patch_protocol;
pub mod repair_adapter;
pub mod routing;
pub mod prompt;
#[cfg(feature = "llama-cpp-ffi")]
pub mod native_ffi;
pub use grammar::OutputGrammar;
pub use prompt::{StablePrompt, StablePromptBuilder, STABLE_SYSTEM_PROMPT};
pub use runner::{route_and_run_verified,run_verified,Completion,CompletionBackend,DeterministicMock,GenerationOptions,LlamaCppRunner,ModelFamily,ResourceLimits,RoutedAnswer,SpeculativeConfig,ThinkingMode,VerifiedCompletion};
pub use routing::{route_fuzzy,route_prompt,EscalationReason,FuzzyRoute,RouteDecision};
