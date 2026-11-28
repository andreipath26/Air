//! Packed binary hypervectors. Operations are deterministic and use only
//! integer/bitwise arithmetic; no neural model or floating-point MAC is used.

pub const HYPERVECTOR_BITS: usize = 10_000;
pub const HYPERVECTOR_WORDS: usize = HYPERVECTOR_BITS.div_ceil(64);
const LAST_WORD_BITS: usize = HYPERVECTOR_BITS % 64;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct HyperVector(pub [u64; HYPERVECTOR_WORDS]);

impl Default for HyperVector {
    fn default() -> Self { Self::zero() }
}

impl HyperVector {
    pub const fn zero() -> Self { Self([0; HYPERVECTOR_WORDS]) }

    /// Deterministic binary item memory entry for a token or canonical concept.
    /// This is a stable hashing baseline, not a learned semantic embedding.
    pub fn item(token: &str) -> Self {
        let mut hash = 0xcbf29ce484222325u64;
        for byte in token.as_bytes() {
            hash ^= u64::from(*byte);
            hash = hash.wrapping_mul(0x100000001b3);
        }
        Self::from_seed(hash)
    }

    /// Create a reproducible pseudo-random hypervector from a seed.
    pub fn from_seed(seed: u64) -> Self {
        let mut state = seed;
        let mut words = [0u64; HYPERVECTOR_WORDS];
        for word in &mut words {
            state = state.wrapping_add(0x9e3779b97f4a7c15);
            let mut z = state;
            z = (z ^ (z >> 30)).wrapping_mul(0xbf58476d1ce4e5b9);
            z = (z ^ (z >> 27)).wrapping_mul(0x94d049bb133111eb);
            *word = z ^ (z >> 31);
        }
        let mut result = Self(words);
        result.mask_padding();
        result
    }

    pub fn bit(&self, index: usize) -> bool {
        assert!(index < HYPERVECTOR_BITS, "hypervector bit index out of range");
        (self.0[index / 64] & (1u64 << (index % 64))) != 0
    }

    pub fn set_bit(&mut self, index: usize, value: bool) {
        assert!(index < HYPERVECTOR_BITS, "hypervector bit index out of range");
        let mask = 1u64 << (index % 64);
        if value { self.0[index / 64] |= mask; } else { self.0[index / 64] &= !mask; }
    }

    /// Bind two vectors with XOR.
    pub fn bind(&self, other: &Self) -> Self {
        let mut out = [0u64; HYPERVECTOR_WORDS];
        for (dst, (a, b)) in out.iter_mut().zip(self.0.iter().zip(other.0.iter())) {
            *dst = a ^ b;
        }
        let mut result = Self(out);
        result.mask_padding();
        result
    }

    /// Circularly permute over the 10,000 logical dimensions (not padded bits).
    pub fn permute(&self, shift: usize) -> Self {
        let shift = shift % HYPERVECTOR_BITS;
        if shift == 0 { return *self; }
        let mut out = Self::zero();
        for word_index in 0..HYPERVECTOR_WORDS {
            let destination = word_index * 64;
            let count = (HYPERVECTOR_BITS - destination).min(64);
            let source = (destination + HYPERVECTOR_BITS - shift) % HYPERVECTOR_BITS;
            out.0[word_index] = self.read_circular(source, count);
        }
        out.mask_padding();
        out
    }

    fn read_circular(&self, start: usize, count: usize) -> u64 {
        let first_count = count.min(HYPERVECTOR_BITS - start);
        let mut value = self.read_linear(start, first_count);
        if first_count < count {
            value |= self.read_linear(0, count - first_count) << first_count;
        }
        value
    }

    fn read_linear(&self, start: usize, count: usize) -> u64 {
        let word_index = start / 64;
        let offset = start % 64;
        let mut value = self.0[word_index] >> offset;
        if offset != 0 && word_index + 1 < HYPERVECTOR_WORDS {
            value |= self.0[word_index + 1] << (64 - offset);
        }
        if count < 64 {
            value &= (1u64 << count) - 1;
        }
        value
    }

    /// Majority bundle. Ties deterministically resolve to zero; empty input is zero.
    pub fn bundle(vectors: &[Self]) -> Self {
        if vectors.is_empty() { return Self::zero(); }
        // Fixed-size scratch keeps memory bounded and avoids one heap allocation
        // for every bundle operation. Count only set bits in each packed word.
        let mut counts = [0usize; HYPERVECTOR_BITS];
        for vector in vectors {
            for (word_index, word) in vector.0.iter().enumerate() {
                let mut bits = *word;
                while bits != 0 {
                    let bit = bits.trailing_zeros() as usize;
                    let index = word_index * 64 + bit;
                    if index < HYPERVECTOR_BITS { counts[index] += 1; }
                    bits &= bits - 1;
                }
            }
        }
        let mut out = Self::zero();
        for (index, count) in counts.iter().enumerate() {
            if count * 2 > vectors.len() { out.set_bit(index, true); }
        }
        out
    }

    pub fn hamming_distance(&self, other: &Self) -> u32 {
        self.0.iter().zip(other.0.iter()).map(|(a, b)| (a ^ b).count_ones()).sum()
    }

    /// Similarity in integer basis points from 0 (different) through 10,000 (equal).
    pub fn similarity_basis_points(&self, other: &Self) -> u16 {
        let distance = self.hamming_distance(other) as usize;
        (((HYPERVECTOR_BITS - distance) * 10_000) / HYPERVECTOR_BITS) as u16
    }

    /// Encode whitespace-separated input by binding token identity with position,
    /// then majority-bundling the positioned vectors.
    pub fn encode_text(text: &str) -> Self {
        let vectors: Vec<Self> = text.split_whitespace().enumerate()
            .map(|(position, token)| Self::item(&token.to_lowercase()).permute(position))
            .collect();
        Self::bundle(&vectors)
    }

    fn mask_padding(&mut self) {
        if LAST_WORD_BITS != 0 {
            self.0[HYPERVECTOR_WORDS - 1] &= (1u64 << LAST_WORD_BITS) - 1;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test] fn dimension_and_padding_are_exact() {
        assert_eq!(HYPERVECTOR_BITS, 10_000);
        assert_eq!(HYPERVECTOR_WORDS, 157);
        let vector = HyperVector::from_seed(42);
        assert_eq!(vector.0[156] >> 16, 0);
    }
    #[test] fn item_memory_is_deterministic_and_token_sensitive() {
        assert_eq!(HyperVector::item("borrow"), HyperVector::item("borrow"));
        assert_ne!(HyperVector::item("borrow"), HyperVector::item("move"));
    }
    #[test] fn xor_binding_is_involutive() {
        let a = HyperVector::from_seed(1);
        let b = HyperVector::from_seed(2);
        assert_eq!(a.bind(&b).bind(&b), a);
    }
    #[test] fn permutation_wraps_over_logical_dimension() {
        let mut value = HyperVector::zero();
        value.set_bit(HYPERVECTOR_BITS - 1, true);
        assert!(value.permute(1).bit(0));
        assert_eq!(value.permute(HYPERVECTOR_BITS), value);
    }
    #[test] fn majority_bundle_and_ties() {
        let a = HyperVector::item("a");
        let b = HyperVector::item("b");
        assert_eq!(HyperVector::bundle(&[a, a, b]), a);
        let tied = HyperVector::bundle(&[a, b]);
        for i in 0..HYPERVECTOR_BITS {
            if a.bit(i) != b.bit(i) { assert!(!tied.bit(i)); }
        }
        assert_eq!(HyperVector::bundle(&[]), HyperVector::zero());
    }
    #[test] fn hamming_distance_and_integer_similarity() {
        let a = HyperVector::zero();
        let mut b = a;
        b.set_bit(0, true);
        assert_eq!(a.hamming_distance(&b), 1);
        assert_eq!(a.similarity_basis_points(&a), 10_000);
        assert_eq!(a.similarity_basis_points(&b), 9_999);
    }
    #[test] fn text_encoding_is_stable_and_order_sensitive() {
        assert_eq!(HyperVector::encode_text("repair borrow"), HyperVector::encode_text("repair borrow"));
        assert_ne!(HyperVector::encode_text("repair borrow"), HyperVector::encode_text("borrow repair"));
    }
}
