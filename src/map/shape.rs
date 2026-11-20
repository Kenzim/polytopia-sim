//! Map shape: presets, capital indexes, suburb placement, and list shuffles.
//!
//! Reproduces the game's map generator shape logic and settings.

use crate::rng::DotNetRandom;
use crate::{MapGeneratorSettings, MapPreset, Tile, WorldCoordinates, DESIRED_DOMAIN_SIZE, MINIMUM_DOMAIN_SIZE};

const WETNESS_045: f32 = f32::from_bits(0x3ee66666);
const WETNESS_030: f32 = f32::from_bits(0x3e99999a);
const WETNESS_050: f32 = f32::from_bits(0x3f000000);
const WETNESS_065: f32 = f32::from_bits(0x3f266666);
const WETNESS_085: f32 = f32::from_bits(0x3f59999a);
const DENSITY_010: f32 = f32::from_bits(0x3dcccccd);
const DENSITY_030: f32 = f32::from_bits(0x3e99999a);
const ONE: f32 = f32::from_bits(0x3f800000);
const HALF: f32 = f32::from_bits(0x3f000000);
const EQUALITY_LIMIT: f32 = f32::from_bits(0x3f8147ae);
/// Suburb ring limit: `width * 0.6f` (the `f32` `0x3F19999A`).
const SUBURB_RING_SCALE: f32 = f32::from_bits(0x3f19999a);
/// Rings start at radius 3. Search is skipped when `width * 0.6f <= 3f`.
const SUBURB_RING_MIN: f32 = f32::from_bits(0x40400000);

/// Map generator settings for a preset.
///
/// Field values match the game, including `equality_iterations = 50` and
/// `equality_limit` bits `0x3F8147AE`. `shallow_percent_of_water` stays 0 because
/// no preset sets it.
pub fn create_from_preset(preset: MapPreset) -> MapGeneratorSettings {
    let mut settings = MapGeneratorSettings {
        wetness: WETNESS_045,
        richness: ONE,
        smooth_iterations: 2,
        pre_terrain_city_density: DENSITY_030,
        post_terrain_city_density: ONE,
        min_suburb_count: 1,
        max_suburb_count: 2,
        surrounding_space_value: HALF,
        shallow_percent_of_water: 0.0,
        equality_iterations: 50,
        equality_limit: EQUALITY_LIMIT,
        map_type: preset,
    };
    match preset {
        MapPreset::None | MapPreset::Continents => {
            settings.wetness = WETNESS_050;
            settings.surrounding_space_value = ONE;
            settings.smooth_iterations = 1;
        }
        MapPreset::Dryland => {
            settings.wetness = 0.0;
            settings.smooth_iterations = 0;
            settings.pre_terrain_city_density = 0.0;
            settings.post_terrain_city_density = ONE;
            settings.min_suburb_count = 0;
            settings.max_suburb_count = 0;
        }
        MapPreset::Lakes => {
            settings.wetness = WETNESS_030;
            settings.smooth_iterations = 2;
            settings.surrounding_space_value = ONE;
        }
        MapPreset::Archipelago => {
            settings.wetness = WETNESS_065;
            settings.smooth_iterations = 0;
        }
        MapPreset::WaterWorld => {
            settings.wetness = WETNESS_085;
            settings.smooth_iterations = 2;
            settings.pre_terrain_city_density = DENSITY_010;
            settings.min_suburb_count = 0;
            settings.max_suburb_count = 0;
        }
        MapPreset::Pangea => {
            settings.wetness = WETNESS_050;
            settings.surrounding_space_value = 0.0;
            settings.smooth_iterations = 1;
        }
    }
    settings
}

/// Minimum distance between capitals.
///
/// `(map_width - 1) / ceil(sqrt(player_count))`, then `4` when that quotient
/// is at least [`DESIRED_DOMAIN_SIZE`] and [`MINIMUM_DOMAIN_SIZE`] otherwise.
pub fn min_capital_distance(map_width: i32, player_count: i32) -> i32 {
    let quotient = (map_width - 1) / grid_side(player_count);
    if quotient >= DESIRED_DOMAIN_SIZE {
        MINIMUM_DOMAIN_SIZE + 1
    } else {
        MINIMUM_DOMAIN_SIZE
    }
}

/// Domain size per player.
///
/// `map_width / ceil(sqrt(player_count))`, truncating toward zero.
pub fn domain_size(map_width: i32, player_count: i32) -> i32 {
    map_width / grid_side(player_count)
}

/// Chooses the capital positions.
///
/// RNG order per capital is `next_range(0, remaining_cells)` then
/// `next_range(0, probability_sum)`, both plain .NET `Random.Next(min, max)` draws.
///
/// Panics when the grid cell size is 2 or less. The game throws there
/// instead of placing capitals.
pub fn generate_player_capital_positions(rng: &mut DotNetRandom, width: i32, player_count: i32) -> Vec<i32> {
    if width <= 0 {
        panic!("map width {width} is not positive");
    }
    let side = grid_side(player_count);
    let cell = width / side;
    let remainder = width % side;
    if cell <= 2 {
        panic!("capital grid cell size {cell} is below the minimum for width {width} and {player_count} players");
    }

    let mut cells: Vec<i32> = (0..side * side).collect();
    let tile_count =
        width.checked_mul(width).unwrap_or_else(|| panic!("map width {width} does not fit a probability table"));
    let mut probabilities = vec![0i32; tile_count as usize];
    let half = cell / 2;
    for row in 0..side {
        for col in 0..side {
            let x = cell * col + cell_adjust(remainder, col) + half;
            let y = cell * row + cell_adjust(remainder, row) + half;
            add_distance_to_probability_table(&mut probabilities, width, WorldCoordinates { x, y }, cell);
        }
    }

    let mut capitals = Vec::with_capacity(player_count as usize);
    let width_inset = width - 2;
    let inset = if cell < 5 { 1 } else { 2 };
    for _ in 0..player_count {
        let pick = rng.next_range(0, cells.len() as i32);
        let cell_id = cells.remove(pick as usize);
        let (cell_x, cell_y) = from_index(cell_id, side);
        let origin_x = cell * cell_x + cell_adjust(remainder, cell_x);
        let origin_y = cell * cell_y + cell_adjust(remainder, cell_y);
        let start_x = 2.max(origin_x + inset);
        let end_x = width_inset.min(origin_x + cell - inset);
        let start_y = 2.max(origin_y + inset);
        let end_y = width_inset.min(origin_y + cell - inset);
        let total = calculate_probability_in_range(&probabilities, width, start_x, end_x, start_y, end_y);
        let value = rng.next_range(0, total);
        capitals.push(index_for_probability_value_in_range(
            &probabilities,
            width,
            value,
            start_x,
            end_x,
            start_y,
            end_y,
        ));
    }
    capitals
}

/// Whether [`generate_player_capital_positions`] can place `player_count` capitals on a map of `width` for every seed:
/// the grid cells must be wider than 2 tiles and every cell must leave a non-empty range to pick from. (Which cells
/// get picked depends on the seed, so one empty cell is enough to fail.)
pub fn capital_grid_fits(width: i32, player_count: i32) -> bool {
    if width <= 0 || player_count <= 0 {
        return false;
    }
    let side = grid_side(player_count);
    let cell = width / side;
    let remainder = width % side;
    if cell <= 2 {
        return false;
    }
    let inset = if cell < 5 { 1 } else { 2 };
    // The x and y ranges use the same arithmetic, so checking one axis covers both.
    (0..side).all(|i| {
        let origin = cell * i + cell_adjust(remainder, i);
        2.max(origin + inset) < (width - 2).min(origin + cell - inset)
    })
}

/// Adds a capital's distance weighting to the probability table.
pub fn add_distance_to_probability_table(
    probabilities: &mut [i32],
    width: i32,
    coordinates: WorldCoordinates,
    domain_size: i32,
) {
    if probabilities.is_empty() {
        return;
    }
    let half = domain_size / 2;
    for (index, slot) in probabilities.iter_mut().enumerate() {
        let (x, y) = from_index(index as i32, width);
        let distance = chebyshev(x, y, coordinates.x, coordinates.y);
        let weight = 1.max(half - distance);
        let cube = weight * weight * weight;
        if *slot == 0 {
            *slot = cube;
        } else {
            *slot = (*slot).max(cube);
        }
    }
}

/// Sums the probability table over a range.
///
/// Sums `max(1, probability)` over `x` in `[start_x, end_x)` and `y` in
/// `[start_y, end_y)`. An empty range returns 0.
pub fn calculate_probability_in_range(
    probabilities: &[i32],
    width: i32,
    start_x: i32,
    end_x: i32,
    start_y: i32,
    end_y: i32,
) -> i32 {
    if start_y >= end_y || start_x >= end_x {
        return 0;
    }
    let mut sum = 0i32;
    let mut y = start_y;
    while y < end_y {
        let mut x = start_x;
        while x < end_x {
            let index = to_index(x, y, width) as usize;
            sum += 1.max(probabilities[index]);
            x += 1;
        }
        y += 1;
    }
    sum
}

/// Finds the tile index for a probability value in a range.
///
/// Walks the same exclusive range as [`calculate_probability_in_range`] and
/// returns the tile index where the running `max(1, probability)` sum first
/// becomes greater than `value`. Panics if the value is never passed, which
/// is the game's exception path.
pub fn index_for_probability_value_in_range(
    probabilities: &[i32],
    width: i32,
    value: i32,
    start_x: i32,
    end_x: i32,
    start_y: i32,
    end_y: i32,
) -> i32 {
    if start_y < end_y && start_x < end_x {
        let mut sum = 0i32;
        let mut y = start_y;
        while y < end_y {
            let mut x = start_x;
            while x < end_x {
                let index = to_index(x, y, width);
                sum += 1.max(probabilities[index as usize]);
                if sum > value {
                    return index;
                }
                x += 1;
            }
            y += 1;
        }
    }
    panic!("probability value {value} was not found in the capital range");
}

/// Shuffles a list of coordinates.
pub fn shuffle_coords(rng: &mut DotNetRandom, list: &mut [WorldCoordinates]) {
    fisher_yates(rng, list);
}

/// Shuffles a list of tiles.
pub fn shuffle_tiles(rng: &mut DotNetRandom, list: &mut [Tile]) {
    fisher_yates(rng, list);
}

/// Places the suburbs.
///
/// `rng` is the generator's own random. The tribe lookups and the verbose logs do
/// not draw from it and are omitted. Returns the suburbs placed on the rings,
/// in round-then-player order. Borrowed fallback indexes are not included;
/// they only affect `land_indices`.
pub fn generate_suburbs(
    rng: &mut DotNetRandom,
    map_width: i32,
    map_height: i32,
    player_count: i32,
    capital_indices: &[i32],
    suburb_count: i32,
    land_indices: &mut Vec<i32>,
) -> Vec<i32> {
    if suburb_count == 0 || player_count <= 0 {
        return Vec::new();
    }
    if suburb_count < 0 {
        panic!("generate_suburbs: negative suburb count");
    }
    if capital_indices.len() < player_count as usize {
        panic!("GenerateSuburbs capital index is out of range");
    }

    let mut per_player = vec![Vec::new(); player_count as usize];
    let mut placed = Vec::new();
    for _round in 0..suburb_count {
        for player in 0..player_count {
            let capital = capital_indices[player as usize];
            let (cx, cy) = from_index(capital, map_width);
            let mut candidates = suburb_ring_candidates(
                cx,
                cy,
                map_width,
                map_height,
                capital_indices,
                &placed,
                &per_player[player as usize],
            );
            if candidates.is_empty() {
                continue;
            }
            let pick = rng.next_range(0, candidates.len() as i32);
            let chosen = candidates[pick as usize];
            per_player[player as usize].push(chosen);
            placed.push(chosen);
            remove_indices_near_index(&mut candidates, chosen, map_width, 2);
        }
    }

    for player in 0..player_count {
        let capital = capital_indices[player as usize];
        let subs = &mut per_player[player as usize];
        if subs.len() as i32 != suburb_count {
            let mut fills = 0i32;
            while fills != suburb_count {
                let best = closest_suburb_borrow(capital, &placed, capital_indices, subs, map_width);
                if best < 0 {
                    break;
                }
                subs.push(best);
                fills += 1;
            }
        }
        link_suburb_land(rng, capital, subs, land_indices, map_width);
    }
    placed
}

fn fisher_yates<T>(rng: &mut DotNetRandom, list: &mut [T]) {
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

fn grid_side(player_count: i32) -> i32 {
    let side = f64::from(player_count).sqrt().ceil();
    if !side.is_finite() || side <= 0.0 {
        panic!("map grid side is undefined for {player_count} players");
    }
    side as i32
}

/// `Math.Min(remainder, Math.Max(1, Math.Min(remainder, index) - 1))`.
fn cell_adjust(remainder: i32, index: i32) -> i32 {
    let clamped = remainder.min(index);
    let raised = 1.max(clamped - 1);
    remainder.min(raised)
}

fn from_index(index: i32, width: i32) -> (i32, i32) {
    (index % width, index / width)
}

fn to_index(x: i32, y: i32, width: i32) -> i32 {
    y * width + x
}

fn chebyshev(x0: i32, y0: i32, x1: i32, y1: i32) -> i32 {
    abs_i32(x0 - x1).max(abs_i32(y0 - y1))
}

fn abs_i32(value: i32) -> i32 {
    if value < 0 {
        value.wrapping_neg()
    } else {
        value
    }
}

fn sign_i32(value: i32) -> i32 {
    if value > 0 {
        1
    } else if value < 0 {
        -1
    } else {
        0
    }
}

/// Whether a coordinate is near any of the indices.
///
/// True when any index is within Chebyshev distance `range` of `coord`.
fn is_within_range_of_indices(coord: (i32, i32), indices: &[i32], width: i32, range: i32) -> bool {
    for &index in indices {
        let (x, y) = from_index(index, width);
        if abs_i32(coord.0.wrapping_sub(x)) <= range && abs_i32(coord.1.wrapping_sub(y)) <= range {
            return true;
        }
    }
    false
}

/// Square Chebyshev ring used for suburb placement.
///
/// Radius starts at 3 and grows while `(radius as f32) < width * 0.6f`.
/// A cell is kept when it sits on that square's perimeter, strictly inside
/// the map border (`0 < x < width - 1`, `0 < y < height - 1`), and outside
/// Chebyshev range 2 of every capital, every placed suburb, and this player's
/// suburbs. Cells are appended in row-major order, so the list stays sorted.
fn suburb_ring_candidates(
    cx: i32,
    cy: i32,
    map_width: i32,
    map_height: i32,
    capitals: &[i32],
    placed: &[i32],
    player_subs: &[i32],
) -> Vec<i32> {
    let limit = (map_width as f32) * SUBURB_RING_SCALE;
    if limit <= SUBURB_RING_MIN {
        return Vec::new();
    }
    let x_border = map_width.wrapping_sub(1);
    let y_border = map_height.wrapping_sub(1);
    let mut radius = 3i32;
    let mut candidates = Vec::new();
    loop {
        candidates.clear();
        let y_min = cy.wrapping_sub(radius);
        let y_max = cy.wrapping_add(radius);
        let x_min = cx.wrapping_sub(radius);
        let x_max = cx.wrapping_add(radius);
        let right_col = radius.wrapping_mul(-2);
        let mut y = y_min;
        while y <= y_max {
            if y >= y_border {
                break;
            }
            if y > 0 {
                let y_edge = y == y_min || y == y_max;
                let mut x = x_min;
                let mut col = 0i32;
                loop {
                    if x >= x_border {
                        break;
                    }
                    if x > 0 && (col == 0 || col == right_col || y_edge) {
                        let coord = (x, y);
                        if !is_within_range_of_indices(coord, capitals, map_width, 2)
                            && !is_within_range_of_indices(coord, placed, map_width, 2)
                            && !is_within_range_of_indices(coord, player_subs, map_width, 2)
                        {
                            candidates.push(y.wrapping_mul(map_width).wrapping_add(x));
                        }
                    }
                    col -= 1;
                    if x >= x_max {
                        break;
                    }
                    x += 1;
                }
            }
            y += 1;
        }
        if !candidates.is_empty() {
            return candidates;
        }
        radius += 1;
        if (radius as f32) >= limit {
            return Vec::new();
        }
    }
}

/// Binary search matching .NET's `List<int>.BinarySearch`: index of `value`, or `None`.
fn binary_search_i32(items: &[i32], value: i32) -> Option<usize> {
    let mut lo = 0i32;
    let mut hi = items.len() as i32 - 1;
    while lo <= hi {
        let mid = lo + ((hi - lo) >> 1);
        let found = items[mid as usize];
        if found == value {
            return Some(mid as usize);
        }
        if found < value {
            lo = mid + 1;
        } else {
            hi = mid - 1;
        }
    }
    None
}

/// Removes indices near an index.
///
/// Deletes every sorted-list entry inside the inclusive Chebyshev box of
/// `range` around `index`. Lookup is the binary search above.
fn remove_indices_near_index(indices: &mut Vec<i32>, index: i32, width: i32, range: i32) {
    let (x, y) = from_index(index, width);
    let mut yy = y.wrapping_sub(range);
    let y_end = y.wrapping_add(range);
    let x_start = x.wrapping_sub(range);
    let x_end = x.wrapping_add(range);
    while yy <= y_end {
        let mut xx = x_start;
        while xx <= x_end {
            let tile = yy.wrapping_mul(width).wrapping_add(xx);
            if let Some(pos) = binary_search_i32(indices, tile) {
                indices.remove(pos);
            }
            xx = xx.wrapping_add(1);
        }
        yy = yy.wrapping_add(1);
    }
}

/// Closest capital or already-placed suburb that this player does not already own.
///
/// Distance is Chebyshev from this capital. Ties keep the earlier index.
/// `placed` is scanned before `capitals`. Returns -1 when nothing qualifies.
fn closest_suburb_borrow(capital: i32, placed: &[i32], capitals: &[i32], player_subs: &[i32], map_width: i32) -> i32 {
    let (cx, cy) = from_index(capital, map_width);
    let mut best = -1i32;
    let mut best_dist = map_width;
    for &index in placed.iter().chain(capitals.iter()) {
        if index == capital || player_subs.contains(&index) {
            continue;
        }
        let (x, y) = from_index(index, map_width);
        let dist = chebyshev(cx, cy, x, y);
        if dist < best_dist {
            best = index;
            best_dist = dist;
        }
    }
    best
}

/// Centroid of the capital plus this player's suburbs, then a random walk to each.
fn link_suburb_land(rng: &mut DotNetRandom, capital: i32, suburbs: &[i32], land_indices: &mut Vec<i32>, width: i32) {
    let (mut sum_x, mut sum_y) = from_index(capital, width);
    for &suburb in suburbs {
        let (x, y) = from_index(suburb, width);
        sum_x = sum_x.wrapping_add(x);
        sum_y = sum_y.wrapping_add(y);
    }
    let divisor = suburbs.len() as i32 + 1;
    let center = (sum_x / divisor, sum_y / divisor);
    add_land_indices_between_tiles(rng, center, capital, land_indices, width);
    for &suburb in suburbs {
        add_land_indices_between_tiles(rng, center, suburb, land_indices, width);
    }
}

/// Adds land indices on a random walk between two tiles.
///
/// Walks at most 1000 steps from `from` toward `to_index`. The current cell is
/// appended when it is not already present. The destination is not appended:
/// the walk returns once the cells are equal or one orthogonal step apart.
/// Each longer step draws `next_range(1, 4)`. Bit 0 steps X and bit 1 steps Y.
/// A zero sign on a stepped axis draws `next_double`, keeps -1 when the float
/// is `<= 0.5f`, and uses +1 otherwise. Each axis is then clamped to
/// `[0, width - 1]`.
fn add_land_indices_between_tiles(
    rng: &mut DotNetRandom,
    mut from: (i32, i32),
    to_index: i32,
    land_indices: &mut Vec<i32>,
    width: i32,
) {
    let to = from_index(to_index, width);
    let width_m1 = width.wrapping_sub(1);
    let mut steps = 1000i32;
    while steps > 0 {
        if from == to {
            return;
        }
        let index = from.1.wrapping_mul(width).wrapping_add(from.0);
        if !land_indices.contains(&index) {
            land_indices.push(index);
        }
        let dx = to.0.wrapping_sub(from.0);
        let dy = to.1.wrapping_sub(from.1);
        if abs_i32(dx).wrapping_add(abs_i32(dy)) == 1 {
            return;
        }
        let mut sign_x = sign_i32(dx);
        let mut sign_y = sign_i32(dy);
        let roll = rng.next_range(1, 4);
        if roll & 1 != 0 {
            if sign_x == 0 {
                sign_x = if (rng.next_double() as f32) <= HALF { -1 } else { 1 };
            }
            from.0 = width_m1.min(0.max(from.0.wrapping_add(sign_x)));
        }
        if roll & 2 != 0 {
            if sign_y == 0 {
                sign_y = if (rng.next_double() as f32) <= HALF { -1 } else { 1 };
            }
            from.1 = width_m1.min(0.max(from.1.wrapping_add(sign_y)));
        }
        steps -= 1;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{MapPreset, MapSize};

    #[test]
    fn preset_field_bits_match_create_from_preset() {
        let continents = create_from_preset(MapPreset::Continents);
        let none = create_from_preset(MapPreset::None);
        assert_eq!(continents.wetness.to_bits(), 0x3f000000);
        assert_eq!(continents.surrounding_space_value.to_bits(), 0x3f800000);
        assert_eq!(continents.smooth_iterations, 1);
        assert_eq!(continents.richness.to_bits(), 0x3f800000);
        assert_eq!(continents.pre_terrain_city_density.to_bits(), 0x3e99999a);
        assert_eq!(continents.post_terrain_city_density.to_bits(), 0x3f800000);
        assert_eq!(continents.min_suburb_count, 1);
        assert_eq!(continents.max_suburb_count, 2);
        assert_eq!(continents.shallow_percent_of_water.to_bits(), 0);
        assert_eq!(continents.equality_iterations, 50);
        assert_eq!(continents.equality_limit.to_bits(), 0x3f8147ae);
        assert_eq!(continents.map_type, MapPreset::Continents);
        assert_eq!(none.wetness.to_bits(), continents.wetness.to_bits());
        assert_eq!(none.smooth_iterations, continents.smooth_iterations);
        assert_eq!(none.map_type, MapPreset::None);

        let dryland = create_from_preset(MapPreset::Dryland);
        assert_eq!(dryland.wetness.to_bits(), 0);
        assert_eq!(dryland.smooth_iterations, 0);
        assert_eq!(dryland.pre_terrain_city_density.to_bits(), 0);
        assert_eq!(dryland.min_suburb_count, 0);
        assert_eq!(dryland.max_suburb_count, 0);
        assert_eq!(dryland.surrounding_space_value.to_bits(), 0x3f000000);

        let lakes = create_from_preset(MapPreset::Lakes);
        assert_eq!(lakes.wetness.to_bits(), 0x3e99999a);
        assert_eq!(lakes.smooth_iterations, 2);
        assert_eq!(lakes.surrounding_space_value.to_bits(), 0x3f800000);

        let archipelago = create_from_preset(MapPreset::Archipelago);
        assert_eq!(archipelago.wetness.to_bits(), 0x3f266666);
        assert_eq!(archipelago.smooth_iterations, 0);
        assert_eq!(archipelago.min_suburb_count, 1);

        let water = create_from_preset(MapPreset::WaterWorld);
        assert_eq!(water.wetness.to_bits(), 0x3f59999a);
        assert_eq!(water.pre_terrain_city_density.to_bits(), 0x3dcccccd);
        assert_eq!(water.min_suburb_count, 0);
        assert_eq!(water.max_suburb_count, 0);
        assert_eq!(water.surrounding_space_value.to_bits(), 0x3f000000);

        let pangea = create_from_preset(MapPreset::Pangea);
        assert_eq!(pangea.wetness.to_bits(), 0x3f000000);
        assert_eq!(pangea.surrounding_space_value.to_bits(), 0);
        assert_eq!(pangea.smooth_iterations, 1);
    }

    #[test]
    fn domain_and_min_distance_follow_ceil_sqrt() {
        assert_eq!(domain_size(16, 4), 8);
        assert_eq!(min_capital_distance(16, 4), 4);
        assert_eq!(domain_size(11, 2), 5);
        assert_eq!(min_capital_distance(11, 2), 4);
        assert_eq!(domain_size(14, 8), 4);
        assert_eq!(min_capital_distance(14, 8), 3);
        assert_eq!(domain_size(30, 16), 7);
        assert_eq!(min_capital_distance(30, 16), 4);
    }

    #[test]
    fn distance_cube_fills_empty_probability_slots() {
        let mut probabilities = vec![0i32; 25];
        add_distance_to_probability_table(&mut probabilities, 5, WorldCoordinates { x: 2, y: 2 }, 4);
        assert_eq!(probabilities[to_index(2, 2, 5) as usize], 8);
        assert_eq!(probabilities[to_index(2, 1, 5) as usize], 1);
        assert_eq!(probabilities[to_index(0, 0, 5) as usize], 1);
        let total = calculate_probability_in_range(&probabilities, 5, 2, 3, 2, 3);
        assert_eq!(total, 8);
        assert_eq!(index_for_probability_value_in_range(&probabilities, 5, 0, 2, 3, 2, 3), to_index(2, 2, 5));
    }

    #[test]
    fn fixed_seed_capital_indexes() {
        let settings = create_from_preset(MapPreset::Continents);
        assert_eq!(settings.map_type, MapPreset::Continents);
        let width = i32::from(MapSize::Normal.width());
        let mut rng = DotNetRandom::new(42);
        let capitals = generate_player_capital_positions(&mut rng, width, 5);
        assert_eq!(capitals, vec![211, 56, 61, 216, 51]);
    }

    #[test]
    fn shuffle_consumes_range_from_the_front() {
        let mut rng = DotNetRandom::new(42);
        let mut values: Vec<i32> = (0..8).collect();
        let mut tiles: Vec<Tile> = values.iter().map(|&index| Tile { index, ..Tile::default() }).collect();
        fisher_yates(&mut rng, &mut values);
        let mut rng = DotNetRandom::new(42);
        shuffle_tiles(&mut rng, &mut tiles);
        assert_eq!(values, vec![5, 1, 2, 0, 4, 3, 7, 6]);
        assert_eq!(tiles.iter().map(|tile| tile.index).collect::<Vec<_>>(), values);
    }

    #[test]
    fn within_range_and_remove_follow_chebyshev_box() {
        let capitals = [to_index(2, 2, 10)];
        assert!(is_within_range_of_indices((4, 3), &capitals, 10, 2));
        assert!(!is_within_range_of_indices((5, 3), &capitals, 10, 2));
        assert!(!is_within_range_of_indices((4, 3), &[], 10, 2));

        let mut indices = vec![6, 7, 12, 13, 20];
        remove_indices_near_index(&mut indices, to_index(2, 2, 5), 5, 1);
        assert_eq!(indices, vec![20]);
    }

    #[test]
    fn fixed_seed_suburbs() {
        let capitals = [211, 56, 61, 216, 51];
        let mut untouched = DotNetRandom::new(42);
        let mut idle = untouched.clone();
        let mut land = Vec::new();
        let empty = generate_suburbs(&mut idle, 16, 16, 5, &capitals, 0, &mut land);
        assert!(empty.is_empty());
        assert!(land.is_empty());
        assert_eq!(idle.next(), untouched.next());

        let mut rng = DotNetRandom::new(42);
        let mut land = Vec::new();
        let suburbs = generate_suburbs(&mut rng, 16, 16, 5, &capitals, 2, &mut land);
        // Produced by this port through DotNetRandom. Not a retail map capture.
        assert_eq!(suburbs, vec![165, 101, 106, 187, 97, 161, 141, 152, 235]);
        assert_eq!(
            land,
            vec![
                179, 194, 195, 163, 164, 162, 88, 72, 87, 102, 104, 120, 121, 122, 139, 156, 157, 106, 91, 92, 77, 137,
                136, 202, 201, 200, 186, 219, 68, 67, 84, 99, 82, 81, 69, 70, 86, 85, 54, 55,
            ]
        );
    }
}
