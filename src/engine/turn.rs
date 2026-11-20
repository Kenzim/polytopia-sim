//! Turn flow, scoring and game end.

use super::*;
use crate::rules::economy::{self, ImprovementLevelInput};
use crate::GameStatus;

/// An improvement level-up follows the old rule up to this version and the newer one (with the surrounding-area check)
/// after it.
pub(super) const VERSION_LEVEL_UP_V103: i32 = 0x67;

impl Engine {
    /// End the current player's turn and start the next living player's.
    pub fn end_turn(&mut self) {
        let me = self.current_player();
        // In FinalTurn an end of turn (raw 15) runs the end-of-turn effects and then the game
        // ends where it stands: no new turn starts. The end-match command (raw 22, [`Engine::end_match`]) ends it
        // without the effects.
        if self.s.current_state == GameStatus::FinalTurn {
            if !self.ending_match {
                self.end_of_turn_effects(me);
            }
            self.finish_game();
            return;
        }
        let reward_open = self.reward_pending();
        self.end_of_turn_effects(me);
        // An effect of the turn's end can raise a city reward (a dragon growing up explores a new corner of the map, which
        // adds population). The action that hands the turn over sits beneath it on the action stack, so the player
        // answers the reward first and the next turn starts after it.
        if !reward_open && self.reward_pending() {
            self.handover_held = true;
            return;
        }
        self.pass_turn();
    }

    /// The end-match command (raw 22): valid only in `FinalTurn`, for the player to move
    /// when they have no pending reward. It sets the game `Ended` and does nothing else, unlike the end-of-turn
    /// command ([`Command::EndTurn`]), which runs the end-of-turn effects first. Nothing changes on an error, and the
    /// command counter does not move.
    pub fn end_match(&mut self) -> Result<(), Reject> {
        if self.s.current_state != GameStatus::FinalTurn {
            return Err(Reject::NotAllowed("the end-match command is only valid in FinalTurn"));
        }
        if self.pending_trigger(self.current_player()).is_some() {
            return Err(Reject::PendingReward);
        }
        self.ending_match = true;
        self.apply_unchecked(Command::EndTurn);
        self.ending_match = false;
        Ok(())
    }

    /// The resign command (raw 35): `resigned` goes on auto-play with the turn and the
    /// command index it resigned at (and handicap 1 in Multiplayer and Competitive games), then the winner check runs,
    /// which sends the game to `FinalTurn` when nobody who plays is left. Retail's validity check only asks for the current
    /// player to issue it, so it works in any state, whoever resigns; the kicker and kicked flag of the raw command
    /// only feed the notification. Errors, and changes nothing, for a player the game does not have.
    pub fn resign(&mut self, resigned: u8) -> Result<(), Reject> {
        if !self.s.players.iter().any(|p| p.id == resigned) {
            return Err(Reject::NotAllowed("the resigning player is not in the game"));
        }
        self.s.current_command = self.s.current_command.wrapping_add(1);
        let game_type = self.s.settings.game_type;
        crate::rules::commands::resign(&mut self.s, resigned, game_type);
        let turn = self.s.current_turn as i32;
        if let Some(rt) = self.s.player_runtime.iter_mut().find(|r| r.player == resigned) {
            rt.resigned_turn = turn;
        }
        self.check_game_over();
        Ok(())
    }

    /// Hand the turn to the next living player and start it (the part of [`Engine::end_turn`] after the end-of-turn
    /// effects).
    pub(super) fn pass_turn(&mut self) {
        self.handover_held = false;
        // End of the player's turn: next living player, a wrap adds a turn.
        let n = self.s.players.len() as u8;
        if n == 0 {
            return;
        }
        let mut idx = self.s.current_player_index;
        let mut wrapped = false;
        for _ in 0..=n {
            idx += 1;
            if idx >= n {
                idx = 0;
                wrapped = true;
            }
            let id = self.s.players[usize::from(idx)].id;
            if id == NATURE || self.is_alive(id) {
                break;
            }
        }
        self.s.current_player_index = idx;
        if wrapped {
            self.s.current_turn += 1;
        }
        // Turn limit.
        let limit = self.s.settings.turn_limit;
        if wrapped && limit > 0 && self.s.current_turn as i32 >= limit {
            if crate::env_var!("OLD_TURN_LIMIT").is_ok() {
                self.finish_game();
                return;
            }
            // The end-of-turn action pushes the game-over action, then the start-of-turn action; the stack is LIFO, so the next player's
            // turn starts and then game-over stores FinalTurn. That player may still act (retail scratch run
            // 2026-10-19, weekly challenge: a turn-10 research was accepted and scored) until the end-match command.
            let next = self.current_player();
            self.start_turn(next);
            self.s.current_state = GameStatus::FinalTurn;
            return;
        }
        let next = self.current_player();
        self.start_turn(next);
    }

    /// End-of-turn effects: healing, growth, and clearing the per-turn flags.
    fn end_of_turn_effects(&mut self, me: u8) {
        if crate::env_var!("PAC_START").is_err() {
            self.tasks_end_of_turn(me);
        }
        self.end_of_turn_heals(me);
        let ids: Vec<u32> = self.s.units.iter().filter(|u| u.owner == me).map(|u| u.id).collect();
        let mut thawed_tentacles = false;
        for id in ids {
            let Some(i) = self.unit_index(id) else { continue };
            let unit = self.s.units[i].clone();
            // Units that grow (dragon eggs) turn into the next stage.
            if self.has_ability(&unit, unit_ability::GROW) {
                if let Some(info) = self.unit_info(&unit) {
                    let age = (self.s.current_turn as i32) - i32::from(unit.created_turn);
                    let rate = info.data.growth_rate.unwrap_or(0);
                    if rate > 0 && age >= rate {
                        let me_name = info.data.id.clone();
                        let into = self
                            .t
                            .logic
                            .units
                            .values()
                            .find(|u| u.upgrades_from.as_deref() == Some(me_name.as_str()))
                            .map(|u| u.idx);
                        if let Some(into) = into {
                            // Growing is an upgrade of cost 0, so it burns a unit id like a bought one.
                            self.upgrade_unit(me, unit.id, into, 0);
                        }
                    }
                }
            }
            let dbl = self.has_ability(&unit, unit_ability::DOUBLE_ATTACK);
            thawed_tentacles |= unit.frozen && self.has_ability(&unit, unit_ability::TENTACLES);
            let u = &mut self.s.units[i];
            u.moved = false;
            u.attacked = false;
            u.frozen = false;
            u.petrified = false;
            u.previous_turn_end = u.coordinates;
            if dbl {
                u.double_ready = true;
            }
        }
        if thawed_tentacles {
            self.update_tentacles();
        }
        // End of turn: every CoolDown relation of the ending player becomes
        // Neutral, in all versions. The diplomacy check at the opponent's turn start can end it earlier for the other side.
        if let Some(row) = self.s.diplomacy.iter_mut().find(|r| r.player == me) {
            for link in &mut row.relations {
                if link.relation.state == crate::DiplomacyRelationState::CoolDown {
                    link.relation.state = crate::DiplomacyRelationState::Neutral;
                }
            }
        }
        // End of turn: improvements on tiles whose owner is the ending player
        // lose the Robbed effect, in all versions.
        for rt in &mut self.s.improvements {
            if self.s.map.tiles.get(rt.tile as usize).is_some_and(|t| t.owner == me) {
                rt.robbed = false;
            }
        }
        self.finish_decomposing(me);
    }

    /// The heals the end of turn queues, run as the action stack runs them: last
    /// pushed first (the action runner takes the last entry), and what a running action
    /// pushes runs before anything older. Walking the tiles in
    /// index order, a tile holding an own unit that can recover queues the rest heal. A tile owned by the player whose
    /// improvement has the Heal ability (mycelium) queues a 40 heal action on its own unit when the heal options
    /// (centre included) list it, and a heal-others action when that list is not empty; that action
    /// queues a 40 heal per neighbour option once it runs. A 40 heal on a poisoned unit only cures it, so a unit next to
    /// two mycelia is healed twice and the order of the queue decides whether the poison cure uses up a heal.
    fn end_of_turn_heals(&mut self, me: u8) {
        enum Step {
            Rest(Wc),
            Fixed(Wc),
            Others(Wc),
        }
        let mut queue: Vec<Step> = Vec::new();
        let coords: Vec<Wc> = self.s.map.tiles.iter().map(|t| t.coordinates).collect();
        let grid = self.unit_grid_scope();
        for at in coords {
            if let Some(unit) = self.unit_at(at) {
                if unit.owner == me && self.can_recover(unit) {
                    queue.push(Step::Rest(at));
                }
            }
            let Some(tile) = self.tile(at) else { continue };
            if tile.owner != me || tile.improvement == 0 {
                continue;
            }
            if !self.t.improvement(tile.improvement).is_some_and(|i| i.abilities.contains(&improvement_ability::HEAL)) {
                continue;
            }
            let options = self.heal_options_with(me, at, true);
            if options.contains(&at) {
                queue.push(Step::Fixed(at));
            }
            if !options.is_empty() {
                queue.push(Step::Others(at));
            }
        }
        drop(grid);
        while let Some(step) = queue.pop() {
            match step {
                Step::Rest(at) => {
                    let Some(unit) = self.unit_at(at).cloned() else { continue };
                    let amount = if unit.poisoned { 0 } else { self.heal_amount(&unit) };
                    if let Some(i) = self.unit_index(unit.id) {
                        let u = &mut self.s.units[i];
                        u.health = u.health.saturating_add(amount as u16);
                        u.poisoned = false;
                        u.charmed = false;
                        if let Some(p) = u.passenger.as_mut() {
                            p.poisoned = false;
                        }
                    }
                }
                Step::Fixed(at) => {
                    let max = self.unit_at(at).map(|u| self.max_health(u)).unwrap_or(0);
                    crate::rules::commands::heal_unit(&mut self.s, at, 40, max.clamp(0, i32::from(u16::MAX)) as u16);
                }
                Step::Others(at) => {
                    for c in self.heal_options(me, at) {
                        queue.push(Step::Fixed(c));
                    }
                }
            }
        }
    }

    /// End of turn: on a tile owned by the ending player, an improvement
    /// with the Decomposing effect queues one 1-star currency increase per point of its cost and an
    /// improvement destruction, unless a unit stands on it and it has improvement ability 13 (bridge).
    fn finish_decomposing(&mut self, me: u8) {
        let marked: Vec<Wc> = self
            .s
            .improvements
            .iter()
            .filter(|r| r.decomposing)
            .filter_map(|r| self.s.map.tiles.get(r.tile as usize))
            .filter(|t| t.owner == me && t.improvement != 0)
            .map(|t| t.coordinates)
            .collect();
        for at in marked {
            let Some(tile) = self.tile(at).copied() else { continue };
            let Some(info) = self.t.improvement(tile.improvement) else { continue };
            if info.abilities.contains(&improvement_ability::BRIDGE) && self.unit_at(at).is_some() {
                continue;
            }
            let cost = info.data.cost.unwrap_or(0);
            self.do_destroy(me, at);
            if cost > 0 {
                self.add_currency(me, cost);
            }
        }
    }

    /// Diplomacy check at the start of a turn: pending messages are received (removed), meeting an unknown
    /// sender; then cool-downs end once the turn is past the broken peace (one turn later against a higher id).
    /// Returns the senders of received peace requests in the order their receive actions run
    /// (queued newest message first, run last-in first-out); each adds a `PeaceRequest` trigger.
    fn check_diplomacy(&mut self, me: u8) -> Vec<u8> {
        let msgs =
            self.s.player_runtime.iter().find(|r| r.player == me).map(|r| r.messages.clone()).unwrap_or_default();
        let mut meet: Vec<u8> = Vec::new();
        let mut peace: Vec<u8> = Vec::new();
        for &(kind, sender) in msgs.iter().rev() {
            if self.s.version >= 0x52 && !self.is_alive(sender) {
                continue;
            }
            if kind == 1 {
                peace.push(sender);
            }
            if !self.knows_player(me, sender) && !meet.contains(&sender) {
                meet.push(sender);
            }
        }
        peace.reverse();
        if let Some(r) = self.s.player_runtime.iter_mut().find(|r| r.player == me) {
            r.messages.clear();
        }
        for o in meet {
            if !self.knows_player(me, o) {
                self.meet(me, o);
            }
        }
        let turn = i64::from(self.s.current_turn);
        let others: Vec<u8> = self.s.players.iter().map(|p| p.id).collect();
        if let Some(row) = self.s.diplomacy.iter_mut().find(|r| r.player == me) {
            for link in &mut row.relations {
                if !others.contains(&link.other) || link.relation.state != crate::DiplomacyRelationState::CoolDown {
                    continue;
                }
                let broken = i64::from(link.relation.last_peace_broken_turn);
                let over = if me > link.other {
                    turn > broken
                } else if link.other > me {
                    turn > broken + 1
                } else {
                    false
                };
                if over {
                    link.relation.state = crate::DiplomacyRelationState::Neutral;
                }
            }
        }
        peace
    }

    /// Run the deferred stack from the top until the current
    /// player has a pending trigger. Returns whether anything ran.
    pub fn run_deferred(&mut self) -> bool {
        let me = self.current_player();
        let mut ran = false;
        while self.pending_trigger(me).is_none() {
            let Some(a) = self.deferred.pop() else { break };
            ran = true;
            self.run_action(a);
        }
        ran
    }

    /// Run the actions this command pushed above `base` on the deferred stack, top first, until the current player
    /// has a pending trigger; what is left waits there for the answer.
    pub(super) fn run_deferred_above(&mut self, base: usize) {
        let me = self.current_player();
        while self.deferred.len() > base && self.pending_trigger(me).is_none() {
            let Some(a) = self.deferred.pop() else { break };
            self.run_action(a);
        }
    }

    fn run_action(&mut self, a: Deferred) {
        match a {
            Deferred::CityLevelUp(c) => self.level_up_city(c),
            Deferred::ImprovementLevelUp(c) => self.improvement_level_up_action(c),
            Deferred::ImprovementLevelDown(c) => self.improvement_level_down(c),
            Deferred::Income(p, amount) => self.add_currency(p, amount),
            // Receiving a peace request adds a pending command trigger.
            Deferred::PeaceRequest { to, from } => {
                self.triggers.push(Trigger { player: to, city: Wc { x: -1, y: -1 }, peace_from: Some(from) });
            }
            // Already on the board; `refresh_scores` counts it from now on.
            Deferred::Score(..) => {}
            Deferred::Population(c, amount) => self.add_population(c, amount),
            Deferred::Scout(mut run) => self.continue_scout(&mut run),
            Deferred::Explore(mut run) => self.continue_explore(&mut run),
            Deferred::Infiltrate(mut run) => self.continue_infiltrate(&mut run),
            Deferred::Train { owner, unit_type, at, home, veteran } => {
                if veteran {
                    self.spawn_veteran(owner, unit_type, at, home);
                } else {
                    self.spawn_unit(owner, unit_type, at, home, true);
                }
            }
        }
    }

    /// The level-ups the surrounding-area check pushes around `at`: the area of radius 1 with diagonals and the centre,
    /// `dx` outer and `dy` inner, and for every adjacency improvement there one level-up per level it is below the
    /// level its neighbours give. Bottom of the stack first.
    pub(super) fn surrounding_level_ups(&self, at: Wc) -> Vec<Wc> {
        let mut ups = Vec::new();
        for dx in -1..=1 {
            for dy in -1..=1 {
                let c = Wc { x: at.x + dx, y: at.y + dy };
                if !self.in_bounds(c) {
                    continue;
                }
                let Some(tile) = self.tile(c) else { continue };
                let Some(info) = self.t.improvement(tile.improvement) else { continue };
                if info.data.adjacency_improvements.is_empty() || info.abilities.contains(&improvement_ability::PATINA)
                {
                    continue;
                }
                let Some(stored) = self.improvement_rt(c).map(|r| i32::from(r.level)) else { continue };
                for _ in stored..self.improvement_level(c) {
                    ups.push(c);
                }
            }
        }
        ups
    }

    /// The level-downs the same check pushes: every adjacency improvement around `at` (centre included) whose stored
    /// level is above the level its neighbours give, one per level lost.
    pub(super) fn surrounding_level_downs(&self, at: Wc) -> Vec<Wc> {
        let mut downs = Vec::new();
        for dx in -1..=1 {
            for dy in -1..=1 {
                let c = Wc { x: at.x + dx, y: at.y + dy };
                if !self.in_bounds(c) {
                    continue;
                }
                let Some(tile) = self.tile(c) else { continue };
                let Some(info) = self.t.improvement(tile.improvement) else { continue };
                if info.data.adjacency_improvements.is_empty() || info.abilities.contains(&improvement_ability::PATINA)
                {
                    continue;
                }
                let Some(stored) = self.improvement_rt(c).map(|r| i32::from(r.level)) else { continue };
                for _ in self.improvement_level(c)..stored {
                    downs.push(c);
                }
            }
        }
        downs
    }

    /// An improvement level-up run from the action stack. It is dropped unrun when the improvement is no longer
    /// below the level its neighbours give (a later push already raised it). Otherwise the level rises by one and the
    /// action pushes one population action per growth population for the ruling city, then (version above 103) the
    /// surrounding-area check around the tile on top, so neighbours that rise with it run before that population.
    pub fn improvement_level_up_action(&mut self, at: Wc) {
        let Some(stored) = self.improvement_rt(at).map(|r| i32::from(r.level)) else { return };
        if self.improvement_level(at) <= stored {
            return;
        }
        if self.s.version <= VERSION_LEVEL_UP_V103 {
            self.improvement_level_up(at);
            return;
        }
        let Some(tile) = self.tile(at).copied() else { return };
        let Some(info) = self.t.improvement(tile.improvement) else { return };
        let pop: i32 = info.data.growth_rewards.iter().map(|g| g.population.unwrap_or(0)).sum();
        if let Some(rt) = self.improvement_rt_mut(at) {
            rt.level += 1;
        }
        let ruling = tile.ruling_city;
        if pop > 0 && ruling.x >= 0 && ruling.y >= 0 && self.is_city(ruling) {
            for _ in 0..pop {
                self.deferred.push(Deferred::Population(ruling, 1));
            }
        }
        let ups = self.surrounding_level_ups(at);
        self.deferred.extend(ups.into_iter().map(Deferred::ImprovementLevelUp));
    }

    /// Work of a tile on another player's tile: their capital pays
    /// the embassy income of the owner towards `me` unless an opponent unit stands there.
    fn foreign_capital_work(&self, me: u8, at: Wc) -> i32 {
        let Some(d) = self.t.logic.diplomacy.as_ref() else { return 0 };
        let Some(p) = self.s.players.iter().find(|p| p.id != me && p.id != NATURE && p.start_tile == at) else {
            return 0;
        };
        let Some(t) = self.tile(at) else { return 0 };
        if t.owner != p.id || t.improvement != improvement::CITY {
            return 0;
        }
        if self.unit_at(at).is_some_and(|u| u.owner != p.id) {
            return 0;
        }
        // The embassy income of owner `p` towards `me` reads the relation `p` holds toward `me`.
        let rel = crate::rules::diplomacy::get_relation(&self.s, p.id, me);
        let war = crate::rules::diplomacy::has_war_with(&self.s, me, p.id);
        let v = economy::embassy_income(me, rel.state as i32, rel.embassy_level, war, d.embassy_income);
        if v != 0 && crate::env_var!("INCOME_DEBUG").is_ok() {
            eprintln!(
                "FOREIGN p{} on p{} cap ({},{}) state {:?} lvl {} war {} -> {}",
                me, p.id, at.x, at.y, rel.state, rel.embassy_level, war, v
            );
        }
        v
    }

    /// One more level, then the growth rewards.
    pub fn improvement_level_up(&mut self, at: Wc) {
        let Some(tile) = self.tile(at).copied() else { return };
        let Some(info) = self.t.improvement(tile.improvement) else { return };
        let pop: i32 = info.data.growth_rewards.iter().map(|g| g.population.unwrap_or(0)).sum();
        if let Some(rt) = self.improvement_rt_mut(at) {
            rt.level += 1;
        }
        let ruling = tile.ruling_city;
        if pop > 0 && ruling.x >= 0 && ruling.y >= 0 && self.is_city(ruling) {
            self.add_population(ruling, pop);
        }
    }

    /// One level lost (a Fungi whose owner no longer has Microbes): the level drops and the city that rules the tile
    /// loses the population the improvement's growth rewards gave for the level, as in the game.
    pub fn improvement_level_down(&mut self, at: Wc) {
        let Some(tile) = self.tile(at).copied() else { return };
        // Retail's own check of the action: it only runs while the level the neighbours give is below the stored one.
        let Some(stored) = self.improvement_rt(at).map(|r| i32::from(r.level)) else { return };
        if self.improvement_level(at) >= stored {
            return;
        }
        if let Some(rt) = self.improvement_rt_mut(at) {
            rt.level = rt.level.saturating_sub(1);
        }
        let Some(info) = self.t.improvement(tile.improvement) else { return };
        let pop: i32 = info.data.growth_rewards.iter().map(|g| g.population.unwrap_or(0).max(0)).sum();
        let ruling = tile.ruling_city;
        if pop > 0 && ruling.x >= 0 && ruling.y >= 0 && self.is_city(ruling) {
            self.add_population(ruling, -pop);
        }
        // Above version 103 the action ends with the surrounding-area check, so neighbours that gave their level
        // through this one come down in turn.
        if self.s.version > VERSION_LEVEL_UP_V103 {
            let mut checks: Vec<Deferred> =
                self.surrounding_level_ups(at).into_iter().map(Deferred::ImprovementLevelUp).collect();
            checks.extend(self.surrounding_level_downs(at).into_iter().map(Deferred::ImprovementLevelDown));
            self.deferred.extend(checks);
        }
    }

    /// Start of turn: level-ups and income.
    pub fn start_turn(&mut self, me: u8) {
        if me == NATURE {
            // Nature's end-turn command is a real command in the stack and advances the counter.
            self.s.current_command = self.s.current_command.wrapping_add(1);
            self.end_turn();
            return;
        }
        let turn = self.s.current_turn;
        // The tribe-connection check and the diplomacy check run on every turn, turn 0 included.
        if crate::env_var!("NO_TURN_MEET").is_err() {
            self.check_tribe_connections(me);
        }
        self.tasks_start_of_turn(me);
        let peace = if crate::env_var!("NO_MESSAGES").is_err() { self.check_diplomacy(me) } else { Vec::new() };
        // The diplomacy check runs after the tile walk, so its messages sit on top of the stack and run first.
        let peace: Vec<Deferred> = peace.into_iter().map(|from| Deferred::PeaceRequest { to: me, from }).collect();
        // Start of turn skips level-ups and income entirely on turn 0.
        if turn == 0 {
            self.deferred.extend(peace);
            self.run_deferred();
            self.refresh_scores();
            return;
        }
        if crate::env_var!("PAC_START").is_ok() {
            self.tasks_end_of_turn(me);
        }
        // Start of turn walks every tile once and pushes its actions: each tile's income is
        // computed from the levels as they are before any pushed level-up has run.
        let tiles: Vec<(Wc, u8, i32)> =
            self.s.map.tiles.iter().map(|t| (t.coordinates, t.owner, t.improvement)).collect();
        let foreign = crate::env_var!("NO_FOREIGN_EMBASSY").is_err();
        let mut walk: Vec<Deferred> = Vec::new();
        let mut attracted: Vec<Wc> = Vec::new();
        let grid = self.unit_grid_scope();
        for (c, owner, imp) in tiles {
            if owner != me {
                let w = if foreign { self.foreign_capital_work(me, c) } else { 0 };
                if w > 0 {
                    walk.push(Deferred::Income(me, w));
                }
                continue;
            }
            if imp == 0 {
                continue;
            }
            attracted.extend(self.attract_target(c, imp));
            if self.is_city(c) {
                let ready = self.improvement_rt(c).is_some_and(|r| economy::should_level_up(r.xp, r.level));
                if ready {
                    walk.push(Deferred::CityLevelUp(c));
                }
            } else {
                // Update the improvement level: one action per level gained or lost.
                let new = self.improvement_level(c).max(0);
                let old = self.improvement_rt(c).map(|r| i32::from(r.level)).unwrap_or(new);
                for _ in old..new {
                    walk.push(Deferred::ImprovementLevelUp(c));
                }
                for _ in new..old {
                    walk.push(Deferred::ImprovementLevelDown(c));
                }
            }
            let w = self.tile_work(c);
            if crate::env_var!("INCOME_DEBUG").is_ok() {
                eprintln!("INCOME turn {} p{} tile ({},{}) work {}", turn, me, c.x, c.y, w);
            }
            if w > 0 {
                walk.push(Deferred::Income(me, w));
            }
        }
        drop(grid);
        // The walk reads the board as it was at the start of the turn; the resources it asked for land after it.
        // Each new resource is its own action in retail, which ends with the surrounding-area check: the sanctuaries
        // around it rise at once instead of at the owner's next turn start.
        let mut created_ups: Vec<Deferred> = Vec::new();
        for at in attracted {
            if let Some(t) = self.tile_mut(at) {
                t.resource = economy::GAME_RESOURCE;
            }
            created_ups.extend(self.surrounding_level_ups(at).into_iter().map(Deferred::ImprovementLevelUp));
        }
        walk.extend(created_ups);
        self.deferred.extend(walk);
        self.deferred.extend(peace);
        self.restore_climate_at_turn_start(me);
        self.run_deferred();
        self.refresh_scores();
    }

    /// The tile an Attract improvement (the Sanctuary) at `at` fills with game at the start of its owner's turn: when
    /// the improvement's age is a multiple of its growth rate, the first tile of the radius-1 area (diagonals in,
    /// centre out) that the same player owns and that is a forest with no improvement and no resource.
    fn attract_target(&self, at: Wc, imp: i32) -> Option<Wc> {
        let info = self.t.improvement(imp)?;
        if !info.has(improvement_ability::ATTRACT) {
            return None;
        }
        let rate = info.data.growth_rate?;
        let founded = self.improvement_rt(at)?.founded;
        // A zero growth rate is a division fault in retail; no data has one.
        if rate == 0 || economy::improvement_age(self.s.current_turn, founded) % rate != 0 {
            return None;
        }
        let owner = self.tile(at)?.owner;
        self.area_iter(at, 1).find(|&n| {
            self.tile(n).is_some_and(|t| {
                t.owner == owner && t.improvement == 0 && t.resource == 0 && t.terrain == economy::FOREST_TERRAIN
            })
        })
    }

    // ---------- score ----------

    pub fn improvement_score(&self, at: Wc) -> i32 {
        let Some(tile) = self.tile(at) else { return 0 };
        if tile.owner == 0 || tile.owner == NATURE || tile.improvement == 0 {
            return 0;
        }
        let Some(rt) = self.improvement_rt(at) else { return 0 };
        self.improvement_score_of(tile, rt)
    }

    /// `improvement_score` for an owned, improved tile and its improvement record.
    fn improvement_score_of(&self, tile: &Tile, rt: &ImprovementRuntime) -> i32 {
        let Some(info) = self.t.improvement(tile.improvement) else { return 0 };
        let mut score = i32::from(rt.base_score);
        if tile.improvement == improvement::CITY {
            let level = i32::from(rt.level);
            let xp = i32::from(rt.xp);
            score += (level + 1) * economy::CITY_LEVEL_SCORE;
            score += economy::CITY_XP_SCORE * xp;
            if self.s.version > 0x72 && xp < 0 {
                score += xp * economy::CITY_STARVATION_PENALTY;
                score = score.max(0);
            }
        }
        if info.has(improvement_ability::PATINA) {
            let per_level: i32 = info.data.growth_rewards.iter().map(|g| g.score.unwrap_or(0)).sum();
            score += i32::from(rt.level).max(0) * per_level;
        }
        score
    }

    /// Player score: territory + cities + culture + technology.
    pub fn compute_score(&self, player: u8) -> i32 {
        let mut owned = 0;
        let mut units = 0;
        let mut explored = 0;
        let mut city = 0;
        let mut culture = 0;
        for t in &self.s.map.tiles {
            if t.owner == player {
                owned += 1;
                if t.improvement != 0 {
                    let s = self.improvement_score(t.coordinates);
                    if t.improvement == improvement::CITY {
                        city += s;
                    } else {
                        culture += s;
                    }
                }
            }
            if self.is_explored(player, t.index as usize) {
                explored += 1;
            }
        }
        for u in self.s.units.iter().filter(|u| u.owner == player) {
            units += self.unit_score(u);
        }
        let mut tech = 0;
        for &tc in self.techs(player) {
            if let Some(td) = self.t.tech(tc) {
                tech += economy::tech_score_points(td.cost);
            }
        }
        owned * economy::TILE_VALUE + units + explored * economy::EXPLORE_VALUE + city + culture + tech
    }

    /// Unit score: 5 per star of the full cost (unit plus passenger).
    pub fn unit_score(&self, u: &UnitState) -> i32 {
        let own = self.unit_info(u).map(|i| i.data.cost).unwrap_or(0);
        let pass = u.passenger.as_ref().and_then(|p| self.unit_info(p)).map(|i| i.data.cost);
        economy::unit_score(self.s.version, own, economy::full_unit_cost(own, pass))
    }

    pub fn refresh_scores(&mut self) {
        let scores = self.all_scores();
        for (slot, score) in scores.into_iter().enumerate() {
            let id = self.s.players[slot].id;
            if id == NATURE {
                continue;
            }
            let held: i32 = self
                .deferred
                .iter()
                .map(|d| match *d {
                    Deferred::Score(p, a) if p == id => a,
                    _ => 0,
                })
                .sum();
            self.s.players[slot].score = (score - held).max(0) as u32;
        }
    }

    /// `compute_score` for every player, in player order, from one pass over the tiles, fog and units.
    pub fn all_scores(&self) -> Vec<i32> {
        // Per player id: territory, improvement and unit points, and explored tiles.
        let mut points = [0i32; 256];
        let mut explored = [0i32; 256];
        let mut seen = [u32::MAX; 256];
        // The first improvement record of each tile index, as `improvement_rt` finds it.
        let mut record = vec![u32::MAX; self.s.map.tiles.len()];
        for (r, rt) in self.s.improvements.iter().enumerate() {
            if let Some(slot) = usize::try_from(rt.tile).ok().and_then(|i| record.get_mut(i)) {
                if *slot == u32::MAX {
                    *slot = u32::try_from(r).unwrap_or(u32::MAX);
                }
            }
        }
        for (n, t) in self.s.map.tiles.iter().enumerate() {
            let o = usize::from(t.owner);
            points[o] += economy::TILE_VALUE;
            if t.improvement != 0 && t.owner != 0 && t.owner != NATURE {
                let r = usize::try_from(t.index).ok().and_then(|i| record.get(i)).map_or(usize::MAX, |&r| r as usize);
                if let Some(rt) = self.s.improvements.get(r) {
                    points[o] += self.improvement_score_of(t, rt);
                }
            }
            if let Some(f) = usize::try_from(t.index).ok().and_then(|i| self.s.fog.get(i)) {
                for &p in &f.explorers {
                    let p = usize::from(p);
                    if seen[p] != n as u32 {
                        seen[p] = n as u32;
                        explored[p] += 1;
                    }
                }
            }
        }
        for u in &self.s.units {
            points[usize::from(u.owner)] += self.unit_score(u);
        }
        self.s
            .players
            .iter()
            .map(|p| {
                let id = usize::from(p.id);
                let tech: i32 = self
                    .techs(p.id)
                    .iter()
                    .filter_map(|&tc| self.t.tech(tc))
                    .map(|td| economy::tech_score_points(td.cost))
                    .sum();
                points[id] + explored[id] * economy::EXPLORE_VALUE + tech
            })
            .collect()
    }

    pub fn score(&self, player: u8) -> i32 {
        self.compute_score(player)
    }

    // ---------- game end ----------

    pub fn check_game_over(&mut self) {
        if self.is_over() {
            return;
        }
        let settings = self.s.settings;
        let alive = self.living_players();
        // The winner check (the default form is used for version > 90; version 90
        // differs only for Glory), run by the city capture and the
        // end of turn, which queue the game-over action when it holds (that action stores FinalTurn, see
        // `queue_game_over`): one player
        // left ends the game only under the win-by-extermination rule (the Domination and Might presets),
        // when no human (not auto-play) is
        // alive, or in Multiplayer (1) and Competitive (4) games. Perfection rules (the weekly challenge) play on to
        // the turn limit after the last opponent is wiped out. Local saves and `setup` games carry empty
        // game rules (no limit, no win rule), under which this check only ends the game once no human is left; what
        // ends a local Domination game is unknown, so they keep ending on extermination.
        let humans = alive.iter().filter(|&&id| self.player(id).is_some_and(|p| !p.auto_play)).count();
        let no_rules = settings.turn_limit <= 0 && settings.score_limit <= 0 && !settings.win_by_capital;
        let ends = settings.win_by_extermination || no_rules || humans == 0 || matches!(settings.game_type, 1 | 4);
        let contested = self.s.players.iter().filter(|p| p.id != NATURE).count() > 1;
        if ends && alive.len() <= 1 && contested {
            self.queue_game_over();
            return;
        }
        // Retail counts the living players who are not on auto-play before
        // anything else, and in Multiplayer and Competitive games one is enough to end the game, whatever the number
        // of living players that are on auto-play. This is how a resignation ends a two-player game. Retail also ends
        // a game when none is left, but a game built here with every seat on auto-play (an AI-only match) has none from
        // the start and must play on, so that case is not applied.
        if contested && humans == 1 && matches!(settings.game_type, 1 | 4) {
            self.queue_game_over();
            return;
        }
        // Capital rule (Might): the game ends the moment one player holds every capital. A game with a turn limit
        // ends at that limit instead, so the rule only applies without one. It needs two or more players, as the
        // extermination rule does.
        if settings.win_by_capital
            && settings.turn_limit <= 0
            && self.s.players.iter().filter(|p| p.id != NATURE).count() > 1
            && self.capital_holder().is_some()
        {
            self.queue_game_over();
            return;
        }
        if settings.score_limit > 0 {
            self.refresh_scores();
            if self.s.players.iter().any(|p| p.id != NATURE && p.score as i32 >= settings.score_limit) {
                self.queue_game_over();
            }
        }
    }

    /// The game-over action: the game goes to `FinalTurn`, not `Ended`. The player to move keeps acting (retail accepted
    /// nine more commands after the capture of the last capital) and only the end-match command, `Command::EndTurn` in
    /// that state, ends the game. Nothing else runs: no end of turn, no new turn, no income.
    fn queue_game_over(&mut self) {
        crate::rules::commands::game_over(&mut self.s);
    }

    pub fn finish_game(&mut self) {
        self.refresh_scores();
        self.s.current_state = GameStatus::Ended;
        self.winners = self.pick_winner().map(|id| vec![id]).unwrap_or_default();
    }

    /// The winner of a finished game, from the stored scores: with the capital rule on, the player holding every
    /// capital. Otherwise the best living player in retail's ranking (`players_sorted_by_rank`): in Domination
    /// (rules mode 2) by cities held and then score, in Might (rules mode 4) by capitals held and then score, in every
    /// other mode by score. Players level on the key keep the order of retail's list sort.
    fn pick_winner(&self) -> Option<u8> {
        if self.s.settings.win_by_capital {
            if let Some(holder) = self.capital_holder() {
                return Some(holder);
            }
        }
        let living = self.living_players();
        self.players_sorted_by_rank().into_iter().find(|id| living.contains(id))
    }

    /// Number of capitals in the game: one per player (Nature has none), whether that player is still alive or not.
    pub fn capital_total(&self) -> i32 {
        self.s.players.iter().filter(|p| p.id != NATURE).count() as i32
    }

    /// How many capitals each player holds, in player order, Nature left out. A capital is the tile a player
    /// started on (`start_tile`, the original capital); it counts for whoever owns that tile now, so a player holds
    /// their own capital until someone captures it and an eliminated player's capital is held by its captor.
    pub fn capital_counts(&self) -> Vec<(u8, i32)> {
        self.s
            .players
            .iter()
            .filter(|p| p.id != NATURE)
            .map(|p| (p.id, crate::rules::diplomacy::count_capitals(&self.s, p.id)))
            .collect()
    }

    /// The living player who holds every capital, if there is one (this includes their own).
    pub fn capital_holder(&self) -> Option<u8> {
        if self.capital_total() == 0 {
            return None;
        }
        // Holding every capital means owning every start tile, so only a common owner of all of them can qualify.
        let (width, height) = (i32::from(self.s.map.width), i32::from(self.s.map.height));
        let mut holder = None;
        for p in self.s.players.iter().filter(|p| p.id != NATURE) {
            let (x, y) = (p.start_tile.x, p.start_tile.y);
            if x < 0 || y < 0 || x >= width || y >= height {
                return None;
            }
            let owner = self.s.map.tiles[(y * width + x) as usize].owner;
            if holder.is_some_and(|h| h != owner) {
                return None;
            }
            holder = Some(owner);
        }
        holder.filter(|&id| id != NATURE && self.s.players.iter().any(|p| p.id == id) && self.is_alive(id))
    }

    /// The winners of a game that is over, empty while it is still running. A game ended by `finish_game` has them in
    /// `winners`; a position loaded from JSON does not carry them, so they are worked out again from the board.
    pub fn game_winners(&self) -> Vec<u8> {
        if !self.is_over() {
            return Vec::new();
        }
        if !self.winners.is_empty() {
            return self.winners.clone();
        }
        self.pick_winner().into_iter().collect()
    }

    /// Ratio-free outcome for tree search: 1.0 for a win, otherwise the share of the leader's score.
    pub fn reward_for(&self, player: u8) -> f32 {
        if self.winners.contains(&player) {
            return 1.0;
        }
        if self.is_over() {
            return 0.0;
        }
        let mine = self.compute_score(player).max(0) as f32;
        let best = self
            .s
            .players
            .iter()
            .filter(|p| p.id != NATURE)
            .map(|p| self.compute_score(p.id).max(0) as f32)
            .fold(1.0_f32, f32::max);
        0.5 * mine / best
    }

    #[allow(dead_code)]
    fn level_input(&self) -> Option<ImprovementLevelInput> {
        None
    }
}
