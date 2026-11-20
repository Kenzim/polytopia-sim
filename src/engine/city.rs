//! Cities, borders, population, levels and improvements.

use super::*;
use crate::hash::HashFunction;
use crate::rules::economy::{self, WorkInput};
use crate::CityReward;

impl Engine {
    /// Tiles whose ruling city is `city`.
    pub fn city_area(&self, city: Wc) -> Vec<Wc> {
        self.s.map.tiles.iter().filter(|t| t.ruling_city == city).map(|t| t.coordinates).collect()
    }

    /// Create the runtime object for a new city at `at`.
    pub fn new_city_runtime(&mut self, at: Wc, owner: u8) {
        let Some(tile) = self.tile(at) else { return };
        let idx = tile.index;
        let turn = self.s.current_turn as u16;
        self.s.improvements.retain(|r| r.tile != idx);
        self.s.improvements.push(ImprovementRuntime {
            tile: idx,
            owner,
            founder: owner,
            level: 1,
            founded: turn,
            xp: 0,
            population: 0,
            production: 1,
            base_score: 0,
            border_size: 1,
            upgrade: 0,
            ..ImprovementRuntime::default()
        });
    }

    /// Claim the unowned tiles within the border radius.
    pub fn rule_area(&mut self, city: Wc, player: u8) {
        self.rule_area_q(city, player, false)
    }

    pub fn rule_area_q(&mut self, city: Wc, player: u8, quiet: bool) {
        let radius = self.improvement_rt(city).map(|r| i32::from(r.border_size)).unwrap_or(1);
        let mut cells = self.area(city, radius);
        cells.push(city);
        for c in cells {
            let Some(i) = self.tile_index(c) else { continue };
            let t = &mut self.s.map.tiles[i];
            if t.owner == 0 || c == city {
                let old = t.owner;
                let holds_city = t.improvement == improvement::CITY;
                t.owner = player;
                t.ruling_city = city;
                // A neutral improvement record on a tile taken into the border (a ruin, a village, a corner lighthouse)
                // takes the owner of the tile.
                if old == 0 && c != city {
                    let tile = t.index;
                    if let Some(rt) = self.s.improvements.iter_mut().find(|r| r.tile == tile) {
                        rt.owner = player;
                    }
                }
                // A city or village tile changing hands moves the owners' city counters, as a capture does.
                if holds_city && old != player {
                    if let Some(rt) = self.s.player_runtime.iter_mut().find(|r| r.player == player) {
                        rt.cities += 1;
                    }
                    if let Some(rt) = self.s.player_runtime.iter_mut().find(|r| r.player == old) {
                        rt.cities -= 1;
                    }
                }
            }
        }
        let sight = if quiet && crate::env_var!("CAP_SIGHT2").is_err() {
            radius
        } else {
            radius + crate::env_var!("RULE_DELTA").ok().and_then(|v| v.parse::<i32>().ok()).unwrap_or(0)
        };
        let mut seen = self.area(city, sight);
        seen.push(city);
        let quiet = quiet && crate::env_var!("CAPTURE_MEET").is_err();
        for c in seen {
            if let Some(i) = self.tile_index(c) {
                if quiet {
                    if !self.is_explored(player, i) {
                        if let Some(f) = self.s.fog.get_mut(i) {
                            f.explorers.push(player);
                        }
                        self.reveal_corner(player, i);
                    }
                } else {
                    self.explore_tile(player, i);
                }
            }
        }
    }

    /// Population for the ruling city of `tile`. Retail queues one single-point action per population and checks the
    /// level-up after each, so a gain that spans two levels raises the first reward, waits for the answer, then the
    /// second, and a reward raised part-way holds the points still to come.
    pub fn add_population(&mut self, city: Wc, amount: i32) {
        // An earlier action of this command raised a city reward: the queue stops there, and this change runs
        // once the reward is answered.
        if self.reward_pending() {
            self.hold(Deferred::Population(city, amount));
            return;
        }
        if amount > 1 {
            for done in 0..amount {
                let below = self.deferred.len();
                self.add_population(city, 1);
                if self.reward_pending() && done + 1 < amount {
                    // The points still to come sit under the score and reward actions this point raised.
                    self.hold_at.get_or_insert(below);
                    self.hold(Deferred::Population(city, amount - done - 1));
                    return;
                }
            }
            return;
        }
        let mut level_up = false;
        let current = self.current_player();
        if crate::env_var!("POP_LOG").is_ok() {
            eprintln!(
                "ENGPOP cmd {} p{} city ({},{}) amount {}",
                self.s.current_command, current, city.x, city.y, amount
            );
            if crate::env_var!("POP_BT").is_ok() {
                eprintln!("{}", std::backtrace::Backtrace::force_capture());
            }
        }
        let cap_bonus: i16 =
            if crate::env_var!("CAP_BONUS").is_ok() && self.s.players.iter().any(|p| p.start_tile == city) {
                1
            } else {
                0
            };
        if let Some(rt) = self.improvement_rt_mut(city) {
            rt.xp = rt.xp.wrapping_add(amount as i16);
            rt.population = rt.population.wrapping_add(amount as i16);
            // Adding population queues a level-up as soon as the city has enough xp.
            level_up = crate::rules::economy::should_level_up(rt.xp + cap_bonus, rt.level) && rt.owner == current;
        }
        if level_up && self.is_city(city) {
            // The population's own score increase sits under the level-up, so it also waits for the reward
            // (seen on retail's action stack in a live game).
            let next = self.improvement_rt(city).map(|r| r.level.wrapping_add(1)).unwrap_or(0);
            if !self.reward_options(next).is_empty() {
                let owner = self.tile(city).map(|t| t.owner).unwrap_or(0);
                self.deferred.push(Deferred::Score(owner, crate::rules::economy::CITY_XP_SCORE * amount));
            }
            self.level_up_city(city);
        }
    }

    /// Adjacency level of a non-city improvement: matching neighbours.
    pub fn adjacency_bonus(&self, at: Wc, kind: i32) -> i32 {
        let Some(info) = self.t.improvement(kind) else { return 0 };
        let owner = self.tile(at).map(|t| t.owner).unwrap_or(0);
        // Picky counts each type once, Simple counts 1 per
        // neighbour, otherwise each neighbour adds its stored level (at least 1) - a market sums its sawmills etc.
        let picky = info.abilities.contains(&improvement_ability::PICKY);
        let simple = info.abilities.contains(&improvement_ability::SIMPLE);
        let old = crate::env_var!("OLD_ADJ").is_ok();
        let mut seen: Vec<i32> = Vec::new();
        let mut count = 0;
        let mut add = |ty: i32, level: i32| {
            if picky {
                if !seen.contains(&ty) {
                    seen.push(ty);
                    count += 1;
                }
            } else {
                count += if simple || level < 2 || old { 1 } else { level };
            }
        };
        for n in self.neighbors(at) {
            let Some(nt) = self.tile(n) else { continue };
            // Only tiles of the same owner count, resources included.
            if nt.owner != owner {
                continue;
            }
            for adj in &info.data.adjacency_improvements {
                if let Some(name) = &adj.improvement {
                    if let Some(&idx) = self.t.improvement_idx.get(name) {
                        if nt.improvement == idx {
                            let level = self.improvement_rt(n).map(|r| i32::from(r.level)).unwrap_or(0);
                            add(idx, level);
                        }
                    }
                }
                // A resource counts only on a tile without an improvement.
                if let Some(res) = &adj.resources {
                    if let Some(&idx) = self.t.resource_idx.get(res) {
                        if nt.resource == idx && nt.improvement == 0 {
                            add(idx, 1);
                        }
                    }
                }
            }
        }
        count
    }

    /// Improvement level for the improvement on `at`.
    pub fn improvement_level(&self, at: Wc) -> i32 {
        let Some(tile) = self.tile(at) else { return 1 };
        let Some(info) = self.t.improvement(tile.improvement) else { return 1 };
        let Some(rt) = self.improvement_rt(at) else { return 1 };
        if tile.improvement == improvement::CITY {
            return i32::from(rt.level);
        }
        let mut level = i32::from(rt.level);
        if info.abilities.contains(&improvement_ability::PATINA) {
            let rate = info.data.growth_rate.unwrap_or(1).max(1);
            level = economy::improvement_age(self.s.current_turn, rt.founded) / rate;
        }
        if !info.data.adjacency_improvements.is_empty() {
            level = self.adjacency_bonus(at, tile.improvement);
        }
        let max = self.improvement_max_level(at);
        if max > 0 {
            level = level.min(max);
        }
        level
    }

    /// Maximum level of the improvement with the tile owner, as `improvement_level` passes it:
    /// Fungi grows one level further when the owner has Microbes.
    pub fn improvement_max_level(&self, at: Wc) -> i32 {
        let Some(tile) = self.tile(at) else { return 0 };
        let Some(info) = self.t.improvement(tile.improvement) else { return 0 };
        // Only Fungi reads the owner's Microbes.
        let microbes = tile.improvement == economy::FUNGI_IMPROVEMENT
            && self.has_player_ability(tile.owner, player_ability::MICROBES);
        economy::max_improvement_level(tile.improvement, info.data.max_level.unwrap_or(0), microbes)
    }

    /// Work of one tile owned by its player.
    pub fn tile_work(&self, at: Wc) -> i32 {
        let Some(tile) = self.tile(at) else { return 0 };
        if tile.improvement == 0 {
            return 0;
        }
        let Some(info) = self.t.improvement(tile.improvement) else { return 0 };
        let Some(rt) = self.improvement_rt(at) else { return 0 };
        let is_city = tile.improvement == improvement::CITY;
        let owner = tile.owner;
        let doubled = self.is_doubled(at, tile.improvement);
        let occupied_by_other = self.unit_at(at).is_some_and(|u| u.owner != owner && u.owner != NATURE);
        let handicap = self.player(owner).map(|p| p.handicap).unwrap_or(0);
        let is_start = self.player(owner).is_some_and(|p| p.start_tile == at);
        let input = WorkInput {
            is_city,
            work: info.data.work.unwrap_or(0),
            level: if is_city { i32::from(rt.level) } else { self.improvement_level(at) },
            doubled,
            production: i32::from(rt.production),
            xp: rt.xp,
            owner,
            occupied_by_other_unit: occupied_by_other,
            version: self.s.version,
            robbed: rt.robbed,
            is_start_tile: is_start,
            handicap,
            embassy_income: if is_start { self.embassy_income_for(owner) } else { 0 },
        };
        let mut w = economy::calculate_work(&input);
        // Empirical (replay of a retail game): every city earns one star more than `production + level - 1`
        // and the capital one more again (lower bounds from the recorded purchases of all seven players).
        if is_city && owner != 0 && !occupied_by_other && !rt.robbed {
            let city_fudge: i32 = crate::env_var!("WORK_CITY").ok().and_then(|v| v.parse().ok()).unwrap_or(0);
            let cap_fudge: i32 = crate::env_var!("WORK_CAP").ok().and_then(|v| v.parse().ok()).unwrap_or(0);
            w += city_fudge;
            if is_start {
                w += cap_fudge;
            }
        }
        w
    }

    fn is_doubled(&self, at: Wc, kind: i32) -> bool {
        if !self.t.has_improvement_ability(kind, improvement_ability::DOUBLED) {
            return false;
        }
        let owner = self.tile(at).map(|t| t.owner).unwrap_or(0);
        self.neighbors(at).iter().any(|n| {
            self.tile(*n).is_some_and(|t| {
                t.improvement == improvement::PORT
                    && economy::income_is_doubled(self.s.version, true, true, t.owner == owner)
            })
        })
    }

    /// Stars the embassies pay `player` each turn, summed.
    pub fn embassy_income_for(&self, player: u8) -> i32 {
        let Some(d) = self.t.logic.diplomacy.as_ref() else { return 0 };
        let mut sum = 0;
        for other in &self.s.players {
            if other.id == player || other.id == NATURE {
                continue;
            }
            // The embassy income reads the relation `player` holds toward `other`: the host
            // of an embassy built by `other` earns from it, the builder does not (it earns on the host's capital).
            let rel = crate::rules::diplomacy::get_relation(&self.s, player, other.id);
            let war = crate::rules::diplomacy::has_war_with(&self.s, other.id, player);
            sum += economy::embassy_income(other.id, rel.state as i32, rel.embassy_level, war, d.embassy_income);
        }
        sum
    }

    pub fn income(&self, player: u8) -> i32 {
        let mut sum = 0;
        for t in &self.s.map.tiles {
            if t.owner == player && t.improvement != 0 {
                sum += self.tile_work(t.coordinates);
            }
        }
        sum
    }

    /// Number of units whose home is `city` (for the unit-support check).
    pub fn city_unit_count(&self, city: Wc) -> i32 {
        self.s.units.iter().filter(|u| u.home == city).count() as i32
    }

    pub fn can_city_support_unit(&self, at: Wc) -> bool {
        let Some(tile) = self.tile(at) else { return false };
        let Some(city) = self.tile(tile.ruling_city) else { return false };
        if city.improvement != improvement::CITY {
            return false;
        }
        let level = self.improvement_rt(city.coordinates).map(|r| i32::from(r.level)).unwrap_or(0);
        self.city_unit_count(city.coordinates) <= level
    }

    // ---------- levels and rewards ----------

    /// Reward choices offered when a city reaches `level`, in the order of retail's static table;
    /// the AI picks by index, so the order matters.
    pub fn reward_options(&self, level: u16) -> Vec<CityReward> {
        match level {
            0 | 1 => Vec::new(),
            2 => vec![CityReward::Workshop, CityReward::Explorer],
            3 => vec![CityReward::CityWall, CityReward::Resources],
            4 => vec![CityReward::PopulationGrowth, CityReward::BorderGrowth],
            _ => vec![CityReward::Park, CityReward::SuperUnit],
        }
    }

    /// Level a city up, plus the score and the pending reward trigger.
    pub fn level_up_city(&mut self, city: Wc) {
        let owner = self.tile(city).map(|t| t.owner).unwrap_or(0);
        let is_cap = self.s.players.iter().any(|p| p.start_tile == city);
        let Some(rt) = self.improvement_rt_mut(city) else { return };
        let level = rt.level.wrapping_add(1);
        rt.level = level;
        let sub = if crate::env_var!("XP_OLD").is_ok() || (is_cap && crate::env_var!("CAP_SUB").is_ok()) {
            level.wrapping_sub(1)
        } else {
            level
        };
        rt.xp = rt.xp.wrapping_sub(sub as i16);
        rt.upgrade = rt.upgrade.wrapping_sub(1);
        self.tasks_after_level_up(owner, level);
        if !self.reward_options(level).is_empty() {
            if crate::env_var!("TRIG_DEBUG").is_ok() {
                eprintln!(
                    "TRIG cmd {} turn {} city ({},{}) owner {} level {}",
                    self.s.current_command, self.s.current_turn, city.x, city.y, owner, level
                );
            }
            // Levelling a city up queues a score increase of the level-up score,
            // which is `CITY_LEVEL_SCORE - level * CITY_XP_SCORE` and waits under the reward trigger.
            let amount =
                crate::rules::economy::CITY_LEVEL_SCORE - i32::from(level) * crate::rules::economy::CITY_XP_SCORE;
            self.deferred.push(Deferred::Score(owner, amount));
            self.triggers.push(Trigger { player: owner, city, peace_from: None });
        }
    }

    /// Apply a city reward.
    pub fn apply_city_reward(&mut self, city: Wc, reward: CityReward) {
        let owner = self.tile(city).map(|t| t.owner).unwrap_or(0);
        if let Some(rt) = self.improvement_rt_mut(city) {
            rt.rewards.push(reward);
        }
        match reward {
            CityReward::Workshop => {
                if let Some(rt) = self.improvement_rt_mut(city) {
                    rt.production = rt.production.wrapping_add(1);
                }
            }
            CityReward::Park => {
                let v99 = self.s.version >= 99;
                if let Some(rt) = self.improvement_rt_mut(city) {
                    if v99 {
                        rt.production = rt.production.wrapping_add(1);
                    }
                    rt.base_score = rt.base_score.wrapping_add(250);
                }
            }
            CityReward::Resources => self.add_currency(owner, 5),
            CityReward::PopulationGrowth => self.add_population(city, 3),
            CityReward::BorderGrowth => {
                if let Some(rt) = self.improvement_rt_mut(city) {
                    rt.border_size = rt.border_size.wrapping_add(1);
                }
                self.rule_area(city, owner);
                // The city's tiles are brought into line with its owner, as after a capture: a spreading climate is
                // taken back, and a tribe with the spread ability freezes them.
                self.capture_climate(owner, city);
                // The new border can cut another player's route (a route may not cross foreign territory), so the
                // game re-runs the route pass and the city connections of every other player at once. The growing
                // player's own connections are left as they are until its next connection update.
                self.update_routes();
                let ids: Vec<u8> =
                    self.s.players.iter().map(|p| p.id).filter(|i| *i != NATURE && *i != owner).collect();
                for p in ids {
                    self.update_city_connections(p);
                }
                // A Polaris city that grows its border freezes the new area, its own tiles only and no units.
                if self.has_tribe_ability(owner, "spread") {
                    let radius = self.improvement_rt(city).map_or(1, |r| i32::from(r.border_size));
                    self.freeze_area(owner, city, radius, false, true);
                }
            }
            CityReward::Explorer => {
                // A temporary explorer (it takes a unit id but never stays on the board)
                // walks the scout's number of moves and reveals the tiles it passes.
                let _id = self.next_unit_id();
                let seed = self.random_hash().get_hash2(city.x, city.y);
                if crate::env_var!("OLD_SCOUT").is_ok() {
                    self.explorer_walk(owner, city, 15);
                } else {
                    self.run_scout(owner, city, seed);
                }
            }
            CityReward::SuperUnit => {
                if let Some(idx) = self.tribe_unit(owner, "giant") {
                    // The push of the occupant explores from its new tile; a city reward that raises stops the
                    // action stack, so the train waits under it.
                    self.push_occupant(owner, city);
                    if self.reward_pending() {
                        self.hold(Deferred::Train { owner, unit_type: idx, at: city, home: city, veteran: false });
                    } else {
                        self.spawn_unit(owner, idx, city, city, true);
                    }
                }
            }
            _ => {}
        }
    }

    /// Spawn a unit. If `at` is occupied it is placed on a free neighbour.
    pub fn spawn_unit(&mut self, owner: u8, type_idx: i32, at: Wc, home: Wc, exhausted: bool) -> Option<u32> {
        let mut place = at;
        if self.unit_at(at).is_some() {
            place = self.neighbors(at).into_iter().find(|n| {
                self.unit_at(*n).is_none() && {
                    let info = self.t.unit(type_idx);
                    let t = self.tile(*n);
                    match (info, t) {
                        (Some(info), Some(t)) => {
                            let water = crate::rules::path::is_water(t.terrain);
                            let aquatic = info.abilities.contains(&unit_ability::SWIM)
                                || info.abilities.contains(&unit_ability::FLY);
                            t.terrain != terrain::MOUNTAIN && (!water || aquatic)
                        }
                        _ => false,
                    }
                }
            })?;
        }
        let health = self.t.unit(type_idx)?.data.health as u16;
        let id = self.next_unit_id();
        let climate = self.new_unit_climate(type_idx, place);
        self.s.units.push(UnitState {
            id,
            owner,
            type_idx,
            coordinates: place,
            previous_turn_end: place,
            home,
            health,
            birth_climate: climate,
            created_turn: self.s.current_turn as u16,
            moved: exhausted,
            attacked: exhausted,
            // New units face the middle of the map.
            direction: Self::direction_from_vector(self.width() / 2 - place.x, self.height() / 2 - place.y),
            ..UnitState::default()
        });
        self.explore_unit(id);
        // Retail's train action puts the unit on the tile and then checks it for poison, as a move would.
        if let Some(unit) = self.unit_at(place).cloned() {
            self.check_step_on_poison(&unit, place);
        }
        if self.t.unit(type_idx).is_some_and(|i| i.abilities.contains(&unit_ability::TENTACLES)) {
            self.update_tentacles();
        }
        Some(id)
    }

    /// Climate of a new unit: only a Polytaur (type 15) takes the tile's climate; every other unit gets 0.
    pub(super) fn new_unit_climate(&self, type_idx: i32, at: Wc) -> i32 {
        if type_idx == 15 {
            self.tile(at).map(|t| t.climate).unwrap_or(0)
        } else {
            0
        }
    }

    // ---------- capture ----------

    /// City capture: the city and all tiles ruled by it change owner.
    pub fn capture_city_at(&mut self, player: u8, city: Wc) {
        let old_owner = self.tile(city).map(|t| t.owner).unwrap_or(0);
        // The capturer's city counter goes up; the previous owner's goes down when it is a player (a village has none).
        if let Some(rt) = self.s.player_runtime.iter_mut().find(|r| r.player == player) {
            rt.cities += 1;
        }
        if let Some(rt) = self.s.player_runtime.iter_mut().find(|r| r.player == old_owner) {
            rt.cities -= 1;
        }
        let had_rt = self.improvement_rt(city).is_some();
        if !had_rt {
            self.new_city_runtime(city, player);
        } else if let Some(rt) = self.improvement_rt_mut(city) {
            rt.owner = player;
        }
        // Tiles ruled by this city change hands; a village (no area yet) claims radius 1.
        let area = self.city_area(city);
        let had_area = !area.is_empty();
        for c in &area {
            if let Some(i) = self.tile_index(*c) {
                let t = &mut self.s.map.tiles[i];
                if t.owner != player {
                    t.owner = player;
                }
                t.ruling_city = city;
            }
        }
        if let Some(i) = self.tile_index(city) {
            self.s.map.tiles[i].owner = player;
            self.s.map.tiles[i].ruling_city = city;
            if old_owner != 0 {
                // A captured capital stops being a capital of its old owner.
            }
        }
        if !had_area {
            self.rule_area_q(city, player, true);
        } else {
            // Re-run the explore for the new owner.
            let radius = self
                .improvement_rt(city)
                .map(|r| {
                    i32::from(r.border_size)
                        + crate::env_var!("RA_DELTA").ok().and_then(|v| v.parse::<i32>().ok()).unwrap_or(0)
                })
                .unwrap_or(2);
            let mut seen = self.area(city, radius);
            seen.push(city);
            let quiet = crate::env_var!("CAPTURE_MEET").is_err();
            for c in seen {
                if let Some(i) = self.tile_index(c) {
                    if quiet {
                        if !self.is_explored(player, i) {
                            if let Some(f) = self.s.fog.get_mut(i) {
                                f.explorers.push(player);
                            }
                            self.reveal_corner(player, i);
                        }
                    } else {
                        self.explore_tile(player, i);
                    }
                }
            }
        }
        // Improvements on captured tiles change owner, and their population joins the city.
        let tiles: Vec<usize> = self.city_area(city).iter().filter_map(|c| self.tile_index(*c)).collect();
        for i in tiles {
            let idx = self.s.map.tiles[i].index;
            if let Some(rt) = self.s.improvements.iter_mut().find(|r| r.tile == idx) {
                rt.owner = player;
            }
        }
        // Adjacency buildings in and next to the captured area take the level their same-owner neighbours now give,
        // and their city gains or loses the growth reward (retail scratch run 2026-10-19: capturing Zebasi's capital
        // dropped a windmill on each side of the new border by one level, and each ruling city by one population).
        if crate::env_var!("OLD_CAPTURE_LEVELS").is_err() {
            for c in self.city_area(city) {
                let mut ups: Vec<Deferred> =
                    self.surrounding_level_ups(c).into_iter().map(Deferred::ImprovementLevelUp).collect();
                ups.extend(self.surrounding_level_downs(c).into_iter().map(Deferred::ImprovementLevelDown));
                let base = self.deferred.len();
                self.deferred.extend(ups);
                self.run_deferred_above(base);
            }
        }
        // Each captured improvement's population joins the captured city.
        if crate::env_var!("CAPTURE_POP").is_ok() {
            let tiles: Vec<Wc> = self.city_area(city);
            for c in tiles {
                let Some(t) = self.tile(c).copied() else { continue };
                if t.improvement == 0 || t.improvement == improvement::CITY {
                    continue;
                }
                let Some(info) = self.t.improvement(t.improvement) else { continue };
                let mut pop = info.population;
                if !info.data.adjacency_improvements.is_empty() {
                    let level = self.improvement_rt(c).map(|r| i32::from(r.level)).unwrap_or(0);
                    pop += info.data.growth_rewards.iter().map(|g| g.population.unwrap_or(0)).sum::<i32>() * level;
                }
                if pop > 0 {
                    self.add_population(city, pop);
                }
            }
        }
        self.capture_climate(player, city);
        // The captured city's tile floods for a Flooded tribe and drains for any other (a neutral village included).
        if self.has_tribe_ability(player, "flooded") {
            self.flood_tile(player, city);
        } else {
            self.drain_tile(city);
        }
        if old_owner != 0 && old_owner != player {
            self.note_attack(player, old_owner);
        }
        // Retail's capture enables and checks the capturer's Metropolis task when the city is above level 1, so a
        // captured level 5 city completes it.
        let level = self.improvement_rt(city).map_or(0, |r| r.level);
        if level >= 2 {
            self.tasks_after_level_up(player, level);
        }
    }
}

impl Engine {
    /// Approximation of the scout's next-target choice: each step moves to the accessible
    /// neighbour that reveals the most unexplored tiles (ties broken by the game hash).
    pub fn explorer_walk(&mut self, player: u8, from: Wc, steps: i32) {
        use crate::hash::HashFunction;
        let mut at = from;
        let mut seen: Vec<Wc> = vec![from];
        for _ in 0..steps {
            let mut best: Option<(i32, i32, Wc)> = None;
            for n in self.neighbors(at) {
                let Some(t) = self.tile(n).copied() else { continue };
                if !self.can_be_accessed_by_player(&t, player) || seen.contains(&n) {
                    continue;
                }
                let unexplored =
                    self.area(n, 1).into_iter().filter(|c| !self.is_explored_at(player, *c)).count() as i32;
                let tie = self.random_hash().get_hash3(n.x, n.y, steps) as i32 & 0xffff;
                if best.is_none_or(|b| (unexplored, tie) > (b.0, b.1)) {
                    best = Some((unexplored, tie, n));
                }
            }
            let Some((_, _, next)) = best else { break };
            at = next;
            seen.push(at);
            let sight = if self.tile(at).is_some_and(|t| t.terrain == terrain::MOUNTAIN) { 2 } else { 1 };
            self.explore_from(player, at, sight);
        }
    }
}
