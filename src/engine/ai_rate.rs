//! Position and battle rating and the helpers they use.

use super::ai_state::*;
use super::*;
use crate::ai::score::{
    self, AttackerTile, CityThreatUnit, ImprovementPositionInput, PositionNeighbor, PositionNeighborUnit,
    RateBattleInput, RatePositionInput,
};
use crate::hash::HashFunction;
use crate::rules::combat::get_battle_results;
use crate::rules::path;

impl Engine {
    // ---------- shared queries ----------

    pub fn full_cost(&self, u: &UnitState) -> i32 {
        let own = self.unit_info(u).map(|i| i.data.cost).unwrap_or(0);
        let pass = u.passenger.as_ref().and_then(|p| self.unit_info(p)).map(|i| i.data.cost);
        own.wrapping_add(pass.unwrap_or(0))
    }

    pub fn has_follower(&self, u: &UnitState) -> bool {
        u.follower != 0
    }

    /// Whether some unit on the 8 surrounding tiles is invisible to this
    /// unit's owner (Invisible effect, another owner, and that owner not at peace with the viewer).
    pub fn is_detecting_hidden(&self, u: &UnitState) -> bool {
        self.area_iter(u.coordinates, 1).any(|n| {
            self.unit_at(n).is_some_and(|o| o.invisible && o.owner != u.owner && !self.at_peace(u.owner, o.owner))
        })
    }

    /// Whether a resource is visible to the player.
    pub fn resource_visible_to(&self, resource: i32, player: u8) -> bool {
        let requires = |imp: i32| self.t.improvement(imp).is_some_and(|i| i.resource_req.contains(&resource));
        // `unlocked_improvements(player)` without building the list: tech unlocks (idx 0 skipped), then task unlocks.
        let tribe = self.tribe_of(player);
        let tt = self.t.tribe_tech(tribe);
        let techs = self.techs(player);
        if techs.iter().any(|&t| tt.improvements(t).iter().any(|&i| i != 0 && requires(i))) {
            return true;
        }
        if self.task_improvements(player).into_iter().any(requires) {
            return true;
        }
        // Improvements one tech away.
        crate::rules::economy::unlockable_tech(tribe, &tt.nodes, techs)
            .into_iter()
            .any(|tech| tt.improvements_raw(tech).iter().any(|&imp| requires(imp)))
    }

    /// Improvements whose requirement names the resource.
    pub fn improvements_for_resource(&self, resource: i32) -> Vec<i32> {
        let mut out = Vec::new();
        for (idx, info) in self.t.improvements.iter().enumerate() {
            if let Some(info) = info {
                if info.resource_req.contains(&resource) {
                    out.push(idx as i32);
                }
            }
        }
        out
    }

    pub fn improvement_unlocked(&self, player: u8, imp: i32) -> bool {
        self.unlocked_improvements(player).contains(&imp)
    }

    /// Whether the population the city area could still gain covers the next level.
    ///
    /// The available population is cached per city (the last population check): it is recomputed only when the
    /// stored key is not above `turn + action_stack_size` (the stack is empty between commands), so it stays
    /// stale for the rest of a turn.
    pub fn can_city_be_upgraded(&self, at: Wc) -> bool {
        let Some(tile) = self.tile(at) else { return false };
        if tile.improvement != improvement::CITY {
            return false;
        }
        let Some(rt) = self.improvement_rt(at) else { return false };
        let key = i64::from(self.s.current_turn);
        let cached = self.pop_cache.borrow().get(&(at.x, at.y)).copied();
        let available = match cached {
            Some((k, v)) if k > key && crate::env_var!("NO_POP_CACHE").is_err() => v,
            _ => {
                let v = self.compute_available_population(at, rt.border_size, tile.owner);
                if crate::env_var!("POPC_DBG").is_ok() {
                    eprintln!(
                        "POPC cmd {} turn {} city ({},{}) owner {} avail {} need {}",
                        self.s.current_command,
                        self.s.current_turn,
                        at.x,
                        at.y,
                        tile.owner,
                        v,
                        i32::from(rt.level) - i32::from(rt.xp) + 1
                    );
                }
                self.pop_cache.borrow_mut().insert((at.x, at.y), (key + 1, v));
                v
            }
        };
        (i32::from(rt.level) - i32::from(rt.xp) + 1) <= available
    }

    fn compute_available_population(&self, at: Wc, radius: u16, owner: u8) -> i32 {
        let kinds = self.unlocked_improvements(owner);
        let mut total = 0i32;
        let mut limited_max: Vec<(i32, i32)> = Vec::new();
        for c in self.area_iter(at, i32::from(radius)) {
            let Some(t) = self.tile(c) else { continue };
            if t.improvement != 0 || t.ruling_city != at {
                continue;
            }
            for &k in &kinds {
                if self.can_build(owner, c, k, i32::MAX) {
                    let pop = self.population_at(c, k);
                    let limited = self.t.has_improvement_ability(k, improvement_ability::LIMITED)
                        || self.t.has_improvement_ability(k, improvement_ability::UNIQUE);
                    if !limited {
                        total += pop;
                    } else {
                        match limited_max.iter_mut().find(|e| e.0 == k) {
                            Some(e) => e.1 = e.1.max(pop),
                            None => limited_max.push((k, pop.max(0))),
                        }
                    }
                }
            }
        }
        if crate::env_var!("POP_DEBUG").is_ok() {
            eprintln!(
                "AVAILPOP city ({},{}) radius {} total {} limited {:?} kinds {:?}",
                at.x, at.y, radius, total, limited_max, kinds
            );
        }
        total + limited_max.iter().map(|e| e.1).sum::<i32>()
    }

    pub fn population_at(&self, at: Wc, kind: i32) -> i32 {
        let Some(info) = self.t.improvement(kind) else { return 0 };
        let mut pop = info.population;
        if !info.data.adjacency_improvements.is_empty() {
            let bonus = self.adjacency_bonus(at, kind);
            for g in &info.data.growth_rewards {
                pop += g.population.unwrap_or(0) * bonus;
            }
        }
        pop
    }

    // ---------- attack / movement option helpers ----------

    /// Attack options of a unit with an explicit range.
    pub fn attack_targets_with_range(&self, unit: &UnitState, range: i32, ignore_diplomacy: bool) -> Vec<Wc> {
        let Some(info) = self.unit_info(unit) else { return Vec::new() };
        path::attack_options(
            &self.s,
            unit,
            range,
            ignore_diplomacy,
            info.data.attack,
            &info.abilities,
            false,
            |i| path::tile_being_captured(&self.s, i),
            |i| self.s.map.tiles.get(i as usize).is_some_and(|t| t.terrain == terrain::FIELD && t.improvement == 0),
        )
    }

    /// Attack options from a given position (the unit need not stand there).
    pub fn attack_targets_at(&self, unit: &UnitState, position: Wc, range: i32) -> Vec<Wc> {
        let Some(info) = self.unit_info(unit) else { return Vec::new() };
        path::attack_options_at(
            &self.s,
            unit.owner,
            position,
            range,
            false,
            unit,
            &info.abilities,
            false,
            |i| path::tile_being_captured(&self.s, i),
            |_| false,
        )
    }

    /// Movement options of a unit: no check of the moved flag.
    pub fn movement_options_ignoring_state(&self, unit: &UnitState) -> Vec<Wc> {
        let Some(info) = self.unit_info(unit) else { return Vec::new() };
        let allowed = self.allowed_terrain(unit);
        let abilities = info.abilities.as_slice();
        let settings = self.path_settings(unit, &allowed, abilities, false);
        let movement = self.unit_movement(unit);
        let step = self.step_ctx_search(unit, abilities, false);
        path::get_move_options(
            &self.s,
            unit.coordinates,
            movement,
            unit,
            &settings,
            None,
            |i| self.is_wetland_index(i),
            |kind, ability| self.t.has_improvement_ability(kind, ability),
            |_| 8,
            |a, b| self.step_cost_with(&step, a, b),
        )
    }

    // ---------- military statistics ----------

    /// Add one unit's contribution to the player's own military statistics table.
    pub(super) fn collect_military_stats_from_tile(&mut self, player: u8, unit_index: usize) {
        let u = self.s.units[unit_index].clone();
        let mut stats = std::mem::take(&mut self.ai_states.entry(player).or_default().player_military_stats);
        Engine::add_unit_stats(&mut stats, self, &u);
        self.ai_states.entry(player).or_default().player_military_stats = stats;
    }

    /// Adjust the AI's military needs from the collected statistics.
    pub(super) fn adjust_military_needs(&mut self, player: u8) {
        let stats = self.ai(player).player_military_stats.clone();
        let mut needs = MilitaryStats::default();
        let mut own = MilitaryStats::default();
        for (pid, st) in &stats {
            if *pid == player {
                own = *st;
                continue;
            }
            if self.player(*pid).is_none() {
                continue;
            }
            let peace = self.at_peace(player, *pid);
            let war = crate::rules::diplomacy::has_war_with(&self.s, player, *pid);
            let factor = if war {
                2.0f32
            } else if peace {
                0.0
            } else {
                1.0
            };
            needs.movement += st.range.max(0.0) * factor;
            needs.range += st.defense.max(0.0) * 0.5 * factor;
            needs.attack += st.defense.max(0.0) * factor;
            needs.defense += st.movement.max(0.0) * factor;
            needs.defense += factor * (st.attack.max(0.0) * 0.25);
        }
        needs.defense -= own.defense;
        needs.attack -= own.attack;
        needs.movement -= own.movement;
        needs.range -= own.range;
        needs.update_normal();
        self.ai_states.entry(player).or_default().military_needs = needs;
    }

    // ---------- battle rating ----------

    /// Rate a battle between `attacker` and the unit on `defender_at`.
    pub fn rate_battle(&self, attacker: &UnitState, defender_at: Wc) -> f32 {
        let input = self.rate_battle_input(attacker, defender_at);
        if input.defender_present && matches!(input.attacker_tile, AttackerTile::Missing) {
            return 0.0;
        }
        score::rate_battle(&input)
    }

    // ---------- position rating ----------

    /// Threat level of a city tile.
    pub fn city_threat_level(&self, at: Wc) -> f32 {
        let Some(tile) = self.tile(at) else { return 0.0 };
        let is_city = tile.improvement == improvement::CITY;
        let owner = tile.owner;
        let mut units = Vec::new();
        for c in self.area_iter(at, 3) {
            let Some(t) = self.tile(c) else { continue };
            let explored = self.is_explored(owner, t.index as usize);
            let Some(u) = self.unit_at(c) else { continue };
            units.push(CityThreatUnit {
                explored,
                same_owner: u.owner == owner,
                chebyshev: Engine::chebyshev(at, c),
                movement: self.unit_movement(u),
                has_dash: self.has_ability(u, unit_ability::DASH),
                health: u.health,
                max_health: self.max_health(u),
                attack: self.unit_attack(u),
            });
        }
        score::get_city_threat_level(is_city, &units)
    }

    /// The number of threatened cities with no defender.
    fn threatened_unguarded_cities(&self, cities: &[Wc]) -> i32 {
        let mut n = 0;
        for c in cities {
            if self.unit_at(*c).is_none() && self.city_threat_level(*c) > 0.0 {
                n += 1;
            }
        }
        n
    }

    /// Whether one of the player's other units is adjacent and could take the tile.
    fn can_ally_capture(&self, player: u8, at: Wc) -> bool {
        for &id in &self.ai(player).units {
            let Some(u) = self.unit(id) else { continue };
            if Engine::chebyshev(u.coordinates, at) <= 1 && self.can_move(u) && self.can_capture_ex(u, at, true, false)
            {
                return true;
            }
        }
        false
    }

    /// Whether the unit can capture the tile, optionally counting next turn and only the unit's own tile.
    pub fn can_capture_ex(&self, unit: &UnitState, at: Wc, include_next_turn: bool, only_on_tile: bool) -> bool {
        let Some(tile) = self.tile(at) else { return false };
        if unit.owner == NATURE {
            return false;
        }
        if !include_next_turn && (!self.can_move(unit) || !self.can_attack(unit)) {
            return false;
        }
        if only_on_tile && tile.coordinates != unit.coordinates {
            return false;
        }
        if unit.leader != 0 {
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
        tile.improvement != 0 && tile.owner != unit.owner
    }

    /// Position score for missions.
    fn position_score_for_missions(&self, player: u8, here: Wc, unit: &UnitState) -> f32 {
        let ai = self.ai(player);
        let mut list = Vec::new();
        for m in &ai.missions {
            let can_capture = if m.kind == score::MissionType::Capture as i32 {
                self.can_capture_ex(unit, m.tile, true, false)
            } else {
                true
            };
            let manhattan = (here.x - m.tile.x).abs() + (here.y - m.tile.y).abs();
            if crate::env_var!("MISS_CAP").is_ok() {
                eprintln!(
                    "MCAP cmd {} unit {} here ({},{}) mission ({},{}) kind {} score {} can_capture {}",
                    self.s.current_command, unit.id, here.x, here.y, m.tile.x, m.tile.y, m.kind, m.score, can_capture
                );
            }
            list.push((
                score::Mission {
                    kind: if m.kind == 1 { score::MissionType::Capture } else { score::MissionType::None },
                    tile: m.tile,
                    score: m.score,
                },
                manhattan,
                can_capture,
            ));
        }
        score::get_position_score_for_missions(here, &list)
    }

    /// Position score for an improvement.
    fn position_score_for_improvement(&self, player: u8, at: Wc, unit: &UnitState, hate: i32) -> f32 {
        let tile = self.tile(at).copied().unwrap_or_default();
        let ai = self.ai(player);
        let p = self.player(player).copied();
        let poison_improvement = self.t.has_improvement_ability(tile.improvement, improvement_ability::POISON);
        // `score::get_position_score_for_improvement` reads the threat only for an own city, the defence terms only
        // when that threat is positive, and the capture terms only for another player's city; the pure queries
        // behind them are evaluated only on those paths (unread fields are left at zero).
        let is_city = tile.improvement == improvement::CITY;
        let own_city = is_city && tile.owner == player;
        let threat = if own_city { self.city_threat_level(at) } else { 0.0 };
        let threatened = own_city && threat > 0.0;
        let cb = threatened.then(|| self.combat_unit(unit));
        let can_capture = is_city && !own_city && self.can_capture_ex(unit, at, true, false);
        let input = ImprovementPositionInput {
            is_city,
            owned_by_player: tile.owner == player,
            hate_towards_owner: hate,
            can_capture,
            can_ally_capture: is_city && !own_city && !can_capture && self.can_ally_capture(player, at),
            threat,
            unguarded_cities: if threatened { self.threatened_unguarded_cities(&ai.city_tiles) } else { 0 },
            can_city_support_unit: threatened && self.can_city_support_unit(at),
            currency: p.map(|p| p.currency).unwrap_or(0),
            defence: cb.map_or(0, |c| c.defence),
            defence_bonus: cb.map_or(0, |c| c.defence_bonus),
            health: unit.health,
            max_health: if threatened { self.max_health(unit) } else { 0 },
            unit_count: ai.units.len() as i32,
            city_count: ai.city_tiles.len() as i32,
            improvement_present: tile.improvement != 0,
            improvement_has_poison: poison_improvement,
            player_has_decompose: self.has_tribe_ability(player, "poisonresist"),
            unit_present: true,
            unit_poisoned: unit.poisoned,
        };
        score::get_position_score_for_improvement(&input)
    }

    /// Rate a position for a unit.
    pub fn rate_position(&self, player: u8, unit_id: u32, at: Wc, can_attack: bool) -> f32 {
        let Some(unit) = self.unit(unit_id) else { return 0.0 };
        self.rate_position_unit(player, unit, at, can_attack)
    }

    /// Rate a position for a unit object (real or hypothetical).
    pub fn rate_position_unit(&self, player: u8, unit: &UnitState, at: Wc, can_attack: bool) -> f32 {
        let Some(tile) = self.tile(at).copied() else { return 0.0 };
        let ab = |a: i32| self.has_ability(unit, a);
        let owner = tile.owner;
        let peace_owner = owner != 0 && self.at_peace(player, owner);
        let opinion_owner = if owner != 0 && peace_owner { self.opinion(player, owner) } else { 0.0 };
        let attacks: Vec<(f32, i32)> = if can_attack {
            let mut v = Vec::new();
            if let Some(info) = self.unit_info(unit) {
                for target in self.attack_targets_at(unit, at, info.data.range) {
                    if let Some(d) = self.unit_at(target) {
                        if crate::env_var!("ATK_DUMP").is_ok() {
                            eprintln!("ATKDUMP cmd {} unit {} from ({},{}) target ({},{}) def {} owner {} type {} hp {} rate {} agg {}", self.s.current_command, unit.id, at.x, at.y, target.x, target.y, d.id, d.owner, d.type_idx, d.health, self.rate_battle(unit, target), self.aggression(player, d.owner));
                        }
                        let agg_to = if crate::env_var!("OLD_ATK_AGG").is_ok() { d.owner } else { owner };
                        v.push((self.rate_battle(unit, target), self.aggression(player, agg_to)));
                    }
                }
            }
            v
        } else {
            Vec::new()
        };
        let mut neighbors = Vec::new();
        for n in self.area_iter(at, 1) {
            let Some(nt) = self.tile(n) else { continue };
            let explored = self.is_explored(player, nt.index as usize);
            let unit_here = if explored { self.visible_unit_for(n, player) } else { None };
            let nu = unit_here.map(|u| {
                let same_owner = u.owner == player;
                let at_peace = !same_owner && self.at_peace(player, u.owner);
                // `score::rate_position` reads `detecting_hidden` only for own or at-peace units and `battle` only
                // for the others, so each is computed only where it is read (both are pure queries).
                let friendly = same_owner || at_peace;
                PositionNeighborUnit {
                    same_owner,
                    at_peace,
                    detecting_hidden: friendly && self.is_detecting_hidden(u),
                    has_freeze_area: self.has_ability(u, unit_ability::FREEZE_AREA),
                    tile_freezable: self.is_freezable(n, player),
                    has_convert: self.has_ability(u, unit_ability::CONVERT),
                    battle: if friendly { RateBattleInput::UNUSED } else { self.rate_battle_input(u, at) },
                }
            });
            neighbors.push(PositionNeighbor { explored, unit: nu });
        }
        let at_war_owner = owner != 0 && owner != player && !peace_owner;
        let hate = if at_war_owner { self.aggression(player, owner) } else { 0 };
        let improvement_score = if tile.improvement != 0 {
            Some(self.position_score_for_improvement(player, at, unit, hate))
        } else {
            None
        };
        let tentacle_attack_damage = None; // tentacle owners are not modelled
        let hash = self.random_hash().get_hash3(i32::from(self.s.current_command).wrapping_mul(1000), at.x, at.y);
        let p = self.player(player).copied();
        let input = RatePositionInput {
            tile_owner: owner,
            player_id: player,
            at_peace_with_tile_owner: peace_owner,
            opinion_of_tile_owner: opinion_owner,
            can_attack,
            attacks,
            has_escape: ab(unit_ability::ESCAPE),
            has_auto_freeze: ab(unit_ability::AUTO_FREEZE),
            has_freeze_area: ab(unit_ability::FREEZE_AREA),
            has_dash: ab(unit_ability::DASH),
            has_invisible: unit.invisible,
            has_skate: ab(unit_ability::SKATE),
            has_amphibious: ab(unit_ability::AMPHIBIOUS),
            neighbors,
            mission_score: self.position_score_for_missions(player, at, unit),
            hate,
            improvement_score,
            tentacle_attack_damage,
            unit_health: unit.health,
            terrain_is_ice: tile.terrain == terrain::ICE,
            tile_is_land: path::is_land(tile.terrain),
            tile_is_flooded: tile.effects & (1 << Self::EFFECT_FLOODED) != 0,
            position_hash: hash,
            auto_play: p.map(|p| p.auto_play).unwrap_or(false),
            handicap: p.map(|p| p.handicap).unwrap_or(0),
        };
        let result = score::rate_position(&input);
        if crate::env_var!("RATE_MISS").is_ok() {
            let ai = self.ai(player);
            eprintln!(
                "RMISS cmd {} unit {} at ({},{}) missions {:?}",
                self.s.current_command,
                unit.id,
                at.x,
                at.y,
                ai.missions.iter().map(|m| (m.kind, m.tile.x, m.tile.y, m.score)).collect::<Vec<_>>()
            );
        }
        if crate::env_var!("RATE_DUMP").is_ok() {
            eprintln!("RDUMP cmd {} unit {} upos ({},{}) tile ({},{}) can {} res {:.6} mission {:.6} impr {:?} hate {} owner {} noise {:.6} nb {}", self.s.current_command, unit.id, unit.coordinates.x, unit.coordinates.y, at.x, at.y, can_attack as u8, result, input.mission_score, input.improvement_score, input.hate, tile.owner, (input.position_hash as f32) * f32::from_bits(0x2f800000) - 0.5, input.neighbors.len());
        }
        if crate::env_var!("RATE_DEBUG").is_ok() {
            let unexplored = input.neighbors.iter().filter(|n| !n.explored).count();
            if crate::env_var!("MISSION_DEBUG").is_ok() {
                eprintln!("  missions {:?}", self.ai(player).missions);
            }
            let sample = (input.position_hash as f32) * f32::from_bits(0x2f800000) + -0.5;
            eprintln!("RATEDBG unit {} at ({},{}) mission {:.4} impr {:?} unexplored {} neighbors {} hash {} noise {:.4} stupidity-handicap {} auto {} -> {:.4}", unit.id, at.x, at.y, input.mission_score, input.improvement_score, unexplored, input.neighbors.len(), input.position_hash, sample * 50.0, input.handicap, input.auto_play, result);
        }
        result
    }

    /// Whether the player's freezing could change the tile: water, a flooded tile, a tile of another climate or skin,
    /// or a tile holding a unit the player could freeze (see `docs/freezing.md`).
    pub fn is_freezable(&self, at: Wc, player: u8) -> bool {
        self.tile_freezable(at, player, false)
    }

    /// The `RateBattleInput` the game builds for `(attacker, defending tile)`.
    pub fn rate_battle_input(&self, attacker: &UnitState, defender_at: Wc) -> RateBattleInput {
        let defender = self.unit_at(defender_at);
        let tile = self.tile(defender_at);
        let (attack_damage, retaliation_damage) = match defender {
            Some(d) => {
                let ac = self.combat_unit(attacker);
                let dc = self.combat_unit(d);
                let q = self.battle_damage_query(attacker, d);
                let r = get_battle_results(&ac, &dc, &q);
                (r.attack_damage, r.retaliation_damage)
            }
            None => (0, 0),
        };
        let atk_info = self.unit_info(attacker);
        let def_info = defender.and_then(|d| self.unit_info(d));
        RateBattleInput {
            attacker_has_infiltrate: self.has_ability(attacker, unit_ability::INFILTRATE),
            defender_is_opponent_city: tile
                .is_some_and(|t| t.improvement == improvement::CITY && t.owner != 0 && t.owner != attacker.owner),
            defender_present: defender.is_some(),
            attack_damage,
            retaliation_damage,
            attacker_full_cost: self.full_cost(attacker),
            defender_full_cost: defender.map(|d| self.full_cost(d)).unwrap_or(0),
            attacker_cost: def_info.map(|i| i.data.cost).unwrap_or(0),
            attacker_max_health: self.max_health(attacker),
            defender_health: defender.map(|d| i32::from(d.health)).unwrap_or(0),
            defender_defence: defender.map(|d| self.combat_unit(d).defence).unwrap_or(0),
            attacker_data_has_poison: atk_info.is_some_and(|i| i.abilities.contains(&unit_ability::POISON)),
            attacker_data_has_eat: atk_info.is_some_and(|i| i.abilities.contains(&unit_ability::EAT)),
            attacker_data_has_persist: atk_info.is_some_and(|i| i.abilities.contains(&unit_ability::PERSIST)),
            attacker_state_has_poison: self.has_ability(attacker, unit_ability::POISON),
            attacker_state_has_freeze: self.has_ability(attacker, unit_ability::FREEZE),
            attacker_state_has_convert: self.has_ability(attacker, unit_ability::CONVERT),
            defender_data_has_poison: def_info.is_some_and(|i| i.abilities.contains(&unit_ability::POISON)),
            attacker_poisoned: attacker.poisoned,
            defender_poisoned: defender.is_some_and(|d| d.poisoned),
            defender_has_follower: defender.is_some_and(|d| self.has_follower(d)),
            // Read by `score::rate_battle` only when the defender is poisoned.
            spores_visible: defender.is_some_and(|d| d.poisoned) && self.resource_visible_to(7, attacker.owner),
            attacker_tile: match self.tile(attacker.coordinates) {
                Some(t) => AttackerTile::Present {
                    is_city: t.improvement == improvement::CITY,
                    is_water: path::is_water(t.terrain),
                    owned_by_attacker: t.owner == attacker.owner,
                },
                None => AttackerTile::Missing,
            },
        }
    }
}
