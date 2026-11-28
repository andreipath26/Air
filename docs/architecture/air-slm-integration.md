# AIR local coding SLM integration contract

Status: design proposal. This distinguishes AIR's generic local-model path from the coding SLM integration still to be implemented. It does not claim Gemma 4 E2B or SmolLM3 3B has been run or benchmarked.

## Current repository boundary

The workspace contains `air-core`, `air-cli`, and `air-bench`; there is no `air-slm` crate. The README documents an operator-started generic llama.cpp loopback server for factual-query and extraction prototypes. That is not a coding Graphmaster adapter, lazy model manager, model-specific Gemma/SmolLM integration, or verified patch path. No `deep` / `!` escalation contract is implemented.

## Purpose and operating modes

The local SLM is optional support around AIR's native representation; it must not become the always-on inference path or count as AIR-only capability.

- **AIR-only:** no SLM initialization, model process, or conventional model weights. This is the weight-replacement benchmark arm.
- **AIR + local SLM:** run AIR first. Invoke the local model only on low confidence, competing reductions, unresolved intent, or explicit escalation. Log every invocation and reason.
- **Offline and no paid API:** use a locally provisioned model through a CPU backend, initially llama.cpp with an operator-supplied GGUF. No hosted inference or runtime model download.
- **Resource caps:** bound context, output tokens, generation time, resident memory, and concurrency. Measure cold/warm latency and require CPU-only behavior on the Latitude 7490; do not assume Intel UHD support.

## Three responsibilities

1. **Offline teacher/organizer:** propose concept taxonomies, canonical reductions, paraphrases, AST templates, and router examples from permitted coding examples, documentation, patches, and compiler/test feedback. Preserve source/license provenance, raw traces, model/runtime hashes, and transformation history. Teacher output is untrusted.
2. **Online request translator:** when AIR cannot confidently interpret a request, return a constrained specification (language, intent, symbols/context, constraints, acceptance conditions, ambiguities, or abstention). Feed it back through AIR's own selection and verification path.
3. **Online tie-breaker/bounded fallback:** rank only supplied top-K AIR candidates or abstain. If none fit, a separately tagged fallback operation may propose a minimal patch. SLM-generated work is never an AIR-only success.

## Verification and learning boundary

- Parse outputs against strict schemas; reject malformed or out-of-scope responses.
- Treat generated facts, rules, templates, and patches as candidates. Apply code only after the same compiler, lint, hidden-test, and diff-safety checks are available to AIR and baseline systems.
- Only independently verified, generalized rules may be staged. Keep reviewable diffs, provenance, validation evidence, versioning, and rollback. One passing fixture does not prove generalization.
- Record model calls and verification separately. Never mutate frozen benchmark packages during scoring.

## Thinking and explicit escalation

The proposed CLI contract reserves a `deep` or `!` prefix for explicit escalation; strip it before sending task text to the model. Translator and tie-breaker calls should use bounded direct output. Explicitly escalated or novel-patch requests may use a separately configured, hard-capped reasoning budget. Validate model-specific thinking controls against the pinned llama.cpp build and chat template.

Strip thought/channel blocks from every response before schema or patch parsing, even when thinking is nominally disabled. Do not persist hidden reasoning as a rule. If output is invalid or exceeds budget, abstain.

## Candidate models and selection status

Gemma 4 E2B and SmolLM3 3B were discussed as CPU candidates in the attached conversation. They are **unbenchmarked candidates**, not selected or integrated models. Compare them using pinned GGUF hashes, llama.cpp revision/build flags, prompt template, context, quantization, and thread settings. Verify license and redistribution terms from authoritative model metadata before packaging.

Benchmark on the Dell: task pass rate, correct abstention, schema validity, fallback rate, time-to-first-token, tokens/second, cold load, warm latency, process-tree peak RSS, energy with a valid meter, storage, and AIR-only impact. Advertised throughput estimates are not evidence.

## Implementation and acceptance sequence

1. Add isolated `air-slm` crate with backend trait, deterministic mock, bounded request/response types, and local llama.cpp adapter. AIR-only must not depend on it at runtime.
2. Add lifecycle/resource limits and distinguish external resident server from AIR-owned lazy start/stop.
3. Wire translator and tie-breaker to AIR confidence/candidates; add `deep` / `!` routing and unconditional thought stripping.
4. Add oracle-gated patch fallback and reviewable versioned staging. Never write directly to the frozen/core AIR archive.
5. Test schemas, abstention, timeout/cancellation, thought stripping, startup failure, and AIR-only zero-spawn behavior offline.
6. Compare AIR-only, AIR+SLM, and pinned conventional model on identical held-out multi-language coding tasks on the Dell. Hybrid success is not weight-replacement evidence.

No SLM gate passes until implementation, pinned model identity, held-out benchmark, and target-hardware resource report exist.
