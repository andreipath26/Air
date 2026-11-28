# AIR — Adaptive Intelligence Representation

**Researching intelligence that runs on the hardware people already own.**

AIR explores whether knowledge and learned capabilities can be curated, indexed, composed and executed in representations other than dense numerical weights. Inspired by A.L.I.C.E., cognitive architectures, modern semantic models and compiler engineering, AIR aims to combine selective retrieval, executable knowledge and optional neural reasoning on ordinary CPUs.

## Status
Early structured-knowledge prototype. No validated efficiency or intelligence claims. See the [master roadmap](ROADMAP.md), [architecture](docs/architecture/overview.md), [research ledger](research/research-ledger.json), [implementation ledger](research/implementation-ledger.json) and [benchmark protocol](docs/benchmark-methodology/protocol.md). See the [AIR evidence index](docs/research/air-evidence-index.md) for exact benchmark, workflow, Actions artifact, and external-reference locations.

## Development
Requires stable Rust and Cargo. No runtime dependencies in the bootstrap.

```sh
cargo test --workspace
cargo run -p air-cli -- doctor
cargo run --release -p air-bench -- 100000
```

The synthetic harness and retrieval benchmark are not AI benchmarks. AIR is independent of KILN; future integration is optional. Target: Dell Latitude 7490, i7-8650U, 16 GB RAM, NVMe, CPU-only offline inference.

## Research priorities
Information curation, overlapping categories, modern Graphmaster, small semantic router, cognitive memory, executable knowledge, intelligence compiler and adaptive inference. Compare against strong historical, neural and hybrid baselines. Publish negative findings as well as successes.

License selection pending owner decision; public visibility alone does not grant a reuse license.

## Indexed prototype
```sh
cargo run -p air-cli -- index-import knowledge/examples/demo.tsv demo.db
cargo run -p air-cli -- ask demo.db "What is the capital of France?"
cargo run -p air-cli -- ask demo.db "calculate 583 * 927"
cargo run --release -p air-bench -- retrieval 1000 benchmark.db
```
SQLite is a provisional indexed reference backend; Rust bindings require a C compiler for the bundled SQLite build. See [prototype limitations](docs/architecture/prototype-02.md). The semantic router currently exposes a typed interface and supplied JSON fallback, not an embedded model.

## Optional resident local SLM
Install llama.cpp separately and obtain an appropriately licensed instruction GGUF. Start a CPU-only loopback server:

```sh
llama-server -m /path/model.gguf -ngl 0 -c 4096 --host 127.0.0.1 --port 8080
export AIR_MODEL_PORT=8080 AIR_MODEL_NAME=local
cargo run -p air-cli -- ask demo.db "Which city serves as France's capital?"
cargo run -p air-cli -- curate-text document.txt extracted.db source-id
bash scripts/benchmark-local.sh 10000
```

Deterministic fast paths run before the model. Extracted claims are explicitly unverified; inspect source quotes. See [prototype 03](docs/architecture/prototype-03.md) for constraints. Actual model quality and hardware results remain unmeasured.

## Cognitive and procedural prototype
```sh
cargo run -p air-cli -- remember memory.db "diesel engine" "inspect air intake"
cargo run -p air-cli -- recall memory.db diesel
cargo run -p air-cli -- learn-skill memory.db multiply knowledge/examples/multiply.json
cargo run -p air-cli -- run-skill memory.db multiply 11 13
AIR_LANGUAGE=fr cargo run -p air-cli -- ask demo.db "What is the capital of France?"
```
Restricted integer program induction and compilation are experiments, not general-purpose learned intelligence. See [prototype 04](docs/architecture/prototype-04.md).

## Integrated inference session
```sh
cargo run -p air-cli -- alias policy.db relation "capital city" capital
cargo run -p air-cli -- session demo.db memory.db policy.db "What is the capital city of France?"
cargo run -p air-cli -- synthesise memory.db compound knowledge/examples/compound.json
cargo run -p air-cli -- approve-skill policy.db memory.db compound 1
cargo run -p air-cli -- session demo.db memory.db policy.db "skill compound 8 10"
```
Operator reviews and skill approvals are separate from untrusted model output. See [prototype 05](docs/architecture/prototype-05.md).

## Knowledge packages and explicit reasoning
`package-import DB POLICY MANIFEST DATA.tsv` verifies a SHA-256 manifest and atomically consolidates reviewed aliases. `probability MODEL.json` evaluates an explicit finite Bayesian model. Integrated sessions support `explain causal A -> B` for operator-reviewed causal paths. See [prototype 06](docs/architecture/prototype-06.md).

## Bounded plans and additional representations
`plan DB MEMORY POLICY PLAN.json` executes explicit steps through the coordinator; `evaluate DB MEMORY POLICY CASES.json` records fixture accuracy, latency and model calls. `compact-put` / `compact-get` provide random-access compressed blocks; `temporal-record` / `temporal-at` preserve time-bounded claims. See [prototype 07](docs/architecture/prototype-07.md).

## Resident runtime and typed composition
`air-cli serve DB MEMORY POLICY` accepts one JSON request per line on stdin and returns one JSON response per line. Example: `{"kind":"ask","question":"calculate 5 * 7"}`. `dataflow DB MEMORY POLICY PROGRAM.json` composes typed intermediate results. See [prototype 08](docs/architecture/prototype-08.md).

## Learned routing and reversible objects
`teach-route POLICY CATEGORY PHRASE` adds operator-provided category examples. Integrated sessions may return a clearly marked category suggestion when other routing abstains. `observe` / `predict` record and retrieve transition counts. `revision-put`, `revision-get`, and `revision-rollback` operate on content-addressed objects. See [prototype 09](docs/architecture/prototype-09.md).

## Planned: AIR Model Transcompiler (AMT)
Reuse existing pretrained models by extracting selected capabilities through behavioural distillation and experimental weight/activation analysis, compiling them into AIR-native representations and retaining neural fallback where needed. No full-model inversion or equivalence is claimed. See [architecture and validation strategy](docs/architecture/model-transcompiler.md) and Phase 23 of the [roadmap](ROADMAP.md).

## AMT pilot
`amt-inspect model.safetensors` inventories a checkpoint and computes its SHA-256. With a CPU-only local model server running, edit the teacher identification and license in `knowledge/examples/amt-pilot.json`, then run `bash scripts/amt-pilot.sh SPEC.json NEW_REPORT.json`. `amt-import REPORT.json MEMORY` imports an unapproved candidate into procedural memory. See [pilot limitations](docs/architecture/amt-pilot.md).

## AMT real-model experiment
The [AMT experiment workflow](.github/workflows/amt-experiment.yml) runs a small CPU-only pretrained teacher and produces weight inventories, numerical statistics, procedure, fact and classification reports. See [pilot 02](docs/architecture/amt-pilot-02.md). Results must be inspected before claiming capability transfer.

## Capability packages and runtime safety
`amt-package REPORT SHA256 NEW_PACKAGE` produces a bounded portable procedure artifact; `amt-package-run PACKAGE ARGS...` explicitly executes the experimental artifact. Integrated sessions require separate content-bound approval; legacy approvals must be renewed. `revoke-skill POLICY NAME VERSION` revokes approval. Resident `{"kind":"metrics"}` returns request counters. See [prototype 10](docs/architecture/prototype-10.md).

## Parallel validation
CI uses six independent validation lanes. See [parallel validation](docs/architecture/parallel-validation.md). The real-model AMT experiment runs separately.

## Post-pilot priorities (v0.4)
See [roadmap](ROADMAP.md) and [post-pilot execution plan](docs/architecture/post-pilot-priorities.md). The next experiment compares two teacher sizes using conservative plain-answer extraction, raw traces and exploratory structural ablations. All six development lanes and prior gates remain active.

## Portable classifier candidates
The AMT pilot compares weightless and lexical baselines. Use classifier-package REPORT SHA256 NEW_PACKAGE to export an experimental candidate and classifier-run PACKAGE TEXT to test it offline. Explicit classifier-install POLICY PACKAGE SHA256 requires preliminary quality and coverage thresholds. Use select-representation CANDIDATES CRITERIA for quality-first validation selection. See [prototype 11](docs/architecture/prototype-11.md).

## Capability discovery and optional hybrid execution
Use amt-discover SPEC REPORT to record bounded capability probes. AIR_PREDICTIVE=1 enables experimental resident category page warming. AIR_RESIDUAL=1 enables clearly labelled unverified local neural fallback. Both are disabled by default. See [prototype 12](docs/architecture/prototype-12.md).

## Evidence-driven acceptance
See [G01 factual capability transfer](docs/architecture/evidence-gate-01.md) and [gate policy](research/evidence-gate-01.json). Run python3 scripts/evidence-gate.py DATASET.json EVIDENCE.json to evaluate submitted numerical evidence. Independent review is mandatory; a numerical pass is not certification.

## G01 execution
See [benchmark execution](docs/architecture/g01-execution.md). The factual-serve DB POLICY ROUTER.json command provides bounded, reviewed factual answers. scripts/g01-benchmark.py supports resident AIR and a hash-pinned CPU-only llama.cpp server. Included smoke data is not independent gate evidence; observations are gate-ineligible until complete resource measurements and review exist.

## G01 corpus and measurements
Use scripts/g01-corpus.py CORPUS OUTPUT --mode reference or --mode teacher with a pinned declared loopback teacher. Run the three benchmark systems independently. scripts/g01-assemble.py joins their observations with separately supplied whole-system measurements; CPU package RAPL and process RSS are diagnostics only. See [corpus and measurement pipeline](docs/architecture/g01-corpus-and-measurement.md).

## One-command G01 runner
See [runner protocol](docs/architecture/g01-runner.md). scripts/g01-development.py generates 240 synthetic integration cases; scripts/g01-run.py orchestrates conversion and the three systems with pinned local teacher files. The synthetic workload is not acceptance evidence.

## Real-fact development workload
See [draft corpus](knowledge/development/README.md). The AMT workflow now runs 200 real-fact conversions and 240 evaluation questions per teacher. This is not independent G01 evidence; the teacher is FP32 and resident during AIR diagnostic runs. Structural interventions include nearest-disjoint-magnitude controls with actual removed activation norms; matching is not guaranteed.

## Consolidated G01 protocol
See [consolidated G01](docs/architecture/g01-consolidated.md). Scientific symbols are case-sensitive, teacher response status is explicit, and a conservative lexical fallback handles reordered phrasing. The manually dispatched G01 quantized workflow requires pinned model and executable source identities. The review template is intentionally incomplete; independent review is required.

## G01 blocker fixes
Context-aware extraction and multi-teacher trace reconciliation are available. Indexed knowledge schema v2 preserves case for notation relations; rebuild v1 databases and repeat claim reviews from source. See [migration and limitations](docs/architecture/g01-blocker-fixes.md).

## Automated teacher reconciliation and quantized experiment
The AMT matrix now has a dependent reconciliation job that checks corpus and teacher identities, builds the reference-validated union and benchmarks it. The quantized workflow resolves and records official upstream commit and LFS hashes on its development push trigger, verifies downloaded bytes, and supports explicit pins for reproduction. See docs/architecture/g01-automation.md. Neither workflow certifies G01.
