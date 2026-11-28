# AIR SLM runtime design and implementation

The air-slm crate adds a bounded escalation path for a locally hosted CPU SLM. It supports a llama.cpp-compatible server loading Gemma 4 E2B or SmolLM3 3B GGUF files, while CI uses a deterministic mock without model downloads.

## Runtime contract

- llama.cpp is launched on loopback with GPU layers set to zero.
- RSS defaults to 6 GiB and cannot be configured above 8 GiB.
- The watchdog samples process RSS every 10 ms and kills llama.cpp when it reaches the configured cap. Startup waits up to 180 seconds for the loopback health endpoint and aborts on early exit or an RSS limit violation.
- Context defaults to 2,048 tokens and cannot exceed 4,096; generation is bounded by runtime and per-call limits.
- Requests, output bytes, timeouts, and thinking budgets are bounded.
- Direct mode disables the template thinking toggle; budgeted mode passes an explicit token budget.
- Answers are not accepted until AIR core's GeneratedOutputVerifier accepts them.
- The deterministic backend is for CI plumbing and benchmark reproducibility, not model-quality evidence.

RSS enforcement uses a short polling interval and process termination. For a kernel-enforced memory.max ceiling with no polling window, launch llama.cpp in an OS memory cgroup/container set at or below the same cap.

## Routing

The deterministic router identifies six Rust coding challenge families. A leading deep word or ! always escalates; otherwise callers provide neuro-symbolic confidence and escalation threshold. Invalid confidence is treated as uncertain. route_and_run_verified dispatches confident requests to the neuro-symbolic callback and verifies escalated model output through AIR core.

## Build

    cargo test -p air-slm

No model file is committed or downloaded by CI.
