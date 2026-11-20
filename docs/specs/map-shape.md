# Map shape

Implemented in `src/map/shape.rs` and `src/map/generate.rs`. Constants below are the game's own values.

## Map generation entry point

This build does not use the older pre-naval generator. The game generates a map as follows:

1. Read the game settings. Map width and height are both the map size setting.
2. Create an empty map of that size and store it on the game state.
3. Build the map generator settings from the game settings.
4. Create the map generator, with an unseeded .NET `System.Random` as its generator.
5. Run the generator with no callback and `num_iterations = -1`.
6. For each player, look up the tribe and its starting unit and train that unit. The trained unit's flag word is set to 0.

The generator entry point is `generate`. When `num_iterations <= 0` it uses `settings.equality_iterations` (50 from `create_from_preset`). A count that is still `<= 0` returns a blank map and does not call `generate_internal`. Each attempt draws `DotNetRandom::next_range(0, 0x7FFFFFFF)` from a generator seeded by the caller's `seed`, then passes that int to `generate_internal` and calls `get_inequality`. The game draws that int from an unseeded system random source; the `DotNetRandom` seed is what makes a run repeatable here. The game seed is not the argument of the internal generator. Scores `< -999` do not compete. The extension counter starts at 15 and an extra attempt is added only while the decremented counter is still positive. The loop stops when the current score is `< equality_limit`. If the winning seed is not the last attempt, `generate_internal` runs once more with that seed. A single attempt always keeps that attempt. The game's final player scoring step at the end of generation is not part of `generate`; `Engine::new_game` scores the players during setup.

## The internal generator

Direct call order of the game. `generate_internal` runs the ported calls below; the table "Steps of the list above that `generate_internal` does not do" says which steps it leaves out and where setup covers them. Logging and stopwatch calls sit between the steps and do not change map data.

1. Clear the game state's action log: set count to 0 and clear the array when the old count was positive.
2. For each player, replace the task list with a new empty list.
3. Create the generator's random: a .NET `System.Random` seeded with `seed`.
4. Set the map generation type: Tutorial (1) when the rules game mode is the tutorial mode (7), otherwise Default (0).
5. Start a stopwatch and log the map width and height.
6. Clear the map data.
7. Start an empty list of land indices.
8. Terrain source:
   - If the game version is `<= 98`: generate from noise.
   - If the version is `> 98` and the map type is `Continents` (3) or `Pangea` (6): try to generate from islands. On false: log a warning, clear the land indices, clear the map data, then generate from noise.
   - If the version is `> 98` and the preset is anything else: generate from noise.
9. Add terrain, using the players, the version, the settings and the land indices.
10. Make ocean. The "convert shallows" bool is 1 when `settings.shallow_percent_of_water == 0`.
11. If the version is `>= 99`: add the lighthouse improvements.
12. Add resources, with the settings' `richness`.
13. For each player whose id is not `255` (Nature):
    - Get the player's starting resources.
    - If the version is `> 113` and the generation type is Tutorial: append the resource with key `1` to that list three times, then add the starting resources to the capital.
    - If the version is `> 113` and it is not the tutorial: add the starting resources to the capital with the list unchanged.
    - If the version is `<= 113` and it is the tutorial: add the singular starting resource (key `1`, minimum count 3).
    - If the version is `<= 113`: add the singular starting resource (`list[0]`, minimum count 2). This path does not call the plural step.
    - Then, still per player: look up the game data for the version and the player's tribe. Take the area around the player's start tile (radius 1, diagonals, centre excluded). If the player's climate is 0, log a warning and set the climate to the tribe. Each tile in that area gets the player's climate and skin.
14. Make ocean a second time, with the same bool.
15. For each player with the Flooded tribe ability (6): take the capital tile from the start tile. For every map tile with the same climate and skin whose terrain is `Field` (3) or `Forest` (5), flood the tile for that player.
16. Add ruins: `tile_count / 40`, where `tile_count` is the number of map tiles. This is integer division by 40, truncating.
17. If the version is `>= 99`: add starfish: `tile_count * wetness / 25f`.
18. Prepare the alien climates.
19. If the version is `>= 104`: for each of the four map corners, resolve the tile and replace its explorer list with a new empty list.
20. Generate the shore lines.
21. Stop the stopwatch, log the elapsed milliseconds, return the map.

Climate assignment, continent growth, noise generation, capital positions and suburbs are not direct steps of the internal generator. They sit inside the terrain and city functions above.

## Orchestrator

`generate(game_version, seed, size, preset, players, num_iterations) -> Map` and `generate_internal(game_version, seed, map, settings, players, rules_game_mode, west_player)` live in `src/map/generate.rs`, declared from `src/map/mod.rs`. Width and height are both `MapSize::width`, as the game reads the map size setting.

`rules_game_mode` is the rules game mode. `7` is tutorial (generation type 1). `west_player` is the player id for the west map placement user; `None` skips the swap. `generate` passes mode `0` and `None` because neither value is on `PlayerState`.

`tries_island_generator` is the version `> 98` and Continents/Pangea gate. Version 98 does not call `try_generate_map_from_islands`.

### Steps that run

| Step | Call |
| --- | --- |
| Retry loop | `generate`, then `get_inequality` |
| Seeded .NET random | `DotNetRandom::new(seed)` |
| Clear map data | reset each `Tile`, keep index and coordinates |
| Terrain source | `try_generate_map_from_islands`, and on false clear then `generate_map_from_noise`. Version `<= 98` and every other preset call `generate_map_from_noise` only |
| Capital start tile | copied from the capital tile. The noise port does not write `PlayerState.start_tile`; island placement already does |
| Add terrain | `add_terrain`. A `continent_of` index past the list `add_climates` left behind is treated as no continent so the call cannot read off the end |
| Make ocean | twice, `should_convert_shallows` when `shallow_percent_of_water == 0` |
| Add resources | `add_resources` for Default generation. Resource rows are the embedded v28 table sorted by `idx`. A tile with a continent uses the continent constructor's weights: crop/fish/fruit/game `0.5`, metal `0x3F4CCCCD`, whale/spores/aquacrop `0`. `add_climates` applies the per-tribe resource modifiers to the continent (`set_player_modifiers`) |
| Starting resources, `version > 113` | `get_starting_resources_for_player`, then `add_starting_resources_to_capital`. Tutorial appends resource `idx` 1 three times first |
| Add ruins | `add_ruins(tile_count / 40)` |
| Add starfish | `version >= 99`, amount `tile_count * wetness / 25` |

The game's lookup of the game data version for a game version is not called. The embedded v28 table is the `GameLogicData` the existing map functions already load. Version 98 would use data version 16 in the game.

### Steps of the list above that `generate_internal` does not do

Status checked against the code. "Setup" means `Engine::new_game` and `Engine::finish_setup` (`src/engine/setup.rs`), which run after `map::generate::generate` returns.

| Step | Status |
| --- | --- |
| 1. Clear the action log | Not done. `GameState` has no action log; a new state starts at command 0 |
| 2. Replace the player task lists | Not a generator step. `GameState.tasks` starts empty and `Engine::enable_task` fills it as tasks are enabled |
| 11. Add lighthouse improvements | Done: `add_lighthouses` (`src/map/contents.rs`), called for `version >= 99` before `add_resources` |
| Tutorial resource placement | Not done. `add_resources` is skipped when `rules_game_mode == 7`, and `generate` always passes mode 0, so `Engine::new_game` never reaches it |
| 13. Singular starting-resource step (`version <= 113`) | Not done. `place_starting_resources` draws the starting resources and then skips the capital placement for `version <= 113` |
| 13. Climate paint | Done: `paint_owning_climate` gives each player's radius-1 ring around the start tile the player's tribe climate. It runs last, after ruins and starfish, not at step 13. The player's own climate field is not stored, so the tribe is always used |
| 13. Skin paint | Not done. `Tile.skin` exists but only the save loaders write it |
| 15. Flooded tribe ability | Not done. Tile effects exist (`Tile.effects`) but generation sets none |
| 18. Alien climate preparation | Not a separate step. The alien handling inside `add_climates` is done (`has_alien_climate`, `requires_fallback_climate`) |
| 19. Lighthouse corner exploration | Equivalent in setup: `Engine::init_fog` gives every tile an empty explorer list (`GameState.fog`) for every version |
| 20. Shore lines | Not done. `Tile` has no shore-line field |
| Training the starting units | Done in setup: `finish_setup` explores radius 2 around the capital and spawns the tribe's starting unit with `Engine::spawn_unit` |
| Player scoring | Done in setup: `Engine::refresh_scores` |

Verification against real games is recorded at the end.

## Ported

| Game function | Rust |
| --- | --- |
| Generate | `generate` |
| Internal generator | `generate_internal` (partial; see the status table above) |
| Settings from a preset | `create_from_preset` |
| Minimum capital distance | `min_capital_distance` |
| Domain size | `domain_size` |
| Capital positions | `generate_player_capital_positions` |
| Add distance to probability table | `add_distance_to_probability_table` |
| Probability in range | `calculate_probability_in_range` |
| Index for probability value in range | `index_for_probability_value_in_range` |
| Shuffle of coordinates | `shuffle_coords` |
| Shuffle of tiles | `shuffle_tiles` |

Shared helpers used by capitals:

- From index: `x = index % width`, `y = index / width` (toward zero).
- To index: `y * width + x`.
- Chebyshev distance: `max(abs(dx), abs(dy))` of the coordinate difference.
- The game's integer range draw is just .NET's `Next(min, max)`. Rust calls `DotNetRandom::next_range(min, max)`. It is not reimplemented in `shape.rs`.

### Preset overrides

The defaults are applied first (wetness `0x3EE66666` ≈ 0.45, richness 1, smooth 2, pre-density `0x3E99999A` ≈ 0.3, post-density 1, suburbs 1..2, surrounding 0.5, shallow 0, equality iterations 50, equality limit `0x3F8147AE` ≈ 1.01). The per-preset values then overwrite:

| Preset | Value | Changes |
| --- | --- | --- |
| None | 0 | wetness 0.5, surrounding 1, smooth 1 |
| Dryland | 1 | wetness 0, smooth 0, pre-density 0, post-density 1, suburbs 0..0 |
| Lakes | 2 | wetness `0x3E99999A` (0.3), smooth 2, surrounding 1 |
| Continents | 3 | same as None |
| Archipelago | 4 | wetness `0x3F266666` (0.65), smooth 0 |
| WaterWorld | 5 | wetness `0x3F59999A` (0.85), smooth 2, pre-density `0x3DCCCCCD` (0.1), suburbs 0..0 |
| Pangea | 6 | wetness 0.5, surrounding 0, smooth 1 |

`MapGeneratorSettings` Rust `Default` still has equality iterations 0 and limit 0. That is the struct default in `types.rs`, not `create_from_preset`.

### Capitals

`side = (int)ceil(sqrt(player_count))`.

`cell = width / side`, `remainder = width % side`. If `cell <= 2`, throw. Otherwise:

1. Build cell ids `0 .. side*side - 1`.
2. Zero an `int[width * width]` probability table.
3. For every `(col, row)` in the side-by-side grid, stamp a center at
   `cell * col + adjust(remainder, col) + cell / 2` (same for row).
   `adjust(r, i) = min(r, max(1, min(r, i) - 1))`.
   The distance weighting uses `domain_size = cell` (not `domain_size()`).
4. For each player, in order:
   - `next_range(0, cells.len())`, then remove that id from the list.
   - Convert `id` to a coordinate with `from_index` (the grid width is `side`, not the map width).
   - Origin is `cell * grid_coord + adjust(remainder, grid_coord)`.
   - `inset = cell < 5 ? 1 : 2`.
   - Range is `x,y` from `max(2, origin + inset)` inclusive to `min(width - 2, origin + cell - inset)` exclusive.
   - `total` is the probability sum over that range.
   - `next_range(0, total)`.
   - The index lookup returns the tile index whose running `max(1, p)` sum first exceeds that value.

The distance weighting: for each index, `cube = max(1, domain_size / 2 - chebyshev)^3`. If the slot is 0, store `cube`; otherwise store `max(slot, cube)`.

The minimum capital distance uses `(width - 1) / side`, then 4 if that is `>= 5`, else 3 (the game's `DESIRED_DOMAIN_SIZE` and `MINIMUM_DOMAIN_SIZE`). The domain size is `width / side` with no clamp.

Both shuffles are forward Fisher-Yates: for `i` in `0 .. count-1`, `j = next_range(i, count)` and swap `i` with `j`. Count is re-read each iteration. Count 0 and 1 return without a draw.

## Suburbs

`suburb_count == 0` or `player_count <= 0` returns an empty list and does not draw from `Random`. A negative `suburb_count` hits a null list in the game; the port panics.

Otherwise, for each suburb round `0 .. suburb_count - 1` and each player `0 .. player_count - 1`:

1. Let the capital coordinate be `from_index(capital_indices[player], width)`.
2. If `width * 0.6f <= 3f` (the `f32` values `0x3F19999A` and `0x40400000`), place nothing. Otherwise scan Chebyshev rings of radius 3, 4, ... while `(radius as f32) < width * 0.6f`. Each radius clears the candidate list. The perimeter is the square from `(cx - radius, cy - radius)` to `(cx + radius, cy + radius)`: full top and bottom rows, and only the left and right columns in between. A cell is kept only when `0 < x < width - 1` and `0 < y < height - 1`. Hitting `y >= height - 1` ends that radius early.
3. Reject a perimeter cell when the within-range test (range 2) is true for the capital list, the suburbs already placed, or this player's suburbs. That helper is Chebyshev `<= range` against any index in the list.
4. The first radius with any candidate draws `next_range(0, candidate_count)`, appends that index to the player's list and to the returned list, then removes the indices near it (range 2) from the candidate list. The candidate list is discarded afterward. A tribe lookup between the pick and the removal is only the verbose log and is not ported.

After every round, each player whose list length is not `suburb_count` borrows up to `suburb_count` extra indexes (the loop is not "fill until full"). Each borrow is the strict-minimum Chebyshev neighbor of this capital among the returned suburbs and then the capital list, skipping this capital and indexes already on this player's list. Ties keep the earlier index. A borrow is appended only to the player list. If nothing qualifies, the game logs an error and stops borrowing.

Land is then linked for every player. The center is the sum of the capital coordinate and each of that player's suburb coordinates (including borrows), divided component-wise toward zero by `count + 1`. The land walk goes from that center to the capital and then to each suburb. The walk appends the current cell when it is missing, stops at equality or Manhattan distance 1 without appending the destination, and otherwise takes at most 1000 steps. Each step draws `next_range(1, 4)`: bit 0 moves X, bit 1 moves Y, toward the target. A zero sign on a moved axis draws `next_double`, converted to f32, and uses -1 when that float is `<= 0.5f`, else +1. Each axis is clamped with `max(0, v)` then `min(width - 1, v)`.

No callee of these functions is left unimplemented.

## Tests

`tests/map_seed.rs` and the unit test in `shape.rs` assert capital indexes `[211, 56, 61, 216, 51]` for seed `42`, `MapSize::Normal` (width 16), `MapPreset::Continents`, and 5 players. The crate test produced that vector through `DotNetRandom::next_range`. It is not a capture from the real game.

`fixed_seed_suburbs` calls `generate_suburbs` with seed `42`, a 16×16 map, those five capitals, and `suburb_count` 2. The returned indexes are `[165, 101, 106, 187, 97, 161, 141, 152, 235]`. That vector and the land-index list were produced by this port. They are not a capture from the real game.

`same_seed_matches_captured_map` stays ignored with `golden map capture not available yet`. Real-game verification is in `tests/map_capture.rs` (see the end of this file).

`generate_version_98_and_123_return_the_requested_size` calls `generate` for versions 98 and 123 and checks the map's width, height, and tile count. Version 98 does not take the island attempt (`tries_island_generator` is false). It does not compare terrain to the real game.


## Retail verification (Large, Continents, 6 players + Nature, version 123)

Live games were recorded with a harness that is not part of this repository. It logged the generator inputs, every .NET `Random` call made on the
generator thread, stage markers, the final tile dump and the fairness scores. Two of those captures are `tests/data/capture_a.txt` and
`capture_b.txt`, used by `tests/map_capture.rs`. For both, `generate_internal` reproduces **every tile** (terrain, climate,
owner, improvement, resource) of all 51 logged attempts, the fairness score of each attempt matches (capture b), and the
retry loop keeps the same attempt as the game.

Attempt seeds are **not** derived from the game state seed: the game draws them from an unseeded source, so the seed saved in a
game save is unrelated to the map. A map is a function of its attempt seed.

Corrections found by the traces:

- Clearing the map data makes every tile **Water** (terrain 1) with altitude -1, not terrain 0.
- The area helper iterates **x in the outer loop, y in the inner loop**. It matters wherever a random index is drawn.
- The player count is the player list length minus 1 (Nature, id 255, is the last entry and is not counted). The game's player list is
  ids `1..=n` in seat order, then Nature.
- `expand_continent`: when a seed has no neighbour scoring above 0 it is marked exhausted and the next shuffled tile is tried in
  the same call. The -10 penalty applies when a neighbouring tile shares x **or** y with the candidate. The continent size limit
  divides by the player count (6), not the list length.
- `add_city_to_continent` returns **false** when the city was placed by the forced-location fallback, and the caller's failure
  counter resets on success (consecutive failures end the loop).
- `add_climates` is a flood fill: each climate continent repeatedly claims a random free neighbour of a random one
  of its tiles (candidates: climate 0 and altitude >= 0, else climate 0). It assigns every tile its continent, which is why no
  tile is left without one (and so no ice). Assigning a player also sets per-tribe terrain and resource modifiers and, for version > 98, aquacrop 0.3.
- `altitude` is stored state: -1 after the map is cleared, set to 1 on continent seeds and claims and on capitals.
- The lighthouse step (`add_lighthouses`) runs before `add_resources`: improvement 47 on each corner, a land corner becomes Field.
- Capitals own their eight neighbours at generation time. After the last `make_ocean`, each player's radius-1 ring takes that
  player's climate.
- Fairness: `create_turfs` skips Nature; stepping from a land tile that is not a city onto water costs 1000.
- City names use separate .NET `Random` instances; they are not part of the map stream.

Later captures extend this to every preset and map size (`tests/map_capture.rs`, one test per file in `tests/data/`):

| Capture | Size | Preset | Players besides Nature |
| --- | --- | --- | --- |
| a, b | Large | Continents | 6 |
| c | Large | Dryland | 6 |
| d | Normal | Dryland | 6 |
| e | Normal | Lakes | 6 |
| f | Massive | Archipelago | 6 |
| g | Huge | Water World | 6 |
| h | Small | Pangea | 7 |
| i | Tiny | Dryland | 9 |
| j | Tiny | Continents (weekly challenge) | 2 |

Not verified against retail: player counts other than 2, 6, 7 and 9, and the presets on sizes other than those in the table (for example Lakes on Massive). Those go through the same code, but no capture backs them.
