# AIR Coding Benchmarks

**Component CI: passing on two separate commits. Original dual-mode gauntlet: blocked; no new benchmark run or Dell/model measurement is claimed.**

This report separates GitHub Actions component-test evidence from historical benchmark figures transcribed from the attached conversation. The original 12-case inputs, hidden graders, runner, and `.airw` model file remain unavailable on the feature branch.

## CI-verified component tests

| PR / component | Tested commit | GitHub Actions run | Result |
|---|---|---|---|
| [#10 — `air-neural` HDC and ternary primitives](https://github.com/andreipath26/Air/pull/10) | `632688391ee796723a5d38e46d3adde9f00f8353` | [Run #154 / `46093790182`](https://github.com/andreipath26/Air/actions/runs/46093790182) | All six jobs completed successfully. `cargo test --workspace`: **72 tests** (49 air-core + 16 air-neural + 7 air-weights). |
| [#11 — `air-core` typed coding reducer and oracle seam](https://github.com/andreipath26/Air/pull/11) | `37c8ae3456cc98b4b26efcef1cf92f71c3f07e44` | [Run #151 / `46093725300`](https://github.com/andreipath26/Air/actions/runs/46093725300) | All six jobs completed successfully. `cargo test --workspace`: **62 tests** (55 air-core + 7 air-weights). |
| Combined feature branch workspace | `8178a958331f2d28b858df52710995f1217bc2cd` | [Run #177 / `46094716372`](https://github.com/andreipath26/Air/actions/runs/46094716372) | All six jobs succeeded. `cargo test --workspace`: **78 passed, 0 failed** (55 air-core + 16 air-neural + 7 air-weights). |

These component runs were on different branch states and their counts must not be added as if they were one suite. A subsequent combined feature-branch run is listed below. None of these runs tests the original coding gauntlet.

The combined run completed 78 tests, which is below the previous 82/92 requested suite thresholds. It verifies the feature-branch workspace crates only; it is not an end-to-end coding-gauntlet or SLM acceptance run.

## Current capability boundary

On `feat/neuro-symbolic-dual-gauntlet`, `air-weights` provides an AIRW archive reader and `air-neural` provides packed HDC/ternary primitives. `air-core::coding` ranks exact typed-concept matches, reads candidate archive sections, and calls a caller-supplied verification oracle.

The components are not yet an end-to-end coding assistant: the neural router is not wired into the reducer or a compiled AIRW archive; the reducer does not parse natural language, generate patches, invoke compilers, or supply language-specific repair rules. No `air-slm` crate is merged on this branch. No Gemma 4 E2B or SmolLM3 3B model has been executed.

## Historical results — transcribed, not reproduced

| Reported run | AIR-only | Qwen 7B Q4_K_M | Qwen 3B Q4_K_M | Limit |
|---|---:|---:|---:|---|
| 8-case mechanical Rust smoke set | 8/8; 0.062 s median/case | 7/8; 16.45 s | 6/8; 8.65 s | Synthetic supported repairs; AMD EPYC; no hidden tests/held-out repos; no energy. |
| Initial 12-case gauntlet, unified-diff protocol | 3/12; 0.13 s; 0.27 GiB | 3/12; 33.99 s; 5.22 GiB | 3/12; 9.74 s; 4.46 GiB | AIR passes were correct Tier 4 abstentions; Qwen patch-format failures confounded comparison. |
| Later 12-case run after Search/Replace and Tier 1 rules | 6/12; 0.152 s; 282.8 MiB | 4/12; 5.14 GiB | 3/12; 4.52 GiB | AIR repaired E0382/E0502/E0277 and abstained on the three traps; same-author development set. |
| Reported neuro-symbolic `.airw` v2 run | 6/12; 183.9 ms; 279.4 MiB | Not rerun | Not rerun | Claimed 41,096-byte archive, 16 nodes/15 edges, 15 HDC prototypes, 60,024 ternary parameters; HDC+router 18,014 μs. Archive unavailable. |

All historical runs were reported on an AMD EPYC host, not the Dell Latitude 7490, and are not G01-Code/G02-Code acceptance evidence. The reported 12-case Rust gauntlet comprised E0382, E0502, E0277; multi-file `Option`→`Result` migration; iterator refactor; shared validation trait; borrowed lifetime; RefCell/state invariant; MutexGuard/thread boundary; missing private dependency; contradictory specification; and undocumented unsafe FFI.

The conversation reported these hashes; they cannot currently be verified against the missing files:

- Frozen-input manifest: `c8fb7bb2dcb6c48832cd47ab9193432093e7bdceb0ab4b12d69a2e6febacc445`
- Hidden-evaluator manifest: `a574c94eaa94023a0e6dcffc31a8c45a10c82c80dc6345fd41240d99e79c7128`
- `weights/core.airw`: `09697b685d1bcb61c8d64d6419bd02a4d5b4679dca19b4dc85d51c16294f377b`

## Remaining blockers

The exact 12-case fixture packages, 52 frozen inputs, 12 hidden evaluators, runner, raw reports, and `.airw` archive are absent from the feature branch. No Gemma GGUF/model run or Dell Latitude 7490 measurement exists. Therefore no fresh Mode A/Mode B pass rate, TTFT, tokens/sec, peak RSS, or SLM wake count can be reported.

Restore and hash-check the original assets, complete core/neural integration, merge the SLM adapter, and then run both modes plus pinned Qwen controls on the Dell with identical process boundaries. Keep component CI evidence separate from that end-to-end result.
