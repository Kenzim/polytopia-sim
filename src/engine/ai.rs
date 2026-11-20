//! The retail AI (the game's computer player): situation analysis, missions, hit list.
//!
//! Each function follows the retail routine of the same purpose. The situation analysis runs before every command
//! the AI issues (the move driver calls the analysis again, which re-analyses
//! unconditionally), and its noise terms change with the command counter.

use super::ai_state::*;
use super::*;
use crate::hash::{HashFunction, XxHash};

/// Mission type ids.
pub const MISSION_NONE: i32 = 0;
pub const MISSION_CAPTURE: i32 = 1;

impl Engine {
    /// The player's AI state (an empty one if it has none yet).
    pub fn ai(&self, player: u8) -> &AiState {
        static EMPTY: std::sync::LazyLock<AiState> = std::sync::LazyLock::new(AiState::default);
        self.ai_states.get(&player).unwrap_or(&EMPTY)
    }

    pub(crate) fn ai_mut(&mut self, player: u8) -> &mut AiState {
        self.ai_states.entry(player).or_default()
    }

    /// The random-hash stream: an xxHash seeded with the game seed.
    pub fn random_hash(&self) -> XxHash {
        XxHash::new(self.s.seed)
    }

    /// The player's stupidity, from its auto-play flag and handicap.
    pub fn stupidity(&self, player: u8) -> i32 {
        self.player(player).map(crate::ai::decide::player_stupidity).unwrap_or(0)
    }

    /// The stupid factor: the stupidity times a hash-derived value centred on zero.
    pub fn stupid_factor(&self, player: u8, c1: i32, c2: i32) -> f32 {
        let off: i32 = crate::env_var!("NOISE_OFF").ok().and_then(|v| v.parse().ok()).unwrap_or(0);
        let h = self.random_hash().value3((i32::from(self.s.current_command) + off).wrapping_mul(1000), c1, c2);
        crate::ai::decide::stupid_factor(h, self.stupidity(player))
    }

    /// The terrain is one the player's techs let units enter, or the tile has a bridge. Retail compares the tile's raw
    /// terrain with the player's unlocked movement terrains and looks at nothing else, so a flooded or algae tile does
    /// not count as wetland here (it does for building and movement, which test the effects themselves).
    pub fn can_be_accessed_by_player(&self, tile: &Tile, player: u8) -> bool {
        self.can_be_accessed_with(tile, &self.unlocked_movement_terrain(player))
    }

    /// [`Self::can_be_accessed_by_player`] with the player's `unlocked_movement_terrain` passed in.
    fn can_be_accessed_with(&self, tile: &Tile, allowed: &[i32]) -> bool {
        allowed.contains(&tile.terrain)
            // An improvement with the Bridge ability makes the tile accessible.
            || (crate::env_var!("NO_BRIDGE_ACCESS").is_err() && tile.improvement != 0 && self.t.has_improvement_ability(tile.improvement, improvement_ability::BRIDGE))
    }

    /// The situation analysis that runs before the AI issues a command.
    pub fn analyze_situation(&mut self, player: u8) {
        self.ai_mut(player).clear();
        if player == NATURE {
            return;
        }
        let mut local: Vec<Mission> = Vec::new();
        let phase = crate::env_var!("BLOCK_PHASE").ok().and_then(|v| v.chars().next()).unwrap_or('M');
        let mut deferred: Vec<u32> = Vec::new();
        let mut explored_count = 0i32;
        let mut rough = 0i32;
        let mut explored_f = 0.0f32;
        let tile_count = self.s.map.tiles.len();
        // The loop below changes only AI state, opinions and the population cache, never the player's techs.
        let movement_terrain = self.unlocked_movement_terrain(player);
        let mut resource_missions: Vec<(i32, usize)> = Vec::new();
        for ti in 0..tile_count {
            let tile = self.s.map.tiles[ti];
            if !self.is_explored(player, ti) {
                continue;
            }
            explored_f += 1.0;
            if tile.terrain == terrain::MOUNTAIN || tile.terrain == terrain::FOREST {
                rough += 1;
            }
            let _ = explored_count;
            if self.can_be_accessed_with(&tile, &movement_terrain) {
                let radius = if tile.terrain == terrain::MOUNTAIN { 2 } else { 1 };
                let unexplored =
                    self.area_iter(tile.coordinates, radius).filter(|c| !self.is_explored_at(player, *c)).count()
                        as i32;
                if let Some(score) = crate::ai::decide::explore_mission_score(unexplored) {
                    local.push(Mission { kind: MISSION_NONE, tile: tile.coordinates, score });
                }
            }
            if tile.improvement == improvement::RUIN && self.unit_at(tile.coordinates).is_none() {
                self.ai_mut(player).missions.push(Mission { kind: MISSION_NONE, tile: tile.coordinates, score: 30.0 });
            }
            if tile.improvement == improvement::CITY {
                self.make_missions_for_city(player, tile.coordinates);
                if self.can_city_be_upgraded(tile.coordinates) {
                    self.ai_mut(player).upgradeable_cities.push(tile.coordinates);
                }
            }
            if tile.resource != 0 {
                // Missions per resource tile depend only on the resource and the player's techs and tasks.
                let n = match resource_missions.iter().find(|e| e.0 == tile.resource) {
                    Some(e) => e.1,
                    None => {
                        let n = if self.resource_visible_to(tile.resource, player) {
                            self.improvements_for_resource(tile.resource)
                                .into_iter()
                                .filter(|&imp| {
                                    self.t.has_improvement_ability(imp, improvement_ability::MANUAL)
                                        && self.improvement_unlocked(player, imp)
                                })
                                .count()
                        } else {
                            0
                        };
                        resource_missions.push((tile.resource, n));
                        n
                    }
                };
                for _ in 0..n {
                    self.ai_mut(player).missions.push(Mission {
                        kind: MISSION_NONE,
                        tile: tile.coordinates,
                        score: 30.0,
                    });
                }
            }
            if let Some(ui) = self.unit_index_at(tile.coordinates) {
                self.collect_military_stats_from_tile(player, ui);
                let owner = self.s.units[ui].owner;
                if owner == player {
                    let unit_id = self.s.units[ui].id;
                    if phase == 'M' {
                        for n in self.neighbors(tile.coordinates) {
                            let rate = self.rate_position(player, unit_id, n, false);
                            self.ai_mut(player).analysis_rates.push((unit_id, n.x, n.y, rate));
                            let e = self.ai_mut(player).blocked_tiles.entry((n.x, n.y)).or_insert(0.0);
                            *e = *e + rate + rate;
                        }
                    } else {
                        deferred.push(unit_id);
                    }
                } else {
                    self.update_hitlist_for_unit(player, tile.coordinates);
                }
            }
        }
        explored_count = explored_f as i32;
        if phase == 'A' {
            self.blocked_pass(player, &deferred);
        }
        // Exploration missions: best `3 * cities` (at least 1) go onto the player.
        if crate::env_var!("MORD_NOSORT").is_err() {
            sort_desc_by_score(&mut local, |m| m.score);
        }
        let cities = self.city_tiles(player).len() as i32;
        let keep = crate::ai::decide::explore_missions_kept(cities).max(1) as usize;
        if crate::env_var!("MORD_LOCALFIRST").is_ok() {
            let mut keepv: Vec<Mission> = self.ai(player).missions.clone();
            let mut front: Vec<Mission> = local.iter().take(keep).copied().collect();
            front.append(&mut keepv);
            self.ai_mut(player).missions = front;
        } else {
            for m in local.iter().take(keep) {
                self.ai_mut(player).missions.push(*m);
            }
        }
        if phase == 'B' {
            self.blocked_pass(player, &deferred);
        }
        let rough_ratio = crate::ai::decide::rough_terrain_ratio(rough, explored_count);
        {
            let ai = self.ai_mut(player);
            ai.rough_terrain = rough_ratio;
            ai.explored_tiles = explored_f;
        }
        self.adjust_military_needs(player);
        // Noise for every mission, then every hit-list entry, indices 501.. in sequence.
        if crate::env_var!("MORD_ALLSORT").is_ok() {
            let ai = self.ai_mut(player);
            sort_desc_by_score(&mut ai.missions, |m| m.score);
        }
        let shift: i32 = crate::env_var!("MISS_SHIFT").ok().and_then(|v| v.parse().ok()).unwrap_or(0);
        let mut idx = crate::ai::decide::mission_noise_y(0).wrapping_add(shift);
        let n_missions = self.ai(player).missions.len();
        for i in 0..n_missions {
            let ov: Option<(i32, i32, i32)> = crate::env_var!("MISS_OVERRIDE").ok().and_then(|v| {
                let p: Vec<i32> = v.split(',').filter_map(|x| x.parse().ok()).collect();
                if p.len() == 3 {
                    Some((p[0], p[1], p[2]))
                } else {
                    None
                }
            });
            let this_idx = match ov {
                Some((x, y, j))
                    if self.ai(player).missions[i].tile.x == x && self.ai(player).missions[i].tile.y == y =>
                {
                    mission_noise_base_for_override(j)
                }
                _ => idx,
            };
            let mut noise =
                if crate::env_var!("NO_MISS_NOISE").is_ok() { 0.0 } else { self.stupid_factor(player, this_idx, 0) };
            if let Some((x, y, j)) = ov {
                if self.ai(player).missions[i].tile.x == x && self.ai(player).missions[i].tile.y == y {
                    let co: i32 = crate::env_var!("MISS_CMD_OFF").ok().and_then(|v| v.parse().ok()).unwrap_or(0);
                    let c2: i32 = match crate::env_var!("MISS_C2").as_deref() {
                        Ok("x") => x,
                        Ok("y") => y,
                        Ok("idx") => this_idx,
                        Ok("tile") => self.tile(Wc { x, y }).map(|t| t.index).unwrap_or(0),
                        _ => 0,
                    };
                    let h = self.random_hash().value3(
                        (i32::from(self.s.current_command) + co).wrapping_mul(1000),
                        mission_noise_base_for_override(j),
                        c2,
                    );
                    noise = crate::ai::decide::stupid_factor(h, self.stupidity(player));
                }
            }
            let m = &mut self.ai_mut(player).missions[i];
            m.score += noise;
            idx += 1;
        }
        let n_hits = self.ai(player).hit_list.len();
        for i in 0..n_hits {
            let noise = self.stupid_factor(player, idx, 0);
            let h = &mut self.ai_mut(player).hit_list[i];
            h.score += noise;
            idx += 1;
        }
        if phase == 'C' {
            self.blocked_pass(player, &deferred);
        }
        let ai = self.ai_mut(player);
        sort_desc_by_score(&mut ai.missions, |m| m.score);
        sort_desc_by_score(&mut ai.hit_list, |h| h.score);
    }

    fn blocked_pass(&mut self, player: u8, units: &[u32]) {
        for &unit_id in units {
            let Some(at) = self.unit(unit_id).map(|u| u.coordinates) else { continue };
            for n in self.neighbors(at) {
                let rate = self.rate_position(player, unit_id, n, false);
                self.ai_mut(player).analysis_rates.push((unit_id, n.x, n.y, rate));
                let e = self.ai_mut(player).blocked_tiles.entry((n.x, n.y)).or_insert(0.0);
                *e = *e + rate + rate;
            }
        }
    }

    /// Missions for one city.
    fn make_missions_for_city(&mut self, player: u8, at: Wc) {
        let Some(tile) = self.tile(at).copied() else { return };
        if crate::env_var!("AGG_DBG").is_ok() && at.x == 10 && at.y == 11 {
            eprintln!(
                "MFC cmd {} turn {} p{} owner {} unit {:?} peace {}",
                self.s.current_command,
                self.s.current_turn,
                player,
                tile.owner,
                self.unit_at(at).map(|u| u.owner),
                self.at_peace(player, tile.owner)
            );
        }
        let owner = tile.owner;
        let unit_owner = self.unit_at(at).map(|u| u.owner);
        if owner == 0 {
            if unit_owner.is_some() {
                return;
            }
            self.ai_mut(player).missions.push(Mission { kind: MISSION_CAPTURE, tile: at, score: 50.0 });
        } else if owner == player {
            match unit_owner {
                Some(o) if o != owner => {}
                _ => return,
            }
            self.ai_mut(player).missions.push(Mission { kind: MISSION_NONE, tile: at, score: 200.0 });
        } else {
            if self.at_peace(player, owner) {
                return;
            }
            let opinion = self.opinion(player, owner);
            if opinion < 0.0 {
                let pop = self.improvement_rt(at).map(|r| i32::from(r.population)).unwrap_or(0);
                let ai = self.ai_mut(player);
                ai.enemy_cities += 1;
                ai.enemy_population += pop.min(5);
            }
            let aggression = self.aggression(player, owner) as f32;
            if crate::env_var!("AGG_DBG").is_ok() && at.x == 10 && at.y == 11 {
                eprintln!(
                    "AGG cmd {} turn {} p{} owner {} agg {} opinion {} unit {:?}",
                    self.s.current_command, self.s.current_turn, player, owner, aggression, opinion, unit_owner
                );
            }
            let score = match crate::env_var!("CAP_SCALE").as_deref() {
                Ok("006") => aggression * 0.06,
                Ok("0005") => aggression * 0.005,
                _ => {
                    if unit_owner.is_none() || unit_owner == Some(player) {
                        aggression * 0.06
                    } else {
                        aggression * 0.005
                    }
                }
            };
            self.ai_mut(player).missions.push(Mission { kind: MISSION_CAPTURE, tile: at, score });
        }
    }

    /// Score an enemy unit as a target for the hit list.
    fn update_hitlist_for_unit(&mut self, player: u8, at: Wc) {
        let Some(ui) = self.unit_index_at(at) else { return };
        let unit = self.s.units[ui].clone();
        let Some(tile) = self.tile(at).copied() else { return };
        let Some(info) = self.unit_info(&unit) else { return };
        let (range0, _) = (info.data.range, ());
        let base = if tile.improvement == improvement::CITY {
            if tile.owner == player {
                100.0
            } else if tile.owner == 0 {
                20.0
            } else {
                15.0
            }
        } else if tile.improvement == improvement::RUIN {
            20.0
        } else {
            0.0
        };
        let mut range = range0;
        if self.has_ability(&unit, unit_ability::DASH) {
            range += self.unit_movement(&unit);
        }
        let targets = self.attack_targets_with_range(&unit, range, false);
        let mut best = 0.0f32;
        for t in &targets {
            if let Some(tt) = self.tile(*t) {
                if self.unit_at(tt.coordinates).is_some_and(|u| u.owner == player) {
                    let rate = self.rate_battle(&unit, tt.coordinates);
                    if rate > best {
                        best = rate;
                    }
                }
            }
        }
        let mut score = base + best / 5.0;
        let moves = self.movement_options_ignoring_state(&unit);
        for m in moves {
            if let Some(mt) = self.tile(m) {
                if mt.improvement == improvement::CITY {
                    if mt.owner == player {
                        score += 30.0;
                    } else if mt.owner == 0 {
                        score += 10.0;
                    }
                }
            }
        }
        if score > 0.0 {
            self.ai_mut(player).hit_list.push(UnitTarget { unit: unit.id, score });
        }
    }
}

fn mission_noise_base_for_override(j: i32) -> i32 {
    crate::ai::decide::mission_noise_y(j)
}
