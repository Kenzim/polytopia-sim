//! Map generation pipeline.
//!
//! `generate` is the game's map generator entry point. `generate_internal` is
//! its internal generator. Both call the shape, terrain,
//! and contents functions that are already ported. Steps with no port are skipped
//! in place so the remaining calls stay in order.

use crate::data::GameLogicData;
use crate::rng::DotNetRandom;
use crate::{Map, MapGeneratorSettings, MapPreset, MapSize, PlayerState, Tile};

use super::contents::{ContinentModifiers, ResourceSpawn, NATURE_PLAYER_ID, REJECTED_INEQUALITY};
use super::shape::create_from_preset;
use super::terrain::{Continent, TERRAIN_WATER};

/// Game version above which the island generator is tried. At or below this, only noise runs.
const ISLAND_VERSION: i32 = 98;
/// Lighthouse and starfish placement require `version >= 99`.
const LIGHTHOUSE_AND_STARFISH_VERSION: i32 = 99;
/// The singular starting-resource-to-capital step runs while `version <= 113`.
const PLURAL_STARTING_RESOURCES_VERSION: i32 = 113;
/// Tutorial game mode. A rules game mode of 7 selects tutorial map generation.
const TUTORIAL_RULES_GAME_MODE: i32 = 7;
/// Exclusive upper bound of the per-attempt seed draw (`0x7FFFFFFF`).
const ATTEMPT_SEED_BOUND: i32 = 0x7fff_ffff;
/// A rejected score extends the budget while the
/// decremented counter is still positive, which is 14 extensions from 15.
const REJECTED_EXTENSIONS: i32 = 15;
/// Starfish divisor (the `f32` `0x41C80000`, 25.0).
const STARFISH_DIVISOR: f32 = f32::from_bits(0x41c8_0000);
/// Initial best inequality (the `f32` `0x7F7FFFFF`, `f32::MAX`).
const INITIAL_BEST_INEQUALITY: f32 = f32::from_bits(0x7f7f_ffff);
/// Default metal weight of a continent's modifiers (the `f32` `0x3F4CCCCD`).
const DEFAULT_METAL: f32 = f32::from_bits(0x3f4c_cccd);

/// Version `> 98` and preset Continents or Pangea is the only island attempt.
///
/// The version test is `> 98`. The preset test is Pangea (`6`) or Continents (`3`).
pub fn tries_island_generator(game_version: i32, map_type: MapPreset) -> bool {
    game_version > ISLAND_VERSION && matches!(map_type, MapPreset::Continents | MapPreset::Pangea)
}

/// The map generator entry point.
///
/// `num_iterations > 0` is the attempt count. Otherwise the count is
/// `settings.equality_iterations` from [`create_from_preset`] (50). A count of
/// `<= 0` returns a blank map of `size` and does not call [`generate_internal`].
///
/// Each attempt draws `DotNetRandom::next_range(0, 0x7FFFFFFF)` and passes that
/// int to [`generate_internal`]. The game draws it from
/// an unseeded system random source. This
/// port seeds that draw with `seed` so a caller can repeat a run. `seed` is not
/// the game seed and is not the `System.Random` seed inside one attempt.
///
/// The inequality score keeps the lowest score that is `>= -999`.
/// The loop stops when the current score is `< settings.equality_limit`.
/// After the loop, a winning seed that is not the last attempt is generated again.
///
/// The rules game mode and the west map placement player are not on [`PlayerState`].
/// This entry point uses generation type Default and no west player.
/// [`generate_internal`] takes both.
pub fn generate(
    game_version: i32,
    seed: i32,
    size: MapSize,
    preset: MapPreset,
    players: &mut [PlayerState],
    num_iterations: i32,
) -> Map {
    let mut attempt_rng = DotNetRandom::new(seed);
    generate_with_seed_source(game_version, size, preset, players, num_iterations, &mut || {
        attempt_rng.next_range(0, ATTEMPT_SEED_BOUND)
    })
}

/// [`generate`] with the per-attempt seeds supplied by the caller.
///
/// Retail draws each attempt seed from an unseeded cryptographic source, so a real
/// game's winning seed cannot be recovered from the game seed. Tests feed the
/// seeds recorded from a live game through `next_seed`.
pub fn generate_with_seed_source(
    game_version: i32,
    size: MapSize,
    preset: MapPreset,
    players: &mut [PlayerState],
    num_iterations: i32,
    next_seed: &mut dyn FnMut() -> i32,
) -> Map {
    generate_side_with_seed_source(game_version, size.width(), preset, players, num_iterations, next_seed)
}

/// [`generate_with_seed_source`] for a map of `side` tiles a side, which need not be one of the game's sizes. The
/// generator runs unchanged; the caller keeps the player count within [`crate::max_players_on_side`].
pub fn generate_side_with_seed_source(
    game_version: i32,
    side: u16,
    preset: MapPreset,
    players: &mut [PlayerState],
    num_iterations: i32,
    next_seed: &mut dyn FnMut() -> i32,
) -> Map {
    let settings = create_from_preset(preset);
    let mut attempt_budget = if num_iterations > 0 { num_iterations } else { settings.equality_iterations };
    if attempt_budget <= 0 {
        return Map::new(side, side);
    }

    let data = crate::data::embedded_game_logic_ref();
    let mut extensions_left = if attempt_budget == 1 { 0 } else { REJECTED_EXTENSIONS };
    let mut best_inequality = INITIAL_BEST_INEQUALITY;
    let mut best_seed = -1i32;
    let mut last_seed = -1i32;
    let mut last_map = Map::new(side, side);
    // The map and players of the attempt `best_seed` came from, so it need not be generated again.
    let mut best: Option<(i32, Map, Vec<PlayerState>)> = None;
    let mut index = 0i32;
    while index < attempt_budget {
        let attempt_seed = next_seed();
        let mut attempt_map = Map::new(side, side);
        generate_with_data(game_version, attempt_seed, &mut attempt_map, &settings, players, 0, None, data);
        let inequality = super::contents::get_inequality(&attempt_map, players);
        last_seed = attempt_seed;
        let mut keep = false;
        if inequality < REJECTED_INEQUALITY {
            extensions_left -= 1;
            if extensions_left > 0 {
                attempt_budget += 1;
            }
        } else {
            if best_inequality > inequality {
                best_inequality = inequality;
                best_seed = attempt_seed;
                keep = true;
            }
            // The loop continues while `limit <= score`.
            if inequality < settings.equality_limit {
                best_seed = attempt_seed;
                last_map = attempt_map;
                break;
            }
        }
        if keep {
            best = Some((attempt_seed, attempt_map.clone(), players.to_vec()));
        }
        last_map = attempt_map;
        index += 1;
    }
    if attempt_budget == 1 {
        best_seed = last_seed;
    }
    if best_seed != last_seed {
        match best {
            Some((seed, map, kept)) if seed == best_seed => {
                last_map = map;
                players.copy_from_slice(&kept);
            }
            _ => {
                last_map = Map::new(side, side);
                generate_with_data(game_version, best_seed, &mut last_map, &settings, players, 0, None, data);
            }
        }
    }
    last_map
}

/// One generation pass.
///
/// `seed` constructs the random generator used for this pass. `rules_game_mode`
/// is the rules game mode; `7` is tutorial. `west_player` is
/// the west map placement player id. `None` skips the capital swap.
///
/// Width and height must already be set on `map`. The game uses one
/// map size value for both.
pub fn generate_internal(
    game_version: i32,
    seed: i32,
    map: &mut Map,
    settings: &MapGeneratorSettings,
    players: &mut [PlayerState],
    rules_game_mode: i32,
    west_player: Option<u8>,
) {
    let data = crate::data::embedded_game_logic_ref();
    generate_with_data(game_version, seed, map, settings, players, rules_game_mode, west_player, data);
}

fn generate_with_data(
    game_version: i32,
    seed: i32,
    map: &mut Map,
    settings: &MapGeneratorSettings,
    players: &mut [PlayerState],
    rules_game_mode: i32,
    west_player: Option<u8>,
    data: Option<&GameLogicData>,
) {
    let mut rng = DotNetRandom::new(seed);
    clear_map(map);
    let mut continents: Vec<Continent> = Vec::new();
    let mut continent_of = vec![None; map.tiles.len()];
    let west_index = west_player.and_then(|id| players.iter().position(|player| player.id == id));

    if tries_island_generator(game_version, settings.map_type) {
        let placed = super::terrain::try_generate_map_from_islands(
            map,
            &mut rng,
            players,
            settings,
            &mut continents,
            &mut continent_of,
            west_player,
        );
        if !placed {
            clear_map(map);
            continents.clear();
            continent_of.fill(None);
            super::terrain::generate_map_from_noise(
                map,
                &mut rng,
                players,
                settings,
                &mut continents,
                &mut continent_of,
                west_index,
            );
        }
    } else {
        super::terrain::generate_map_from_noise(
            map,
            &mut rng,
            players,
            settings,
            &mut continents,
            &mut continent_of,
            west_index,
        );
    }

    // Placing a capital from noise writes the player's start tile in the game.
    // The ported noise path does not. Island placement
    // already wrote it. Copy it from the capital tile before inequality and
    // starting resources read it.
    sync_start_tiles(map, players);

    let paintable: Vec<Option<usize>> =
        continent_of.iter().map(|slot| slot.filter(|index| *index < continents.len())).collect();
    super::terrain::add_terrain(map, &mut rng, &mut continents, &paintable);

    let convert_shallows = settings.shallow_percent_of_water == 0.0;
    super::terrain::make_ocean(map, convert_shallows);

    if game_version >= LIGHTHOUSE_AND_STARFISH_VERSION {
        super::contents::add_lighthouses(map);
    }

    // Tutorial games use their own fixed resource placement, which
    // `add_resources` does not call. Default generation does.
    if rules_game_mode != TUTORIAL_RULES_GAME_MODE {
        if let Some(data) = data {
            let resources = resource_spawns(data);
            let modifiers = continent_modifiers(&continent_of, &continents);
            super::contents::add_resources(map, &resources, &modifiers, settings.richness, &mut rng);
        }
    }

    if let Some(data) = data {
        place_starting_resources(map, players, game_version, rules_game_mode, data, &mut rng);
    }

    super::terrain::make_ocean(map, convert_shallows);

    // Flooding for tribes with that ability is not ported.
    // The lighthouse corner exploration writes explorer lists `Tile` does not have.
    // Shore line generation is not ported.
    // The player's skin is not on `Tile`. Climate is painted below.

    let tile_count = map.tiles.len() as i32;
    super::contents::add_ruins(map, tile_count / 40, &mut rng);
    if game_version >= LIGHTHOUSE_AND_STARFISH_VERSION {
        let amount = tile_count as f32 * settings.wetness / STARFISH_DIVISOR;
        super::contents::add_starfish(map, amount, &mut rng);
    }
    if let Some(data) = data {
        freeze_alien_capitals(map, players, data);
    }

    paint_owning_climate(map, players);
}

/// A tribe with `alienclimate` (Polaris) starts with the water and ocean on its capital and the 8 tiles round it
/// turned to ice. Nothing else on those tiles changes.
fn freeze_alien_capitals(map: &mut Map, players: &[PlayerState], data: &GameLogicData) {
    for player in players {
        if player.id == NATURE_PLAYER_ID {
            continue;
        }
        let alien =
            data.tribe_by_idx(player.tribe).is_some_and(|t| t.tribe_abilities.iter().any(|a| a == "alienclimate"));
        let center = player.start_tile;
        if !alien || center.x < 0 || center.y < 0 {
            continue;
        }
        for dx in -1..=1 {
            for dy in -1..=1 {
                let (x, y) = (center.x + dx, center.y + dy);
                if x < 0 || y < 0 || x >= i32::from(map.width) || y >= i32::from(map.height) {
                    continue;
                }
                let tile = &mut map.tiles[(y * i32::from(map.width) + x) as usize];
                if super::terrain::is_water(tile.terrain) {
                    tile.terrain = super::terrain::TERRAIN_ICE;
                }
            }
        }
    }
}

/// Step 13 of the generation pass: for each player, the radius 1 area
/// (diagonals on, centre off) around the start tile; every tile in that ring gets the
/// player's climate (`get_climate`: `_climate`, or the tribe when it is 0). The
/// rest of the climate map is written by the climate pass.
fn paint_owning_climate(map: &mut Map, players: &[PlayerState]) {
    for player in players {
        if player.id == NATURE_PLAYER_ID {
            continue;
        }
        let center = player.start_tile;
        if center.x < 0 || center.y < 0 {
            continue;
        }
        for dx in -1..=1 {
            for dy in -1..=1 {
                if dx == 0 && dy == 0 {
                    continue;
                }
                let (x, y) = (center.x + dx, center.y + dy);
                if x < 0 || y < 0 || x >= i32::from(map.width) || y >= i32::from(map.height) {
                    continue;
                }
                map.tiles[(y * i32::from(map.width) + x) as usize].climate = player.tribe;
            }
        }
    }
}

fn clear_map(map: &mut Map) {
    for tile in &mut map.tiles {
        let index = tile.index;
        let coordinates = tile.coordinates;
        *tile = Tile::default();
        tile.index = index;
        tile.coordinates = coordinates;
        // Clearing the map allocates a fresh tile and sets
        // `terrain = Water` (plus altitude -1, owner 0, no improvement).
        tile.terrain = TERRAIN_WATER;
    }
}

fn sync_start_tiles(map: &Map, players: &mut [PlayerState]) {
    for player in players.iter_mut() {
        if player.id == NATURE_PLAYER_ID {
            continue;
        }
        if let Some(tile) = map.tiles.iter().find(|tile| tile.is_capital && tile.owner == player.id) {
            player.start_tile = tile.coordinates;
        }
    }
}

/// Default continent weights. Whale, spores, and aquacrop stay 0.
fn constructor_modifiers() -> ContinentModifiers {
    ContinentModifiers {
        game: 0.5,
        crop: 0.5,
        fish: 0.5,
        whale: 0.0,
        metal: DEFAULT_METAL,
        fruit: 0.5,
        spores: 0.0,
        aquacrop: 0.0,
    }
}

/// Per-tile continent modifiers from the tile's continent. An index past the end
/// of `continents` (a tile the climate pass never claimed still points at an island
/// continent that no longer exists in the list) falls back to the constructor
/// defaults, which is what that discarded object holds.
fn continent_modifiers(continent_of: &[Option<usize>], continents: &[Continent]) -> Vec<Option<ContinentModifiers>> {
    let defaults = constructor_modifiers();
    continent_of
        .iter()
        .map(|slot| {
            slot.map(|index| match continents.get(index) {
                Some(c) => ContinentModifiers {
                    game: c.game,
                    crop: c.crop,
                    fish: c.fish,
                    whale: c.whale,
                    metal: c.metal,
                    fruit: c.fruit,
                    spores: c.spores,
                    aquacrop: c.aquacrop,
                },
                None => defaults,
            })
        })
        .collect()
}

pub(crate) fn resource_spawns(data: &GameLogicData) -> Vec<ResourceSpawn> {
    let mut rows: Vec<&crate::data::ResourceData> = data.resources.values().collect();
    rows.sort_by_key(|resource| resource.idx);
    rows.into_iter()
        .map(|resource| ResourceSpawn {
            kind: resource.idx,
            terrains: resource
                .resource_terrain_requirements
                .iter()
                .filter_map(|id| data.terrain(id).map(|terrain| terrain.idx))
                .collect(),
        })
        .collect()
}

fn resource_id(data: &GameLogicData, idx: i32) -> Option<String> {
    data.resources.values().find(|resource| resource.idx == idx).map(|resource| resource.id.clone())
}

fn place_starting_resources(
    map: &mut Map,
    players: &[PlayerState],
    game_version: i32,
    rules_game_mode: i32,
    data: &GameLogicData,
    rng: &mut DotNetRandom,
) {
    for player in players {
        if player.id == NATURE_PLAYER_ID {
            continue;
        }
        let kinds = super::contents::get_starting_resources_for_player(data, player.tribe, rng);
        // The singular starting-resource-to-capital step is the `version <= 113` arm.
        // It is not ported, so its `Random` draws are absent on that arm.
        if game_version <= PLURAL_STARTING_RESOURCES_VERSION {
            continue;
        }
        let mut ids: Vec<String> = kinds.iter().filter_map(|idx| resource_id(data, *idx)).collect();
        if rules_game_mode == TUTORIAL_RULES_GAME_MODE {
            if let Some(id) = resource_id(data, 1) {
                ids.extend(std::iter::repeat_n(id, 3));
            }
        }
        super::contents::add_starting_resources_to_capital(map, player.start_tile, &ids, data, rng);
    }
}
