//! Infiltration: an attack by an Infiltrate unit queues an infiltrate action,
//! which kills the infiltrator and runs the infiltration reward.

use super::*;
use crate::hash::HashFunction;
use crate::rules::combat::get_battle_results;

/// City reward ids for Rebellion and Poison.
const REWARD_REBELLION: i32 = 11;
const REWARD_POISON: i32 = 13;
/// Unit type ids read by the reward.
const UNIT_DAGGER: i32 = 37;
const UNIT_PIRATE: i32 = 40;
const UNIT_POISON_SPAWN: i32 = 60;
const UNIT_POISON_INFILTRATOR: i32 = 61;

/// Most units one infiltration reward spawns (a level 5 city's border).
pub const INFILTRATE_SPAWN_MAX: usize = 5;

/// The spawns and payment of an infiltration reward still to run: the tiles in order (`next` is the first one
/// left), the unit types, and the stars to pay once the last unit is placed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct InfiltrateRun {
    pub player: u8,
    pub city: Wc,
    pub land: i32,
    pub water: Option<i32>,
    /// Number of tiles of `tiles` in use.
    pub len: u8,
    pub next: u8,
    pub pay: i32,
    pub tiles: [Wc; INFILTRATE_SPAWN_MAX],
    /// The tile of a poisoned infiltrator, whose spores (and algae on water) grow after the spawns and payment.
    pub spores: Option<Wc>,
}

impl Engine {
    pub fn is_infiltrator(&self, unit: &UnitState) -> bool {
        self.has_ability(unit, unit_ability::INFILTRATE)
            || unit.passenger.as_ref().is_some_and(|p| self.has_ability(p, unit_ability::INFILTRATE))
    }

    /// The infiltrating attack: the unit is spent, the attack counts against the city owner, then the reward runs.
    pub fn do_infiltrate(&mut self, unit_id: u32, target: Wc) {
        let Some(i) = self.unit_index(unit_id) else { return };
        self.s.units[i].moved = true;
        self.s.units[i].attacked = true;
        let unit = self.s.units[i].clone();
        let owner = unit.owner;
        let city_owner = self.tile(target).map(|t| t.owner).unwrap_or(0);
        self.note_attack(owner, city_owner);
        // Infiltrate: kill the infiltrator, then the reward for its type. The reward is queued above the spore
        // actions of the kill, so it picks its spawn tiles before a poisoned infiltrator's tile turns to algae.
        let kind = unit.type_idx;
        let spores = self.kill_unit_deferring_spores(unit_id);
        let reward = if kind == UNIT_POISON_INFILTRATOR { REWARD_POISON } else { REWARD_REBELLION };
        self.infiltration_reward(owner, target, reward, kind, spores);
    }

    /// How much an infiltration reward values a tile.
    fn infiltration_tile_value(&self, player: u8, at: Wc) -> f32 {
        let Some(tile) = self.tile(at).copied() else { return 0.0 };
        let h = self.random_hash();
        let seed = h.get_hash3(at.x, at.y, self.s.current_turn as i32);
        let mut v = 0.1f32 * h.value1(seed as i32) + 0.0;
        v += 0.0;
        if tile.improvement == improvement::CITY {
            v += 2.0;
        }
        if crate::rules::path::is_water(tile.terrain) {
            v += -0.1;
        }
        if self.get_resource(at, player).is_some() {
            v += 0.5;
        }
        if self.player_terrain_defence_bonus(player, tile.terrain) >= 2 {
            v += 1.0;
        }
        if tile.improvement != 0
            && self.t.has_improvement_ability(tile.improvement, improvement_ability::POISON)
            && !self.has_tribe_ability(player, "poisonresist")
        {
            v += -0.5;
        }
        v
    }

    /// Defence bonus of a terrain for a player: the first unlocked tech listing the terrain, else 1.
    fn player_terrain_defence_bonus(&self, player: u8, terrain: i32) -> i32 {
        self.t.unlocked_defence_bonus(self.techs(player), terrain)
    }

    /// The infiltration reward.
    fn infiltration_reward(&mut self, player: u8, city: Wc, reward: i32, unit_type: i32, spores: Option<Wc>) {
        let rt = self
            .tile(city)
            .filter(|t| t.improvement != 0 && self.player(player).is_some())
            .and_then(|_| self.improvement_rt(city).cloned());
        let Some(rt) = rt else {
            if let Some(at) = spores {
                self.spawn_spore_resource(at, Some(player));
            }
            return;
        };
        let level = i32::from(rt.level);
        let border = i32::from(rt.border_size);
        // Rebellion pays the city's raw production (queued first, so it runs after the spawns).
        let pay = if reward == REWARD_REBELLION { self.raw_production(city) } else { 0 };
        if let Some(r) = self.improvement_rt_mut(city) {
            r.robbed = true;
        }
        // A probe unit of the infiltrator's type standing in the city fights any defender.
        let health = self.t.unit(unit_type).map(|u| u.data.health as u16).unwrap_or(100);
        let probe = UnitState {
            id: 0,
            owner: player,
            type_idx: unit_type,
            coordinates: city,
            previous_turn_end: city,
            home: city,
            health,
            ..UnitState::default()
        };
        let mut battle: Option<i32> = None;
        let mut defender_survives = false;
        if let Some(def) = self.unit_at(city).cloned() {
            let q = self.battle_query_pub(&probe, &def, false);
            let r = get_battle_results(&self.combat_unit(&probe), &self.combat_unit(&def), &q);
            battle = Some(r.attack_damage);
            defender_survives = r.attack_damage < i32::from(def.health);
        }
        // Spawn types.
        let tribe = self.player(player).map(|p| p.tribe).unwrap_or(0);
        let water_ok = self.unlocked_movement_terrain(player).contains(&terrain::WATER);
        let (land_type, water_type) = if reward == REWARD_POISON {
            (UNIT_POISON_SPAWN, None)
        } else {
            (UNIT_DAGGER, if water_ok { Some(UNIT_PIRATE) } else { None })
        };
        let land_type = self.tribe_override_unit(tribe, land_type);
        let water_type = water_type.map(|w| self.tribe_override_unit(tribe, w));
        let accessible = |e: &Engine, ty: i32, at: Wc| e.infiltration_accessible(player, city, ty, at);
        let mut cands: Vec<Wc> = Vec::new();
        for c in self.area(city, border) {
            if c == city {
                continue;
            }
            let Some(t) = self.tile(c) else { continue };
            if t.ruling_city != city || self.unit_at(c).is_some() {
                continue;
            }
            let land = accessible(self, land_type, c);
            let water = water_type.is_some_and(|w| accessible(self, w, c));
            if land || water {
                cands.push(c);
            }
        }
        if !defender_survives {
            cands.push(city);
        }
        let mut sorted: Vec<(Wc, f32)> = cands.iter().map(|c| (*c, self.infiltration_tile_value(player, *c))).collect();
        // Sorted by value, highest first.
        sorted.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
        let count = level.min(5).min(sorted.len() as i32).max(0) as usize;
        // The attack runs at once; the queued spawns run after it, index 0 first.
        if let Some(dmg) = battle {
            self.perform_attack_pub(Wc { x: -1, y: -1 }, city, dmg);
            // A poison infiltration poisons the defender that survives the hit (from version 118).
            if reward == REWARD_POISON && defender_survives && self.s.version > 0x75 {
                self.poison_unit_action(player, city);
            }
        }
        let mut run = InfiltrateRun {
            player,
            city,
            land: land_type,
            water: water_type,
            len: 0,
            next: 0,
            pay,
            tiles: [city; INFILTRATE_SPAWN_MAX],
            spores,
        };
        for (at, _) in sorted.into_iter().take(count.min(INFILTRATE_SPAWN_MAX)) {
            run.tiles[run.len as usize] = at;
            run.len += 1;
        }
        self.continue_infiltrate(&mut run);
    }

    /// Whether a unit of type `ty` could stand on `at` (the city's tile is where it would come from).
    fn infiltration_accessible(&self, player: u8, city: Wc, ty: i32, at: Wc) -> bool {
        let u = UnitState {
            id: 0,
            owner: player,
            type_idx: ty,
            coordinates: city,
            previous_turn_end: city,
            home: city,
            ..UnitState::default()
        };
        let allowed = self.allowed_terrain(&u);
        let abilities = self.unit_info(&u).map(|i| i.abilities.clone()).unwrap_or_default();
        let settings = self.path_settings(&u, &allowed, &abilities, false);
        let Some(t) = self.tile(at) else { return false };
        let blocked = |i| self.blocked_bridge_direction(player, i);
        crate::rules::path::is_tile_accessible(
            &self.s,
            t,
            t,
            &settings,
            &|i| self.is_wetland_index(i),
            &|k, a| self.t.has_improvement_ability(k, a),
            &blocked,
        )
    }

    /// Run the queued spawns of an infiltration reward, then the payment, then the infiltrator's spores. Each spawn is an action of its own on
    /// retail's stack: a city reward raised by one (the unit's sight can reveal a map corner, which levels a city up)
    /// holds the spawns and the payment still to come until the reward is answered.
    pub(super) fn continue_infiltrate(&mut self, run: &mut InfiltrateRun) {
        while run.next < run.len {
            if self.reward_pending() {
                self.hold(Deferred::Infiltrate(*run));
                return;
            }
            let below = self.deferred.len();
            let at = run.tiles[run.next as usize];
            run.next += 1;
            let water_tile = self.tile(at).is_some_and(|t| crate::rules::path::is_water(t.terrain));
            // A spawn on the city's own tile (a poison reward's egg) is homed there like a trained unit; an independent
            // dagger keeps no home.
            let independent =
                self.t.unit(run.land).is_some_and(|u| u.data.unit_abilities.iter().any(|a| a == "independent"));
            let home = if independent { Wc { x: -1, y: -1 } } else { self.trained_unit_home(at) };
            if let Some(id) = self.spawn_unit(run.player, run.land, at, home, true) {
                // Retail's train action checks the tile for poison like a step does, so a spawn on a poisonous
                // improvement (the daggers go wherever the city's border allows) starts poisoned.
                if let Some(spawned) = self.unit(id).cloned() {
                    self.check_step_on_poison(&spawned, at);
                }
                // The spawn is a train action given the city as its look coordinates: the unit turns to face it.
                if let Some(i) = self.unit_index(id) {
                    self.s.units[i].direction = Self::direction_of(self.s.units[i].coordinates, run.city);
                }
                if run.water.is_some()
                    && water_tile
                    && !self.infiltration_accessible(run.player, run.city, run.land, at)
                {
                    self.embark_pub(id);
                }
            }
            if self.reward_pending() && (run.next < run.len || run.pay > 0 || run.spores.is_some()) {
                // What is left sits under the score and reward actions this spawn raised.
                self.hold_at.get_or_insert(below);
                self.hold(Deferred::Infiltrate(*run));
                return;
            }
        }
        if run.pay > 0 {
            if self.reward_pending() {
                self.hold(Deferred::Infiltrate(*run));
                return;
            }
            self.add_currency(run.player, run.pay);
            run.pay = 0;
        }
        if let Some(at) = run.spores {
            if self.reward_pending() {
                self.hold(Deferred::Infiltrate(*run));
                return;
            }
            run.spores = None;
            self.spawn_spore_resource(at, Some(run.player));
        }
    }

    /// Raw production: a city makes level + production, one less away from
    /// its owner's capital.
    fn raw_production(&self, at: Wc) -> i32 {
        let Some(t) = self.tile(at).copied() else { return 0 };
        let Some(rt) = self.improvement_rt(at) else { return 0 };
        if t.improvement != improvement::CITY || t.owner == 0 {
            return 0;
        }
        let capital = self.player(t.owner).is_some_and(|p| p.start_tile == at);
        i32::from(rt.level) + i32::from(rt.production) - if capital { 0 } else { 1 }
    }

    fn tribe_override_unit(&self, tribe: i32, unit_type: i32) -> i32 {
        let Some(info) = self.t.unit(unit_type) else { return unit_type };
        let name = info.data.id.clone();
        self.t
            .tribe(tribe)
            .and_then(|t| t.unit_overrides.get(&name).cloned())
            .and_then(|n| self.t.unit_idx.get(n.as_str()).copied())
            .unwrap_or(unit_type)
    }
}
