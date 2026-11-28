//! CPU-native, dependency-free representation experiments for AIR.
//!
//! This crate is a standalone prototype. It does not load conventional model
//! weights and is not yet wired into air-core or the .airw archive. HDC and
//! ternary routing are candidates that require held-out quality and target
//! hardware measurements before they can be treated as useful AIR learning.

pub mod hdc;
pub mod ternary_snn;

pub use hdc::{HyperVector, HYPERVECTOR_BITS, HYPERVECTOR_WORDS};
pub use ternary_snn::{PackedTernaryRouter, RouteScore, Ternary};

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
        let scores = router.top_k(&input, 2).unwrap();
        assert_eq!(scores[0].output, 0);
        assert!(scores[0].fired);
    }
}
