//! BITCOS sparse ternary encoding: one presence bit per weight and one
//! compacted sign bit per nonzero weight. Decode and score are integer-only.
use crate::ternary_snn::{Ternary, PackedTernaryRouter};
use crate::hdc::{HyperVector, HYPERVECTOR_BITS};

/// AIRW feature flag marking sections encoded with BITCOS.
pub use air_weights::AIRW_FLAG_BITCOS_TERNARY;

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum BitcosError {
    InvalidDimensions,
    InputDimensionMismatch,
    InvalidEncoding,
    Router(String),
}

/// Sparse encoding with a dense presence bitmap and compacted sign stream.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BitcosTernary {
    len: usize,
    active: usize,
    presence: Vec<u8>,
    signs: Vec<u8>,
}

impl BitcosTernary {
    pub fn encode(weights: &[Ternary]) -> Result<Self, BitcosError> {
        if weights.is_empty() { return Err(BitcosError::InvalidDimensions); }
        let mut presence = vec![0u8; weights.len().div_ceil(8)];
        let active = weights.iter().filter(|weight| **weight != Ternary::Zero).count();
        let mut signs = vec![0u8; active.div_ceil(8)];
        let mut sign_index = 0usize;
        for (index, weight) in weights.iter().enumerate() {
            match weight {
                Ternary::Zero => {}
                Ternary::Negative | Ternary::Positive => {
                    presence[index / 8] |= 1 << (index % 8);
                    if *weight == Ternary::Positive {
                        signs[sign_index / 8] |= 1 << (sign_index % 8);
                    }
                    sign_index += 1;
                }
            }
        }
        Ok(Self { len: weights.len(), active, presence, signs })
    }

    /// Opens an encoded stream by borrowing its two byte sections; validation
    /// checks lengths and zero padding before any decoding operation.
    pub fn from_borrowed<'a>(
        len: usize,
        active: usize,
        presence: &'a [u8],
        signs: &'a [u8],
    ) -> Result<BorrowedBitcos<'a>, BitcosError> {
        let value = BorrowedBitcos { len, active, presence, signs };
        value.validate()?;
        Ok(value)
    }

    pub fn len(&self) -> usize { self.len }
    pub fn active(&self) -> usize { self.active }
    pub fn presence(&self) -> &[u8] { &self.presence }
    pub fn signs(&self) -> &[u8] { &self.signs }

    /// Encoded density in bits per ternary weight. The raw weight stream
    /// excludes archive metadata and section descriptors.
    pub fn bits_per_weight(&self) -> f64 {
        ((self.presence.len() + self.signs.len()) * 8) as f64 / self.len as f64
    }

    pub fn decode_into(&self, output: &mut [Ternary]) -> Result<(), BitcosError> {
        BorrowedBitcos::from_parts(self.len, self.active, &self.presence, &self.signs)?
            .decode_into(output)
    }

    pub fn score(&self, input: &HyperVector) -> Result<i32, BitcosError> {
        BorrowedBitcos::from_parts(self.len, self.active, &self.presence, &self.signs)?.score(input)
    }
}

/// Validated zero-copy view suitable for a section borrowed from ArchiveReader.
#[derive(Clone, Copy, Debug)]
pub struct BorrowedBitcos<'a> {
    len: usize,
    active: usize,
    presence: &'a [u8],
    signs: &'a [u8],
}
impl<'a> BorrowedBitcos<'a> {
    fn from_parts(len: usize, active: usize, presence: &'a [u8], signs: &'a [u8]) -> Result<Self, BitcosError> {
        let view = Self { len, active, presence, signs };
        view.validate()?;
        Ok(view)
    }

    pub fn len(&self) -> usize { self.len }
    pub fn active(&self) -> usize { self.active }

    pub fn validate(&self) -> Result<(), BitcosError> {
        if self.len == 0 || self.active > self.len
            || self.presence.len() != self.len.div_ceil(8)
            || self.signs.len() != self.active.div_ceil(8) {
            return Err(BitcosError::InvalidEncoding);
        }
        if self.bit_count(self.presence, self.len) != self.active {
            return Err(BitcosError::InvalidEncoding);
        }
        if !padding_is_zero(self.presence, self.len) || !padding_is_zero(self.signs, self.active) {
            return Err(BitcosError::InvalidEncoding);
        }
        Ok(())
    }

    pub fn decode_into(&self, output: &mut [Ternary]) -> Result<(), BitcosError> {
        if output.len() != self.len { return Err(BitcosError::InvalidDimensions); }
        let mut compacted = 0usize;
        for (index, destination) in output.iter_mut().enumerate() {
            if bit(self.presence, index) {
                *destination = if bit(self.signs, compacted) { Ternary::Positive } else { Ternary::Negative };
                compacted += 1;
            } else {
                *destination = Ternary::Zero;
            }
        }
        Ok(())
    }

    /// Integer-only ternary dot product against a hypervector prefix.
    pub fn score(&self, input: &HyperVector) -> Result<i32, BitcosError> {
        if self.len != HYPERVECTOR_BITS { return Err(BitcosError::InputDimensionMismatch); }
        let mut score = 0i32;
        let mut compacted = 0usize;
        for index in 0..self.len {
            if bit(self.presence, index) {
                let x = if input.bit(index) { 1 } else { -1 };
                score += if bit(self.signs, compacted) { x } else { -x };
                compacted += 1;
            }
        }
        Ok(score)
    }

    fn bit_count(&self, bytes: &[u8], bit_len: usize) -> usize {
        bytes.iter().map(|byte| byte.count_ones() as usize).sum::<usize>()
            - padding_count(bytes.last().copied().unwrap_or(0), bit_len)
    }
}

fn padding_count(last: u8, bit_len: usize) -> usize {
    let used = bit_len % 8;
    if used == 0 { 0 } else { (last & !((1u8 << used) - 1)).count_ones() as usize }
}
fn padding_is_zero(bytes: &[u8], bit_len: usize) -> bool {
    let used = bit_len % 8;
    used == 0 || bytes.last().is_some_and(|last| last & !((1u8 << used) - 1) == 0)
}
#[inline]
fn bit(bytes: &[u8], index: usize) -> bool {
    (bytes[index / 8] & (1 << (index % 8))) != 0
}

/// Build an AIRW archive carrying BITCOS-encoded router weights.
///
/// This uses the AIRW header's previously-zero reserved flag. Existing archives
/// with a zero flag remain valid and decode as the legacy encoding.
pub fn encode_router_archive(
    router: &PackedTernaryRouter,
    package: &str,
    language: &str,
) -> Result<Vec<u8>, BitcosError> {
    let mut packed = Vec::new();
    packed.extend_from_slice(&(router.input_bits() as u32).to_le_bytes());
    packed.extend_from_slice(&(router.outputs() as u32).to_le_bytes());
    for output in 0..router.outputs() {
        let values: Vec<Ternary> = (0..router.input_bits())
            .map(|input| router.weight(output, input).expect("validated router coordinate"))
            .collect();
        let encoding = BitcosTernary::encode(&values)?;
        packed.extend_from_slice(&(encoding.active as u32).to_le_bytes());
        packed.extend_from_slice(&(encoding.presence.len() as u32).to_le_bytes());
        packed.extend_from_slice(&(encoding.signs.len() as u32).to_le_bytes());
        packed.extend_from_slice(&encoding.presence);
        packed.extend_from_slice(&encoding.signs);
    }
    let mut builder = air_weights::ArchiveBuilder::new(package);
    builder.add_language(language).map_err(|e| BitcosError::Router(e.to_string()))?;
    builder.add_section("router.bitcos", "ternary-bitcos-v1", &packed)
        .map_err(|e| BitcosError::Router(e.to_string()))?;
    builder.set_header_flags(AIRW_FLAG_BITCOS_TERNARY)
        .map_err(|e| BitcosError::Router(e.to_string()))?;
    builder.finish().map_err(|e| BitcosError::Router(e.to_string()))
}

/// Decodes a BITCOS router section directly from an AIRW reader.
///
/// The section bytes are borrowed from the reader (and therefore from its
/// mmap when opened with `ArchiveReader::open`). The threshold is supplied by
/// the caller because the legacy router format does not serialize it.
pub fn decode_router_archive(
    reader: &air_weights::ArchiveReader,
    threshold: i32,
) -> Result<PackedTernaryRouter, BitcosError> {
    if reader.header_flags() & AIRW_FLAG_BITCOS_TERNARY == 0 {
        return Err(BitcosError::InvalidEncoding);
    }
    let bytes = reader.section("router.bitcos").ok_or(BitcosError::InvalidEncoding)?;
    let mut cursor = 0usize;
    fn take_u32(bytes: &[u8], cursor: &mut usize) -> Result<u32, BitcosError> {
        let end = cursor.checked_add(4).ok_or(BitcosError::InvalidEncoding)?;
        let raw = bytes.get(*cursor..end).ok_or(BitcosError::InvalidEncoding)?;
        *cursor = end;
        Ok(u32::from_le_bytes(raw.try_into().map_err(|_| BitcosError::InvalidEncoding)?))
    }

    let input_bits = take_u32(bytes, &mut cursor)? as usize;
    let outputs = take_u32(bytes, &mut cursor)? as usize;
    if input_bits != HYPERVECTOR_BITS || outputs == 0 {
        return Err(BitcosError::InvalidDimensions);
    }
    let mut router = PackedTernaryRouter::new(input_bits, outputs, threshold)
        .map_err(|error| BitcosError::Router(format!("{error:?}")))?;
    let presence_len = input_bits.div_ceil(8);
    for output in 0..outputs {
        let active = take_u32(bytes, &mut cursor)? as usize;
        let encoded_presence_len = take_u32(bytes, &mut cursor)? as usize;
        let encoded_sign_len = take_u32(bytes, &mut cursor)? as usize;
        if encoded_presence_len != presence_len || active > input_bits
            || encoded_sign_len != active.div_ceil(8) {
            return Err(BitcosError::InvalidEncoding);
        }
        let presence_end = cursor.checked_add(encoded_presence_len).ok_or(BitcosError::InvalidEncoding)?;
        let presence = bytes.get(cursor..presence_end).ok_or(BitcosError::InvalidEncoding)?;
        cursor = presence_end;
        let signs_end = cursor.checked_add(encoded_sign_len).ok_or(BitcosError::InvalidEncoding)?;
        let signs = bytes.get(cursor..signs_end).ok_or(BitcosError::InvalidEncoding)?;
        cursor = signs_end;
        let view = BorrowedBitcos::from_parts(input_bits, active, presence, signs)?;
        let mut weights = vec![Ternary::Zero; input_bits];
        view.decode_into(&mut weights)?;
        for (input, weight) in weights.into_iter().enumerate() {
            router.set_weight(output, input, weight)
                .map_err(|error| BitcosError::Router(format!("{error:?}")))?;
        }
    }
    if cursor != bytes.len() { return Err(BitcosError::InvalidEncoding); }
    Ok(router)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips_sparse_signs_and_measures_bits_per_weight() {
        let weights: Vec<_> = (0..10_000).map(|index| match index % 10 {
            0 => Ternary::Positive,
            1 => Ternary::Negative,
            _ => Ternary::Zero,
        }).collect();
        let encoded = BitcosTernary::encode(&weights).unwrap();
        assert_eq!(encoded.active(), 2_000);
        assert!(encoded.bits_per_weight() < 1.625);
        let mut decoded = vec![Ternary::Zero; weights.len()];
        encoded.decode_into(&mut decoded).unwrap();
        assert_eq!(decoded, weights);
    }

    #[test]
    fn borrowed_view_validates_without_copying_and_scores_as_integer_dot() {
        let weights: Vec<_> = (0..HYPERVECTOR_BITS).map(|index| match index % 4 {
            0 => Ternary::Positive, 1 => Ternary::Negative, _ => Ternary::Zero,
        }).collect();
        let encoded = BitcosTernary::encode(&weights).unwrap();
        let view = BitcosTernary::from_borrowed(encoded.len, encoded.active, encoded.presence(), encoded.signs()).unwrap();
        assert_eq!(view.presence.as_ptr(), encoded.presence.as_ptr());
        assert_eq!(view.signs.as_ptr(), encoded.signs.as_ptr());
        assert_eq!(view.score(&HyperVector::zero()).unwrap(), 0);
    }

    #[test]
    fn rejects_padding_corruption_and_inconsistent_activity_count() {
        assert!(BorrowedBitcos::from_parts(9, 1, &[0b1000_0001, 0b1000_0000], &[0]).is_err());
        assert!(BorrowedBitcos::from_parts(8, 2, &[1], &[0]).is_err());
    }

    #[test]
    fn sparse_router_archive_uses_backward_compatible_header_flag() {
        let mut router = PackedTernaryRouter::new(HYPERVECTOR_BITS, 1, 0).unwrap();
        router.set_weight(0, 2, Ternary::Positive).unwrap();
        let bytes = encode_router_archive(&router, "test-router", "rust").unwrap();
        let reader = air_weights::ArchiveReader::from_bytes(bytes).unwrap();
        assert_eq!(reader.header_flags(), AIRW_FLAG_BITCOS_TERNARY);
        let section = reader.section("router.bitcos").unwrap();
        assert!(!section.is_empty());
        assert_eq!(reader.encoding_flag(), AIRW_FLAG_BITCOS_TERNARY);
    }
    #[test]
    fn bitcos_airw_router_round_trips_through_mmap_reader() {
        use std::sync::atomic::{AtomicU64, Ordering};
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let path = std::env::temp_dir().join(format!(
            "airw-bitcos-router-{}-{}.airw",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        let mut router = PackedTernaryRouter::new(HYPERVECTOR_BITS, 2, 17).unwrap();
        router.set_weight(0, 3, Ternary::Positive).unwrap();
        router.set_weight(0, 9, Ternary::Negative).unwrap();
        router.set_weight(1, 1, Ternary::Negative).unwrap();
        let bytes = encode_router_archive(&router, "roundtrip-router", "rust").unwrap();
        std::fs::write(&path, bytes).unwrap();
        let reader = air_weights::ArchiveReader::open(&path).unwrap();
        let decoded = decode_router_archive(&reader, 17).unwrap();
        assert!(reader.is_memory_mapped());
        for output in 0..router.outputs() {
            for input in 0..router.input_bits() {
                assert_eq!(decoded.weight(output, input), router.weight(output, input));
            }
        }
        drop(reader);
        std::fs::remove_file(path).unwrap();
    }

    #[test]
    fn memory_mapped_bitcos_sections_are_borrowed_without_copy_or_alignment_assumption() {
        use std::sync::atomic::{AtomicU64, Ordering};
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let path = std::env::temp_dir().join(format!(
            "airw-bitcos-{}-{}.airw",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        let weights: Vec<_> = (0..HYPERVECTOR_BITS).map(|index| match index % 5 {
            0 => Ternary::Positive,
            1 => Ternary::Negative,
            _ => Ternary::Zero,
        }).collect();
        let encoding = BitcosTernary::encode(&weights).unwrap();
        let mut builder = crate::ArchiveBuilder::new("bitcos-mmap");
        builder.add_language("rust").unwrap();
        builder.set_header_flags(AIRW_FLAG_BITCOS_TERNARY).unwrap();
        builder.add_section("presence", "bitcos-presence-v1", encoding.presence()).unwrap();
        builder.add_section("signs", "bitcos-signs-v1", encoding.signs()).unwrap();
        std::fs::write(&path, builder.finish().unwrap()).unwrap();

        let reader = crate::ArchiveReader::open(&path).unwrap();
        assert!(reader.is_memory_mapped());
        let presence = reader.section("presence").unwrap();
        let signs = reader.section("signs").unwrap();
        let view = BitcosTernary::from_borrowed(encoding.len(), encoding.active(), presence, signs).unwrap();
        assert_eq!(view.presence.as_ptr(), presence.as_ptr());
        assert_eq!(view.signs.as_ptr(), signs.as_ptr());
        // BITCOS is a packed byte stream and therefore requires alignment 1;
        // no pointer cast or copy is needed, regardless of manifest length.
        assert_eq!((presence.as_ptr() as usize) % std::mem::align_of::<u8>(), 0);
        drop(reader);
        std::fs::remove_file(path).unwrap();
    }
}
