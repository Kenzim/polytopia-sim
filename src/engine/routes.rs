//! Trade routes and city connections.
//!
//! The route update resets every tile's route flag and
//! then, per player, joins each owned "router" improvement (port, outpost, atoll, ...) to every explored router
//! within its range by a path over the shared route terrain; each path tile gets the route flag.
//! The city-connection update then flood-fills from the
//! capital over connectable tiles and connects/disconnects cities (a connected city gives the capital and the
//! city one population each).

use super::*;
use crate::rules::path::{self, PathSettings};

impl Engine {
    fn router_range(&self, kind: i32) -> Option<(i32, TerrainList)> {
        let info = self.t.improvement(kind)?;
        if info.data.routes.is_empty() {
            return None;
        }
        let terrains: TerrainList = info.route_terrain.iter().copied().collect();
        Some((info.data.range.unwrap_or(0), terrains))
    }

    /// Path between two routers: true when a path was found (its tiles are flagged). A terrain is allowed
    /// when the origin routes over it and either the target routes over it too (and it is unlocked) or the target is a city.
    /// `unlocked` holds the player's unlocked movement terrain once a call has needed it (the caller keeps it per player).
    fn route_between(&mut self, player: u8, a: Wc, b: Wc, unlocked: &mut Option<TerrainList>) -> bool {
        let (Some(ta), Some(tb)) = (self.tile(a), self.tile(b)) else { return false };
        let target_city = tb.improvement == improvement::CITY;
        // Cheap rejections first, in an order that gives the same answer as building both terrain lists.
        let Some(info_a) = self.t.improvement(ta.improvement).filter(|i| !i.data.routes.is_empty()) else {
            return false;
        };
        let b_routes = self.t.improvement(tb.improvement).is_some_and(|i| !i.data.routes.is_empty());
        if !b_routes && !target_city {
            return false;
        }
        if info_a.data.range.unwrap_or(0) < Self::chebyshev(a, b) {
            return false;
        }
        let Some((range, from_terrain)) = self.router_range(ta.improvement) else { return false };
        let to_terrain = match self.router_range(tb.improvement) {
            Some((_, t)) => t,
            None if target_city => TerrainList::new(),
            None => return false,
        };
        if range < Self::chebyshev(a, b) {
            return false;
        }
        let unlocked = unlocked.get_or_insert_with(|| self.movement_terrain_list(player));
        let allowed: TerrainList = from_terrain
            .iter()
            .copied()
            .filter(|t| (to_terrain.contains(t) && unlocked.contains(t)) || target_city)
            .collect();
        if allowed.is_empty() {
            return false;
        }
        let abilities: Vec<i32> = Vec::new();
        let settings = PathSettings {
            version: self.s.version,
            player_id: player,
            allowed_terrain: &allowed,
            abilities: &abilities,
            mover: None,
            require_port: true,
            allow_occupied: true,
            allow_enemy: false,
            allow_allies: self.s.version > 0x3b,
            can_player_embark: self.can_player_embark(player),
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
        let found = path::get_path(
            &self.s,
            a,
            b,
            range,
            &settings,
            |i| self.is_wetland_index(i),
            |kind, ability| self.t.has_improvement_ability(kind, ability),
            |_| 8,
            cost,
        );
        let Some(found) = found else { return false };
        if found.is_empty() {
            return false;
        }
        for c in found {
            if let Some(t) = self.tile_mut(c) {
                t.has_route = true;
            }
        }
        true
    }

    /// Route update: per player, every pair of owned routers (map order, i < j),
    /// then each owned Network router to each of the player's cities.
    ///
    /// The result (which tiles get the route flag) is a function of `Engine::route_inputs` alone, so it is
    /// memoised per thread: search paths that reach the same route inputs share one computation.
    pub fn update_routes(&mut self) {
        let old = crate::env_var!("OLD_ROUTES").is_ok();
        if !old && !self.any_router_pair() {
            for t in self.s.map.tiles.iter_mut() {
                t.has_route = false;
            }
            return;
        }
        let use_cache = !old && ROUTE_CACHE.with(|c| c.borrow_mut().worth_trying());
        let inputs = if use_cache { self.route_inputs() } else { None };
        if let Some(inputs) = &inputs {
            let hit = ROUTE_CACHE.with(|c| {
                let mut c = c.borrow_mut();
                let bits = c.get(&self.t, inputs).map(|bits| bits.to_vec());
                c.record(bits.is_some());
                bits
            });
            if let Some(bits) = hit {
                for (i, t) in self.s.map.tiles.iter_mut().enumerate() {
                    t.has_route = bits[i / 64] >> (i % 64) & 1 != 0;
                }
                #[cfg(feature = "route_check")]
                {
                    let cached: Vec<bool> = self.s.map.tiles.iter().map(|t| t.has_route).collect();
                    self.update_routes_uncached(false);
                    assert!(self.s.map.tiles.iter().map(|t| t.has_route).eq(cached), "stale route cache entry");
                }
                return;
            }
        }
        self.update_routes_uncached(old);
        if let Some(inputs) = inputs {
            let mut bits = vec![0u64; self.s.map.tiles.len().div_ceil(64)];
            for (i, t) in self.s.map.tiles.iter().enumerate() {
                if t.has_route {
                    bits[i / 64] |= 1 << (i % 64);
                }
            }
            ROUTE_CACHE.with(|c| c.borrow_mut().insert(&self.t, inputs, bits));
        }
    }

    /// Whether `update_routes_uncached` could join anything: some owner has two routers, or a Network router (which
    /// joins to cities). Without one every route flag ends up cleared.
    fn any_router_pair(&self) -> bool {
        let mut count = [0u8; 256];
        for t in &self.s.map.tiles {
            if t.improvement == 0 || t.owner == NATURE || !self.is_route_opener(t.improvement) {
                continue;
            }
            let c = &mut count[usize::from(t.owner)];
            *c += 1;
            if *c >= 2 || self.t.has_improvement_ability(t.improvement, improvement_ability::NETWORK) {
                return true;
            }
        }
        false
    }

    /// Everything `update_routes` reads, packed into words; `None` when the state does not fit the packing (the
    /// caller then computes without the cache). Per tile: owner, terrain, the improvement when it matters to a route
    /// (see [`Engine::route_relevant_improvement`]), road, wetland, and which players have explored it. Then the
    /// version, the players, each player's unlocked movement terrain and every diplomacy relation state. Units are
    /// not read: route paths allow occupied tiles and have no mover.
    fn route_inputs(&self) -> Option<RouteInputs> {
        let s = &self.s;
        let routers: Vec<u8> = s.players.iter().map(|p| p.id).filter(|id| *id != NATURE).collect();
        if routers.len() > 30 {
            return None;
        }
        // Every variable-length section is preceded by its length, so equal word lists mean equal inputs.
        let width = i32::from(s.map.width);
        let mut words: Vec<u64> = Vec::with_capacity(s.map.tiles.len() + 64);
        words.push(s.version as u32 as u64 | u64::from(s.map.width) << 32 | u64::from(s.map.height) << 48);
        words.push(s.map.tiles.len() as u64);
        for (i, t) in s.map.tiles.iter().enumerate() {
            let i = i as i32;
            if t.index != i || t.coordinates != (Wc { x: i % width, y: i / width }) {
                return None;
            }
            let improvement = if self.route_relevant_improvement(t.improvement) { t.improvement } else { 0 };
            if !(0..256).contains(&t.terrain) || !(0..65536).contains(&improvement) {
                return None;
            }
            let wetland = self.is_wetland_index(t.index);
            let mut explored = 0u64;
            for (j, &p) in routers.iter().enumerate() {
                if path::explored(s, t.index, p) {
                    explored |= 1 << j;
                }
            }
            words.push(
                u64::from(t.owner)
                    | (t.terrain as u64) << 8
                    | (improvement as u64) << 16
                    | u64::from(t.has_road) << 32
                    | u64::from(wetland) << 33
                    | explored << 34,
            );
        }
        words.push(s.players.len() as u64);
        words.extend(s.players.iter().map(|p| u64::from(p.id)));
        for &p in &routers {
            let terrain = self.movement_terrain_list(p);
            words.push(terrain.len() as u64);
            words.extend(terrain.iter().map(|&t| t as u32 as u64));
        }
        words.push(s.diplomacy.len() as u64);
        for row in &s.diplomacy {
            words.push(u64::from(row.player) | (row.relations.len() as u64) << 8);
            words.extend(
                row.relations.iter().map(|l| u64::from(l.other) | (l.relation.state as i32 as u32 as u64) << 8),
            );
        }
        Some(RouteInputs::new(words))
    }

    /// An improvement `update_routes` can tell apart from an empty tile: a City, Bridge or Port (compared by id), one
    /// with the Bridge or Embark ability, or a router. Every other one only ever meets `improvement != 0` checks that
    /// are followed by one of these tests.
    fn route_relevant_improvement(&self, kind: i32) -> bool {
        kind == improvement::CITY
            || kind == improvement::BRIDGE
            || kind == improvement::PORT
            || self.t.has_improvement_ability(kind, improvement_ability::BRIDGE)
            || self.t.has_improvement_ability(kind, improvement_ability::EMBARK)
            || self.is_route_opener(kind)
    }

    fn update_routes_uncached(&mut self, old: bool) {
        for t in self.s.map.tiles.iter_mut() {
            t.has_route = false;
        }
        let players: Vec<u8> = self.s.players.iter().map(|p| p.id).filter(|id| *id != NATURE).collect();
        if old {
            for p in players {
                let owned: Vec<Wc> = self
                    .s
                    .map
                    .tiles
                    .iter()
                    .filter(|t| t.owner == p && t.improvement != 0 && self.is_route_opener(t.improvement))
                    .map(|t| t.coordinates)
                    .collect();
                let explored: Vec<Wc> = self
                    .s
                    .map
                    .tiles
                    .iter()
                    .filter(|t| {
                        t.improvement != 0
                            && self.is_route_opener(t.improvement)
                            && self.is_explored_at(p, t.coordinates)
                    })
                    .map(|t| t.coordinates)
                    .collect();
                let mut unlocked = None;
                for a in &owned {
                    for b in &explored {
                        if a != b {
                            self.route_between(p, *a, *b, &mut unlocked);
                        }
                    }
                }
            }
            return;
        }
        for p in players {
            let routers: Vec<Wc> = self
                .s
                .map
                .tiles
                .iter()
                .filter(|t| t.owner == p && t.improvement != 0 && self.is_route_opener(t.improvement))
                .map(|t| t.coordinates)
                .collect();
            let cities = self.city_tiles(p);
            let mut unlocked = None;
            for (i, a) in routers.iter().enumerate() {
                for b in &routers[i + 1..] {
                    self.route_between(p, *a, *b, &mut unlocked);
                }
                let network = self
                    .tile(*a)
                    .is_some_and(|t| self.t.has_improvement_ability(t.improvement, improvement_ability::NETWORK));
                if network {
                    for c in &cities {
                        self.route_between(p, *a, *c, &mut unlocked);
                    }
                }
            }
        }
    }

    /// Cities reachable from the capital, capital excluded.
    pub fn find_connected_cities(&self, player: u8) -> Vec<Wc> {
        let Some(start) = self.player(player).map(|p| p.start_tile) else { return Vec::new() };
        if self.tile(start).map(|t| t.owner) != Some(player) {
            return Vec::new();
        }
        // Two neighbouring tiles are linked when both are connectable (road, city or router) or both carry a route,
        // unless the second belongs to a player the first one's owner is not at peace with. Every coordinate here is
        // on the map: the start tile is, and links are only followed onto map tiles.
        let tiles = &self.s.map.tiles;
        let mut seen = vec![false; tiles.len()];
        let index = |c: Wc| self.tile_index(c).expect("on the map");
        let connectable = |t: &Tile| {
            t.has_road
                || t.improvement == improvement::CITY
                || (t.improvement != 0 && self.is_route_opener(t.improvement))
        };
        let mut stack = vec![start];
        let mut out = Vec::new();
        while let Some(cur) = stack.pop() {
            let ci = index(cur);
            if std::mem::replace(&mut seen[ci], true) {
                continue;
            }
            let ta = &tiles[ci];
            if cur != start && ta.improvement == improvement::CITY {
                out.push(cur);
            }
            let a_conn = connectable(ta);
            for n in self.area_iter(cur, 1) {
                let ni = index(n);
                if seen[ni] {
                    continue;
                }
                let tb = &tiles[ni];
                let joined = (a_conn && connectable(tb)) || (ta.has_route && tb.has_route);
                // Pure reads, so the relation lookups run only for a joined neighbour.
                let hostile = || {
                    tb.owner != 0
                        && tb.owner != ta.owner
                        && self.player(ta.owner).is_some()
                        && !self.at_peace(ta.owner, tb.owner)
                };
                if joined && !hostile() {
                    stack.push(n);
                }
            }
        }
        out
    }

    /// City-connection update, including the connected-city changes it causes.
    pub fn update_city_connections(&mut self, player: u8) {
        let cities: Vec<Wc> = self.city_tiles(player);
        // The capital is never in the connected list, so with no other city the search cannot change anything.
        let start = self.player(player).map(|p| p.start_tile);
        let connected =
            if cities.iter().any(|c| Some(*c) != start) { self.find_connected_cities(player) } else { Vec::new() };
        if crate::env_var!("ROUTE_DEBUG").is_ok() {
            eprintln!(
                "CONN p{} cmd {} cities {:?} connected {:?} routes {:?}",
                player,
                self.s.current_command,
                cities,
                connected,
                self.s
                    .map
                    .tiles
                    .iter()
                    .filter(|t| t.has_route)
                    .map(|t| (t.coordinates.x, t.coordinates.y))
                    .collect::<Vec<_>>()
            );
        }
        let Some(capital) = self.player(player).map(|p| p.start_tile) else { return };
        // Retail queues one action per city, in city order, on the game's action stack, which runs last in, first out:
        // the last city's change runs first. The order matters when a population change raises a city reward, which
        // holds the changes behind it.
        let mut ops: Vec<(Wc, bool, u8)> = Vec::new();
        for city in cities {
            let current = self.improvement_rt(city).map(|r| r.connected_to_capital_of_player).unwrap_or(0);
            if connected.contains(&city) {
                if current != player {
                    ops.push((city, true, current));
                }
            } else if current != 0 {
                ops.push((city, false, current));
            }
        }
        for (city, connect, current) in ops.into_iter().rev() {
            if connect {
                if current == 0 {
                    // The connect action enables the Network task before it queues the population changes.
                    self.enable_task(player, crate::TaskKind::Network);
                    // Retail pushes the capital's population change, then the city's, onto its action stack, which runs
                    // last in, first out: the city's point comes first, and its level-up holds the capital's.
                    self.add_population(city, 1);
                    self.add_population(capital, 1);
                } else {
                    self.add_population(capital, 1);
                    if let Some(old) = self.player(current).map(|p| p.start_tile) {
                        self.add_population(old, -1);
                    }
                }
                if let Some(rt) = self.improvement_rt_mut(city) {
                    rt.connected_to_capital_of_player = player;
                }
                self.check_network(player);
            } else {
                // A captured city only loses its own point; the old capital keeps its bonus (replay-verified).
                // On a disconnect the capital it was connected to and the city each lose one.
                if current == player || crate::env_var!("OLD_DISCONNECT").is_err() {
                    if let Some(old) = self.player(current).map(|p| p.start_tile) {
                        self.add_population(old, -1);
                    }
                }
                self.add_population(city, -1);
                if let Some(rt) = self.improvement_rt_mut(city) {
                    rt.connected_to_capital_of_player = 0;
                }
            }
        }
    }

    /// The route update followed by the city-connection update for `player`.
    pub fn refresh_routes_and_connections(&mut self, player: u8) {
        self.update_routes();
        self.update_city_connections(player);
    }
}

/// The packed inputs of one `update_routes` call (see `Engine::route_inputs`) and their hash.
struct RouteInputs {
    hash: u64,
    words: Box<[u64]>,
}

impl RouteInputs {
    fn new(words: Vec<u64>) -> Self {
        let mut hash = words.len() as u64;
        for &w in &words {
            hash = (hash.rotate_left(5) ^ w).wrapping_mul(0x517c_c1b7_2722_0a95);
        }
        Self { hash, words: words.into_boxed_slice() }
    }
}

/// A two-way set-associative memo of `update_routes` results (route flags as a tile bitset), keyed by the full
/// inputs, for the engines of one `Tables`. Hits compare every input word, so a hit is exact. 1024 slots hold about
/// 2 MB on a 14x14 map; in the weekly search most repeats come within a few hundred calls.
struct RouteCache {
    tables: std::sync::Weak<Tables>,
    slots: Vec<Option<(RouteInputs, Box<[u64]>)>>,
    /// Lookups and hits in the current window of `ROUTE_CACHE_WINDOW` lookups.
    lookups: u32,
    hits: u32,
    /// Updates left to run without the cache, after a window whose hit rate was too low to pay for the keys.
    bypass: u32,
}

const ROUTE_CACHE_SLOTS: usize = 1024;
const ROUTE_CACHE_WINDOW: u32 = 64;
const ROUTE_CACHE_BYPASS: u32 = 7 * ROUTE_CACHE_WINDOW;

thread_local! {
    static ROUTE_CACHE: std::cell::RefCell<RouteCache> = const {
        std::cell::RefCell::new(RouteCache { tables: std::sync::Weak::new(), slots: Vec::new(), lookups: 0, hits: 0, bypass: 0 })
    };
}

impl RouteCache {
    /// Whether this update should build its key and look it up. Building the key costs about as much as a
    /// small route update, so when fewer than one lookup in eight hits (random play, where positions rarely
    /// repeat), the next `ROUTE_CACHE_BYPASS` updates skip the cache. Either way the route flags are the same.
    fn worth_trying(&mut self) -> bool {
        if self.bypass > 0 {
            self.bypass -= 1;
            return false;
        }
        true
    }

    fn record(&mut self, hit: bool) {
        self.lookups += 1;
        self.hits += u32::from(hit);
        if self.lookups >= ROUTE_CACHE_WINDOW {
            if self.hits * 8 < self.lookups {
                self.bypass = ROUTE_CACHE_BYPASS;
            }
            self.lookups = 0;
            self.hits = 0;
        }
    }

    fn ways(hash: u64) -> [usize; 2] {
        [hash as usize % ROUTE_CACHE_SLOTS, (hash >> 32) as usize % ROUTE_CACHE_SLOTS]
    }

    fn get(&mut self, tables: &Arc<Tables>, inputs: &RouteInputs) -> Option<&[u64]> {
        // The `Weak` keeps the allocation, so a live pointer match is the same tables.
        if !std::ptr::eq(self.tables.as_ptr(), Arc::as_ptr(tables)) {
            self.tables = Arc::downgrade(tables);
            self.slots.clear();
            return None;
        }
        let way = Self::ways(inputs.hash).into_iter().find(|&w| {
            self.slots
                .get(w)
                .and_then(Option::as_ref)
                .is_some_and(|(k, _)| k.hash == inputs.hash && k.words == inputs.words)
        })?;
        self.slots[way].as_ref().map(|(_, bits)| &bits[..])
    }

    /// Called after a `get` miss for the same tables. The new entry takes its first way; what was there moves to its
    /// own other way, evicting the entry there.
    fn insert(&mut self, tables: &Arc<Tables>, inputs: RouteInputs, bits: Vec<u64>) {
        if !std::ptr::eq(self.tables.as_ptr(), Arc::as_ptr(tables)) {
            return;
        }
        if self.slots.is_empty() {
            self.slots.resize_with(ROUTE_CACHE_SLOTS, || None);
        }
        let a = Self::ways(inputs.hash)[0];
        let old = self.slots[a].replace((inputs, bits.into_boxed_slice()));
        if let Some(old) = old {
            let alt = Self::ways(old.0.hash).into_iter().find(|&w| w != a);
            if let Some(alt) = alt {
                self.slots[alt] = Some(old);
            }
        }
    }
}
