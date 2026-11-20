//! `OpinionManager`: how much each AI likes each other player.
//!
//! Follows the retail opinion update: the per-opponent update, the per-tile observations,
//! the judgement of an opponent, the win-hate of a player and the military advantage.
//! The end-of-command action marks every player's opinions dirty after each command, so the
//! first query after a command recomputes them from the current state.

use super::ai_state::MilitaryStats;
use super::*;
use crate::flat_map::FlatMap;

/// Opinion types.
pub mod opinion_type {
    pub const COMMON_RELATION: i32 = 0;
    pub const DIFFERENT_RELATION: i32 = 1;
    pub const PEACEFUL: i32 = 2;
    pub const AGGRESSION: i32 = 3;
    pub const EMBASSY: i32 = 4;
    pub const THREATENING: i32 = 5;
    pub const INTRUSIVE: i32 = 6;
    pub const WINNING: i32 = 7;
    pub const BRAVE: i32 = 8;
    pub const DISLIKE: i32 = 9;
    pub const LIKE: i32 = 10;
    pub const WEAK: i32 = 11;
    pub const POWERFUL: i32 = 12;
}
use opinion_type::*;

/// Longsight opinion type.
const LONGSIGHT: i32 = 4;

#[derive(Debug, Clone, Default)]
pub struct OpinionState {
    pub reasons: FlatMap<i32, f32>,
    pub total: f32,
}

impl OpinionState {
    /// Add an opinion amount of the given type.
    pub fn add(&mut self, amount: f32, kind: i32) {
        let e = self.reasons.entry(kind).or_insert(0.0);
        *e += amount;
        self.total += amount;
    }

    pub fn get(&self, kind: i32) -> f32 {
        self.reasons.get(&kind).copied().unwrap_or(0.0)
    }
}

#[derive(Debug, Clone, Default)]
pub struct OpinionManager {
    pub opinions: FlatMap<u8, OpinionState>,
    /// Insertion order of `opinions` (the retail `Dictionary` enumerates in this order).
    pub order: Vec<u8>,
    /// Command counter of the last update; `None` while dirty.
    pub clean_at: Option<(u16, u32)>,
}

#[derive(Debug, Clone, Default)]
pub struct OpinionStore {
    pub managers: FlatMap<u8, OpinionManager>,
}

impl Engine {
    /// Player ids sorted by rank: best first, Nature last.
    pub fn players_sorted_by_rank(&self) -> Vec<u8> {
        let mode = self.s.settings.rules_game_mode;
        let mut list: Vec<u8> = self.s.players.iter().map(|p| p.id).collect();
        let score = |id: u8| self.player(id).map(|p| p.score).unwrap_or(0);
        let cities = |id: u8| self.recorded_city_count(id);
        let caps = |id: u8| crate::rules::diplomacy::count_capitals(&self.s, id);
        // Comparison is `b.key.CompareTo(a.key)`: descending, ties by score for city/capital modes.
        super::ai_state::sort_with(&mut list, |a, b| match mode {
            2 => {
                if cities(*a) != cities(*b) {
                    cities(*b).cmp(&cities(*a))
                } else {
                    score(*b).cmp(&score(*a))
                }
            }
            4 => {
                if caps(*a) != caps(*b) {
                    caps(*b).cmp(&caps(*a))
                } else {
                    score(*b).cmp(&score(*a))
                }
            }
            _ => score(*b).cmp(&score(*a)),
        });
        // Elimination: living players first, wiped players by elimination index (later first), Nature last.
        let wiped = |id: u8| self.player_runtime(id).map(|r| r.wiped_at_command).unwrap_or(-1);
        let mut alive: Vec<u8> = Vec::new();
        let mut gone: Vec<u8> = Vec::new();
        let mut nature = None;
        for id in list {
            if id == NATURE {
                nature = Some(id);
                continue;
            }
            if wiped(id) != -1 {
                gone.push(id);
            } else {
                alive.push(id);
            }
        }
        super::ai_state::sort_with(&mut gone, |a, b| wiped(*b).cmp(&wiped(*a)));
        alive.extend(gone);
        if let Some(n) = nature {
            alive.push(n);
        }
        alive
    }

    /// Players excluding Nature.
    pub fn player_count(&self) -> i32 {
        self.s.players.len() as i32 - 1
    }

    /// Win-hate of `opponent` towards the winner.
    fn win_hate(&self, opponent: u8, winner: u8, count: f32) -> f32 {
        let mode = self.s.settings.rules_game_mode;
        let mut result = 0.0f32;
        if mode == 4 {
            let caps = crate::rules::diplomacy::count_capitals(&self.s, opponent);
            result += caps as f32 / count;
        }
        match mode {
            1 => {
                let turn = self.s.current_turn as i64 as f32;
                let limit = self.s.settings.turn_limit as f32;
                result += turn / limit;
            }
            2 => {
                let caps = crate::rules::diplomacy::count_capitals(&self.s, winner);
                result += caps as f32 / self.player_count() as f32;
            }
            3 => {
                let score = self.player(opponent).map(|p| p.score).unwrap_or(0);
                let limit = self.s.settings.score_limit;
                let q = if limit == 0 { 0 } else { (score as i64 / limit as i64) as i32 };
                result += q as f32;
            }
            _ => {}
        }
        result
    }

    /// The judgement of an opponent.
    fn judgement(&self, player: u8, opponent: u8) -> f32 {
        let mut score = 0.0f32;
        let war = |a: u8, b: u8| crate::rules::diplomacy::has_war_with(&self.s, a, b);
        for other in self.s.players.iter().map(|p| p.id) {
            if other == player || other == opponent {
                continue;
            }
            if war(player, other) {
                let mut tmp = score;
                if war(opponent, other) {
                    tmp += 2.0;
                }
                score = tmp;
                if self.at_peace(opponent, other) {
                    score = tmp + -1.0;
                }
            }
            if self.at_peace(player, other) {
                let mut tmp = score;
                if war(opponent, other) {
                    tmp += -2.0;
                }
                score = tmp;
                if self.at_peace(opponent, other) {
                    score = tmp + 1.0;
                }
            }
        }
        let rel = crate::rules::diplomacy::get_relation(&self.s, player, opponent);
        if rel.last_peace_broken_turn > 0 {
            let diff = (i64::from(self.s.current_turn) - i64::from(rel.last_peace_broken_turn)) as f32 + 1.0;
            score -= LONGSIGHT as f32 / diff;
        }
        if 2.0 < score {
            2.0
        } else {
            score
        }
    }

    /// The military advantage of `a` over `b`.
    pub fn military_advantage(stats: &[(u8, MilitaryStats)], a: u8, b: u8) -> f32 {
        let get = |id: u8| stats.iter().find(|(p, _)| *p == id).map(|(_, s)| s.weighted_total()).unwrap_or(0.0);
        let own = get(a);
        let other = get(b);
        let denom = if other > 1.0 { other } else { 1.0 };
        let q = own / denom;
        let ratio = if 2.0 < q { 2.0 } else { q };
        if own == 0.0 && other == 0.0 {
            1.0
        } else {
            ratio
        }
    }

    /// Every unit's contribution, keyed by owner in first-seen order.
    pub fn all_military_stats(&self) -> Vec<(u8, MilitaryStats)> {
        let mut out: Vec<(u8, MilitaryStats)> = Vec::new();
        for t in &self.s.map.tiles {
            let Some(u) = self.unit_at(t.coordinates) else { continue };
            Self::add_unit_stats(&mut out, self, u);
        }
        out
    }

    pub fn add_unit_stats(out: &mut Vec<(u8, MilitaryStats)>, e: &Engine, u: &UnitState) {
        let Some(info) = e.unit_info(u) else { return };
        let at = match out.iter().position(|(p, _)| *p == u.owner) {
            Some(i) => i,
            None => {
                out.push((u.owner, MilitaryStats::default()));
                out.len() - 1
            }
        };
        let defence = if crate::env_var!("MIL_RAWDEF").is_ok() {
            e.combat_unit(u).raw_defence as f32
        } else {
            e.combat_unit(u).defence as f32
        };
        let attack = if crate::env_var!("MIL_RAWATT").is_ok() { info.data.attack } else { e.unit_attack(u) };
        let movement = e.unit_movement(u);
        let st = &mut out[at].1;
        st.defense += defence * 0.01;
        st.attack += (attack as f32) * 0.01;
        let m = (movement - 1) as f32;
        st.movement += m + m;
        let r = (info.data.range - 1) as f32;
        st.range += r + r;
    }

    /// The opinion of `player` towards `opponent`.
    pub fn opinion(&self, player: u8, opponent: u8) -> f32 {
        self.ensure_opinions(player);
        let store = self.opinions.borrow();
        store.managers.get(&player).and_then(|m| m.opinions.get(&opponent)).map(|s| s.total).unwrap_or(0.0)
    }

    pub fn opinion_reasons(&self, player: u8, opponent: u8) -> Option<OpinionState> {
        self.ensure_opinions(player);
        self.opinions.borrow().managers.get(&player).and_then(|m| m.opinions.get(&opponent).cloned())
    }

    fn ensure_opinions(&self, player: u8) {
        let key = (self.s.current_command, self.s.current_turn);
        {
            let store = self.opinions.borrow();
            if let Some(m) = store.managers.get(&player) {
                if m.clean_at == Some(key) {
                    return;
                }
            }
        }
        // Take the manager out, update it, put it back.
        let mut manager = self.opinions.borrow_mut().managers.remove(&player).unwrap_or_default();
        self.update_opinions(player, &mut manager);
        manager.clean_at = Some(key);
        self.opinions.borrow_mut().managers.insert(player, manager);
    }

    /// Update all of a player's opinions.
    #[allow(clippy::neg_cmp_op_on_partial_ord)] // `!(a > b)` is deliberately true for NaN
    fn update_opinions(&self, player: u8, mgr: &mut OpinionManager) {
        let _grid = self.unit_grid_scope();
        let ranked = self.players_sorted_by_rank();
        let stats = self.all_military_stats();
        // Tiles around the player's own explored cities (used for border observations), each once, in first-seen order.
        let mut tiles: Vec<Wc> = Vec::new();
        let mut listed = vec![false; self.s.map.tiles.len()];
        for t in &self.s.map.tiles {
            if self.is_explored(player, t.index as usize) && t.improvement == improvement::CITY && t.owner == player {
                let radius = self.improvement_rt(t.coordinates).map(|r| i32::from(r.border_size)).unwrap_or(1);
                for a in self.area_iter(t.coordinates, radius).chain(std::iter::once(t.coordinates)) {
                    if self.tile(a).is_some_and(|x| x.owner == player) {
                        for c in self.area_iter(a, 1).chain(std::iter::once(a)) {
                            if let Some(i) = self.tile_index(c) {
                                if !std::mem::replace(&mut listed[i], true) {
                                    tiles.push(c);
                                }
                            }
                        }
                    }
                }
            }
        }
        // `PlayerState.opinions` is not serialised. In online games the server's AI works on a freshly loaded state,
        // so the table holds only the opponents updated in this pass, in player order: a dead opponent has no
        // entry and can neither take the worst-opinion dislike nor keep accumulating it (a multiplayer game, command 1952).
        // A local client keeps the table for the session, stale entries included (a single-player game, turns 21-26).
        // `update_opinion` rebuilds every entry it touches, so starting empty loses nothing else.
        // Game type: 1 multiplayer, 3 matchmaking, 4 competitive (only 1 is confirmed).
        if matches!(self.s.settings.game_type, 1 | 3 | 4) {
            mgr.opinions.clear();
            mgr.order.clear();
        }
        let mut total = 0.0f32;
        let mut peace = 0.0f32;
        let others: Vec<u8> = self.s.players.iter().map(|p| p.id).collect();
        for &other in &others {
            if !self.is_alive(other) || other == player {
                continue;
            }
            self.update_opinion(player, other, &ranked, &stats, &tiles, mgr);
            if self.at_peace(player, other) {
                peace += 1.0;
            }
            total += 1.0;
        }
        let denom = if 1.0 < total { total } else { 1.0 };
        let peace_factor = -peace / denom;
        // Every opponent not at peace gets the peace factor; the lowest opinion (first in dictionary order on ties)
        // then gets another -0.5.
        let mut worst: Option<u8> = None;
        let keys: Vec<u8> = mgr.order.clone();
        for pid in keys {
            if self.player(pid).is_none() {
                continue;
            }
            if !self.at_peace(player, pid) {
                if let Some(s) = mgr.opinions.get_mut(&pid) {
                    s.add(peace_factor, DISLIKE);
                }
            }
            let cur = mgr.opinions.get(&pid).map(|s| s.total).unwrap_or(0.0);
            match worst {
                Some(w) if !(mgr.opinions.get(&w).map(|s| s.total).unwrap_or(0.0) > cur) => {}
                _ => worst = Some(pid),
            }
        }
        if crate::env_var!("NO_WORST_DISLIKE").is_err() {
            if let Some(w) = worst {
                if let Some(s) = mgr.opinions.get_mut(&w) {
                    s.add(-0.5, DISLIKE);
                }
            }
        }
    }

    /// Update one opinion.
    fn update_opinion(
        &self,
        player: u8,
        opponent: u8,
        ranked: &[u8],
        stats: &[(u8, MilitaryStats)],
        tiles: &[Wc],
        mgr: &mut OpinionManager,
    ) {
        if player == opponent || player == NATURE || opponent == NATURE || !self.is_alive(opponent) {
            return;
        }
        let rel_pp = crate::rules::diplomacy::get_relation(&self.s, player, opponent);
        if rel_pp.first_meet < 0 {
            let rel_op = crate::rules::diplomacy::get_relation(&self.s, opponent, player);
            if rel_op.last_attack_turn < 0 {
                return;
            }
        }
        let mut obs = OpinionState::default();
        let winner = ranked.first().copied().unwrap_or(player);
        let count = ranked.len() as f32;
        let win_hate = self.win_hate(opponent, winner, count);
        let war = |a: u8, b: u8| crate::rules::diplomacy::has_war_with(&self.s, a, b);
        if winner != opponent && war(opponent, winner) && winner != player {
            obs.add(win_hate * win_hate, BRAVE);
        }
        if let Some(idx) = ranked.iter().position(|p| *p == opponent) {
            let n = (idx as i32 + 1) as f32;
            obs.add(win_hate * win_hate * 4.0 / (n * n), WINNING);
        }
        let j = self.judgement(player, opponent);
        if j > 0.0 {
            obs.add(j, COMMON_RELATION);
        }
        if 0.0 > j {
            obs.add(-j, DIFFERENT_RELATION);
        }
        // How long since they last attacked us, or since we met.
        let last_attack = crate::rules::diplomacy::get_last_attack(&self.s, opponent, player);
        let first_meet = crate::rules::diplomacy::get_relation(&self.s, player, opponent).first_meet;
        let turn = self.s.current_turn as i32;
        let elapsed = if last_attack < first_meet { turn - first_meet } else { turn - (last_attack + LONGSIGHT) };
        let x = elapsed as f32 / LONGSIGHT as f32;
        if x > 0.0 {
            let m = if 2.0 < x { 2.0 } else { x };
            obs.add(m * 0.5, PEACEFUL);
        } else {
            obs.add(x * -2.0, AGGRESSION);
        }
        if crate::rules::diplomacy::has_embassy_with(&self.s, player, opponent) {
            obs.add(1.0, EMBASSY);
        }
        if crate::rules::diplomacy::has_embassy_with(&self.s, opponent, player) {
            obs.add(1.0, EMBASSY);
        }
        for t in tiles {
            self.add_observations_for_tile(*t, player, opponent, &mut obs);
        }
        let opp_auto = self.player(opponent).map(|p| p.auto_play).unwrap_or(false);
        if !opp_auto {
            let handicap = self.player(player).map(|p| p.handicap).unwrap_or(0);
            let v = 2 - handicap;
            if v > 0 {
                obs.add((v as f32) + (v as f32), LIKE);
            }
        }
        let adv = Self::military_advantage(stats, player, opponent);
        let mil_branch = if crate::env_var!("OPN_B").is_ok() {
            opp_auto || (2 - self.player(player).map(|p| p.handicap).unwrap_or(0)) < 1
        } else {
            true
        };
        if mil_branch {
            if 1.0 > adv {
                obs.add(-(adv + -1.0), POWERFUL);
            } else if adv > 1.0 {
                obs.add(adv + -1.0, WEAK);
            }
        }
        let start = self.player(player).map(|p| p.start_tile).unwrap_or(Wc { x: -1, y: -1 });
        let intrusive_ok = if crate::env_var!("OPN_A").is_ok() { adv == 1.0 } else { true };
        if mil_branch && intrusive_ok && self.tile(start).is_some_and(|t| t.owner == opponent) {
            obs.add(2.0, INTRUSIVE);
        }
        // Rebuild the stored state from the observations in a fixed order.
        if !mgr.order.contains(&opponent) {
            mgr.order.push(opponent);
        }
        let state = mgr.opinions.entry(opponent).or_default();
        state.total = 0.0;
        state.reasons = FlatMap::new();
        for (kind, sign) in [
            (BRAVE, 1.0f32),
            (LIKE, 1.0),
            (EMBASSY, 1.0),
            (PEACEFUL, 1.0),
            (COMMON_RELATION, 1.0),
            (POWERFUL, 1.0),
            (THREATENING, -1.0),
            (AGGRESSION, -1.0),
            (DIFFERENT_RELATION, -1.0),
            (WEAK, -1.0),
            (WINNING, -1.0),
            (INTRUSIVE, -1.0),
        ] {
            let v = obs.get(kind);
            state.add(if sign < 0.0 { -v } else { v }, kind);
        }
    }

    /// Add the observations from one tile.
    fn add_observations_for_tile(&self, at: Wc, player: u8, opponent: u8, obs: &mut OpinionState) {
        let Some(tile) = self.tile(at) else { return };
        if let Some(u) = self.visible_unit_for(at, player) {
            if u.owner == opponent {
                let peace = self.at_peace(player, u.owner);
                if crate::env_var!("OBS_DBG").is_ok() {
                    eprintln!(
                        "OBS cmd {} p{} opp {} tile ({},{}) owner {} unit {} type {} invisible {} peace {}",
                        self.s.current_command,
                        player,
                        opponent,
                        at.x,
                        at.y,
                        tile.owner,
                        u.id,
                        u.type_idx,
                        u.invisible,
                        peace
                    );
                }
                if tile.owner == player {
                    obs.add(if peace { 0.2 } else { 1.0 }, THREATENING);
                    if tile.improvement == improvement::CITY {
                        obs.add(2.0, THREATENING);
                    }
                } else if !peace {
                    obs.add(0.5, THREATENING);
                }
            }
        }
        if tile.owner == opponent {
            obs.add(0.1, INTRUSIVE);
        }
    }

    /// The unit on a tile as seen by `viewer`; hidden units excluded.
    pub fn visible_unit_for(&self, at: Wc, viewer: u8) -> Option<&UnitState> {
        // Nothing on a tile the viewer has not explored.
        if viewer != NATURE && crate::env_var!("NO_GETUNIT_EXPLORED").is_err() && !self.is_explored_at(viewer, at) {
            return None;
        }
        let u = self.unit_at(at)?;
        if !u.invisible || u.owner == viewer || self.at_peace(viewer, u.owner) {
            Some(u)
        } else {
            None
        }
    }

    /// Aggression for versions above 59: the truncation toward zero of `-opinion * 1000`, so an
    /// opinion of `-inf` gives `i32::MIN`, not `i32::MAX`.
    pub fn aggression(&self, player: u8, other: u8) -> i32 {
        if self.s.version > 59 {
            crate::ai::score::cvtt_i32(self.opinion(player, other) * -1000.0)
        } else {
            1000
        }
    }
}
