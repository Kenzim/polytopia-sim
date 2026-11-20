//! Noise, terrain, water, and climate.
//! Reproduces the game's map generator.

use crate::data::{GameLogicData, TribeData};
use crate::map::contents::{
    add_post_terrain_cities, distance_to_edge, generate_pre_terrain_cities, is_good_city_location, is_near_city,
    try_add_capitals_to_continents,
};
use crate::map::shape::{generate_player_capital_positions, generate_suburbs, shuffle_coords};
use crate::rng::{range_f32, value, DotNetRandom};
use crate::{Map, MapGeneratorSettings, MapPreset, PlayerState, WorldCoordinates, DEFAULT_CLIMATE};

/// Terrain type ids.
pub const TERRAIN_NONE: i32 = 0;
pub const TERRAIN_WATER: i32 = 1;
pub const TERRAIN_OCEAN: i32 = 2;
pub const TERRAIN_FIELD: i32 = 3;
pub const TERRAIN_MOUNTAIN: i32 = 4;
pub const TERRAIN_FOREST: i32 = 5;
pub const TERRAIN_ICE: i32 = 6;
pub const TERRAIN_WETLAND: i32 = 7;
pub const TERRAIN_MANGROVE: i32 = 8;

/// Improvement type id of a city.
pub const IMPROVEMENT_CITY: i32 = 1;

/// True when the terrain is Water or Ocean.
pub fn is_water(terrain: i32) -> bool {
    let shifted = terrain.wrapping_sub(1);
    (shifted as u32) < 2
}

/// Continent used by [`patch_continents`] and [`add_terrain`].
///
/// Modifier defaults are the game's continent constructor defaults
/// (anything the constructor does not store starts at zero).
#[derive(Debug, Clone, PartialEq)]
pub struct Continent {
    pub tiles: Vec<WorldCoordinates>,
    pub climate: i32,
    pub land_tile_count: i32,
    pub field: f32,
    pub mountain: f32,
    pub forest: f32,
    pub water: f32,
    pub ocean: f32,
    pub ice: f32,
    pub crop: f32,
    pub fish: f32,
    pub fruit: f32,
    pub game: f32,
    pub metal: f32,
    pub whale: f32,
    pub spores: f32,
    pub aquacrop: f32,
    /// Number of capitals placed on this continent.
    pub number_of_capitals: i32,
    /// Set when a player is assigned to the continent (see [`add_climates`]).
    pub has_alien_climate: bool,
}

impl Default for Continent {
    fn default() -> Self {
        Self {
            tiles: Vec::new(),
            climate: 0,
            land_tile_count: 0,
            field: 0.5,
            mountain: 0.15,
            forest: 0.4,
            water: 0.0,
            ocean: 0.0,
            ice: 0.5,
            crop: 0.5,
            fish: 0.5,
            fruit: 0.5,
            game: 0.5,
            metal: f32::from_bits(0x3f4c_cccd),
            whale: 0.0,
            spores: 0.0,
            aquacrop: 0.0,
            number_of_capitals: 0,
            has_alien_climate: false,
        }
    }
}

/// Fills the noise grid.
///
/// Each cell is a random value in `[0, 1)`. Cells in `land_indices` are
/// remapped with `value * (1 - wetness) + wetness`.
pub fn generate_noise(rng: &mut DotNetRandom, width: i32, height: i32, land_indices: &[i32], wetness: f32) -> Vec<f32> {
    let len = width.wrapping_mul(height);
    if len <= 0 {
        return Vec::new();
    }
    let len = len as usize;
    let one_minus = 1.0 - wetness;
    let mut noise = Vec::with_capacity(len);
    for index in 0..len {
        let mut sample = value(rng);
        if land_indices.contains(&(index as i32)) {
            sample = sample * one_minus + wetness;
        }
        noise.push(sample);
    }
    noise
}

/// Smooths the noise grid.
///
/// Each iteration replaces a cell with the mean of its 3x3 neighbourhood.
/// Out-of-range neighbours contribute `surrounding_space_value`. Land indices
/// are copied back from the pre-iteration buffer, then the buffer is written
/// over `noise`. `iterations <= 0` leaves `noise` unchanged.
pub fn smooth_noise(
    width: i32,
    height: i32,
    noise: &mut [f32],
    land_indices: &[i32],
    iterations: i32,
    surrounding_space_value: f32,
) {
    if iterations <= 0 || noise.is_empty() {
        return;
    }
    let mut smoothed = vec![0.0; noise.len()];
    for _ in 0..iterations {
        for (index, cell) in smoothed.iter_mut().enumerate() {
            *cell = smooth_cell(noise, width, height, index as i32, surrounding_space_value);
        }
        for &land in land_indices {
            let land = land as usize;
            smoothed[land] = noise[land];
        }
        noise.copy_from_slice(&smoothed);
    }
}

fn smooth_cell(noise: &[f32], width: i32, height: i32, index: i32, space: f32) -> f32 {
    let x = index % width;
    let y = index / width;
    let mut sum = 0.0f32;
    let mut count = 0i32;
    for cy in (y - 1)..=(y + 1) {
        for cx in (x - 1)..=(x + 1) {
            count += 1;
            if cx >= 0 && cy >= 0 && cx < width && cy < height {
                sum += noise[(cy * width + cx) as usize];
            } else {
                sum += space;
            }
        }
    }
    let missing = 9 - count;
    sum += (missing as f32) * space;
    sum / 9.0
}

/// Turns the noise grid into terrain.
///
/// Sorted noise is split with `Math.Round` (half to even) at
/// `(length - 1) * wetness`, then again at that index times
/// `percent_shallows` (verified on the Normal Lakes capture: 0 means no Ocean here, and
/// [`make_ocean`] makes Ocean later). Below the shallow cutoff is Ocean, below the
/// water cutoff is Water, and the rest is Field. City indices become Field
/// with improvement City. A city index that is also a capital takes the
/// matching `players` entry: `owner` is that player's id (the game also records it as the
/// capital's owner), `climate` is `player.tribe` (the game uses the tribe when the
/// player's own climate field is 0), and `is_capital` is set.
/// `land_tile_indices` are forced to Field afterwards.
///
/// Placing a capital in the game also names the city and copies the skin.
/// Those fields are not on `Tile`.
pub fn set_terrain_from_noise(
    map: &mut Map,
    noise: &[f32],
    city_tile_indices: &[i32],
    capital_tile_indices: &[i32],
    land_tile_indices: &[i32],
    players: &[PlayerState],
    wetness: f32,
    percent_shallows: f32,
) {
    if noise.is_empty() {
        return;
    }
    let mut sorted = noise.to_vec();
    sorted.sort_by(|a, b| a.total_cmp(b));
    let water_index = round_to_even((noise.len() as i32 - 1) as f32 * wetness);
    let water_threshold = sorted[water_index as usize];
    let shallow_index = round_to_even(water_index as f32 * percent_shallows);
    let shallow_threshold = sorted[shallow_index as usize];

    // Index-based so a capital can claim its ring in the same pass, as placing a capital
    // does inside the game's loop. A later city tile in that ring still resets its owner to 0.
    #[allow(clippy::needless_range_loop)] // `index` is also a tile id compared against the city and capital lists
    for index in 0..map.tiles.len() {
        let index_i = index as i32;
        if city_tile_indices.contains(&index_i) {
            if let Some(capital_at) = capital_tile_indices.iter().position(|&c| c == index_i) {
                let player = &players[capital_at];
                let tile = &mut map.tiles[index];
                tile.terrain = TERRAIN_FIELD;
                tile.climate = player.tribe;
                tile.resource = 0;
                tile.improvement = IMPROVEMENT_CITY;
                tile.owner = player.id;
                tile.is_capital = true;
                tile.capital_of = player.id;
                tile.is_city = true;
                tile.altitude = 1;
                // The city's starting border (radius 1) claims the surrounding ring.
                let center = tile.coordinates;
                for neighbor in area_indices(map, center.x, center.y, 1, true, false) {
                    if map.tiles[neighbor].owner == 0 {
                        map.tiles[neighbor].owner = player.id;
                    }
                }
            } else {
                let tile = &mut map.tiles[index];
                tile.owner = 0;
                tile.terrain = TERRAIN_FIELD;
                tile.resource = 0;
                tile.improvement = IMPROVEMENT_CITY;
                tile.is_city = true;
                tile.altitude = 1;
            }
        } else {
            let sample = noise[index];
            let tile = &mut map.tiles[index];
            if shallow_threshold > sample {
                tile.terrain = TERRAIN_OCEAN;
                tile.altitude = -2;
            } else if water_threshold > sample {
                tile.terrain = TERRAIN_WATER;
                tile.altitude = -1;
            } else {
                tile.terrain = TERRAIN_FIELD;
                tile.altitude = 1;
            }
        }
    }

    for &land in land_tile_indices {
        map.tiles[land as usize].terrain = TERRAIN_FIELD;
        map.tiles[land as usize].altitude = 1;
    }
}

/// Rounds with ties to even, like `Math.Round` on the converted `f64`.
fn round_to_even(value: f32) -> i32 {
    let value = f64::from(value);
    let trunc = value.trunc();
    let frac = value - trunc;
    let rounded = if value >= 0.0 {
        if frac == 0.5 {
            if (trunc as i64) & 1 != 0 {
                trunc + 1.0
            } else {
                trunc
            }
        } else {
            (value + 0.5).floor()
        }
    } else if frac == -0.5 {
        if (trunc as i64) & 1 != 0 {
            trunc - 1.0
        } else {
            trunc
        }
    } else {
        (value - 0.5).ceil()
    };
    rounded as i32
}

/// Ocean pass, terrain only.
///
/// Water tiles with an orthogonal land neighbour become Water. With
/// `should_convert_shallows`, water tiles with no land neighbour become Ocean.
/// The north neighbour is read only when `index > width` (so the
/// tile at `index == width` has none). The game also removes resources that are
/// not allowed on the new terrain; that is not applied here: this build has
/// no resource allow-list on `Tile`.
pub fn make_ocean(map: &mut Map, should_convert_shallows: bool) {
    let width = i32::from(map.width);
    let height = i32::from(map.height);
    if width <= 0 || height <= 0 {
        return;
    }
    let last_row = width * (height - 1);
    let len = map.tiles.len();
    for index in 0..len {
        if !is_water(map.tiles[index].terrain) {
            continue;
        }
        let index_i = index as i32;
        let mut land_neighbour = false;
        if index_i < last_row {
            let south = (index_i + width) as usize;
            land_neighbour |= !is_water(map.tiles[south].terrain);
        }
        if index_i > width {
            let north = (index_i - width) as usize;
            land_neighbour |= !is_water(map.tiles[north].terrain);
        }
        let x = index_i % width;
        if x != 0 {
            land_neighbour |= !is_water(map.tiles[index - 1].terrain);
        }
        if x < width - 1 {
            land_neighbour |= !is_water(map.tiles[index + 1].terrain);
        }
        if land_neighbour || should_convert_shallows {
            map.tiles[index].terrain = if land_neighbour { TERRAIN_WATER } else { TERRAIN_OCEAN };
        }
    }
}

/// Rebuilds the continents' tile lists from `continent_of`.
///
/// Clears each continent's tile list and land count, then appends every tile
/// whose `continent_of` entry is `Some`. Land count increments for tiles that
/// are not water and have no improvement.
pub fn patch_continents(map: &Map, continents: &mut [Continent], continent_of: &[Option<usize>]) {
    for continent in continents.iter_mut() {
        continent.tiles.clear();
        continent.land_tile_count = 0;
    }
    for (index, tile) in map.tiles.iter().enumerate() {
        let Some(continent_index) = continent_of.get(index).copied().flatten() else {
            continue;
        };
        let continent = &mut continents[continent_index];
        continent.tiles.push(tile.coordinates);
        if !is_water(tile.terrain) && tile.improvement == 0 {
            continent.land_tile_count += 1;
        }
    }
}

/// Field, then mountain, then forest counts for the terrain bag of [`add_terrain`].
///
/// `share > count` keeps appending, using the same `f32` compare as the
/// game, so the count is the smallest integer greater than or equal to a
/// positive share.
pub fn terrain_bag(field: f32, mountain: f32, forest: f32, land_tile_count: i32) -> Vec<i32> {
    let sum = (field + forest) + mountain;
    let land = land_tile_count as f32;
    let mut bag = Vec::new();
    push_quota(&mut bag, (field / sum) * land, TERRAIN_FIELD);
    push_quota(&mut bag, (mountain / sum) * land, TERRAIN_MOUNTAIN);
    push_quota(&mut bag, (forest / sum) * land, TERRAIN_FOREST);
    bag
}

#[allow(clippy::neg_cmp_op_on_partial_ord)] // `!(a > b)` is deliberately true for NaN
fn push_quota(bag: &mut Vec<i32>, share: f32, terrain: i32) {
    if !(share > 0.0) {
        return;
    }
    let mut count = 1i32;
    loop {
        bag.push(terrain);
        let counted = count as f32;
        count += 1;
        if !(share > counted) {
            break;
        }
    }
}

/// Paints terrain onto the continents.
///
/// Calls [`patch_continents`], builds a terrain bag per continent, and deals
/// it with `Random.Next(0, bag.Count)` (`next_range`, exclusive upper bound)
/// onto tiles that are not water and have no improvement. An empty bag stops
/// the continent. The game also writes an altitude that `Tile`
/// does not store.
pub fn add_terrain(
    map: &mut Map,
    rng: &mut DotNetRandom,
    continents: &mut [Continent],
    continent_of: &[Option<usize>],
) {
    patch_continents(map, continents, continent_of);
    for continent in continents.iter() {
        let mut bag = terrain_bag(continent.field, continent.mountain, continent.forest, continent.land_tile_count);
        for coords in &continent.tiles {
            let index = (coords.y * i32::from(map.width) + coords.x) as usize;
            let tile = &map.tiles[index];
            if is_water(tile.terrain) || tile.improvement != 0 {
                continue;
            }
            if bag.is_empty() {
                break;
            }
            let pick = rng.next_range(0, bag.len() as i32) as usize;
            let terrain = bag.remove(pick);
            map.tiles[index].terrain = terrain;
            map.tiles[index].resource = 0;
        }
    }
}

/// Side length of the noise domain: truncate `width * 0.33`.
pub fn noise_domain_side(width: i32) -> i32 {
    (width as f32 * 0.33) as i32
}

/// `(side^2 - capital_count) * post_terrain_city_density`, truncated.
pub fn post_terrain_city_budget(width: i32, capital_count: i32, post_terrain_city_density: f32) -> i32 {
    let remaining = noise_domain_side(width) * noise_domain_side(width) - capital_count;
    (remaining as f32 * post_terrain_city_density) as i32
}

/// Suburb count clamped to `[min_suburb, max_suburb]` after dividing the
/// post-terrain budget by `player_count`.
pub fn suburb_count(city_budget: i32, player_count: i32, min_suburb: i32, max_suburb: i32) -> i32 {
    let per_player = city_budget / player_count;
    per_player.min(max_suburb).max(min_suburb)
}

/// Pre-terrain city count: truncate
/// `(side^2 - capitals - suburbs) * pre_terrain_city_density`.
pub fn pre_terrain_city_count(width: i32, capital_count: i32, suburb_total: i32, pre_terrain_city_density: f32) -> i32 {
    let remaining = noise_domain_side(width) * noise_domain_side(width) - capital_count - suburb_total;
    (remaining as f32 * pre_terrain_city_density) as i32
}

/// Noise-based generation.
///
/// Places capitals, optional west-player swap, suburbs, and pre-terrain cities,
/// builds noise from those indices plus capital neighbours, smooths it, writes
/// terrain, then places post-terrain cities and climate continents.
/// `west_player` is the west map placement player id: `None` is the empty
/// value and skips the swap.
pub fn generate_map_from_noise(
    map: &mut Map,
    rng: &mut DotNetRandom,
    players: &mut [PlayerState],
    settings: &MapGeneratorSettings,
    continents: &mut Vec<Continent>,
    continent_of: &mut [Option<usize>],
    west_player: Option<usize>,
) {
    let width = i32::from(map.width);
    let height = i32::from(map.height);
    let player_count = super::contents::player_count(players);
    let mut capitals = generate_player_capital_positions(rng, width, player_count);
    reassign_capitals_for_west(&mut capitals, width, west_player);

    let post_budget = post_terrain_city_budget(width, capitals.len() as i32, settings.post_terrain_city_density);
    let suburbs_each = suburb_count(post_budget, player_count, settings.min_suburb_count, settings.max_suburb_count);
    let mut land = Vec::new();
    let suburbs = generate_suburbs(rng, width, height, player_count, &capitals, suburbs_each, &mut land);
    let mut cities = capitals.clone();
    cities.extend_from_slice(&suburbs);
    let pre_cities =
        pre_terrain_city_count(width, capitals.len() as i32, suburbs.len() as i32, settings.pre_terrain_city_density);
    generate_pre_terrain_cities(map, &mut cities, pre_cities, rng);
    land.extend_from_slice(&cities);
    // Retail smooths with the land list before the capital neighbours are appended;
    // only the noise generation receives them (74 vs 98 entries on the Lakes capture).
    let smooth_land = land.clone();
    land.extend(capital_neighbor_indices(&capitals, width, map.tiles.len() as i32));

    let mut noise = generate_noise(rng, width, height, &land, settings.wetness);
    smooth_noise(width, height, &mut noise, &smooth_land, settings.smooth_iterations, settings.surrounding_space_value);
    set_terrain_from_noise(
        map,
        &noise,
        &cities,
        &capitals,
        &smooth_land,
        players,
        settings.wetness,
        settings.shallow_percent_of_water,
    );
    let post_cities = post_budget - pre_cities;
    add_post_terrain_cities(map, post_cities, rng);
    // Placing a capital from noise writes the player's start tile in the game.
    // `add_climates` seeds its flood fill from those tiles, so record them first.
    for (player, &capital) in players.iter_mut().zip(&capitals) {
        if player.id != 255 {
            if let Some(tile) = usize::try_from(capital).ok().and_then(|i| map.tiles.get(i)) {
                player.start_tile = tile.coordinates;
            }
        }
    }
    add_climates(map, rng, players, settings, continents, continent_of);
}

/// Island-based generation.
///
/// Builds noise, and on the Continents preset a divisor quota of 2..=7 whose
/// dry-land budget sits in `[30, 200]`. An empty quota returns false (the
/// game draws a range of `0..0` and throws). Pangea inserts the map center.
/// The used continent count shrinks while `1 < max(1, players/count) < 2`.
/// `add_continents` then grows those seeds.
///
/// `west_player` is the west map placement player id.
/// `None` skips the swap. After continents grow, each continent receives cities,
/// capitals are placed, the water scan may turn an isolated water tile into a
/// city, then `add_climates` runs and the function returns true. A failed
/// capital placement returns false before the water scan.
pub fn try_generate_map_from_islands(
    map: &mut Map,
    rng: &mut DotNetRandom,
    players: &mut [PlayerState],
    settings: &MapGeneratorSettings,
    continents: &mut Vec<Continent>,
    continent_of: &mut [Option<usize>],
    west_player: Option<u8>,
) -> bool {
    let width = i32::from(map.width);
    let height = i32::from(map.height);
    let tile_count = map.tiles.len() as i32;
    let mut noise = generate_noise(rng, width, height, &[], settings.wetness);
    smooth_noise(width, height, &mut noise, &[], settings.smooth_iterations, settings.surrounding_space_value);

    let mut starts = Vec::new();
    if settings.map_type == MapPreset::Continents {
        let quotas = continent_island_quotas(tile_count, settings.wetness);
        if quotas.is_empty() {
            return false;
        }
        let pick = rng.next_range(0, quotas.len() as i32) as usize;
        starts = generate_player_capital_positions(rng, width, quotas[pick]);
    }
    if settings.map_type == MapPreset::Pangea {
        let x = width >> 1;
        let y = height >> 1;
        starts.push(y * width + x);
    }

    if !starts.is_empty() {
        let populated = shrink_populated_count(starts.len() as i32, super::contents::player_count(players));
        add_continents(map, rng, players, settings, &starts, &noise, populated, continents, continent_of);
        add_cities_to_continents(map, rng, continents, continent_of);
        let lists = continent_tile_indices(map, continents);
        let (bonus, climate) = tribe_capital_args(players);
        if !try_add_capitals_to_continents(map, players, &lists, populated, rng, west_player, &bonus, &climate) {
            return false;
        }
        scan_island_water(map, rng, settings.wetness);
    }

    add_climates(map, rng, players, settings, continents, continent_of);
    true
}

/// Adds a city to every continent.
///
/// A successful call resets the failure counter, so the walk stops after
/// `continent count` consecutive failures (confirmed against real games). A city placed by
/// the forced fallback counts as a failure.
fn add_cities_to_continents(
    map: &mut Map,
    rng: &mut DotNetRandom,
    continents: &[Continent],
    continent_of: &[Option<usize>],
) {
    let count = continents.len() as i32;
    if count <= 0 {
        return;
    }
    let mut failures = 0i32;
    let mut index = 0i32;
    loop {
        let placed = add_city_to_continent(map, rng, &continents[index as usize], index as usize, continent_of);
        index += 1;
        if placed {
            failures = 0;
        } else {
            failures += 1;
        }
        if index >= count {
            index = 0;
        }
        if failures >= count {
            break;
        }
    }
}

/// Adds a city to one continent.
///
/// A continent with fewer than two tiles returns false. Otherwise its
/// coordinates are shuffled and the first [`is_good_city_location`] tile becomes a
/// city (owner 0, Field, resource 0, city improvement). If that pass misses,
/// the same order is scanned with [`is_valid_forced_city_on_continent`].
fn add_city_to_continent(
    map: &mut Map,
    rng: &mut DotNetRandom,
    continent: &Continent,
    continent_index: usize,
    continent_of: &[Option<usize>],
) -> bool {
    if continent.tiles.len() < 2 {
        return false;
    }
    let mut coords = continent.tiles.clone();
    shuffle_coords(rng, &mut coords);
    let good = first_accepted_index(map, &coords, |index| is_good_city_location(map, index));
    // The forced pass places the city but the game reports failure for it,
    // so the caller counts it as a failed call. Confirmed against real games.
    let (chosen, returns) = match good {
        Some(index) => (Some(index), true),
        None => (
            first_accepted_index(map, &coords, |index| {
                is_valid_forced_city_on_continent(map, index, continent_index, continent_of)
            }),
            false,
        ),
    };
    let Some(index) = chosen else {
        return false;
    };
    let tile = &mut map.tiles[index];
    tile.owner = 0;
    tile.terrain = TERRAIN_FIELD;
    tile.resource = 0;
    tile.improvement = IMPROVEMENT_CITY;
    tile.is_city = true;
    returns
}

fn first_accepted_index(
    map: &Map,
    coords: &[WorldCoordinates],
    mut accept: impl FnMut(usize) -> bool,
) -> Option<usize> {
    let width = i32::from(map.width);
    for coord in coords {
        let index = coord.y * width + coord.x;
        if index < 0 || index as usize >= map.tiles.len() {
            continue;
        }
        let index = index as usize;
        if accept(index) {
            return Some(index);
        }
    }
    None
}

/// The forced city location test.
///
/// Edge distance must be positive. Every non-water neighbor in radius 1 must
/// belong to this continent, and at least one such neighbor must exist. Radius
/// 2, including the center, must contain no improvement.
fn is_valid_forced_city_on_continent(
    map: &Map,
    index: usize,
    continent_index: usize,
    continent_of: &[Option<usize>],
) -> bool {
    let Some(tile) = map.tiles.get(index) else {
        return false;
    };
    if distance_to_edge(map, tile.coordinates) <= 0 {
        return false;
    }
    let mut saw_same_land = false;
    for neighbor in area_indices_iter(map, tile.coordinates.x, tile.coordinates.y, 1, true, false) {
        if is_water(map.tiles[neighbor].terrain) {
            continue;
        }
        if continent_of.get(neighbor).copied().flatten() != Some(continent_index) {
            return false;
        }
        saw_same_land = true;
    }
    if !saw_same_land {
        return false;
    }
    !area_indices_iter(map, tile.coordinates.x, tile.coordinates.y, 2, true, true)
        .any(|neighbor| map.tiles[neighbor].improvement != 0)
}

/// Island water scan that runs after the capitals are placed.
///
/// Tiles are shuffled. Budget is `trunc(tile_count * wetness / 50)`. A water
/// tile with no improvement and distance to the edge above 0 is a candidate only when
/// every radius-1 diagonal neighbor is water and radius 2 contains no city.
/// Each placement decrements the budget. The loop stops when the budget is no
/// longer positive, so a zero budget still places one city when a candidate exists.
fn scan_island_water(map: &mut Map, rng: &mut DotNetRandom, wetness: f32) {
    let tile_count = map.tiles.len();
    if tile_count == 0 {
        return;
    }
    let mut order: Vec<usize> = (0..tile_count).collect();
    shuffle_indices(rng, &mut order);
    let mut budget = (tile_count as f32 * wetness / 50.0) as i32;
    for index in order {
        let tile = &map.tiles[index];
        if !is_water(tile.terrain) || tile.improvement != 0 {
            continue;
        }
        if distance_to_edge(map, tile.coordinates) <= 0 {
            continue;
        }
        let touches_land = area_indices_iter(map, tile.coordinates.x, tile.coordinates.y, 1, true, false)
            .any(|neighbor| !is_water(map.tiles[neighbor].terrain));
        if touches_land || is_near_city(map, index, 2) {
            continue;
        }
        let tile = &mut map.tiles[index];
        tile.owner = 0;
        tile.terrain = TERRAIN_FIELD;
        tile.resource = 0;
        tile.improvement = IMPROVEMENT_CITY;
        tile.is_city = true;
        budget -= 1;
        if budget <= 0 {
            break;
        }
    }
}

/// Grows the continents from their start indices.
///
/// Each start index becomes a Field continent. Players other than id `255`
/// increment the continent's capital count, wrapping at `populated_continents_count`.
/// Each continent then expands while its tile count is below
/// `capitals * per_capital + base_per`, at most 1000 times.
pub fn add_continents(
    map: &mut Map,
    rng: &mut DotNetRandom,
    players: &[PlayerState],
    settings: &MapGeneratorSettings,
    starting_positions: &[i32],
    noise: &[f32],
    populated_continents_count: i32,
    continents: &mut Vec<Continent>,
    continent_of: &mut [Option<usize>],
) {
    continents.clear();
    for &index in starting_positions {
        let index = index as usize;
        let mut continent = Continent::default();
        let tile = &mut map.tiles[index];
        tile.terrain = TERRAIN_FIELD;
        tile.resource = 0;
        tile.altitude = 1;
        continent.tiles.push(tile.coordinates);
        continent_of[index] = Some(continents.len());
        continents.push(continent);
    }

    let continent_count = continents.len() as i32;
    if continent_count <= 0 {
        return;
    }
    let mut target = (1.0 - settings.wetness) * map.tiles.len() as f32;
    if continent_count >= 2 {
        target *= 1.0 + 0.5 / continent_count as f32;
    }
    // The game divides by the players that can own a capital; Nature (id 255)
    // is in the list but not in the count (confirmed against real games: limit 101.25 with 6 + Nature).
    let player_count = players.iter().filter(|player| player.id != 255).count() as f32;
    let per_capital = (target * 0.5) / player_count;
    let base_per = (target - target * 0.5) / continent_count as f32;

    let mut slot = 0i32;
    for player in players {
        if player.id == 255 {
            continue;
        }
        continents[slot as usize].number_of_capitals += 1;
        slot += 1;
        if populated_continents_count <= 0 || slot >= populated_continents_count {
            slot = 0;
        }
    }

    let mut exhausted = crate::map::IndexSet::with_len(map.tiles.len());
    #[allow(clippy::needless_range_loop)] // the loop body mutates `continents` and uses the index as an id
    for continent_index in 0..continents.len() {
        let mut attempts = 0i32;
        loop {
            let count = continents[continent_index].tiles.len() as f32;
            let limit = continents[continent_index].number_of_capitals as f32 * per_capital + base_per;
            if limit <= count || attempts >= 1000 {
                break;
            }
            attempts += 1;
            expand_continent(
                map,
                rng,
                &mut continents[continent_index],
                continent_index,
                continent_of,
                noise,
                settings.map_type,
                &mut exhausted,
            );
        }
    }
}

/// Expands one continent by a tile.
///
/// Shuffles the continent's tiles and expands from the first seed that is not
/// in `exhausted`. Claims the unowned orthogonal neighbour with the highest
/// score. When none scores above 0 that seed is marked exhausted and the next
/// shuffled tile is tried in the same call.
pub fn expand_continent(
    map: &mut Map,
    rng: &mut DotNetRandom,
    continent: &mut Continent,
    continent_index: usize,
    continent_of: &mut [Option<usize>],
    noise: &[f32],
    map_type: MapPreset,
    exhausted: &mut crate::map::IndexSet,
) -> bool {
    let width = i32::from(map.width);
    let mut seeds = continent.tiles.clone();
    shuffle_coords(rng, &mut seeds);
    for seed in seeds {
        let seed_index = seed.y.wrapping_mul(width).wrapping_add(seed.x);
        if exhausted.contains(i64::from(seed_index)) {
            continue;
        }
        let mut neighbors = orthogonal_neighbors(map, seed.x, seed.y);
        shuffle_indices(rng, &mut neighbors);
        let mut best_score = 0.0f32;
        let mut chosen: Option<usize> = None;
        for neighbor in neighbors {
            if continent_of[neighbor].is_some() {
                continue;
            }
            let mut score = noise[neighbor] + 1.0;
            let edge = distance_to_edge(map, map.tiles[neighbor].coordinates) as f32;
            if map_type == MapPreset::Pangea {
                score += -1.0 / edge;
            }
            if map_type == MapPreset::Continents {
                score += edge / -5.0;
            }
            let candidate = map.tiles[neighbor].coordinates;
            for around in area_indices_iter(map, candidate.x, candidate.y, 1, true, false) {
                let owner = continent_of[around];
                if owner.is_none() || owner == Some(continent_index) {
                    continue;
                }
                if is_water(map.tiles[around].terrain) {
                    score -= range_f32(rng, 0.0, 0.5);
                } else {
                    score -= range_f32(rng, 0.5, 3.0);
                    let other = map.tiles[around].coordinates;
                    // The penalty applies when x OR y is equal.
                    if other.x == candidate.x || other.y == candidate.y {
                        score += -10.0;
                    }
                }
            }
            if score > best_score {
                best_score = score;
                chosen = Some(neighbor);
            }
        }
        if let Some(neighbor) = chosen {
            continent_of[neighbor] = Some(continent_index);
            let tile = &mut map.tiles[neighbor];
            tile.terrain = TERRAIN_FIELD;
            tile.resource = 0;
            tile.altitude = 1;
            continent.tiles.push(tile.coordinates);
            return true;
        }
        // No neighbour scored above 0: the game marks this seed exhausted and
        // goes on to the next shuffled tile without reshuffling (confirmed against real games).
        exhausted.insert(i64::from(seed_index));
    }
    false
}

/// Assigns climates.
///
/// Replaces `continents` with `players.len() - 1` climate continents.
/// Assigning a player looks the tribe up in the game data ([`GameLogicData::tribe_by_idx`]). A miss leaves climate 0.
/// A hit stores [`DEFAULT_CLIMATE`] when the tribe has `alienclimate` and every
/// looked-up player does too; otherwise it stores the player's climate,
/// which is `player.tribe` while the player's own climate field is 0.
///
/// Wetness `0` clears the water and ocean modifiers. The radius-1 diagonal
/// spread runs only when the continent count is greater than `player_count - 1`.
/// This allocation makes them equal, so the spread does not paint tiles.
pub fn add_climates(
    map: &mut Map,
    rng: &mut DotNetRandom,
    players: &[PlayerState],
    settings: &MapGeneratorSettings,
    continents: &mut Vec<Continent>,
    continent_of: &mut [Option<usize>],
) {
    let slots = players.len().saturating_sub(1);
    if slots == 0 {
        return;
    }
    let data = crate::data::embedded_game_logic_ref();
    let fallback = data.as_ref().is_some_and(|data| requires_fallback_climate(data, players));
    continents.clear();
    for player in players.iter().take(slots) {
        let mut continent = Continent::default();
        if let Some(data) = data.as_ref() {
            apply_player_climate(&mut continent, player, data, fallback);
        }
        if settings.wetness == 0.0 {
            continent.water = 0.0;
            continent.ocean = 0.0;
        }
        continents.push(continent);
    }

    // Flood fill (confirmed against real games). Each pass visits every climate
    // continent in order; one that has tiles picks a random one of them, then a
    // random free neighbour of it. `placed` starts at the seed count and the loop
    // ends at `tile_count` or when the budget (only spent on skips and removals)
    // runs out.
    let tile_count = map.tiles.len() as i32;
    let count = slots as i32;
    if count >= tile_count {
        return;
    }
    let mut placed = count;
    let mut budget = tile_count.wrapping_mul(10);
    let ring = super::Ring1::for_map(map);
    while placed < tile_count && budget > 0 {
        let mut j = 0usize;
        while j < slots && placed < tile_count && budget > 0 {
            let skip = continents[j].tiles.is_empty() || (continents[j].has_alien_climate && !fallback);
            if skip {
                budget -= 1;
                j += 1;
                continue;
            }
            let pick = rng.next_range(0, continents[j].tiles.len() as i32) as usize;
            let center = continents[j].tiles[pick];
            let width = i32::from(map.width);
            let height = i32::from(map.height);
            if center.x < 0 || center.y < 0 || center.x >= width || center.y >= height {
                // The game logs an error and carries on with a null tile (it would then throw).
                return;
            }
            // At most 8 neighbours: fixed buffers instead of vectors, same order. Each one is written and kept by
            // the test's result, without a branch on the (random) terrain.
            let area = ring.around(center.x, center.y).unwrap_or_default();
            let (mut candidates, mut len) = ([0usize; 8], 0);
            for i in area.iter().map(|&i| i as usize) {
                let t = &map.tiles[i];
                candidates[len] = i;
                len += usize::from((t.climate == 0) & (t.altitude >= 0));
            }
            if len == 0 {
                for i in area.iter().map(|&i| i as usize) {
                    candidates[len] = i;
                    len += usize::from(map.tiles[i].climate == 0);
                }
            }
            if len == 0 {
                // Remove the centre from the list, then visit the same continent again.
                if let Some(at) = continents[j].tiles.iter().position(|c| *c == center) {
                    continents[j].tiles.remove(at);
                }
                budget -= 1;
                continue;
            }
            let chosen = candidates[rng.next_range(0, len as i32) as usize];
            let climate = continents[j].climate;
            let tile = &mut map.tiles[chosen];
            tile.climate = climate;
            if tile.altitude >= 0 {
                tile.altitude = 1;
                tile.terrain = TERRAIN_FIELD;
                tile.resource = 0;
            }
            let coordinates = tile.coordinates;
            continent_of[chosen] = Some(j);
            continents[j].tiles.push(coordinates);
            placed += 1;
            j += 1;
        }
    }
}

/// The player's climate once the tribe lookup has succeeded.
/// The player's own climate field is not on [`PlayerState`], so this is the tribe index.
fn player_climate(player: &PlayerState, tribe: &TribeData, fallback: bool) -> i32 {
    if fallback && has_alien_climate(tribe) {
        DEFAULT_CLIMATE
    } else {
        player.tribe
    }
}

/// Tail of assigning a player to a continent: version-dependent defaults,
/// then the terrain modifiers and the resource modifiers.
///
/// Terrain: field, mountain, forest and ice multiply the current value. Water is
/// `value * 0.2` when `value > 1`, and ocean multiplies when `value > 1`.
/// Resource: `value * (current > 0 ? current : 1.0)`; starfish is ignored.
/// Only version 123 is modelled for the defaults (whale stays 0, aquacrop 0.3).
fn set_player_modifiers(continent: &mut Continent, tribe: &TribeData) {
    continent.aquacrop = f32::from_bits(0x3e99_999a);
    for (kind, value) in &tribe.terrain_modifier {
        let value = *value as f32;
        match kind.as_str() {
            "water" if value > 1.0 => continent.water = value * 0.2,
            "ocean" if value > 1.0 => continent.ocean *= value,
            "field" => continent.field *= value,
            "mountain" => continent.mountain *= value,
            "forest" => continent.forest *= value,
            "ice" => continent.ice *= value,
            _ => {}
        }
    }
    let scale = |current: f32, value: f32| value * if current > 0.0 { current } else { 1.0 };
    for (kind, value) in &tribe.resource_modifier {
        let value = *value as f32;
        match kind.as_str() {
            "game" => continent.game = scale(continent.game, value),
            "crop" => continent.crop = scale(continent.crop, value),
            "fish" => continent.fish = scale(continent.fish, value),
            "whale" => continent.whale = scale(continent.whale, value),
            "metal" => continent.metal = scale(continent.metal, value),
            "fruit" => continent.fruit = scale(continent.fruit, value),
            "spores" => continent.spores = scale(continent.spores, value),
            "aquacrop" => continent.aquacrop = scale(continent.aquacrop, value),
            _ => {}
        }
    }
}

/// The resource weights a continent takes from a player's tribe (as the game assigns a player to a continent, without a fallback climate).
pub fn tribe_continent_modifiers(tribe: &TribeData) -> super::contents::ContinentModifiers {
    let mut continent = Continent::default();
    set_player_modifiers(&mut continent, tribe);
    super::contents::ContinentModifiers {
        game: continent.game,
        crop: continent.crop,
        fish: continent.fish,
        whale: continent.whale,
        metal: continent.metal,
        fruit: continent.fruit,
        spores: continent.spores,
        aquacrop: continent.aquacrop,
    }
}

fn has_alien_climate(tribe: &TribeData) -> bool {
    tribe.tribe_abilities.iter().any(|ability| ability == "alienclimate")
}

/// True when every player whose tribe loads has the alien climate ability.
/// A failed lookup is skipped. The flag is also true when nobody loads.
fn requires_fallback_climate(data: &GameLogicData, players: &[PlayerState]) -> bool {
    for player in players {
        if player.id == 255 {
            continue;
        }
        let Some(tribe) = data.tribe_by_idx(player.tribe) else {
            continue;
        };
        if !has_alien_climate(tribe) {
            return false;
        }
    }
    true
}

fn apply_player_climate(continent: &mut Continent, player: &PlayerState, data: &GameLogicData, fallback: bool) {
    let Some(tribe) = data.tribe_by_idx(player.tribe) else {
        return;
    };
    continent.climate = player_climate(player, tribe, fallback);
    continent.has_alien_climate = has_alien_climate(tribe);
    set_player_modifiers(continent, tribe);
    continent.tiles.push(player.start_tile);
}

/// Bonus from the tribe data and the climate for each capital.
fn tribe_capital_args(players: &[PlayerState]) -> (Vec<i32>, Vec<i32>) {
    let data = crate::data::embedded_game_logic_ref();
    let mut bonus = Vec::with_capacity(players.len());
    let mut climate = Vec::with_capacity(players.len());
    for player in players {
        match data.as_ref().and_then(|data| data.tribe_by_idx(player.tribe)) {
            Some(tribe) => {
                bonus.push(tribe.bonus);
                climate.push(player.tribe);
            }
            None => {
                bonus.push(0);
                climate.push(0);
            }
        }
    }
    (bonus, climate)
}

fn continent_tile_indices(map: &Map, continents: &[Continent]) -> Vec<Vec<i32>> {
    let width = i32::from(map.width);
    continents
        .iter()
        .map(|continent| {
            continent.tiles.iter().map(|coord| coord.y.wrapping_mul(width).wrapping_add(coord.x)).collect()
        })
        .collect()
}

fn reassign_capitals_for_west(capitals: &mut [i32], width: i32, west_player: Option<usize>) {
    let Some(player) = west_player else {
        return;
    };
    if player >= capitals.len() {
        panic!("west player index {player} is outside the capital list");
    }
    let mut best_score = i32::MIN;
    let mut best = -1i32;
    for (index, &tile) in capitals.iter().enumerate() {
        let x = tile % width;
        let y = tile / width;
        let score = y.wrapping_sub(x);
        if score > best_score {
            best_score = score;
            best = index as i32;
        }
    }
    if best >= 0 {
        capitals.swap(player, best as usize);
    }
}

/// Capital neighbour indices: `+1`, `-1`, `+width`, `-width`.
/// Out-of-map results are omitted; the game does not check and would fault
/// while smoothing the noise.
fn capital_neighbor_indices(capitals: &[i32], width: i32, tile_count: i32) -> Vec<i32> {
    let mut neighbors = Vec::new();
    for &index in capitals {
        for neighbor in [index + 1, index - 1, index + width, index - width] {
            if (0..tile_count).contains(&neighbor) {
                neighbors.push(neighbor);
            }
        }
    }
    neighbors
}

/// Dry-land budgets for divisors 2..=7. A value in `[30, 200]` is kept.
fn continent_island_quotas(tile_count: i32, wetness: f32) -> Vec<i32> {
    let dry = 1.0 - wetness;
    let mut quotas = Vec::new();
    for divisor in 2..=7 {
        let budget = dry * (tile_count / divisor) as f32;
        if (30.0..=200.0).contains(&budget) {
            quotas.push(divisor);
        }
    }
    quotas
}

/// Shrink the populated-continent count while `1 < max(1, players/count) < 2`.
fn shrink_populated_count(mut count: i32, player_count: i32) -> i32 {
    while count >= 2 {
        let ratio = 1.0f32.max(player_count as f32 / count as f32);
        if ratio >= 2.0 || ratio == 1.0 {
            return count;
        }
        let previous = count;
        count -= 1;
        if previous <= 2 {
            return 1;
        }
    }
    count
}

fn orthogonal_neighbors(map: &Map, x: i32, y: i32) -> Vec<usize> {
    area_indices(map, x, y, 1, false, false)
}

fn area_indices(map: &Map, x: i32, y: i32, radius: i32, allow_diagonal: bool, include_center: bool) -> Vec<usize> {
    area_indices_iter(map, x, y, radius, allow_diagonal, include_center).collect()
}

/// [`area_indices`] without the allocation: same indices, same order (outer `dx`, inner `dy`).
fn area_indices_iter(
    map: &Map,
    x: i32,
    y: i32,
    radius: i32,
    allow_diagonal: bool,
    include_center: bool,
) -> impl Iterator<Item = usize> {
    let width = i32::from(map.width);
    let height = i32::from(map.height);
    crate::rules::path::square_offsets(radius).filter_map(move |(dx, dy)| {
        if dx == 0 && dy == 0 && !include_center {
            return None;
        }
        if !allow_diagonal && dx.abs() + dy.abs() > radius {
            return None;
        }
        let (nx, ny) = (x + dx, y + dy);
        if nx < 0 || ny < 0 || nx >= width || ny >= height {
            return None;
        }
        Some((ny * width + nx) as usize)
    })
}

fn shuffle_indices(rng: &mut DotNetRandom, list: &mut [usize]) {
    let mut count = list.len() as i32;
    let mut limit = count - 1;
    if limit <= 0 {
        return;
    }
    let mut index = 0i32;
    while index < limit {
        let swap_with = rng.next_range(index, count);
        list.swap(index as usize, swap_with as usize);
        index += 1;
        count = list.len() as i32;
        limit = count - 1;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Map;

    fn bits(values: &[f32]) -> Vec<u32> {
        values.iter().copied().map(f32::to_bits).collect()
    }

    #[test]
    fn ring1_matches_area_iterator() {
        for (w, h) in [(1u16, 1u16), (2, 2), (3, 5), (11, 11), (16, 16), (7, 4)] {
            let map = Map::new(w, h);
            let ring = crate::map::Ring1::for_map(&map);
            for y in -1..=i32::from(h) {
                for x in -1..=i32::from(w) {
                    let expected: Vec<usize> = area_indices_iter(&map, x, y, 1, true, false).collect();
                    match ring.around(x, y) {
                        Some(around) => assert_eq!(
                            around.iter().map(|&i| i as usize).collect::<Vec<_>>(),
                            expected,
                            "{w}x{h} at ({x}, {y})"
                        ),
                        None => {
                            assert!(x < 0 || y < 0 || x >= i32::from(w) || y >= i32::from(h), "{w}x{h} at ({x}, {y})")
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn smooth_noise_matches_three_by_three() {
        let mut noise: Vec<f32> = (0..9).map(|i| (i as f32) / 10.0).collect();
        let original = noise.clone();
        smooth_noise(3, 3, &mut noise, &[4], 0, 0.5);
        assert_eq!(noise, original);

        smooth_noise(3, 3, &mut noise, &[4], 1, 0.5);
        assert_eq!(
            bits(&noise),
            vec![
                0x3ebbbbbc, 0x3eaaaaac, 0x3ed27d29, 0x3ecccccd, 0x3ecccccd, 0x3eeeeeee, 0x3f000000, 0x3f088889,
                0x3f0b60b5,
            ]
        );

        smooth_noise(3, 3, &mut noise, &[4], 1, 0.5);
        assert_eq!(
            bits(&noise),
            vec![
                0x3ee38e3b, 0x3edc9a3c, 0x3ee9e067, 0x3ee573ae, 0x3ecccccd, 0x3eee4d1c, 0x3ef684bc, 0x3ef7268e,
                0x3efcd6eb,
            ]
        );
    }

    #[test]
    fn set_terrain_thresholds_cities_and_land() {
        let mut map = Map::new(4, 1);
        let noise = [0.1, 0.2, 0.8, 0.9];
        set_terrain_from_noise(&mut map, &noise, &[], &[], &[], &[], 0.5, 0.0);
        // With no shallow fraction nothing is Ocean at this stage (retail Lakes capture).
        assert_eq!(
            map.tiles.iter().map(|t| t.terrain).collect::<Vec<_>>(),
            vec![TERRAIN_WATER, TERRAIN_WATER, TERRAIN_FIELD, TERRAIN_FIELD]
        );

        let mut map = Map::new(4, 1);
        set_terrain_from_noise(&mut map, &noise, &[], &[], &[], &[], 0.5, 0.5);
        assert_eq!(
            map.tiles.iter().map(|t| t.terrain).collect::<Vec<_>>(),
            vec![TERRAIN_OCEAN, TERRAIN_WATER, TERRAIN_FIELD, TERRAIN_FIELD]
        );

        let mut map = Map::new(4, 1);
        let player = PlayerState {
            id: 3,
            handicap: 0,
            currency: 0,
            score: 0,
            tribe: DEFAULT_CLIMATE,
            auto_play: false,
            start_tile: WorldCoordinates { x: 0, y: 0 },
        };
        set_terrain_from_noise(&mut map, &noise, &[0, 2], &[0], &[1], &[player], 0.5, 0.0);
        assert_eq!(
            map.tiles.iter().map(|t| t.terrain).collect::<Vec<_>>(),
            vec![TERRAIN_FIELD, TERRAIN_FIELD, TERRAIN_FIELD, TERRAIN_FIELD]
        );
        assert!(map.tiles[0].is_capital && map.tiles[0].is_city);
        assert_eq!(map.tiles[0].owner, 3);
        assert_eq!(map.tiles[0].climate, DEFAULT_CLIMATE);
        assert_eq!(map.tiles[0].improvement, IMPROVEMENT_CITY);
        assert!(map.tiles[2].is_city && !map.tiles[2].is_capital);
        assert_eq!(map.tiles[2].owner, 0);
        assert_eq!(map.tiles[2].improvement, IMPROVEMENT_CITY);
        assert!(!map.tiles[1].is_city);
    }

    #[test]
    fn terrain_bag_uses_default_continent_ratios() {
        let continent = Continent::default();
        assert_eq!(
            terrain_bag(continent.field, continent.mountain, continent.forest, 10),
            vec![
                TERRAIN_FIELD,
                TERRAIN_FIELD,
                TERRAIN_FIELD,
                TERRAIN_FIELD,
                TERRAIN_FIELD,
                TERRAIN_MOUNTAIN,
                TERRAIN_MOUNTAIN,
                TERRAIN_FOREST,
                TERRAIN_FOREST,
                TERRAIN_FOREST,
                TERRAIN_FOREST,
            ]
        );
        assert!(terrain_bag(0.0, 0.0, 0.0, 10).is_empty());
    }

    #[test]
    fn patch_continents_counts_open_land() {
        let mut map = Map::new(2, 2);
        map.tiles[0].terrain = TERRAIN_FIELD;
        map.tiles[2].terrain = TERRAIN_WATER;
        map.tiles[3].terrain = TERRAIN_FIELD;
        map.tiles[3].improvement = IMPROVEMENT_CITY;
        let mut continents = vec![Continent::default(), Continent::default()];
        patch_continents(&map, &mut continents, &[Some(0), None, Some(0), Some(1)]);
        assert_eq!(continents[0].land_tile_count, 1);
        assert_eq!(continents[0].tiles, vec![WorldCoordinates { x: 0, y: 0 }, WorldCoordinates { x: 0, y: 1 },]);
        assert_eq!(continents[1].land_tile_count, 0);
        assert_eq!(continents[1].tiles, vec![WorldCoordinates { x: 1, y: 1 }]);
    }

    #[test]
    fn add_terrain_deals_default_bag_with_seed_one() {
        let mut map = Map::new(2, 1);
        let mut continents = vec![Continent::default()];
        let mut rng = DotNetRandom::new(1);
        add_terrain(&mut map, &mut rng, &mut continents, &[Some(0), Some(0)]);
        assert_eq!(
            map.tiles.iter().map(|tile| tile.terrain).collect::<Vec<_>>(),
            vec![TERRAIN_FIELD, TERRAIN_MOUNTAIN]
        );
        assert_eq!(continents[0].land_tile_count, 2);
    }

    #[test]
    fn noise_city_schedule_for_width_16() {
        assert_eq!(noise_domain_side(16), 5);
        assert_eq!(post_terrain_city_budget(16, 4, 1.0), 21);
        assert_eq!(suburb_count(21, 2, 1, 2), 2);
        assert_eq!(pre_terrain_city_count(16, 4, 2, 0.3), 5);
    }

    #[test]
    fn round_half_to_even_matches_math_round() {
        assert_eq!(round_to_even(1.5), 2);
        assert_eq!(round_to_even(2.5), 2);
        assert_eq!(round_to_even(0.5), 0);
        assert_eq!(round_to_even(6.75), 7);
        assert_eq!(round_to_even(-1.5), -2);
        assert_eq!(round_to_even(-2.5), -2);
    }

    #[test]
    fn make_ocean_keeps_shallows_beside_land() {
        let mut map = Map::new(3, 3);
        for tile in &mut map.tiles {
            tile.terrain = TERRAIN_WATER;
        }
        map.tiles[4].terrain = TERRAIN_FIELD;
        make_ocean(&mut map, true);
        let terrains: Vec<i32> = map.tiles.iter().map(|t| t.terrain).collect();
        assert_eq!(
            terrains,
            vec![
                TERRAIN_OCEAN,
                TERRAIN_WATER,
                TERRAIN_OCEAN,
                TERRAIN_WATER,
                TERRAIN_FIELD,
                TERRAIN_WATER,
                TERRAIN_OCEAN,
                TERRAIN_WATER,
                TERRAIN_OCEAN,
            ]
        );
    }

    #[test]
    fn generate_noise_seed_one_biases_land_cell() {
        let mut rng = DotNetRandom::new(1);
        let noise = generate_noise(&mut rng, 2, 2, &[0], 0.5);
        assert_eq!(bits(&noise), vec![0x3f1fd45f, 0x3de2cdbd, 0x3eef1c06, 0x3f4587d9]);
    }

    fn player(id: u8, tribe: i32) -> PlayerState {
        PlayerState {
            id,
            handicap: 0,
            currency: 0,
            score: 0,
            tribe,
            auto_play: false,
            start_tile: WorldCoordinates { x: 0, y: 0 },
        }
    }

    #[test]
    fn expand_continent_seed_one_claims_the_west_neighbor() {
        let mut map = Map::new(5, 5);
        let mut rng = DotNetRandom::new(1);
        let mut continent = Continent::default();
        continent.tiles.push(map.tiles[12].coordinates);
        continent.number_of_capitals = 1;
        let mut continent_of = vec![None; map.tiles.len()];
        continent_of[12] = Some(0);
        map.tiles[12].terrain = TERRAIN_FIELD;
        let noise = vec![1.0; map.tiles.len()];
        let mut exhausted = crate::map::IndexSet::with_len(map.tiles.len());
        assert!(expand_continent(
            &mut map,
            &mut rng,
            &mut continent,
            0,
            &mut continent_of,
            &noise,
            MapPreset::None,
            &mut exhausted,
        ));
        assert_eq!(continent.tiles, vec![WorldCoordinates { x: 2, y: 2 }, WorldCoordinates { x: 1, y: 2 },]);
        assert_eq!(map.tiles[11].terrain, TERRAIN_FIELD);
    }

    #[test]
    fn add_continents_seed_one_grows_to_the_size_target() {
        let mut map = Map::new(5, 5);
        let mut rng = DotNetRandom::new(1);
        let players = [player(0, 2)];
        let settings = MapGeneratorSettings { wetness: 0.0, map_type: MapPreset::None, ..Default::default() };
        let noise = vec![1.0; map.tiles.len()];
        let mut continents = Vec::new();
        let mut continent_of = vec![None; map.tiles.len()];
        add_continents(&mut map, &mut rng, &players, &settings, &[12], &noise, 1, &mut continents, &mut continent_of);
        assert_eq!(continents.len(), 1);
        assert_eq!(continents[0].number_of_capitals, 1);
        assert_eq!(continents[0].tiles.len(), 25);
        assert!(map.tiles.iter().all(|tile| tile.terrain == TERRAIN_FIELD));
    }

    #[test]
    fn add_climates_builds_one_slot_per_player_except_the_last() {
        let mut map = Map::new(4, 4);
        let mut rng = DotNetRandom::new(1);
        let players = [player(0, 3), player(1, 4), player(2, 5)];
        let settings = MapGeneratorSettings { wetness: 0.0, ..Default::default() };
        let mut continents = vec![Continent::default()];
        let mut continent_of = vec![None; map.tiles.len()];
        add_climates(&mut map, &mut rng, &players, &settings, &mut continents, &mut continent_of);
        assert_eq!(continents.len(), 2);
        assert_eq!(continents[0].climate, 3);
        assert_eq!(continents[1].climate, 4);
        assert!(continents.iter().all(|continent| continent.water == 0.0 && continent.ocean == 0.0));
        // The flood fill counts the two seeds as placed even though they share a
        // tile, so it stops at the map size one claim early: 14 of the 16 tiles are claimed.
        let claimed = map.tiles.iter().filter(|tile| tile.climate == 3 || tile.climate == 4).count();
        assert_eq!(claimed, 14);
        assert_eq!(continent_of.iter().filter(|slot| slot.is_some()).count(), 14);
    }

    #[test]
    fn try_generate_without_island_starts_returns_true() {
        let mut map = Map::new(4, 4);
        let mut rng = DotNetRandom::new(1);
        let mut players = [player(0, 2), player(1, 3)];
        let settings = MapGeneratorSettings { map_type: MapPreset::None, wetness: 0.0, ..Default::default() };
        let mut continents = Vec::new();
        let mut continent_of = vec![None; map.tiles.len()];
        let ok = try_generate_map_from_islands(
            &mut map,
            &mut rng,
            &mut players,
            &settings,
            &mut continents,
            &mut continent_of,
            None,
        );
        assert!(ok);
        assert_eq!(continents.len(), 1);
        assert!(map.tiles.iter().all(|tile| tile.terrain == TERRAIN_NONE));
    }

    #[test]
    fn try_generate_continents_quota_miss_returns_false() {
        let mut map = Map::new(2, 2);
        let mut rng = DotNetRandom::new(1);
        let mut players = [player(0, 2)];
        let settings = MapGeneratorSettings { map_type: MapPreset::Continents, wetness: 0.0, ..Default::default() };
        let mut continents = Vec::new();
        let mut continent_of = vec![None; map.tiles.len()];
        let ok = try_generate_map_from_islands(
            &mut map,
            &mut rng,
            &mut players,
            &settings,
            &mut continents,
            &mut continent_of,
            None,
        );
        assert!(!ok);
        assert!(continents.is_empty());
    }

    #[test]
    fn generate_map_from_noise_seed_one_places_capitals() {
        let mut map = Map::new(11, 11);
        let mut rng = DotNetRandom::new(1);
        let mut players = [player(0, 2), player(1, 4)];
        let settings = MapGeneratorSettings::default();
        let mut continents = Vec::new();
        let mut continent_of = vec![None; map.tiles.len()];
        generate_map_from_noise(&mut map, &mut rng, &mut players, &settings, &mut continents, &mut continent_of, None);
        let capitals: Vec<(i32, u8, i32)> = map
            .tiles
            .iter()
            .filter(|tile| tile.is_capital)
            .map(|tile| (tile.index, tile.owner, tile.climate))
            .collect();
        assert_eq!(capitals, vec![(36, 0, 2), (91, 1, 4)]);
        assert_eq!(continents.len(), 1);
        assert_eq!(continents[0].climate, 2);
        let terrains: Vec<i32> = map.tiles.iter().map(|tile| tile.terrain).collect();
        let count = |want: i32| terrains.iter().filter(|terrain| **terrain == want).count();
        assert_eq!(count(TERRAIN_FIELD), 68);
        assert_eq!(count(TERRAIN_WATER), 53);
        assert_eq!(count(TERRAIN_OCEAN), 0);
    }

    #[test]
    fn island_seed_one_places_a_capital_with_tribe_climate() {
        let mut map = Map::new(8, 8);
        let mut rng = DotNetRandom::new(1);
        let mut players = [player(0, 2), player(1, 4)];
        let settings = MapGeneratorSettings { map_type: MapPreset::Continents, wetness: 0.0, ..Default::default() };
        let mut continents = Vec::new();
        let mut continent_of = vec![None; map.tiles.len()];
        let ok = try_generate_map_from_islands(
            &mut map,
            &mut rng,
            &mut players,
            &settings,
            &mut continents,
            &mut continent_of,
            None,
        );
        assert!(ok);
        let capitals: Vec<(u8, i32)> =
            map.tiles.iter().filter(|tile| tile.is_capital).map(|tile| (tile.owner, tile.climate)).collect();
        assert_eq!(capitals, vec![(1, 4), (0, 2)]);
        assert_eq!(continents[0].climate, 2);
    }

    #[test]
    fn add_city_to_continent_seed_one_places_one_city() {
        let mut map = Map::new(7, 7);
        for tile in &mut map.tiles {
            tile.terrain = TERRAIN_FIELD;
        }
        let continent =
            Continent { tiles: map.tiles.iter().map(|tile| tile.coordinates).collect(), ..Default::default() };
        let continent_of = vec![Some(0usize); map.tiles.len()];
        let mut rng = DotNetRandom::new(1);
        let placed = add_city_to_continent(&mut map, &mut rng, &continent, 0, &continent_of);
        assert!(placed);
        let cities: Vec<usize> = map
            .tiles
            .iter()
            .enumerate()
            .filter(|(_, tile)| tile.is_city)
            .map(|(index, tile)| {
                assert_eq!(tile.owner, 0);
                assert_eq!(tile.terrain, TERRAIN_FIELD);
                assert_eq!(tile.resource, 0);
                assert_eq!(tile.improvement, IMPROVEMENT_CITY);
                assert!(!tile.is_capital);
                index
            })
            .collect();
        assert_eq!(cities, vec![32]);
    }

    #[test]
    fn island_water_scan_seed_one_places_one_city_at_zero_budget() {
        let mut map = Map::new(5, 5);
        for tile in &mut map.tiles {
            tile.terrain = TERRAIN_WATER;
        }
        let mut rng = DotNetRandom::new(1);
        scan_island_water(&mut map, &mut rng, 0.0);
        let cities: Vec<usize> = map
            .tiles
            .iter()
            .enumerate()
            .filter(|(_, tile)| tile.is_city)
            .map(|(index, tile)| {
                assert_eq!(tile.terrain, TERRAIN_FIELD);
                assert_eq!(tile.improvement, IMPROVEMENT_CITY);
                assert_eq!(tile.owner, 0);
                index
            })
            .collect();
        assert_eq!(cities, vec![6]);

        let mut shore = Map::new(3, 3);
        for tile in &mut shore.tiles {
            tile.terrain = TERRAIN_FIELD;
        }
        shore.tiles[4].terrain = TERRAIN_WATER;
        let mut shore_rng = DotNetRandom::new(1);
        scan_island_water(&mut shore, &mut shore_rng, 0.0);
        assert!(shore.tiles.iter().all(|tile| !tile.is_city));
    }
}
