# Map contents

Implemented in `src/map/contents.rs`. The .NET `System.Random` generator and the game's xxHash32 hash are `crate::rng::DotNetRandom` and `crate::rng::XxHash`. This module does not reimplement them.

`Tile.improvement == 0` and the city/capital/ruin flags stand in for a missing improvement object. A non-zero `improvement` wins over the flags. `resource == 0` is a missing resource object. `owner == 0` is the unowned byte the game writes on post-terrain cities.

## Ported

| Function | Notes |
| --- | --- |
| `count_score_for_turf_tile` | See constants below |
| `count_turf_score` | Sum of tile scores |
| `inequality_from_scores` / `get_inequality` | `max / min`, floor `0.001` when `min <= 0`. Empty scores stay at `±f32::MAX` and return `-1` |
| `create_turfs` | Includes the neighbour expansion and the lowest-cost turf tile pop |
| `is_valid_city_location` | Not water, edge distance `> 0`, no improvement in Chebyshev radius 2 |
| `is_good_city_location` | Valid, edge distance `>= 2`, and neither axis distance is exactly 3 |
| `is_valid_capital_location` | Good-city edge rules, then not near a capital |
| `is_capital_position_possible` | Edge `>= 2`, clear radius 2, not near a capital. Does not reject water |
| `get_all_valid_city_spot_indices` | Tile order |
| `valid_ruin_location` | No city or ruin in Chebyshev radius 2 |
| `generate_pre_terrain_cities` | Removes the indices near each pick |
| `add_post_terrain_cities` | Field, owner 0, city improvement, resource cleared |
| `add_ruins` | Uses the game's tile shuffle |
| `add_starfish` |  |
| `add_resource` | `XXHash` via `hash_i32s` |
| `add_resources` | Default generation only |
| `try_add_capitals_to_contents_tight` | Scan only. See partial note |
| `try_add_capitals_to_continents` | Includes the per-continent capital placement, the best city coordinates search and the emergency city position |
| `reassign` inside that function | Only when a west player id is passed |
| `set_tile_as_capital` | Name, skin, city level, explore skipped. See below |
| `get_starting_resources_for_player` | Includes the resource-for-tech lookup |
| `add_starting_resources_to_capital` | Does not call the singular starting-resource step |

The listed functions also rely on helpers that follow the game: the distance to the edge (overall, on X and on Y), the area around a tile, the water, land, improvement and resource tests, the near-city, near-ruin and near-capital tests, the four map corners, the lost-city location test, the hash-based `value` / `range` draws (see `hash.rs`), and the per-continent modifier lookup.

## How capitals are assigned to continents

`try_add_capitals_to_continents` skips nature (`id == 255`). Every other player takes the next continent and the cursor wraps to 0 at `populated_continents`. `Map` has no continent list, so the caller passes tile-index lists. Those lists are the stand-in for each tile's continent.

The best city coordinates search only considers unowned city improvements on that continent. Score starts at 20 and drops to the minimum Chebyshev distance to a capital already placed. The tile is rejected when that distance is below 3, or below 4 when `(width - 1) / ceil(sqrt(player_count))` is at least 5. Two to six water neighbors add 1; seven or eight add 1 and then -1. Edge distance must be at least 2. The highest score wins. `(0, 0)` means not found and triggers the emergency city position: a random valid capital spot, or the first `is_capital_position_possible` tile.

A second pass clears each capital's owner and places that player again on the continent that contains the tile. The west placement reassignment then runs only when a west player id is passed (the west map placement user id is not on `PlayerState`). It swaps that player's capital with the capital whose `y - x` is largest. The game's `width` argument is unused.

`set_tile_as_capital` then sets `is_capital`, `owner`, city `improvement`, field terrain, `start_tile`, and `climate`. City name and skin are not stored on `Tile` and are skipped. The city-level-3 bonus only changes an improvement level that `Tile` does not have. The spores bonus (`2`) sets fungi (`37`) on field neighbors in the radius-1 ring. The city's rule area and the exploration from the tile are skipped.

`try_add_capitals_to_contents_tight` is still the index scan only. It does not call reassignment or `set_tile_as_capital`. Tests do not call it.

## How starting resources are chosen

`get_starting_resources_for_player` takes the caller's `GameLogicData`, which selects the game version's data. Resource keys are JSON ids resolved to `ResourceData.idx`.

If `starting_resource` is non-empty, those idxs are returned in order. Otherwise only the last `starting_tech` is used. The resource-for-tech lookup walks that tech's `improvement_unlocks`, applies `improvement_overrides`, and returns the first terrain requirement whose resource idx is not 0. If the last tech has none, `TechData` with `idx == 0` (`Basic`) supplies one resource, picked with `Range(0, count)` from its `tech_unlocks` in list order.

`add_starting_resources_to_capital` does not call the singular starting-resource step. The minimum resource count setting is unused. The radius-1 ring around the start tile, excluding the center, is shuffled. A neighbor that already has one of those resource idxs drops that resource and itself. Remaining resources are placed in three passes: a neighbor whose terrain matches a random requirement, then the first land neighbor (terrain rewritten to a random requirement), then neighbor 0 (terrain rewritten the same way). An empty id list places nothing.

## Blocked

| Function | Why |
| --- | --- |
| Singular starting-resource step | Not called by `add_starting_resources_to_capital` |
| Tutorial resource placement | Tutorial branch of resource placement when `map_generation_type == 1` |

`add_resources` implements only default map generation.

## Fairness constants

The game's names for the constants in the turf tile score:

| Constant | Value | Use |
| --- | --- | --- |
| `SCORE_PER_CITY` | `100` | Added after `-90 * factor` |
| `CITY_DISTANCE_PENALTY` | `90` | Used as `-90` |
| `VERY_CLOSE_CITY_BONUS` | `40` | `cost == 3` |
| `CLOSE_CITY_BONUS` | `20` | `cost == 4` |
| `SCORE_PER_RUIN` | `5` | |
| `RUIN_DISTANCE_PENALTY` | `3` | Used as `-3` |
| `CLOSE_RUIN_BONUS` | `5` | Extra `+5` when `cost < 5` |
| `SCORE_PER_LAND_TILE` | `1.5` | Land adds `1.5 - factor` |
| `LAND_TILE_DISTANCE_PENALTY` | `1` | The `factor` subtracted on land |
| `SCORE_PER_TILE` | `0.1` | Added for cities, ruins, and bare tiles |

`factor = clamp((cost - 3) / (width * 0.8 - 3), 0, 1)`. The `-3` bias is the same float as `RUIN_DISTANCE_PENALTY`. Land is field, mountain, forest, or ice only.

Water steps in `create_turfs`: ordinary water costs 4. Leaving a city onto water costs 8 when that city's coordinates equal some `PlayerState.start_tile`, otherwise 16. The `1000` written before that player scan is replaced before the neighbor is queued. Mountain costs 4. Other non-water costs 1. The game throws if the visited set size is not `width²`; this port returns the turfs it built.

The inequality walks each player's turf list and keeps the smallest and largest scores. Denominator is `min` when `min > 0`, else `0.001`.

## Equality retries

The retry loop is in the map generator entry point (`generate`), not in the internal generator (`generate_internal`).

- If the `num_iterations` argument is `> 0`, that is the attempt count. Otherwise the count is `settings.equality_iterations`.
- If that count is `<= 0`, the loop is skipped.
- Each attempt draws a seed from an unseeded random source in `[0, int.MaxValue)`, runs the internal generator, then the inequality score.
- A result `< -999` is logged and does not compete. Unless the attempt count is 1, up to 15 such failures can extend the budget.
- Among results `>= -999`, the lowest inequality is kept.
- The loop stops early when `inequality < settings.equality_limit`. Otherwise it continues.

`equality_iterations` and `equality_limit` are the fields on `MapGeneratorSettings`. This module does not run the retry.

## Cities, ruins, resources

Capital exclusion radius is 3 when `(width - 1) / ceil(sqrt(player_count)) >= 5`, otherwise 2. A non-positive player count faults the integer division in the game; these checks return false instead. A capital for the near-capital test is a city whose owner byte is non-zero.

The pre-terrain city step fills indices with `x` and `y` in `1..size-1`, then deletes the Chebyshev radius-2 square around each existing city and around each new pick. Picks use `Next(0, count)`.

The post-terrain city step does nothing when `max_city_count <= 0`. Otherwise it samples valid spots the same way and writes a field city with owner 0.

`add_ruins` shuffles tile order with `for i in 0..len-1 { swap(i, Next(i, len)) }`. It skips the four map corners: `(width-1, height-1)`, `(0, 0)`, `(0, height-1)`, `(width-1, 0)`. A tile is rejected when it is within radius 1 of a city, radius 2 of a ruin, or radius 2 of a capital. Water must also pass the lost city location test (ocean, owner 0, edge distance `> 0`, no city or ruin in radius 2 excluding the center). The loop stops when `placed > amount`, so a non-negative amount places `amount + 1` ruins.

`add_starfish` returns immediately when `amount <= 0`. It samples with replacement at most `tile_count` times and stops once the placed count is not strictly below `amount`.

`add_resource` hashes `[x, y, resource_index]` and `[x, y, resource_count]` through `XxHash::hash_i32s`. The hash-based value is `(hash as f32) * (1/2^32)`, and the hash-based range is `min + hash % (max - min)` for a positive span (`range_from_hash`). A null `fixed` hash is the xxHash32 of `trunc(value() * 1000)`, where `value()` is the random-source draw from `rng.rs`. Starfish's continent modifier is 0, so `add_resource` never selects it.

`add_resources` phase 1 uses full `richness` on each city's radius-1 ring. Phase 2 divides richness by 3 once and walks radius 2. Both rings exclude the city tile. The `value()` roll happens even for a tile already visited, except phase 2 skips the roll when the tile already has an improvement. `continents[tile_index] == None` is a tile with no continent.

## Tests

`cargo test --lib map::contents -- --test-threads=1` runs the pure checks: turf score bit patterns, the inequality ratio, city and capital predicates, turf expansion, and `add_resource` with a fixed `XxHash`. RNG-driven placement is implemented and not covered by those tests.
