//! CPU-native hyperdimensional routing primitives and packed ternary representations.
//!
//! The AVX2 Hamming path is runtime-dispatched and retains a portable scalar
//! fallback. Ternary and sparse BITCOS scoring use integer arithmetic.

pub mod hdc;
pub mod simd;
pub mod ternary_snn;
pub mod ambiguity;

pub use hdc::{HyperVector, HYPERVECTOR_BITS, HYPERVECTOR_WORDS};
pub use ambiguity::{
    assess_route, has_explicit_trigger, normalized_pair_entropy, AmbiguityAssessment,
    AmbiguityError, AmbiguityThresholds, RoutingAction,
};
pub use simd::{hamming_distance, hamming_distance_scalar};
pub use ternary_snn::{PackedTernaryRouter, RouteScore, Ternary};
pub mod bitcos;

#[cfg(test)]
mod integration_tests {
    use super::*;

    #[test]
    fn hypervector_can_route_through_packed_ternary_weights() {
        let input = HyperVector::item("rust borrow repair");
        let mut router = PackedTernaryRouter::new(HYPERVECTOR_BITS, 2, 1).unwrap();
        for bit in 0..HYPERVECTOR_BITS {
            router.set_weight(0, bit, if input.bit(bit) { Ternary::Positive } else { Ternary::Negative }).unwrap();
            router.set_weight(1, bit, Ternary::Zero).unwrap();
        }
        let scores = router.top_k(&input, 2);
        assert_eq!(scores[0].output, 0);
        assert!(scores[0].fired);
    }
}
