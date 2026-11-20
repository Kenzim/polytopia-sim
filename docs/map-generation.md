# Map generation

A game starts with `Engine::new_game(&GameOptions)` (`src/engine/setup.rs`). It calls `map::generate::generate`, which builds the map, and then sets up players, techs, fog, starting units and scores. The generator reproduces the game's own, down to the order of its random draws, so a map is a function of its inputs. The per-function detail is in the specs: [map-shape](specs/map-shape.md) (entry point, orchestration, presets, capitals, fairness retries), [map-terrain](specs/map-terrain.md) (noise, land, continents, climates) and [map-contents](specs/map-contents.md) (cities, ruins, resources, fairness score). Random numbers come from [the rng module](specs/rng.md).

## Inputs

| Input | Values |
| --- | --- |
| Size | `MapSize`: Tiny 11, Small 14, Normal 16, Large 18, Huge 20, Massive 30 tiles per side. Maps are square. |
| Preset | `MapPreset`: Dryland, Lakes, Continents, Archipelago, WaterWorld, Pangea (and `None`, which uses the same settings as Continents). |
| Players | One tribe per player, ids `1..=n` in seat order, then Nature (id 255) last. Nature must be in the list the generator gets (the climate pass gives a region to every entry but the last), but it gets no capital and does not count towards the player count. `MapSize::max_players` gives how many seats a size holds on every seed: Tiny 9, Small 16, Normal 16, Large 25, Huge 36, Massive 81. `Engine::try_new_game` returns an error past that; `Engine::new_game` panics. |
| Version | The game version. It gates several steps (see below). |
| Seed | A `i32` that seeds the attempt draws. |

The preset only changes the generator settings (`create_from_preset`): how much of the map is water (`wetness`), how much the noise is smoothed, and how many extra cities are scattered before and after terrain is laid down. Dryland sets `wetness` to 0, so the terrain step makes no water. Lakes and Archipelago add water, WaterWorld has the most, and Pangea and Continents differ in how the land is grouped. The values are listed in [map-shape](specs/map-shape.md#preset-overrides).

## The seed and fairness retries

The retail game draws a fresh seed for every attempt from an unseeded source, so the seed saved with a game does not reproduce its map. In the engine the caller's seed starts a `DotNetRandom` that supplies the attempt seeds, which is what makes a run repeatable.

`generate` makes up to 50 attempts (`equality_iterations`). Each attempt runs `generate_internal` with its own attempt seed and is scored by `get_inequality`: for every player a "turf" is worked out (the tiles that player reaches first, valuing cities, ruins and land and penalising distance), and the score compares the best and worst turf. An attempt whose score is below the equality limit (1.01) is accepted at once; otherwise the lowest score among the attempts is kept. An attempt whose score is below -999 is discarded and can extend the budget by up to 15 attempts. If the winning attempt is not the last one made, the generator is run once more with its seed. Details: [map-contents](specs/map-contents.md#equality-retries) and [map-shape](specs/map-shape.md#map-generation-entry-point).

## What one attempt does

`generate_internal` follows the game's call order:

1. **Terrain source.** For versions up to 98, or any preset other than Continents and Pangea, land is made from noise (`generate_map_from_noise`): capitals and suburb cities are placed first, random noise is spread over the land area and smoothed, and the share of tiles set by `wetness` becomes water or ocean while the rest becomes field. For Continents and Pangea from version 99 the island generator is tried first (`try_generate_map_from_islands`): capitals are put on separate continents (one shared continent for Pangea), and each continent grows tile by tile towards a land quota. If the island generator fails, the noise generator is used.
2. **Terrain mix.** `add_terrain` turns land into field, forest and mountain by each continent's shares, which follow the tribes living there. `add_climates` gives every tile a climate, spreading each climate outwards from a start tile, with the per-tribe terrain and resource modifiers applied to the continent a tribe lives on.
3. **Ocean.** `make_ocean` keeps the water tiles that touch land as shallow water and, when the preset has no shallow share, turns the rest into deep ocean. It runs twice.
4. **Lighthouses**, from version 99: one on each corner of the map.
5. **Resources.** `add_resources` scatters the resources with weights by terrain and climate, richer near cities. The tribes' own weights are in [the tribe reference](reference/tribes.md), the resources and where they grow in [terrain and resources](reference/terrain-and-resources.md).
6. **Starting resources.** From version 114 each capital gets the tribe's starting resources (or those its last starting tech needs), placed on the tiles around the capital.
7. **Ruins and starfish.** `add_ruins` places `tiles / 40` ruins away from cities and capitals. From version 99 `add_starfish` adds starfish on water in proportion to `wetness`.
8. **Climate under the capitals.** The ring around each start tile takes its tribe's climate.

Which steps of the original are left out is tabulated in [map-shape](specs/map-shape.md#steps-of-the-list-above-that-generate_internal-does-not-do).

## What setup adds

After the map exists, `Engine::new_game` and `finish_setup`:

- gives each player the tribe's starting stars and starting techs ([tribes](reference/tribes.md));
- explores a radius of 2 around each capital and puts the tribe's starting unit on it;
- works out the first scores ([game modes and scoring](game-modes.md#scoring)).

Cities that are not capitals start with no owner and are taken later ([capture](capture.md)).

## Checked against the real game

`tests/map_capture.rs` replays captures of the real generator, tile by tile: every preset on Tiny to Massive maps with 2 to 9 players. Each capture also checks that the retry loop keeps the same attempt. The table of captures is at the end of [map-shape](specs/map-shape.md). Combinations that no capture covers go through the same code but have not been compared with the game. `tests/new_game_map.rs` checks that `Engine::new_game` produces the same map as the generator called the way the captures call it.

## Seed search

`src/search.rs` does the reverse: given the 5x5 area a player can see at the start of a Normal Dryland game with 9 players, it lists the seeds whose maps match. It uses the capital positions to throw out seeds before generating a map. `examples/seed_search.rs` runs it (`cargo run --release --example seed_search -- examples/normal-dryland-9.json 0 64`), `examples/export_map.rs` writes a generated map as JSON and `examples/make_view.rs` prints such a 5x5 input for a generated map. More than three unknown (hidden) tribes is rejected instead of searched.
