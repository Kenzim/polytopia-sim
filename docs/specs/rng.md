# RNG

The game uses three random sources that the engine reproduces exactly: .NET's legacy `System.Random`, the game's xxHash32 hash function, and the combat helpers that floor and round to tens. The game's random helpers built on the generator (`value`, `range_f32` and friends) are described below.

`System.Random` in this game is the legacy Knuth subtractive generator (an `inext` and an `inextp` index into a 56-int seed array), not the newer .NET 6 xoshiro generator. `Next`, `InternalSample` and `Sample` all update the same 56-int state.

## Exported API (`src/rng.rs`)

- `DotNetRandom::new(seed) -> Self`
- `DotNetRandom::next`, `next_max`, `next_range`, `next_double`
- `XxHash::new(seed) -> Self`
- `XxHash::hash_i32`, `hash_i32s`
- `value`, `range_f32`, `range_i32_max`, `range_i32`
- `floor_to_10`, `round_to_10`

`next_max` panics if `max < 0`. `next_range` panics if `min > max`. Those are the `ArgumentOutOfRangeException` paths; the exception objects themselves are not reproduced. `min == max` returns `min` and still consumes one sample.

## System.Random

| Method | Role |
| --- | --- |
| `Random(int)` | Seed the 56-int array |
| `Sample` | `InternalSample * (1.0/MBIG)` |
| `InternalSample` | One subtractive step |
| `Next()` | The same as `InternalSample` |
| `GetSampleForLargeRange` | Two samples, used when `max-min > int.MaxValue` |
| `Next(int,int)` | `next_range` |
| `Next(int)` | `next_max` |
| `NextDouble` | The same as `Sample` |

Constants:

- `MBIG = 2147483647` (`0x7FFFFFFF`)
- `MSEED = 161803398` (`0x09A4EC86`)
- `Sample` multiplier = `0x3E00000000200000` as `f64` bits, which is exactly `1.0/2147483647`
- Large-range bias = `2147483646.0`
- Large-range divisor = `4294967293.0` (`2*(uint)int.MaxValue - 1`)
- An index that reaches `56` wraps to `1`. The second step inside `GetSampleForLargeRange` compares the pre-increment index with `55`.

Seeding follows the classic Numerical Recipes loop:

1. If `seed == int.MinValue`, the subtraction is `int.MaxValue`. Otherwise `mj = MSEED - abs(seed)`.
2. `seed_array[55] = mj`, then 54 steps of `ii = (21*i) % 55` for `i = 1..54`, with `mk = mj - mk` and `if (mk < 0) mk += MBIG`.
3. Four passes of `seed_array[i] -= seed_array[1+(i+30)%55]` for `i = 1..55`, with the same negative adjustment.
4. `inext = 0`, `inextp = 21`.

`InternalSample` does `ret = seed_array[inext] - seed_array[inextp]` after incrementing both indices into `1..=55`. If `ret == MBIG`, the result is `MBIG-1`. If `ret < 0`, add `MBIG`. The value stored and returned is that adjusted result. `Next()` returns it as an `int`. `Sample` converts it to a double and multiplies by the scale above. Every `Next()` result is in `0..=2147483646`.

`Next(max)` returns `(int)(Sample() * max)`, truncating toward zero. `Next(min, max)` uses that against `(long)max - (long)min` when the range fits in a positive `int`. Wider ranges call `GetSampleForLargeRange`: the first sample is the magnitude, the second sample's low bit chooses the sign (even negates), then `(result + 2147483646) / 4294967293`. The product is truncated toward zero and added to `min` with a 32-bit add.

Not ported: the parameterless constructor and the global seed generation, which draw from a source that cannot be reproduced.

### `DotNetRandom::new(1).next()` first 8

```
534011718
237820880
1002897798
1657007234
1412011072
929393559
760389092
2026928803
```

`new(-1)` produces the same stream (`Abs`). `new(int.MinValue).next()` is `1559595546`, not the `Abs` stream. First `next_double()` for seed 1 has bits `0x3FCFD45F463FA8BF`.

## xxHash32

The game's hash is xxHash32 over little-endian `int` words, keyed by a seed. It has a single-`int` form, an `int[]` form and further forms over bytes and `uint` arrays that share the same round function. Only the `int` and `int[]` forms are exported (`XxHash::hash_i32`, `XxHash::hash_i32s`).

Primes:

- `PRIME32_1 = 2654435761` (`0x9E3779B1`)
- `PRIME32_2 = 2246822519` (`0x85EBCA77`)
- `PRIME32_3 = 3266489917` (`0xC2B2AE3D`)
- `PRIME32_4 = 668265263` (`0x27D4EB2F`)
- `PRIME32_5 = 374761393` (`0x165667B1`)

The round function is the xxHash32 round: `acc += input * PRIME32_2; rotate left 13; acc *= PRIME32_1`.

`hash_i32` is xxHash32 of one little-endian word: `h = seed + value*PRIME32_3 + PRIME32_5 + 4`, rotate left 17, `* PRIME32_4`, then the avalanche (`>>15`, `*PRIME32_2`, `>>13`, `*PRIME32_3`, `>>16`).

`hash_i32s` is xxHash32 of `length*4` bytes, each `int` as a little-endian word. Under 4 ints: `seed + PRIME32_5 + byte_len`, then one 17/PRIME32_4 mix per int. Four or more: four lanes `seed+P1+P2`, `seed+P2`, `seed`, `seed-P1`, stripe rounds, merge rotations 1/7/12/18, add `byte_len`, then the same tail. The game throws on a null array. `hash_i32s(&[])` is the zero-length hash (`seed 0` gives `0x02CC5D05`). `hash_i32(v)` equals `hash_i32s(&[v])`.

## Random helpers

| Rust | Behaviour |
| --- | --- |
| `value` | `(float)NextDouble()` |
| `range_f32` | `(float)((double)min + (double)(float)(max-min) * NextDouble())` |
| `range_i32_max` | `Next(max)` |
| `range_i32` | `Next(min, max)` |

In `range_f32` the subtraction happens in `f32` before the widen, so the span is an `f32`.

## Floor and round to tens

`floor_to_10` is signed `n/10*10`. Truncation is toward zero.

`round_to_10` sets `base = floor_to_10(n)`. It returns `base + 10` when `n - base >= 5`, otherwise `base`. The `base + 10` is a 32-bit add, so `round_to_10(int.MaxValue)` is `-2147483646`, not `2147483650`.

`floor_to_10` matches truncating division for the vectors below and for `int.MinValue` / `int.MaxValue`. Negative `round_to_10` never steps, because `n - base` is in `-9..=0`.

| `n` | `floor_to_10` | `round_to_10` |
| --- | --- | --- |
| 0 | 0 | 0 |
| 4 | 0 | 0 |
| 5 | 0 | 10 |
| 9 | 0 | 10 |
| 10 | 10 | 10 |
| 14 | 10 | 10 |
| 15 | 10 | 20 |
| -1 | 0 | 0 |
| -15 | -10 | -10 |

## What is checked

- The seeding index recurrence `(21*i)%55` for `i = 1..54` and the shuffle index `1+(i+30)%55` for `i = 1..55`.
- `1.0/2147483647` has the same bits as the `Sample` multiplier.
- `floor_to_10` matches truncating `n/10*10` on the vectors above and the int32 extremes.
- `round_to_10` matches the formula above, including the `int.MaxValue` wrap.
- Fixed-seed tests in `rng.rs`: the seed-1 `Next` stream, `Next(10)`, `Next(3,7)`, full-range `Next(int.MinValue, int.MaxValue)`, `NextDouble` bits, `value` / `range_f32` bits, and xxHash32 empty-input `0x02CC5D05` plus one-word, short, and striped `int` buffers.
- `new(1)` first `Next()` is `534011718`, the known .NET Framework / .NET 5 Knuth `Random` value.

## What is not checked

- Argument-exception message strings.
- The unported constructors and global seed generation.
