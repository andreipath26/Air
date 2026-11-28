# AIR evidence and external-reference index

**Purpose:** one durable map to the checked-in AIR coding smoke, its workflow, the preserved Actions evidence, and the external `llm.asm` reference. Checked against `main` after PR #26 was merged (2026-11-28).

Use this page before describing AIR benchmark assets as present or absent. Distinguish the current checked-in smoke from the older frozen benchmark package.

## Current AIR implementation and benchmark files

| Item | Current repository location | What it contains |
|---|---|---|
| Benchmark report and recorded results | [`BENCHMARKS.md`](../../BENCHMARKS.md) | Current CI/smoke results, historical run links, resource figures, and evidence limits. |
| Local SLM Rust integration | [`crates/air-slm`](../../crates/air-slm) | Bounded local llama.cpp loopback runner, routing, thinking controls, deterministic mock, and verification boundary. No model weights are checked in or downloaded by CI. |
| Checked-in 12-case smoke suite | [`crates/air-slm/tests/pipeline_smoke.rs`](../../crates/air-slm/tests/pipeline_smoke.rs) | Twelve static Rust snippets exercising the AIR reduction/oracle path and `rustc --emit=metadata`. Mode B uses `DeterministicMock`, not real model inference. |
| Current benchmark workflow | [`.github/workflows/pipeline-smoke.yml`](../../.github/workflows/pipeline-smoke.yml) | Workspace tests, six fuzzy-routing cases, and Mode A/Mode B static smoke gates. It is the current workflow name; the old `rust-dual-gauntlet.yml` filename is not in the current tree. |
| AIRW archive code | [`crates/air-weights/src/lib.rs`](../../crates/air-weights/src/lib.rs) | Bounded AIRW archive parsing, checksum/manifest validation, and mmap-backed borrowed sections. This is a format/library implementation, not a trained production `weights/core.airw` model. |
| Packed neural primitives | [`crates/air-neural`](../../crates/air-neural) | Packed HDC and ternary routing/spiking primitives; not a trained general-purpose language model. |

### Terminology that must stay precise

The repository **does contain a 12-case Rust pipeline smoke suite and its current workflow**. Do not summarize those as “missing.” The smoke suite is a deterministic test fixture with static snippets. It is distinct from the older frozen benchmark package described in recovered historical notes, which included separately frozen inputs and hidden evaluators. The current smoke does not restore those historical inputs/evaluators or prove the historical results.

Likewise, a historical workflow name may no longer exist as a file even though its successful run and artifact remain accessible in GitHub Actions. State the current filename and link the historical run rather than calling the benchmark workflow absent.

## Successful Actions run and retained artifact

- **Run:** [Rust Dual Gauntlet Benchmark, run 46096546657](https://github.com/andreipath26/Air/actions/runs/46096546657)
- **Result:** completed successfully on feature branch `feat/neuro-symbolic-dual-gauntlet`, head `89102418c3ea42d8a91bb2f44426b2d9e38ed3aa`.
- **Recorded checks:** 110 tests passed, 0 failed; fuzzy routing 6/6; twelve static cases passed in both modes. Mode B used a deterministic mock, with no model inference.
- **Artifact:** [`rust-dual-gauntlet-46096546657`](https://api.github.com/repos/andreipath26/Air/actions/artifacts/15243790532), artifact ID `15243790532`, SHA-256 `a488cd73381a9815c4514cab1bb078db287263e9cd3869942a0b40d7f60a1040`. GitHub reported it unexpired, with expiration on **2027-02-26** when checked.
- **Recorded performance:** see the run-specific table in [`BENCHMARKS.md`](../../BENCHMARKS.md). Its numbers are GitHub-hosted-runner smoke figures, not Dell Latitude 7490 measurements, real-model TTFT/tokens per second, or coding-quality comparisons.

The report and a summary of results are committed in AIR. Raw logs and per-case tables are held in the Actions artifact, not as ordinary source files. The run link and artifact ID are recorded above so they can be located without relying on conversation history.

## External reference: `alesha-pro/llm.asm`

Sources: [project README](https://github.com/alesha-pro/llm.asm/blob/main/README.md) and [Phase 2 bare-metal OS README](https://github.com/alesha-pro/llm.asm/blob/main/02-baremetal-os/README.md). The figures below are claims in that upstream repository, not results independently reproduced by AIR.

The project describes multiple pure x86-64 NASM implementations. Its bare-metal OS page reports a roughly 24 KB kernel that loads Qwen3-0.6B from an `.asmbin` model file, boots without a general-purpose OS, and performs CPU inference. It reports 134.9 tokens/s on a 16-vCPU AMD EPYC QEMU/KVM setup and 55.4 tokens/s on a 12-core Intel Xeon Proxmox VM. Its comparison table reports Qwen3-0.6B Q8 decode against llama.cpp on an AMD EPYC host. The repository also describes a separate MiniGPT trainer and reports Tiny Shakespeare training results.

### Relevance to AIR

- **Useful as a systems and baseline reference:** packed quantized kernels, cache layout, SIMD, threading, model loading, boot/runtime footprint, deterministic parity checks, and a same-host comparison protocol are worth studying.
- **Not evidence of AIR weight replacement:** the bare-metal inference example still loads conventional Qwen3-0.6B model weights from disk/RAM. It is a low-level implementation of model inference, not an alternative learned representation that removes the model weights.
- **Not a Dell performance prediction:** the reported throughput uses different processors and virtualized/server-class systems. AIR must benchmark on the Latitude 7490 with pinned model bytes, compiler flags, threads, prompts, token counts, and identical measurement boundaries before making a target-hardware comparison.
- **Separate the research questions:** a `llm.asm`/llama.cpp comparison can inform low-level inference efficiency; AIR-only coding tests must still run with conventional model weights and model processes absent.
- **Reuse policy:** inspect the upstream license and provenance before copying code or assets. This record evaluates the architecture and published measurements; it does not import their implementation.

### Suggested AIR follow-up experiment

If low-level CPU inference is useful to the roadmap, treat it as a separate baseline study: pin the upstream revision, model-file hash, and build settings; verify the target CPU instruction requirements; compare against llama.cpp on the Latitude 7490; record prompt-evaluation and decode throughput, time-to-first-token, peak process-tree RSS, energy, output parity, and failure cases. Keep this study separate from the AIR-only capability-replacement acceptance gate.
