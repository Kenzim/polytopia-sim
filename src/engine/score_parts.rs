//! `Engine::score` split into its parts (the player score, with the difficulty bonus that
//! weekly challenges zero). The parts are recomputed here from the state rather than taken from `compute_score`, so
//! the sum test in `tests/score_parts.rs` checks the two against each other.

use super::*;
use crate::rules::economy;
use std::collections::BTreeMap;

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ScoreParts {
    /// Owned tiles (including city and building tiles) x `TILE_VALUE` (20).
    pub territory: i32,
    /// Explored tiles x `EXPLORE_VALUE` (5).
    pub exploration: i32,
    /// Unit score summed over living units: 5 per star of the unit's cost (plus a passenger's).
    pub units: i32,
    /// Researched techs: 100 per tier (tech score: `cost * 100`).
    pub techs: i32,
    /// Cities: `(level + 1) * CITY_LEVEL_SCORE` (50).
    pub city_levels: i32,
    /// Cities: `CITY_XP_SCORE` (5) per point of population progress (`xp`).
    pub city_xp: i32,
    /// Cities: the runtime `base_score` (city rewards that grant score, e.g. the park).
    pub city_base: i32,
    /// Cities: the starvation term for negative `xp` (v > 0x72), including the clamp of a city's score at 0.
    pub city_starvation: i32,
    /// Non-city improvements: their runtime `base_score` (monuments, parks, temples' base, ...).
    pub buildings_base: i32,
    /// Non-city improvements with `patina` (temples): the per-level growth score.
    pub buildings_patina: i32,
    /// `buildings_base + buildings_patina` per improvement type.
    pub buildings_by_type: BTreeMap<i32, i32>,
    pub owned_tiles: i32,
    pub explored_tiles: i32,
    pub cities: i32,
}

impl ScoreParts {
    pub fn cities_total(&self) -> i32 {
        self.city_levels + self.city_xp + self.city_base + self.city_starvation
    }

    pub fn buildings_total(&self) -> i32 {
        self.buildings_base + self.buildings_patina
    }

    pub fn total(&self) -> i32 {
        self.territory + self.exploration + self.units + self.techs + self.cities_total() + self.buildings_total()
    }

    /// `(name, points)` for printing.
    pub fn named(&self) -> Vec<(&'static str, i32)> {
        vec![
            ("territory", self.territory),
            ("exploration", self.exploration),
            ("units", self.units),
            ("techs", self.techs),
            ("city_levels", self.city_levels),
            ("city_xp", self.city_xp),
            ("city_base", self.city_base),
            ("city_starvation", self.city_starvation),
            ("buildings_base", self.buildings_base),
            ("buildings_patina", self.buildings_patina),
        ]
    }
}

impl Engine {
    /// The parts of [`Engine::score`]; `score_parts(p).total() == score(p)`.
    pub fn score_parts(&self, player: u8) -> ScoreParts {
        let mut p = ScoreParts::default();
        for t in &self.s.map.tiles {
            if self.is_explored(player, t.index as usize) {
                p.explored_tiles += 1;
            }
            if t.owner != player {
                continue;
            }
            p.owned_tiles += 1;
            if t.improvement == 0 || player == 0 || player == NATURE {
                continue;
            }
            let Some(rt) = self.improvement_rt(t.coordinates) else { continue };
            let Some(info) = self.t.improvement(t.improvement) else { continue };
            let base = i32::from(rt.base_score);
            let patina = if info.abilities.contains(&improvement_ability::PATINA) {
                let per_level: i32 = info.data.growth_rewards.iter().map(|g| g.score.unwrap_or(0)).sum();
                i32::from(rt.level).max(0) * per_level
            } else {
                0
            };
            if t.improvement == improvement::CITY {
                p.cities += 1;
                let level = (i32::from(rt.level) + 1) * economy::CITY_LEVEL_SCORE;
                let xp = i32::from(rt.xp) * economy::CITY_XP_SCORE;
                let mut raw = level + base + xp;
                if self.s.version > 0x72 && rt.xp < 0 {
                    raw = (raw + i32::from(rt.xp) * economy::CITY_STARVATION_PENALTY).max(0);
                }
                p.city_levels += level;
                p.city_xp += xp;
                p.city_base += base;
                p.city_starvation += raw - (level + base + xp);
                // A city with patina is not in the data, but keep the sum exact if one appears.
                p.city_base += patina;
            } else {
                p.buildings_base += base;
                p.buildings_patina += patina;
                *p.buildings_by_type.entry(t.improvement).or_default() += base + patina;
            }
        }
        p.territory = p.owned_tiles * economy::TILE_VALUE;
        p.exploration = p.explored_tiles * economy::EXPLORE_VALUE;
        p.units = self.s.units.iter().filter(|u| u.owner == player).map(|u| self.unit_score(u)).sum();
        p.techs = self
            .techs(player)
            .iter()
            .filter_map(|&tc| self.t.tech(tc))
            .map(|td| economy::tech_score_points(td.cost))
            .sum();
        p
    }
}
