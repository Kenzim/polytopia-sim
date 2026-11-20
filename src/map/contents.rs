//! Cities, resources, ruins, starfish, and map fairness.
//!
//! Formulas reproduce the game's. See `docs/specs/map-contents.md`.
//! The .NET-compatible random generator and the seeded hash come from [`crate::rng`]; this module does not reimplement them.

use crate::data::GameLogicData;
use crate::rng::{self, DotNetRandom, XxHash};
use crate::{Map, PlayerState, Tile, WorldCoordinates};

/// Map fairness scoring constants.
pub const SCORE_PER_CITY: f32 = 100.0;
pub const CITY_DISTANCE_PENALTY: f32 = 90.0;
pub const VERY_CLOSE_CITY_BONUS: f32 = 40.0;
pub const CLOSE_CITY_BONUS: f32 = 20.0;
pub const SCORE_PER_RUIN: f32 = 5.0;
pub const RUIN_DISTANCE_PENALTY: f32 = 3.0;
pub const CLOSE_RUIN_BONUS: f32 = 5.0;
pub const SCORE_PER_LAND_TILE: f32 = 1.5;
pub const LAND_TILE_DISTANCE_PENALTY: f32 = 1.0;
pub const SCORE_PER_TILE: f32 = 0.1;

/// The inequality ratio substitutes this when the smaller turf score is not positive.
pub const INEQUALITY_MIN_FLOOR: f32 = 0.001;

/// Map generation treats a turf ratio below this as a failed attempt.
pub const REJECTED_INEQUALITY: f32 = -999.0;

/// Terrain type ids used by the checks below.
pub const TERRAIN_WATER: i32 = 1;
pub const TERRAIN_OCEAN: i32 = 2;
pub const TERRAIN_FIELD: i32 = 3;
pub const TERRAIN_MOUNTAIN: i32 = 4;
pub const TERRAIN_ICE: i32 = 6;

pub const IMPROVEMENT_CITY: i32 = 1;
pub const IMPROVEMENT_RUIN: i32 = 2;
/// Fungi improvement type. Placing a capital writes this on field neighbors for the Spores bonus.
pub const IMPROVEMENT_FUNGI: i32 = 37;
pub const IMPROVEMENT_LIGHTHOUSE: i32 = 47;

/// Tribe bonus ids.
pub const BONUS_CITY_LEVEL_3: i32 = 1;
pub const BONUS_SPORES: i32 = 2;

pub const RESOURCE_GAME: i32 = 1;
pub const RESOURCE_CROP: i32 = 2;
pub const RESOURCE_FISH: i32 = 3;
pub const RESOURCE_WHALE: i32 = 4;
pub const RESOURCE_METAL: i32 = 5;
pub const RESOURCE_FRUIT: i32 = 6;
pub const RESOURCE_SPORES: i32 = 7;
pub const RESOURCE_STARFISH: i32 = 8;
pub const RESOURCE_AQUACROP: i32 = 9;

/// Player id of Nature.
pub const NATURE_PLAYER_ID: u8 = 255;

/// The game's player count: the player list length minus one, i.e. the players that can
/// own a capital. The list always ends with Nature (id 255), which is not counted.
pub fn player_count(players: &[PlayerState]) -> i32 {
    players.iter().filter(|player| player.id != NATURE_PLAYER_ID).count() as i32
}

/// Unit-hash scale, `1/2^32`.
const HASH_TO_UNIT: f32 = f32::from_bits(0x2f80_0000);

/// Width term in the per-tile turf score.
const TURF_WIDTH_SCALE: f32 = 0.8;

/// Per-continent resource weights.
///
/// Starfish has no weight; it reads as `0`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ContinentModifiers {
    pub game: f32,
    pub crop: f32,
    pub fish: f32,
    pub whale: f32,
    pub metal: f32,
    pub fruit: f32,
    pub spores: f32,
    pub aquacrop: f32,
}

impl ContinentModifiers {
    pub fn get(self, resource_kind: i32) -> f32 {
        match resource_kind {
            RESOURCE_GAME => self.game,
            RESOURCE_CROP => self.crop,
            RESOURCE_FISH => self.fish,
            RESOURCE_WHALE => self.whale,
            RESOURCE_METAL => self.metal,
            RESOURCE_FRUIT => self.fruit,
            RESOURCE_SPORES => self.spores,
            RESOURCE_AQUACROP => self.aquacrop,
            _ => 0.0,
        }
    }
}

/// One resource row: `idx` is the resource type, and
/// `resource_terrain_requirements` is the allowed terrain list (terrain `idx` values).
/// An empty list means every terrain is allowed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ResourceSpawn {
    pub kind: i32,
    pub terrains: Vec<i32>,
}

/// One tile of a player's turf: coordinates, path cost, owning player id.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TurfTile {
    pub coordinates: WorldCoordinates,
    pub cost: i32,
    pub player: i32,
}

/// True when the terrain is water or ocean.
pub fn is_water(tile: &Tile) -> bool {
    let terrain = tile.terrain - 1;
    (0..2).contains(&terrain)
}

/// True when the terrain is field, mountain, forest, or ice.
/// Wetland and mangrove are neither land nor water.
pub fn is_land(tile: &Tile) -> bool {
    let terrain = tile.terrain - 3;
    (0..4).contains(&terrain)
}

fn improvement_kind(tile: &Tile) -> i32 {
    if tile.improvement != 0 {
        return tile.improvement;
    }
    if tile.is_city || tile.is_capital {
        return IMPROVEMENT_CITY;
    }
    if tile.has_ruin {
        return IMPROVEMENT_RUIN;
    }
    0
}

fn has_improvement_object(tile: &Tile) -> bool {
    improvement_kind(tile) != 0
}

/// True when an improvement object is present, its type is non-zero, and it matches `kind`.
pub fn has_improvement(tile: &Tile, kind: i32) -> bool {
    let current = improvement_kind(tile);
    current != 0 && current == kind
}

fn has_resource(tile: &Tile, kind: i32) -> bool {
    tile.resource != 0 && tile.resource == kind
}

fn index_at(map: &Map, x: i32, y: i32) -> Option<usize> {
    index_in(i32::from(map.width), i32::from(map.height), map.tiles.len(), x, y)
}

/// [`index_at`] for a map of `width` by `height` holding `len` tiles.
fn index_in(width: i32, height: i32, len: usize, x: i32, y: i32) -> Option<usize> {
    if x < 0 || y < 0 {
        return None;
    }
    if x >= width || y >= height {
        return None;
    }
    let index = y * width + x;
    let index = usize::try_from(index).ok()?;
    if index < len {
        Some(index)
    } else {
        None
    }
}

fn to_index(x: i32, y: i32, width: i32) -> i32 {
    y.wrapping_mul(width).wrapping_add(x)
}

fn from_index(index: i32, width: i32) -> (i32, i32) {
    let x = index % width;
    let y = index / width;
    (x, y)
}

/// Distance to the nearest vertical edge: `min(x, width - x - 1)`.
pub fn distance_to_edge_x(width: i32, x: i32) -> i32 {
    let clamped = width.min(x);
    let opposite = width - x - 1;
    clamped.min(opposite)
}

/// Distance to the nearest horizontal edge: `min(y, height - y - 1)`.
pub fn distance_to_edge_y(height: i32, y: i32) -> i32 {
    let clamped = height.min(y);
    let opposite = height - y - 1;
    clamped.min(opposite)
}

/// Distance to the nearest edge: the smaller of the two above.
pub fn distance_to_edge(map: &Map, coordinates: WorldCoordinates) -> i32 {
    let dx = distance_to_edge_x(i32::from(map.width), coordinates.x);
    let dy = distance_to_edge_y(i32::from(map.height), coordinates.y);
    dx.min(dy)
}

/// The tiles around a centre, with Chebyshev (diagonal) or Manhattan filtering.
/// Outer loop is `dx`, inner loop is `dy`, both from `-radius` through `radius`
/// (the game builds the coordinates as `(outer, inner)`; confirmed against real games).
fn area_indices(
    map: &Map,
    center: WorldCoordinates,
    radius: i32,
    allow_diagonal: bool,
    include_center: bool,
) -> Vec<usize> {
    area_indices_iter(map, center, radius, allow_diagonal, include_center).collect()
}

/// [`area_indices`] without the allocation: same indices, same order. It reads only the map's size, so the map
/// may change while it runs.
fn area_indices_iter(
    map: &Map,
    center: WorldCoordinates,
    radius: i32,
    allow_diagonal: bool,
    include_center: bool,
) -> impl Iterator<Item = usize> {
    let (width, height, len) = (i32::from(map.width), i32::from(map.height), map.tiles.len());
    crate::rules::path::square_offsets(radius).filter_map(move |(dx, dy)| {
        if dx == 0 && dy == 0 && !include_center {
            return None;
        }
        if !allow_diagonal && dx.abs() + dy.abs() > radius {
            return None;
        }
        index_in(width, height, len, center.x + dx, center.y + dy)
    })
}

/// True when a city improvement is inside Chebyshev `radius`, including the centre.
pub fn is_near_city(map: &Map, tile_index: usize, radius: i32) -> bool {
    let Some(tile) = map.tiles.get(tile_index) else {
        return false;
    };
    area_indices_iter(map, tile.coordinates, radius, true, true)
        .any(|index| has_improvement(&map.tiles[index], IMPROVEMENT_CITY))
}

/// True when a ruin improvement is inside Chebyshev `radius`, including the centre.
pub fn is_near_ruin(map: &Map, tile_index: usize, radius: i32) -> bool {
    let Some(tile) = map.tiles.get(tile_index) else {
        return false;
    };
    area_indices_iter(map, tile.coordinates, radius, true, true)
        .any(|index| has_improvement(&map.tiles[index], IMPROVEMENT_RUIN))
}

/// True when a capital is inside Chebyshev `radius`.
///
/// A capital is a city improvement whose tile owner is non-zero. Post-terrain cities
/// are placed with owner `0` and do not count.
pub fn is_near_capital(map: &Map, tile_index: usize, radius: i32, include_center: bool) -> bool {
    let Some(tile) = map.tiles.get(tile_index) else {
        return false;
    };
    area_indices_iter(map, tile.coordinates, radius, true, include_center).any(|index| {
        let other = &map.tiles[index];
        other.owner != 0 && has_improvement(other, IMPROVEMENT_CITY)
    })
}

/// `(map_width - 1) / ceil(sqrt(player_count))`, the divisor used by
/// [`is_valid_capital_location`] and [`is_capital_position_possible`].
///
/// A non-positive player count makes that integer division fault in the game.
fn capital_domain_size(map_width: i32, player_count: i32) -> Option<i32> {
    if player_count <= 0 {
        return None;
    }
    let divisor = (player_count as f64).sqrt().ceil() as i32;
    if divisor == 0 {
        return None;
    }
    Some((map_width - 1) / divisor)
}

fn capital_exclusion_radius(map_width: i32, player_count: i32) -> Option<i32> {
    let domain = capital_domain_size(map_width, player_count)?;
    Some(if domain >= 5 { 3 } else { 2 })
}

/// Whether a tile is a valid city location.
///
/// Not water, distance to the edge above 0, and every tile in Chebyshev radius 2
/// (including the center) has no improvement object.
pub fn is_valid_city_location(map: &Map, tile_index: usize) -> bool {
    let Some(tile) = map.tiles.get(tile_index) else {
        return false;
    };
    if is_water(tile) {
        return false;
    }
    if distance_to_edge(map, tile.coordinates) <= 0 {
        return false;
    }
    area_indices_iter(map, tile.coordinates, 2, true, true).all(|index| !has_improvement_object(&map.tiles[index]))
}

/// Whether a tile is a good city location.
///
/// [`is_valid_city_location`], then edge distance `>= 2`, and neither axis distance is exactly 3.
pub fn is_good_city_location(map: &Map, tile_index: usize) -> bool {
    let Some(tile) = map.tiles.get(tile_index) else {
        return false;
    };
    if !is_valid_city_location(map, tile_index) {
        return false;
    }
    if distance_to_edge(map, tile.coordinates) < 2 {
        return false;
    }
    if distance_to_edge_x(i32::from(map.width), tile.coordinates.x) == 3 {
        return false;
    }
    distance_to_edge_y(i32::from(map.height), tile.coordinates.y) != 3
}

/// Whether a tile is a valid capital location.
///
/// Same edge rules as [`is_good_city_location`], then not near a capital.
/// The exclusion radius is 3 when the domain size is at least `DESIRED_DOMAIN_SIZE` (5), otherwise 2.
pub fn is_valid_capital_location(map: &Map, tile_index: usize, player_count: i32) -> bool {
    let Some(tile) = map.tiles.get(tile_index) else {
        return false;
    };
    if !is_valid_city_location(map, tile_index) {
        return false;
    }
    if distance_to_edge(map, tile.coordinates) < 2 {
        return false;
    }
    if distance_to_edge_x(i32::from(map.width), tile.coordinates.x) == 3 {
        return false;
    }
    if distance_to_edge_y(i32::from(map.height), tile.coordinates.y) == 3 {
        return false;
    }
    let Some(radius) = capital_exclusion_radius(i32::from(map.width), player_count) else {
        return false;
    };
    !is_near_capital(map, tile_index, radius, true)
}

/// Whether a capital could be placed on a tile.
///
/// Edge distance `>= 2`, no improvement object in Chebyshev radius 2, and not near a capital.
/// This check does not reject water.
pub fn is_capital_position_possible(map: &Map, tile_index: usize, player_count: i32) -> bool {
    let Some(tile) = map.tiles.get(tile_index) else {
        return false;
    };
    if distance_to_edge(map, tile.coordinates) < 2 {
        return false;
    }
    let blocked =
        area_indices_iter(map, tile.coordinates, 2, true, true).any(|index| has_improvement_object(&map.tiles[index]));
    if blocked {
        return false;
    }
    let Some(radius) = capital_exclusion_radius(i32::from(map.width), player_count) else {
        return false;
    };
    !is_near_capital(map, tile_index, radius, true)
}

/// Every valid city spot, in tile-array order.
pub fn get_all_valid_city_spot_indices(map: &Map) -> Vec<i32> {
    let mut spots = Vec::new();
    for index in 0..map.tiles.len() {
        if is_valid_city_location(map, index) {
            spots.push(index as i32);
        }
    }
    spots
}

/// Whether a tile is a valid ruin location.
///
/// No city and no ruin in Chebyshev radius 2, including the center.
pub fn valid_ruin_location(map: &Map, tile_index: usize) -> bool {
    let Some(tile) = map.tiles.get(tile_index) else {
        return false;
    };
    area_indices_iter(map, tile.coordinates, 2, true, true).all(|index| {
        let other = &map.tiles[index];
        !has_improvement(other, IMPROVEMENT_CITY) && !has_improvement(other, IMPROVEMENT_RUIN)
    })
}

/// The lost city location test, required by water ruins.
fn valid_lost_city_location(map: &Map, tile_index: usize) -> bool {
    let Some(tile) = map.tiles.get(tile_index) else {
        return false;
    };
    if distance_to_edge(map, tile.coordinates) <= 0 || tile.owner != 0 || tile.terrain != TERRAIN_OCEAN {
        return false;
    }
    let coordinates = tile.coordinates;
    area_indices_iter(map, coordinates, 2, true, false).all(|index| {
        let other = &map.tiles[index];
        !has_improvement(other, IMPROVEMENT_CITY) && !has_improvement(other, IMPROVEMENT_RUIN)
    })
}

/// The four map corners, in the order the game visits them.
fn map_corners(map: &Map) -> [WorldCoordinates; 4] {
    let max_x = i32::from(map.width) - 1;
    let max_y = i32::from(map.height) - 1;
    [
        WorldCoordinates { x: max_x, y: max_y },
        WorldCoordinates { x: 0, y: 0 },
        WorldCoordinates { x: 0, y: max_y },
        WorldCoordinates { x: max_x, y: 0 },
    ]
}

/// Adds the lighthouses: each map corner gets a
/// lighthouse (improvement 47), and a corner that is not water becomes Field.
pub fn add_lighthouses(map: &mut Map) {
    for corner in map_corners(map) {
        if let Some(index) = index_at(map, corner.x, corner.y) {
            let tile = &mut map.tiles[index];
            tile.improvement = IMPROVEMENT_LIGHTHOUSE;
            if !is_water(tile) {
                tile.terrain = TERRAIN_FIELD;
            }
        }
    }
}

/// Removes indices near an index.
///
/// Drops every index in the inclusive square of Chebyshev `range`, using binary search
/// so the candidate list must stay sorted.
fn remove_indices_near(indices: &mut Vec<i32>, index: i32, width: i32, range: i32) {
    if width <= 0 {
        return;
    }
    let (x, y) = from_index(index, width);
    for yy in (y - range)..=(y + range) {
        for xx in (x - range)..=(x + range) {
            let candidate = to_index(xx, yy, width);
            if let Ok(position) = indices.binary_search(&candidate) {
                indices.remove(position);
            }
        }
    }
}

fn place_city(tile: &mut Tile, owner: u8, capital: bool) {
    tile.owner = owner;
    tile.terrain = TERRAIN_FIELD;
    tile.resource = 0;
    tile.improvement = IMPROVEMENT_CITY;
    tile.is_city = true;
    tile.is_capital = capital;
    tile.capital_of = if capital { owner } else { 0 };
    tile.has_ruin = false;
}

/// Chooses the pre-terrain city indices.
///
/// Builds every index with `x` and `y` in `1..size-1`, removes a radius-2 square around
/// each index already in `cities`, then appends `city_count` random picks. Each pick
/// also clears its radius-2 square. The candidate list stays sorted because removals
/// use binary search.
pub fn generate_pre_terrain_cities(map: &Map, cities: &mut Vec<i32>, city_count: i32, rng: &mut DotNetRandom) {
    let width = i32::from(map.width);
    let height = i32::from(map.height);
    let mut candidates = Vec::new();
    if height >= 3 && width >= 3 {
        for y in 1..height - 1 {
            for x in 1..width - 1 {
                candidates.push(to_index(x, y, width));
            }
        }
    }
    for &city in cities.iter() {
        remove_indices_near(&mut candidates, city, width, 2);
    }
    let mut remaining = city_count;
    while remaining > 0 && !candidates.is_empty() {
        let pick = rng::range_i32(rng, 0, candidates.len() as i32) as usize;
        let chosen = candidates[pick];
        cities.push(chosen);
        remove_indices_near(&mut candidates, chosen, width, 2);
        remaining -= 1;
    }
}

/// Adds the post-terrain cities.
///
/// Places up to `max_city_count` cities on random valid spots. Each city becomes field,
/// owner 0, city improvement, and clears resource. A radius-2 square is then removed
/// from the candidate index list.
pub fn add_post_terrain_cities(map: &mut Map, max_city_count: i32, rng: &mut DotNetRandom) {
    if max_city_count <= 0 {
        return;
    }
    let mut spots = get_all_valid_city_spot_indices(map);
    let mut placed = 0;
    while placed < max_city_count && !spots.is_empty() {
        let pick = rng::range_i32(rng, 0, spots.len() as i32) as usize;
        let index = spots[pick] as usize;
        if index >= map.tiles.len() {
            break;
        }
        let coordinates = map.tiles[index].coordinates;
        place_city(&mut map.tiles[index], 0, false);
        let width = i32::from(map.width);
        for y in (coordinates.y - 2)..=(coordinates.y + 2) {
            for x in (coordinates.x - 2)..=(coordinates.x + 2) {
                let candidate = to_index(x, y, width);
                if let Ok(position) = spots.binary_search(&candidate) {
                    spots.remove(position);
                }
            }
        }
        placed += 1;
    }
}

/// Index-order scan that places each player's capital (the tight variant).
///
/// Nature (`id == 255`) is skipped. Each other player takes the next
/// [`is_valid_capital_location`] tile, scanning forward and never restarting.
/// Returns false when a player cannot be placed.
///
/// Not applied, both owned outside this module: the west-player capital reassignment
/// and the full capital placement ([`set_tile_as_capital`]). This function records the capital on the
/// tile (`is_capital`, owner, field, city improvement) and copies the coordinates to
/// `PlayerState.start_tile`.
pub fn try_add_capitals_to_contents_tight(map: &mut Map, players: &mut [PlayerState]) -> bool {
    let player_count = player_count(players);
    let mut cursor = 0usize;
    for player in players.iter_mut() {
        if player.id == NATURE_PLAYER_ID {
            continue;
        }
        let mut placed = false;
        while cursor < map.tiles.len() {
            if is_valid_capital_location(map, cursor, player_count) {
                let coordinates = map.tiles[cursor].coordinates;
                place_city(&mut map.tiles[cursor], player.id, true);
                player.start_tile = coordinates;
                placed = true;
                break;
            }
            cursor += 1;
        }
        if !placed {
            return false;
        }
    }
    true
}

/// Starting score when choosing the best city coordinates.
const CAPITAL_CITY_SCORE: f32 = 20.0;
/// Added when a candidate has 2 to 6 water neighbors.
const CAPITAL_WATER_BONUS: f32 = 1.0;
/// Cancels that bonus when 7 or 8 neighbors are water.
const CAPITAL_SURROUNDED_PENALTY: f32 = -1.0;

/// Picks the best unowned city on a continent.
///
/// Candidates are unowned city improvements on `continent_tiles`, in that order.
/// Score starts at 20 and falls to the minimum Chebyshev distance to a capital already
/// in `capitals`. A tile closer than 3, or 4 when the capital domain is at least 5, is
/// rejected. Two to six water neighbors in the radius-1 ring add 1. Seven or eight do not.
/// Edge distance must be at least 2. The highest score wins; ties keep the earlier tile.
/// `(0, 0)` is the game's "not found" coordinate and is not returned.
fn best_unowned_city(map: &Map, continent_tiles: &[i32], capitals: &[usize], player_count: i32) -> Option<usize> {
    let spacing = capital_spacing(i32::from(map.width), player_count)?;
    let mut best_score = 0.0f32;
    let mut best: Option<usize> = None;
    for &raw in continent_tiles {
        let Ok(index) = usize::try_from(raw) else {
            continue;
        };
        let Some(tile) = map.tiles.get(index) else {
            continue;
        };
        if tile.owner != 0 || !has_improvement(tile, IMPROVEMENT_CITY) {
            continue;
        }
        let mut score = CAPITAL_CITY_SCORE;
        for &capital in capitals {
            let Some(other) = map.tiles.get(capital) else {
                continue;
            };
            let dist = chebyshev(tile.coordinates, other.coordinates) as f32;
            if score > dist {
                score = dist;
            }
        }
        if (spacing as f32) > score {
            continue;
        }
        let water = area_indices_iter(map, tile.coordinates, 1, true, false)
            .filter(|&neighbor| is_water(&map.tiles[neighbor]))
            .count() as i32;
        if water >= 2 {
            score += CAPITAL_WATER_BONUS;
            if water >= 7 {
                score += CAPITAL_SURROUNDED_PENALTY;
            }
        }
        if distance_to_edge(map, tile.coordinates) < 2 {
            continue;
        }
        if tile.coordinates.x == 0 && tile.coordinates.y == 0 {
            continue;
        }
        if score > best_score {
            best_score = score;
            best = Some(index);
        }
    }
    best
}

fn capital_spacing(map_width: i32, player_count: i32) -> Option<i32> {
    let domain = capital_domain_size(map_width, player_count)?;
    Some(if domain >= 5 { 4 } else { 3 })
}

fn chebyshev(a: WorldCoordinates, b: WorldCoordinates) -> i32 {
    (a.x - b.x).abs().max((a.y - b.y).abs())
}

fn valid_capital_spots(map: &Map, player_count: i32) -> Vec<i32> {
    let mut spots = Vec::new();
    for index in 0..map.tiles.len() {
        if is_valid_capital_location(map, index, player_count) {
            spots.push(index as i32);
        }
    }
    spots
}

/// Emergency capital position.
///
/// Picks a random index from the valid capital spots. If that list
/// is empty, returns the first tile where [`is_capital_position_possible`] is true.
fn emergency_capital(map: &Map, player_count: i32, rng: &mut DotNetRandom) -> Option<usize> {
    let spots = valid_capital_spots(map, player_count);
    if !spots.is_empty() {
        let pick = rng::range_i32_max(rng, spots.len() as i32);
        let Ok(pick) = usize::try_from(pick) else {
            return None;
        };
        return usize::try_from(spots.get(pick).copied()?).ok();
    }
    (0..map.tiles.len()).find(|&index| is_capital_position_possible(map, index, player_count))
}

fn continent_of(continents: &[Vec<i32>], tile_index: usize) -> Option<usize> {
    let raw = i32::try_from(tile_index).ok()?;
    continents.iter().position(|tiles| tiles.contains(&raw))
}

fn place_capital_tile(tile: &mut Tile, owner: u8) {
    tile.owner = owner;
    tile.terrain = TERRAIN_FIELD;
    tile.resource = 0;
    tile.improvement = IMPROVEMENT_CITY;
    tile.is_city = true;
    tile.is_capital = false;
    tile.capital_of = 0;
    tile.has_ruin = false;
}

/// Places a capital on the fields [`Tile`] actually has.
///
/// Sets `is_capital`, `owner`, city `improvement`, field terrain, cleared resource, and
/// `player.start_tile`. `climate` is written because `Tile.climate` exists; the game
/// reads it from the player's climate, which this `PlayerState` does not store.
///
/// Skipped, because `Tile` has no field for them: city name, the player's skin, the
/// discovered-by flag, city level (the city-level-3 bonus rewrites the city's
/// level), the city's border area, and exploration from the tile. The spores bonus (`2`) does map onto
/// `improvement`: every field tile in the radius-1 ring becomes fungi (`37`).
pub fn set_tile_as_capital(map: &mut Map, tile_index: usize, player: &mut PlayerState, climate: i32, bonus: i32) {
    let Some(tile) = map.tiles.get_mut(tile_index) else {
        return;
    };
    let coordinates = tile.coordinates;
    player.start_tile = coordinates;
    tile.terrain = TERRAIN_FIELD;
    tile.altitude = 1;
    tile.climate = climate;
    tile.resource = 0;
    tile.improvement = IMPROVEMENT_CITY;
    tile.owner = player.id;
    tile.is_city = true;
    tile.is_capital = true;
    tile.capital_of = player.id;
    tile.has_ruin = false;
    // The city's starting border (radius 1) claims the surrounding ring.
    for neighbor in area_indices_iter(map, coordinates, 1, true, false) {
        if map.tiles[neighbor].owner == 0 {
            map.tiles[neighbor].owner = player.id;
        }
    }
    if bonus != BONUS_SPORES {
        return;
    }
    for neighbor in area_indices_iter(map, coordinates, 1, true, false) {
        if map.tiles[neighbor].terrain == TERRAIN_FIELD {
            map.tiles[neighbor].improvement = IMPROVEMENT_FUNGI;
        }
    }
}

/// Reassigns capitals for the west map placement setting.
///
/// Runs only when a west-placement player id is supplied (the west map placement user id
/// has a value; that guid is not on [`PlayerState`]). The capital with the largest `y - x`
/// swaps owners with the capital owned by `west_player`. The `width` argument is unused.
/// Returns false when that player owns none of the capitals (the game throws).
fn reassign_capitals_for_west_player(map: &mut Map, capitals: &mut [(usize, u8)], west_player: u8) -> bool {
    if capitals.is_empty() {
        return false;
    }
    let mut best_score = i32::MIN;
    let mut best_index: Option<usize> = None;
    for (index, &(tile_index, _)) in capitals.iter().enumerate() {
        let Some(tile) = map.tiles.get(tile_index) else {
            return false;
        };
        let score = tile.coordinates.y - tile.coordinates.x;
        if score > best_score {
            best_score = score;
            best_index = Some(index);
        }
    }
    let Some(best_index) = best_index else {
        return false;
    };
    let Some(west_index) = capitals.iter().position(|(_, owner)| *owner == west_player) else {
        return false;
    };
    let best_owner = capitals[best_index].1;
    let west_tile = capitals[west_index].0;
    let best_tile = capitals[best_index].0;
    if let Some(tile) = map.tiles.get_mut(west_tile) {
        tile.owner = best_owner;
    }
    if let Some(tile) = map.tiles.get_mut(best_tile) {
        tile.owner = west_player;
    }
    capitals[west_index].1 = best_owner;
    capitals[best_index].1 = west_player;
    true
}

fn try_place_on_continent(
    map: &mut Map,
    continents: &[Vec<i32>],
    continent: usize,
    owner: u8,
    player_count: i32,
    capitals: &mut Vec<(usize, u8)>,
    rng: &mut DotNetRandom,
) -> bool {
    let Some(tiles) = continents.get(continent) else {
        return false;
    };
    let existing: Vec<usize> = capitals.iter().map(|(index, _)| *index).collect();
    let spot =
        best_unowned_city(map, tiles, &existing, player_count).or_else(|| emergency_capital(map, player_count, rng));
    let Some(spot) = spot else {
        return false;
    };
    if spot >= map.tiles.len() {
        return false;
    }
    place_capital_tile(&mut map.tiles[spot], owner);
    capitals.push((spot, owner));
    true
}

/// Places capitals on continents.
///
/// Nature (`id == 255`) is skipped and does not consume a continent. Each other player is
/// placed on continent `cursor`, then `cursor` increments and wraps to 0 once it reaches
/// `populated_continents`. Placement takes the best
/// unowned city on that continent, or the emergency capital position when that search returns
/// `(0, 0)`.
///
/// A second pass repeats that placement for every capital whose tile belongs to a continent
/// (the game follows each tile's continent). The previous owner byte is cleared first, so
/// the old city can be chosen again. `Tile` has no continent field; membership is the first
/// `continents` list that contains the index. Lists should be disjoint.
///
/// `west_player` is the west map placement player id. `None` skips reassignment.
/// `tribe_bonus` and `tribe_climate` are parallel to `players` because those values are not
/// on [`PlayerState`]. Missing entries are 0.
///
/// Returns false when a player cannot be placed or the west player is not among the capitals.
pub fn try_add_capitals_to_continents(
    map: &mut Map,
    players: &mut [PlayerState],
    continents: &[Vec<i32>],
    populated_continents: i32,
    rng: &mut DotNetRandom,
    west_player: Option<u8>,
    tribe_bonus: &[i32],
    tribe_climate: &[i32],
) -> bool {
    let player_count = player_count(players);
    if player_count <= 0 {
        return false;
    }
    let mut capitals: Vec<(usize, u8)> = Vec::new();
    let mut cursor = 0i32;
    for player in players.iter() {
        if player.id == NATURE_PLAYER_ID {
            continue;
        }
        if continents.is_empty() {
            return false;
        }
        let Some(continent) = usize::try_from(cursor).ok().filter(|index| *index < continents.len()) else {
            return false;
        };
        if !try_place_on_continent(map, continents, continent, player.id, player_count, &mut capitals, rng) {
            return false;
        }
        cursor += 1;
        if cursor >= populated_continents {
            cursor = 0;
        }
    }

    let snapshot = capitals.clone();
    for (tile_index, owner) in snapshot {
        let Some(continent) = continent_of(continents, tile_index) else {
            continue;
        };
        if let Some(tile) = map.tiles.get_mut(tile_index) {
            tile.owner = 0;
        }
        capitals.retain(|(index, _)| *index != tile_index);
        if !try_place_on_continent(map, continents, continent, owner, player_count, &mut capitals, rng) {
            return false;
        }
    }

    if let Some(west_player) = west_player {
        if !reassign_capitals_for_west_player(map, &mut capitals, west_player) {
            return false;
        }
    }

    for (tile_index, owner) in capitals {
        let Some(player_index) = players.iter().position(|player| player.id == owner) else {
            return false;
        };
        let bonus = tribe_bonus.get(player_index).copied().unwrap_or(0);
        let climate = tribe_climate.get(player_index).copied().unwrap_or(0);
        let player = &mut players[player_index];
        set_tile_as_capital(map, tile_index, player, climate, bonus);
    }
    true
}

fn resource_idx_for_tech(
    data: &GameLogicData,
    overrides: &std::collections::BTreeMap<String, String>,
    tech_id: &str,
) -> Option<i32> {
    let tech = data.tech(tech_id)?;
    for improvement_id in &tech.improvement_unlocks {
        let resolved = overrides.get(improvement_id).map(String::as_str).unwrap_or(improvement_id);
        let Some(improvement) = data.improvement(resolved) else {
            continue;
        };
        for requirement in &improvement.terrain_requirements {
            let Some(resource_id) = requirement.resource.as_deref() else {
                continue;
            };
            let Some(resource) = data.resource(resource_id) else {
                continue;
            };
            if resource.idx != 0 {
                return Some(resource.idx);
            }
        }
    }
    None
}

fn resolve_resource_ids(data: &GameLogicData, ids: &[String]) -> Vec<i32> {
    ids.iter().filter_map(|id| data.resource(id).map(|resource| resource.idx)).collect()
}

/// The starting resources for a player.
///
/// The caller passes [`GameLogicData`] for the game version; choosing it is outside this crate.
///
/// A non-empty `starting_resource` list is returned as resource `idx` values, in order.
/// Otherwise only the last `starting_tech` is asked for a resource: the first non-zero resource on an unlocked improvement's terrain
/// requirements, after `improvement_overrides`. If that is missing, the basic tech
/// (`idx == 0`) contributes one resource chosen with `Range(0, count)` from the resources
/// of its `tech_unlocks`, in list order.
pub fn get_starting_resources_for_player(data: &GameLogicData, tribe_idx: i32, rng: &mut DotNetRandom) -> Vec<i32> {
    let Some(tribe) = data.tribe_by_idx(tribe_idx) else {
        return Vec::new();
    };
    let starting_resource = tribe.starting_resource.clone();
    let starting_tech = tribe.starting_tech.clone();
    let overrides = tribe.improvement_overrides.clone();
    if !starting_resource.is_empty() {
        return resolve_resource_ids(data, &starting_resource);
    }
    if let Some(last) = starting_tech.last() {
        if let Some(kind) = resource_idx_for_tech(data, &overrides, last) {
            return vec![kind];
        }
    }
    let Some(basic) = data.tech.values().find(|tech| tech.idx == 0) else {
        return Vec::new();
    };
    let mut found = Vec::new();
    for tech_id in &basic.tech_unlocks {
        if let Some(kind) = resource_idx_for_tech(data, &overrides, tech_id) {
            found.push(kind);
        }
    }
    if found.is_empty() {
        return found;
    }
    let pick = rng::range_i32_max(rng, found.len() as i32);
    let Some(kind) = usize::try_from(pick).ok().and_then(|index| found.get(index).copied()) else {
        return Vec::new();
    };
    vec![kind]
}

struct StartingSpawn {
    kind: i32,
    terrains: Vec<i32>,
}

fn starting_spawns(data: &GameLogicData, resource_ids: &[String]) -> Vec<StartingSpawn> {
    let mut spawns = Vec::new();
    for id in resource_ids {
        let Some(resource) = data.resource(id) else {
            continue;
        };
        let terrains = resource
            .resource_terrain_requirements
            .iter()
            .filter_map(|terrain_id| data.terrain(terrain_id).map(|terrain| terrain.idx))
            .collect();
        spawns.push(StartingSpawn { kind: resource.idx, terrains });
    }
    spawns
}

fn drop_neighbors_that_already_have_spawn(map: &Map, spawns: &mut Vec<StartingSpawn>, neighbors: &mut Vec<usize>) {
    let mut index = 0;
    while index < neighbors.len() {
        let kind = map.tiles[neighbors[index]].resource;
        if kind == 0 {
            index += 1;
            continue;
        }
        if let Some(spawn_index) = spawns.iter().position(|spawn| spawn.kind == kind) {
            spawns.remove(spawn_index);
            neighbors.remove(index);
        } else {
            index += 1;
        }
    }
}

enum StartingPass {
    MatchTerrain,
    AnyLand,
    FirstNeighbor,
}

fn place_starting_pass(
    map: &mut Map,
    spawns: &mut Vec<StartingSpawn>,
    neighbors: &mut Vec<usize>,
    rng: &mut DotNetRandom,
    pass: StartingPass,
) {
    let mut index = 0;
    while index < spawns.len() {
        if spawns[index].terrains.is_empty() {
            index += 1;
            continue;
        }
        let pick = rng::range_i32_max(rng, spawns[index].terrains.len() as i32);
        let Some(terrain) = usize::try_from(pick).ok().and_then(|picked| spawns[index].terrains.get(picked).copied())
        else {
            index += 1;
            continue;
        };
        if neighbors.is_empty() {
            index += 1;
            continue;
        }
        let found = match pass {
            StartingPass::MatchTerrain => neighbors.iter().position(|&neighbor| map.tiles[neighbor].terrain == terrain),
            StartingPass::AnyLand => neighbors.iter().position(|&neighbor| !is_water(&map.tiles[neighbor])),
            StartingPass::FirstNeighbor => Some(0),
        };
        let Some(neighbor_index) = found else {
            index += 1;
            continue;
        };
        let tile_index = neighbors[neighbor_index];
        if !matches!(pass, StartingPass::MatchTerrain) {
            map.tiles[tile_index].terrain = terrain;
        }
        map.tiles[tile_index].resource = spawns[index].kind;
        spawns.remove(index);
        neighbors.remove(neighbor_index);
    }
}

/// Adds the starting resources around a capital.
///
/// Does not call the singular starting-resource step. The minimum resource count setting is unused.
/// An empty id list places nothing.
///
/// Neighbors are the radius-1 ring around `start`, excluding the center, shuffled with
/// [`DotNetRandom`]. A neighbor that already has one of the requested resource idxs drops
/// that resource and itself. Three passes then place what remains: matching terrain,
/// then the first land neighbor (its terrain is changed to a random requirement), then
/// neighbor 0 (terrain changed the same way). Resource idxs and terrain idxs come from
/// [`GameLogicData`]. Unknown ids are omitted.
pub fn add_starting_resources_to_capital(
    map: &mut Map,
    start: WorldCoordinates,
    resource_ids: &[String],
    data: &GameLogicData,
    rng: &mut DotNetRandom,
) {
    if resource_ids.is_empty() {
        return;
    }
    let mut spawns = starting_spawns(data, resource_ids);
    if spawns.is_empty() {
        return;
    }
    let mut neighbors = area_indices(map, start, 1, true, false);
    shuffle_indices(&mut neighbors, rng);
    drop_neighbors_that_already_have_spawn(map, &mut spawns, &mut neighbors);
    if spawns.is_empty() {
        return;
    }
    place_starting_pass(map, &mut spawns, &mut neighbors, rng, StartingPass::MatchTerrain);
    if spawns.is_empty() {
        return;
    }
    place_starting_pass(map, &mut spawns, &mut neighbors, rng, StartingPass::AnyLand);
    if spawns.is_empty() {
        return;
    }
    place_starting_pass(map, &mut spawns, &mut neighbors, rng, StartingPass::FirstNeighbor);
}

/// Fisher-Yates matching the game's tile shuffle:
/// for `i` in `0..len-1`, swap `i` with `Range(rng, i, len)`.
fn shuffle_indices(indices: &mut [usize], rng: &mut DotNetRandom) {
    if indices.len() < 2 {
        return;
    }
    let last = indices.len() - 1;
    for i in 0..last {
        let j = rng::range_i32(rng, i as i32, indices.len() as i32) as usize;
        indices.swap(i, j);
    }
}

/// Adds the ruins.
///
/// Shuffles every tile, skips lighthouse corners, and places a ruin when the tile is
/// not within radius 1 of a city, radius 2 of a ruin, or radius 2 of a capital.
/// Water also has to pass `valid_lost_city_location`. Placement stops once
/// `placed > amount`, so a non-negative `amount` produces `amount + 1` ruins.
pub fn add_ruins(map: &mut Map, amount: i32, rng: &mut DotNetRandom) {
    let corners = map_corners(map);
    let mut order: Vec<usize> = (0..map.tiles.len()).collect();
    shuffle_indices(&mut order, rng);
    let mut placed = 0;
    let mut water_ruins = 0;
    for index in order {
        let coordinates = map.tiles[index].coordinates;
        if corners.contains(&coordinates) {
            continue;
        }
        let water = is_water(&map.tiles[index]);
        if water_ruins > amount && water {
            continue;
        }
        if is_near_city(map, index, 1) || is_near_ruin(map, index, 2) || is_near_capital(map, index, 2, true) {
            continue;
        }
        if water && !valid_lost_city_location(map, index) {
            continue;
        }
        let tile = &mut map.tiles[index];
        tile.improvement = IMPROVEMENT_RUIN;
        tile.has_ruin = true;
        placed += 1;
        if water {
            water_ruins += 1;
        }
        if placed > amount {
            break;
        }
    }
}

/// Adds the starfish.
///
/// While `amount > 0` and fewer than `tile_count` samples have been drawn, pick a
/// random tile. Water tiles with no improvement are kept when their radius-1 neighborhood
/// (center excluded) has neither a city nor an existing starfish. Each success sets
/// `resource` to starfish. Sampling is with replacement and stops at `tile_count` draws
/// or when the placed count is no longer strictly below `amount`.
pub fn add_starfish(map: &mut Map, amount: f32, rng: &mut DotNetRandom) {
    if amount <= 0.0 || map.tiles.is_empty() {
        return;
    }
    let mut placed = 0i32;
    let mut attempts = 0i32;
    let tile_count = map.tiles.len() as i32;
    while attempts < tile_count && (placed as f32) < amount {
        let index = rng::range_i32_max(rng, tile_count) as usize;
        attempts += 1;
        if index >= map.tiles.len() {
            break;
        }
        if !is_water(&map.tiles[index]) || has_improvement_object(&map.tiles[index]) {
            continue;
        }
        let coordinates = map.tiles[index].coordinates;
        let blocked = area_indices_iter(map, coordinates, 1, true, false).any(|neighbor| {
            let tile = &map.tiles[neighbor];
            has_improvement(tile, IMPROVEMENT_CITY) || has_resource(tile, RESOURCE_STARFISH)
        });
        if blocked {
            continue;
        }
        map.tiles[index].resource = RESOURCE_STARFISH;
        placed += 1;
    }
}

fn hash_value(hash: &XxHash, x: i32, y: i32, z: i32) -> f32 {
    (hash.hash_i32s(&[x, y, z]) as f32) * HASH_TO_UNIT
}

fn hash_range(hash: &XxHash, min: i32, max: i32, x: i32, y: i32, z: i32) -> i32 {
    let span = max.wrapping_sub(min);
    if span <= 0 {
        return min;
    }
    let mixed = hash.hash_i32s(&[x, y, z]);
    min.wrapping_add((mixed % span as u32) as i32)
}

/// Adds the resource to one tile.
///
/// A tile that already has an improvement is left unchanged and yields `None`.
/// A missing continent sets terrain to ice and yields `None` (the game logs a warning).
/// Otherwise each resource is kept when its continent modifier is positive and
/// the unit hash of `(x, y, list_index)` is at most the modifier. Whales are dropped when a
/// radius-1 neighbor (center excluded) already has a whale or a city. Terrain
/// requirements then filter the list; an empty requirement list allows every terrain.
/// The chosen row is a hash range draw `0..eligibleCount` over `(x, y, resource_count)`.
///
/// `fixed` is the hash seed. When it is `None`, the game builds one from
/// `trunc(value * 1000)` toward zero, for a random unit float `value`.
pub fn add_resource(
    map: &mut Map,
    tile_index: usize,
    resources: &[ResourceSpawn],
    modifiers: Option<&ContinentModifiers>,
    rng: &mut DotNetRandom,
    fixed: Option<&XxHash>,
) -> Option<i32> {
    if tile_index >= map.tiles.len() {
        return None;
    }
    if has_improvement_object(&map.tiles[tile_index]) {
        return None;
    }
    let Some(modifiers) = modifiers else {
        map.tiles[tile_index].terrain = TERRAIN_ICE;
        return None;
    };
    let hash = match fixed {
        Some(hash) => *hash,
        None => {
            let seed = (rng::value(rng) * 1000.0) as i32;
            XxHash::new(seed)
        }
    };
    let x = map.tiles[tile_index].coordinates.x;
    let y = map.tiles[tile_index].coordinates.y;
    let terrain = map.tiles[tile_index].terrain;
    let coordinates = map.tiles[tile_index].coordinates;
    let mut eligible = Vec::new();
    for (index, resource) in resources.iter().enumerate() {
        let modifier = modifiers.get(resource.kind);
        let sample = hash_value(&hash, x, y, index as i32);
        if modifier == 0.0 || sample > modifier {
            continue;
        }
        if resource.kind == RESOURCE_WHALE {
            let blocked = area_indices_iter(map, coordinates, 1, true, false).any(|neighbor| {
                let tile = &map.tiles[neighbor];
                has_resource(tile, RESOURCE_WHALE) || has_improvement(tile, IMPROVEMENT_CITY)
            });
            if blocked {
                continue;
            }
        }
        if !resource.terrains.is_empty() && !resource.terrains.contains(&terrain) {
            continue;
        }
        eligible.push(resource.kind);
    }
    if eligible.is_empty() {
        return None;
    }
    let pick = hash_range(&hash, 0, eligible.len() as i32, x, y, resources.len() as i32) as usize;
    let kind = eligible[pick];
    map.tiles[tile_index].resource = kind;
    Some(kind)
}

/// Adds the resources for default map generation.
///
/// `continents` is indexed by tile. A missing entry is a null continent.
/// Phase 1 walks each city's radius-1 neighborhood (center excluded) at `richness`.
/// Phase 2 walks each city's radius-2 neighborhood at `richness / 3`.
/// Every considered tile is marked visited. The richness roll happens before the
/// visited check, so a tile next to two cities consumes a roll for each city.
/// The tutorial resource placement is not called.
pub fn add_resources(
    map: &mut Map,
    resources: &[ResourceSpawn],
    continents: &[Option<ContinentModifiers>],
    richness: f32,
    rng: &mut DotNetRandom,
) {
    let cities: Vec<usize> = map
        .tiles
        .iter()
        .enumerate()
        .filter(|(_, tile)| has_improvement(tile, IMPROVEMENT_CITY))
        .map(|(index, _)| index)
        .collect();
    let mut visited = crate::map::IndexSet::with_len(map.tiles.len());
    for &city in &cities {
        let coordinates = map.tiles[city].coordinates;
        for neighbor in area_indices_iter(map, coordinates, 1, true, false) {
            let roll = rng::value(rng);
            if richness <= roll
                || visited.contains(neighbor as i64)
                || map.tiles[neighbor].resource != 0
                || has_improvement_object(&map.tiles[neighbor])
            {
                visited.insert(neighbor as i64);
                continue;
            }
            let modifiers = continents.get(neighbor).copied().flatten();
            add_resource(map, neighbor, resources, modifiers.as_ref(), rng, None);
            visited.insert(neighbor as i64);
        }
    }
    let richness = richness / 3.0;
    for &city in &cities {
        let coordinates = map.tiles[city].coordinates;
        for neighbor in area_indices_iter(map, coordinates, 2, true, false) {
            if has_improvement_object(&map.tiles[neighbor]) {
                visited.insert(neighbor as i64);
                continue;
            }
            let roll = rng::value(rng);
            if richness <= roll || visited.contains(neighbor as i64) || map.tiles[neighbor].resource != 0 {
                visited.insert(neighbor as i64);
                continue;
            }
            let modifiers = continents.get(neighbor).copied().flatten();
            add_resource(map, neighbor, resources, modifiers.as_ref(), rng, None);
            visited.insert(neighbor as i64);
        }
    }
}

fn turf_distance_factor(map_width: i32, cost: i32) -> f32 {
    let width = map_width as f32 * TURF_WIDTH_SCALE;
    let numerator = cost as f32 - RUIN_DISTANCE_PENALTY;
    let denominator = width - RUIN_DISTANCE_PENALTY;
    let ratio = numerator / denominator;
    ratio.clamp(0.0, 1.0)
}

/// The score of one turf tile.
///
/// Distance factor is `clamp((cost - 3) / (width * 0.8 - 3), 0, 1)`.
/// A city scores `-90 * factor + 100 + 0.1`, plus 40 when `cost == 3` and 20 when `cost == 4`.
/// A ruin scores `-3 * factor + 5 + 0.1`, plus another 5 when `cost < 5`.
/// Any other tile starts at `0.1`. Land (field, mountain, forest, ice) then adds `1.5 - factor`.
pub fn count_score_for_turf_tile(map: &Map, turf: &TurfTile) -> f32 {
    let Some(index) = index_at(map, turf.coordinates.x, turf.coordinates.y) else {
        return 0.0;
    };
    let tile = &map.tiles[index];
    let factor = turf_distance_factor(i32::from(map.width), turf.cost);
    let mut score = if has_improvement(tile, IMPROVEMENT_CITY) {
        let mut score = -CITY_DISTANCE_PENALTY * factor + SCORE_PER_CITY;
        score += SCORE_PER_TILE;
        if turf.cost == 4 {
            score += CLOSE_CITY_BONUS;
        } else if turf.cost == 3 {
            score += VERY_CLOSE_CITY_BONUS;
        }
        score
    } else if has_improvement(tile, IMPROVEMENT_RUIN) {
        let mut score = -RUIN_DISTANCE_PENALTY * factor + SCORE_PER_RUIN;
        score += SCORE_PER_TILE;
        if turf.cost < 5 {
            score += CLOSE_RUIN_BONUS;
        }
        score
    } else {
        SCORE_PER_TILE
    };
    if is_land(tile) {
        score += SCORE_PER_LAND_TILE - factor;
    }
    score
}

/// Sum of per-tile scores.
pub fn count_turf_score(map: &Map, turfs: &[TurfTile]) -> f32 {
    let mut total = 0.0;
    for turf in turfs {
        total += count_score_for_turf_tile(map, turf);
    }
    total
}

/// Ratio half of [`get_inequality`].
///
/// `max(scores) / min(scores)`, with the denominator replaced by [`INEQUALITY_MIN_FLOOR`]
/// when the minimum is not strictly positive. An empty slice leaves the sentinels
/// `-f32::MAX` and `f32::MAX`, so the ratio is `-1`.
pub fn inequality_from_scores(scores: &[f32]) -> f32 {
    let mut max_score = -f32::MAX;
    let mut min_score = f32::MAX;
    for &score in scores {
        if score < min_score {
            min_score = score;
        }
        if score > max_score {
            max_score = score;
        }
    }
    let denominator = if min_score > 0.0 { min_score } else { INEQUALITY_MIN_FLOOR };
    max_score / denominator
}

fn player_index(players: &[PlayerState], player_id: i32) -> i32 {
    players.iter().position(|player| i32::from(player.id) == player_id).map(|index| index as i32).unwrap_or(-1)
}

fn step_cost(map: &Map, source: WorldCoordinates, neighbor: usize, players: &[PlayerState]) -> i32 {
    let tile = &map.tiles[neighbor];
    if tile.terrain == TERRAIN_MOUNTAIN {
        return 4;
    }
    if !is_water(tile) {
        return 1;
    }
    let Some(source_index) = index_at(map, source.x, source.y) else {
        return 4;
    };
    let source_tile = &map.tiles[source_index];
    if is_water(source_tile) {
        return 4;
    }
    // Land that is not a city to water: 1000, a wall in practice.
    if !has_improvement(source_tile, IMPROVEMENT_CITY) {
        return 1000;
    }
    if players.iter().any(|player| player.start_tile == source) {
        8
    } else {
        16
    }
}

/// Bucket count of [`TurfQueue`]: more than the largest step cost of [`step_cost`], so every queued cost lies
/// within one turn of the ring above the lowest one.
const TURF_BUCKETS: usize = 1024;

/// The open set of [`create_turfs`]: per tile index its cost, player and insertion number while queued, and
/// (insertion number, index) entries in buckets by cost, where entries whose tile has since changed cost are
/// skipped. Popping the lowest (cost, insertion number) is the same as scanning a list in insertion order for the
/// first lowest cost. Costs only grow from the first pop on (every step costs at least 1), so the buckets form a
/// ring walked from the lowest cost upwards.
struct TurfQueue {
    queued: Vec<bool>,
    cost: Vec<i32>,
    player: Vec<i32>,
    seq: Vec<u32>,
    next_seq: u32,
    /// Per cost modulo [`TURF_BUCKETS`], the entries in descending insertion number, so the next one is last.
    buckets: Vec<Vec<(u32, usize)>>,
    /// The lowest cost that may still have entries.
    current: i32,
    /// Entries in the buckets, stale ones included.
    live: usize,
}

impl TurfQueue {
    fn new(len: usize) -> Self {
        Self {
            queued: vec![false; len],
            cost: vec![0; len],
            player: vec![0; len],
            seq: vec![0; len],
            next_seq: 0,
            buckets: vec![Vec::new(); TURF_BUCKETS],
            current: 0,
            live: 0,
        }
    }

    fn push(&mut self, cost: i32, seq: u32, i: usize) {
        debug_assert!(
            cost >= self.current && cost - self.current < TURF_BUCKETS as i32,
            "turf cost {cost} outside the ring"
        );
        let bucket = &mut self.buckets[cost as usize % TURF_BUCKETS];
        let at = bucket.partition_point(|&(s, _)| s > seq);
        bucket.insert(at, (seq, i));
        self.live += 1;
    }

    /// Empty again, as [`TurfQueue::new`] for a map of `len` tiles, keeping the storage.
    fn reset(&mut self, len: usize) {
        for v in [&mut self.cost, &mut self.player] {
            v.clear();
            v.resize(len, 0);
        }
        self.queued.clear();
        self.queued.resize(len, false);
        self.seq.clear();
        self.seq.resize(len, 0);
        if self.live > 0 {
            for bucket in &mut self.buckets {
                bucket.clear();
            }
        }
        self.next_seq = 0;
        self.current = 0;
        self.live = 0;
    }

    fn pop(&mut self, map: &Map) -> Option<TurfTile> {
        while self.live > 0 {
            let cost = self.current;
            let Some((seq, i)) = self.buckets[cost as usize % TURF_BUCKETS].pop() else {
                self.current += 1;
                continue;
            };
            self.live -= 1;
            if self.queued[i] && self.cost[i] == cost && self.seq[i] == seq {
                self.queued[i] = false;
                return Some(TurfTile { coordinates: map.tiles[i].coordinates, cost, player: self.player[i] });
            }
        }
        None
    }
}

fn enqueue_neighbor(
    queue: &mut TurfQueue,
    map: &Map,
    visited: &crate::map::IndexSet,
    current: &TurfTile,
    neighbor: usize,
    players: &[PlayerState],
) {
    let coordinates = map.tiles[neighbor].coordinates;
    let width = i32::from(map.width);
    let index = to_index(coordinates.x, coordinates.y, width);
    if visited.contains(i64::from(index)) {
        return;
    }
    let cost = current.cost + step_cost(map, current.coordinates, neighbor, players);
    let q = queue;
    if !q.queued[neighbor] {
        q.queued[neighbor] = true;
        q.cost[neighbor] = cost;
        q.player[neighbor] = current.player;
        q.seq[neighbor] = q.next_seq;
        q.push(cost, q.next_seq, neighbor);
        q.next_seq += 1;
        return;
    }
    if q.cost[neighbor] > cost {
        q.cost[neighbor] = cost;
        q.player[neighbor] = current.player;
        q.push(cost, q.seq[neighbor], neighbor);
    } else if q.cost[neighbor] == cost
        && player_index(players, q.player[neighbor]) > player_index(players, current.player)
    {
        q.player[neighbor] = current.player;
    }
}

fn expand_turf(
    queue: &mut TurfQueue,
    map: &Map,
    ring: &crate::map::Ring1,
    visited: &crate::map::IndexSet,
    current: &TurfTile,
    players: &[PlayerState],
) {
    let c = current.coordinates;
    if let Some(around) = ring.around(c.x, c.y) {
        for &neighbor in around {
            enqueue_neighbor(queue, map, visited, current, neighbor as usize, players);
        }
        return;
    }
    for neighbor in area_indices_iter(map, c, 1, true, false) {
        enqueue_neighbor(queue, map, visited, current, neighbor, players);
    }
}

/// Builds each player's turf.
///
/// Each player's start tile is a cost-0 turf. Expansion is a linear Dijkstra over Chebyshev
/// radius 1. Land that is not mountain costs 1. Mountain costs 4. Water costs 4, except a
/// step off a city onto water costs 8 when that city is some player's start tile and 16 otherwise.
/// Equal cost keeps the player with the smaller index in `players`.
///
/// The game throws if the visited set's size is not `width * width`. This returns the turfs
/// it built. Groups follow player-list order.
pub fn create_turfs(map: &Map, players: &[PlayerState]) -> Vec<(i32, Vec<TurfTile>)> {
    let width = i32::from(map.width);
    let mut groups: Vec<(i32, Vec<TurfTile>)> = Vec::new();
    let mut visited = crate::map::IndexSet::with_len(map.tiles.len());
    let mut queue = TURF_SCRATCH.with(|s| s.borrow_mut().take()).unwrap_or_else(|| TurfQueue::new(0));
    queue.reset(map.tiles.len());
    let ring = crate::map::Ring1::for_map(map);
    for player in players {
        if player.id == NATURE_PLAYER_ID {
            continue;
        }
        let player_id = i32::from(player.id);
        let turf = TurfTile { coordinates: player.start_tile, cost: 0, player: player_id };
        if let Some(index) = index_at(map, player.start_tile.x, player.start_tile.y) {
            visited.insert(index as i64);
        }
        if let Some((_, tiles)) = groups.iter_mut().find(|(id, _)| *id == player_id) {
            tiles.push(turf.clone());
        } else {
            groups.push((player_id, vec![turf.clone()]));
        }
        expand_turf(&mut queue, map, &ring, &visited, &turf, players);
    }
    let mut guard = width.saturating_mul(width).saturating_add(1);
    loop {
        guard -= 1;
        if guard <= 0 {
            break;
        }
        let Some(turf) = queue.pop(map) else {
            break;
        };
        if let Some(index) = index_at(map, turf.coordinates.x, turf.coordinates.y) {
            visited.insert(index as i64);
        }
        if let Some((_, tiles)) = groups.iter_mut().find(|(id, _)| *id == turf.player) {
            tiles.push(turf.clone());
        } else {
            groups.push((turf.player, vec![turf.clone()]));
        }
        expand_turf(&mut queue, map, &ring, &visited, &turf, players);
    }
    TURF_SCRATCH.with(|s| *s.borrow_mut() = Some(queue));
    groups
}

thread_local! {
    /// The queue of [`create_turfs`], kept between calls.
    static TURF_SCRATCH: std::cell::RefCell<Option<TurfQueue>> = const { std::cell::RefCell::new(None) };
}

/// The inequality score of the map.
///
/// Builds turfs, scores each player's group, and returns [`inequality_from_scores`].
/// The game's spam-level log call is omitted.
pub fn get_inequality(map: &Map, players: &[PlayerState]) -> f32 {
    let scores: Vec<f32> =
        create_turfs(map, players).into_iter().map(|(_, turfs)| count_turf_score(map, &turfs)).collect();
    inequality_from_scores(&scores)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn field_map(width: u16, height: u16) -> Map {
        let mut map = Map::new(width, height);
        for tile in &mut map.tiles {
            tile.terrain = TERRAIN_FIELD;
        }
        map
    }

    fn player(id: u8, x: i32, y: i32) -> PlayerState {
        PlayerState {
            id,
            handicap: 0,
            currency: 0,
            score: 0,
            tribe: 0,
            auto_play: false,
            start_tile: WorldCoordinates { x, y },
        }
    }

    fn turf(x: i32, y: i32, cost: i32) -> TurfTile {
        TurfTile { coordinates: WorldCoordinates { x, y }, cost, player: 1 }
    }

    #[test]
    fn turf_tile_scores_have_the_expected_values() {
        let mut map = field_map(16, 16);
        map.tiles[0].improvement = IMPROVEMENT_CITY;
        map.tiles[0].is_city = true;
        assert_eq!(count_score_for_turf_tile(&map, &turf(0, 0, 0)).to_bits(), 0x42cb_3333);
        assert_eq!(count_score_for_turf_tile(&map, &turf(0, 0, 3)).to_bits(), 0x430d_999a);
        assert_eq!(count_score_for_turf_tile(&map, &turf(0, 0, 4)).to_bits(), 0x42e0_a0ea);

        map.tiles[0].improvement = IMPROVEMENT_RUIN;
        map.tiles[0].is_city = false;
        map.tiles[0].has_ruin = true;
        assert_eq!(count_score_for_turf_tile(&map, &turf(0, 0, 0)).to_bits(), 0x4139_999a);
        assert_eq!(count_score_for_turf_tile(&map, &turf(0, 0, 5)).to_bits(), 0x40b9_13db);

        map.tiles[1].terrain = TERRAIN_FIELD;
        assert_eq!(count_score_for_turf_tile(&map, &turf(1, 0, 2)).to_bits(), 0x3fcc_cccd);
        map.tiles[2].terrain = TERRAIN_WATER;
        assert_eq!(count_score_for_turf_tile(&map, &turf(2, 0, 2)).to_bits(), 0x3dcc_cccd);
    }

    #[test]
    fn turf_score_is_the_sum() {
        let map = field_map(8, 8);
        let tiles = [turf(1, 1, 2), turf(2, 1, 2)];
        let sum = count_score_for_turf_tile(&map, &tiles[0]) + count_score_for_turf_tile(&map, &tiles[1]);
        assert_eq!(count_turf_score(&map, &tiles), sum);
    }

    #[test]
    fn inequality_ratio_uses_floor_and_empty_sentinels() {
        assert_eq!(inequality_from_scores(&[]).to_bits(), (-1.0f32).to_bits());
        assert_eq!(inequality_from_scores(&[2.0, 4.0]), 2.0);
        assert_eq!(inequality_from_scores(&[10.0, 0.0]), 10.0 / INEQUALITY_MIN_FLOOR);
        assert_eq!(inequality_from_scores(&[10.0, -2.0]), 10.0 / INEQUALITY_MIN_FLOOR);
    }

    #[test]
    fn city_spots_reject_edges_water_and_nearby_improvements() {
        let mut map = field_map(11, 11);
        assert!(!is_valid_city_location(&map, 0));
        let inner = index_at(&map, 2, 2).unwrap();
        assert!(is_valid_city_location(&map, inner));
        assert!(is_good_city_location(&map, inner));
        let distance_three = index_at(&map, 3, 2).unwrap();
        assert!(is_valid_city_location(&map, distance_three));
        assert!(!is_good_city_location(&map, distance_three));

        let water = index_at(&map, 4, 2).unwrap();
        map.tiles[water].terrain = TERRAIN_WATER;
        assert!(!is_valid_city_location(&map, water));

        let ruin = index_at(&map, 5, 5).unwrap();
        map.tiles[ruin].improvement = IMPROVEMENT_RUIN;
        map.tiles[ruin].has_ruin = true;
        let beside_ruin = index_at(&map, 6, 5).unwrap();
        let far_ruin = index_at(&map, 6, 6).unwrap();
        let clear = index_at(&map, 1, 1).unwrap();
        assert!(!is_valid_city_location(&map, ruin));
        assert!(!is_valid_city_location(&map, beside_ruin));
        assert!(!valid_ruin_location(&map, far_ruin));
        assert!(valid_ruin_location(&map, clear));
    }

    #[test]
    fn capital_radius_grows_with_domain_and_ignores_ownerless_cities() {
        let mut map = field_map(16, 16);
        let spot = index_at(&map, 4, 4).unwrap();
        assert!(is_valid_capital_location(&map, spot, 4));
        map.tiles[spot].improvement = IMPROVEMENT_CITY;
        map.tiles[spot].is_city = true;
        map.tiles[spot].owner = 1;
        assert!(is_near_capital(&map, spot, 2, true));
        let nearby = index_at(&map, 6, 4).unwrap();
        assert!(!is_valid_capital_location(&map, nearby, 4));
        map.tiles[spot].owner = 0;
        assert!(!is_near_capital(&map, spot, 2, true));
        assert!(is_capital_position_possible(&map, index_at(&map, 2, 8).unwrap(), 16));
        let water = index_at(&map, 8, 8).unwrap();
        map.tiles[water].terrain = TERRAIN_OCEAN;
        assert!(is_capital_position_possible(&map, water, 2));
        assert!(!is_valid_capital_location(&map, water, 2));
    }

    #[test]
    fn valid_city_spots_are_the_interior_without_improvements() {
        let map = field_map(6, 6);
        let spots = get_all_valid_city_spot_indices(&map);
        assert!(spots.iter().all(|index| {
            let tile = &map.tiles[*index as usize];
            tile.coordinates.x > 0 && tile.coordinates.y > 0 && tile.coordinates.x < 5 && tile.coordinates.y < 5
        }));
        assert_eq!(spots.len(), 16);
    }

    #[test]
    fn turfs_expand_from_the_start_tile_and_delay_mountains() {
        let mut map = field_map(3, 3);
        let mountain = index_at(&map, 2, 1).unwrap();
        map.tiles[mountain].terrain = TERRAIN_MOUNTAIN;
        let groups = create_turfs(&map, &[player(1, 1, 1)]);
        assert_eq!(groups.len(), 1);
        let tiles = &groups[0].1;
        assert_eq!(tiles[0].coordinates, WorldCoordinates { x: 1, y: 1 });
        assert_eq!(tiles[0].cost, 0);
        assert!(tiles.iter().any(|turf| turf.coordinates == WorldCoordinates { x: 2, y: 1 } && turf.cost == 4));
        assert!(tiles.iter().any(|turf| turf.coordinates == WorldCoordinates { x: 0, y: 1 } && turf.cost == 1));
        assert_eq!(tiles.len(), 9);
        let mountain = tiles.iter().position(|turf| turf.cost == 4).unwrap();
        assert!(tiles[..mountain].iter().all(|turf| turf.cost < 4));
    }

    #[test]
    fn single_player_inequality_is_one_when_scores_match() {
        let map = field_map(3, 3);
        let ratio = get_inequality(&map, &[player(1, 1, 1)]);
        assert_eq!(ratio, 1.0);
    }

    #[test]
    fn fixed_hash_picks_a_resource_and_rejects_a_blocked_whale() {
        let mut map = field_map(5, 5);
        let index = index_at(&map, 2, 2).unwrap();
        let resources = vec![
            ResourceSpawn { kind: RESOURCE_CROP, terrains: vec![TERRAIN_FIELD] },
            ResourceSpawn { kind: RESOURCE_WHALE, terrains: vec![] },
            ResourceSpawn { kind: RESOURCE_STARFISH, terrains: vec![] },
        ];
        let modifiers = ContinentModifiers {
            game: 1.0,
            crop: 1.0,
            fish: 1.0,
            whale: 1.0,
            metal: 1.0,
            fruit: 1.0,
            spores: 1.0,
            aquacrop: 1.0,
        };
        let hash = XxHash::new(7);
        let mut rng = DotNetRandom::new(1);
        let placed = add_resource(&mut map, index, &resources, Some(&modifiers), &mut rng, Some(&hash));
        assert_eq!(placed, Some(map.tiles[index].resource));
        assert_ne!(map.tiles[index].resource, RESOURCE_STARFISH);

        let city = index_at(&map, 2, 1).unwrap();
        map.tiles[city].improvement = IMPROVEMENT_CITY;
        map.tiles[city].is_city = true;
        map.tiles[index].resource = 0;
        let only_whale = [ResourceSpawn { kind: RESOURCE_WHALE, terrains: vec![] }];
        assert_eq!(add_resource(&mut map, index, &only_whale, Some(&modifiers), &mut rng, Some(&hash)), None);
        assert_eq!(map.tiles[index].resource, 0);

        let bare = index_at(&map, 0, 0).unwrap();
        assert_eq!(add_resource(&mut map, bare, &resources, None, &mut rng, Some(&hash)), None);
        assert_eq!(map.tiles[bare].terrain, TERRAIN_ICE);
    }

    #[test]
    fn capitals_follow_continent_order_and_the_west_player_takes_max_y_minus_x() {
        let mut map = field_map(8, 8);
        let west = index_at(&map, 2, 5).unwrap();
        let east = index_at(&map, 5, 2).unwrap();
        map.tiles[west].improvement = IMPROVEMENT_CITY;
        map.tiles[west].is_city = true;
        map.tiles[east].improvement = IMPROVEMENT_CITY;
        map.tiles[east].is_city = true;
        let continents = vec![vec![west as i32], vec![east as i32]];
        let mut players = vec![player(1, 0, 0), player(2, 0, 0)];
        let mut rng = DotNetRandom::new(1);
        assert!(try_add_capitals_to_continents(
            &mut map,
            &mut players,
            &continents,
            2,
            &mut rng,
            Some(2),
            &[BONUS_SPORES, 0],
            &[4, 9],
        ));
        assert!(map.tiles[west].is_capital);
        assert_eq!(map.tiles[west].owner, 2);
        assert_eq!(map.tiles[west].improvement, IMPROVEMENT_CITY);
        assert_eq!(players[1].start_tile, map.tiles[west].coordinates);
        assert_eq!(map.tiles[east].owner, 1);
        assert!(map.tiles[east].is_capital);
        assert_eq!(players[0].start_tile, map.tiles[east].coordinates);
        assert_eq!(map.tiles[east].climate, 4);
        assert_eq!(map.tiles[west].climate, 9);
        let east_neighbor = index_at(&map, 4, 2).unwrap();
        let west_neighbor = index_at(&map, 1, 5).unwrap();
        assert_eq!(map.tiles[east_neighbor].improvement, IMPROVEMENT_FUNGI);
        assert_eq!(map.tiles[west_neighbor].improvement, 0);
        assert_eq!(map.tiles[east].improvement, IMPROVEMENT_CITY);
    }

    #[test]
    fn starting_resources_use_tribe_rows_then_place_on_a_matching_neighbor() {
        let data = crate::data::load_embedded_game_logic().expect("embedded rules");
        let aimo = data.tribe("aimo").expect("aimo");
        let mut rng = DotNetRandom::new(1);
        assert_eq!(get_starting_resources_for_player(&data, aimo.idx, &mut rng), vec![RESOURCE_GAME, RESOURCE_GAME]);

        let mut map = field_map(5, 5);
        let forest = index_at(&map, 2, 1).unwrap();
        let forest_idx = data.terrain("forest").expect("forest").idx;
        map.tiles[forest].terrain = forest_idx;
        let start = WorldCoordinates { x: 2, y: 2 };
        add_starting_resources_to_capital(&mut map, start, &[], &data, &mut rng);
        assert_eq!(map.tiles[forest].resource, 0);
        add_starting_resources_to_capital(&mut map, start, &aimo.starting_resource, &data, &mut rng);
        assert_eq!(map.tiles[forest].resource, RESOURCE_GAME);
        assert_eq!(map.tiles[forest].terrain, forest_idx);
        let placed = map.tiles.iter().filter(|tile| tile.resource == RESOURCE_GAME).count();
        assert_eq!(placed, 2);
        assert_eq!(map.tiles.iter().filter(|tile| tile.terrain == forest_idx).count(), 2);
    }
}
