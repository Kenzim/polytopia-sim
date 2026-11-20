# Map terrain

Implemented in `src/map/terrain.rs`, reproducing the game's map generator.

`Tile` has no altitude. The game's altitude update is recorded here and not stored.

## Terrain ids

| Id | Name |
| --- | --- |
| 0 | None |
| 1 | Water |
| 2 | Ocean |
| 3 | Field |
| 4 | Mountain |
| 5 | Forest |
| 6 | Ice |
| 7 | Wetland |
| 8 | Mangrove |

Water means Water or Ocean. The game's altitude for ids 1..=5 is -1, -2, 1, 2, 1. City improvement id is 1. `DEFAULT_CLIMATE` is 7 (Imperius).

## Ported

### `generate_noise`

`value(rng)` per cell. If the index is in `land_indices`, `sample * (1 - wetness) + wetness`. `value` is `(float)NextDouble()`.

### `smooth_noise`

For each iteration, each cell becomes the mean of the 3x3 area around it (radius 1, diagonals, centre). Invalid coordinates add `surrounding_space_value` instead of a sample. The divisor is `9.0`. With that radius the area always has 9 coordinates, so the extra `(9 - count) * surrounding` term is 0. Land indices are restored from the pre-iteration buffer, then the buffer is copied back. `iterations <= 0` does nothing.

### `set_terrain_from_noise`

Copy noise, sort it (.NET's float array sort), then round half-to-even (.NET's `Math.Round`) of `(length - 1) * wetness`. That sorted sample is the water cutoff. A second round of `water_index * (1 - percent_shallows)` is the shallow cutoff. `shallow > sample` is Ocean, else `water > sample` is Water, else Field. City indices become Field, improvement City, resource 0. If the index is also in the capital list, `owner` is the matching player's id (the game also records it as the capital's owner), `climate` is `player.tribe` (the game uses the tribe when the player's own climate field is 0), `is_capital` and `is_city` are set. Placing a capital in the game also assigns a city name and skin, which `Tile` does not have. Afterwards every `land_tile_indices` entry is forced to Field.

### `make_ocean` (terrain only)

Orthogonal land next to a water tile makes it Water. `should_convert_shallows` turns the rest into Ocean. North is inspected only when `index > width`. Stripping resources that are not allowed on the new terrain is not applied.

### `patch_continents`

Clears continent tile lists and land counts. Tiles with a continent are appended in map order. Land count increments when the tile is not water and has no improvement.

### `add_terrain`

Calls `patch_continents`. Per continent, shares are `modifier / (field + forest + mountain) * land_tile_count` for Field (3), Mountain (4), and Forest (5), using the continent's modifiers. A positive share appends that terrain while `share > count` (`f32`). Tiles that are not water and have no improvement take `next_range(0, bag.len())` and the picked id is removed (list shift, not swap). An empty bag stops that continent. Resource is cleared. Altitude is not stored.

The continent constructor's defaults (anything not stored starts at zero): field 0.5, mountain 0.15, forest 0.4, ice 0.5, metal 0.8, crop/fish/fruit/game 0.5, water and ocean 0.

### Counts inside `generate_map_from_noise`

`noise_domain_side`, `post_terrain_city_budget`, `suburb_count`, and `pre_terrain_city_count`. The side factor is `0.33f`.

- side = truncate(`width * 0.33`)
- post budget = truncate(`(side * side - capitals) * post_terrain_city_density`)
- suburbs = clamp(`post budget / player_count`, min suburb, max suburb)
- pre-terrain cities = truncate(`(side * side - capitals - suburbs) * pre_terrain_city_density`)
- the post-terrain city step is called with `post budget - pre-terrain cities`

### `generate_map_from_noise`

Capitals come from the capital position generator. The west placement reassignment returns immediately when the west-player id is absent; otherwise it swaps that player's capital with the capital that maximizes `y - x` (strict `>`). Suburbs append their walks to the land list. Cities are capitals, then suburbs, then the pre-terrain cities. Land is that walk list plus the cities plus each capital's `+1`, `-1`, `+width`, `-width` neighbours. Noise is generated and smoothed on that list, `set_terrain_from_noise` writes the map, post-terrain cities are placed, then `add_climates`.

### `add_continents` and `expand_continent`

Each start index becomes its own continent, terrain Field, resource 0. Target land is `(1 - wetness) * tile_count`, multiplied by `1 + 0.5 / continent_count` when there are at least two continents. Half of that, divided by the player count, is the per-capital share. The other half, divided by the continent count, is the base share. Players with id `255` are skipped. Each other player increments the continent's capital count on continents in order, wrapping at `populated_continents_count`.

A continent expands while `capitals * per_capital + base_per > tile_count` and the attempt count is under 1000. One expand shuffles the continent's coordinates, then takes the first seed not already in the exhausted set. It shuffles the four orthogonal neighbours and keeps the unowned one with the highest score. Score starts at `noise + 1`. Pangea adds `-1 / distance_to_edge`. Continents adds `distance_to_edge / -5`. Each foreign neighbour (another continent, radius 1, diagonals, center off) subtracts `range(0, 0.5)` on water and `range(0.5, 3)` on land. A score must be strictly greater than the best so far, which starts at 0. The winner becomes Field and joins the continent. If nobody scores above 0, that seed is exhausted and the call returns.

### `add_climates`

The tribe lookup is [`GameLogicData::tribe_by_idx`] on the embedded v28 table (`load_embedded_game_logic`). A miss leaves climate 0. A hit stores `DEFAULT_CLIMATE` (7) when that tribe's `tribe_abilities` contain `alienclimate` and every successfully loaded player has that ability. Otherwise the climate is `player.tribe` (the player's own climate field is 0 here; that field is not on `PlayerState`). The function replaces the continent list with `player_count - 1` continents and appends each player's start coordinate. Wetness `0` sets the water and ocean modifiers to 0. The radius-1 diagonal spread (budget `10 * continent_count`) runs only when the continent count is greater than `player_count - 1`. This allocation makes them equal, so map tiles are not painted. The terrain and resource modifier updates are not copied: `Continent` has no resource-modifier fields.

### `try_generate_map_from_islands`

Generates noise and smooths it. On the Continents preset it keeps divisors 2 through 7 whose `(1 - wetness) * (tile_count / divisor)` lies in `[30, 200]`, draws one, and places that many capitals. An empty quota returns false. Pangea appends the center index (`width >> 1`, `height >> 1`). The populated count shrinks while `1 < max(1, player_count / count) < 2`. `add_continents` grows those seeds.

A non-empty start list then adds a city to each continent (see below), retrying from the start until the number of failed calls reaches the continent count. Tribe bonus comes from `TribeData.bonus`. The climate argument is `player.tribe` after `tribe_by_idx` succeeds, and 0 when it does not. `west_player` is the west-placement player id; `None` skips the swap. [`try_add_capitals_to_continents`] runs next. A failed capital placement returns false.

On success the inline water scan shuffles tile indices and places cities on water. Budget is `trunc(tile_count * wetness / 50)` (the divisor is 50). A candidate is water, has no improvement, and `distance_to_edge > 0`. It is skipped when any radius-1 diagonal neighbor (center excluded) is not water, or when radius 2 including the center contains a city (improvement type 1). Each placement decrements the budget and the scan stops once that value is not positive, so a zero budget still places one city. `add_climates` then runs and the function returns true.

### Adding a city to a continent

Fewer than two continent tiles returns false without shuffling. Otherwise the coordinate list is shuffled. The first tile that passes [`is_good_city_location`] becomes a city: owner 0, Field, resource 0, city improvement. If none match, the same order is scanned with the forced city location test: edge distance greater than 0, every non-water radius-1 neighbor on this continent, at least one such neighbor, and no improvement in radius 2 including the center. The first hit is placed the same way. Neither path reads resource or terrain modifier dictionaries.

## Still open

`make_ocean` resource removal is unchanged. Assigning a player does not copy terrain or resource modifier dictionaries onto `Continent`. The water scan and the add-a-city step do not read those dictionaries.

Fixed-seed tests check this port's own grids.
