# AIR Model Transcompiler (AMT)

## Research objective
Convert selected learned capabilities of existing pretrained models into alternative AIR representations, without necessarily training a new foundation model. Conversion may require substantial offline compute, targeted training or calibration. The original checkpoint remains unchanged. No assumption of perfect extraction, full factual coverage or a lossless inverse from weights to human-readable information.

## Proposed architecture
```text
Pinned pretrained checkpoint + tokenizer + licence
    |                              |
    v                              v
Weight/activation inspection       Behavioural teacher queries
    |                              |
Features / candidate circuits      Claims / traces / demonstrations
    +---------------+--------------+
                    v
         Candidate capability inventory
                    v
         AIR Curation Engine / quarantine
                    v
         Representation compiler and selector
                    v
    Facts / graphs / procedures / rules / grammars /
    binary classifiers / predictive states / neural residuals
                    v
       Independent validation + lifecycle cost tests
                    v
          Versioned AIR intelligence package
                    v
       Adaptive runtime + neural fallback / abstention
```

## Interfaces (proposed, not implemented)
- Model intake manifest: architecture, tensor metadata, revision, hashes, tokenizer/chat template, quantisation, license and restrictions. Safe readers only; no arbitrary pickle execution.
- Capability candidate: stable ID, task family, teacher checkpoint, discovery method, sample inputs/outputs, evidence references, assumptions, confidence, known counterexamples and representation candidates.
- Compilation result: AIR-IR payload, runtime dependencies, candidate representation cost, validation split IDs, quality metrics, execution limits and rollback reference.
- Package manifest: approved capability IDs, source licenses, content hashes, index versions, compatible runtime, residual model references, routing and fallback policies.
- Experiment evidence: tool versions, seeds, hardware, original teacher baseline, independent dataset provenance, elapsed time, RAM, energy and storage, quality per task, failures and uncertainty.

## Extraction tracks
**Internal analysis:** inspect tensor distributions and redundancy; examine activations via probes, sparse autoencoders, transcoders and causal intervention. Candidate associations must survive intervention and independent testing before being described as causal circuits. No assumption that a category of knowledge is stored in a contiguous weight block. Support small checkpoints first; stream large tensors under bounded memory.

**Behavioural analysis:** systematic teacher elicitation across tasks, with paraphrases, exceptions, contradiction tests and unseen inputs. Convert facts to records (teacher output is unverified); extract procedures from demonstrations rather than trusting generated explanations; use independent reference implementations for arithmetic and code and independently sourced evidence for factual claims. Avoid harvesting sensitive memorised content or violating model terms.

**Hybrid conversion:** use internal findings to propose extraction targets and behavioural evidence to evaluate them. Compare multiple AIR representations and retain neural residuals when cheaper representations fail. Quantisation/pruning/distillation are optional candidate optimisations, not mandatory stages. No original checkpoint modification. Avoid double-counting teacher abilities when neural fallback answers a task.

## Evaluation gates
Freeze discovery, validation and independent final test splits. Report per-domain quality, abstention, coverage, error severity, routing errors, residual invocation frequency, RAM including all concurrent processes, disk bytes, CPU/GPU energy, latency and throughput. Compare against original teacher, standard quantised teacher, conventional distillation and AIR without AMT; include ablations of internal analysis vs behavioural transfer. Conversion cost must be amortised over expected correct tasks; include teacher generation, training, validation, index building and model fallback. Initial targets inherit the master roadmap, not performance promises. A capability-specific pass does not imply general model equivalence. Final inference tests use the mandatory Dell Latitude CPU-only platform; conversion hardware can differ but must be documented.

## Implementation sequence
First establish reproducible small-model teacher runner and capability inventory; then behavioural extraction into existing curation, procedure synthesis and packages; evaluate a held-out pilot; in parallel develop safe weight metadata readers and trace analysis; add representation selection and residual routing; scale only when evidence justifies it. Full independent factual verification and general neural-to-symbolic extraction remain research challenges.
