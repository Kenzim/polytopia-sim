//! The AI's diplomacy offers: peace offers, broken peace and embassies.

use super::*;
use crate::hash::HashFunction;

/// `DiplomacyData.embassyIncome`.
const EMBASSY_INCOME: i32 = 2;
/// The opinion above which the AI accepts or offers peace.
const LOVE_LIMIT: f32 = 2.0;
/// The null coordinate.
const NULL_COORD: Wc = Wc { x: -1, y: -1 };

impl Engine {
    /// Cost of an embassy with `player`.
    pub fn embassy_cost(&self, player: u8) -> i32 {
        if self.s.version > 0x71 {
            crate::rules::diplomacy::get_embassy_count(&self.s, player).wrapping_mul(2).wrapping_add(4)
        } else {
            5
        }
    }

    /// Whether `player` owns the capital of the opponent being considered.
    fn owns_their_capital(&self, player: u8) -> bool {
        self.player(player).and_then(|p| self.tile(p.start_tile)).is_some_and(|t| t.owner == player)
    }

    /// Whether a peace treaty command between `me` and `opp` is valid.
    fn peace_treaty_valid(&self, me: u8, opp: u8) -> bool {
        self.has_player_ability(me, player_ability::PEACE_TREATY)
            && !self.at_peace(me, opp)
            && !self.player_runtime(opp).is_some_and(|r| r.messages.contains(&(1, me)))
            && self.knows_player(me, opp)
            && self.is_alive(opp)
    }

    /// Whether to accept a peace request: accept when the opinion of the requester is above `LOVE_LIMIT`.
    pub fn should_accept_peace(&self, me: u8, opp: u8) -> bool {
        self.opinion(me, opp) > LOVE_LIMIT
    }

    /// Whether a break-peace command between `me` and `opp` is valid.
    fn break_peace_valid(&self, me: u8, opp: u8) -> bool {
        self.at_peace(me, opp) && self.knows_player(me, opp) && self.is_alive(opp)
    }

    /// Whether an establish-embassy command towards `opp` from the tile `at` is valid.
    fn embassy_valid(&self, me: u8, opp: u8, at: Wc) -> bool {
        let Some(tile) = self.tile(at) else { return false };
        let city_ok = tile.improvement == improvement::CITY && tile.owner != me && tile.owner != 0 && tile.owner == opp;
        let unit_ok = self.unit_at(at).is_some_and(|u| u.owner != me && u.owner == opp);
        if !city_ok && !unit_ok {
            return false;
        }
        let currency = self.player(me).map(|p| p.currency).unwrap_or(0);
        if currency < self.embassy_cost(me) {
            return false;
        }
        if !self.has_player_ability(me, player_ability::EMBASSY) {
            return false;
        }
        if crate::rules::diplomacy::has_embassy_with(&self.s, me, opp) {
            return false;
        }
        self.s.version <= 0x4f || self.owns_their_capital(opp)
    }

    /// Diplomacy commands towards one opponent, without the unavailable ones.
    pub fn diplomacy_commands_for(&self, me: u8, opp: u8) -> Vec<(Command, Wc)> {
        let mut out = Vec::new();
        if self.s.version < 0x3c || !self.knows_player(me, opp) || !self.is_alive(opp) {
            return out;
        }
        let cap = self.capital_of(opp).unwrap_or(NULL_COORD);
        let peace = self.at_peace(me, opp);
        let unlocked = self.has_player_ability(me, player_ability::PEACE_TREATY);
        if unlocked && !peace {
            if self.peace_treaty_valid(me, opp) {
                out.push((Command::PeaceTreaty { with: opp }, cap));
            }
        } else if peace && self.break_peace_valid(me, opp) {
            out.push((Command::BreakPeace { with: opp }, cap));
        }
        if self.owns_their_capital(opp)
            && self.has_player_ability(me, player_ability::EMBASSY)
            && self.embassy_valid(me, opp, cap)
        {
            out.push((Command::EstablishEmbassy { with: opp }, cap));
        }
        out
    }

    /// The AI's score for establishing an embassy with `opp`.
    fn embassy_score(&self, me: u8, opp: u8) -> f32 {
        let cost = self.embassy_cost(me);
        let currency = self.player(me).map(|p| p.currency).unwrap_or(0);
        let cost_f = cost as f32;
        let priced = (cost_f / (currency as f32).max(1.0) * 0.1 * cost_f).max(1.0);
        let agg = self.aggression(me, opp);
        let peace_bonus = if self.at_peace(me, opp) { (EMBASSY_INCOME * 5) as f32 } else { 0.0 };
        if agg > 5000 {
            return 0.0;
        }
        if crate::rules::diplomacy::has_war_with(&self.s, me, opp) {
            return 0.0;
        }
        let income = EMBASSY_INCOME as f32;
        let a = (agg as f32) * -0.001;
        peace_bonus + (a + (1.0 / priced) * income)
    }

    /// The AI's possible diplomacy commands with their scores.
    pub fn diplomacy_ai_commands(&self, me: u8) -> Vec<(Command, f32)> {
        let known: Vec<u8> = self.player_runtime(me).map(|r| r.known_players.clone()).unwrap_or_default();
        let mut cmds: Vec<(Command, Wc)> = Vec::new();
        for opp in known {
            cmds.extend(self.diplomacy_commands_for(me, opp));
        }
        let mut out = Vec::new();
        for (c, cap) in cmds {
            match c {
                Command::EstablishEmbassy { with } => {
                    let s = self.embassy_score(me, with);
                    if s > 0.0 {
                        out.push((c, s));
                    }
                }
                Command::PeaceTreaty { with } => {
                    let h = self.random_hash().value3(cap.x, cap.y, i32::from(self.s.current_command));
                    let op = self.opinion(me, with);
                    if h * op > LOVE_LIMIT {
                        out.push((c, 1000.0));
                    }
                }
                Command::BreakPeace { with } => {
                    let op = self.opinion(me, with);
                    if op < 0.0 {
                        let units = self
                            .s
                            .map
                            .tiles
                            .iter()
                            .filter(|t| t.owner == with && self.unit_at(t.coordinates).is_some_and(|u| u.owner == me))
                            .count() as i32;
                        let s = op * -500.0 - (units.wrapping_mul(200)) as f32;
                        if s > 0.0 {
                            out.push((c, s));
                        }
                    }
                }
                _ => {}
            }
        }
        out
    }
}
