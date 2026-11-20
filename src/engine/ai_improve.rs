//! Improvement scoring and build offers.

use super::*;
use crate::ai::score::{self, ImprovementOffer, ImprovementScoreInput, RulingCityScore};

impl Engine {
    /// Whether the city tile is connected to its owner's capital.
    pub fn city_is_connected(&self, at: Wc) -> bool {
        let Some(t) = self.tile(at) else { return false };
        if t.improvement != improvement::CITY || t.owner == 0 {
            return false;
        }
        // Connected means the connection owner is the tile owner or the capital belongs to it. A captured enemy
        // capital (its first owner is the capital owner) is not connected.
        let connected = self.improvement_rt(at).is_some_and(|r| r.connected_to_capital_of_player == t.owner);
        connected || t.capital_of == t.owner
    }

    /// Whether the improvement lists routes (a route opener).
    pub fn is_route_opener(&self, kind: i32) -> bool {
        self.t.improvement(kind).is_some_and(|i| !i.data.routes.is_empty())
    }

    /// The score for connecting a city, from the cities within `range`.
    pub fn connection_score(&self, me: u8, at: Wc, range: i32) -> f32 {
        let start = self.player(me).map(|p| p.start_tile).unwrap_or(Wc { x: -1, y: -1 });
        let mut count = 0.0f32;
        let mut factor = 0.1f32;
        for c in self.area_iter(at, range) {
            let Some(t) = self.tile(c) else { continue };
            if t.improvement != improvement::CITY || t.owner != me {
                continue;
            }
            count += 1.0;
            if self.city_is_connected(c) || start == c {
                factor = 1.0;
            } else {
                count += 0.5;
            }
        }
        count * count * factor * 5.0
    }

    /// The score of one improvement ability on a tile.
    pub fn improvement_ability_score(&self, me: u8, kind: i32, at: Wc) -> f32 {
        let Some(info) = self.t.improvement(kind) else { return 0.0 };
        let mut score = 0.0f32;
        let mut algae = false;
        for c in &info.data.creates {
            if c.effect.as_deref() == Some("algae") {
                algae = true;
            }
            if c.resource.is_some() {
                score += 20.0;
            }
        }
        let embark = info.abilities.contains(&improvement_ability::EMBARK);
        let bridge = info.abilities.contains(&improvement_ability::BRIDGE);
        if algae || embark || bridge {
            if let Some(v) = self.ai(me).blocked_tiles.get(&(at.x, at.y)) {
                score += *v;
            }
        }
        if info.abilities.contains(&improvement_ability::NETWORK) {
            score += self.connection_score(me, at, info.data.range.unwrap_or(0));
        }
        score
    }

    /// The AI's score for building an improvement on a tile.
    pub fn ai_improvement_score(&self, me: u8, at: Wc, kind: i32) -> f32 {
        let Some(info) = self.t.improvement(kind) else { return 0.0 };
        let Some(tile) = self.tile(at).copied() else { return 0.0 };
        let can_build = self.can_build(me, at, kind, i32::MAX);
        if !can_build {
            return 0.0;
        }
        let growth_population: i32 = info.data.growth_rewards.iter().map(|g| g.population.unwrap_or(0)).sum();
        let reward_score: i32 = info.data.rewards.iter().map(|r| r.score.unwrap_or(0)).sum();
        let ai = self.ai(me);
        let created_unit_score = info
            .data
            .creates
            .iter()
            .find_map(|c| c.unit.as_ref())
            .and_then(|n| self.t.unit_idx.get(n).copied())
            .and_then(|u| self.t.unit(u))
            .map(|u| {
                let desired = self.desired_stats(me, Some(at));
                let pos = self.hypothetical_position_score(me, u.data.idx, at);
                score::get_build_unit_score(
                    u.data.defence,
                    u.data.attack,
                    u.data.movement,
                    u.data.range,
                    &desired,
                    &u.abilities,
                    Some(pos),
                    ai.unit_need,
                )
            });
        let adjacency_bonus =
            if info.data.adjacency_improvements.is_empty() { None } else { Some(self.adjacency_bonus(at, kind)) };
        let ruling = tile.ruling_city;
        let ruling_city = if ruling.x >= 0 && ruling.y >= 0 {
            self.tile(ruling).map(|rt| {
                let rs = self.improvement_rt(ruling);
                RulingCityScore {
                    is_city: rt.improvement == improvement::CITY,
                    xp: rs.map(|r| r.xp).unwrap_or(0),
                    level: rs.map(|r| r.level).unwrap_or(0),
                    is_connected: self.city_is_connected(ruling),
                    neighbor_route_opener: self.area_iter(ruling, 1).filter(|c| *c != ruling).any(|c| {
                        self.tile(c).is_some_and(|t| t.improvement != 0 && self.is_route_opener(t.improvement))
                    }),
                    same_as_start_tile: self.player(me).is_some_and(|p| p.start_tile == ruling),
                }
            })
        } else {
            None
        };
        let resource_required = {
            let res = if tile.resource != 0 && self.resource_visible_to(tile.resource, me) { tile.resource } else { 0 };
            if res == 0 {
                None
            } else {
                Some(info.resource_req.contains(&res))
            }
        };
        let input = ImprovementScoreInput {
            can_build,
            reward_population: info.population,
            growth_population,
            has_patina: info.abilities.contains(&improvement_ability::PATINA),
            max_level: info.data.max_level.unwrap_or(0),
            reward_currency: info.currency,
            work: info.data.work.unwrap_or(0),
            cost: info.data.cost.unwrap_or(0),
            created_unit_score,
            adjacency_bonus,
            adjacency_contains_polaris_climate: info.data.adjacency_improvements.iter().any(|a| {
                a.improvement.as_ref().and_then(|n| self.t.improvement_idx.get(n))
                    == Some(&crate::engine::tables::improvement::POLARIS_CLIMATE)
            }),
            frozen_tile_count: crate::ai::decide::frozen_tiles_owned(&self.s, me),
            ruling_city,
            is_route_opener: self.is_route_opener(kind),
            player_cities: self.player(me).map(|_| self.recorded_city_count(me)).unwrap_or(0),
            rules_are_perfection: self.s.settings.rules_game_mode == 1,
            reward_score,
            ability_score: self.improvement_ability_score(me, kind, at),
            improvement_idx: kind,
            resource_required,
        };
        score::get_improvement_score(&input)
    }

    /// Position rating of a unit of `type_idx` as if it stood on `at`.
    pub fn hypothetical_position_score(&self, me: u8, type_idx: i32, at: Wc) -> f32 {
        let Some(info) = self.t.unit(type_idx) else { return 0.0 };
        let u = UnitState {
            id: 0,
            owner: me,
            type_idx,
            coordinates: at,
            previous_turn_end: at,
            home: at,
            health: info.data.health as u16,
            ..UnitState::default()
        };
        self.rate_position_unit(me, &u, at, false)
    }

    /// Scored `Build` commands.
    pub fn improvement_commands(&self, me: u8) -> Vec<(Command, f32)> {
        let mut legal = Vec::new();
        self.build_commands_with(
            me,
            if crate::env_var!("IMP_AFFORD").is_ok() {
                self.player(me).map(|p| p.currency).unwrap_or(0)
            } else {
                i32::MAX
            },
            &mut legal,
        );
        if crate::env_var!("TILE_ORDER").as_deref() == Ok("desc") {
            // Experiment knob: descending tile order (retail evidence favours ascending).
            let mut groups: Vec<Vec<Command>> = Vec::new();
            for c in legal.drain(..) {
                let at = if let Command::Build { at, .. } = c { Some(at) } else { None };
                match (groups.last_mut(), at) {
                    (Some(g), Some(a)) if matches!(g[0], Command::Build { at: b, .. } if b == a) => g.push(c),
                    _ => groups.push(vec![c]),
                }
            }
            groups.reverse();
            legal = groups.into_iter().flatten().collect();
        }
        let currency = self.player(me).map(|p| p.currency).unwrap_or(0);
        let ai = self.ai(me);
        let mut offers = Vec::new();
        let mut cmds = Vec::new();
        for c in legal {
            let Command::Build { at, improvement: kind } = c else { continue };
            let Some(info) = self.t.improvement(kind) else { continue };
            // The buildable-improvements list skips Manual improvements; only a unit builds those (see the unit action options).
            if info.abilities.contains(&improvement_ability::MANUAL) {
                continue;
            }
            let Some(tile) = self.tile(at) else { continue };
            let ruling = tile.ruling_city;
            let growth: i32 = info.data.growth_rewards.iter().map(|g| g.population.unwrap_or(0)).sum();
            let upgrade_need = if ruling.x >= 0 {
                self.improvement_rt(ruling).map(|r| i32::from(r.level) - i32::from(r.xp) + 1).unwrap_or(0)
            } else {
                0
            };
            let tracked = ruling.x >= 0 && ai.upgradeable_cities.contains(&ruling);
            offers.push(ImprovementOffer {
                player: me,
                improvement: kind,
                at,
                base_score: self.ai_improvement_score(me, at, kind),
                cost: info.data.cost.unwrap_or(0),
                reward_population: info.population,
                growth_population: growth,
                city_tracked: tracked,
                upgrade_need: if tracked { upgrade_need } else { 0 },
            });
        }
        if self.dbg("IMP_DEBUG") {
            for o in &offers {
                eprintln!("IMPOFFER p{} {:?}", me, o);
            }
        }
        for o in score::add_possible_improvement_commands(currency, &offers) {
            if let score::TileCommand::Build { improvement, at, .. } = o.command {
                cmds.push((Command::Build { at, improvement }, o.score));
            }
        }
        cmds
    }

    /// Whether the city tile is connected to its owner's capital (or is that capital).
    pub fn city_tile_connected(&self, city: Wc) -> bool {
        let Some(t) = self.tile(city) else { return false };
        let Some(rt) = self.improvement_rt(city) else { return false };
        if t.improvement != improvement::CITY {
            return false;
        }
        let owner = t.owner;
        owner != 0 && (rt.connected_to_capital_of_player == owner || t.capital_of == owner)
    }

    /// City potential: ten per city level plus ten per visible resource without an improvement within the border.
    pub fn city_potential(&self, me: u8, city: Wc) -> i32 {
        let Some(t) = self.tile(city) else { return 0 };
        if t.improvement != improvement::CITY {
            return 0;
        }
        let Some(rt) = self.improvement_rt(city) else { return 0 };
        let mut potential = i32::from(rt.level) * 10;
        let mut area = self.area(city, i32::from(rt.border_size));
        area.push(city);
        for c in area {
            let Some(tt) = self.tile(c) else { continue };
            if tt.resource != 0 && self.resource_visible_to(tt.resource, me) && tt.improvement == 0 {
                potential += 10;
            }
        }
        potential
    }

    /// For every unconnected city, a road plan along the cheapest path
    /// from the capital; each tile without a road becomes a Road build scored `potential / remaining tiles`.
    pub fn road_commands(&self, me: u8) -> Vec<(Command, f32)> {
        use crate::rules::path::{self, PathSettings};
        let mut out = Vec::new();
        let Some(start) = self.player(me).map(|p| p.start_tile) else { return out };
        if !self.unlocked_improvements(me).contains(&improvement::ROAD) {
            // The command list is only built when Road is unlocked (checked per tile in the original).
            return out;
        }
        let allowed = [terrain::FOREST, terrain::FIELD];
        let abilities: Vec<i32> = Vec::new();
        let settings = PathSettings {
            version: self.s.version,
            player_id: me,
            allowed_terrain: &allowed,
            abilities: &abilities,
            mover: None,
            require_port: true,
            allow_occupied: true,
            allow_enemy: false,
            allow_allies: false,
            can_player_embark: self.can_player_embark(me),
        };
        let tiles = &self.s.map.tiles;
        let width = self.s.map.width as i32;
        let cost = |_from: Wc, to: Wc| -> i32 {
            let idx = (to.y * width + to.x) as usize;
            match tiles.get(idx) {
                Some(t)
                    if t.has_road
                        || t.improvement == improvement::CITY
                        || t.improvement == improvement::BRIDGE
                        || t.has_route =>
                {
                    5
                }
                _ => 10,
            }
        };
        for city in self.ai(me).city_tiles.clone() {
            if self.city_tile_connected(city) {
                continue;
            }
            let found = path::get_path(
                &self.s,
                start,
                city,
                1000,
                &settings,
                |i| self.is_wetland_index(i),
                |kind, ability| self.t.has_improvement_ability(kind, ability),
                |_| 8,
                cost,
            );
            let Some(mut found) = found else { continue };
            found.retain(|c| {
                !self.tile(*c).is_some_and(|t| {
                    t.has_road || t.improvement == improvement::CITY || t.improvement == improvement::BRIDGE
                })
            });
            let potential = self.city_potential(me, city);
            if found.is_empty() {
                continue;
            }
            let score = potential as f32 / found.len() as f32;
            for c in found {
                out.push((Command::Build { at: c, improvement: improvement::ROAD }, score));
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::json::Json;

    const PORT: i32 = 8;
    const WATER: i32 = 1;
    const FIELD: i32 = 3;

    /// A 7x4 board: water along the bottom row, land above it, all of it but the last column ruled by player 1's
    /// capital in the corner. Player 1 (the AI, to move) knows every tech and has a warrior beside the water.
    fn board() -> Engine {
        let (w, h) = (7, 4);
        let mut tiles = Vec::new();
        for y in 0..h {
            for x in 0..w {
                let terrain = if y == h - 1 { WATER } else { FIELD };
                let mut t = format!(r#""x":{x},"y":{y},"terrain":{terrain},"climate":2"#);
                if (x, y) == (0, 0) {
                    t.push_str(r#","city":true,"capital":true,"owner":1,"ruling":[0,0]"#);
                } else if (x, y) == (w - 1, 0) {
                    t.push_str(r#","city":true,"capital":true,"owner":2,"ruling":[6,0]"#);
                } else if x < w - 1 {
                    t.push_str(r#","owner":1,"ruling":[0,0]"#);
                }
                tiles.push(format!("{{{t}}}"));
            }
        }
        let techs = (0..60).map(|t| t.to_string()).collect::<Vec<_>>().join(",");
        let text = format!(
            r#"{{"format":"polytopia-position","game":{{"game_version":124,"turn":6,"width":{w},"height":{h},"current_player":1}},
               "players":[{{"id":1,"tribe":3,"stars":30,"tech":[{techs}]}},{{"id":2,"tribe":3,"stars":5}}],
               "tiles":[{}],
               "improvements":[{{"x":0,"y":0,"level":1}},{{"x":6,"y":0,"level":1}}],
               "units":[{{"id":1,"type":2,"owner":1,"x":3,"y":2,"health":100}}]}}"#,
            tiles.join(",")
        );
        Engine::from_position(&Json::parse(&text).unwrap()).unwrap()
    }

    fn port_scores(e: &Engine) -> Vec<(Wc, f32)> {
        e.improvement_commands(1)
            .into_iter()
            .filter_map(|(c, s)| match c {
                Command::Build { at, improvement } if improvement == PORT => Some((at, s)),
                _ => None,
            })
            .collect()
    }

    /// A port on a tile the AI rates as dangerous scores below zero. Retail adds the improvement's reward
    /// population only to a score above zero, so the port is not a candidate; the build itself stays legal.
    #[test]
    fn a_port_with_a_score_below_zero_gets_no_population_bonus() {
        let mut e = board();
        e.refresh_map_data_pub(1);
        e.analyze_situation_pub(1);
        let tile = Wc { x: 2, y: 3 };
        let other = Wc { x: 0, y: 3 };
        assert!(e.can_build(1, tile, PORT, i32::MAX), "a port is legal on the water tile");
        assert!(e
            .legal_commands()
            .iter()
            .any(|c| matches!(c, Command::Build { at, improvement } if *at == tile && *improvement == PORT)));

        let calm = port_scores(&e);
        assert!(calm.iter().any(|(at, _)| *at == tile), "offered while nothing counts against it: {calm:?}");

        // The tile's blocked value is the unit position ratings around it; a negative one outweighs the port's
        // own score (25 for its one population).
        e.ai_mut(1).blocked_tiles.insert((tile.x, tile.y), -30.0);
        let base = e.ai_improvement_score(1, tile, PORT);
        assert!(base < 0.0, "the port scores below zero, not {base}");
        let dangerous = port_scores(&e);
        assert!(!dangerous.iter().any(|(at, _)| *at == tile), "not offered: {dangerous:?}");
        assert!(dangerous.iter().any(|(at, _)| *at == other), "ports elsewhere still are: {dangerous:?}");
    }
}
