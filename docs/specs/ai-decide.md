# AI decisions and difficulty

This file records how the retail AI picks a move and how difficulty changes it. Float constants are the `f32` values the game uses.

Bot difficulty values: Easy 0, Normal 1, Hard 2, Crazy 3, Frozen 4.

## What each difficulty changes

Difficulty is not stored on `PlayerState`. The difficulty-to-handicap conversion writes `handicap`, and the AI reads `handicap`.

| Difficulty | Value | Handicap | Stupidity if auto-play | Move search | Score bonus (`bonus/5` before opponent terms) |
| --- | --- | --- | --- | --- | --- |
| Easy | 0 | 0 | 2 | runs | 0 |
| Normal | 1 | 1 | 1 | runs | 0.2 |
| Hard | 2 | 2 | 0 | runs | 0.4 |
| Crazy | 3 | 4 | 0 | runs | 0.8 |
| Frozen | 4 | -1 | 3 | does not run | -0.2 |

Handicap 3 is not produced by the difficulty-to-handicap conversion. Handicap 3 converts back to Normal. Handicap above 4 or below -1 also converts to Normal. A difficulty outside 0..=4 (unsigned compare) has handicap 0 and bonus 0.

### Frozen does end the turn

When the AI is enabled and the game state is not the final turn (3), the move search compares `player.handicap` with the handicap of Frozen, which is -1. If they are equal, the search goes straight to the end-of-turn choice and does not build map data, analyse, or pick a command.

The end-of-turn choice builds an end-match command when the state is the final turn, otherwise an end-turn command. So a Frozen bot (`handicap == -1`) is told to end the turn. On the final turn every bot, Frozen or not, is told to end the match.

The AI starts with logging off and `ai_enabled` true. With that default, the only handicap that skips the search is -1. If `ai_enabled` is later cleared, the search ends immediately for every handicap.

Frozen's stupidity of 3 is real (`max(0, 2 - (-1))`) but the move search never reaches the stupid factor for that handicap. The noise would apply only if some other caller used it.

### Easy, Normal, Hard, Crazy

These four do not have their own branches in the move search, tile commands, command pick, or situation analysis. They differ only by the handicap they store:

- Stupidity (`stupidity`) is `max(0, 2 - handicap)` when auto-play is on, else 0. Easy is 2, Normal is 1, Hard and Crazy are 0.
- The stupid factor is `stupidity * (h - 0.5) * 50`, where `h` is the hash scaled to [0, 1). Hard and Crazy add 0. Easy can add about ±50, Normal about ±25.
- `h` is the seeded hash of three words, zero-extended, converted to `f32`, times `2^-32`. The first word is `current_command * 1000`. The hash itself is in [`crate::rng`]; it is not reimplemented here.
- The difficulty bonus multiplier is not called from the move functions. It changes the score bonus, not which command is chosen. See below.

Logging only writes a log line. It does not change the command.

## Handicap conversion

Difficulty to handicap: a difficulty above 4 (unsigned) gives 0; otherwise the table `0, 1, 2, 4, -1` indexed by difficulty.

Handicap to difficulty: `index = handicap + 1` (wrapping). The default result is Normal (1). If `index` (unsigned) is `<= 5`, the result is the table `4, 0, 1, 2, 1, 3` at that index (Frozen, Easy, Normal, Hard, Normal, Crazy).

## Score bonus multiplier

The bonus floats match the handicap table: `0, 1, 2, 4, -1`. Out of range stays 0.

`version > 103`:

```
base = bonus / 5
if opponent_count < 2: return base
else return (float)(base + sum_{i=1}^{opponent_count-1} (0.2f / (float)i))
```

The sum is `f32` division accumulated in `f64`, then narrowed to `f32`. When `opponent_count == 2` the single term divides by `1.0`, which is the same as `i = 1`.

`version <= 103`:

```
n = max(1, opponent_count)
x = ln(n) / (double)11.4f + (double)(bonus / 5)
x = round-half-to-even(x * 100)
return (float)x / 100
```

`11.4f` is promoted to `f64` before the division. The rounding splits `x * 100` into integer and fractional parts and breaks an exact tie by the low bit of the integer part (so ties go to even), with 0.5 and -0.5 as the tie values.

## Stupid factor

```
hash = game_state.random_hash         # seeded from the game seed
unit = hash_to_unit(hash, [current_command * 1000, rng_counter_1, rng_counter_2])
return stupidity * (unit + -0.5) * 50
```

`current_command` is an unsigned 16-bit field of the game state.

Callers do not always pass the default zeros:

- The command pick uses `rng_counter_1 = 101 + index`, `rng_counter_2 = 0`.
- Situation analysis uses `rng_counter_1 = 501 + index` for missions, then keeps incrementing that counter through the hit list. `rng_counter_2 = 0`.

## Move search

After the AI-enabled check and the Frozen/final-turn exits:

1. The player map data fills three lists from the map (the player's units, empire tiles and city tiles) and stores them in the player's AI state.
2. Situation analysis is requested. It writes the current turn into `ai_state.missions_for_turn` and always runs the analysis. There is no "already analysed" skip in this build.
3. If logging is on, it logs the three list counts.
4. First pass over units: range above 1, or range at most 1 and the Surprise ability (31). Then "can perform any action". Then the best unit move. The first non-null command is returned.
5. Second pass over the same list: "can perform any action" then the best unit move again, including units the first pass already tried.
6. The tile commands, with `specific_command`.
7. If that is null, the end-of-turn choice.

Range is the unit data range. The ability test is membership of the ability ids. "Can perform any action" is true when "can perform any ability" is, when the unit has not moved, or when a moved unit still has attack options. "Can be promoted" is an argument. The best unit move builds stay (`current * 0.8`), moves (position rating `+ 5`, Carry on water subtracts `cost * 10`), and attacks, then runs the command pick. In `src/ai/decide.rs` the unit action options step is not called, and the position rating, the battle results, aggression and movement cost stay arguments; the engine's move search (`Engine::ai_get_move` in `src/engine/ai_move.rs`) supplies them and does add the unit action options (`add_unit_action_options`).

## End of turn

Final turn (state 3): an end-match command for the player. Otherwise an end-turn command for the player. No difficulty is read.

## Tile commands

1. Allocate a tech-need dictionary and a list of scored commands.
2. Compute the tech needs for the player, then add the possible research commands. The tech-need computation walks the map.
3. Write `unit_need` (below).
4. Add the possible train-unit commands, improvement commands, diplomacy commands, and road-building commands (for the city tiles).
5. `frozen_tile_count` is the number of frozen tiles owned by the player, stored in `ai_state.frozen_tile_count`. This is a map query, not the Frozen difficulty.
6. If `specific_command != 0`, keep only the commands whose command type equals that value.
7. Run the command pick.

`unit_need` is stored in the player's AI state:

```
pressure = max(0, (20 * cities + 25 - explored_tiles) / 10)
x = (pressure + hit_list.Count + missions.Count - units.Count) / 3 + 1
unit_need = x < 0 ? x+x : x
```

`cities` is the player's city count. `explored_tiles` is `ai_state.explored_tiles`. Divisors are 10 and 3. The `20 * cities + 25` is an integer before the float conversion.

## Command pick

Returns null when the list is null or empty.

Otherwise, for each index `i`, add the stupid factor at `(101 + i, 0)` to that command's score. Sort descending by score (higher first). Then read index 0 only. If that command is null or fails its validity check against the game state, return null. Do not try the next command. Logging only logs.

## Situation analysis

Clears the AI state first. If `player.Id == 255` (`NATURE_PLAYER_ID`), return.

For each map tile, a tile the player has not explored is skipped. Otherwise `explored_tiles += 1`.

- Terrain 4 or 5 (Mountain, Forest) increments the rough counter.
- If the player can access the tile: take the area around it (radius 2 on mountain and 1 otherwise, diagonals allowed, centre excluded). Count neighbours that are not explored. If that count is at least 2, append a mission to a temporary list with type 0 and score `count * 5`.
- A Ruin improvement with no unit on the tile: append a mission to `player.missions` with type 0 and score 30.
- A City improvement: make the city missions. If the city can be upgraded, append the tile coordinates to `upgradeable_cities`.
- If the tile has a resource that is visible to the player: for each improvement for that resource with the Manual ability (18) that is unlocked, append another score-30 type-0 mission.
- If the tile has a unit: collect its military stats. If the unit owner is this player, each neighbour's `blocked_tiles` entry is increased by `2 * position rating`. Otherwise update the hit list for the unit.

After the scan:

- Sort the temporary explore missions descending by score and copy the first `3 * city_tile_count` of them onto `player.missions`, or 1 if `3 * city_tile_count < 2`.
- `rough_terrain = (float)rough_count / (float)explored_count`. Zero explored tiles produce NaN.
- Adjust the military needs.
- Add stupid-factor noise to mission scores starting at y=501, then to hit-list scores with the counter continuing.
- Sort `player.missions` and the hit list descending by score.

The city missions, the hit-list update and the military needs adjustment are not described here; the engine has them in `analyze_situation` (`src/engine/ai.rs`) and `src/engine/ai_rate.rs`. The position rating is described in `ai-score.md`.

Both mission sorts and the hit-list sort put the higher score first.

## Constants checked and not used here

`LATE_GAME_PROGRESS_LIMIT` (0.8), `BREAK_PEACE_LIMIT` (0), and `HIGH_AGGRESSION` (5000) are AI class constants. None of the move search, end-of-turn choice, tile commands, command pick, situation analysis or stupid factor loads them. They belong to the diplomacy and progress functions.

## Ported vs blocked

Ported in `src/ai/decide.rs` (tested): handicap conversion both ways, difficulty bonus multiplier (both version paths), stupidity, hash unit-interval scale, stupid factor arithmetic, hash argument packing, immediate move-search exit, end-of-turn kind, priority-pass predicate, specific-command filter, `unit_need`, explore mission score, explore keep count, explore radius, rough-terrain ratio, range, ability test, can-attack, can-move, can-perform-any-action, the best unit move (stay, move, attack), the two-pass unit loop, the tile commands, and the command pick using the basic validation.

The move search on a version-123 board with one warrior and one city returns that unit's move when the position rating is supplied as 0. A Frozen handicap (`-1`) still returns an end-turn command and does not call the search callbacks. That result is this port's score, not a captured retail replay.

Still arguments of `src/ai/decide.rs`, not reimplemented there (the engine's own AI in `src/engine/ai*.rs` supplies all of them): the seeded hash, the world walk of the position rating, the battle results, aggression, the opponent city test, movement cost, "can be promoted", the tail of "can perform any ability" after the health gate, the unit action options, the tech-needs computation, and the tile walk of situation analysis. The pending command trigger is treated as empty in this module because the board has no trigger list; the engine answers triggers in `ai_trigger_command` (`src/engine/ai_move.rs`).

Where the engine's AI differs from the module: it scores diplomacy and road building with its own code (`src/engine/ai_diplomacy.rs`, `src/engine/ai_improve.rs`), and does not use `specific_command`. See [the AI doc](../ai.md).
