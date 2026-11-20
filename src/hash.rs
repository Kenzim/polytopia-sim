//! The game's seeded hash (xxHash32 over `int` words) and the helpers layered on it.
//!
//! The AI draws its "randomness" from this hash of `(current_command * 1000, x, y)` keyed by
//! the game seed (the game state's `int` seed). The hash over an `int[]` is xxHash32 of the
//! little-endian bytes: 4-word stripes, then one 4-byte step per remaining word.

pub use crate::rng::XxHash;

/// Scale from a 32-bit hash to a unit float: `2^-32`.
const UNIT_SCALE: f32 = 1.0 / 4_294_967_296.0;

/// Helpers layered on the hash of an `int` array or a single `int`.
pub trait HashFunction {
    /// Hash of two words.
    fn get_hash2(&self, x: i32, y: i32) -> u32;
    /// Hash of three words.
    fn get_hash3(&self, x: i32, y: i32, z: i32) -> u32;
    /// Unit value of a word array: `(float)hash * 2^-32`.
    fn value(&self, buf: &[i32]) -> f32;
    /// Unit value of one word.
    fn value1(&self, x: i32) -> f32;
    /// Unit value of two words.
    fn value2(&self, x: i32, y: i32) -> f32;
    /// Unit value of three words.
    fn value3(&self, x: i32, y: i32, z: i32) -> f32;
    /// A value in `min..max` drawn from the hash of a word array.
    ///
    /// `hash % (max - min) + min`. The divisor is the sign-extended 32-bit difference.
    /// A non-negative divisor gives an unsigned 32-bit remainder; a negative one a signed 64-bit
    /// remainder of the zero-extended hash, which is non-negative. A zero divisor
    /// is a division fault in the game and returns `None` here.
    fn range(&self, min: i32, max: i32, buf: &[i32]) -> Option<i32>;
    /// As [`HashFunction::range`], for one data word.
    fn range1(&self, min: i32, max: i32, x: i32) -> Option<i32>;
}

impl HashFunction for XxHash {
    fn get_hash2(&self, x: i32, y: i32) -> u32 {
        self.hash_i32s(&[x, y])
    }

    fn get_hash3(&self, x: i32, y: i32, z: i32) -> u32 {
        self.hash_i32s(&[x, y, z])
    }

    fn value(&self, buf: &[i32]) -> f32 {
        unit(self.hash_i32s(buf))
    }

    fn value1(&self, x: i32) -> f32 {
        unit(self.hash_i32(x))
    }

    fn value2(&self, x: i32, y: i32) -> f32 {
        unit(self.get_hash2(x, y))
    }

    fn value3(&self, x: i32, y: i32, z: i32) -> f32 {
        unit(self.get_hash3(x, y, z))
    }

    fn range(&self, min: i32, max: i32, buf: &[i32]) -> Option<i32> {
        range_from_hash(self.hash_i32s(buf), min, max)
    }

    fn range1(&self, min: i32, max: i32, x: i32) -> Option<i32> {
        range_from_hash(self.hash_i32(x), min, max)
    }
}

/// The remainder step shared by every `Range` overload.
pub fn range_from_hash(hash: u32, min: i32, max: i32) -> Option<i32> {
    let divisor = max.wrapping_sub(min);
    if divisor == 0 {
        return None;
    }
    let remainder = if divisor > 0 {
        hash % divisor as u32
    } else {
        // `idiv rcx` with a negative divisor and a non-negative dividend.
        (u64::from(hash) % u64::from(divisor.unsigned_abs())) as u32
    };
    Some((remainder as i32).wrapping_add(min))
}

/// The zero-extended hash converted to `f32`, then multiplied by `2^-32`.
fn unit(hash: u32) -> f32 {
    (hash as f32) * UNIT_SCALE
}

#[cfg(test)]
mod tests {
    use super::*;

    const PRIME32_1: u32 = 2_654_435_761;
    const PRIME32_2: u32 = 2_246_822_519;
    const PRIME32_3: u32 = 3_266_489_917;
    const PRIME32_4: u32 = 668_265_263;
    const PRIME32_5: u32 = 374_761_393;

    /// Straight xxHash32 over bytes, written independently of the port above.
    fn xxh32(bytes: &[u8], seed: u32) -> u32 {
        let read = |i: usize| u32::from_le_bytes([bytes[i], bytes[i + 1], bytes[i + 2], bytes[i + 3]]);
        let round = |acc: u32, input: u32| {
            acc.wrapping_add(input.wrapping_mul(PRIME32_2)).rotate_left(13).wrapping_mul(PRIME32_1)
        };
        let len = bytes.len();
        let mut i = 0;
        let mut h = if len >= 16 {
            let mut v = [
                seed.wrapping_add(PRIME32_1).wrapping_add(PRIME32_2),
                seed.wrapping_add(PRIME32_2),
                seed,
                seed.wrapping_sub(PRIME32_1),
            ];
            while i + 16 <= len {
                for (lane, acc) in v.iter_mut().enumerate() {
                    *acc = round(*acc, read(i + lane * 4));
                }
                i += 16;
            }
            v[0].rotate_left(1)
                .wrapping_add(v[1].rotate_left(7))
                .wrapping_add(v[2].rotate_left(12))
                .wrapping_add(v[3].rotate_left(18))
        } else {
            seed.wrapping_add(PRIME32_5)
        };
        h = h.wrapping_add(len as u32);
        while i + 4 <= len {
            h = h.wrapping_add(read(i).wrapping_mul(PRIME32_3)).rotate_left(17).wrapping_mul(PRIME32_4);
            i += 4;
        }
        while i < len {
            h = h.wrapping_add(u32::from(bytes[i]).wrapping_mul(PRIME32_5)).rotate_left(11).wrapping_mul(PRIME32_1);
            i += 1;
        }
        h ^= h >> 15;
        h = h.wrapping_mul(PRIME32_2);
        h ^= h >> 13;
        h = h.wrapping_mul(PRIME32_3);
        h ^ (h >> 16)
    }

    #[test]
    fn empty_input_matches_the_published_xxh32_vector() {
        assert_eq!(XxHash::new(0).hash_i32s(&[]), 0x02CC_5D05);
    }

    #[test]
    fn matches_a_byte_oriented_xxhash32_for_every_short_length_and_seed() {
        for seed in [0, 1, -7, 123_456_789, i32::MIN, i32::MAX] {
            let hash = XxHash::new(seed);
            for len in 0..40usize {
                let words: Vec<i32> =
                    (0..len as i32).map(|i| i.wrapping_mul(2_654_435_761u32 as i32) ^ 0x1234_5678).collect();
                let bytes: Vec<u8> = words.iter().flat_map(|w| w.to_le_bytes()).collect();
                assert_eq!(hash.hash_i32s(&words), xxh32(&bytes, seed as u32), "seed {seed} len {len}");
            }
        }
    }

    #[test]
    fn single_word_form_equals_the_array_form() {
        let hash = XxHash::new(42);
        for x in [0, 1, -1, 1000, i32::MIN, i32::MAX] {
            assert_eq!(hash.hash_i32(x), hash.hash_i32s(&[x]));
        }
    }

    #[test]
    fn value_is_the_hash_over_two_to_the_32() {
        let hash = XxHash::new(9);
        let raw = hash.get_hash3(1000, 3, 4);
        assert_eq!(hash.value3(1000, 3, 4), (raw as f32) * (1.0 / 4_294_967_296.0));
        assert!(hash.value3(1000, 3, 4) >= 0.0 && hash.value3(1000, 3, 4) <= 1.0);
    }

    #[test]
    fn range_matches_the_remainder_rules() {
        assert_eq!(range_from_hash(10, 5, 8), Some(5 + 10 % 3));
        assert_eq!(range_from_hash(10, 0, 0), None);
        // Negative divisor: the zero-extended hash is divided by the signed 64-bit value.
        assert_eq!(range_from_hash(10, 8, 5), Some(8 + 10 % 3));
    }
}
