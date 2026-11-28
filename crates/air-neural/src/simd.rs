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

    // Harley-Seal: each carry-save operation reduces three bit planes into a
    // sum plane (weight 1) and carry plane (weight 2). The population of the
    // planes is accumulated using a byte-nibble lookup.
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
    unsafe fn popcount_bytes(value: __m256i, lut: __m256i, low_mask: __m256i) -> __m256i {
        let low = _mm256_and_si256(value, low_mask);
        let high = _mm256_and_si256(_mm256_srli_epi16(value, 4), low_mask);
        let counts = _mm256_add_epi8(
            _mm256_shuffle_epi8(lut, low),
            _mm256_shuffle_epi8(lut, high),
        );
        _mm256_sad_epu8(counts, _mm256_setzero_si256())
    }

    let base = a.0.as_ptr().cast::<u8>();
    let other = b.0.as_ptr().cast::<u8>();
    let bytes = HYPERVECTOR_WORDS * std::mem::size_of::<u64>();
    let groups = bytes / 32;
    let mut total = 0u32;
    let lut = _mm256_setr_epi8(
        0, 1, 1, 2, 1, 2, 2, 3, 1, 2, 2, 3, 2, 3, 3, 4,
        0, 1, 1, 2, 1, 2, 2, 3, 1, 2, 2, 3, 2, 3, 3, 4,
    );
    let mask = _mm256_set1_epi8(0x0f);
    let zero = _mm256_setzero_si256();
    let mut ones = zero;
    let mut twos = zero;
    let mut fours = zero;
    let mut eights = zero;
    let mut sixteens = zero;
    let mut index = 0usize;

    // Process fifteen vectors per superblock. Remainders use the same SIMD
    // byte popcount without carry-save compression.
    while index + 15 <= groups {
        let mut block = [zero; 15];
        for slot in 0..15 {
            let offset = (index + slot) * 32;
            let av = _mm256_loadu_si256(base.add(offset).cast::<__m256i>());
            let bv = _mm256_loadu_si256(other.add(offset).cast::<__m256i>());
            block[slot] = _mm256_xor_si256(av, bv);
        }
        let (twos_a, ones_a) = csa(block[0], block[1], block[2]);
        let (twos_b, ones_b) = csa(block[3], block[4], block[5]);
        let (twos_c, ones_c) = csa(block[6], block[7], block[8]);
        let (twos_d, ones_d) = csa(block[9], block[10], block[11]);
        let (twos_e, ones_e) = csa(block[12], block[13], block[14]);

        let (fours_a, twos_a) = csa(twos_a, twos_b, twos_c);
        let (fours_b, twos_b) = csa(twos_d, twos_e, twos);
        let (eights_a, fours_a) = csa(fours_a, fours_b, fours);
        let (sixteens_a, eights_a) = csa(eights_a, eights);
        sixteens = _mm256_add_epi8(
            sixteens,
            _mm256_add_epi8(sixteens_a, _mm256_set1_epi8(1)),
        );
        eights = eights_a;
        fours = fours_a;
        twos = twos_a;
        ones = _mm256_add_epi8(
            ones,
            _mm256_add_epi8(
                _mm256_add_epi8(ones_a, ones_b),
                _mm256_add_epi8(_mm256_add_epi8(ones_c, ones_d), ones_e),
            ),
        );
        index += 15;
    }

    let mut sums = _mm256_setzero_si256();
    sums = _mm256_add_epi64(sums, popcount_bytes(ones, lut, mask));
    sums = _mm256_add_epi64(sums, _mm256_slli_epi64(popcount_bytes(twos, lut, mask), 1));
    sums = _mm256_add_epi64(sums, _mm256_slli_epi64(popcount_bytes(fours, lut, mask), 2));
    sums = _mm256_add_epi64(sums, _mm256_slli_epi64(popcount_bytes(eights, lut, mask), 3));
    sums = _mm256_add_epi64(sums, _mm256_slli_epi64(popcount_bytes(sixteens, lut, mask), 4));

    while index < groups {
        let offset = index * 32;
        let av = _mm256_loadu_si256(base.add(offset).cast::<__m256i>());
        let bv = _mm256_loadu_si256(other.add(offset).cast::<__m256i>());
        sums = _mm256_add_epi64(sums, popcount_bytes(_mm256_xor_si256(av, bv), lut, mask));
        index += 1;
    }

    let mut lanes = [0u64; 4];
    _mm256_storeu_si256(lanes.as_mut_ptr().cast::<__m256i>(), sums);
    total += lanes.iter().sum::<u64>() as u32;

    // 157 words leave eight bytes after the 39 complete AVX2 vectors.
    for byte_index in groups * 32..bytes {
        total += (base.add(byte_index).read() ^ other.add(byte_index).read()).count_ones();
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
