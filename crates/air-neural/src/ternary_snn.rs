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

    pub fn score(&self, input: &HyperVector) -> Vec<RouteScore> {
        let mut scores = Vec::with_capacity(self.outputs);
        for output in 0..self.outputs {
            let mut sum = 0i32;
            let base = output * self.input_bits;
            for input_index in 0..self.input_bits {
                let x = if input.bit(input_index) { 1 } else { -1 };
                match self.get_flat(base + input_index) {
                    Ternary::Zero => {}
                    Ternary::Positive => sum += x,
                    Ternary::Negative => sum -= x,
                }
            }
            scores.push(RouteScore { output, score: sum, fired: sum >= self.threshold });
        }
        scores
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
        assert_eq!(router.score(&input)[0].score, 1);
    }
    #[test] fn dimensions_and_indices_are_checked() {
        assert_eq!(PackedTernaryRouter::new(12, 2, 0), Err(RouterError::InvalidDimensions));
        let router = PackedTernaryRouter::new(HYPERVECTOR_BITS, 1, 0).unwrap();
        assert_eq!(router.weight(1, 0), Err(RouterError::OutputOutOfRange));
        assert_eq!(router.weight(0, HYPERVECTOR_BITS), Err(RouterError::InputOutOfRange));
    }
}
