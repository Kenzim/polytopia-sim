//! The AI move driver: unit passes, then tile commands, else end of turn.

use super::*;
use crate::ai::decide::{self, attack_option_score, AttackFacts};
use crate::ai::score::{
    self, CityReward as AiReward, DesiredUnitInput, FreezeScanTile, NormalizedMilitary, TrainCity, TrainOffer,
    UnitStats,
};
use crate::hash::HashFunction;
use crate::rules::{combat::get_battle_results, path};

/// The Polaris tribe's id.
const POLARIS_TRIBE: i32 = 16;

impl Engine {
    /// Refresh the AI's player map data at the top of a move.
    fn refresh_map_data(&mut self, me: u8) {
        // The player map data's unit list is filled by scanning the map, so units come in tile-index order.
        let mut owned: Vec<(i32, u32)> = self
            .s
            .units
            .iter()
            .filter(|u| u.owner == me)
            .map(|u| (self.tile(u.coordinates).map(|t| t.index).unwrap_or(i32::MAX), u.id))
            .collect();
        owned.sort();
        let units: Vec<u32> = owned.into_iter().map(|x| x.1).collect();
        let empire: Vec<Wc> = self.s.map.tiles.iter().filter(|t| t.owner == me).map(|t| t.coordinates).collect();
        let cities: Vec<Wc> = self
            .s
            .map
            .tiles
            .iter()
            .filter(|t| t.owner == me && t.improvement == improvement::CITY)
            .map(|t| t.coordinates)
            .collect();
        let st = self.ai_states.entry(me).or_default();
        st.units = units;
        st.empire_tiles = empire;
        st.city_tiles = cities;
    }

    #[doc(hidden)]
    pub fn ai_analysis_rates(&self, me: u8) -> Vec<(u32, i32, i32, f32)> {
        self.ai(me).analysis_rates.clone()
    }
    #[doc(hidden)]
    pub fn clear_missions_pub(&mut self, me: u8) {
        self.ai_mut(me).missions.clear();
    }
    #[doc(hidden)]
    pub fn refresh_map_data_pub(&mut self, me: u8) {
        self.refresh_map_data(me);
    }
    #[doc(hidden)]
    pub fn analyze_situation_pub(&mut self, me: u8) {
        self.analyze_situation(me);
    }

    /// The command the retail AI would issue next for the current player.
    pub fn ai_get_move(&mut self) -> Command {
        // Choosing a move changes AI state, opinions and caches but never the unit list.
        self.with_unit_grid(Self::ai_get_move_inner)
    }

    /// Apply the command the AI picks for the current player. Returns false when the game is over.
    pub fn step_ai(&mut self, per_turn: &mut u32) -> bool {
        self.step_ai_cmd(per_turn).is_some()
    }

    /// [`Engine::step_ai`] that also returns the command played (`None` when the game is over). `per_turn` counts the
    /// commands of the current player's turn: past 400, or on an illegal pick, the AI ends its turn instead.
    pub fn step_ai_cmd(&mut self, per_turn: &mut u32) -> Option<Command> {
        if self.is_over() {
            return None;
        }
        let mut cmd = self.ai_get_move();
        *per_turn += 1;
        if *per_turn > 400 {
            cmd = Command::EndTurn;
        }
        if !matches!(cmd, Command::EndTurn | Command::Stay { .. }) && !self.is_legal(&cmd) {
            cmd = Command::EndTurn;
        }
        let before = self.s.current_turn;
        let player = self.current_player();
        self.apply_unchecked(cmd);
        if matches!(cmd, Command::EndTurn) || self.s.current_turn != before || self.current_player() != player {
            *per_turn = 0;
        }
        Some(cmd)
    }

    fn ai_get_move_inner(&mut self) -> Command {
        let me = self.current_player();
        if let Some(c) = self.ai_trigger_command(me) {
            return c;
        }
        let handicap = self.player(me).map(|p| p.handicap).unwrap_or(0);
        // In FinalTurn the AI ends its turn at once (an end of turn there runs the end-of-turn effects and ends the game).
        let final_turn = self.s.current_state == crate::GameStatus::FinalTurn;
        if me == NATURE
            || self.is_over()
            || final_turn
            || handicap == decide::handicap_from_difficulty(crate::BotDifficulty::Frozen)
        {
            return Command::EndTurn;
        }
        self.refresh_map_data(me);
        self.analyze_situation(me);
        let units = self.ai(me).units.clone();
        for priority_only in [true, false] {
            for &id in &units {
                let Some(unit) = self.unit(id).cloned() else { continue };
                let Some(info) = self.unit_info(&unit) else { continue };
                let range = self.unit_range(&unit);
                let surprise = info.abilities.contains(&unit_ability::SURPRISE);
                if priority_only && !decide::unit_in_priority_pass(range, surprise) {
                    continue;
                }
                // The "can perform any action" check counts attack options without a can-attack check.
                let n_attack = self.attack_targets(&unit).len() as i32;
                let promotable = self.can_be_promoted(&unit);
                // The "can perform any ability" check follows, after its hurt check.
                let later = (self.can_heal_others(&unit) && !self.heal_options(me, unit.coordinates).is_empty())
                    || promotable
                    || self.can_freeze_area(&unit)
                    || self.can_break_ice(&unit)
                    || self.can_explode(&unit)
                    || (self.has_player_ability(me, crate::engine::tables::player_ability::DISBAND)
                        && !unit.moved
                        && !unit.attacked);
                let currency = self.player(me).map(|p| p.currency).unwrap_or(0);
                let can_upgrade = self
                    .unlocked_upgrades_for(me, &unit)
                    .into_iter()
                    .any(|t| self.t.unit(t).is_some_and(|i| currency >= i.data.cost));
                let max_health = self.max_health(&unit);
                let abil = info.abilities.clone();
                if !decide::can_perform_any_action(&unit, max_health, &abil, later, can_upgrade, promotable, n_attack) {
                    continue;
                }
                if let Some(c) = self.best_unit_move(&unit) {
                    return c;
                }
                // The best-unit-move step may re-run situation analysis between units in retail only through the command counter.
            }
        }
        if let Some(c) = self.tile_commands(me) {
            return c;
        }
        Command::EndTurn
    }

    /// Answer the newest pending trigger, a city reward or a peace request.
    fn ai_trigger_command(&self, me: u8) -> Option<Command> {
        let trigger = self.pending_trigger(me)?;
        Some(match trigger.peace_from {
            Some(with) => Command::PeaceResponse { with, accept: self.should_accept_peace(me, with) },
            None => self.ai_choose_city_reward(trigger.city),
        })
    }

    /// Choose the reward for a city level-up.
    fn ai_choose_city_reward(&self, city: Wc) -> Command {
        let me = self.current_player();
        let level = self.improvement_rt(city).map(|r| r.level).unwrap_or(0);
        let rewards: Vec<crate::CityReward> = self.reward_options(level);
        let ai_rewards: Vec<AiReward> = rewards.iter().map(|r| to_ai_reward(*r)).collect();
        let hash = self.random_hash().get_hash3(city.x, city.y, i32::from(self.s.current_command)) as i32;
        let pick = score::choose_city_reward(&ai_rewards, hash);
        let reward = rewards.iter().copied().find(|r| to_ai_reward(*r) == pick).unwrap_or(rewards[0]);
        let _ = me;
        Command::CityReward { city, reward }
    }

    /// Every command the AI would score for the current player, best first (for planning, no noise).
    pub fn ai_candidates(&mut self) -> Vec<(Command, f32)> {
        self.with_unit_grid(Self::ai_candidates_inner)
    }

    fn ai_candidates_inner(&mut self) -> Vec<(Command, f32)> {
        let me = self.current_player();
        if let Some(c) = self.ai_trigger_command(me) {
            return vec![(c, 0.0)];
        }
        self.refresh_map_data(me);
        self.analyze_situation(me);
        let mut all = Vec::new();
        for id in self.ai(me).units.clone() {
            let Some(unit) = self.unit(id).cloned() else { continue };
            all.extend(self.unit_move_candidates(&unit));
        }
        all.extend(self.tile_command_candidates(me));
        ai_state::sort_desc_by_score(&mut all, |c| c.1);
        all
    }

    /// Sort descending by score with the .NET introsort, add noise, pick index 0.
    fn pick_best_command(&self, me: u8, mut cmds: Vec<(Command, f32)>) -> Option<Command> {
        if cmds.is_empty() {
            return None;
        }
        for (i, c) in cmds.iter_mut().enumerate() {
            let n = self.stupid_factor(me, decide::pick_noise_y(i as i32), 0);
            if crate::env_var!("AI_DEBUG").is_ok() {
                eprintln!("  noise cmd {} idx {} score {} noise {} -> {:?}", self.s.current_command, i, c.1, n, c.0);
            }
            c.1 += n;
        }
        ai_state::sort_desc_by_score(&mut cmds, |c| c.1);
        // Only the head of the sorted list is looked at: an invalid (unaffordable) head ends the turn.
        let head = cmds[0].0;
        if !self.ai_head_valid(me, &head) {
            return None;
        }
        Some(head)
    }

    fn ai_head_valid(&self, me: u8, cmd: &Command) -> bool {
        let currency = self.player(me).map(|p| p.currency).unwrap_or(0);
        match *cmd {
            Command::Research { tech } => self.tech_price(me, tech) <= currency,
            Command::Build { improvement, .. } => {
                self.t.improvement(improvement).and_then(|i| i.data.cost).unwrap_or(0) <= currency
            }
            _ => true,
        }
    }

    /// Debug helper: where does the recorded command rank among this step's noisy candidates?
    pub fn ai_rank_report(&mut self, recorded: &Command) {
        let me = self.current_player();
        if self.pending_trigger(me).is_some() {
            return;
        }
        self.refresh_map_data(me);
        self.analyze_situation(me);
        // Unit moves are examined unit by unit in `ai_get_move`; report only tile commands here.
        let mut v = self.tile_command_candidates(me);
        let mut noisy: Vec<(Command, f32, f32)> = v
            .drain(..)
            .enumerate()
            .map(|(i, (c, s))| {
                let n = self.stupid_factor(me, decide::pick_noise_y(i as i32), 0);
                (c, s, n)
            })
            .collect();
        noisy.sort_by(|a, b| (b.1 + b.2).partial_cmp(&(a.1 + a.2)).unwrap());
        let pos = noisy.iter().position(|c| &c.0 == recorded);
        match pos {
            Some(p) => eprintln!(
                "RANK cmd {} p{} recorded {:?} rank {} adjusted {:.1} (raw {:.1}) best {:?} adjusted {:.1} (raw {:.1})",
                self.s.current_command,
                me,
                recorded,
                p,
                noisy[p].1 + noisy[p].2,
                noisy[p].1,
                noisy[0].0,
                noisy[0].1 + noisy[0].2,
                noisy[0].1
            ),
            None => eprintln!(
                "RANK cmd {} p{} recorded {:?} NOT IN LIST (best {:?} adjusted {:.1})",
                self.s.current_command,
                me,
                recorded,
                noisy.first().map(|c| &c.0),
                noisy.first().map(|c| c.1 + c.2).unwrap_or(0.0)
            ),
        }
    }

    /// The best move for one unit.
    fn best_unit_move(&self, unit: &UnitState) -> Option<Command> {
        let cmds = self.unit_move_candidates(unit);
        if crate::env_var!("AI_DEBUG").is_ok() {
            let mut v = cmds.clone();
            ai_state::sort_desc_by_score(&mut v, |c| c.1);
            v.truncate(8);
            eprintln!(
                "AI_UNIT cmd {} unit {} at {:?} cands {:?}",
                self.s.current_command, unit.id, unit.coordinates, v
            );
        }
        self.pick_best_command(unit.owner, cmds)
    }

    /// Every scored command the best-unit-move step considers, before noise.
    pub fn unit_move_candidates(&self, unit: &UnitState) -> Vec<(Command, f32)> {
        let me = unit.owner;
        let id = unit.id;
        let at = unit.coordinates;
        let current = self.rate_position_unit(me, unit, at, false);
        let mut cmds: Vec<(Command, f32)> = Vec::new();
        let ai = self.ai(me);
        if self.can_attack(unit) {
            for target in self.attack_targets(unit) {
                let Some(defender) = self.unit_at(target) else { continue };
                let mut battle = self.rate_battle(unit, target);
                for e in &ai.hit_list {
                    if e.unit == defender.id {
                        battle += e.score;
                    }
                }
                let q = self.battle_query_pub(unit, defender, false);
                let r = get_battle_results(&self.combat_unit(unit), &self.combat_unit(defender), &q);
                let facts = AttackFacts {
                    battle_sum: battle,
                    adjust_for_position: r.should_move_to_defeated_enemy_tile,
                    position_score: if r.should_move_to_defeated_enemy_tile {
                        self.rate_position_unit(me, unit, target, false)
                    } else {
                        0.0
                    },
                    aggression: self.aggression(me, defender.owner),
                    infiltrate_city: false,
                };
                if let Some(s) = attack_option_score(current, &facts) {
                    cmds.push((Command::Attack { unit: id, target }, s));
                }
            }
        }
        if self.can_move(unit) {
            cmds.push((Command::Stay { unit: id }, current * 0.8));
            let can_atk = self.can_attack(unit);
            let carry = self.has_ability(unit, unit_ability::CARRY);
            let cost = self.unit_info(unit).map(|i| i.data.cost).unwrap_or(0);
            let origin_water = self.is_water_tile(at);
            for to in self.move_destinations(unit) {
                let mut s = self.rate_position_unit(me, unit, to, can_atk);
                if carry && origin_water && !self.is_water_tile(to) {
                    s += cost as f32 * -10.0;
                }
                s += 5.0;
                cmds.push((Command::Move { unit: id, to }, s));
            }
        }
        self.add_unit_action_options(unit, current, &mut cmds);
        cmds
    }

    /// Score the unit actions the game offers, in its order (Capture,
    /// ExamineRuins, Recover, HealOthers, Disembark, Promote, FreezeArea, BreakIce, Upgrade, Disband, Boost, Swarm,
    /// Explode, manual Build, ClearTileEffect). Disembark, Upgrade, Disband and ClearTileEffect get no score here.
    fn add_unit_action_options(&self, unit: &UnitState, current: f32, cmds: &mut Vec<(Command, f32)>) {
        let me = unit.owner;
        let id = unit.id;
        let at = unit.coordinates;
        if self.can_capture(unit, at) {
            let owner = self.tile(at).map(|t| t.owner).unwrap_or(0);
            // `aggression / 2 + 400` in integer arithmetic, then converted.
            let agg = self.aggression(me, owner);
            cmds.push((Command::Capture { unit: id, at }, (agg / 2 + 400) as f32));
        }
        if self.can_examine_ruins(unit) {
            cmds.push((Command::ExamineRuins { unit: id }, 500.0));
        }
        if self.can_recover(unit) {
            cmds.push((Command::Recover { unit: id }, self.heal_amount(unit) as f32 * 0.1 + current));
        }
        if self.can_heal_others(unit) {
            let mut sum = 0.0f32;
            for c in self.heal_options(me, at) {
                if let Some(t) = self.unit_at(c) {
                    sum += self.heal_amount(t) as f32 * 0.1;
                }
            }
            if sum > 0.0 {
                cmds.push((Command::HealOthers { unit: id }, sum * 50.0));
            }
        }
        if self.can_be_promoted(unit) {
            cmds.push((Command::Promote { unit: id }, 5000.0));
        }
        // FreezeArea scores 50 per freezable tile of the square round the unit, its own tile included. BreakIce
        // scores 50 per breakable ice tile beside it, and a Polaris player never does it.
        if self.can_freeze_area(unit) {
            let n =
                self.area_iter(at, 1).chain(std::iter::once(at)).filter(|&c| self.tile_freezable(c, me, false)).count()
                    as i32;
            cmds.push((Command::FreezeArea { unit: id }, (n * 50) as f32));
        }
        if self.can_break_ice(unit) && self.player(me).is_some_and(|p| p.tribe != POLARIS_TRIBE) {
            let n = self.area_iter(at, 1).filter(|&c| self.has_breakable_ice(c, me)).count() as i32;
            cmds.push((Command::BreakIce { unit: id }, (n * 50) as f32));
        }
        // Boost and Swarm sit inside the can-attack and owner-is-current-player check of the unit actions; both score
        // `50 * boost options` when there is at least one option.
        if self.can_attack(unit) && me == self.current_player() {
            let n = self.boost_options(me, at).len() as i32;
            if n > 0 {
                if self.has_ability(unit, unit_ability::BOOST) {
                    cmds.push((Command::Boost { unit: id }, (n * 50) as f32));
                }
                if self.has_ability(unit, unit_ability::SWARM) {
                    cmds.push((Command::Swarm { unit: id }, (n * 50) as f32));
                }
            }
        }
        if self.can_explode(unit) {
            if let Some(s) = self.explode_score(unit) {
                cmds.push((Command::Explode { unit: id }, s));
            }
        }
        // Can-build (needs can-attack and can-move): every unlocked Manual improvement the unit can build on
        // the unit's tile, scored `improvement score * 10` when positive.
        if self.can_attack(unit) && self.can_move(unit) {
            let currency = self.player(me).map(|p| p.currency).unwrap_or(0);
            for kind in self.unlocked_improvements(me) {
                if !self.t.has_improvement_ability(kind, improvement_ability::MANUAL)
                    || !self.can_build(me, at, kind, currency)
                {
                    continue;
                }
                let s = self.ai_improvement_score(me, at, kind);
                if s > 0.0 {
                    cmds.push((Command::Build { at, improvement: kind }, s * 10.0));
                }
            }
        }
    }

    /// The Explode branch of the unit action options: the blocked-tile value on water, -1 with a leader, -2 with a
    /// follower, +2 without spores on the tile, plus `cost * damage / max_health` for every visible enemy in radius 1;
    /// when positive, minus the unit's own `cost * health / max_health`; kept `* 100` when still positive.
    fn explode_score(&self, unit: &UnitState) -> Option<f32> {
        let me = unit.owner;
        let at = unit.coordinates;
        let mut s = 0.0f32;
        if self.is_water_tile(at) {
            if let Some(v) = self.ai(me).blocked_tiles.get(&(at.x, at.y)) {
                s = *v + 0.0;
            }
        }
        if unit.leader != 0 {
            s += -1.0;
        }
        if unit.follower != 0 {
            s += -2.0;
        }
        if self.tile(at).map(|t| t.resource) != Some(7) {
            s += 2.0;
        }
        for n in self.area_iter(at, 1) {
            let Some(v) = self.visible_unit_for(n, me) else { continue };
            if v.owner == me {
                continue;
            }
            let q = self.battle_damage_query(unit, v);
            let r = get_battle_results(&self.combat_unit(unit), &self.combat_unit(v), &q);
            let cost = self.unit_info(v).map(|i| i.data.cost).unwrap_or(0);
            s += cost as f32 * (r.attack_damage as f32 / self.max_health(v) as f32);
        }
        if s <= 0.0 {
            return None;
        }
        let own = self.unit_info(unit).map(|i| i.data.cost).unwrap_or(0);
        s -= own as f32 * (f32::from(unit.health) / self.max_health(unit) as f32);
        (s > 0.0).then_some(s * 100.0)
    }

    /// Input builder for the desired unit stats.
    pub fn desired_stats(&self, me: u8, tile: Option<Wc>) -> UnitStats {
        let ai = self.ai(me);
        let n = ai.military_needs.get_normal();
        let infiltrate_units =
            self.s.units.iter().filter(|u| u.owner == me && self.has_ability(u, unit_ability::INFILTRATE)).count()
                as i32;
        let tribe = self.player(me).map(|p| p.tribe).unwrap_or(0);
        let climate = self.t.tribe(tribe).map(|t| t.climate).unwrap_or(0);
        let tribe_freeze = self.tribe_has_unit_ability(me, unit_ability::FREEZE);
        let freeze_scan = match tile {
            Some(c) if tribe_freeze => Some(
                self.area_iter(c, 3)
                    .chain(std::iter::once(c))
                    .filter_map(|a| self.tile(a).copied())
                    .map(|t| FreezeScanTile {
                        explored: self.is_explored(me, t.index as usize),
                        climate: t.climate,
                        owner: t.owner,
                        is_water: path::is_water(t.terrain),
                        friendly_freeze_area: self
                            .unit_at(t.coordinates)
                            .is_some_and(|u| u.owner == me && self.has_ability(u, unit_ability::FREEZE_AREA)),
                        is_ice: t.terrain == terrain::ICE,
                    })
                    .collect(),
            ),
            _ => None,
        };
        score::get_desired_unit_stats(&DesiredUnitInput {
            normalized: NormalizedMilitary { defence: n[0], attack: n[1], movement: n[2], range: n[3] },
            enemy_population: ai.enemy_population,
            infiltrate_units,
            player_climate: climate,
            freeze_scan,
        })
    }

    /// Whether some unit the tribe's tech tree can unlock has the ability.
    fn tribe_has_unit_ability(&self, me: u8, ability: i32) -> bool {
        // Every tech's `unit_unlocks` through `tribe_unit`, resolved once per tribe in the tables.
        self.t.tribe_tech(self.tribe_of(me)).unit_abilities.contains(&ability)
    }

    fn train_offer(&self, me: u8, city: Wc, unit_type: i32, position: bool, valid: bool) -> Option<TrainOffer> {
        let info = self.t.unit(unit_type)?;
        let position_score = if position && crate::env_var!("TRAIN_POS0").is_err() {
            let u = UnitState {
                id: 0,
                owner: me,
                type_idx: unit_type,
                coordinates: city,
                previous_turn_end: city,
                home: city,
                health: info.data.health as u16,
                ..UnitState::default()
            };
            Some(self.rate_position_unit(me, &u, city, false))
        } else {
            None
        };
        Some(TrainOffer {
            player: me,
            unit_type,
            at: city,
            cost: info.data.cost,
            defence: info.data.defence,
            attack: info.data.attack,
            movement: info.data.movement,
            range: info.data.range,
            abilities: info.abilities.clone(),
            valid,
            position_score,
        })
    }

    /// The AI's tile commands: score the candidates and pick the best.
    fn tile_commands(&mut self, me: u8) -> Option<Command> {
        let scored = self.tile_command_candidates(me);
        if crate::env_var!("AI_DEBUG").is_ok() {
            let mut v = scored.clone();
            ai_state::sort_desc_by_score(&mut v, |c| c.1);
            eprintln!(
                "AI_DEBUG cmd {} p{} stars {} candidates {:?}",
                self.s.current_command,
                me,
                self.player(me).map(|p| p.currency).unwrap_or(0),
                v
            );
        }
        self.pick_best_command(me, scored)
    }

    /// Every scored tile command, before noise. Also updates `unit_need`.
    pub fn tile_command_candidates(&mut self, me: u8) -> Vec<(Command, f32)> {
        let ai = self.ai(me);
        let cities = self.recorded_city_count(me);
        let units = self.s.units.iter().filter(|u| u.owner == me).count() as i32;
        let need =
            decide::unit_need(cities, ai.explored_tiles, ai.hit_list.len() as i32, ai.missions.len() as i32, units);
        // Research is scored before the unit-training step refreshes `unit_need`, so it sees the previous value.
        let prev_need = ai.unit_need;
        let city_tiles = ai.city_tiles.clone();
        let currency = self.player(me).map(|p| p.currency).unwrap_or(0);

        let mut scored: Vec<(Command, f32)> = Vec::new();
        // Research.
        scored
            .extend(self.research_candidates(me, if crate::env_var!("FRESH_NEED").is_ok() { need } else { prev_need }));
        self.ai_mut_unit_need(me, need);
        for _ in 0..crate::env_var!("SHIFT_A").ok().and_then(|v| v.parse::<usize>().ok()).unwrap_or(0) {
            scored.push((Command::EndTurn, -1.0e9));
        }
        // Training and upgrades: every explored map tile is walked in index order, scoring
        // the city's trainable units, then the Upgrade actions of an own unit standing there.
        let tile_list: Vec<Wc> =
            self.s.map.tiles.iter().filter(|t| self.is_explored_at(me, t.coordinates)).map(|t| t.coordinates).collect();
        for at in tile_list {
            let is_own_city = city_tiles.contains(&at);
            // `desired_stats` is a pure query, so it is evaluated only where it is read (own cities, upgrades).
            let mut desired: Option<UnitStats> = None;
            if is_own_city {
                let mut d = self.desired_stats(me, Some(at));
                let mut legal = Vec::new();
                self.train_commands_pub(me, at, &mut legal);
                let threat = self.city_threat_level(at);
                let trains: Vec<TrainOffer> = legal
                    .into_iter()
                    .filter_map(|c| {
                        if let Command::Train { unit_type, .. } = c {
                            self.train_offer(me, at, unit_type, true, true)
                        } else {
                            None
                        }
                    })
                    .collect();
                if !trains.is_empty() && threat > 0.0 {
                    d.defence += threat * 20.0;
                }
                desired = Some(d);
                let tc = TrainCity {
                    explored: true,
                    threat,
                    desired: self.desired_stats(me, Some(at)),
                    trains,
                    upgrades: Vec::new(),
                };
                for c in score::add_possible_train_unit_commands(currency, need, &[tc]) {
                    if let score::TileCommand::Train { unit_type, at, .. } = c.command {
                        scored.push((Command::Train { city: at, unit_type }, c.score));
                    }
                }
            }
            if crate::env_var!("NO_AI_UPGRADE").is_ok() {
                continue;
            }
            let Some(unit) = self.unit_at(at).filter(|u| u.owner == me).cloned() else { continue };
            let mut actions = Vec::new();
            self.unit_commands_pub(&unit, &mut actions);
            let targets = self.unlocked_upgrades_for(me, &unit);
            for a in actions {
                let Command::Upgrade { .. } = a else { continue };
                let desired = desired.get_or_insert_with(|| self.desired_stats(me, Some(at)));
                for &t in &targets {
                    let Some(offer) = self.train_offer(me, at, t, true, true) else { continue };
                    let built = score::get_build_unit_score(
                        offer.defence,
                        offer.attack,
                        offer.movement,
                        offer.range,
                        desired,
                        &offer.abilities,
                        offer.position_score,
                        need,
                    );
                    scored.push((a, built * score::price_factor(offer.cost, currency)));
                }
            }
        }
        for _ in 0..crate::env_var!("SHIFT_B").ok().and_then(|v| v.parse::<usize>().ok()).unwrap_or(0) {
            scored.push((Command::EndTurn, -1.0e9));
        }
        scored.extend(self.improvement_commands(me));
        if crate::env_var!("NO_AI_DIPLO").is_err() {
            scored.extend(self.diplomacy_ai_commands(me));
        }
        if crate::env_var!("NO_AI_ROADS").is_err() {
            scored.extend(self.road_commands(me));
        }
        match crate::env_var!("ORDER").as_deref() {
            Ok("rev_all") => scored.reverse(),
            Ok("imp_first") => {
                let n = scored.len();
                let imps: Vec<_> = scored.iter().filter(|c| matches!(c.0, Command::Build { .. })).cloned().collect();
                let rest: Vec<_> = scored.iter().filter(|c| !matches!(c.0, Command::Build { .. })).cloned().collect();
                scored = imps.into_iter().chain(rest).collect();
                let _ = n;
            }
            _ => {}
        }
        scored
    }

    fn ai_mut_unit_need(&mut self, me: u8, need: f32) {
        self.ai_states.entry(me).or_default().unit_need = need;
    }
}

fn to_ai_reward(r: crate::CityReward) -> AiReward {
    use crate::CityReward as R;
    match r {
        R::CityWall => AiReward::CityWall,
        R::Park => AiReward::Park,
        R::Workshop => AiReward::Workshop,
        R::Explorer => AiReward::Explorer,
        R::BorderGrowth => AiReward::BorderGrowth,
        R::SuperUnit => AiReward::SuperUnit,
        R::Resources => AiReward::Resources,
        R::PopulationGrowth => AiReward::PopulationGrowth,
        _ => AiReward::None,
    }
}
