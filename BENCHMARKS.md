# AIR Coding Benchmarks

**End-to-end coding gauntlet: blocked. Component CI: passing for the merged HDC/ternary and coding-reducer work described below.**

This report uses GitHub API records only. No local benchmark, Dell measurement, or model inference was performed for this update.

## CI-verified component work

These are successful GitHub Actions workspace checks for the listed merge commits. They verify Rust workspace/component tests, not the missing 12-case gauntlet or runtime model compatibility. The job summaries show `cargo test --workspace` completed successfully; they do not expose a test-count total here.

| PR | Merge commit | GitHub Actions run | Verified scope |
|---|---|---|---|
| [#10](https://github.com/andreipath26/Air/pull/10) | `632688391ee796723a5d38e46d3adde9f00f8353` | [#154 / `46093790182`](https://github.com/andreipath26/Air/actions/runs/46093790182) — all six jobs completed successfully | `cargo test --workspace` passed. Includes the dependency-free `air-neural` component: packed 10,000-bit HDC operations, ternary routing, and bounded batch-scoring tests. The release microbenchmark harness exists, but this CI run is not a Dell performance measurement. |
| [#11](https://github.com/andreipath26/Air/pull/11) | `37c8ae3456cc98b4b26efcef1cf92f71c3f07e44` | [#151 / `46093725300`](https://github.com/andreipath26/Air/actions/runs/46093725300) — all six jobs completed successfully | `cargo test --workspace` passed, including the bounded `air-core::coding` reducer/oracle seam and its source-level tests. |

### Current component boundary on `feat/neuro-symbolic-dual-gauntlet`

- `air-weights` is in the workspace and supplies an archive reader. No `weights/core.airw` archive is present.
- `air-neural` supplies HDC and packed ternary primitives. It is **not yet wired into** the coding reducer or a compiled `.airw` model.
- `air-core::coding` accepts typed concepts, ranks matching rules deterministically, resolves candidate sections through the archive reader, and delegates acceptance to a caller-supplied oracle. It does **not** parse natural language, generate patches, invoke compilers, include language repair rules, or integrate Tree-sitter/HDC/SLM.
- No `air-slm` crate is present on this feature branch at this update. No Gemma 4 E2B or SmolLM3 3B model was executed. Thinking-mode controls, `deep`/`!` routing, and model-backed patch fallback remain unverified/unmerged.

## Historical coding results (transcribed, not reproduced)

The following measurements were reported in the attached conversation and earlier roadmap text. They are **unverified historical development reports**, not results reproduced from available benchmark inputs. They were not run on the Dell Latitude 7490 and do not pass G01-Code/G02-Code.

| Reported run | AIR-only | Qwen 7B Q4_K_M | Qwen 3B Q4_K_M | Limitation |
|---|---:|---:|---:|---|
| 8-case mechanical Rust smoke set | 8/8; 0.062 s median/case | 7/8; 16.45 s | 6/8; 8.65 s | Synthetic supported repairs; no hidden tests/held-out repos; shared AMD EPYC host; differing memory boundaries; no energy. |
| Initial 12-case gauntlet, unified-diff protocol | 3/12; 0.13 s; 0.27 GiB | 3/12; 33.99 s; 5.22 GiB | 3/12; 9.74 s; 4.46 GiB | AIR's three passes were correct Tier 4 abstentions. Qwen patch-format failures confounded the comparison. AMD EPYC; no energy. |
| Later 12-case run after Search/Replace and Tier 1 rules | 6/12; 0.152 s; 282.8 MiB | 4/12; 5.14 GiB | 3/12; 4.52 GiB | AIR repaired E0382, E0502, E0277 and correctly abstained on the three traps. Same-author development set; AMD EPYC; no energy. |
| Neuro-symbolic `.airw` v2 report | 6/12; 183.9 ms; 279.4 MiB | Not rerun | Not rerun | Reported 41,096-byte archive, 16 nodes/15 edges, 15 HDC prototypes, 60,024 ternary parameters, HDC+router 18,014 μs. The archive and cases are still absent. |

The reported Rust gauntlet cases were: E0382 ownership, E0502 borrow conflict, E0277 trait bound, multi-file `Option`→`Result` migration, iterator refactor, shared validation trait, borrowed lifetime, RefCell/state invariant, MutexGuard/thread boundary, missing private dependency, contradictory specification, and undocumented unsafe FFI. In the later report AIR passed cases 01–03 and abstained correctly on 10–12; it did not solve 04–09.

Reported manifest identifiers remain unverified because the source files are unavailable:

- Inputs: `c8fb7bb2dcb6c48832cd47ab9193432093e7bdceb0ab4b12d69a2e6febacc445`
- Hidden evaluators: `a574c94eaa94023a0e6dcffc31a8c45a10c82c80dc6345fd41240d99e79c7128`
- Historical `weights/core.airw`: `09697b685d1bcb61c8d64d6419bd02a4d5b4679dca19b4dc85d51c16294f377b`

## Remaining blockers

The original 12-case fixture packages, 52-file input set, 12 hidden evaluators, runner, raw per-arm reports, and `.airw` archive are absent from the feature branch. No Gemma GGUF/llama.cpp model run is recorded, and no measurements were taken on the Dell Latitude 7490. Therefore there is no fresh Mode A/Mode B score, TTFT, tokens/sec, peak RSS, SLM wake count, or hardware comparison to report.

Next: recover the exact frozen assets and verify their manifests; finish wiring the neural primitives and coding reducer; merge the SLM integration; then run both modes and pinned Qwen controls on the Dell with identical process boundaries. Keep CI component-test results separate from end-to-end benchmark evidence.
