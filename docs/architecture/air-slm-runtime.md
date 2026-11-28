# AIR SLM runtime design and implementation

The air-slm crate adds a bounded escalation path for a locally hosted CPU SLM. It supports a llama.cpp-compatible server loading Gemma 4 E2B or SmolLM3 3B GGUF files, while CI uses a deterministic mock without model downloads.

## Runtime contract

- llama.cpp is launched on loopback with GPU layers set to zero.
- The server is launched through Linux prlimit with RLIMIT_AS set to the configured memory cap before llama.cpp executes or maps the GGUF. If prlimit is missing or cannot establish the limit, launch fails closed; there is no direct-launch fallback.
- The configured address-space cap defaults to 6 GiB and cannot exceed 8 GiB. Since resident pages must be mapped into the process address space, this bounds the process's maximum possible RSS before model allocations begin.
- The 10 ms RSS watchdog remains as a secondary guard and terminates llama.cpp if measured resident memory exceeds the cap.
- Startup waits up to 180 seconds for the loopback health endpoint and aborts on early exit or a memory-limit violation.
- Context defaults to 2,048 tokens and cannot exceed 4,096; generation is bounded by runtime and per-call limits.
- Requests, output bytes, timeouts, and thinking budgets are bounded.
- Direct mode disables the template thinking toggle; budgeted mode passes an explicit token budget.
- Answers are not accepted until AIR core's GeneratedOutputVerifier accepts them.
- The deterministic backend is for CI plumbing and benchmark reproducibility, not model-quality evidence.

RLIMIT_AS is a hard virtual-address-space bound, not cgroup memory accounting. GGUF mappings or runtime allocations that do not fit cause llama.cpp startup/allocation failure instead of exceeding the configured address-space ceiling.

## Routing

The deterministic router identifies six Rust coding challenge families. A leading deep word or ! always escalates; otherwise callers provide neuro-symbolic confidence and escalation threshold. Invalid confidence is treated as uncertain. route_and_run_verified dispatches confident requests to the neuro-symbolic callback and verifies escalated model output through AIR core.

## Build

    cargo test -p air-slm

No model file is committed or downloaded by CI.
