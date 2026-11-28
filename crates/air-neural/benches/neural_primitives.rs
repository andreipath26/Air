//! Run with: cargo bench -p air-neural --bench neural_primitives
//! Reports local release timings only; these are not hardware acceptance results.
use air_neural::{HyperVector, PackedTernaryRouter, Ternary, HYPERVECTOR_BITS};
use std::hint::black_box;
use std::time::Instant;

fn measure<T>(label: &str, iterations: usize, mut operation: impl FnMut() -> T) {
    let start = Instant::now();
    for _ in 0..iterations {
        black_box(operation());
    }
    let elapsed = start.elapsed();
    println!(
        "{label}: iterations={iterations}, total_us={}, mean_us={:.3}",
        elapsed.as_micros(),
        elapsed.as_secs_f64() * 1_000_000.0 / iterations as f64
    );
}

fn main() {
    let vector = HyperVector::from_seed(0x1234_5678);
    let batch: Vec<_> = (0..16).map(HyperVector::from_seed).collect();
    let mut router = PackedTernaryRouter::new(HYPERVECTOR_BITS, 8, 100).expect("dimensions");
    for output in 0..router.outputs() {
        for bit in (output..HYPERVECTOR_BITS).step_by(97) {
            let weight = match (bit + output) % 3 {
                0 => Ternary::Zero,
                1 => Ternary::Negative,
                _ => Ternary::Positive,
            };
            router.set_weight(output, bit, weight).expect("valid index");
        }
    }

    measure("hdc_permute", 1_000, || vector.permute(137));
    measure("hdc_bundle_16", 100, || HyperVector::bundle(black_box(&batch)));
    measure("ternary_score_8_outputs", 500, || router.score(&vector));
    measure("batch_score_sequential_16", 50, || router.score_batch(&batch));
    measure("batch_score_parallel_16_workers_4", 50, || {
        router.score_batch_parallel(&batch, 4).expect("workers")
    });
}
