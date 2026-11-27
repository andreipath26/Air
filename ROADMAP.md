# AIR — Adaptive Intelligence Representation
## Master research and development roadmap v0.6 (Coding Assistant Deployment Focus)

Status: active engineering and research programme; no performance or general-intelligence claims. All targets remain hypotheses until verified by passing acceptance gates.

# v0.6 Scope Directive

The immediate deployment target is a CPU-first hybrid AI coding assistant running on the mandatory Dell Latitude 7490 (i7-8650U, 4 cores/8 threads, 16 GB DDR4, NVMe, no dedicated GPU). General-intelligence research, historical architecture recovery, and non-coding phases are preserved in this roadmap but explicitly deferred until the coding assistant reaches validated v1.0 deployment.

The v0.5 research programme, hypotheses, implementation phases, technology inventory, baselines, experiments, historical results, and failure clauses remain below as preserved Track B material. Where older text describes immediate or parallel work, this v0.6 directive supersedes it: only Track A is active until its deployment gates pass. Coding-scoped portions of older phases are active only through the v1.0 crosswalk below.

## Track A — Active Execution: Hybrid Coding Assistant v1.0

### Mission and principle

Develop CPU-first local coding assistance with lower quality-adjusted resource costs by separating offline compilation, curation, representation, retrieval, deterministic tool execution, and on-demand neural fallback. Support offline inference, no dedicated GPU, selective activation, inspectability, provenance, rollback, and optional KILN integration. Expensive offline compilation is allowed only with full lifecycle cost and break-even accounting.

**Principle:** Do not store as weights what can be stored as indexed knowledge or code patterns; do not calculate what can be retrieved; do not repeatedly infer what can be compiled and verified by a compiler; do not load a neural model when deterministic tools and compiled packages are sufficient.

### Active architecture

**Offline compilation pipeline:** permissively licensed developer corpora (standard-library documentation, compiler error registries, RFCs, and permitted repositories) plus a licensed frontier teacher → behavioural extraction and canonical reduction → automated verification with the relevant compiler, linter, and tests → versioned, content-addressed AIR coding packages using SQLite and compressed indexes. Every teacher, corpus, extraction run, verification environment, and package must have recorded provenance and license status. Generated candidates are quarantined until checks pass; compilation success alone does not establish semantic correctness.

A frontier teacher such as DeepSeek V4.1-Flash may be evaluated for offline compilation only after the exact model revision, license, local inference support, hardware requirements, and redistribution conditions are verified. Teacher compute, storage, energy, and human review are included in lifecycle and break-even accounting. A teacher used for package compilation is not required at runtime.

**Online CPU-first runtime:** user or IDE request → deterministic canonical reduction and lexical/intent routing → Tree-sitter incremental syntax parsing and language-server queries (initially rust-analyzer; pyright and clangd are later adapters) → compiled AIR coding packages for API lookup, verified diagnostic-to-fix rules, and AST rewrite templates → deterministic patch verification → on-demand quantized neural residual (1.5B–4B through llama.cpp or KILN), unloaded by default and called only on abstention or novel glue logic → verified response or explicit abstention.

Tree-sitter supplies syntax structure, not type or behavioral proof. LSP answers depend on the language server, project configuration, and index state. Their CPU and memory costs are measured as part of the system rather than treated as zero.

### Active execution lanes

| Lane | Active v1.0 priorities | Exit evidence |
|---|---|---|
| W1 Curation and verification | Preserve raw teacher traces; represent errors and abstentions explicitly; track provenance; deduplicate; automatically verify every compiled code artifact with compiler, linter, and tests | No unverified snippet or fix rule admitted; reproducible error classification and full source/license provenance |
| W2 Offline compilation (behavioural AMT subset) | Compile API tables, paraphrase reductions, compiler-error-to-fix rules, and AST templates from approved sources and teachers | Per-capability compilation yield; held-out validation; complete token, time, storage, CPU/GPU energy, and review cost |
| W4 Package compiler | Produce portable, versioned SQLite/Rust packages with hashes, rollback, dependency manifests, and language/toolchain versions | Portable artifacts pass held-out coding tests and reproduce from manifests |
| W5 Coding runtime | Extend air-core and air-cli; integrate Tree-sitter and LSP; deterministic routing; bounded memory; unloaded-by-default neural fallback | End-to-end coding task completion, reliable abstention, and every fallback counted |
| W6 Validation and economics | Independently authored coding benchmark; whole-system RAM and energy; latency; lifecycle cost and break-even against pinned quantized llama.cpp baselines | Reproducible CPU-only evidence on the mandatory Dell |

**W3 Structural AMT is deferred to Track B.**

### Active acceptance gates

**G01-Code — Coding capability transfer:** Freeze at least 240 independently authored tasks: at least 50 tasks in each of four substantive categories (standard-library/API lookup, compiler-error diagnosis and repair, idiomatic AST refactoring, and cross-file/API migration), plus at least 40 unsupported or out-of-scope cases. Use real compilers, linters, unit tests, and hidden regression tests for objective scoring; report human review separately for behavior or maintainability that automated checks cannot establish. Compare a pinned CPU-only quantized coding model, AIR-only end-to-end from natural language with held-out paraphrases, and an optional tool/reference-fed ceiling reported separately (never as the learned-system baseline). Proposed thresholds: at least 90% absolute pass rate; at least 95% of the pinned model's pass rate; at least 80% in every in-scope category; at least 95% correct abstention on unknown cases; at least 2x correct-task throughput; and at least 50% reductions in whole-system peak RAM and energy per correct task on the mandatory Dell. Publish confidence intervals, raw failures, category-level coverage, test-strength limitations, compilation cost, and break-even volume. These are proposed targets, not results.

**G02-Code — Selective model-weight replacement:** Test bounded coding capabilities with the quantized coding model completely unloaded (AIR-only) and with AIR-first routing plus on-demand model fallback. Compare against the same pinned CPU-only model, on identical tasks and the mandatory Dell. Count every fallback, cold model load, compiler/test invocation, and package lookup. AIR-only must meet G01-Code quality targets and lower whole-system peak RAM and energy per correct task by at least 50% before claiming weight replacement. A hybrid result can support a selective-routing/cost-saving claim only if quality remains within one percentage point of baseline and lifecycle-adjusted resource savings are measured; hybrid success alone does not show that model weights were replaced. Create a coding-specific protocol and machine-readable gate before scoring. Preserve the existing G02 factual retrieval protocol and its evidence files without changing their scope.

### Coding comparison models and controls

The proposed primary baseline is Qwen2.5-Coder-7B-Instruct, Q4_K_M GGUF, CPU-only with llama.cpp. Add Qwen2.5-Coder-3B-Instruct, Q4_K_M GGUF, as a size-matched control for a small residual model. Before a benchmark is frozen, pin each exact model revision and GGUF SHA-256, llama.cpp commit, build flags, prompt template, context and generation settings, thread count, and warm/cold protocol. Do not select or retune a model after seeing the held-out test results. Record VRAM as not applicable on the no-dGPU target. A newer model may replace a candidate only through a documented pre-benchmark review, with the old baseline retained for reproducibility.

### Active implementation phases (v1.0 scope)

| AIR phase/work packages | v1.0 coding scope |
|---|---|
| P02 | One-command Dell measurement of peak RAM, resident working set, CPU cycles, latency, energy, compiler/test pass rate, and lifecycle break-even |
| P03 and P05 | Typed coding schema; SQLite indexed retrieval; language/toolchain-versioned manifests (initially rust-std); content hashes and rollback |
| P04, P09, P23 behavioural subset | Offline coding-package extraction and curation from approved sources/teachers, with automated verification; no broad factual transfer work |
| P06 and P07 | Deterministic-first coding router, canonical reduction, lexical/intent routing, confidence thresholds, and explicit fallback accounting |
| P08 and P16 code-native subset | Tree-sitter, LSP, exact symbol/type diagnostics, deterministic patch realization; general NLP interpretation and realization stay deferred |
| P18 | CPU-native Rust runtime, air-core/air-cli/air-bench, mmap and compressed indexes, optional llama.cpp/KILN residual backend |
| P20 | Sandboxed compiler/test execution, resource budgets, package integrity, explicit approval/revocation, audit, and rollback |
| P22 | Local coding CLI and resident JSON/LSP-compatible runtime, package manager, diagnostics, threat model, docs, and benchmark publication |

No v1.0 deployment claim until G01-Code and G02-Code pass and P22's coding-scoped deployment criteria are met. Phase IDs are retained for continuity.

## Track B — Deferred Research Programme (post-v1.0)

**Deferral policy:** Track B is frozen: no implementation, benchmarking, or integration work until Track A passes G01-Code, G02-Code, and the coding-scoped P22 deployment gate. The historical phase definitions below remain available as research backlog and are not current execution instructions. A v1.0 coding result does not establish general intelligence, general weight replacement, or success on non-coding domains.

### Deferred lanes and active-plan exclusions

- W3 structural AMT and mechanistic interpretability (P23.02–P23.03): weight entropy/rank, activation clustering, sparse autoencoders, transcoders, activation patching, circuit tracing, and experiments E01, E31, E32.
- All general-purpose knowledge acquisition, non-coding domain packages, and broad conversational capabilities.
- All old general-purpose phase work except the coding-scoped crosswalk explicitly listed in Track A.

### Gate and record separation

G01-Code/G02-Code are new coding gates. Existing G01 factual capability transfer, existing G02 factual weight replacement, their JSON gate files, datasets, and historical artifacts remain intact and deferred. Coding gates require their own frozen workload, evaluator, model hashes, and protocol; results must not be merged with or used to pass the factual gates. The active coding baseline candidates are Qwen2.5-Coder-7B-Instruct Q4_K_M and the 3B control; the existing factual gate's pinned baseline remains governed by its own manifest.

## Preserved v0.5 record and deferred programme

The following v0.5 material is retained for traceability. Its general-purpose execution priorities are superseded by Track A until coding v1.0 is validated. Existing development scores remain diagnostics and do not constitute coding acceptance evidence.


## Mission and directives
Develop CPU-first local intelligence with substantially lower quality-adjusted resource costs. Separate learning, curation, representation, retrieval, reasoning, execution and language realisation. Offline inference, no dedicated GPU, selective activation, representation independence, automatic learning, compositional reuse, continuous incremental updates, scientific validity, generalisation, reuse before rebuilding, portability, inspectability and optional KILN integration. Expensive training/compilation is permissible only with lifecycle cost accounting. Never assume transformers or weights are mandatory.

## Preserved historical execution priorities — post-pilot revision (v0.4; superseded by v0.6)

Initial real-model experiment: GitHub Actions run 45971173610, artifact 15185626215. Small public fixtures suggest promising classification transfer; factual and procedural extraction failed. The bridge ignored JSON constraints and did not preserve raw responses. Missing model knowledge versus formatting failures cannot yet be distinguished. No acceptance gate is passed.

Six concurrent development lanes govern implementation. All prior hypotheses, phases, experiments, baselines and gates remain binding.

| Lane | Immediate priorities | Exit evidence |
|---|---|---|
| W1 Curation | Raw responses, structured and plain extraction, explicit error/abstention states, entity resolution, provenance, independent verification, conflict handling and deduplication | Independently checked claims and reproducible error classification |
| W2 Behavioural AMT | Universal interrogation, capability inventory, paraphrases, counterexamples, 0.5B/1.5B/3B/7B and other licensed teachers, resumable orchestration | Per-capability results distinguished from communication failures |
| W3 Structural AMT | Weight entropy/rank, activation clustering, sparse autoencoders, transcoders, activation patching and circuit tracing | Controlled causal interventions, not mere correlations |
| W4 Compiler | Compare facts, graphs, programs, logic, decision trees, weightless classifiers, predictive states, grammar, causal and probabilistic structures, case memory and neural residuals | Held-out quality, lifecycle cost, portable artifacts and explicit approval |
| W5 Runtime | Integrate validated capabilities, adaptive routing, selective loading, bounded memory, caching, neural fallback, verification and language realisation | End-to-end task completion and measured resource use |
| W6 Validation | Independent larger datasets, conventional lexical and neural baselines, quality-adjusted latency/throughput, whole-system RAM, energy and conversion amortisation | Reproducible target-laptop CPU-only evidence |

Immediate sequence within lanes: repair and instrument communication; compare teachers; evaluate candidate representations; integrate validated capabilities; measure full-system costs. Structural work proceeds independently. Initial research targets remain >=95% teacher accuracy on meaningful independent workloads, >=2x quality-adjusted throughput, >=50% RAM and energy-per-correct-task reductions. Report absolute accuracy, coverage, per-domain regressions and negative results. Weak teachers and trivial tasks do not establish general capability preservation.

## Preserved v0.5 evidence-driven priority (general factual capability; deferred under v0.6)

The master research programme is preserved. W1 and W6 are prerequisites for interpreting behavioural transfer results; W2, W4 and W5 concentrate on the next falsifiable capability gate. W3 remains an independent exploratory lane with bounded experiments and appropriate controls. Technology inventory is a research backlog, not a requirement to integrate every system.

**G01 — factual capability transfer:** Freeze an independently reviewed question workload of at least 240 cases, including four substantive domains and at least 40 unknown/unsupported cases. Compare pinned quantized llama.cpp inference, direct reference-fed indexed retrieval and teacher-fed AIR retrieval, end-to-end from natural language with held-out paraphrases. Require >=90% absolute accuracy, >=95% relative teacher accuracy, >=80% per-domain accuracy, >=95% correct abstention on unknown cases, >=2x correct-answer throughput and >=50% reductions in measured RAM and energy per correct task on the mandatory Dell. Record conversion costs and break-even. These proposed gate-specific thresholds supplement, not replace, existing phase gates. See research/evidence-gate-01.json.

**G02 — selective model-weight replacement:** Compare a pinned CPU quantized model, AIR-only with weights unloaded, and AIR-first hybrid with every fallback counted. The runner, evaluator, and resident/on-demand fallback integration are implemented; passing still requires independently reviewed frozen data and measurements on the mandatory Dell. AIR-only must preserve quality and meet RAM, energy, and throughput thresholds before claiming weight replacement. See [the runnable protocol](docs/architecture/weight-replacement-test.md) and [machine-readable gate](research/evidence-gate-02.json).

## Verified implementation snapshot before coding pivot — 2026-11-27

Audited against `main` commit `dcf5fdb` (`Automate teacher ensemble reconciliation and pinned quantized development experiment`). This section records code that exists; it does not mark the associated research phase complete or claim that its acceptance gate passed.

| Area | Implemented in this repository | Current boundary |
|---|---|---|
| Core and CLI | Rust workspace with `air-core`, `air-cli` and `air-bench`; CLI import, lookup, category, query, session, diagnostics and resident line-oriented JSON runtime | Local `cargo` is unavailable in the audit environment. The latest CI run on this commit passed the six correctness, storage, cognition, compiler/security, performance and AMT lanes. |
| Knowledge and retrieval | Typed facts, SQLite-backed indexed retrieval, categories, bounded lookups, source metadata, package manifests, aliases, revision history, compression and temporal facts | Indexed/reference backends and fixtures exist; the roadmap's portable AIR-IR and larger-than-RAM scaling gates remain open. |
| Reasoning and memory | Deterministic arithmetic, reviewed factual answers, causal/deductive/probabilistic paths, bounded plans/dataflow, procedural memory and restricted integer-skill induction | These are narrow prototypes with explicit operator review/approval paths; they do not establish broad semantic understanding or general program learning. |
| Routing and language | Pattern/lexical routing, trainable lexical classifier, optional loopback local-model routing/fallback, English/French/Romanian templates | No embedded SLM, general semantic parser, or broad multilingual generation is implemented. |
| AMT and capability packages | Safe-tensor checkpoint inventory/statistics, teacher interrogation, fact/classification/procedure tracks, experimental package export and explicit package execution/approval | Extraction remains experimental. Teacher answers are candidates, not verified truth; no capability-preservation gate has passed. |
| G01 pipeline | Draft corpus/router, teacher and reference ingestion, natural-language benchmark, trace reconciliation, answer normalization, evidence assembly, review template, real-model development workload and pinned quantized comparison workflow | The current 200-fact/240-question workload is same-author development data with draft references and related domains. CI smoke cases and development runs are not independent acceptance evidence. Complete target-laptop whole-system RAM/energy measurements and independent review are missing. |
| Safety and operations | Resource budgets, bounded inputs, provenance/review records, package hashes, explicit skill approval/revocation, rollback and CI workflows | Controls are prototype-scoped. Production hardening, authenticated independent review and deployment readiness remain unproven. |

The ledger now marks P09 `prototype_partial` because teacher interrogation and extraction code exists. `planned` at a work-package level means the specified work and its acceptance evidence are not complete; it must not be read as “no code exists.” Phase-level `prototype_partial` likewise does not imply a passed gate. Keep completion at zero until the corresponding acceptance criteria have reproducible evidence. The phase and work-package inventory remains in `research/implementation-ledger.json`.

Latest completed CI on the audited commit passed. The AMT matrix and quantized comparison were still running at audit time. Inspecting the preceding failed AMT matrix job found 502 successful HTTP responses; `g01-real-development.py` had counted malformed model answers as generic errors and then mislabeled them as transport failures. This revision records raw answers and separates malformed/abstained outputs from actual request exceptions, and adds regression coverage. It also makes AMT reconciliation run after non-cancelled pilot failures so available artifacts are analyzed, while preserving the failed pilot result and never converting a partial run into a green experiment. See [G01 automation](docs/architecture/g01-automation.md).

The preceding completed 0.5B development artifact (AMT run `45978770516`, teacher revision `7ae557604adf67be50417f59c2c2f167def9a775`) converted 112/200 draft facts. On the 240-question same-author workload, the FP32 teacher scored 121/240 (50.4%), teacher-fed AIR scored 152/240 (63.3%), and direct reference-fed retrieval scored 240/240 (100%). Per-group AIR accuracy was geography 82%, science 82%, engineering 56%, general reference 4%, and unsupported questions 100%; the teacher scored 80%, 82%, 46%, 16%, and 22.5%, respectively. The reference-fed result is an oracle-style ceiling, not a fair learned-system baseline. The draft corpus, router and paraphrases were authored together, the unknown group is synthetic, the reference-fed system was supplied the answers, and the FP32 teacher remained resident during AIR timing. These figures are development diagnostics only: G01 remains unpassed. The one reported “error” was a malformed answer, not a transport failure.

The corrected 0.5B rerun (AMT run `45982358534`, artifact `15189469633`, teacher revision `7ae557604adf67be50417f59c2c2f167def9a775`) scored teacher-fed AIR 157/240 (65.4%); the FP32 teacher scored 121/240 (50.4%), and direct reference-fed retrieval scored 240/240 (100%). AIR group accuracy was geography 84%, science 90%, engineering 56%, general reference 4%, and unsupported questions 100%. The teacher had one malformed answer, nine abstentions, and zero transport errors; the pilot job passed.

The completed 1.5B run from the same matrix (artifact `15190766881`, teacher revision `989aa7980e4cf806f80c7fef2b1adb7bc71aa306`) converted 190/200 draft facts. Teacher-fed AIR scored 230/240 (95.8%); the FP32 teacher scored 186/240 (77.5%). The automated, reference-validated union of both teacher traces retained 192/200 facts and scored 232/240 (96.7%) on AIR, with group results of geography 96%, science 100%, engineering 98%, general reference 90%, and unknown abstention 100%. Reconciliation completed successfully (artifact `15191510844`). These scores clear the numerical accuracy targets on this development workload, but do not pass G01: the dataset and paraphrases are same-author, unknown examples are synthetic, reference-fed retrieval is an oracle ceiling, and no target-machine whole-system RAM/energy evidence exists. The FP32 teacher also remained resident during AIR timing, so the apparent latency contrast is not a valid end-to-end resource comparison.

**Pre-pivot factual-lane next target (deferred under v0.6):** improve factual-store coverage without relaxing reference checks. The saved 0.5B trace contains four SI-symbol answers in verbose but recoverable formats; relation-aware extraction now normalizes these and admits a candidate only when it matches the reviewed reference. Offline replay raises its accepted draft facts from 117/200 to 121/200. Analysis of the eight misses in the 192-fact teacher union found two geography answers that differed only by diacritics and six genuinely incorrect model answers. Geography matching now accepts accent variants while preserving canonical stored values; scientific notation stays case-sensitive. Replaying the two saved teacher traces raises the reference-matched union to 194/200, recovering Iceland and Colombia. This is an extraction replay, not a new model run or AIR accuracy measurement. The end-to-end retrieval benchmark remains to be rerun; this workspace lacks `target/release/air-cli` and Cargo. Continue coverage and failure reporting by relation/domain, keeping teacher knowledge gaps distinct from router failures.

Historical small pilot 45974698319: 0.5B facts 3/6, 1.5B facts 6/6; weightless classifier 3/6 and 5/6, reference-trained lexical baseline 6/6 for both; no arithmetic candidate. Coordinate ablation changed top predictions on 5/6 and 4/6 prompts, respectively; fixed disjoint controls 0/6, but not magnitude-matched. No scientific acceptance gate passed. Prioritize factual extraction with real natural-language routing and a direct reference-fed retrieval comparator. Retain weightless and procedural work as hypotheses, not presumed winners. Preserve all negative results.

## Hypotheses
- H01 **Generative intelligence encoding:** Compact reusable instructions may replace independently stored numerical relationships. Foundations: Genetics, linguistics, program synthesis.
- H02 **Structural intelligence:** Connectivity, topology and executable relationships may encode learned information. Foundations: Chemistry, graph theory, neuroscience.
- H03 **Intelligence compilation:** Optimal learning representation may differ from optimal inference representation. Foundations: Compiler engineering, consolidation.
- H04 **Addressable intelligence:** Much knowledge may be retrieved instead of reconstructed. Foundations: Databases, bioinformatics, associative memory.
- H05 **Modernised A.L.I.C.E.:** Modern semantic learning may overcome historical conversational architecture limitations. Foundations: AIML, ChatScript, semantic parsing.
- H06 **Weightless intelligence:** Selected capabilities may execute through logic, binary patterns and lookup. Foundations: Digital electronics, HDC, logic networks.
- H07 **Category-based selective activation:** SLM coordinates larger stores with small active working set. Foundations: Information science, cognition.
- H08 **Task-centric inference:** Compute may scale with task complexity rather than total knowledge. Foundations: Control theory, NARS, adaptive execution.

- H09 **Pretrained intelligence transcompilation:** Selected capabilities of pretrained models may transfer into cheaper AIR representations without full foundation-model retraining; residual neural inference remains when necessary.

## Cross-disciplinary research
- Genetics: generative instructions
- Epigenetics: conditional expression
- Developmental biology: reusable regulation
- Immunology: recombination
- Evolutionary biology: selection
- Neuroscience: memory and consolidation
- Cognitive psychology: schemas
- Chemistry: structural behaviour
- Crystallography: repeated structures
- Materials science: structure-dependent function
- Statistical physics: minimal predictive states
- Thermodynamics: energy and data movement
- Quantum information: correlations
- Optics: distributed representations
- Acoustics: harmonic decomposition
- Information theory: compression limits
- Algorithmic information: generating programs
- Linguistics: composition
- Semiotics: symbols and meaning
- Category theory: transformation composition
- Topology: connectivity and invariance
- Automata: predictive states
- Cybernetics: feedback
- Control theory: budgeted decisions
- Digital electronics: logic
- Compiler engineering: executable representations
- Databases: addressing
- Bioinformatics: compressed sequences
- Cryptography: content addresses
- Telecommunications: error correction
- Distributed systems: coordination
- Signal processing: sparse representations
- Complexity science: emergence
- Statistical prediction: language models
- Evolutionary computation: discovery
- Computer architecture: cache and hierarchy

## Technology inventory
### Historical dialogue
ELIZA; PARRY; ALICE; AIML; Graphmaster; ChatScript; RiveScript; OpenDial; Rasa.

### Cognitive architectures
Soar; ACT-R; OpenCog Hyperon; AtomSpace; MeTTa; MORK; NARS; MicroPsi; Copycat / Metacat; Hierarchical Temporal Memory; Case-Based Reasoning.

### Expert systems
DENDRAL; MYCIN; PROSPECTOR; XCON; CLIPS; Rete; Prolog; Datalog; Drools; Cyc; Wolfram Alpha; Soufflé; Ascent.

### Language
WordNet; FrameNet; ConceptNet; Grammatical Framework; Universal Dependencies; AMR; Construction Grammar; Finite-state transducers; Infini-gram; Context-tree prediction; Grammar compression.

### Hybrid AI
Engram; RETRO; Memory Layers at Scale; Product Key Memory; RouterRetriever; RAPTOR; FrugalGPT; LINC; Logic-LM; Scallop; DeepProbLog; AlphaGeometry; MemGPT; GraphRAG; Knowledge Vault.

### Compilation
DreamCoder; Stitch; LILO; MIPS; Tracr; Learning Transformer Programs; AI Feynman; egg; Knowledge compilation; Weightless neural networks; Differentiable logic gates; HDC; Vector symbolic architectures; Probabilistic circuits; Binary decision diagrams; Predictive state machines; BitNet; T-MAC; ZipNN.

### Storage
FM-index; Burrows–Wheeler transform; Succinct data structures; Wavelet trees; Compressed suffix arrays; Suffixient arrays; De Bruijn graphs; Content-addressable storage; Oxigraph; SQLite; MORK; Memory mapping; Dictionary encoding.

Every technology requires source, scientific foundations, algorithms, language, license, maintenance, capabilities, benchmarks, limitations, memory, CPU suitability and integration cost audit before adoption. Distinguish historically constrained mechanisms from fundamental limitations. Priority: ALICE/ChatScript; Hyperon/MORK; Soar; NARS; Grammatical Framework; DreamCoder/Stitch/LILO; LINC/Scallop; FM-index/succinct structures; MIPS/egg; Engram/Infini-gram.

## Architecture
Training and experience -> teacher/discovery -> curator -> compiler -> universal store. User request -> deterministic router / SLM -> Graphmaster 2.0 -> selected knowledge and capabilities -> adaptive reasoning -> CPU runtime -> language realisation -> verified response or uncertainty. Feedback updates are independently validated. Store supports relational facts, semantic and causal graphs, temporal records, programs, decision diagrams, logic, state machines, grammars, binary patterns, compressed sequences, probabilistic structures, episodic records and residual neural tensors. Categories are overlapping references, not duplicated facts. No assumption that existing dense weights can be categorised by topic.

## Preserved general-purpose implementation phases (Track B; coding-scoped work is governed by Track A)
### Phase 00 — Scientific foundations
Complete cross-disciplinary ledger; formalise hypotheses; information-theoretic limits; storage–compute tradeoffs; competing architectures; prior art; reusable code; licensing; falsifiable experiments.

**Acceptance gate:** Every hypothesis has baseline, experiment and failure criterion.

### Phase 01 — Historical AI architecture recovery
Source-level AIML/ALICE, Graphmaster, ChatScript; historical benchmarks; cognitive and expert systems; distinguish historical constraints from fundamental limits; modernised specification.

**Acceptance gate:** Reproducible historical baselines and evidence-based reusable mechanism catalogue.

### Phase 02 — Benchmark and experimental infrastructure
One-command measurement of peak RAM, resident working set, CPU cycles, instructions, cache misses, SSD reads, energy, first response, latency, accuracy, unseen tasks, storage and lifecycle cost.

**Acceptance gate:** Reproducible measurements across historical, neural, symbolic and hybrid baselines.

### Phase 03 — Universal information representation
Typed schema; compact binary encoding; content addresses; references; versioning; mmap; compression/random access; executable representation; neural compatibility; corruption detection.

**Acceptance gate:** Portable validated heterogeneous directly addressable format.

### Phase 04 — Information Curation Engine
Acquisition; semantic interpretation; entity resolution; category discovery; semantic deduplication; provenance; contradictions; temporal validity; relationships; abstraction; representation selection; compression; quality; incremental updates; multilingual consolidation; quarantine.

**Acceptance gate:** Automatically curate validated searchable knowledge without manually authoring each record.

### Phase 05 — Categorised knowledge storage
Multidimensional categories; canonical records with multiple references; MORK/Oxigraph/compressed indexes; disk-backed storage; domain package manifests.

**Acceptance gate:** Store larger than RAM searchable under controlled memory budget.

### Phase 06 — Graphmaster 2.0
Deterministic and approximate matching; semantic retrieval; contextual disambiguation; cross-category traversal; recursion; wildcards; dynamic updates; confidence fallback.

**Acceptance gate:** Supported queries without large-model inference and unseen paraphrases via semantic routing.

### Phase 07 — Intelligent SLM router
Intent, domain, subdomain, knowledge, capability, budget, confidence; deterministic classifier then semantic model then resident 0.5–1.5B SLM, testing smaller options first.

**Acceptance gate:** High routing recall on unseen/interdisciplinary questions, measured overhead and recovery.

### Phase 08 — Semantic language interpretation
AMR, FrameNet, WordNet, ConceptNet, Universal Dependencies, GF; typed intent/entities/relations/constraints/context/uncertainty/actions.

**Acceptance gate:** Equivalent paraphrases map compatibly; ambiguity distinguished.

### Phase 09 — AI teacher and automated learning
Extract knowledge, categories, relationships, procedures, rules, exceptions and tests; independent validation.

**Acceptance gate:** Transfer selected skills and knowledge without teacher at inference.

### Phase 10 — Cognitive memory architecture
Working, semantic, episodic, procedural, associative, predictive and metacognitive memory; independent retrieval/update/consolidation/activation.

**Acceptance gate:** Each memory type independently usable.

### Phase 11 — Intelligence Genome
Primitive discovery; behaviour decomposition; reusable program libraries; cross-domain abstraction; composition; evolutionary optimisation; vocabulary expansion; generalisation.

**Acceptance gate:** Automatically discovered primitives solve unseen compositions with lower resource cost.

### Phase 12 — Intelligence Compiler
MIPS, AI Feynman, equality saturation, synthesis, symbolic regression, state extraction, probabilistic circuits, knowledge compilation; native code, rules, graphs, states, patterns and neural residual.

**Acceptance gate:** Replace selected learned behaviour with cheaper validated representations.

### Phase 13 — Weightless intelligence
WNN, differentiable logic gates, binary associative memory, HDC, VSA, automata and structural learning.

**Acceptance gate:** Meaningful learned capability beats optimised neural baseline at comparable quality or record negative result.

### Phase 14 — Hybrid reasoning engine
Deductive, inductive, abductive, causal, analogical, temporal, probabilistic, mathematical, procedural, common-sense, case-based and neural reasoning.

**Acceptance gate:** Cross-domain composition with inspectable evidence where applicable.

### Phase 15 — Resource-aware cognitive controller
Task priority, budgets, depth, residency, confidence, escalation and stopping; prevent runaway traversal and loops.

**Acceptance gate:** Respect explicit RAM/time/compute budgets at specified quality.

### Phase 16 — Language realisation engine
Templates, GF, finite-state transducers, compressed patterns, SLM and neural generation; multilingual tests.

**Acceptance gate:** Structured answers without neural decoding.

### Phase 17 — Adaptive task-centric inference
Interpret, route, retrieve, execute, verify, express; simple fact lookup and arithmetic, causal diagnosis, creative generation, research paths.

**Acceptance gate:** Adaptive mixed-workload execution beats static baseline.

### Phase 18 — CPU-native execution runtime
Rust, C/C++ kernels, GGML, optional KILN, AVX2/SIMD, Cranelift/LLVM, MORK, egg, mmap, predictive cache, compressed indexes, threading.

**Acceptance gate:** Portable heterogeneous operations with measured CPU/memory efficiency.

### Phase 19 — Continuous learning and consolidation
Incremental acquisition, revision, contradictions, abstraction, reusable skills, versioning and rollback.

**Acceptance gate:** Improve selected capabilities without corrupting knowledge or full retraining.

### Phase 20 — System integration and safety
Provenance, poisoning and injection resistance, sandbox, permissions, budgets, transactions, rollback, corruption recovery, loops, uncertainty, audit.

**Acceptance gate:** Adversarial and recovery tests before unrestricted acquisition/execution.

### Phase 21 — Scientific validation and scaling
Increasing task complexity and store sizes; competitive baselines; ablations; independent generalisation tests.

**Acceptance gate:** Reproducible quality-adjusted improvements on mandatory hardware.

### Phase 22 — Production deployment
Runtime, CLI, local API, chat, knowledge manager/packages, model manager, diagnostics, benchmarks, documentation, threat model, research publication.

**Acceptance gate:** All required gates pass; no deployment claim on code completion alone.

## Phase 23 — AIR Model Transcompiler (AMT; general-purpose scope deferred, coding subset active only as specified in Track A)

**Status:** prototype_partial. Checkpoint inspection, behavioral probes, selected extraction tracks, candidate packaging, and experiment automation exist; no working full-model transcompiler or capability-preservation result has passed its gate.

**Purpose:** Reuse intelligence already learned by existing pretrained models without requiring full model training from scratch. Treat AMT as a hybrid conversion pipeline, not a lossless decoder of all knowledge from weights. Behavioural extraction is the practical first path; direct weight/activation analysis is a complementary research path. Existing weights are distributed and cannot be assumed to map one-to-one to facts, topics or procedures.

### Work packages
- P23.01 **Model intake and licensing:** Validate model identity, exact revision, tokenizer, architecture, tensor inventory, hashes and licenses; reject unsafe deserialisation and unauthorised extraction.
- P23.02 **Weight and activation inspection:** Read safe tensors and GGUF metadata; stream tensor statistics, entropy, redundancy, rank and sparsity; optionally collect bounded activation traces.
- P23.03 **Mechanistic interpretability:** Evaluate sparse autoencoders, transcoders, probing, activation patching and circuit tracing; distinguish association from causal intervention.
- P23.04 **Behavioural capability discovery:** Build domain and capability inventories, teacher queries, paraphrases, counterexamples, multilingual tasks, exceptions and abstention tests; record teacher provenance.
- P23.05 **Teacher-to-AIR knowledge transfer:** Convert candidate claims into curated canonical knowledge with evidence, conflict detection and independent validation; never treat teacher output as truth.
- P23.06 **Procedure and structure extraction:** Extract demonstrations, rules, causal candidates, grammar patterns and reusable algorithms; use bounded synthesis and validation on unseen compositions.
- P23.07 **Representation selection and compilation:** Evaluate candidate facts, graphs, programs, decision diagrams, state machines, grammars, binary classifiers and neural residuals using measured quality and lifecycle costs.
- P23.08 **Residual neural execution:** Retain original model as reference and fallback; evaluate targeted distillation, quantisation, pruning or specialised modules only under capability-preservation gates.
- P23.09 **Conversion orchestration and checkpoints:** Resumable offline pipeline, deterministic manifests, shard streaming, bounded resources, reproducible experiment metadata, candidate quarantine and rollback.
- P23.10 **Capability-preservation validation:** Independent ground truth, held-out paraphrases, reasoning, adversarial and cross-domain tasks; compare teacher and converted system, record regressions and abstentions.
- P23.11 **AIR package and runtime integration:** Build versioned installable intelligence packages with provenance, dependencies, router and residual model references; integrate with AIR-IR and CPU runtime.
- P23.12 **Lifecycle economics and scaling:** Measure conversion time, GPU/CPU energy, peak RAM, storage, inference energy and latency per correct task; report amortisation and break-even.
- P23.13 **Safety, negative results and release gates:** Test poisoning, memorised sensitive material, prompt injection, unsafe outputs, model compatibility, license compliance, rollback and reproducibility; publish failures.

**Acceptance gate:** Demonstrate reproducible conversion of at least one selected pretrained-model capability into a cheaper AIR-native representation, with independently validated held-out quality, full lifecycle accounting, explicit fallback and rollback. General model conversion and removal of neural residuals require separate stronger evidence.

**Dependencies:** P02–P05, P07–P09, P11–P13, P15, P17–P21. AMT is numbered P23 to preserve all existing phase IDs, but starts alongside the research, teacher, curation and compiler workstreams, before production deployment. P22 must not imply AMT readiness until its applicable gates pass.

### Conversion strategy
1. Freeze and fingerprint an appropriately licensed pretrained teacher (initial small open-weight instruction model; Qwen, Llama or Mistral families subject to model-specific permissions).
2. Run internal analysis and behavioural discovery in parallel; begin with bounded factual and procedural tasks, then expand to dialogue, classification, causal candidates and composition.
3. Curate candidates into canonical records with source, uncertainty, version and provenance; independent evidence rather than teacher agreement is required for factual validation.
4. Compile and select the least expensive validated representation per capability: structured knowledge, executable procedures, graphs, rules, grammars, binary patterns, predictive states or neural residuals.
5. Compare original teacher, ordinary compressed neural baseline, behavioural-only conversion, interpretability-assisted conversion and hybrid AIR. Keep residual inference when quality is not preserved; do not remove weights merely because a behaviour was reproduced in a limited test.
6. Package validated components and residual references; maintain rollback and independently test the converted system on the mandatory CPU-only laptop.

**Failure clauses:** No general lossless weight-to-knowledge inversion assumed. If direct internal extraction fails, continue behavioural transfer; if a compiled capability fails quality gates, retain neural fallback or abstain. If conversion costs outweigh inference savings, report the measured negative result. Never claim full model equivalence from selected-task success.

## Knowledge acquisition
Wikidata; Wikipedia; DBpedia; ConceptNet; WordNet; FrameNet; BabelNet; SUMO; OpenCyc archives; ATOMIC; OpenAlex; OpenStax; Project Gutenberg; OpenStreetMap; Open-source software; Scientific publications; Verified technical documentation. Review licenses, source quality, provenance and redistribution restrictions individually. Domain packages (general, science, programming, automotive, mathematics, languages, medicine) have manifests, dependencies, versions, source provenance, compressed indexes, compatibility and integrity. Install selectively.

## Hardware and research targets
Mandatory Dell Latitude 7490 i7-8650U, 4 cores/8 threads, 16 GB DDR4, NVMe, no dedicated GPU. Initial / long-term: quality >=95% of baseline on defined tasks / match or exceed; peak RAM and energy per correct task 2x / 10x lower; throughput 2x / 10x higher; neural computation 25% / >=90% reduction; active data movement long-term 90% reduction. Offline inference, no GPU, stores beyond RAM, incremental updates, automatic representation selection. Measure each capability separately; do not hide regressions behind aggregates.

## Baselines
- B01 Original ALICE/AIML
- B02 ChatScript
- B03 Expert system
- B04 Cognitive architecture
- B05 Conventional SLM
- B06 SLM + RAG
- B07 SLM + knowledge graph
- B08 Modern neuro-symbolic system
- B09 Quantized llama.cpp LLM
- B10 KILN neural inference
- B11 AIR without compilation
- B12 Complete AIR

- B13 Original pinned pretrained teacher
- B14 Conventional neural student / quantised teacher comparison
- B15 AIR behavioural-only conversion
- B16 AIR hybrid internal-analysis plus behavioural conversion

## Experiments
- E01 Neural weight entropy and redundancy
- E02 Neural behaviour extraction
- E03 Automatic abstraction discovery
- E04 Predictive state consolidation
- E05 Non-neural language prediction
- E06 Binary associative memory
- E07 Structural learning
- E08 Logic compilation
- E09 Automatic representation selection
- E10 CPU-cache locality
- E11 Heterogeneous representation integration
- E12 Generalisability preservation
- E13 Energy efficiency
- E14 Knowledge capacity scaling
- E15 Continuous consolidation
- E16 Modernised ALICE architecture
- E17 SLM category routing
- E18 Cross-category retrieval
- E19 Semantic deduplication
- E20 Automated knowledge curation
- E21 Teacher-to-structured-knowledge transfer
- E22 Cognitive memory separation
- E23 Reusable procedural memory
- E24 Resource-budgeted reasoning
- E25 Grammar-based response generation
- E26 Adaptive neural activation
- E27 SSD-backed knowledge execution
- E28 Multilingual semantic representation
- E29 Case-based reasoning reuse
- E30 Full-system comparative evaluation

- E31 Safe checkpoint inspection and weight redundancy
- E32 Internal feature and circuit extraction with causal intervention
- E33 Behavioural capability discovery and teacher transfer
- E34 Hybrid extraction versus behavioural-only conversion
- E35 Representation selection and held-out capability preservation
- E36 Residual neural fallback and safe replacement ablation
- E37 Conversion lifecycle cost, energy and break-even
- E38 Converted package portability, rollback and adversarial safety

Each experiment requires hypothesis/alternatives, baseline configuration, dataset and contamination controls, hardware/software, quality and generalisation, RAM/storage/compute/energy, training/curation/compilation overhead, reproducibility, success/failure thresholds, negative results and architectural implications.

## Preserved general-purpose implementation strategy (deferred under v0.6)
Parallel research/benchmarks, curation, format/storage/Graphmaster and semantic routing. First prototype integrates AIR-IR, curator, category store, Graphmaster, SLM router, symbolic execution and language realisation; compare SLM, RAG, ChatScript and neuro-symbolic baseline. Then pursue Genome, Compiler and Weightless independently; integrate only validated components. Finally scale, validate and deploy. Rust proposed; optional native C/C++ and KILN backends; avoid premature crate fragmentation.

AMT (P23) starts in parallel with the existing research, teacher, curation and compiler workstreams; preserve original checkpoints and release converted capabilities only after their independent quality gates.

## Failure clauses
If full weightless intelligence fails retain efficient specialised components; if general compilation fails retain selected compiled behaviours; if Genome fails at scale retain reusable libraries; if symbolic reasoning fails use neural fallback; if non-neural generation fails retain grammar fast paths; if full extraction fails retain curated knowledge; if SLM reasoning fails use selectively loaded larger modules; if tenfold gains fail report smaller measured gains; if frontier-level capability fails retain domain-specific utility. No deployment before gates.

## Preserved v0.5 status snapshot
Nine hypotheses, 24 phases, 38 core experiments; a multi-crate Rust prototype and preliminary real-model experiments exist. The prototype demonstrates selected retrieval, reasoning, memory, routing, packaging and measurement paths, not the roadmap's full architecture. No scientific acceptance gate or production-readiness gate is currently established as passed. Machine-readable ledgers are authoritative for gate evidence; completion is based on validated acceptance tests, not code presence or subjective percentages.

**Principle:** Do not store as weights what can be stored as knowledge; do not calculate what can be retrieved; do not repeatedly infer what can be compiled; do not load what is not needed; do not execute an expensive mechanism when a cheaper one is sufficient, subject to validated quality.
