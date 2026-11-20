//! A playable Polytopia engine on top of the verified primitives.
//!
//! [`Engine`] owns a [`GameState`] plus shared rule [`Tables`]. It can list legal
//! [`Command`]s for the current player, apply one, and run the turn flow, so an
//! AI or a tree search can play whole games. Formulas taken from the retail game (combat,
//! movement cost, tech price, defence bonus, income) are called from `crate::rules`; the glue around them
//! follows the retail rules.

mod ai;
mod ai_diplomacy;
mod ai_improve;
mod ai_move;
mod ai_rate;
mod ai_research;
mod ai_state;
mod apply;
mod city;
mod freeze;
mod infiltrate;
mod legal;
pub mod load;
mod opinion;
pub mod position;
pub mod replay;
mod routes;
pub mod score_parts;
mod scout;
mod setup;
pub mod tables;
mod tasks;
mod turn;
mod vision;

pub use ai_state::{AiState, MilitaryStats, Mission, UnitTarget};
pub use infiltrate::{InfiltrateRun, INFILTRATE_SPAWN_MAX};
pub use opinion::OpinionState;
pub use score_parts::ScoreParts;
pub use scout::ScoutRun;
pub use setup::{GameOptions, NewGameError, PlayerSetup};
pub use tables::Tables;
pub use vision::{ExploreRun, EXPLORE_PATH_MAX};

use crate::flat_map::FlatMap;
use crate::{GameState, ImprovementRuntime, PlayerState, Tile, UnitState, WorldCoordinates};
use std::sync::Arc;
use tables::*;

/// Short name for [`WorldCoordinates`].
pub type Wc = WorldCoordinates;

/// A terrain id list for move and route searches, in the order pushed (duplicates kept), held inline up to
/// [`TerrainList::INLINE`] ids so building one does not allocate; a longer list moves to the heap.
#[derive(Clone)]
pub(crate) struct TerrainList {
    ids: [i32; TerrainList::INLINE],
    len: usize,
    /// Every id once the list outgrows `ids`; empty (and unused) before that.
    spill: Vec<i32>,
}

impl TerrainList {
    const INLINE: usize = 16;

    pub(crate) fn new() -> Self {
        Self { ids: [0; Self::INLINE], len: 0, spill: Vec::new() }
    }

    pub(crate) fn push(&mut self, id: i32) {
        if self.spill.is_empty() && self.len < Self::INLINE {
            self.ids[self.len] = id;
            self.len += 1;
            return;
        }
        if self.spill.is_empty() {
            self.spill.extend_from_slice(&self.ids[..self.len]);
        }
        self.spill.push(id);
    }

    /// Push `id` unless the list already holds it.
    pub(crate) fn push_new(&mut self, id: i32) {
        if !self.contains(&id) {
            self.push(id);
        }
    }

    pub(crate) fn retain(&mut self, mut keep: impl FnMut(i32) -> bool) {
        let mut out = Self::new();
        for &id in self.iter() {
            if keep(id) {
                out.push(id);
            }
        }
        *self = out;
    }
}

impl std::ops::Deref for TerrainList {
    type Target = [i32];
    fn deref(&self) -> &[i32] {
        if self.spill.is_empty() {
            &self.ids[..self.len]
        } else {
            &self.spill
        }
    }
}

impl FromIterator<i32> for TerrainList {
    fn from_iter<I: IntoIterator<Item = i32>>(iter: I) -> Self {
        let mut out = Self::new();
        for id in iter {
            out.push(id);
        }
        out
    }
}

/// The player id of Nature, which owns animals and neutral tiles and never takes a turn.
pub const NATURE: u8 = 255;

/// One thing a player can do. [`Engine::legal_commands`] lists the ones allowed now and [`Engine::apply`] plays one.
/// Units are named by their id, tiles by [`Wc`], unit types, improvements and techs by their number in the rules data.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Command {
    Move {
        unit: u32,
        to: Wc,
    },
    Attack {
        unit: u32,
        target: Wc,
    },
    Capture {
        unit: u32,
        at: Wc,
    },
    Train {
        city: Wc,
        unit_type: i32,
    },
    Build {
        at: Wc,
        improvement: i32,
    },
    Research {
        tech: i32,
    },
    CityReward {
        city: Wc,
        reward: crate::CityReward,
    },
    Promote {
        unit: u32,
    },
    Recover {
        unit: u32,
    },
    HealOthers {
        unit: u32,
    },
    Explode {
        unit: u32,
    },
    Disband {
        unit: u32,
    },
    Destroy {
        at: Wc,
    },
    Decompose {
        at: Wc,
    },
    Upgrade {
        unit: u32,
        to: i32,
    },
    ExamineRuins {
        unit: u32,
    },
    Stay {
        unit: u32,
    },
    Swarm {
        unit: u32,
    },
    /// Boost command (25): the pre-Swarm Shaman ability (gamedata up to v23). Nothing builds it in version 123.
    Boost {
        unit: u32,
    },
    Disembark {
        unit: u32,
    },
    /// Freeze-area command (17): the unit freezes the tiles and units round it (a Gaami).
    FreezeArea {
        unit: u32,
    },
    /// Break-ice command (18): the unit breaks the ice round it.
    BreakIce {
        unit: u32,
    },
    /// Clear-tile-effect command: remove a tile effect (Flooded 1, Algae 4) from a tile.
    ClearTileEffect {
        at: Wc,
        effect: u16,
    },
    BreakPeace {
        with: u8,
    },
    PeaceTreaty {
        with: u8,
    },
    PeaceResponse {
        with: u8,
        accept: bool,
    },
    EstablishEmbassy {
        with: u8,
    },
    EndTurn,
}

/// Why [`Engine::apply`] refused a command.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Reject {
    NotYourTurn,
    PendingReward,
    NoSuchUnit,
    NotAllowed(&'static str),
}

/// A pending command trigger: a city level-up reward choice (`CityLevelUp`), or with `peace_from` set a peace
/// request from that player waiting for an answer (`PeaceRequest`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Trigger {
    pub player: u8,
    pub city: Wc,
    pub peace_from: Option<u8>,
}

/// An action the start-of-turn processing left on the game's action stack. The action runner
/// runs the last one first and stops while the current player has a pending trigger, so a city reward is answered
/// before the next queued level-up runs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Deferred {
    CityLevelUp(Wc),
    ImprovementLevelUp(Wc),
    ImprovementLevelDown(Wc),
    /// A currency increase (player, amount).
    Income(u8, i32),
    /// A received peace request: `to` gets a trigger to answer `from`.
    PeaceRequest {
        to: u8,
        from: u8,
    },
    /// A score increase (player, amount) for a change already on the board (a city level-up and the population
    /// behind it) that retail credits only when the action runs, i.e. after the city reward it waits under.
    /// `refresh_scores` leaves it out until then.
    Score(u8, i32),
    /// A population change (city, amount) held back because an earlier action of the same command raised a city
    /// reward. Retail's action queue stops at the level-up and runs the rest only once the reward is answered.
    Population(Wc, i32),
    /// The rest of a scout's move chain, held back by a city reward that an earlier step raised.
    Scout(ScoutRun),
    /// The rest of a move's explore actions, held back by a city reward that one of its tiles raised.
    Explore(ExploreRun),
    /// The spawns and payment of an infiltration reward, held back by a city reward that one of its spawns raised.
    Infiltrate(InfiltrateRun),
    /// A unit train whose occupant push raised a city reward: the unit appears once the reward is answered.
    /// `veteran` is the ruins' swordsman, promoted once and healed to full on arrival.
    Train {
        owner: u8,
        unit_type: i32,
        at: Wc,
        home: Wc,
        veteran: bool,
    },
}

/// A running game: the [`GameState`] (`s`), the shared rule [`Tables`] (`t`) and the bookkeeping around them (pending
/// choices, AI memory, caches).
///
/// Start one with [`Engine::new_game`], list moves with [`Engine::legal_commands`], play them with [`Engine::apply`],
/// or let the built-in AI move with [`Engine::step_ai`]. Cloning an engine copies the whole game, which is how a search
/// tries moves out.
#[derive(Clone)]
pub struct Engine {
    pub t: Arc<Tables>,
    pub s: GameState,
    pub triggers: Vec<Trigger>,
    /// Action stack entries left by the start-of-turn processing, top last.
    pub deferred: Vec<Deferred>,
    /// A peace request waiting for an answer: (from, to).
    pub peace_requests: Vec<(u8, u8)>,
    /// The ids of the winning players, filled when the game ends (see [`Engine::game_winners`]).
    pub winners: Vec<u8>,
    pub ai_states: FlatMap<u8, ai_state::AiState>,
    pub opinions: std::cell::RefCell<opinion::OpinionStore>,
    /// Per city, the last population check and the available population: (key, value), recomputed when `key <= turn`.
    pub pop_cache: std::cell::RefCell<FlatMap<(i32, i32), (i64, i32)>>,
    /// Set by an explore or a road build, consumed by the deferred action that ends every command
    /// (it triggers the route update).
    pub routes_flagged: bool,
    /// Where the next action held by the running command goes in `deferred`: below the ones it already held, so
    /// they run in the order they were raised. Reset at the start of every command.
    hold_at: Option<usize>,
    /// The end of the current player's turn is waiting for a city reward the turn's end raised (see
    /// [`Engine::end_turn`]); the turn passes on once it is answered.
    handover_held: bool,
    /// Set while [`Engine::end_match`] runs its end-turn step, so that step ends the game without the end-of-turn effects.
    ending_match: bool,
}

impl Engine {
    /// Debug gate: true when env `name` is set and (if `DBG_CMD` is set) the command counter equals it.
    pub fn dbg(&self, name: &str) -> bool {
        if crate::env_var(name).is_err() {
            return false;
        }
        match crate::env_var!("DBG_CMD").ok().and_then(|v| v.parse::<u16>().ok()) {
            Some(c) => self.s.current_command == c,
            None => true,
        }
    }

    /// The newest pending trigger of `player`.
    pub fn pending_trigger(&self, player: u8) -> Option<Trigger> {
        self.triggers.iter().rev().find(|t| t.player == player).copied()
    }

    /// An engine around an existing state. [`Engine::new_game`] is the usual way to start a game.
    pub fn new(t: Arc<Tables>, s: GameState) -> Self {
        Self {
            t,
            s,
            triggers: Vec::new(),
            deferred: Vec::new(),
            peace_requests: Vec::new(),
            winners: Vec::new(),
            ai_states: Default::default(),
            opinions: Default::default(),
            pop_cache: Default::default(),
            routes_flagged: false,
            hold_at: None,
            handover_held: false,
            ending_match: false,
        }
    }

    /// Whether the current player has a city reward to answer. The rest of the running command's actions wait for it.
    pub fn reward_pending(&self) -> bool {
        let me = self.current_player();
        self.triggers.iter().any(|t| t.player == me && t.peace_from.is_none())
    }

    /// Queue an action behind the pending city reward, after the ones this command already queued.
    pub(super) fn hold(&mut self, action: Deferred) {
        let at = self.hold_at.unwrap_or(self.deferred.len()).min(self.deferred.len());
        self.deferred.insert(at, action);
        self.hold_at = Some(at);
    }

    // ---------- basic lookups ----------

    pub fn width(&self) -> i32 {
        i32::from(self.s.map.width)
    }

    pub fn height(&self) -> i32 {
        i32::from(self.s.map.height)
    }

    pub fn in_bounds(&self, c: Wc) -> bool {
        c.x >= 0 && c.y >= 0 && c.x < self.width() && c.y < self.height()
    }

    pub fn tile_index(&self, c: Wc) -> Option<usize> {
        if self.in_bounds(c) {
            Some((c.y * self.width() + c.x) as usize)
        } else {
            None
        }
    }

    pub fn tile(&self, c: Wc) -> Option<&Tile> {
        self.tile_index(c).map(|i| &self.s.map.tiles[i])
    }

    pub fn tile_mut(&mut self, c: Wc) -> Option<&mut Tile> {
        let i = self.tile_index(c)?;
        Some(&mut self.s.map.tiles[i])
    }

    /// Use the unit grid ([`crate::UnitGrid`]) until the returned scope drops; `self.s.units` cannot change meanwhile.
    pub(crate) fn unit_grid_scope(&self) -> GridScope<'_> {
        GridScope(&self.s, self.s.unit_grid_begin())
    }

    /// Share the lookups that many questions about one unchanged position have in common (the tile to unit grid and
    /// the zone-of-control maps of the move searches) until the returned scope drops. Answers are the same with or
    /// without it. A scope opened while another is alive adds nothing, whichever engine holds the first.
    pub fn query_scope(&self) -> QueryScope<'_> {
        QueryScope { _grid: self.unit_grid_scope(), _zoc: self.zoc_scope() }
    }

    /// Run `f` with the unit grid filled and the zone-of-control maps shared; `f` must not change `self.s.units`, the
    /// relations between players or their techs (checked by feature `grid_check`).
    pub(crate) fn with_unit_grid<R>(&mut self, f: impl FnOnce(&mut Engine) -> R) -> R {
        let began = self.s.unit_grid_begin();
        let sharing = self.zoc_share_begin();
        let scope = GridScopeMut(self, began, sharing);
        f(&mut *scope.0)
    }

    pub fn unit_index_at(&self, c: Wc) -> Option<usize> {
        self.s.unit_index_at(c)
    }

    pub fn unit_at(&self, c: Wc) -> Option<&UnitState> {
        self.s.unit_at(c)
    }

    pub fn unit_index(&self, id: u32) -> Option<usize> {
        self.s.units.iter().position(|u| u.id == id)
    }

    pub fn unit(&self, id: u32) -> Option<&UnitState> {
        self.s.units.iter().find(|u| u.id == id)
    }

    pub fn player(&self, id: u8) -> Option<&PlayerState> {
        self.s.players.iter().find(|p| p.id == id)
    }

    pub fn player_mut(&mut self, id: u8) -> Option<&mut PlayerState> {
        self.s.players.iter_mut().find(|p| p.id == id)
    }

    /// The id of the player whose turn it is.
    pub fn current_player(&self) -> u8 {
        self.s.current_player_id().unwrap_or(0)
    }

    pub fn is_water_tile(&self, c: Wc) -> bool {
        self.tile(c).is_some_and(|t| crate::rules::path::is_water(t.terrain))
    }

    /// Radius `r` Chebyshev square, diagonals on, centre excluded, only in-bounds coordinates.
    pub fn area(&self, center: Wc, r: i32) -> Vec<Wc> {
        self.area_iter(center, r).collect()
    }

    /// [`Engine::area`] without the allocation: same coordinates, same order.
    pub fn area_iter(&self, center: Wc, r: i32) -> impl Iterator<Item = Wc> + '_ {
        crate::rules::path::area_iter(center, r).filter(|c| self.in_bounds(*c))
    }

    pub fn neighbors(&self, c: Wc) -> Vec<Wc> {
        self.area(c, 1)
    }

    pub fn chebyshev(a: Wc, b: Wc) -> i32 {
        (a.x - b.x).abs().max((a.y - b.y).abs())
    }

    // ---------- improvements / cities ----------

    pub fn improvement_rt(&self, c: Wc) -> Option<&ImprovementRuntime> {
        let i = self.tile(c)?.index;
        self.s.improvements.iter().find(|r| r.tile == i)
    }

    pub fn improvement_rt_mut(&mut self, c: Wc) -> Option<&mut ImprovementRuntime> {
        let i = self.tile(c)?.index;
        self.s.improvements.iter_mut().find(|r| r.tile == i)
    }

    pub fn is_city(&self, c: Wc) -> bool {
        self.tile(c).is_some_and(|t| t.improvement == improvement::CITY)
    }

    pub fn city_tiles(&self, player: u8) -> Vec<Wc> {
        self.s
            .map
            .tiles
            .iter()
            .filter(|t| t.owner == player && t.improvement == improvement::CITY)
            .map(|t| t.coordinates)
            .collect()
    }

    pub fn city_count(&self, player: u8) -> i32 {
        self.s.map.tiles.iter().filter(|t| t.owner == player && t.improvement == improvement::CITY).count() as i32
    }

    /// The city counter kept on the player's record: set to one when the capital is placed, raised by a capture or
    /// when the player's border claims a village, and lowered for the previous owner of a captured city. Tech prices,
    /// the AI's unit need, research and improvement scores and the opinion ranking read it rather than counting the
    /// map, so a game file whose counter differs from its map keeps that difference. Falls back to the map count for
    /// a player without a record.
    pub fn recorded_city_count(&self, player: u8) -> i32 {
        self.player_runtime(player).map_or_else(|| self.city_count(player), |r| r.cities)
    }

    /// The start tile while the player owns it,
    /// else the first of the player's cities in map order.
    pub fn capital_of(&self, player: u8) -> Option<Wc> {
        let p = self.player(player)?;
        let st = p.start_tile;
        let t = self.tile(st)?;
        if t.owner == player {
            return Some(st);
        }
        if crate::env_var!("OLD_CAPITAL").is_ok() {
            return None;
        }
        self.city_tiles(player).first().copied()
    }

    pub fn has_effect(&self, c: Wc, effect: u16) -> bool {
        self.tile(c).is_some_and(|t| t.effects & (1 << effect) != 0)
    }

    pub const EFFECT_FLOODED: u16 = 1;
    pub const EFFECT_SWAMPED: u16 = 2;
    pub const EFFECT_TENTACLE: u16 = 3;
    pub const EFFECT_ALGAE: u16 = 4;
    pub const EFFECT_FOAM: u16 = 5;

    pub fn is_wetland_index(&self, tile_index: i32) -> bool {
        self.s
            .map
            .tiles
            .get(tile_index as usize)
            .is_some_and(|t| t.effects & ((1 << Self::EFFECT_FLOODED) | (1 << Self::EFFECT_ALGAE)) != 0)
    }

    /// The direction a bridge on tile `tile_index` cannot be crossed in, for `player`'s units. A bridge runs along
    /// direction 3 when both vertical neighbours are anchors, else along direction 5 when both horizontal ones are,
    /// and the blocked direction is the other of the two; with no anchor pair it is 8. An anchor is a tile that is
    /// not water, and before version 104 it must also be explored by the player.
    pub fn blocked_bridge_direction(&self, player: u8, tile_index: i32) -> i32 {
        let Some(at) = self.s.map.tiles.get(tile_index as usize).map(|t| t.coordinates) else { return 8 };
        let anchor = |dx: i32, dy: i32| {
            let c = Wc { x: at.x + dx, y: at.y + dy };
            self.tile(c).is_some_and(|t| {
                !crate::rules::path::is_water(t.terrain) && (self.s.version > 103 || self.is_explored_at(player, c))
            })
        };
        if anchor(0, -1) && anchor(0, 1) {
            5
        } else if anchor(-1, 0) && anchor(1, 0) {
            3
        } else {
            8
        }
    }

    // ---------- techs and unlocks ----------

    pub fn techs(&self, player: u8) -> &[i32] {
        self.s.tech.iter().find(|r| r.player == player).map(|r| r.tech.as_slice()).unwrap_or(&[])
    }

    pub fn has_tech(&self, player: u8, tech: i32) -> bool {
        self.techs(player).contains(&tech)
    }

    fn tribe_of(&self, player: u8) -> i32 {
        self.player(player).map(|p| p.tribe).unwrap_or(0)
    }

    /// Replace a unit name through the tribe's `unit_overrides` and return its idx.
    pub fn tribe_unit(&self, player: u8, name: &str) -> Option<i32> {
        let tribe = self.t.tribe(self.tribe_of(player))?;
        let n = tribe.unit_overrides.get(name).map(String::as_str).unwrap_or(name);
        self.t.unit_idx.get(n).copied()
    }

    pub fn tribe_improvement(&self, player: u8, name: &str) -> Option<i32> {
        let tribe = self.t.tribe(self.tribe_of(player))?;
        let n = tribe.improvement_overrides.get(name).map(String::as_str).unwrap_or(name);
        self.t.improvement_idx.get(n).copied()
    }

    /// Units the player has unlocked, optionally including hidden ones.
    pub fn unlocked_units(&self, player: u8, include_hidden: bool) -> Vec<i32> {
        let mut out: Vec<i32> = Vec::new();
        let mut seen = crate::rules::economy::IdSet::default();
        let tt = self.t.tribe_tech(self.tribe_of(player));
        for &tech in self.techs(player) {
            for &idx in tt.units(tech) {
                let Some(info) = self.t.unit(idx) else { continue };
                if !include_hidden && info.data.hidden {
                    continue;
                }
                if idx == 0 || !seen.insert(idx) {
                    continue;
                }
                out.push(idx);
            }
        }
        out
    }

    /// Improvements the player has unlocked.
    pub fn unlocked_improvements(&self, player: u8) -> Vec<i32> {
        let mut out: Vec<i32> = Vec::new();
        let mut seen = crate::rules::economy::IdSet::default();
        let tt = self.t.tribe_tech(self.tribe_of(player));
        for &tech in self.techs(player) {
            for &idx in tt.improvements(tech) {
                if idx == 0 || !seen.insert(idx) {
                    continue;
                }
                out.push(idx);
            }
        }
        for idx in self.task_improvements(player) {
            if seen.insert(idx) {
                out.push(idx);
            }
        }
        out
    }

    pub fn has_player_ability(&self, player: u8, ability: i32) -> bool {
        if let Some(bits) = self.shared_ability_bits(player) {
            return (0..64).contains(&ability) && bits & (1 << ability) != 0;
        }
        self.techs(player)
            .iter()
            .any(|&tech| self.t.tech_info(tech).is_some_and(|i| i.ability_mask.contains(&i.abilities, ability)))
    }

    pub fn has_tribe_ability(&self, player: u8, name: &str) -> bool {
        self.t.tribe(self.tribe_of(player)).is_some_and(|t| t.tribe_abilities.iter().any(|a| a == name))
    }

    /// Terrain ids the player's unlocked techs let units enter.
    pub fn unlocked_movement_terrain(&self, player: u8) -> Vec<i32> {
        self.movement_terrain_list(player).to_vec()
    }

    /// [`Engine::unlocked_movement_terrain`] without the allocation, shared within a legal-command listing.
    pub(crate) fn movement_terrain_list(&self, player: u8) -> TerrainList {
        self.shared_movement_terrain(player).unwrap_or_else(|| self.unlocked_movement_terrain_of(player))
    }

    fn unlocked_movement_terrain_of(&self, player: u8) -> TerrainList {
        let mut out = TerrainList::new();
        for &tech in self.techs(player) {
            let Some(info) = self.t.tech_info(tech) else { continue };
            for &id in &info.movement {
                out.push_new(id);
            }
        }
        out
    }

    /// Terrain the unit may enter.
    pub fn allowed_terrain(&self, unit: &UnitState) -> Vec<i32> {
        self.allowed_terrain_list(unit).to_vec()
    }

    /// [`Engine::allowed_terrain`] without the allocation.
    pub(crate) fn allowed_terrain_list(&self, unit: &UnitState) -> TerrainList {
        let Some(info) = self.t.unit(unit.type_idx) else { return TerrainList::new() };
        let mut allowed: TerrainList = info.terrain.iter().copied().collect();
        if info.terrain.is_empty() {
            allowed = self.movement_terrain_list(unit.owner);
            if info.abilities.contains(&unit_ability::WATER) && !info.abilities.contains(&unit_ability::LAND) {
                allowed.retain(|t| matches!(t, terrain::WATER | terrain::OCEAN | terrain::WETLAND));
            }
            if info.abilities.contains(&unit_ability::FLY) {
                for t in 1..=8 {
                    allowed.push_new(t);
                }
                return allowed;
            }
        }
        // A swimmer may always enter shallow water, whatever its owner has researched: a boat converted by a player
        // without Fishing still sails.
        if info.abilities.contains(&unit_ability::SWIM) {
            allowed.push_new(terrain::WATER);
        }
        allowed
    }

    pub fn tech_price(&self, player: u8, tech: i32) -> i32 {
        let base = self.t.tech(tech).map(|t| t.cost).unwrap_or(0);
        let literacy = self.has_player_ability(player, player_ability::LITERACY);
        crate::rules::economy::tech_price(base, self.recorded_city_count(player), literacy)
    }

    /// Techs the player can research now.
    pub fn researchable(&self, player: u8) -> Vec<i32> {
        let tribe = self.tribe_of(player);
        let owned = self.techs(player);
        crate::rules::economy::unlockable_tech(tribe, self.tech_nodes(tribe), owned)
    }

    /// The tribe's tech tree (built once per tribe in [`Tables`]).
    pub(crate) fn tech_nodes(&self, tribe: i32) -> &[crate::rules::economy::TechNode] {
        &self.t.tribe_tech(tribe).nodes
    }

    // ---------- unit stats ----------

    pub fn unit_info(&self, unit: &UnitState) -> Option<&UnitInfo> {
        self.t.unit(unit.type_idx)
    }

    pub fn has_ability(&self, unit: &UnitState, ability: i32) -> bool {
        self.t.has_unit_ability(unit.type_idx, ability)
    }

    pub fn max_health(&self, unit: &UnitState) -> i32 {
        // Max health: a carrier without Protect (39) uses its passenger's.
        if crate::env_var!("OLD_MAXHP").is_err() {
            if let Some(p) = unit.passenger.as_ref() {
                if !self.has_ability(unit, 39) {
                    return self.max_health(p);
                }
            }
        }
        let base = self.unit_info(unit).map(|i| i.data.health).unwrap_or(0);
        crate::rules::combat::promoted_max_health(base, i32::from(unit.promotion))
    }

    pub fn unit_attack(&self, unit: &UnitState) -> i32 {
        let raw = self.unit_info(unit).map(|i| i.data.attack).unwrap_or(0);
        crate::rules::combat::scaled_attack(raw, unit.boosted)
    }

    pub fn unit_movement(&self, unit: &UnitState) -> i32 {
        let raw = self.unit_info(unit).map(|i| i.data.movement).unwrap_or(0);
        crate::rules::path::movement_points(self.s.version, raw, unit)
    }

    pub fn unit_range(&self, unit: &UnitState) -> i32 {
        self.unit_info(unit).map(|i| i.data.range).unwrap_or(0)
    }

    pub fn sight_range(&self, unit: &UnitState) -> i32 {
        1 + i32::from(self.has_ability(unit, unit_ability::SCOUT))
    }

    /// Defence bonus of the unit.
    pub fn defence_bonus(&self, unit: &UnitState) -> i32 {
        use crate::rules::combat::{defence_bonus, DefenceBonusInput};
        let tile = self.tile(unit.coordinates);
        let mut input = DefenceBonusInput {
            version: self.s.version,
            poisoned: unit.poisoned,
            tile_present: tile.is_some(),
            owner: unit.owner,
            player_resolved: self.player(unit.owner).is_some(),
            unlocked_defence_bonus: 1,
            improvement_known: false,
            is_city: false,
            same_owner: false,
            has_fortify: self.has_ability(unit, unit_ability::FORTIFY),
            has_city_wall: false,
        };
        if let Some(tile) = tile {
            input.unlocked_defence_bonus = self.t.unlocked_defence_bonus(self.techs(unit.owner), tile.terrain);
            input.improvement_known = tile.improvement != 0;
            input.is_city = tile.improvement == improvement::CITY;
            input.same_owner = tile.owner == unit.owner;
            // `defence_bonus` reads the wall only for a fortifying unit in a city of its owner.
            input.has_city_wall = input.is_city
                && input.same_owner
                && input.has_fortify
                && self
                    .improvement_rt(tile.coordinates)
                    .is_some_and(|r| r.rewards.contains(&crate::CityReward::CityWall));
        }
        defence_bonus(&input)
    }

    pub fn combat_unit(&self, unit: &UnitState) -> crate::rules::combat::CombatUnit {
        let info = self.unit_info(unit);
        let (raw_defence, raw_attack, range, weapon) =
            info.map(|i| (i.data.defence, i.data.attack, i.data.range, i.data.weapon)).unwrap_or((0, 0, 0, 0));
        let bonus = self.defence_bonus(unit);
        let ab = |a: i32| self.has_ability(unit, a);
        crate::rules::combat::CombatUnit {
            health: i32::from(unit.health),
            max_health: self.max_health(unit),
            attack: self.unit_attack(unit),
            defence: raw_defence * bonus,
            defence_bonus: bonus,
            raw_attack,
            raw_defence,
            range,
            weapon,
            has_stiff: ab(unit_ability::STIFF),
            has_convert: ab(unit_ability::CONVERT),
            has_surprise: ab(unit_ability::SURPRISE),
            has_freeze: ab(unit_ability::FREEZE),
            has_tentacles: ab(unit_ability::TENTACLES),
            has_leader: unit.leader != 0,
            has_infiltrate: ab(unit_ability::INFILTRATE),
            frozen: unit.frozen,
            invisible: unit.invisible,
        }
    }

    // ---------- diplomacy ----------

    pub fn at_peace(&self, a: u8, b: u8) -> bool {
        crate::rules::diplomacy::has_peace_with(&self.s, a, b)
    }

    pub fn is_alive(&self, player: u8) -> bool {
        crate::rules::diplomacy::is_alive(&self.s, player)
    }

    pub fn living_players(&self) -> Vec<u8> {
        let alive = crate::rules::diplomacy::alive_by_id(&self.s);
        self.s
            .players
            .iter()
            .map(|p| p.id)
            .filter(|&id| id != NATURE && alive.map_or_else(|| self.is_alive(id), |a| a[usize::from(id)]))
            .collect()
    }

    // ---------- scoring ----------

    pub fn add_score(&mut self, player: u8, delta: i32) {
        if let Some(p) = self.player_mut(player) {
            p.score = crate::rules::economy::add_score(p.score, delta);
        }
    }

    pub fn add_currency(&mut self, player: u8, delta: i32) {
        if crate::env_var!("CUR_LOG").is_ok() {
            let cur = self.player(player).map(|p| p.currency).unwrap_or(0);
            eprintln!(
                "ENGCUR cmd={} turn={} p{} {} -> {}",
                self.s.current_command,
                self.s.current_turn,
                player,
                cur,
                (cur + delta).max(0)
            );
        }
        if let Some(p) = self.player_mut(player) {
            p.currency += delta;
        }
        if delta > 0 {
            self.check_wealth(player);
        }
    }

    pub fn next_unit_id(&mut self) -> u32 {
        self.s.current_unit_id += 1;
        if crate::env_var!("ID_DEBUG").is_ok() {
            eprintln!("ID {} at cmd {}", self.s.current_unit_id, self.s.current_command);
        }
        self.s.current_unit_id
    }

    /// Whether the game has ended; [`Engine::game_winners`] then gives the winning players.
    pub fn is_over(&self) -> bool {
        self.s.current_state == crate::GameStatus::Ended
    }
}

/// Ends the unit grid it began (also when unwinding from a panic).
pub(crate) struct GridScope<'a>(&'a GameState, bool);

impl Drop for GridScope<'_> {
    fn drop(&mut self) {
        if self.1 {
            self.0.unit_grid_end();
        }
    }
}

/// See [`Engine::query_scope`].
pub struct QueryScope<'a> {
    _grid: GridScope<'a>,
    _zoc: legal::ZocScope<'a>,
}

struct GridScopeMut<'a>(&'a mut Engine, bool, bool);

impl Drop for GridScopeMut<'_> {
    fn drop(&mut self) {
        if self.2 {
            #[cfg(feature = "grid_check")]
            if !std::thread::panicking() {
                self.0.zoc_share_check();
            }
            legal::zoc_share_end();
        }
        if self.1 {
            #[cfg(feature = "grid_check")]
            self.0.s.unit_grid_check();
            self.0.s.unit_grid_end();
        }
    }
}
