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
