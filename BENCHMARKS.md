# AIR Coding Benchmarks

**Status: GitHub Actions verifies the current Rust workspace and deterministic pipeline smoke. Historical model results remain unverified, and the original frozen gauntlet remains unavailable.**

This report separates current CI checks and a deterministic pipeline smoke from historical pre-loss model results. The smoke uses static snippets and generated AIRW fixtures; Mode B uses `DeterministicMock` with no model. It did not run Gemma, SmolLM, Qwen, or any model on the Dell Latitude 7490.

## Latest CI verification — 2026-11-28

- CI workflow [run #243](https://github.com/andreipath26/Air/actions/runs/46096786108) completed successfully for head `77c185253b21d37cfadd9f70e88be9e95f5c42c5` (PR merge-ref checkout `da486ae1865e7a31223faf1cdbb518b5d4c74710`). The run summary reports **116 passed, 0 failed**; fuzzy routing passed **6/6**.
- This validates the tested PR merge ref and its included workspace checks; it does not establish a 12-case hidden-gauntlet score or a combined `>=82/92` acceptance threshold for other branch states.

## Pipeline smoke — 2026-11-28

- Workflow [Rust Coding Pipeline Smoke run #3](https://github.com/andreipath26/Air/actions/runs/46096786107) completed successfully on head `77c185253b21d37cfadd9f70e88be9e95f5c42c5` (PR merge-ref checkout `da486ae1865e7a31223faf1cdbb518b5d4c74710`).
- The 12 entries are fixed static smoke snippets. They exercise the pipeline path and Rust metadata compilation; they are not the missing original 12-case benchmark or its hidden evaluators. Mode B uses `DeterministicMock` (**no model inference**).

| Mode | Smoke cases passed | Failed | Mean case time (ms) | CI command wall time (ms) | Peak process RSS (MB) |
|---|---:|---:|---:|---:|---:|
| A: neuro-symbolic pipeline smoke | 12/12 | 0 | 2.443 | 80 | 196.67 |
| B: dual pipeline with `DeterministicMock` (no model) | 12/12 | 0 | 2.451 | 70 | 196.73 |

These timing and RSS values come from a GitHub-hosted Actions runner. They are not Dell Latitude 7490 measurements, do not measure real inference, and provide no TTFT or tokens/second. They do not establish coding quality against Qwen, Gemma, or SmolLM.

## Earlier GitHub Actions session — 2026-11-28

- Workflow: [Rust Dual Gauntlet Benchmark run 13](https://github.com/andreipath26/Air/actions/runs/46096546657), successful on `feat/neuro-symbolic-dual-gauntlet` at `89102418c3ea42d8a91bb2f44426b2d9e38ed3aa`.
- Workspace tests: **110 passed, 0 failed** across 11 libtest summaries; fuzzy routing: **6/6 passed**.
- The same 12 Rust challenges passed through AIR core verification and `rustc --emit=metadata` in both modes. Cases: ownership, borrowing, lifetime, trait, generic, display, async, concurrency, result, option, macro, iterator.
- Mode B uses explicit `deep`/`!` triggers, `DeterministicMock`, and `GeneratedOutputVerifier`; it does not measure real model inference.
- [Actions artifact rust-dual-gauntlet-46096546657](https://api.github.com/repos/andreipath26/Air/actions/artifacts/15243790532) contains raw logs, per-case tables, resource files, and the job summary. The job log was fetched from GitHub Actions API job `140829080436`.

| Mode | Cases passed | Cases failed | Mean case latency (ms) | Wall latency (ms) | Peak Rust test-process RSS (MB) | Peak cargo-invocation RSS (MB) |
|---|---:|---:|---:|---:|---:|---:|
| A: neuro-symbolic only | 12/12 | 0 | 2.748 | 90.0 | 4.00 | 196.75 |
| B: dual + mock SLM escalation | 12/12 | 0 | 2.713 | 90.0 | 4.00 | 196.72 |

RSS boundaries: per-case values read `VmHWM` for the Rust test process and round up to MB. Cargo-invocation values are `/usr/bin/time` maximum RSS (`%M`) converted from KiB to MiB. These are GitHub-hosted runner measurements and do not establish Tier 0 hardware performance or native-model memory use.

## Pre-loss asset audit snapshot

The following inventory records the earlier pre-loss audit, before the current `air-slm` crate and gauntlet implementation were added. It describes missing historical benchmark inputs, not the current workspace state. Checked `main` and `feat/neuro-symbolic-dual-gauntlet` at that earlier point; the original frozen archive and evaluator assets were not present:

- `weights/core.airw` and the original frozen benchmark archive
- `air-neural` and `air-slm` implementations (now added separately; they do not recreate the old frozen benchmark inputs)
- `benchmarks/g01-code-gauntlet/` case packages and runner
- the reported 52 frozen input files, 12 hidden evaluator files, and per-arm raw benchmark outputs

At that time the workspace manifest listed only `crates/air-core`, `crates/air-cli`, and `crates/air-bench`. Searches for the historical `.airw` and gauntlet paths returned no indexed files; direct reads returned 404. The current runtime-generated fixture suite is new work and does not restore those original artifacts.

Because the input packages, hidden graders, executable runner, archive, and Gemma GGUF are missing, this task cannot truthfully report fresh Mode A/Mode B pass rates, TTFT, tokens/second, peak RSS, or model wake counts. Reconstructing new fixtures would create a new benchmark, not recover or rerun the frozen benchmark, so none were fabricated.

## Historical results recovered from ROADMAP.md

These are **unverified historical reports transcribed from the attached conversation**, not results reproduced from files in the current repository. They are not acceptance evidence and were not run on the Dell Latitude 7490.

| Reported run | AIR-only | Qwen 7B Q4_K_M | Qwen 3B Q4_K_M | Evidence boundary |
|---|---:|---:|---:|---|
| 8-case mechanical Rust smoke set | 8/8; 0.062 s median/case | 7/8; 16.45 s | 6/8; 8.65 s | Synthetic supported repairs; no hidden tests or held-out repos; shared AMD EPYC host; inconsistent RSS boundaries; no energy. |
| First 12-case gauntlet, unified-diff interface | 3/12; 0.13 s; 0.27 GiB | 3/12; 33.99 s; 5.22 GiB | 3/12; 9.74 s; 4.46 GiB | All AIR passes were Tier 4 abstentions. Qwen patch-format failures confounded the coding comparison. Shared AMD EPYC, not Dell; energy unavailable. |
| Later 12-case run after Search/Replace and Tier 1 rules | 6/12; 0.152 s; 282.8 MiB | 4/12; 5.14 GiB | 3/12; 4.52 GiB | AIR repaired E0382, E0502, E0277 and abstained on all three traps. Same-author development set; AMD EPYC; no energy. |
| Neuro-symbolic `.airw` v2 report | 6/12; 183.9 ms; 279.4 MiB | Not rerun | Not rerun | Reported archive 41,096 bytes, 16 nodes/15 edges, 15 HDC prototypes, 60,024 ternary parameters; HDC+router 18,014 μs. Archive and tests are absent from checked branches. |

The 12 cases were reported as: E0382 ownership; E0502 borrow conflict; E0277 trait bound; multi-file `Option`→`Result` migration; iterator refactor; shared validation trait; borrowed lifetime; RefCell/state invariant; MutexGuard/thread boundary; missing private dependency; contradictory specification; undocumented unsafe FFI. In the later Tier 1 run, cases 01–03 passed and cases 10–12 were correctly abstained; cases 04–09 were not solved.

Reported integrity identifiers (not re-verified because the corresponding files are missing):

- Frozen input manifest: `c8fb7bb2dcb6c48832cd47ab9193432093e7bdceb0ab4b12d69a2e6febacc445`
- Hidden evaluator manifest: `a574c94eaa94023a0e6dcffc31a8c45a10c82c80dc6345fd41240d99e79c7128`
- `weights/core.airw` v2: `09697b685d1bcb61c8d64d6419bd02a4d5b4679dca19b4dc85d51c16294f377b`

## Remaining work for reproducing historical model benchmarks

1. Restore the exact frozen benchmark source, hidden evaluator files, and original manifests if the goal is to compare directly with the pre-loss reports.
2. Provide pinned Gemma 4 E2B and/or SmolLM3 3B GGUF files plus the llama.cpp runtime for native Mode B. The CI mock intentionally downloads no model.
3. Run the real-model paths on the Latitude 7490 and capture task outcomes, TTFT, generation throughput, process-tree RSS, cold/warm timings, and SLM invocation counts.

The new CI comparison above is valid for deterministic routing, verification, compilation, and fixture-path behavior. It is not comparable to the historical real-model runs.
