//! The AI's research side: the tech needs of the explored map and the offers
//! the research commands score.

use super::*;
use crate::ai::score::{self, ResearchImprovement, ResearchOffer};
use crate::rules::economy::{required_tech, RequiredTechKind};
use std::collections::BTreeMap;

impl Engine {
    /// Whether the population this tech's improvements would add exceeds what the city needs to level.
    fn can_tech_upgrade_city(&self, city: Wc, acc: &[(Wc, f32)]) -> bool {
        let Some(value) = acc.iter().find(|e| e.0 == city).map(|e| e.1) else { return false };
        let Some(rt) = self.improvement_rt(city) else { return false };
        let need = i32::from(rt.level) - i32::from(rt.xp) + 1;
        value > need as f32
    }

    /// Tech needs: tech id -> how much the explored map asks for it.
    pub fn check_for_tech_needs(&self, me: u8) -> BTreeMap<i32, i32> {
        let mut needed: BTreeMap<i32, i32> = BTreeMap::new();
        let nodes = self.tech_nodes(self.tribe_of(me));
        let mut land_tiles = 0i32;
        let mut unconnected = 0i32;
        for t in self.s.map.tiles.iter() {
            if !self.is_explored_at(me, t.coordinates) {
                continue;
            }
            if t.owner == me {
                if t.improvement == improvement::CITY && !self.city_is_connected(t.coordinates) {
                    unconnected += 1;
                }
                if t.terrain == terrain::FOREST || t.terrain == terrain::FIELD {
                    land_tiles += 1;
                }
            }
            if !self.can_be_accessed_by_player(t, me) {
                if let Some(tech) = required_tech(nodes, RequiredTechKind::TerrainMovement, t.terrain) {
                    if self.dbg("NEED_DEBUG") {
                        eprintln!(
                            "NEEDTILE p{} ({},{}) terrain {} tech {} wet {} owner {}",
                            me,
                            t.coordinates.x,
                            t.coordinates.y,
                            t.terrain,
                            tech,
                            self.is_wetland_index(t.index),
                            t.owner
                        );
                    }
                    *needed.entry(tech).or_insert(0) += 1;
                }
            }
            if t.resource != 0 && self.resource_visible_to(t.resource, me) {
                for imp in self.improvements_for_resource(t.resource) {
                    let freelance =
                        self.t.improvement(imp).is_some_and(|i| i.abilities.contains(&improvement_ability::FREELANCE));
                    if freelance && !self.improvement_unlocked(me, imp) {
                        if let Some(tech) = required_tech(nodes, RequiredTechKind::Improvement, imp) {
                            *needed.entry(tech).or_insert(0) += 5;
                        }
                    }
                }
            }
        }
        if land_tiles > 0 {
            *needed.entry(4).or_insert(0) += (unconnected + 1) * land_tiles;
        }
        needed
    }

    /// Every researchable tech with its research command score.
    pub fn research_candidates(&self, me: u8, need_for_units: f32) -> Vec<(Command, f32)> {
        let cities = self.recorded_city_count(me);
        let currency = self.player(me).map(|p| p.currency).unwrap_or(0);
        let needed = self.check_for_tech_needs(me);
        if self.dbg("RES_DEBUG") {
            eprintln!("NEEDED p{} {:?}", me, needed);
        }
        let desired = self.desired_stats(me, None);
        let ai = self.ai(me);
        let empire = ai.empire_tiles.clone();
        let city_count = ai.city_tiles.len() as f32;
        let mut out = Vec::new();
        for tech in self.researchable(me) {
            let price = self.tech_price(me, tech);
            if crate::env_var!("RES_FILTER").is_ok() && price > currency {
                continue;
            }
            let Some(td) = self.t.tech(tech) else { continue };
            let mut unit_scores = Vec::new();
            for name in &td.unit_unlocks {
                if let Some(idx) = self.tribe_unit(me, name) {
                    if let Some(info) = self.t.unit(idx) {
                        unit_scores.push(score::get_build_unit_score(
                            info.data.defence,
                            info.data.attack,
                            info.data.movement,
                            info.data.range,
                            &desired,
                            &info.abilities,
                            None,
                            need_for_units,
                        ));
                    }
                }
            }
            let mut improvements = Vec::new();
            let mut upgrade_cities: Vec<(Wc, f32)> = Vec::new();
            for name in &td.improvement_unlocks {
                let Some(kind) = self.tribe_improvement(me, name) else { continue };
                let Some(info) = self.t.improvement(kind) else { continue };
                if info.data.hidden {
                    continue;
                }
                let mut sum = 0.0f32;
                for &tile in &empire {
                    let s = self.ai_improvement_score(me, tile, kind);
                    if s != 0.0 {
                        // A cheap-per-population improvement marks its ruling city as upgradeable by this tech.
                        let mut pop: i32 = info.population;
                        if !info.data.adjacency_improvements.is_empty() {
                            let growth: i32 = info.data.growth_rewards.iter().map(|g| g.population.unwrap_or(0)).sum();
                            pop += growth * self.adjacency_bonus(tile, kind);
                        }
                        let cost = info.data.cost.unwrap_or(0) as f32;
                        let ruling = self.tile(tile).map(|t| t.ruling_city).unwrap_or(Wc { x: -1, y: -1 });
                        if (ruling.x != -1 || ruling.y != -1) && cost / (pop as f32) < 5.0 {
                            match upgrade_cities.iter_mut().find(|e| e.0 == ruling) {
                                Some(e) => e.1 += pop as f32,
                                None => upgrade_cities.push((ruling, pop as f32)),
                            }
                        }
                        if self.dbg("RES_DEBUG") {
                            eprintln!("    tile ({},{}) kind {} score {}", tile.x, tile.y, kind, s);
                        }
                        sum += s;
                    }
                }
                improvements.push(ResearchImprovement {
                    score_sum: sum,
                    limited: info.abilities.contains(&improvement_ability::LIMITED),
                    limited_scale: city_count,
                });
            }
            let offer = ResearchOffer {
                player: me,
                tech,
                price,
                cities,
                unit_build_scores: unit_scores,
                improvements,
                ability_unlocks: td.ability_unlocks.len() as i32,
                upgradeable_cities: if crate::env_var!("NO_UPGRADE_TECH").is_ok() {
                    0
                } else {
                    ai.city_tiles.iter().filter(|c| self.can_tech_upgrade_city(**c, &upgrade_cities)).count() as i32
                },
                need: needed.get(&tech).copied().unwrap_or(0),
            };
            if self.dbg("RES_DEBUG") {
                eprintln!("RESEARCH p{} tech {} offer {:?}", me, tech, offer);
            }
            for c in score::add_possible_research_commands(currency, &[offer]) {
                out.push((Command::Research { tech }, c.score));
            }
        }
        out
    }
}
