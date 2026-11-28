//! Runtime-dispatched Hamming distance for fixed-width HDC vectors.
//!
//! The AVX2 path uses Harley-Seal carry-save accumulation followed by nibble
//! lookup popcount. It never requires AVX-512 VPOPCNTDQ and the scalar path is
//! available on every supported target.

use crate::hdc::{HyperVector, HYPERVECTOR_WORDS};

/// Hamming distance over the logical 10,000-bit hypervectors.
///
/// AVX2 is selected at runtime when the current CPU supports it. The final
/// padded bits are masked by the HyperVector invariant and are ignored anyway.
#[inline]
pub fn hamming_distance(a: &HyperVector, b: &HyperVector) -> u32 {
    #[cfg(target_arch = "x86_64")]
    {
        if std::is_x86_feature_detected!("avx2") {
            // SAFETY: the runtime feature check above guarantees AVX2 support.
            return unsafe { hamming_distance_avx2(a, b) };
        }
    }
    hamming_distance_scalar(a, b)
}

/// Portable reference implementation used for fallback and parity testing.
#[inline]
pub fn hamming_distance_scalar(a: &HyperVector, b: &HyperVector) -> u32 {
    a.0.iter().zip(b.0.iter()).map(|(x, y)| (x ^ y).count_ones()).sum()
}

#[cfg(target_arch = "x86_64")]
#[target_feature(enable = "avx2")]
unsafe fn hamming_distance_avx2(a: &HyperVector, b: &HyperVector) -> u32 {
    use std::arch::x86_64::*;

    #[inline]
    unsafe fn csa(a: __m256i, b: __m256i, c: __m256i) -> (__m256i, __m256i) {
        let u = _mm256_xor_si256(a, b);
        let ones = _mm256_xor_si256(u, c);
        let twos = _mm256_or_si256(
            _mm256_and_si256(a, b),
            _mm256_and_si256(u, c),
        );
        (twos, ones)
    }

    #[inline]
    unsafe fn popcount_lanes(value: __m256i, lut: __m256i, low_mask: __m256i) -> __m256i {
        let low = _mm256_and_si256(value, low_mask);
        let high = _mm256_and_si256(_mm256_srli_epi16(value, 4), low_mask);
        let counts = _mm256_add_epi8(
            _mm256_shuffle_epi8(lut, low),
            _mm256_shuffle_epi8(lut, high),
        );
        _mm256_sad_epu8(counts, _mm256_setzero_si256())
    }

    let left = a.0.as_ptr().cast::<u8>();
    let right = b.0.as_ptr().cast::<u8>();
    let vector_count = (HYPERVECTOR_WORDS * std::mem::size_of::<u64>()) / 32;
    let triplets = vector_count / 3;
    let lut = _mm256_setr_epi8(
        0, 1, 1, 2, 1, 2, 2, 3, 1, 2, 2, 3, 2, 3, 3, 4,
        0, 1, 1, 2, 1, 2, 2, 3, 1, 2, 2, 3, 2, 3, 3, 4,
    );
    let nibble_mask = _mm256_set1_epi8(0x0f);
    let zero = _mm256_setzero_si256();
    let mut total_lanes = zero;

    // Carry-save reduction: each three input vectors become a sum plane and a
    // carry plane. Popcounting those two planes requires fewer lookup passes
    // than popcounting all three source vectors independently.
    for group in 0..triplets {
        let offset = group * 96;
        let x0 = _mm256_xor_si256(
            _mm256_loadu_si256(left.add(offset).cast::<__m256i>()),
            _mm256_loadu_si256(right.add(offset).cast::<__m256i>()),
        );
        let x1 = _mm256_xor_si256(
            _mm256_loadu_si256(left.add(offset + 32).cast::<__m256i>()),
            _mm256_loadu_si256(right.add(offset + 32).cast::<__m256i>()),
        );
        let x2 = _mm256_xor_si256(
            _mm256_loadu_si256(left.add(offset + 64).cast::<__m256i>()),
            _mm256_loadu_si256(right.add(offset + 64).cast::<__m256i>()),
        );
        let (twos, ones) = csa(x0, x1, x2);
        total_lanes = _mm256_add_epi64(
            total_lanes,
            popcount_lanes(ones, lut, nibble_mask),
        );
        total_lanes = _mm256_add_epi64(
            total_lanes,
            _mm256_slli_epi64(popcount_lanes(twos, lut, nibble_mask), 1),
        );
    }

    // Remaining complete vectors are counted directly.
    for vector in triplets * 3..vector_count {
        let offset = vector * 32;
        let diff = _mm256_xor_si256(
            _mm256_loadu_si256(left.add(offset).cast::<__m256i>()),
            _mm256_loadu_si256(right.add(offset).cast::<__m256i>()),
        );
        total_lanes = _mm256_add_epi64(
            total_lanes,
            popcount_lanes(diff, lut, nibble_mask),
        );
    }

    let mut lanes = [0u64; 4];
    _mm256_storeu_si256(lanes.as_mut_ptr().cast::<__m256i>(), total_lanes);
    let mut total = lanes.iter().sum::<u64>() as u32;
    let total_bytes = HYPERVECTOR_WORDS * std::mem::size_of::<u64>();
    for byte_index in vector_count * 32..total_bytes {
        total += (left.add(byte_index).read() ^ right.add(byte_index).read()).count_ones();
    }
    total
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Instant;

    #[test]
    fn scalar_matches_existing_hamming_distance() {
        for seed in 0..64 {
            let a = HyperVector::from_seed(seed);
            let b = HyperVector::from_seed(seed.wrapping_mul(7919));
            assert_eq!(hamming_distance_scalar(&a, &b), a.hamming_distance(&b));
        }
    }

    #[test]
    fn dispatched_kernel_matches_scalar_for_deterministic_vectors() {
        for seed in 0..64u64 {
            let a = HyperVector::from_seed(seed);
            let b = HyperVector::from_seed(seed.wrapping_mul(0x9e37_79b9));
            assert_eq!(hamming_distance(&a, &b), hamming_distance_scalar(&a, &b));
        }
        let vector = HyperVector::from_seed(17);
        assert_eq!(hamming_distance(&vector, &vector), 0);
    }

    #[cfg(target_arch = "x86_64")]
    #[test]
    fn avx2_kernel_matches_scalar_when_available() {
        if std::is_x86_feature_detected!("avx2") {
            for seed in 0..32u64 {
                let a = HyperVector::from_seed(seed);
                let b = HyperVector::from_seed(!seed);
                // SAFETY: guarded by the AVX2 feature detection.
                let actual = unsafe { hamming_distance_avx2(&a, &b) };
                assert_eq!(actual, hamming_distance_scalar(&a, &b));
            }
        }
    }

    #[test]
    fn reports_hamming_latency_without_flaky_threshold() {
        let a = HyperVector::from_seed(11);
        let b = HyperVector::from_seed(29);
        let start = Instant::now();
        let mut checksum = 0u32;
        for _ in 0..10_000 {
            checksum = checksum.wrapping_add(hamming_distance(&a, &b));
        }
        let elapsed = start.elapsed();
        assert_ne!(checksum, 0);
        eprintln!(
            "hdc_hamming_10000: iterations=10000 total_us={} mean_ns={:.1}",
            elapsed.as_micros(),
            elapsed.as_nanos() as f64 / 10_000.0
        );
    }
}
