//! Pathfinding and the attack-option scan.
//!
//! The movement cost of a tile is not inlined. Version `<= 114` uses
//! [`get_movement_cost`]'s V114 body (`movement_cost_v114`); later versions use
//! `movement_cost_default`. Callers pass that result.

use crate::{GameState, Tile, UnitState, WorldCoordinates};

/// Number of grid directions, used by [`opposite_direction`].
const DIRECTION_COUNT: i32 = 8;
/// The "no direction" value.
const DIRECTION_NONE: i32 = 8;

/// Unit ability values read by these functions.
const ABILITY_HIDE: i32 = 5;
const ABILITY_CONVERT: i32 = 8;
const ABILITY_FLY: i32 = 13;
const ABILITY_INFILTRATE: i32 = 35;
const ABILITY_DRENCH: i32 = 46;

/// The City and Port improvement types.
const IMPROVEMENT_CITY: i32 = 1;
const IMPROVEMENT_PORT: i32 = 8;
/// The Bridge improvement type.
const IMPROVEMENT_BRIDGE: i32 = 48;
/// The Bridge and Embark improvement abilities.
const IMPROVEMENT_ABILITY_BRIDGE: i32 = 13;
const IMPROVEMENT_ABILITY_EMBARK: i32 = 16;

/// Terrain type values these checks compare against.
const TERRAIN_WETLAND: i32 = 7;

/// Diplomacy relation states Peace and CoolDown.
const PEACE: i32 = 1;
const COOLDOWN: i32 = 3;

/// Dequeue cap in [`get_move_options`] and [`get_path`] (1000).
const SEARCH_LIMIT: i32 = 1000;

/// [`is_tile_accessible`] uses Hide above this version and Invisible at or below it.
pub const VERSION_OCCUPIED_HIDE: i32 = 0x52;
/// Embark gate switches at `version > 98`.
pub const VERSION_EMBARK_GATE: i32 = 0x62;
/// The embark improvement check uses the Port idx at `version <= 94`.
pub const VERSION_EMBARK_PORT: i32 = 0x5E;
/// A leader blocks the port step at `version >= 120`.
pub const VERSION_LEADER_BLOCKS_PORT: i32 = 0x78;
/// Poison reduces movement at `version >= 115` ([`movement_points`]).
pub const VERSION_POISON_MOVEMENT: i32 = 0x73;
/// The movement cost uses the V114 body at `version <= 114`.
pub const VERSION_MOVEMENT_COST: i32 = 0x72;
/// Infiltrate cities also reject a robbed improvement above this version.
pub const VERSION_INFILTRATE_ROBBED: i32 = 0x52;

/// Path finder settings.
#[derive(Clone, Debug)]
pub struct PathSettings<'a> {
    pub version: i32,
    pub player_id: u8,
    pub allowed_terrain: &'a [i32],
    pub abilities: &'a [i32],
    pub mover: Option<&'a UnitState>,
    /// Whether a port is required to go into water.
    pub require_port: bool,
    /// Whether occupied tiles are allowed.
    pub allow_occupied: bool,
    /// Whether enemy tiles are allowed.
    pub allow_enemy: bool,
    /// Whether allied tiles are allowed.
    pub allow_allies: bool,
    /// Whether the player can embark. Not inlined.
    pub can_player_embark: bool,
}

/// Whether the path settings for a unit require a port, minus the terrain list.
///
/// `is_aquatic` is whether the unit is aquatic.
/// Port is required unless the unit is aquatic or has Fly.
pub fn require_port_for_unit(is_aquatic: bool, abilities: &[i32]) -> bool {
    if is_aquatic {
        false
    } else {
        !has_ability(abilities, ABILITY_FLY)
    }
}

/// A unit's movement points.
///
/// `data_movement` is the movement from the unit data. Boosted adds 1. Swift or Bubble adds
/// another 1. At `version >= 115`, Poisoned then does `max(movement, 2) - 1`
/// when movement is still positive, so 1 stays 1 and every larger value drops by 1.
pub fn movement_points(version: i32, data_movement: i32, unit: &UnitState) -> i32 {
    let mut movement = data_movement;
    if unit.boosted {
        movement = movement.wrapping_add(1);
    }
    if unit.swift || unit.bubble {
        movement = movement.wrapping_add(1);
    }
    if version >= VERSION_POISON_MOVEMENT && unit.poisoned && movement > 0 {
        let floored = if movement >= 3 { movement } else { 2 };
        movement = floored.wrapping_sub(1);
    }
    movement
}

/// Eight neighbors in the game's area order: outer `x` from
/// `-1` to `1`, inner `y` the same, skipping the center. Radius 1, diagonals on.
pub fn neighbor_coords(center: WorldCoordinates) -> Vec<WorldCoordinates> {
    area(center, 1)
}

/// [`neighbor_coords`] without the allocation: same coordinates, same order.
pub fn neighbors_iter(center: WorldCoordinates) -> impl Iterator<Item = WorldCoordinates> {
    const D: [(i32, i32); 8] = [(-1, -1), (-1, 0), (-1, 1), (0, -1), (0, 1), (1, -1), (1, 0), (1, 1)];
    D.into_iter().filter_map(move |(dx, dy)| {
        let x = center.x.wrapping_add(dx);
        let y = center.y.wrapping_add(dy);
        (x >= 0 && y >= 0).then_some(WorldCoordinates { x, y })
    })
}

/// Chebyshev square of `radius`, diagonals included, center excluded.
pub fn area(center: WorldCoordinates, radius: i32) -> Vec<WorldCoordinates> {
    if radius < 0 {
        return Vec::new();
    }
    let side = radius.min(8) as usize * 2 + 1;
    let mut out = Vec::with_capacity(side * side - 1);
    let mut dx = radius.wrapping_neg();
    while dx <= radius {
        let mut dy = radius.wrapping_neg();
        while dy <= radius {
            if dx != 0 || dy != 0 {
                let x = center.x.wrapping_add(dx);
                let y = center.y.wrapping_add(dy);
                if x >= 0 && y >= 0 {
                    out.push(WorldCoordinates { x, y });
                }
            }
            dy = dy.wrapping_add(1);
        }
        dx = dx.wrapping_add(1);
    }
    out
}

/// [`area`] without the allocation: same coordinates, same order.
pub fn area_iter(center: WorldCoordinates, radius: i32) -> AreaIter {
    let (lo, hi) = if radius < 0 { (1, 0) } else { (-i64::from(radius), i64::from(radius)) };
    AreaIter { center, lo, hi, dx: lo, dy: lo }
}

/// Iterator of [`area_iter`]: `dx` outer and `dy` inner, both from `lo` to `hi` (64-bit, so no step overflows).
#[derive(Clone)]
pub struct AreaIter {
    center: WorldCoordinates,
    lo: i64,
    hi: i64,
    dx: i64,
    dy: i64,
}

impl Iterator for AreaIter {
    type Item = WorldCoordinates;

    #[inline]
    fn next(&mut self) -> Option<WorldCoordinates> {
        while self.dx <= self.hi {
            if self.dy > self.hi {
                self.dx += 1;
                self.dy = self.lo;
                continue;
            }
            let (dx, dy) = (self.dx as i32, self.dy as i32);
            self.dy += 1;
            let x = self.center.x.wrapping_add(dx);
            let y = self.center.y.wrapping_add(dy);
            if (dx != 0 || dy != 0) && x >= 0 && y >= 0 {
                return Some(WorldCoordinates { x, y });
            }
        }
        None
    }
}

/// The offsets `(dx, dy)` of the square of `radius`, centre included: outer `dx` and inner `dy`, each from
/// `-radius` to `radius`; empty for a negative radius.
pub fn square_offsets(radius: i32) -> SquareOffsets {
    let (lo, hi) = if radius < 0 { (1, 0) } else { (-i64::from(radius), i64::from(radius)) };
    SquareOffsets { lo, hi, dx: lo, dy: lo }
}

/// Iterator of [`square_offsets`].
#[derive(Clone)]
pub struct SquareOffsets {
    lo: i64,
    hi: i64,
    dx: i64,
    dy: i64,
}

impl Iterator for SquareOffsets {
    type Item = (i32, i32);

    #[inline]
    fn next(&mut self) -> Option<(i32, i32)> {
        if self.dy > self.hi {
            self.dx += 1;
            self.dy = self.lo;
        }
        if self.dx > self.hi {
            return None;
        }
        let d = (self.dx as i32, self.dy as i32);
        self.dy += 1;
        Some(d)
    }
}

/// Terrain is Water or Ocean.
pub fn is_water(terrain: i32) -> bool {
    (terrain.wrapping_sub(1) as u32) < 2
}

/// Field through Ice. Wetland and Mangrove are not land.
pub fn is_land(terrain: i32) -> bool {
    (terrain.wrapping_sub(3) as u32) < 4
}

/// The grid direction of a one-step delta.
pub fn direction_of_delta(dx: i32, dy: i32) -> i32 {
    match (dx, dy) {
        (-1, -1) => 0,
        (-1, 0) => 1,
        (-1, 1) => 2,
        (0, 1) => 3,
        (1, 1) => 4,
        (1, 0) => 5,
        (1, -1) => 6,
        (0, -1) => 7,
        _ => DIRECTION_NONE,
    }
}

/// The opposite grid direction, with `DIRECTION_COUNT == 8`.
pub fn opposite_direction(direction: i32) -> i32 {
    let half = DIRECTION_COUNT / 2;
    if direction < half {
        direction.wrapping_add(half)
    } else {
        direction.wrapping_sub(half)
    }
}

/// Whether a tile is accessible to the path finder.
///
/// `is_wetland` is whether the tile is wetland: tile effect Flooded or Algae.
/// That list is not on [`Tile`]. `improvement_ability` is
/// the improvement ability test. `blocked_bridge` is
/// the non-allowed bridge direction test.
pub fn is_tile_accessible<W, A, B>(
    state: &GameState,
    tile: &Tile,
    origin: &Tile,
    settings: &PathSettings<'_>,
    is_wetland: &W,
    improvement_ability: &A,
    blocked_bridge: &B,
) -> bool
where
    W: Fn(i32) -> bool,
    A: Fn(i32, i32) -> bool,
    B: Fn(i32) -> i32,
{
    let access = AccessCtx::new(settings);
    tile_accessible_in(state, tile, origin, settings, &access, is_wetland, improvement_ability, blocked_bridge)
}

/// The inputs of [`is_tile_accessible`] that depend only on the settings, worked out once per search.
struct AccessCtx {
    fly: bool,
    /// Bit `t` for each allowed terrain id `t`, or `None` when an allowed id is outside `0..32`.
    terrain_bits: Option<u32>,
    wetland_allowed: bool,
}

impl AccessCtx {
    fn new(settings: &PathSettings<'_>) -> Self {
        let allowed = settings.allowed_terrain;
        AccessCtx {
            fly: has_ability(settings.abilities, ABILITY_FLY),
            terrain_bits: allowed
                .iter()
                .try_fold(0u32, |m, &t| u32::try_from(t).ok().filter(|&b| b < 32).map(|b| m | 1 << b)),
            wetland_allowed: allowed.contains(&TERRAIN_WETLAND),
        }
    }

    /// `settings.allowed_terrain.contains(&terrain)` for the settings this was made from.
    #[inline]
    fn allows(&self, settings: &PathSettings<'_>, terrain: i32) -> bool {
        match self.terrain_bits {
            Some(bits) => u32::try_from(terrain).ok().filter(|&b| b < 32).is_some_and(|b| bits & (1 << b) != 0),
            None => settings.allowed_terrain.contains(&terrain),
        }
    }
}

#[allow(clippy::too_many_arguments)]
#[inline]
fn tile_accessible_in<W, A, B>(
    state: &GameState,
    tile: &Tile,
    origin: &Tile,
    settings: &PathSettings<'_>,
    access: &AccessCtx,
    is_wetland: &W,
    improvement_ability: &A,
    blocked_bridge: &B,
) -> bool
where
    W: Fn(i32) -> bool,
    A: Fn(i32, i32) -> bool,
    B: Fn(i32) -> i32,
{
    let player = settings.player_id;
    let owner = tile.owner;
    // Only another player's tile needs the relation (a pure read), so it is looked up last.
    if !settings.allow_enemy
        && different_players(owner, player)
        && !(settings.allow_allies && relation_is(state, player, owner, PEACE))
    {
        return false;
    }
    if !explored(state, tile.index, player) {
        return false;
    }
    // Every test here only reads, so the terrain tests, which reject most tiles, run before the unit lookup.
    if !terrain_accessible(state, tile, origin, settings, access, is_wetland, improvement_ability, blocked_bridge) {
        return false;
    }
    if !settings.allow_occupied {
        if let Some(occupant) = visible_unit(state, tile.coordinates, player) {
            if occupant.owner != player {
                let peace_occ = occupant.owner != 0 && relation_is(state, player, occupant.owner, PEACE);
                if !peace_occ {
                    let pass = if settings.version > VERSION_OCCUPIED_HIDE {
                        has_ability(settings.abilities, ABILITY_HIDE)
                    } else {
                        settings.mover.is_some_and(|unit| unit.invisible)
                    };
                    if !pass {
                        return false;
                    }
                }
            }
        }
    }
    true
}

/// The terrain, bridge and port part of [`is_tile_accessible`].
#[allow(clippy::too_many_arguments)]
fn terrain_accessible<W, A, B>(
    state: &GameState,
    tile: &Tile,
    origin: &Tile,
    settings: &PathSettings<'_>,
    access: &AccessCtx,
    is_wetland: &W,
    improvement_ability: &A,
    blocked_bridge: &B,
) -> bool
where
    W: Fn(i32) -> bool,
    A: Fn(i32, i32) -> bool,
    B: Fn(i32) -> i32,
{
    if access.fly {
        return true;
    }

    if tile.improvement != 0 && improvement_ability(tile.improvement, IMPROVEMENT_ABILITY_BRIDGE) {
        let terrain_ok = access.allows(settings, tile.terrain);
        if terrain_ok || tile.improvement != IMPROVEMENT_BRIDGE {
            return true;
        }
        let dir = direction_of_delta(
            origin.coordinates.x.wrapping_sub(tile.coordinates.x),
            origin.coordinates.y.wrapping_sub(tile.coordinates.y),
        );
        let blocked = blocked_bridge(tile.index);
        if dir == blocked || opposite_direction(dir) == blocked {
            return false;
        }
        return true;
    }

    if is_water(tile.terrain) {
        if access.wetland_allowed && is_wetland(tile.index) {
            return true;
        }
        let origin_dry_land = is_land(origin.terrain) && !is_wetland(origin.index);
        let check_port = origin_dry_land || settings.mover.is_some();
        if check_port && settings.require_port {
            let (player, owner) = (settings.player_id, tile.owner);
            let friendly = || owner == player || (owner != 0 && relation_is(state, player, owner, PEACE));
            if !port_entry_allowed(tile, settings, friendly, improvement_ability) {
                return false;
            }
            if let Some(unit) = settings.mover {
                if unit.follower != 0 {
                    return false;
                }
                if settings.version >= VERSION_LEADER_BLOCKS_PORT && unit.leader != 0 {
                    return false;
                }
            }
        }
    }

    if tile.improvement == IMPROVEMENT_CITY {
        return true;
    }
    access.allows(settings, tile.terrain) || (access.wetland_allowed && is_wetland(tile.index))
}

/// Whether the unit may finish its move on the tile.
pub fn is_allowed_to_finish<A>(
    state: &GameState,
    tile: &Tile,
    settings: &PathSettings<'_>,
    improvement_ability: &A,
) -> bool
where
    A: Fn(i32, i32) -> bool,
{
    let _ = improvement_ability;
    if !settings.allow_occupied && visible_unit(state, tile.coordinates, settings.player_id).is_some() {
        return false;
    }
    if tile.improvement != IMPROVEMENT_CITY {
        return true;
    }
    if has_ability(settings.abilities, ABILITY_INFILTRATE) && opponent_city(tile, settings.player_id) {
        return false;
    }
    if tile.owner == 0 {
        return true;
    }
    let state_id = relation_value(state, settings.player_id, tile.owner);
    state_id != PEACE && state_id != COOLDOWN
}

/// The tiles a unit can move to.
///
/// `movement_cost(from, to)` is the movement cost for stepping from `from` onto `to`.
/// A leader yields an empty list. A follower whose [`movement_points`] is lower
/// than `max_cost` replaces `max_cost`. The enqueue limit is `max_cost * 10`
/// in 32-bit wrapping arithmetic. At most `SEARCH_LIMIT` coordinates are dequeued.
pub fn get_move_options<W, A, B, C>(
    state: &GameState,
    start: WorldCoordinates,
    max_cost: i32,
    unit: &UnitState,
    settings: &PathSettings<'_>,
    follower_data_movement: Option<i32>,
    is_wetland: W,
    improvement_ability: A,
    blocked_bridge: B,
    movement_cost: C,
) -> Vec<WorldCoordinates>
where
    W: Fn(i32) -> bool,
    A: Fn(i32, i32) -> bool,
    B: Fn(i32) -> i32,
    C: FnMut(WorldCoordinates, WorldCoordinates) -> i32,
{
    get_move_options_min_step(
        state,
        start,
        max_cost,
        unit,
        settings,
        follower_data_movement,
        is_wetland,
        improvement_ability,
        blocked_bridge,
        movement_cost,
        None,
    )
}

/// The smallest cost [`get_movement_cost`] returns (a road step).
pub const MIN_MOVEMENT_COST: i32 = COST_ROAD;

/// [`get_move_options`] for a `movement_cost` that never returns less than `min_step`: a step onto a tile already
/// reached for at most the current cost plus `min_step` cannot lower its cost, so it is skipped unpriced. The result
/// is the same.
#[allow(clippy::too_many_arguments)]
pub fn get_move_options_min_step<W, A, B, C>(
    state: &GameState,
    start: WorldCoordinates,
    max_cost: i32,
    unit: &UnitState,
    settings: &PathSettings<'_>,
    follower_data_movement: Option<i32>,
    is_wetland: W,
    improvement_ability: A,
    blocked_bridge: B,
    mut movement_cost: C,
    min_step: Option<i32>,
) -> Vec<WorldCoordinates>
where
    W: Fn(i32) -> bool,
    A: Fn(i32, i32) -> bool,
    B: Fn(i32) -> i32,
    C: FnMut(WorldCoordinates, WorldCoordinates) -> i32,
{
    if unit.leader != 0 {
        return Vec::new();
    }
    let max_cost = capped_max_cost(state, unit, settings.version, max_cost, follower_data_movement);
    let limit = max_cost.wrapping_mul(10);
    let Some(start_index) = coord_index(&state.map, start) else {
        return Vec::new();
    };
    let access = AccessCtx::new(settings);
    let (mut cost, mut queue) = MOVE_SCRATCH.with(|s| std::mem::take(&mut *s.borrow_mut()));
    cost.clear();
    cost.resize(state.map.tiles.len(), None);
    cost[start_index] = Some(0);

    queue.clear();
    queue.push(start);
    let mut head = 0usize;
    let mut result = Vec::with_capacity(16);
    let mut steps = SEARCH_LIMIT;
    while head < queue.len() {
        if steps == 0 {
            break;
        }
        steps -= 1;
        let current = queue[head];
        head += 1;
        let Some(current_index) = coord_index(&state.map, current) else {
            continue;
        };
        let current_cost = cost[current_index].unwrap_or(0);
        let origin = &state.map.tiles[current_index];
        for next in neighbors_iter(current) {
            let Some(next_index) = coord_index(&state.map, next) else {
                continue;
            };
            let next_tile = &state.map.tiles[next_index];
            if let (Some(min), Some(existing)) = (min_step, cost[next_index]) {
                if existing <= current_cost.saturating_add(min) {
                    continue;
                }
            }
            if !tile_accessible_in(
                state,
                next_tile,
                origin,
                settings,
                &access,
                &is_wetland,
                &improvement_ability,
                &blocked_bridge,
            ) {
                continue;
            }
            let step = movement_cost(current, next);
            debug_assert!(min_step.is_none_or(|m| step >= m), "step cost {step} below the promised minimum");
            let new_cost = step.wrapping_add(current_cost);
            let seen = cost[next_index];
            if seen.is_some_and(|existing| existing <= new_cost) {
                continue;
            }
            let first = seen.is_none();
            cost[next_index] = Some(new_cost);
            if first && is_allowed_to_finish(state, next_tile, settings, &improvement_ability) {
                result.push(next);
            }
            if new_cost < limit {
                queue.push(next);
            }
        }
    }
    MOVE_SCRATCH.with(|s| *s.borrow_mut() = (cost, queue));
    result
}

thread_local! {
    /// The cost table and queue of [`get_move_options`], kept between calls; a nested call takes empty ones.
    static MOVE_SCRATCH: std::cell::RefCell<(Vec<Option<i32>>, Vec<WorldCoordinates>)> = const { std::cell::RefCell::new((Vec::new(), Vec::new())) };
}

/// The path from one tile to another.
///
/// Equal coordinates return a one-element list before the finish check. A
/// destination that fails [`is_allowed_to_finish`] returns `None` (the game
/// returns null). The reconstructed list is destination-first.
/// `movement_cost` is the same movement cost input as [`get_move_options`].
pub fn get_path<W, A, B, C>(
    state: &GameState,
    start: WorldCoordinates,
    destination: WorldCoordinates,
    max_cost: i32,
    settings: &PathSettings<'_>,
    is_wetland: W,
    improvement_ability: A,
    blocked_bridge: B,
    mut movement_cost: C,
) -> Option<Vec<WorldCoordinates>>
where
    W: Fn(i32) -> bool,
    A: Fn(i32, i32) -> bool,
    B: Fn(i32) -> i32,
    C: FnMut(WorldCoordinates, WorldCoordinates) -> i32,
{
    if start == destination {
        return Some(vec![start]);
    }
    let dest_index = coord_index(&state.map, destination)?;
    let dest_tile = state.map.tiles[dest_index];
    if !is_allowed_to_finish(state, &dest_tile, settings, &improvement_ability) {
        return None;
    }

    let limit = (max_cost.wrapping_mul(10)) as f32;
    let mut scratch = PATH_SCRATCH.with(|s| std::mem::take(&mut *s.borrow_mut()));
    let found = path_search(
        state,
        start,
        destination,
        limit,
        settings,
        &is_wetland,
        &improvement_ability,
        &blocked_bridge,
        &mut movement_cost,
        &mut scratch,
    );
    scratch.clean();
    PATH_SCRATCH.with(|s| *s.borrow_mut() = scratch);
    found
}

/// Buffers of [`get_path`], kept per thread between searches. `cost` and `came_from` hold the default for every tile
/// except those listed in `touched`, which [`PathScratch::clean`] resets.
#[derive(Default)]
struct PathScratch {
    cost: Vec<i32>,
    came_from: Vec<Option<WorldCoordinates>>,
    touched: Vec<usize>,
    /// The open list in descending priority, entries of equal priority newest first, so the next node is last.
    heap: Vec<HeapNode>,
}

impl PathScratch {
    fn clean(&mut self) {
        for &i in &self.touched {
            self.cost[i] = 0;
            self.came_from[i] = None;
        }
        self.touched.clear();
        self.heap.clear();
    }
}

thread_local! {
    static PATH_SCRATCH: std::cell::RefCell<PathScratch> = std::cell::RefCell::new(PathScratch::default());
}

#[allow(clippy::too_many_arguments)]
fn path_search<W, A, B, C>(
    state: &GameState,
    start: WorldCoordinates,
    destination: WorldCoordinates,
    limit: f32,
    settings: &PathSettings<'_>,
    is_wetland: &W,
    improvement_ability: &A,
    blocked_bridge: &B,
    movement_cost: &mut C,
    scratch: &mut PathScratch,
) -> Option<Vec<WorldCoordinates>>
where
    W: Fn(i32) -> bool,
    A: Fn(i32, i32) -> bool,
    B: Fn(i32) -> i32,
    C: FnMut(WorldCoordinates, WorldCoordinates) -> i32,
{
    let n = state.map.tiles.len();
    if scratch.cost.len() != n {
        scratch.cost = vec![0; n];
        scratch.came_from = vec![None; n];
    }
    let PathScratch { cost, came_from, touched, heap } = scratch;
    let access = AccessCtx::new(settings);
    heap.push(HeapNode { coord: start, priority: manhattan(start, destination) as f32 });
    let mut steps = SEARCH_LIMIT;
    while let Some(node) = heap.pop() {
        if steps <= 0 {
            break;
        }
        steps -= 1;
        if node.coord == destination {
            return reconstruct(start, destination, came_from, &state.map);
        }
        let Some(current_index) = coord_index(&state.map, node.coord) else {
            continue;
        };
        let current_cost = cost[current_index] as f32;
        if current_cost >= limit {
            continue;
        }
        let origin = state.map.tiles[current_index];
        for next in neighbors_iter(node.coord) {
            let Some(next_index) = coord_index(&state.map, next) else {
                continue;
            };
            let next_tile = state.map.tiles[next_index];
            if !tile_accessible_in(
                state,
                &next_tile,
                &origin,
                settings,
                &access,
                is_wetland,
                improvement_ability,
                blocked_bridge,
            ) {
                continue;
            }
            let step = movement_cost(node.coord, next);
            let new_cost = step.wrapping_add(cost[current_index]);
            let existing = cost[next_index];
            if existing > 0 && new_cost >= existing {
                continue;
            }
            if existing == 0 && came_from[next_index].is_none() {
                touched.push(next_index);
            }
            cost[next_index] = new_cost;
            came_from[next_index] = Some(node.coord);
            let priority = euclidean(next, destination) + (new_cost as f32);
            debug_assert!(priority.is_finite(), "path priority {priority}");
            // Behind every entry of higher priority, in front of the older ones of equal priority.
            let at = heap.iter().position(|e| e.priority <= priority).unwrap_or(heap.len());
            heap.insert(at, HeapNode { coord: next, priority });
        }
    }
    None
}

/// Whether a tile is being captured: it is a city with an owner, and the unit standing on it belongs to
/// someone else (an invader that has moved in and not yet captured it). The tile's own owner field decides, not
/// the city record, so a city whose border owner changed counts by the tile. The argument to hand to
/// [`attack_options`] as `being_captured`.
pub fn tile_being_captured(state: &GameState, tile_index: i32) -> bool {
    let Some(tile) = usize::try_from(tile_index).ok().and_then(|i| state.map.tiles.get(i)) else {
        return false;
    };
    if tile.improvement != IMPROVEMENT_CITY || tile.owner == 0 {
        return false;
    }
    state.unit_at(tile.coordinates).is_some_and(|unit| unit.owner != tile.owner)
}

/// The tiles a unit can attack.
///
/// A leader, or raw attack 0 without Convert or Infiltrate, yields nothing.
/// `being_captured` is whether the tile is being captured. `floodable` is
/// whether the tile is floodable. Neither is stored on [`Tile`].
pub fn attack_options<C, F>(
    state: &GameState,
    unit: &UnitState,
    range: i32,
    ignore_diplomacy: bool,
    raw_attack: i32,
    abilities: &[i32],
    include_hidden: bool,
    being_captured: C,
    floodable: F,
) -> Vec<WorldCoordinates>
where
    C: Fn(i32) -> bool,
    F: Fn(i32) -> bool,
{
    if unit.leader != 0 {
        return Vec::new();
    }
    let scaled = raw_attack.wrapping_mul(10);
    if scaled == 0 && !has_ability(abilities, ABILITY_CONVERT) && !has_ability(abilities, ABILITY_INFILTRATE) {
        return Vec::new();
    }
    attack_options_at(
        state,
        unit.owner,
        unit.coordinates,
        range,
        include_hidden,
        unit,
        abilities,
        ignore_diplomacy,
        being_captured,
        floodable,
    )
}

pub fn attack_options_at<C, F>(
    state: &GameState,
    player_id: u8,
    position: WorldCoordinates,
    range: i32,
    include_hidden: bool,
    unit: &UnitState,
    abilities: &[i32],
    ignore_diplomacy: bool,
    being_captured: C,
    floodable: F,
) -> Vec<WorldCoordinates>
where
    C: Fn(i32) -> bool,
    F: Fn(i32) -> bool,
{
    let _ = unit;
    let mut found = Vec::new();
    let infiltrate = has_ability(abilities, ABILITY_INFILTRATE);
    let drench = has_ability(abilities, ABILITY_DRENCH);
    for candidate in area_iter(position, range) {
        let Some(index) = coord_index(&state.map, candidate) else {
            continue;
        };
        // Without Infiltrate or Drench only a tile with a unit can be a target; every test here only reads.
        if !infiltrate && !drench && state.unit_index_at(candidate).is_none() {
            continue;
        }
        let tile = &state.map.tiles[index];
        if !include_hidden && !explored(state, tile.index, player_id) {
            continue;
        }
        if infiltrate {
            if infiltrate_city(state, tile, player_id, ignore_diplomacy, &being_captured) {
                found.push(candidate);
            }
            continue;
        }
        if drench && floodable(tile.index) {
            found.push(candidate);
            continue;
        }
        let Some(defender) = visible_unit(state, candidate, player_id) else {
            continue;
        };
        if defender.owner == player_id {
            continue;
        }
        if !ignore_diplomacy && blocked_by_diplomacy(state, player_id, defender.owner) {
            continue;
        }
        found.push(candidate);
    }
    found
}

fn infiltrate_city<C>(state: &GameState, tile: &Tile, player_id: u8, ignore_diplomacy: bool, being_captured: &C) -> bool
where
    C: Fn(i32) -> bool,
{
    if tile.improvement != IMPROVEMENT_CITY || tile.owner == 0 || tile.owner == player_id {
        return false;
    }
    if being_captured(tile.index) {
        return false;
    }
    if state.version > VERSION_INFILTRATE_ROBBED {
        let robbed = state
            .improvements
            .iter()
            .find(|improvement| improvement.tile == tile.index)
            .is_some_and(|improvement| improvement.robbed);
        if robbed {
            return false;
        }
    }
    ignore_diplomacy || !blocked_by_diplomacy(state, player_id, tile.owner)
}

fn blocked_by_diplomacy(state: &GameState, player: u8, other: u8) -> bool {
    if other == 0 {
        return false;
    }
    let value = relation_value(state, player, other);
    value == PEACE || value == COOLDOWN
}

fn port_entry_allowed<A>(
    tile: &Tile,
    settings: &PathSettings<'_>,
    friendly: impl Fn() -> bool,
    improvement_ability: &A,
) -> bool
where
    A: Fn(i32, i32) -> bool,
{
    let embark = has_embark(settings.version, tile.improvement, improvement_ability);
    if settings.version > VERSION_EMBARK_GATE {
        friendly() && embark && settings.can_player_embark
    } else {
        embark && tile.owner == settings.player_id
    }
}

/// Whether the improvement lets units embark.
fn has_embark<A>(version: i32, improvement: i32, improvement_ability: &A) -> bool
where
    A: Fn(i32, i32) -> bool,
{
    if improvement == 0 {
        return false;
    }
    if version <= VERSION_EMBARK_PORT {
        improvement == IMPROVEMENT_PORT
    } else {
        improvement_ability(improvement, IMPROVEMENT_ABILITY_EMBARK)
    }
}

fn capped_max_cost(
    state: &GameState,
    unit: &UnitState,
    version: i32,
    max_cost: i32,
    follower_data_movement: Option<i32>,
) -> i32 {
    if unit.follower == 0 {
        return max_cost;
    }
    let Some(data_movement) = follower_data_movement else {
        return max_cost;
    };
    let Some(follower) = state.units.iter().find(|other| other.id == unit.follower) else {
        return max_cost;
    };
    let movement = movement_points(version, data_movement, follower);
    if movement < max_cost {
        movement
    } else {
        max_cost
    }
}

/// Inputs the movement cost reads that are not fields on [`Tile`].
///
/// `will_embark` is whether the unit would embark (non-zero result).
/// `dest_has_embark` is whether the destination has an embark improvement.
/// `has_road_to` is whether the tile has a road to the destination.
/// `has_glide` is whether the player has the Glide ability.
/// `improvement_slows` is improvement ability Slow (19).
/// `has_tech_32` is whether the player's tech list contains `0x20`.
/// `water_bound` / `land_bound` are whether the unit is water-bound or land-bound.
/// `zoc`, when present, is [`zoc_map`] for this state, player and version, read in place of the neighbour scan.
/// Roads, routes, Flooded, and Algae are not stored on [`Tile`].
#[derive(Clone, Debug)]
pub struct MovementCostQuery<'a> {
    pub state: &'a GameState,
    pub player_id: u8,
    pub from: WorldCoordinates,
    pub dest: WorldCoordinates,
    pub from_terrain: i32,
    pub dest_terrain: i32,
    pub from_improvement: i32,
    pub dest_improvement: i32,
    pub abilities: &'a [i32],
    pub unit: Option<&'a UnitState>,
    pub require_port: bool,
    pub allow_occupied: bool,
    pub follow_transport: bool,
    pub will_embark: bool,
    pub dest_has_embark: bool,
    pub dest_has_road: bool,
    pub from_has_road: bool,
    pub dest_has_route: bool,
    pub dest_flooded: bool,
    pub dest_algae: bool,
    pub has_road_to: bool,
    pub has_glide: bool,
    pub improvement_slows: bool,
    pub has_tech_32: bool,
    pub water_bound: bool,
    pub land_bound: bool,
    pub zoc: Option<&'a [bool]>,
    /// [`ability_bits`] of `abilities`, when the caller has it.
    pub ability_bits: Option<u64>,
}

/// `abilities` as a bit set (bit `id` per ability), or `None` when an id does not fit in 64 bits.
pub fn ability_bits(abilities: &[i32]) -> Option<u64> {
    abilities.iter().try_fold(0u64, |m, &a| u32::try_from(a).ok().filter(|&b| b < 64).map(|b| m | 1u64 << b))
}

impl MovementCostQuery<'_> {
    #[inline]
    fn has(&self, ability: i32) -> bool {
        match self.ability_bits {
            Some(bits) => u32::try_from(ability).ok().filter(|&b| b < 64).is_some_and(|b| bits & (1u64 << b) != 0),
            None => has_ability(self.abilities, ability),
        }
    }
}

const ABILITY_SNEAK: i32 = 4;
const ABILITY_SWIM: i32 = 10;
const ABILITY_CARRY: i32 = 11;
const ABILITY_SKATE: i32 = 21;
const ABILITY_CREEP: i32 = 23;
const COST_BLOCKED: i32 = 0x3E8;
const COST_STEP: i32 = 10;
const COST_ROAD: i32 = 5;
const COST_SKATE_OFF_ICE: i32 = 0x1E;
const COST_OLD_SKATE: i32 = 0x14;
const COST_GLIDE: i32 = 9;
const COST_SWIM_V114: i32 = 0x1E;
const COST_SWIM_OLD: i32 = 0x14;
const TERRAIN_ICE_TILE: i32 = 6;
const TERRAIN_MOUNTAIN: i32 = 4;
const VERSION_EMBARK_COST: i32 = 0x68;
const VERSION_EMBARK_OLD: i32 = 0x53;
const VERSION_SKATE_COST: i32 = 0x71;

/// The movement cost of a tile.
///
/// Version `<= 114` is `movement_cost_v114`. Later versions are
/// `movement_cost_default`. This does not replace the
/// `movement_cost` callback on [`get_move_options`] or [`get_path`].
pub fn get_movement_cost(query: &MovementCostQuery<'_>) -> i32 {
    if query.state.version <= VERSION_MOVEMENT_COST {
        movement_cost_v114(query)
    } else {
        movement_cost_default(query)
    }
}

fn movement_cost_v114(query: &MovementCostQuery<'_>) -> i32 {
    let version = query.state.version;
    if embark_blocks_v114(query, version) {
        return COST_BLOCKED;
    }
    if carry_blocks(query, version) {
        return COST_BLOCKED;
    }
    if zoc_blocks(query, false) {
        return COST_BLOCKED;
    }
    if query.unit.is_none() {
        return idle_step(query);
    }
    if query.has(ABILITY_FLY) {
        return COST_STEP;
    }
    if query.has(ABILITY_SKATE) && version > VERSION_SKATE_COST {
        return if query.dest_terrain == TERRAIN_ICE_TILE { COST_STEP } else { COST_SKATE_OFF_ICE };
    }
    if version <= VERSION_SKATE_COST && (query.has(ABILITY_SKATE) || query.has_tech_32) {
        if query.dest_terrain == TERRAIN_ICE_TILE && old_ice_chain(query) {
            return COST_ROAD;
        }
        if query.has(ABILITY_SKATE) {
            return COST_OLD_SKATE;
        }
    }
    if version > VERSION_EMBARK_COST {
        if !is_water(query.dest_terrain) && !query.dest_flooded && query.has(ABILITY_SWIM) {
            return COST_SWIM_V114;
        }
        if query.has(ABILITY_CREEP) {
            return COST_STEP;
        }
    } else {
        if query.has(ABILITY_CREEP) {
            return COST_STEP;
        }
        if !is_water(query.dest_terrain) && !query.dest_flooded && query.has(ABILITY_SWIM) {
            return COST_SWIM_OLD;
        }
    }
    finish_step(query)
}

fn movement_cost_default(query: &MovementCostQuery<'_>) -> i32 {
    if query.unit.is_none() {
        return idle_step(query);
    }
    if query.will_embark {
        return COST_BLOCKED;
    }
    if zoc_blocks(query, true) {
        return COST_BLOCKED;
    }
    if query.has(ABILITY_FLY) {
        return COST_STEP;
    }
    if query.has(ABILITY_SKATE) {
        return if query.dest_terrain == TERRAIN_ICE_TILE { COST_STEP } else { COST_BLOCKED };
    }
    if query.dest_terrain == TERRAIN_ICE_TILE && query.has_glide {
        return COST_GLIDE;
    }
    if !is_water(query.dest_terrain) && !query.dest_flooded && query.has(ABILITY_SWIM) {
        return COST_BLOCKED;
    }
    if query.dest_terrain != TERRAIN_MOUNTAIN && query.has(ABILITY_CREEP) {
        return COST_STEP;
    }
    finish_step(query)
}

fn embark_blocks_v114(query: &MovementCostQuery<'_>, version: i32) -> bool {
    if version > VERSION_EMBARK_COST {
        return query.will_embark;
    }
    if version > VERSION_EMBARK_OLD {
        return query.require_port && is_water(query.dest_terrain) && query.dest_has_embark && query.unit.is_some();
    }
    is_water(query.from_terrain) != is_water(query.dest_terrain) && query.require_port && query.dest_has_embark
}

fn carry_blocks(query: &MovementCostQuery<'_>, version: i32) -> bool {
    let leaving = !is_water(query.dest_terrain) && query.unit.is_some() && query.has(ABILITY_CARRY);
    if version <= VERSION_EMBARK_OLD {
        is_water(query.from_terrain) && leaving
    } else {
        leaving
    }
}

fn zoc_blocks(query: &MovementCostQuery<'_>, leaders_exempt: bool) -> bool {
    let Some(unit) = query.unit else {
        return false;
    };
    if query.allow_occupied || query.has(ABILITY_SNEAK) || query.has(ABILITY_HIDE) || unit.invisible {
        return false;
    }
    if let (Some(zoc), Some(i)) = (query.zoc, coord_index(&query.state.map, query.dest)) {
        return zoc[i];
    }
    neighbors_iter(query.dest).any(|coord| {
        query.state.unit_at(coord).is_some_and(|other| exerts_zoc(query.state, query.player_id, other, leaders_exempt))
    })
}

fn exerts_zoc(state: &GameState, player: u8, other: &UnitState, leaders_exempt: bool) -> bool {
    other.owner != player
        && !other.invisible
        && !relation_is(state, player, other.owner, PEACE)
        && (!leaders_exempt || other.leader == 0)
}

/// Per tile index, the neighbour scan of `zoc_blocks` for a step onto that tile: whether the first unit on some
/// neighbouring coordinate exerts a zone of control on `player`. `leaders_exempt` is true after version 114.
pub fn zoc_map(state: &GameState, player: u8, leaders_exempt: bool) -> Vec<bool> {
    let map = &state.map;
    let mut out = vec![false; map.tiles.len()];
    let mut taken = vec![false; map.tiles.len()];
    let mut taken_off_map: Vec<WorldCoordinates> = Vec::new();
    for unit in &state.units {
        let c = unit.coordinates;
        // The scan only looks at non-negative coordinates, and only at the first unit on each.
        if c.x < 0 || c.y < 0 {
            continue;
        }
        let first = match coord_index(map, c) {
            Some(i) => !std::mem::replace(&mut taken[i], true),
            None if taken_off_map.contains(&c) => false,
            None => {
                taken_off_map.push(c);
                true
            }
        };
        if !first || !exerts_zoc(state, player, unit, leaders_exempt) {
            continue;
        }
        for n in neighbors_iter(c) {
            if let Some(k) = coord_index(map, n) {
                out[k] = true;
            }
        }
    }
    out
}

fn old_ice_chain(query: &MovementCostQuery<'_>) -> bool {
    query.from_terrain == TERRAIN_ICE_TILE
        || query.from_has_road
        || query.from_improvement == IMPROVEMENT_CITY
        || query.from_improvement == IMPROVEMENT_BRIDGE
}

fn idle_step(query: &MovementCostQuery<'_>) -> i32 {
    if query.follow_transport
        && (query.dest_has_road
            || query.dest_improvement == IMPROVEMENT_CITY
            || query.dest_improvement == IMPROVEMENT_BRIDGE
            || query.dest_has_route)
    {
        COST_ROAD
    } else {
        COST_STEP
    }
}

fn finish_step(query: &MovementCostQuery<'_>) -> i32 {
    let skate = query.has(ABILITY_SKATE);
    let creep = query.has(ABILITY_CREEP);
    let swim = query.has(ABILITY_SWIM);
    if query.has_road_to && query.dest_terrain != TERRAIN_ICE_TILE && !skate && !creep && !swim {
        return COST_ROAD;
    }
    if query.improvement_slows {
        return COST_BLOCKED;
    }
    if query.dest_terrain == TERRAIN_MOUNTAIN || query.dest_terrain == 5 {
        return COST_BLOCKED;
    }
    if query.dest_flooded && query.water_bound {
        return COST_BLOCKED;
    }
    if query.dest_algae && query.land_bound {
        return COST_BLOCKED;
    }
    COST_STEP
}

fn opponent_city(tile: &Tile, player: u8) -> bool {
    tile.improvement == IMPROVEMENT_CITY && tile.owner != 0 && tile.owner != player
}

fn different_players(left: u8, right: u8) -> bool {
    left != right && left != 0 && right != 0
}

fn has_ability(abilities: &[i32], ability: i32) -> bool {
    abilities.contains(&ability)
}

#[allow(clippy::manual_contains)] // `iter().any` is deliberate here, see the note inside
pub(crate) fn explored(state: &GameState, tile: i32, player: u8) -> bool {
    if player == 255 {
        return true;
    }
    // `iter().any` rather than `contains`: the lists hold a few ids, and `contains::<u8>` goes through memchr.
    if let Some(entry) = usize::try_from(tile).ok().and_then(|i| state.fog.get(i)) {
        if entry.tile == tile {
            return entry.explorers.iter().any(|&p| p == player);
        }
    }
    state.fog.iter().find(|entry| entry.tile == tile).is_some_and(|entry| entry.explorers.iter().any(|&p| p == player))
}

fn relation_is(state: &GameState, player: u8, other: u8, expected: i32) -> bool {
    relation_value(state, player, other) == expected
}

fn relation_value(state: &GameState, player: u8, other: u8) -> i32 {
    if other == 0 {
        return 0;
    }
    state
        .diplomacy
        .iter()
        .find(|row| row.player == player)
        .and_then(|row| row.relations.iter().find(|link| link.other == other))
        .map(|link| link.relation.state as i32)
        .unwrap_or(0)
}

/// The unit on a tile as seen by a viewer (hidden units excluded), plus
/// the invisibility test.
fn visible_unit(state: &GameState, coords: WorldCoordinates, viewer: u8) -> Option<&UnitState> {
    let unit = state.unit_at(coords)?;
    if !unit.invisible {
        return Some(unit);
    }
    if unit.owner == viewer {
        return Some(unit);
    }
    if relation_is(state, viewer, unit.owner, PEACE) {
        return Some(unit);
    }
    None
}

pub(crate) fn coord_index(map: &crate::Map, coords: WorldCoordinates) -> Option<usize> {
    if coords.x < 0 || coords.y < 0 {
        return None;
    }
    let width = i32::from(map.width);
    let height = i32::from(map.height);
    if coords.x >= width || coords.y >= height {
        return None;
    }
    let index = coords.y.wrapping_mul(width).wrapping_add(coords.x);
    let index = usize::try_from(index).ok()?;
    if index < map.tiles.len() {
        Some(index)
    } else {
        None
    }
}

fn manhattan(a: WorldCoordinates, b: WorldCoordinates) -> i32 {
    abs_delta(a.x.wrapping_sub(b.x)).wrapping_add(abs_delta(a.y.wrapping_sub(b.y)))
}

fn abs_delta(value: i32) -> i32 {
    let negated = value.wrapping_neg();
    if value < 0 {
        negated
    } else {
        value
    }
}

fn euclidean(a: WorldCoordinates, b: WorldCoordinates) -> f32 {
    let dx = a.x.wrapping_sub(b.x);
    let dy = a.y.wrapping_sub(b.y);
    let square = dx.wrapping_mul(dx).wrapping_add(dy.wrapping_mul(dy));
    (square as f64).sqrt() as f32
}

struct HeapNode {
    coord: WorldCoordinates,
    priority: f32,
}

fn reconstruct(
    start: WorldCoordinates,
    destination: WorldCoordinates,
    came_from: &[Option<WorldCoordinates>],
    map: &crate::Map,
) -> Option<Vec<WorldCoordinates>> {
    let mut path = vec![destination];
    let mut current = destination;
    for _ in 0..came_from.len() {
        let index = coord_index(map, current)?;
        current = came_from.get(index).copied().flatten()?;
        path.push(current);
        if current == start {
            return Some(path);
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{DiplomacyRelationState, Map, PlayerState, TileExplorers, UNIT_CTOR_HEALTH};

    const TERRAIN_WATER: i32 = 1;

    fn never_wet(index: i32) -> bool {
        let _ = index;
        false
    }

    #[test]
    fn area_iter_matches_area() {
        for radius in -2..=4 {
            for (x, y) in [(0, 0), (1, 2), (5, 5), (-3, 1), (20, 0)] {
                let c = WorldCoordinates { x, y };
                assert_eq!(area_iter(c, radius).collect::<Vec<_>>(), area(c, radius), "radius {radius} at {c:?}");
            }
        }
    }

    fn no_ability(_kind: i32, _ability: i32) -> bool {
        false
    }

    fn no_bridge(_index: i32) -> i32 {
        DIRECTION_NONE
    }

    fn flat_cost(_from: WorldCoordinates, _to: WorldCoordinates) -> i32 {
        10
    }

    fn explored_map(width: u16, height: u16, player: u8) -> GameState {
        let mut state = GameState::new(123, 1);
        state.map = Map::new(width, height);
        for tile in &mut state.map.tiles {
            tile.terrain = 3;
            state.fog.push(TileExplorers { tile: tile.index, explorers: vec![player].into() });
        }
        state.players.push(PlayerState {
            id: player,
            handicap: 1,
            currency: 0,
            score: 0,
            tribe: 1,
            auto_play: false,
            start_tile: WorldCoordinates { x: 0, y: 0 },
        });
        state
    }

    fn mover(owner: u8) -> UnitState {
        UnitState {
            id: 1,
            owner,
            type_idx: 2,
            coordinates: WorldCoordinates { x: 2, y: 2 },
            health: UNIT_CTOR_HEALTH,
            ..UnitState::default()
        }
    }

    fn settings<'a>(
        state_version: i32,
        unit: &'a UnitState,
        abilities: &'a [i32],
        terrain: &'a [i32],
    ) -> PathSettings<'a> {
        PathSettings {
            version: state_version,
            player_id: unit.owner,
            allowed_terrain: terrain,
            abilities,
            mover: Some(unit),
            require_port: require_port_for_unit(false, abilities),
            allow_occupied: false,
            allow_enemy: true,
            allow_allies: false,
            can_player_embark: false,
        }
    }

    #[test]
    fn move_options_are_the_eight_neighbors_in_area_order() {
        let state = explored_map(5, 5, 1);
        let unit = mover(1);
        let terrain = [3];
        let options = get_move_options(
            &state,
            unit.coordinates,
            1,
            &unit,
            &settings(123, &unit, &[], &terrain),
            None,
            never_wet,
            no_ability,
            no_bridge,
            flat_cost,
        );
        assert_eq!(
            options,
            vec![
                WorldCoordinates { x: 1, y: 1 },
                WorldCoordinates { x: 1, y: 2 },
                WorldCoordinates { x: 1, y: 3 },
                WorldCoordinates { x: 2, y: 1 },
                WorldCoordinates { x: 2, y: 3 },
                WorldCoordinates { x: 3, y: 1 },
                WorldCoordinates { x: 3, y: 2 },
                WorldCoordinates { x: 3, y: 3 },
            ]
        );
    }

    #[test]
    fn path_of_one_step_is_destination_then_start() {
        let state = explored_map(3, 1, 1);
        let unit = mover(1);
        let terrain = [3];
        let path = get_path(
            &state,
            WorldCoordinates { x: 0, y: 0 },
            WorldCoordinates { x: 1, y: 0 },
            1,
            &settings(123, &unit, &[], &terrain),
            never_wet,
            no_ability,
            no_bridge,
            flat_cost,
        );
        assert_eq!(path, Some(vec![WorldCoordinates { x: 1, y: 0 }, WorldCoordinates { x: 0, y: 0 },]));
        assert_eq!(
            get_path(
                &state,
                WorldCoordinates { x: 0, y: 0 },
                WorldCoordinates { x: 0, y: 0 },
                0,
                &settings(123, &unit, &[], &terrain),
                never_wet,
                no_ability,
                no_bridge,
                flat_cost,
            ),
            Some(vec![WorldCoordinates { x: 0, y: 0 }])
        );
    }

    #[test]
    fn occupied_enemy_uses_invisible_then_hide() {
        let mut state = explored_map(3, 1, 1);
        state.units.push(UnitState {
            id: 9,
            owner: 2,
            coordinates: WorldCoordinates { x: 1, y: 0 },
            ..UnitState::default()
        });
        let mut unit = mover(1);
        unit.coordinates = WorldCoordinates { x: 0, y: 0 };
        let terrain = [3];
        let mut blocked = |version: i32, abilities: &[i32], invisible: bool| {
            unit.invisible = invisible;
            let options = get_move_options(
                &state,
                unit.coordinates,
                2,
                &unit,
                &settings(version, &unit, abilities, &terrain),
                None,
                never_wet,
                no_ability,
                no_bridge,
                flat_cost,
            );
            // The occupied tile is not a finish tile. (2, 0) is reached only
            // when the version gate allows stepping through the enemy.
            options.contains(&WorldCoordinates { x: 2, y: 0 })
        };
        assert!(!blocked(VERSION_OCCUPIED_HIDE, &[], false));
        assert!(blocked(VERSION_OCCUPIED_HIDE, &[], true));
        assert!(blocked(VERSION_OCCUPIED_HIDE + 1, &[ABILITY_HIDE], false));
        assert!(!blocked(VERSION_OCCUPIED_HIDE + 1, &[], true));
    }

    #[test]
    fn port_gate_and_leader_follow_the_version_splits() {
        let mut state = explored_map(2, 1, 1);
        state.map.tiles[0].terrain = 3;
        state.map.tiles[1].terrain = TERRAIN_WATER;
        state.map.tiles[1].improvement = IMPROVEMENT_PORT;
        state.map.tiles[1].owner = 1;
        let origin = state.map.tiles[0];
        let water = state.map.tiles[1];
        let mut unit = mover(1);
        let water_terrain = [TERRAIN_WATER];
        let embark = |kind: i32, ability: i32| kind == IMPROVEMENT_PORT && ability == IMPROVEMENT_ABILITY_EMBARK;

        let mut old = settings(VERSION_EMBARK_GATE, &unit, &[], &water_terrain);
        old.require_port = true;
        old.can_player_embark = false;
        assert!(is_tile_accessible(&state, &water, &origin, &old, &never_wet, &embark, &no_bridge));

        let mut newer = settings(VERSION_EMBARK_GATE + 1, &unit, &[], &water_terrain);
        newer.require_port = true;
        newer.can_player_embark = false;
        assert!(!is_tile_accessible(&state, &water, &origin, &newer, &never_wet, &embark, &no_bridge));
        newer.can_player_embark = true;
        assert!(is_tile_accessible(&state, &water, &origin, &newer, &never_wet, &embark, &no_bridge));

        unit.leader = 4;
        let mut before = settings(VERSION_LEADER_BLOCKS_PORT - 1, &unit, &[], &water_terrain);
        before.require_port = true;
        before.can_player_embark = true;
        assert!(is_tile_accessible(&state, &water, &origin, &before, &never_wet, &embark, &no_bridge));
        let mut after = settings(VERSION_LEADER_BLOCKS_PORT, &unit, &[], &water_terrain);
        after.require_port = true;
        after.can_player_embark = true;
        assert!(!is_tile_accessible(&state, &water, &origin, &after, &never_wet, &embark, &no_bridge));
    }

    #[test]
    fn poison_reduces_movement_only_from_version_115() {
        let mut unit = mover(1);
        unit.poisoned = true;
        assert_eq!(movement_points(VERSION_POISON_MOVEMENT - 1, 3, &unit), 3);
        assert_eq!(movement_points(VERSION_POISON_MOVEMENT, 3, &unit), 2);
        assert_eq!(movement_points(VERSION_POISON_MOVEMENT, 1, &unit), 1);
        unit.boosted = true;
        assert_eq!(movement_points(VERSION_POISON_MOVEMENT, 1, &unit), 1);
    }

    #[test]
    fn attack_scan_skips_peace_and_empty_attack_and_checks_robbed() {
        let mut state = explored_map(3, 1, 1);
        state.version = VERSION_INFILTRATE_ROBBED + 1;
        state.units.push(UnitState {
            id: 2,
            owner: 2,
            coordinates: WorldCoordinates { x: 1, y: 0 },
            ..UnitState::default()
        });
        let mut unit = mover(1);
        unit.coordinates = WorldCoordinates { x: 0, y: 0 };
        let hits = attack_options(&state, &unit, 1, false, 2, &[], false, |_| false, |_| false);
        assert!(hits.contains(&WorldCoordinates { x: 1, y: 0 }));
        assert!(!attack_options(&state, &unit, 1, false, 0, &[], false, |_| false, |_| false)
            .contains(&WorldCoordinates { x: 1, y: 0 }));

        state.diplomacy.push(crate::PlayerDiplomacy {
            player: 1,
            relations: vec![crate::DiplomacyLink {
                other: 2,
                relation: crate::DiplomacyRelation {
                    state: DiplomacyRelationState::Peace,
                    ..crate::DiplomacyRelation::default()
                },
            }],
        });
        let peaceful = attack_options(&state, &unit, 1, false, 2, &[], false, |_| false, |_| false);
        assert!(!peaceful.contains(&WorldCoordinates { x: 1, y: 0 }));

        state.map.tiles[1].improvement = IMPROVEMENT_CITY;
        state.map.tiles[1].owner = 2;
        state.improvements.push(crate::ImprovementRuntime {
            tile: 1,
            robbed: true,
            ..crate::ImprovementRuntime::default()
        });
        let robbed = attack_options(&state, &unit, 1, true, 0, &[ABILITY_INFILTRATE], false, |_| false, |_| false);
        assert!(!robbed.contains(&WorldCoordinates { x: 1, y: 0 }));
        state.version = VERSION_INFILTRATE_ROBBED;
        let older = attack_options(&state, &unit, 1, true, 0, &[ABILITY_INFILTRATE], false, |_| false, |_| false);
        assert!(older.contains(&WorldCoordinates { x: 1, y: 0 }));
    }

    fn cost_state(version: i32) -> GameState {
        let mut state = GameState::new(version, 1);
        state.map = crate::Map::new(3, 3);
        state
    }

    fn step_cost<'a>(state: &'a GameState, unit: Option<&'a UnitState>) -> MovementCostQuery<'a> {
        MovementCostQuery {
            state,
            player_id: 1,
            from: WorldCoordinates { x: 0, y: 1 },
            dest: WorldCoordinates { x: 1, y: 1 },
            from_terrain: 3,
            dest_terrain: 3,
            from_improvement: 0,
            dest_improvement: 0,
            abilities: &[],
            unit,
            require_port: false,
            allow_occupied: false,
            follow_transport: false,
            will_embark: false,
            dest_has_embark: false,
            dest_has_road: false,
            from_has_road: false,
            dest_has_route: false,
            dest_flooded: false,
            dest_algae: false,
            has_road_to: false,
            has_glide: false,
            improvement_slows: false,
            has_tech_32: false,
            water_bound: false,
            land_bound: false,
            zoc: None,
            ability_bits: None,
        }
    }

    #[test]
    fn movement_cost_splits_skate_embark_and_leader_zoc() {
        let walker =
            UnitState { id: 1, owner: 1, coordinates: WorldCoordinates { x: 0, y: 1 }, ..UnitState::default() };
        let old = cost_state(VERSION_MOVEMENT_COST);
        let mut query = step_cost(&old, Some(&walker));
        query.abilities = &[21];
        query.dest_terrain = 3;
        assert_eq!(get_movement_cost(&query), 0x1E);
        query.dest_terrain = 6;
        assert_eq!(get_movement_cost(&query), 10);

        let newer = cost_state(VERSION_MOVEMENT_COST + 1);
        let mut query = step_cost(&newer, Some(&walker));
        query.abilities = &[21];
        query.dest_terrain = 3;
        assert_eq!(get_movement_cost(&query), 0x3E8);
        query.dest_terrain = 6;
        assert_eq!(get_movement_cost(&query), 10);

        let ice = cost_state(0x71);
        let mut query = step_cost(&ice, Some(&walker));
        query.abilities = &[21];
        query.from_terrain = 6;
        query.dest_terrain = 6;
        assert_eq!(get_movement_cost(&query), 5);

        let mut embarked = step_cost(&newer, Some(&walker));
        embarked.will_embark = true;
        assert_eq!(get_movement_cost(&embarked), 0x3E8);
        let early = cost_state(VERSION_EMBARK_OLD);
        let mut embarked = step_cost(&early, Some(&walker));
        embarked.will_embark = true;
        embarked.require_port = false;
        assert_eq!(get_movement_cost(&embarked), 10);

        let mut guarded = cost_state(VERSION_MOVEMENT_COST);
        guarded.units.push(UnitState {
            id: 9,
            owner: 2,
            leader: 4,
            coordinates: WorldCoordinates { x: 1, y: 0 },
            ..UnitState::default()
        });
        let query = step_cost(&guarded, Some(&walker));
        assert_eq!(get_movement_cost(&query), 0x3E8);
        guarded.version = VERSION_MOVEMENT_COST + 1;
        let query = step_cost(&guarded, Some(&walker));
        assert_eq!(get_movement_cost(&query), 10);
    }
}
