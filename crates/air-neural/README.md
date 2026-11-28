# air-neural prototype

Dependency-free CPU primitives for AIR experiments.

- `hdc::HyperVector` stores 10,000 logical bits in 157 `u64` words and provides deterministic item vectors, XOR binding, circular permutation, bundling, Hamming distance, and text encoding.
- `PackedTernaryRouter` stores four `{-1, 0, +1}` parameters per byte, scores 64-feature blocks with integer population counts, and offers caller-bounded batch parallelism.
- Tests compare optimized scoring/permutation with scalar references. Run the microbenchmark in release mode with `cargo bench -p air-neural --bench neural_primitives`.

This remains an experimental crate. It is not integrated with `air-core` or a compiled `.airw` archive, and the benchmark harness does not establish Dell Latitude 7490 performance or generalization quality.
