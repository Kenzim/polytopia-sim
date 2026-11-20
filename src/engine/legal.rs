//! Legal command generation and the path/attack queries behind it.

use super::*;
use crate::rules::path::{self, MovementCostQuery, PathSettings};

impl Engine {
    /// Has road to: both tiles are connected (road, city or bridge) and passable for `owner`.
    pub fn has_road_to(&self, from: Wc, to: Wc, mover_owner: u8) -> bool {
        let (Some(a), Some(b)) = (self.tile(from), self.tile(to)) else { return false };
        self.has_road_between(a, b, mover_owner)
    }

    fn has_road_between(&self, a: &Tile, b: &Tile, mover_owner: u8) -> bool {
        let linked =
            |t: &Tile| t.has_road || t.improvement == improvement::CITY || t.improvement == improvement::BRIDGE;
        if !linked(a) || !linked(b) {
            return false;
        }
        let ok = |t: &Tile| t.owner == 0 || t.owner == mover_owner || self.at_peace(t.owner, mover_owner);
        ok(a) && ok(b)
    }

    /// Whether the unit embarks or disembarks stepping to `dest`: 1 embark, 2 disembark, 0 neither.
    pub fn will_embark(&self, unit: &UnitState, dest: Wc) -> i32 {
        let Some(d) = self.tile(dest) else { return 0 };
        self.will_embark_onto(self.embark_ctx(unit), d)
    }

    /// The unit-only inputs of [`Engine::will_embark`]: `None` when the unit type or its tile is unknown.
    fn embark_ctx(&self, unit: &UnitState) -> Option<EmbarkCtx> {
        let info = self.unit_info(unit)?;
        Some(EmbarkCtx {
            aquatic: info.abilities.contains(&unit_ability::SWIM) || info.abilities.contains(&unit_ability::FLY),
            carrier: info.abilities.contains(&unit_ability::CARRY),
            chained: unit.leader != 0 || unit.follower != 0,
            owner: unit.owner,
            can_embark: self.can_player_embark(unit.owner),
        })
    }

    fn will_embark_onto(&self, e: Option<EmbarkCtx>, d: &Tile) -> i32 {
        let Some(EmbarkCtx { aquatic, carrier, chained, owner, can_embark }) = e else { return 0 };
        // The destination counts as water for a flooded or algae-covered land tile too (retail's embark test asks
        // whether the tile is water or wetland); a city stays land for a transport landing.
        let dest_water =
            path::is_water(d.terrain) || (d.improvement != improvement::CITY && self.is_wetland_index(d.index));
        if aquatic && (!carrier || dest_water) {
            return 0;
        }
        if !dest_water {
            // A transport lands on dry land or a city, not on wetland (land under the Flooded effect).
            let landing = !self.is_wetland_index(d.index) || d.improvement == improvement::CITY;
            return if carrier && landing { 2 } else { 0 };
        }
        // Where the unit stands does not matter: a land unit on an Algae tile (water a land unit may enter) that steps
        // onto a port embarks like one stepping off dry land.
        // A unit in a chain never embarks, its owner must know a technology that lets units onto water, and the port
        // must be its owner's or belong to a player at peace with it (checked against versions 121 to 124). A land unit
        // may still step onto a port without the technology when the tile has algae, but it does not embark.
        if !carrier && !chained && can_embark && (d.owner == owner || self.at_peace(owner, d.owner)) {
            let embark =
                d.improvement != 0 && self.t.has_improvement_ability(d.improvement, improvement_ability::EMBARK);
            if embark {
                return 1;
            }
        }
        0
    }

    pub fn path_settings<'a>(
        &self,
        unit: &'a UnitState,
        allowed: &'a [i32],
        abilities: &'a [i32],
        for_attack_target: bool,
    ) -> PathSettings<'a> {
        let aquatic = abilities.contains(&unit_ability::SWIM);
        let _ = for_attack_target;
        PathSettings {
            version: self.s.version,
            player_id: unit.owner,
            allowed_terrain: allowed,
            abilities,
            mover: Some(unit),
            require_port: path::require_port_for_unit(aquatic, abilities),
            allow_occupied: false,
            allow_enemy: true,
            allow_allies: false,
            can_player_embark: self.can_player_embark(unit.owner),
        }
    }

    /// The player has unlocked an improvement that lets land units embark (a port). Retail asks the technologies the
    /// player knows for the improvements they unlock for its tribe; improvements from tasks do not count.
    pub fn can_player_embark(&self, player: u8) -> bool {
        let tribe_tech = self.t.tribe_tech(self.tribe_of(player));
        self.techs(player).iter().any(|&tech| {
            tribe_tech
                .improvements(tech)
                .iter()
                .any(|&i| self.t.has_improvement_ability(i, improvement_ability::EMBARK))
        })
    }

    /// Cost of stepping `from` -> `to` for `unit`.
    pub fn step_cost(&self, unit: &UnitState, abilities: &[i32], from: Wc, to: Wc) -> i32 {
        self.step_cost_ex(unit, abilities, from, to, false)
    }

    pub fn step_cost_ex(&self, unit: &UnitState, abilities: &[i32], from: Wc, to: Wc, allow_occupied: bool) -> i32 {
        self.step_cost_with(&self.step_ctx(unit, abilities, allow_occupied), from, to)
    }

    /// The inputs of [`Engine::step_cost_ex`] that depend only on the unit, computed once per path search.
    pub(crate) fn step_ctx<'a>(&self, unit: &'a UnitState, abilities: &'a [i32], allow_occupied: bool) -> StepCtx<'a> {
        let info = self.unit_info(unit);
        StepCtx {
            unit,
            abilities,
            allow_occupied,
            water_bound: info.is_some_and(|i| {
                i.abilities.contains(&unit_ability::WATER) && !i.abilities.contains(&unit_ability::LAND)
            }),
            land_bound: info.is_some_and(|i| {
                i.abilities.contains(&unit_ability::LAND) && !i.abilities.contains(&unit_ability::WATER)
            }),
            require_port: path::require_port_for_unit(abilities.contains(&unit_ability::SWIM), abilities),
            has_glide: self.has_player_ability(unit.owner, player_ability::GLIDE),
            has_tech_32: self.has_tech(unit.owner, 32),
            embark: self.embark_ctx(unit),
            zoc: None,
            ability_bits: path::ability_bits(abilities),
            old_disembark: crate::env_var!("OLD_DISEMBARK").is_ok(),
        }
    }

    /// [`Engine::step_ctx`] with the zone-of-control map filled in, for a search that prices many steps.
    pub(crate) fn step_ctx_search<'a>(
        &self,
        unit: &'a UnitState,
        abilities: &'a [i32],
        allow_occupied: bool,
    ) -> StepCtx<'a> {
        let mut c = self.step_ctx(unit, abilities, allow_occupied);
        let exempt = allow_occupied
            || [unit_ability::SNEAK, unit_ability::HIDE].iter().any(|a| abilities.contains(a))
            || unit.invisible;
        if !exempt {
            c.zoc = Some(self.shared_zoc_map(unit.owner, self.s.version > path::VERSION_MOVEMENT_COST));
        }
        c
    }

    /// [`Engine::step_cost_ex`] with the unit's inputs from [`Engine::step_ctx`].
    pub(crate) fn step_cost_with(&self, c: &StepCtx<'_>, from: Wc, to: Wc) -> i32 {
        let (Some(f), Some(d)) = (self.tile(from), self.tile(to)) else { return 1000 };
        let unit = c.unit;
        let slows = d.improvement != 0 && self.t.has_improvement_ability(d.improvement, improvement_ability::SLOW);
        let embark = self.will_embark_onto(c.embark, d);
        let q = MovementCostQuery {
            state: &self.s,
            player_id: unit.owner,
            from,
            dest: to,
            from_terrain: f.terrain,
            dest_terrain: d.terrain,
            from_improvement: f.improvement,
            dest_improvement: d.improvement,
            abilities: c.abilities,
            unit: Some(unit),
            require_port: c.require_port,
            allow_occupied: c.allow_occupied,
            follow_transport: false,
            // Any embark status other than None costs 1000 (disembark too).
            will_embark: if c.old_disembark { embark == 1 } else { embark != 0 },
            dest_has_embark: d.improvement != 0
                && self.t.has_improvement_ability(d.improvement, improvement_ability::EMBARK),
            dest_has_road: d.has_road,
            from_has_road: f.has_road,
            dest_has_route: d.has_route,
            dest_flooded: d.effects & (1 << Self::EFFECT_FLOODED) != 0,
            dest_algae: d.effects & (1 << Self::EFFECT_ALGAE) != 0,
            has_road_to: self.has_road_between(f, d, unit.owner),
            has_glide: c.has_glide,
            improvement_slows: slows,
            has_tech_32: c.has_tech_32,
            water_bound: c.water_bound,
            land_bound: c.land_bound,
            zoc: c.zoc.as_deref().map(Vec::as_slice),
            ability_bits: c.ability_bits,
        };
        path::get_movement_cost(&q)
    }

    /// Destinations the unit can reach this turn.
    pub fn move_destinations(&self, unit: &UnitState) -> Vec<Wc> {
        let Some(info) = self.unit_info(unit) else { return Vec::new() };
        // The movement budget counts Boosted, Swift and Bubble, so a unit whose data movement is 0 (a swarmed egg) can
        // still step.
        let movement = self.unit_movement(unit);
        if unit.moved || unit.frozen || unit.leader != 0 || movement <= 0 {
            return Vec::new();
        }
        let allowed = self.move_terrain_list(unit);
        let abilities = info.abilities.as_slice();
        let settings = self.path_settings(unit, &allowed, abilities, false);
        if crate::env_var!("MOVE_DBG").ok().and_then(|v| v.parse::<u32>().ok()) == Some(unit.id) {
            for n in self.neighbors(unit.coordinates) {
                let (Some(t), Some(o)) = (self.tile(n), self.tile(unit.coordinates)) else { continue };
                let acc = path::is_tile_accessible(
                    &self.s,
                    t,
                    o,
                    &settings,
                    &|i| self.is_wetland_index(i),
                    &|k, a| self.t.has_improvement_ability(k, a),
                    &|i| self.blocked_bridge_direction(unit.owner, i),
                );
                eprintln!(
                    "MOVEDBG unit {} to ({},{}) terrain {} owner {} acc {} cost {} allowed {:?} req_port {}",
                    unit.id,
                    n.x,
                    n.y,
                    t.terrain,
                    t.owner,
                    acc,
                    self.step_cost(unit, abilities, unit.coordinates, n),
                    &*allowed,
                    settings.require_port
                );
            }
        }
        let step = self.step_ctx_search(unit, abilities, false);
        let mut options = path::get_move_options_min_step(
            &self.s,
            unit.coordinates,
            movement,
            unit,
            &settings,
            None,
            |i| self.is_wetland_index(i),
            |kind, ability| self.t.has_improvement_ability(kind, ability),
            |i| self.blocked_bridge_direction(unit.owner, i),
            |a, b| self.step_cost_with(&step, a, b),
            Some(path::MIN_MOVEMENT_COST),
        );
        if crate::env_var!("OLD_DISEMBARK").is_ok() {
            for c in self.disembark_tiles(unit) {
                if !options.contains(&c) {
                    options.push(c);
                }
            }
        }
        options
    }

    /// Terrain a move search may enter: a carrier with a passenger may end on the passenger's land (the step costs
    /// 1000, so the search stops there; the move options still list it).
    pub fn move_terrain(&self, unit: &UnitState) -> Vec<i32> {
        self.move_terrain_list(unit).to_vec()
    }

    /// [`Engine::move_terrain`] without the allocation.
    pub(crate) fn move_terrain_list(&self, unit: &UnitState) -> TerrainList {
        let mut allowed = self.allowed_terrain_list(unit);
        if crate::env_var!("OLD_DISEMBARK").is_err() && self.has_ability(unit, unit_ability::CARRY) {
            if let Some(p) = unit.passenger.as_ref() {
                for &t in self.allowed_terrain_list(p).iter() {
                    allowed.push_new(t);
                }
            }
        }
        allowed
    }

    /// A carrier in a port may step onto an adjacent land tile (the embark check's result 2); the passenger lands.
    pub fn disembark_tiles(&self, unit: &UnitState) -> Vec<Wc> {
        let Some(p) = unit.passenger.as_ref() else { return Vec::new() };
        if unit.moved || !self.has_ability(unit, unit_ability::CARRY) {
            return Vec::new();
        }
        if !self.tile(unit.coordinates).is_some_and(|t| path::is_water(t.terrain)) {
            return Vec::new();
        }
        let allowed = self.allowed_terrain_list(p);
        self.neighbors(unit.coordinates)
            .into_iter()
            .filter(|n| {
                self.tile(*n).is_some_and(|t| {
                    !path::is_water(t.terrain) && allowed.contains(&t.terrain) && self.is_explored_at(unit.owner, *n)
                }) && self.unit_at(*n).is_none()
            })
            .collect()
    }

    /// Full path (start .. destination) for a move, or `None`.
    pub fn move_path(&self, unit: &UnitState, dest: Wc) -> Option<Vec<Wc>> {
        let info = self.unit_info(unit)?;
        let _grid = self.unit_grid_scope();
        if crate::env_var!("OLD_DISEMBARK").is_ok() && self.disembark_tiles(unit).contains(&dest) {
            return Some(vec![unit.coordinates, dest]);
        }
        let allowed = self.move_terrain_list(unit);
        let abilities = info.abilities.as_slice();
        let settings = self.path_settings(unit, &allowed, abilities, false);
        let movement = self.unit_movement(unit);
        let step = self.step_ctx_search(unit, abilities, false);
        let mut p = path::get_path(
            &self.s,
            unit.coordinates,
            dest,
            movement,
            &settings,
            |i| self.is_wetland_index(i),
            |kind, ability| self.t.has_improvement_ability(kind, ability),
            |i| self.blocked_bridge_direction(unit.owner, i),
            |a, b| self.step_cost_with(&step, a, b),
        )?;
        p.reverse();
        Some(p)
    }

    /// Tiles the unit could attack from where it stands.
    pub fn attack_targets(&self, unit: &UnitState) -> Vec<Wc> {
        self.attack_targets_ex(unit, false)
    }

    pub fn attack_targets_ex(&self, unit: &UnitState, ignore_diplomacy: bool) -> Vec<Wc> {
        let Some(info) = self.unit_info(unit) else { return Vec::new() };
        path::attack_options(
            &self.s,
            unit,
            info.data.range,
            ignore_diplomacy,
            info.data.attack,
            &info.abilities,
            false,
            |i| path::tile_being_captured(&self.s, i),
            |i| self.floodable(i),
        )
    }

    /// Whether a tile can flood: any land tile that does not carry the Flooded effect yet. Water, ocean, mountains
    /// and ice never flood; an improvement or a city does not matter.
    pub(super) fn floodable(&self, tile_index: i32) -> bool {
        self.s.map.tiles.get(tile_index as usize).is_some_and(|t| {
            !matches!(t.terrain, terrain::WATER | terrain::OCEAN | terrain::MOUNTAIN | terrain::ICE)
                && t.effects & (1 << Self::EFFECT_FLOODED) == 0
        })
    }

    /// Whether the unit can attack.
    pub fn can_attack(&self, unit: &UnitState) -> bool {
        let abilities = self.unit_info(unit).map(|i| i.abilities.as_slice()).unwrap_or(&[]);
        crate::ai::decide::can_attack(unit, abilities)
    }

    /// Whether the unit can move.
    pub fn can_move(&self, unit: &UnitState) -> bool {
        crate::ai::decide::can_move(unit)
    }

    /// Can recover: no leader, hurt or poisoned, can move and can attack.
    pub fn can_recover(&self, unit: &UnitState) -> bool {
        unit.leader == 0
            && (self.max_health(unit) > i32::from(unit.health) || unit.poisoned)
            && self.can_move(unit)
            && self.can_attack(unit)
    }

    /// Whether the unit can be promoted. A carrier is judged by its passenger: the passenger's type, experience and
    /// promotion count, as retail reads them.
    pub fn can_be_promoted(&self, unit: &UnitState) -> bool {
        let unit = unit.passenger.as_deref().unwrap_or(unit);
        let Some(info) = self.unit_info(unit) else { return false };
        if info.abilities.contains(&unit_ability::STATIC) || info.data.hidden {
            return false;
        }
        let limit = if self.has_player_ability(unit.owner, player_ability::DISCIPLINE) { 2 } else { 3 };
        i32::from(unit.xp) >= limit && unit.promotion == 0
    }

    /// Whether the unit can capture the tile (not counting next turn, the tile itself only).
    pub fn can_capture(&self, unit: &UnitState, at: Wc) -> bool {
        let Some(tile) = self.tile(at) else { return false };
        if unit.owner == NATURE || tile.improvement != improvement::CITY || unit.coordinates != at {
            return false;
        }
        if !self.can_move(unit) || !self.can_attack(unit) || unit.leader != 0 {
            return false;
        }
        if tile.owner == unit.owner {
            return false;
        }
        if tile.owner != 0 {
            if self.at_peace(unit.owner, tile.owner) {
                return false;
            }
            if self.s.version >= 0x78 && crate::rules::diplomacy::has_cool_down_with(&self.s, unit.owner, tile.owner) {
                return false;
            }
        }
        true
    }

    /// Battle-result path: a path onto the defender's tile (occupied tiles allowed, max cost = movement).
    pub fn can_path_onto(&self, attacker: &UnitState, target: Wc) -> bool {
        let Some(info) = self.unit_info(attacker) else { return false };
        let allowed = self.move_terrain_list(attacker);
        let abilities = info.abilities.as_slice();
        let mut settings = self.path_settings(attacker, &allowed, abilities, false);
        settings.allow_occupied = true;
        let movement = self.unit_movement(attacker);
        let step = self.step_ctx(attacker, abilities, true);
        path::get_path(
            &self.s,
            attacker.coordinates,
            target,
            movement,
            &settings,
            |i| self.is_wetland_index(i),
            |kind, ability| self.t.has_improvement_ability(kind, ability),
            |i| self.blocked_bridge_direction(attacker.owner, i),
            |a, b| self.step_cost_with(&step, a, b),
        )
        .is_some()
    }
}

/// Unit-only inputs of the embark check (see [`Engine::will_embark`]).
#[derive(Clone, Copy)]
struct EmbarkCtx {
    aquatic: bool,
    carrier: bool,
    /// The unit is part of a centipede chain (it has a leader or a follower).
    chained: bool,
    owner: u8,
    /// The owner knows a technology that lets its units embark.
    can_embark: bool,
}

/// Unit-only inputs of [`Engine::step_cost_ex`] (see [`Engine::step_ctx`]).
pub(crate) struct StepCtx<'a> {
    unit: &'a UnitState,
    abilities: &'a [i32],
    allow_occupied: bool,
    water_bound: bool,
    land_bound: bool,
    require_port: bool,
    has_glide: bool,
    has_tech_32: bool,
    embark: Option<EmbarkCtx>,
    zoc: Option<Rc<Vec<bool>>>,
    ability_bits: Option<u64>,
    old_disembark: bool,
}

use std::cell::RefCell;
use std::rc::Rc;

/// Zone-of-control maps shared by the move searches of one [`ZocScope`]: the state they were made from (by
/// address; it cannot change while the scope's borrow lives) and, per (player, leaders exempt), the map.
struct ZocShare {
    state: *const GameState,
    maps: Vec<(u8, bool, Rc<Vec<bool>>)>,
    /// Per player, the union of its techs' ability masks, or `None` when some mask is not exact.
    abilities: Vec<(u8, Option<u64>)>,
    /// Per player, [`Engine::movement_terrain_list`].
    movement: Vec<(u8, TerrainList)>,
    /// Per tile index asked for, [`Engine::city_threat_level`].
    city_threats: Vec<(i32, f32)>,
    /// [`Engine::position_neighbors`] of one player, per tile asked for.
    neighbors: Option<NeighborShare>,
}

/// The neighbour lists kept for one player while a position is shared.
struct NeighborShare {
    player: u8,
    /// Per tile index: 0, or 1 + the position in `spans` of the tile's list.
    slot: Vec<u32>,
    /// Per list: tile index, start in `items`, length.
    spans: Vec<(i32, u32, u8)>,
    items: Vec<crate::ai::score::PositionNeighbor>,
}

thread_local! {
    static ZOC_SHARE: RefCell<Option<ZocShare>> = const { RefCell::new(None) };
}

/// Shares zone-of-control maps between the move searches run while it lives; see [`Engine::zoc_scope`].
pub(crate) struct ZocScope<'a>(std::marker::PhantomData<&'a GameState>, bool);

impl Drop for ZocScope<'_> {
    fn drop(&mut self) {
        if self.1 {
            zoc_share_end();
        }
    }
}

/// End the sharing a [`Engine::zoc_share_begin`] that returned true started.
pub(crate) fn zoc_share_end() {
    ZOC_SHARE.with(|z| *z.borrow_mut() = None);
}

impl Engine {
    /// Start sharing zone-of-control maps for this state until the scope ends; nested scopes do nothing.
    pub(crate) fn zoc_scope(&self) -> ZocScope<'_> {
        ZocScope(std::marker::PhantomData, self.zoc_share_begin())
    }

    /// [`Engine::zoc_scope`] without the borrow, for a stretch that holds the engine mutably and changes nothing the
    /// shared values are made from (units, relations, techs). Returns whether sharing began here; the caller then
    /// ends it with [`zoc_share_end`].
    pub(crate) fn zoc_share_begin(&self) -> bool {
        ZOC_SHARE.with(|z| {
            let mut z = z.borrow_mut();
            if z.is_some() {
                return false;
            }
            *z = Some(ZocShare {
                state: &self.s,
                maps: Vec::new(),
                abilities: Vec::new(),
                movement: Vec::new(),
                city_threats: Vec::new(),
                neighbors: None,
            });
            true
        })
    }

    /// Panic unless every value shared for this state is what the state gives now.
    #[cfg(feature = "grid_check")]
    pub(crate) fn zoc_share_check(&self) {
        let Some(share) = ZOC_SHARE.with(|z| z.borrow_mut().take()) else { return };
        if !std::ptr::eq(share.state, &self.s) {
            ZOC_SHARE.with(|z| *z.borrow_mut() = Some(share));
            return;
        }
        for (player, leaders_exempt, map) in &share.maps {
            assert_eq!(
                **map,
                path::zoc_map(&self.s, *player, *leaders_exempt),
                "stale zone-of-control map of player {player}"
            );
        }
        for (player, list) in &share.movement {
            assert_eq!(
                list.to_vec(),
                self.unlocked_movement_terrain_of(*player).to_vec(),
                "stale movement terrain of player {player}"
            );
        }
        if let Some(kept) = &share.neighbors {
            for &(index, start, len) in &kept.spans {
                let mut now = Vec::new();
                self.position_neighbors(kept.player, self.s.map.tiles[index as usize].coordinates, &mut now);
                assert_eq!(
                    kept.items[start as usize..start as usize + usize::from(len)],
                    now[..],
                    "stale position neighbours of tile {index}"
                );
            }
        }
        for &(index, level) in &share.city_threats {
            let now = self.city_threat_level_of(&self.s.map.tiles[index as usize]);
            assert_eq!(level.to_bits(), now.to_bits(), "stale city threat level of tile {index}");
        }
        for &(player, bits) in &share.abilities {
            if let Some(bits) = bits {
                for ability in 0..64 {
                    let has = self.techs(player).iter().any(|&tech| {
                        self.t.tech_info(tech).is_some_and(|i| i.ability_mask.contains(&i.abilities, ability))
                    });
                    assert_eq!(bits & (1 << ability) != 0, has, "stale ability {ability} of player {player}");
                }
            }
        }
        ZOC_SHARE.with(|z| *z.borrow_mut() = Some(share));
    }

    /// `path::zoc_map`, taken from the active [`ZocScope`] when there is one for this state.
    fn shared_zoc_map(&self, player: u8, leaders_exempt: bool) -> Rc<Vec<bool>> {
        ZOC_SHARE.with(|z| {
            let mut z = z.borrow_mut();
            match z.as_mut() {
                Some(share) if std::ptr::eq(share.state, &self.s) => {
                    if let Some((_, _, m)) = share.maps.iter().find(|(p, l, _)| *p == player && *l == leaders_exempt) {
                        return m.clone();
                    }
                    let m = Rc::new(path::zoc_map(&self.s, player, leaders_exempt));
                    share.maps.push((player, leaders_exempt, m.clone()));
                    m
                }
                _ => Rc::new(path::zoc_map(&self.s, player, leaders_exempt)),
            }
        })
    }

    /// The union of the player's tech ability masks from the active [`ZocScope`]; `None` outside one for this state
    /// or when some tech's mask is not exact (then scan the techs).
    pub(crate) fn shared_ability_bits(&self, player: u8) -> Option<u64> {
        ZOC_SHARE.with(|z| {
            let mut z = z.borrow_mut();
            let share = z.as_mut().filter(|share| std::ptr::eq(share.state, &self.s))?;
            if let Some(&(_, bits)) = share.abilities.iter().find(|(p, _)| *p == player) {
                return bits;
            }
            let mut bits = Some(0u64);
            for &tech in self.techs(player) {
                if let Some(info) = self.t.tech_info(tech) {
                    bits = match (bits, info.ability_mask.exact_bits()) {
                        (Some(all), Some(b)) => Some(all | b),
                        _ => None,
                    };
                }
            }
            share.abilities.push((player, bits));
            bits
        })
    }

    /// Add the neighbour list kept for `(player, tile)` in the active [`ZocScope`] to `out`; false when there is
    /// none (no scope for this state, another player's lists are kept, or the tile has not been asked for).
    pub(crate) fn shared_position_neighbors(
        &self,
        player: u8,
        index: i32,
        out: &mut Vec<crate::ai::score::PositionNeighbor>,
    ) -> bool {
        ZOC_SHARE.with(|z| {
            let z = z.borrow();
            let Some(kept) = z
                .as_ref()
                .filter(|share| std::ptr::eq(share.state, &self.s))
                .and_then(|share| share.neighbors.as_ref())
            else {
                return false;
            };
            if kept.player != player {
                return false;
            }
            match kept.slot[index as usize] {
                0 => false,
                at => {
                    let (_, start, len) = kept.spans[at as usize - 1];
                    out.extend_from_slice(&kept.items[start as usize..start as usize + usize::from(len)]);
                    true
                }
            }
        })
    }

    /// Keep a neighbour list for the rest of the active [`ZocScope`], if there is one for this state. The lists of
    /// the first player asked for are kept; other players' are not.
    pub(crate) fn share_position_neighbors(&self, player: u8, index: i32, list: &[crate::ai::score::PositionNeighbor]) {
        ZOC_SHARE.with(|z| {
            let mut z = z.borrow_mut();
            let Some(share) = z.as_mut().filter(|share| std::ptr::eq(share.state, &self.s)) else { return };
            let kept = share.neighbors.get_or_insert_with(|| NeighborShare {
                player,
                slot: vec![0; self.s.map.tiles.len()],
                spans: Vec::new(),
                items: Vec::new(),
            });
            if kept.player != player || kept.slot[index as usize] != 0 {
                return;
            }
            kept.spans.push((index, kept.items.len() as u32, list.len() as u8));
            kept.items.extend_from_slice(list);
            kept.slot[index as usize] = kept.spans.len() as u32;
        });
    }

    /// The city threat level kept for the tile in the active [`ZocScope`]; `None` outside one for this state or
    /// when the tile has not been asked for.
    pub(crate) fn shared_city_threat(&self, index: i32) -> Option<f32> {
        ZOC_SHARE.with(|z| {
            let z = z.borrow();
            let share = z.as_ref().filter(|share| std::ptr::eq(share.state, &self.s))?;
            share.city_threats.iter().find(|e| e.0 == index).map(|e| e.1)
        })
    }

    /// Keep a tile's city threat level for the rest of the active [`ZocScope`], if there is one for this state.
    pub(crate) fn share_city_threat(&self, index: i32, level: f32) {
        ZOC_SHARE.with(|z| {
            if let Some(share) = z.borrow_mut().as_mut().filter(|share| std::ptr::eq(share.state, &self.s)) {
                share.city_threats.push((index, level));
            }
        });
    }

    /// [`Engine::movement_terrain_list`] from the active [`ZocScope`]; `None` outside one for this state.
    pub(crate) fn shared_movement_terrain(&self, player: u8) -> Option<TerrainList> {
        ZOC_SHARE.with(|z| {
            let mut z = z.borrow_mut();
            let share = z.as_mut().filter(|share| std::ptr::eq(share.state, &self.s))?;
            if let Some((_, list)) = share.movement.iter().find(|(p, _)| *p == player) {
                return Some(list.clone());
            }
            let list = self.unlocked_movement_terrain_of(player);
            share.movement.push((player, list.clone()));
            Some(list)
        })
    }
}
