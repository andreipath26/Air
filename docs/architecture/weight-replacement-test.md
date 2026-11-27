# G02 — selective weight-replacement test

## Question

Can one selected capability currently carried by a model be transferred to AIR so that the original model weights can be omitted for that capability, while preserving answer quality and reducing end-to-end resource cost?

This gate tests one capability at a time. Passing factual retrieval does not establish replacement of general language, reasoning, or other model capabilities.

## Systems

Run all systems against the same frozen, independently reviewed workload:

1. **Quantized model baseline:** one pinned CPU-only llama.cpp model, revision, GGUF hash, quantization and flags.
2. **AIR-only:** the converted capability package and router, with no teacher process loaded and no neural fallback.
3. **Hybrid:** AIR handles eligible requests and the pinned model handles only logged fallbacks. Report both model-resident and model-on-demand modes when available; never hide fallback work.
4. **Reference-fed AIR:** optional oracle ceiling, reported separately and never used as the learned-system baseline.

The initial capability is closed-world factual retrieval under G01. Later gates may test procedural skills, classification, or another explicitly bounded capability.

## Workload and controls

- Use at least 240 held-out natural-language questions: four substantive domains with at least 40 cases per domain, plus at least 40 unknown or unsupported cases.
- Have independent reviewers author the final questions and verify reference answers and source provenance. Keep extraction prompts/cases, tuning data, and final test cases disjoint.
- Include paraphrases, ambiguity, conflicting or stale facts, adversarially similar entities, and unsupported questions.
- Freeze and publish dataset, router, corpus, and package hashes before scoring. No changes to the final set after seeing results.
- Run each system on the same Dell Latitude 7490 (i7-8650U, 16 GB RAM, no GPU), with identical task order and documented warm and cold conditions.
- Keep raw prompts, outputs, errors, fallback decisions, logs, and environment identities as artifacts.

## Measurements

Record exact-match and task-appropriate quality, per-domain results, unknown abstention, AIR direct-answer coverage, hybrid fallback rate, first-answer latency, throughput, total wall time, process and whole-system peak RAM, CPU/package energy with a documented meter boundary, package/model storage, and conversion time and energy. Report both inference-only and lifecycle-inclusive costs. Calculate break-even query volume from measured conversion cost and per-query savings; do not treat conversion as free.

Use repeated runs and report uncertainty intervals. Report results per task and as aggregates so an easy domain cannot hide a regression.

## Decision rules

A capability may be called **replaced by AIR** only when AIR-only, with the original model unloaded, meets all of these:

- At least 90% absolute accuracy and at least 95% of the pinned model baseline's accuracy.
- At least 80% accuracy in every domain and at least 95% correct abstention on unknown cases.
- At least 2x correct-answer throughput, 50% lower whole-system peak RAM, and 50% lower energy per correct answer than the pinned model.
- No hidden fallback, and lifecycle costs plus break-even are reported.

The hybrid system may be called **cost-saving selective routing** only if it stays within one percentage point of baseline quality, exposes every fallback, and demonstrates lower measured lifecycle-adjusted resource cost. Hybrid success alone does not prove that weights were replaced: report the fraction of requests answered by AIR without loading or invoking the model.

Failure of any required quality or resource threshold means the capability is not yet demonstrated as replaceable. Preserve negative results and narrow claims to the exact capability tested.

## Scope boundary

G01 already contains a preliminary factual-retrieval gate and development data. Those same-author development results are not G02 acceptance evidence. The G01 harness currently lacks the hybrid fallback comparison and target-laptop whole-system energy measurement; those components must be implemented before claiming the full gate is runnable.
