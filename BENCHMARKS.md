# AIR Coding Benchmarks

**Status: blocked — historical benchmark assets are not recoverable from the checked repository branches.**

This report is a GitHub-API-only audit of `andreipath26/Air` as of 2026-11-28. No benchmark was run in this task. No local shell, `git`, compiler, or model process was used.

## Asset audit

Checked `main` and the coordinator branch `feat/neuro-symbolic-dual-gauntlet`, searched the repository for the historical archive and gauntlet paths, and attempted direct reads of the expected files. The following are absent from both inspected branches:

- `air-weights` crate and `weights/core.airw`
- `air-neural` and `air-slm` crates
- `benchmarks/g01-code-gauntlet/` case packages and runner
- the reported 52 frozen input files, 12 hidden evaluator files, and per-arm raw benchmark outputs

The workspace manifest lists only `crates/air-core`, `crates/air-cli`, and `crates/air-bench`. Searches for the reported `.airw` and gauntlet names returned no indexed files; direct file reads returned 404. The historical feature branch names described in the conversation are not currently present in the audited branch list. Other workstream branches may be created during this task, but they do not restore the missing historical assets unless those exact files appear there.

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

## Blockers to a valid dual-mode rerun

1. Restore the exact frozen benchmark source and hidden evaluator files, including their original manifest files.
2. Restore the matching `air-weights`, `air-neural`, and coding-adapter implementation plus the exact `.airw` archive.
3. Restore or implement the runner that records task outcomes, TTFT, generation throughput, process-tree RSS, cold/warm timings, and SLM invocation counts.
4. Provide a pinned, locally available Gemma 4 E2B GGUF and llama.cpp runtime for Mode B. No API inference is permitted.
5. Execute both modes on the Latitude 7490. Keep the existing results labeled historical and do not replace them with reconstructed values.

Until these inputs exist on an accessible branch, there is no defensible Mode A/Mode B comparison to publish.
