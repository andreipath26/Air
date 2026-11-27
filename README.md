# AIR — Adaptive Intelligence Representation

**Researching intelligence that runs on the hardware people already own.**

AIR explores whether knowledge and learned capabilities can be curated, indexed, composed and executed in representations other than dense numerical weights. Inspired by A.L.I.C.E., cognitive architectures, modern semantic models and compiler engineering, AIR aims to combine selective retrieval, executable knowledge and optional neural reasoning on ordinary CPUs.

## Status
Early structured-knowledge prototype. No validated efficiency or intelligence claims. See the [master roadmap](ROADMAP.md), [architecture](docs/architecture/overview.md), [research ledger](research/research-ledger.json), [implementation ledger](research/implementation-ledger.json) and [benchmark protocol](docs/benchmark-methodology/protocol.md).

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
