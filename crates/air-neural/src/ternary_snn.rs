//! Packed two-bit ternary router with integer scoring and bounded discrete
//! feedback updates. This is a research primitive, not a trained production model.

use crate::hdc::{HyperVector, HYPERVECTOR_BITS};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Ternary { Zero, Negative, Positive }

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RouteScore {
    pub output: usize,
    pub score: i32,
    pub fired: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PackedTernaryRouter {
    input_bits: usize,
    outputs: usize,
    threshold: i32,
    // Four 2-bit parameters per byte: 00 zero, 01 negative, 10 positive.
    packed: Vec<u8>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RouterError {
    InvalidDimensions,
    InputDimensionMismatch,
    OutputOutOfRange,
    InputOutOfRange,
    InvalidFeedbackClass,
    ThreadSpawnFailed,
    WorkerPanicked,
}

impl PackedTernaryRouter {
    pub fn new(input_bits: usize, outputs: usize, threshold: i32) -> Result<Self, RouterError> {
        if input_bits == 0 || outputs == 0 || input_bits != HYPERVECTOR_BITS {
            return Err(RouterError::InvalidDimensions);
        }
        let parameter_count = input_bits.checked_mul(outputs).ok_or(RouterError::InvalidDimensions)?;
        Ok(Self { input_bits, outputs, threshold, packed: vec![0; parameter_count.div_ceil(4)] })
    }

    pub fn input_bits(&self) -> usize { self.input_bits }
    pub fn outputs(&self) -> usize { self.outputs }
    pub fn packed_bytes(&self) -> usize { self.packed.len() }

    pub fn weight(&self, output: usize, input: usize) -> Result<Ternary, RouterError> {
        let index = self.flat_index(output, input)?;
        Ok(self.get_flat(index))
    }

    pub fn set_weight(&mut self, output: usize, input: usize, weight: Ternary) -> Result<(), RouterError> {
        let index = self.flat_index(output, input)?;
        self.set_flat(index, weight);
        Ok(())
    }

    /// Score all outputs using packed 64-feature blocks and population counts.
    /// No allocation occurs inside the per-feature hot loop.
    pub fn score(&self, input: &HyperVector) -> Vec<RouteScore> {
        let mut scores = Vec::with_capacity(self.outputs);
        for output in 0..self.outputs {
            let mut sum = 0i32;
            let base = output * self.input_bits;
            for (word_index, input_word) in input.0.iter().copied().enumerate() {
                let feature_start = word_index * 64;
                if feature_start >= self.input_bits { break; }
                let feature_count = (self.input_bits - feature_start).min(64);
                let packed_start = (base + feature_start) / 4;
                let byte_count = feature_count / 4;
                let mut weights = 0u128;
                if byte_count == 16 {
                    let bytes: [u8; 16] = self.packed[packed_start..packed_start + 16]
                        .try_into().expect("packed 64-feature block");
                    weights = u128::from_le_bytes(bytes);
                } else {
                    for offset in 0..byte_count {
                        weights |= u128::from(self.packed[packed_start + offset]) << (offset * 8);
                    }
                }
                let used_mask = if feature_count == 64 {
                    u128::MAX
                } else {
                    (1u128 << (feature_count * 2)) - 1
                };
                let lane_mask = 0x5555_5555_5555_5555_5555_5555_5555_5555u128 & used_mask;
                let positive = (weights >> 1) & lane_mask;
                let negative = weights & lane_mask;
                let x = spread_to_even_bits(input_word) & used_mask;
                let matches = (positive & x).count_ones()
                    + (negative & (!x & lane_mask)).count_ones();
                let active = (positive | negative).count_ones();
                sum += (2 * matches as i32) - active as i32;
            }
            scores.push(RouteScore { output, score: sum, fired: sum >= self.threshold });
        }
        scores
    }

    /// Sequential batch scoring. Results preserve input order.
    pub fn score_batch(&self, inputs: &[HyperVector]) -> Vec<Vec<RouteScore>> {
        inputs.iter().map(|input| self.score(input)).collect()
    }

    /// Explicit bounded parallel scoring for independent requests. Small or
    /// single-item batches stay sequential to avoid thread startup overhead.
    pub fn score_batch_parallel(
        &self,
        inputs: &[HyperVector],
        max_workers: usize,
    ) -> Result<Vec<Vec<RouteScore>>, RouterError> {
        if max_workers == 0 { return Err(RouterError::InvalidDimensions); }
        if inputs.is_empty() { return Ok(Vec::new()); }
        let workers = max_workers.min(inputs.len());
        if workers == 1 { return Ok(self.score_batch(inputs)); }
        let chunk_size = inputs.len().div_ceil(workers);
        std::thread::scope(|scope| {
            let mut handles = Vec::with_capacity(workers);
            for chunk in inputs.chunks(chunk_size) {
                let handle = std::thread::Builder::new()
                    .spawn_scoped(scope, move || chunk.iter().map(|input| self.score(input)).collect::<Vec<_>>())
                    .map_err(|_| RouterError::ThreadSpawnFailed)?;
                handles.push(handle);
            }
            let mut results = Vec::with_capacity(inputs.len());
            for handle in handles {
                let chunk = handle.join().map_err(|_| RouterError::WorkerPanicked)?;
                results.extend(chunk);
            }
            Ok(results)
        })
    }

    pub fn top_k(&self, input: &HyperVector, k: usize) -> Vec<RouteScore> {
        let mut scores = self.score(input);
        scores.sort_by(|a, b| b.score.cmp(&a.score).then_with(|| a.output.cmp(&b.output)));
        scores.truncate(k.min(scores.len()));
        scores
    }

    /// Single-example reward update: strengthen the expected output and weaken
    /// the incorrect predicted output. Each parameter remains ternary.
    pub fn update_from_feedback(
        &mut self,
        input: &HyperVector,
        expected: usize,
        predicted: usize,
    ) -> Result<(), RouterError> {
        if expected >= self.outputs || predicted >= self.outputs {
            return Err(RouterError::InvalidFeedbackClass);
        }
        if expected == predicted { return Ok(()); }
        for input_index in 0..self.input_bits {
            let x = if input.bit(input_index) { 1 } else { -1 };
            self.adjust(expected, input_index, x)?;
            self.adjust(predicted, input_index, -x)?;
        }
        Ok(())
    }

    fn adjust(&mut self, output: usize, input: usize, delta: i8) -> Result<(), RouterError> {
        let current = self.weight(output, input)?;
        let next = match (current, delta) {
            (Ternary::Zero, 1) | (Ternary::Negative, 1) => Ternary::Positive,
            (Ternary::Positive, -1) | (Ternary::Zero, -1) => Ternary::Negative,
            _ => current,
        };
        self.set_weight(output, input, next)
    }

    fn flat_index(&self, output: usize, input: usize) -> Result<usize, RouterError> {
        if output >= self.outputs { return Err(RouterError::OutputOutOfRange); }
        if input >= self.input_bits { return Err(RouterError::InputOutOfRange); }
        Ok(output * self.input_bits + input)
    }

    fn get_flat(&self, index: usize) -> Ternary {
        let shift = (index % 4) * 2;
        match (self.packed[index / 4] >> shift) & 0b11 {
            0b01 => Ternary::Negative,
            0b10 => Ternary::Positive,
            _ => Ternary::Zero,
        }
    }

    fn set_flat(&mut self, index: usize, value: Ternary) {
        let shift = (index % 4) * 2;
        let mask = 0b11 << shift;
        let encoded = match value { Ternary::Zero => 0b00, Ternary::Negative => 0b01, Ternary::Positive => 0b10 };
        self.packed[index / 4] = (self.packed[index / 4] & !mask) | (encoded << shift);
    }
}

/// Spread each source bit into the even positions of a u128 lane mask.
fn spread_to_even_bits(value: u64) -> u128 {
    let mut x = u128::from(value);
    x = (x | (x << 32)) & 0x00000000FFFFFFFF00000000FFFFFFFFu128;
    x = (x | (x << 16)) & 0x0000FFFF0000FFFF0000FFFF0000FFFFu128;
    x = (x | (x << 8)) & 0x00FF00FF00FF00FF00FF00FF00FF00FFu128;
    x = (x | (x << 4)) & 0x0F0F0F0F0F0F0F0F0F0F0F0F0F0F0F0Fu128;
    x = (x | (x << 2)) & 0x33333333333333333333333333333333u128;
    (x | (x << 1)) & 0x55555555555555555555555555555555u128
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test] fn packs_and_unpacks_ternary_parameters() {
        let mut router = PackedTernaryRouter::new(HYPERVECTOR_BITS, 1, 0).unwrap();
        router.set_weight(0, 0, Ternary::Negative).unwrap();
        router.set_weight(0, 1, Ternary::Zero).unwrap();
        router.set_weight(0, 2, Ternary::Positive).unwrap();
        assert_eq!(router.weight(0, 0), Ok(Ternary::Negative));
        assert_eq!(router.weight(0, 1), Ok(Ternary::Zero));
        assert_eq!(router.weight(0, 2), Ok(Ternary::Positive));
        assert_eq!(router.packed_bytes(), HYPERVECTOR_BITS.div_ceil(4));
    }
    #[test] fn score_and_spike_threshold_are_integer_only() {
        let mut input = HyperVector::zero();
        input.set_bit(0, true);
        let mut router = PackedTernaryRouter::new(HYPERVECTOR_BITS, 2, 1).unwrap();
        router.set_weight(0, 0, Ternary::Positive).unwrap();
        router.set_weight(1, 0, Ternary::Negative).unwrap();
        let scores = router.score(&input);
        assert_eq!(scores[0].score, 1);
        assert!(scores[0].fired);
        assert_eq!(scores[1].score, -1);
        assert!(!scores[1].fired);
    }
    #[test] fn top_k_is_ranked_and_deterministic_on_ties() {
        let router = PackedTernaryRouter::new(HYPERVECTOR_BITS, 3, 0).unwrap();
        let ranked = router.top_k(&HyperVector::zero(), 2);
        assert_eq!(ranked.iter().map(|s| s.output).collect::<Vec<_>>(), vec![0, 1]);
    }
    #[test] fn feedback_update_remains_ternary_and_changes_scores() {
        let mut input = HyperVector::zero();
        input.set_bit(4, true);
        let mut router = PackedTernaryRouter::new(HYPERVECTOR_BITS, 2, 1).unwrap();
        router.update_from_feedback(&input, 0, 1).unwrap();
        assert_eq!(router.weight(0, 4), Ok(Ternary::Positive));
        assert_eq!(router.weight(1, 4), Ok(Ternary::Negative));
        assert_eq!(router.score(&input)[0].score, HYPERVECTOR_BITS as i32, "feedback updates all 10,000 features");
    }

    #[test] fn packed_scores_match_reference_bitwise_dot() {
        let mut router = PackedTernaryRouter::new(HYPERVECTOR_BITS, 2, 3).unwrap();
        let input = HyperVector::from_seed(123);
        for bit in 0..HYPERVECTOR_BITS {
            let weight = match (bit * 17 + bit / 7) % 3 {
                0 => Ternary::Zero,
                1 => Ternary::Negative,
                _ => Ternary::Positive,
            };
            router.set_weight(bit % 2, bit, weight).unwrap();
        }
        let got = router.score(&input);
        for output in 0..2 {
            let mut expected = 0i32;
            for bit in 0..HYPERVECTOR_BITS {
                let x = if input.bit(bit) { 1 } else { -1 };
                expected += match router.weight(output, bit).unwrap() {
                    Ternary::Zero => 0,
                    Ternary::Positive => x,
                    Ternary::Negative => -x,
                };
            }
            assert_eq!(got[output].score, expected);
        }
    }

    #[test] fn bounded_parallel_batch_matches_sequential_order() {
        let inputs: Vec<_> = (0..8).map(|seed| HyperVector::from_seed(seed)).collect();
        let router = PackedTernaryRouter::new(HYPERVECTOR_BITS, 4, 10).unwrap();
        let sequential = router.score_batch(&inputs);
        let parallel = router.score_batch_parallel(&inputs, 3).unwrap();
        assert_eq!(parallel, sequential);
        assert!(router.score_batch_parallel(&[], 2).unwrap().is_empty());
        assert_eq!(router.score_batch_parallel(&inputs, 0), Err(RouterError::InvalidDimensions));
    }

    #[test] fn dimensions_and_indices_are_checked() {
        assert_eq!(PackedTernaryRouter::new(12, 2, 0), Err(RouterError::InvalidDimensions));
        let router = PackedTernaryRouter::new(HYPERVECTOR_BITS, 1, 0).unwrap();
        assert_eq!(router.weight(1, 0), Err(RouterError::OutputOutOfRange));
        assert_eq!(router.weight(0, HYPERVECTOR_BITS), Err(RouterError::InputOutOfRange));
    }
}
