//! Legacy .NET `System.Random`, the game's seeded xxHash32, and combat rounding to tens.
//!
//! Constants and control flow reproduce the game's own generators. See `docs/specs/rng.md`.

/// `Int32.MaxValue`, the subtractive generator modulus (`MBIG`).
const MBIG: i32 = 2147483647;
/// Knuth/Numerical Recipes seed (`MSEED`), `0x09A4EC86`.
const MSEED: i32 = 161803398;
/// `Sample` scale: `1.0 / MBIG`.
const SAMPLE_SCALE: f64 = f64::from_bits(0x3e00000000200000);
/// Large-range sample addend (`Int32.MaxValue - 1`).
const LARGE_SAMPLE_BIAS: f64 = 2147483646.0;
/// Large-range sample divisor (`2 * (uint)MaxValue - 1`).
const LARGE_SAMPLE_DIVISOR: f64 = 4294967293.0;

const PRIME32_1: u32 = 2654435761;
const PRIME32_2: u32 = 2246822519;
const PRIME32_3: u32 = 3266489917;
const PRIME32_4: u32 = 668265263;
const PRIME32_5: u32 = 374761393;

/// .NET's legacy `System.Random`: Knuth subtractive generator.
///
/// State is `inext`, `inextp` and a 56-entry seed array, as in .NET.
#[derive(Clone, Debug)]
pub struct DotNetRandom {
    inext: i32,
    inextp: i32,
    seed_array: [i32; 56],
}

/// Recorded calls: (method, argument, argument, result).
type CallLog = Vec<(&'static str, i32, i32, String)>;

thread_local! {
    static CALL_TRACE: std::cell::RefCell<Option<CallLog>> =
        const { std::cell::RefCell::new(None) };
}

/// Threads with a trace started and not yet taken; while it is 0, [`trace`] skips the thread-local lookup.
static TRACING_THREADS: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);

/// Start recording every public draw on this thread as `(name, arg0, arg1, result)`.
/// Used to diff against a trace of the real game's draws.
pub fn trace_start() {
    let was_on = CALL_TRACE.with(|t| t.borrow_mut().replace(Vec::new()).is_some());
    if !was_on {
        TRACING_THREADS.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
    }
}

/// Stop recording and return what was captured.
pub fn trace_take() -> Vec<(&'static str, i32, i32, String)> {
    let log = CALL_TRACE.with(|t| t.borrow_mut().take());
    if log.is_some() {
        TRACING_THREADS.fetch_sub(1, std::sync::atomic::Ordering::SeqCst);
    }
    log.unwrap_or_default()
}

#[inline]
fn trace(name: &'static str, a: i32, b: i32, result: impl FnOnce() -> String) {
    if TRACING_THREADS.load(std::sync::atomic::Ordering::Relaxed) == 0 {
        return;
    }
    CALL_TRACE.with(|t| {
        if let Some(log) = t.borrow_mut().as_mut() {
            log.push((name, a, b, result()));
        }
    });
}

impl DotNetRandom {
    /// `System.Random(int seed)`.
    pub fn new(seed: i32) -> Self {
        let mut seed_array = [0i32; 56];
        // `Seed == int.MinValue` skips `Abs` and uses `MSEED - int.MaxValue`.
        let subtraction = if seed == i32::MIN { i32::MAX } else { seed.wrapping_abs() };
        let mut mj = MSEED.wrapping_sub(subtraction);
        seed_array[55] = mj;
        let mut mk = 1i32;
        for i in 1..55 {
            let ii = (21 * i) % 55;
            seed_array[ii] = mk;
            let mut diff = mj.wrapping_sub(mk);
            if diff < 0 {
                diff = diff.wrapping_add(MBIG);
            }
            mk = diff;
            mj = seed_array[ii];
        }
        for _ in 0..4 {
            for i in 1..56 {
                let other = 1 + (i + 30) % 55;
                let mut val = seed_array[i].wrapping_sub(seed_array[other]);
                if val < 0 {
                    val = val.wrapping_add(MBIG);
                }
                seed_array[i] = val;
            }
        }
        Self { inext: 0, inextp: 21, seed_array }
    }

    /// `Random.Next()`: one raw internal sample.
    #[allow(clippy::should_implement_trait)] // returns a plain `i32`, not an `Option`, and is public API
    pub fn next(&mut self) -> i32 {
        let value = self.internal_sample();
        trace("Next", 0, 0, || value.to_string());
        value
    }

    /// `Random.Next(int)`. Panics if `max_value < 0`.
    pub fn next_max(&mut self, max_value: i32) -> i32 {
        if max_value < 0 {
            panic!("Random.Next maxValue is negative");
        }
        let value = (self.sample() * max_value as f64) as i32;
        trace("NextMax", max_value, 0, || value.to_string());
        value
    }

    /// `Random.Next(int, int)`. Panics if `min_value > max_value`.
    pub fn next_range(&mut self, min_value: i32, max_value: i32) -> i32 {
        if min_value > max_value {
            panic!("Random.Next minValue is greater than maxValue");
        }
        let range = max_value as i64 - min_value as i64;
        if range <= i32::MAX as i64 {
            let scaled = (self.sample() * range as f64) as i32;
            let value = scaled.wrapping_add(min_value);
            trace("NextMM", min_value, max_value, || value.to_string());
            value
        } else {
            let scaled = (self.sample_for_large_range() * range as f64) as i64;
            let value = (scaled as i32).wrapping_add(min_value);
            trace("NextMM", min_value, max_value, || value.to_string());
            value
        }
    }

    /// `Random.NextDouble()`: the scaled sample.
    pub fn next_double(&mut self) -> f64 {
        let value = self.sample();
        trace("NextDouble", 0, 0, || format!("{value:?}"));
        value
    }

    /// The raw subtractive-generator step.
    fn internal_sample(&mut self) -> i32 {
        let loc_inext = wrap_index(self.inext);
        let loc_inextp = wrap_index(self.inextp);
        let orig = self.seed_array[loc_inext as usize].wrapping_sub(self.seed_array[loc_inextp as usize]);
        let ret = if orig == MBIG {
            MBIG - 1
        } else if orig < 0 {
            orig.wrapping_add(MBIG)
        } else {
            orig
        };
        self.seed_array[loc_inext as usize] = ret;
        self.inext = loc_inext;
        self.inextp = loc_inextp;
        ret
    }

    /// The raw step scaled into `[0, 1)`.
    fn sample(&mut self) -> f64 {
        self.internal_sample() as f64 * SAMPLE_SCALE
    }

    /// Sample used when the range does not fit in an `i32`.
    fn sample_for_large_range(&mut self) -> f64 {
        let mut result = self.internal_sample();
        if self.internal_sample() % 2 == 0 {
            result = result.wrapping_neg();
        }
        (result as f64 + LARGE_SAMPLE_BIAS) / LARGE_SAMPLE_DIVISOR
    }
}

/// Increment and wrap into `1..=55`: an index reaching 56 goes back to 1.
fn wrap_index(index: i32) -> i32 {
    let next = index + 1;
    if next >= 56 {
        1
    } else {
        next
    }
}

/// The game's seeded hash: xxHash32 over little-endian `int` words.
#[derive(Clone, Copy, Debug)]
pub struct XxHash {
    seed: u32,
}

impl XxHash {
    /// A hash keyed by `seed`. The `int` is stored as a raw `uint`.
    pub fn new(seed: i32) -> Self {
        Self { seed: seed as u32 }
    }

    /// Hash of one word.
    pub fn hash_i32(&self, value: i32) -> u32 {
        let mut h =
            self.seed.wrapping_add((value as u32).wrapping_mul(PRIME32_3)).wrapping_add(PRIME32_5.wrapping_add(4));
        h = h.rotate_left(17).wrapping_mul(PRIME32_4);
        xxh_avalanche(h)
    }

    /// Hash of a word array.
    ///
    /// An empty slice is the zero-length hash. The game throws on a null array;
    /// a Rust slice has no null state.
    pub fn hash_i32s(&self, buf: &[i32]) -> u32 {
        let byte_len = (buf.len() as u32).wrapping_mul(4);
        let mut h;
        let mut i = 0usize;
        if buf.len() >= 4 {
            let mut v1 = self.seed.wrapping_add(PRIME32_1).wrapping_add(PRIME32_2);
            let mut v2 = self.seed.wrapping_add(PRIME32_2);
            let mut v3 = self.seed;
            let mut v4 = self.seed.wrapping_sub(PRIME32_1);
            while i + 4 <= buf.len() {
                v1 = xxh_round(v1, buf[i] as u32);
                v2 = xxh_round(v2, buf[i + 1] as u32);
                v3 = xxh_round(v3, buf[i + 2] as u32);
                v4 = xxh_round(v4, buf[i + 3] as u32);
                i += 4;
            }
            h = v1
                .rotate_left(1)
                .wrapping_add(v2.rotate_left(7))
                .wrapping_add(v3.rotate_left(12))
                .wrapping_add(v4.rotate_left(18))
                .wrapping_add(byte_len);
        } else {
            h = self.seed.wrapping_add(PRIME32_5).wrapping_add(byte_len);
        }
        while i < buf.len() {
            h = h.wrapping_add((buf[i] as u32).wrapping_mul(PRIME32_3)).rotate_left(17).wrapping_mul(PRIME32_4);
            i += 1;
        }
        xxh_avalanche(h)
    }
}

/// The per-word mix (the byte variant uses the same mix).
fn xxh_round(acc: u32, input: u32) -> u32 {
    acc.wrapping_add(input.wrapping_mul(PRIME32_2)).rotate_left(13).wrapping_mul(PRIME32_1)
}

/// Final mix shared by the single-word and array hashes.
fn xxh_avalanche(mut h: u32) -> u32 {
    h ^= h >> 15;
    h = h.wrapping_mul(PRIME32_2);
    h ^= h >> 13;
    h = h.wrapping_mul(PRIME32_3);
    h ^= h >> 16;
    h
}

/// A unit float: `(float)NextDouble()`.
pub fn value(rng: &mut DotNetRandom) -> f32 {
    rng.next_double() as f32
}

/// A float in `min..max`.
///
/// `(max - min)` is computed in `f32`, then the scale and add happen in `f64`.
pub fn range_f32(rng: &mut DotNetRandom, min: f32, max: f32) -> f32 {
    let span = (max - min) as f64;
    (min as f64 + span * rng.next_double()) as f32
}

/// An int in `0..max`: just `Random.Next(max)`.
pub fn range_i32_max(rng: &mut DotNetRandom, max: i32) -> i32 {
    rng.next_max(max)
}

/// An int in `min..max`: just `Random.Next(min, max)`.
pub fn range_i32(rng: &mut DotNetRandom, min: i32, max: i32) -> i32 {
    rng.next_range(min, max)
}

/// Combat floor to a multiple of ten.
///
/// Signed integer division by 10, so this truncates toward zero.
pub fn floor_to_10(n: i32) -> i32 {
    n / 10 * 10
}

/// Combat round to the nearest ten, halves up.
///
/// `base = floor_to_10(n)`; if `n - base >= 5` then `base + 10`, else `base`.
/// Both the compare and the `+ 10` are 32-bit, so `i32::MAX` wraps.
pub fn round_to_10(n: i32) -> i32 {
    let base = floor_to_10(n);
    if n.wrapping_sub(base) >= 5 {
        base.wrapping_add(10)
    } else {
        base
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn trace_records_draws_only_between_start_and_take() {
        let mut rng = DotNetRandom::new(7);
        rng.next_range(0, 10);
        trace_start();
        let a = rng.next_range(0, 10);
        let b = rng.next_max(5);
        let log = trace_take();
        rng.next_range(0, 10);
        assert_eq!(log.len(), 2);
        assert_eq!((log[0].0, log[0].1, log[0].2, log[0].3.as_str()), ("NextMM", 0, 10, a.to_string().as_str()));
        assert_eq!((log[1].0, log[1].1, log[1].3.as_str()), ("NextMax", 5, b.to_string().as_str()));
        assert!(trace_take().is_empty());
    }

    #[test]
    fn floor_to_10_vectors() {
        assert_eq!(floor_to_10(0), 0);
        assert_eq!(floor_to_10(4), 0);
        assert_eq!(floor_to_10(9), 0);
        assert_eq!(floor_to_10(10), 10);
        assert_eq!(floor_to_10(15), 10);
        assert_eq!(floor_to_10(-1), 0);
        assert_eq!(floor_to_10(-15), -10);
        assert_eq!(floor_to_10(i32::MIN), -2147483640);
        assert_eq!(floor_to_10(i32::MAX), 2147483640);
    }

    #[test]
    fn round_to_10_vectors() {
        assert_eq!(round_to_10(0), 0);
        assert_eq!(round_to_10(4), 0);
        assert_eq!(round_to_10(5), 10);
        assert_eq!(round_to_10(14), 10);
        assert_eq!(round_to_10(15), 20);
        assert_eq!(round_to_10(-1), 0);
        assert_eq!(round_to_10(-5), 0);
        assert_eq!(round_to_10(-15), -10);
        // `base + 10` overflows int32; the wrapped result is what the game returns.
        assert_eq!(round_to_10(i32::MAX), -2147483646);
    }

    #[test]
    fn dotnet_random_seed_1_first_eight() {
        let mut rng = DotNetRandom::new(1);
        let got = std::array::from_fn(|_| rng.next());
        assert_eq!(got, [534011718, 237820880, 1002897798, 1657007234, 1412011072, 929393559, 760389092, 2026928803,]);
    }

    #[test]
    fn negative_seed_uses_absolute_value() {
        let mut pos = DotNetRandom::new(1);
        let mut neg = DotNetRandom::new(-1);
        for _ in 0..8 {
            assert_eq!(pos.next(), neg.next());
        }
    }

    #[test]
    fn min_seed_is_not_abs() {
        let mut rng = DotNetRandom::new(i32::MIN);
        assert_eq!(rng.next(), 1559595546);
    }

    #[test]
    fn next_double_and_ranges_seed_1() {
        let mut rng = DotNetRandom::new(1);
        assert_eq!(rng.next_double().to_bits(), 0x3fcfd45f463fa8bf);

        let mut rng = DotNetRandom::new(1);
        assert_eq!(value(&mut rng).to_bits(), 0x3e7ea2fa);

        let mut rng = DotNetRandom::new(1);
        let got: [i32; 8] = std::array::from_fn(|_| range_i32_max(&mut rng, 10));
        assert_eq!(got, [2, 1, 4, 7, 6, 4, 3, 9]);

        let mut rng = DotNetRandom::new(1);
        let got: [i32; 8] = std::array::from_fn(|_| range_i32(&mut rng, 3, 7));
        assert_eq!(got, [3, 3, 4, 6, 5, 4, 4, 6]);

        let mut rng = DotNetRandom::new(1);
        assert_eq!(rng.next_max(0), 0);
        assert_eq!(rng.next_range(5, 5), 5);

        let mut rng = DotNetRandom::new(1);
        let got: [i32; 4] = std::array::from_fn(|_| rng.next_range(i32::MIN, i32::MAX));
        assert_eq!(got, [-534011720, -1002897800, 1412011071, 760389091]);

        let mut rng = DotNetRandom::new(1);
        let got: [u32; 4] = std::array::from_fn(|_| range_f32(&mut rng, 0.0, 10.0).to_bits());
        assert_eq!(got, [0x401f25dc, 0x3f8dc096, 0x40957184, 0x40f6e9cf]);
    }

    #[test]
    #[should_panic]
    fn next_max_rejects_negative() {
        let mut rng = DotNetRandom::new(1);
        rng.next_max(-1);
    }

    #[test]
    #[should_panic]
    fn next_range_rejects_inverted() {
        let mut rng = DotNetRandom::new(1);
        rng.next_range(2, 1);
    }

    #[test]
    fn xxhash_matches_xxh32_words() {
        let h = XxHash::new(0);
        assert_eq!(h.hash_i32s(&[]), 0x02cc5d05);
        assert_eq!(h.hash_i32(0), 0x08d6d969);
        assert_eq!(h.hash_i32s(&[0]), h.hash_i32(0));
        assert_eq!(h.hash_i32(1), 0xf3bb7693);
        assert_eq!(XxHash::new(1).hash_i32(-1), 0x5fb84d20);
        assert_eq!(XxHash::new(1).hash_i32s(&[-1]), 0x5fb84d20);
        assert_eq!(XxHash::new(7).hash_i32(i32::MIN), 0xb21b9d25);
        assert_eq!(XxHash::new(1).hash_i32s(&[]), 0x0b2cb792);
        assert_eq!(XxHash::new(1).hash_i32s(&[4, 5, 6]), 0xc070dd4f);
        assert_eq!(XxHash::new(1).hash_i32s(&[4, 5, 6, 7]), 0x714e577b);
        assert_eq!(XxHash::new(0).hash_i32s(&[1, 2, 3, 4, 5, -7, 9]), 0x69b58e2c);
        assert_eq!(XxHash::new(123).hash_i32s(&[1, 2, 3, 4, 5, -7, 9]), 0x218c8e3b);
    }
}
