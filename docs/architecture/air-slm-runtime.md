# AIR SLM runtime design and implementation

The air-slm crate adds a bounded escalation path for a locally hosted CPU SLM. It supports a llama.cpp-compatible server loading Gemma 4 E2B or SmolLM3 3B GGUF files, while CI uses a deterministic mock without model downloads.

## Runtime contract

- llama.cpp server is launched on loopback with GPU layers set to zero.
- Default RSS admission cap is 6 GiB; callers may lower it, never raise it above 8 GiB.
- Context defaults to 2,048 tokens and cannot exceed 4,096; generation is bounded by runtime and per-call limits.
- Requests, output bytes, timeouts, and thinking budgets are bounded.
- Direct mode disables the template thinking toggle; budgeted mode passes an explicit token budget.
- Answers are not accepted until an implementation of AIR core's GeneratedOutputVerifier accepts them.
- The deterministic backend is for CI plumbing and benchmark reproducibility, not model-quality evidence.

The runner samples the llama.cpp process resident set before and after each generation and terminates it if the configured ceiling is exceeded. Sampling is an enforcement guard, not a pre-allocation kernel memory limit. For a strict kernel-enforced RSS ceiling, run llama.cpp in an OS memory cgroup/container configured at or below the same limit.

## Routing

The deterministic router identifies six Rust coding challenge families. A leading deep word or ! always escalates; otherwise callers provide neuro-symbolic confidence and escalation threshold. Invalid confidence is treated as uncertain.

## Build

    cargo test -p air-slm

No model file is committed or downloaded by CI.
