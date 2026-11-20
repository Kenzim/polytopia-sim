//! Fog of war: which tiles each player has seen.

use super::*;
use crate::TileExplorers;

impl Engine {
    pub fn init_fog(&mut self) {
        self.s.fog = self
            .s
            .map
            .tiles
            .iter()
            .map(|t| TileExplorers { tile: t.index, explorers: crate::Explorers::new() })
            .collect();
    }

    #[allow(clippy::manual_contains)] // `iter().any` is deliberate here, see the note inside
    pub fn is_explored(&self, player: u8, tile_index: usize) -> bool {
        if player == NATURE {
            return true;
        }
        // Not `contains`: for `u8` it goes through memchr, slow on lists of a few ids.
        self.s.fog.get(tile_index).is_some_and(|f| f.explorers.iter().any(|&p| p == player))
    }

    pub fn is_explored_at(&self, player: u8, c: Wc) -> bool {
        self.tile_index(c).is_some_and(|i| self.is_explored(player, i))
    }

    /// Explore a tile: first sight scores `EXPLORE_VALUE`. Returns whether it was new.
    pub fn explore_tile(&mut self, player: u8, tile_index: usize) -> bool {
        if self.is_explored(player, tile_index) {
            return false;
        }
        if let Some(f) = self.s.fog.get_mut(tile_index) {
            f.explorers.push(player);
        }
        self.routes_flagged = true;
        if crate::env_var!("NO_MEET").is_err() {
            self.meet_on_explore(player, tile_index);
        }
        self.reveal_corner(player, tile_index);
        true
    }

    /// A newly explored map corner is a lighthouse discovery. A capture's quiet reveal also counts, even though
    /// it meets no players.
    pub(crate) fn reveal_corner(&mut self, player: u8, tile_index: usize) {
        if crate::env_var!("NO_LIGHTHOUSE").is_err() && player != NATURE {
            let c = self.s.map.tiles[tile_index].coordinates;
            let (w, h) = (self.width() - 1, self.height() - 1);
            if (c.x == 0 || c.x == w) && (c.y == 0 || c.y == h) {
                self.on_reveal_lighthouse(player);
                if crate::env_var!("NO_LH_LEVEL").is_err() {
                    self.level_up_lighthouse(c);
                }
            }
        }
    }

    pub fn knows_player(&self, player: u8, other: u8) -> bool {
        player == other
            || self
                .s
                .player_runtime
                .iter()
                .find(|r| r.player == player)
                .is_some_and(|r| r.known_players.contains(&other))
    }

    /// Every explored tile holding an unknown player's unit or city triggers a meeting.
    pub fn check_tribe_connections(&mut self, player: u8) {
        if player == NATURE || self.player(player).is_none() {
            return;
        }
        let known =
            self.s.player_runtime.iter().find(|r| r.player == player).map(|r| r.known_players.len()).unwrap_or(0)
                as i32;
        let count = self.s.players.iter().filter(|p| p.id != NATURE).count() as i32;
        if known >= count {
            return;
        }
        let tiles: Vec<usize> = if crate::env_var!("SEE_DEBUG").is_ok() {
            (0..self.s.map.tiles.len()).collect()
        } else {
            self.meeting_candidates(player)
        };
        self.meet_over_tiles(player, &tiles);
    }

    /// The tiles, in index order, on which [`Engine::meets_on_tile`] can queue a meeting for `player`: those whose
    /// coordinates hold a unit of a player it does not know, or a city (any tile with `MEET_TERRITORY`) owned by one.
    /// Every other tile queues nothing.
    fn meeting_candidates(&self, player: u8) -> Vec<usize> {
        let mut known = [false; 256];
        known[usize::from(player)] = true;
        if let Some(r) = self.s.player_runtime.iter().find(|r| r.player == player) {
            for &o in &r.known_players {
                known[usize::from(o)] = true;
            }
        }
        let unknown = |o: u8| o != NATURE && o != 0 && !known[usize::from(o)];
        let mut unit_here = vec![false; self.s.map.tiles.len()];
        for u in self.s.units.iter().filter(|u| unknown(u.owner)) {
            if let Some(k) = self.tile_index(u.coordinates).filter(|&k| k < unit_here.len()) {
                unit_here[k] = true;
            }
        }
        let territory = crate::env_var!("MEET_TERRITORY").is_ok();
        let tiles = &self.s.map.tiles;
        (0..tiles.len())
            .filter(|&i| {
                let t = &tiles[i];
                let unit = self.tile_index(t.coordinates).filter(|&k| k < unit_here.len()).is_none_or(|k| unit_here[k]);
                unit || ((t.improvement == improvement::CITY || territory) && unknown(t.owner))
            })
            .collect()
    }

    /// The players a tile introduces to `player`, in the order retail queues the meetings: the owner of a visible unit
    /// the player does not know, then the owner of a city on the tile that it does not know (unless that is the
    /// unit's owner). Nothing changes here; the knowledge is read as it stands.
    fn meets_on_tile(&self, player: u8, tile_index: usize) -> Vec<u8> {
        if player == NATURE || self.player(player).is_none() {
            return Vec::new();
        }
        let tile = self.s.map.tiles[tile_index];
        let unit_owner = self.visible_unit_for(tile.coordinates, player).map(|u| u.owner);
        if crate::env_var!("SEE_DEBUG").is_ok()
            && (unit_owner.is_some() || (tile.improvement == improvement::CITY && tile.owner != player))
        {
            eprintln!(
                "SEE cmd {} turn {} p{} sees tile ({},{}) unit_owner {:?} city_owner {} knows_unit {} knows_city {}",
                self.s.current_command,
                self.s.current_turn,
                player,
                tile.coordinates.x,
                tile.coordinates.y,
                unit_owner,
                if tile.improvement == improvement::CITY { tile.owner as i32 } else { -1 },
                unit_owner.map(|o| self.knows_player(player, o)).unwrap_or(false),
                self.knows_player(player, tile.owner)
            );
        }
        let unit_unknown = unit_owner.is_some_and(|o| o != NATURE && o != 0 && !self.knows_player(player, o));
        let city_unknown = (tile.improvement == improvement::CITY || crate::env_var!("MEET_TERRITORY").is_ok())
            && tile.owner != 0
            && tile.owner != NATURE
            && tile.owner != player
            && !self.knows_player(player, tile.owner);
        let mut out = Vec::new();
        if unit_unknown {
            let o = unit_owner.unwrap();
            out.push(o);
            if city_unknown && tile.owner != o {
                out.push(tile.owner);
            }
        } else if city_unknown {
            out.push(tile.owner);
        }
        out
    }

    /// Run queued meetings the way retail's action stack does: last queued first. A player queued twice is met once.
    fn run_meets(&mut self, player: u8, queued: Vec<u8>) {
        for other in queued.into_iter().rev() {
            if !self.knows_player(player, other) {
                self.meet(player, other);
            }
        }
    }

    /// Seeing an unknown player's unit or city on exploring triggers a meeting.
    fn meet_on_explore(&mut self, player: u8, tile_index: usize) {
        let queued = self.meets_on_tile(player, tile_index);
        self.run_meets(player, queued);
    }

    /// Retail's tribe-connection check over a list of tiles: it queues the meetings tile by tile, reading the
    /// knowledge as it was before any of them ran, and the stack then runs them in reverse.
    fn meet_over_tiles(&mut self, player: u8, tiles: &[usize]) {
        let mut queued = Vec::new();
        let grid = self.unit_grid_scope();
        for &i in tiles {
            if self.is_explored(player, i) {
                queued.extend(self.meets_on_tile(player, i));
            }
        }
        drop(grid);
        self.run_meets(player, queued);
    }

    /// Meet: reveal the capital (diplomacy tech), remember the player, stamp the first meeting, pay the reward.
    pub fn meet(&mut self, player: u8, other: u8) {
        if crate::env_var!("MEET_DEBUG").is_ok() {
            eprintln!("MEET cmd {} p{} meets p{}", self.s.current_command, player, other);
        }
        if let Some(r) = self.s.player_runtime.iter_mut().find(|r| r.player == player) {
            r.known_players.push(other);
        }
        if self.has_player_ability(player, player_ability::CAPITAL_VISION) {
            if let Some(start) = self.player(other).map(|p| p.start_tile) {
                if let Some(i) = self.tile_index(start) {
                    self.explore_tile(player, i);
                }
            }
        }
        let turn = self.s.current_turn as i32;
        crate::rules::diplomacy::get_relation_mut(&mut self.s, player, other).first_meet = turn;
        let score = if crate::env_var!("MEET_STORED_SCORE").is_ok() {
            self.player(other).map(|p| p.score).unwrap_or(0)
        } else {
            self.compute_score(other).max(0) as u32
        };
        let mut reward = 2 * (((score as f32) / 1000.0).ceil() as i32).min(5) + 1;
        let count = self.s.players.iter().filter(|p| p.id != NATURE).count();
        if self.s.version >= 123 && count == 2 && other == 1 {
            reward += 2;
        }
        if crate::env_var!("MEET_DEBUG").is_ok() {
            eprintln!("  reward {} (other score {})", reward, score);
        }
        self.add_currency(player, reward);
    }

    /// Explore from a tile with a sight range.
    ///
    /// A mountain tile always sees radius 2. The area includes the centre.
    pub fn explore_from(&mut self, player: u8, at: Wc, sight: i32) {
        self.explore_area(player, at, sight, 0, false);
    }

    /// Explore the area around `at` from position `start` of retail's order. With `stoppable`, a city reward raised
    /// by a tile lets one more explore action run and holds the rest: the level-up that the lighthouse's population
    /// change leads to reaches the action stack one action after the explore that caused it. Returns the position the
    /// held rest starts at.
    pub(super) fn explore_area(
        &mut self,
        player: u8,
        at: Wc,
        sight: i32,
        start: usize,
        stoppable: bool,
    ) -> Option<usize> {
        let tile = self.tile(at)?;
        let village2 = crate::env_var!("CITY_SIGHT").is_ok() && tile.improvement == improvement::CITY;
        let radius = if tile.terrain == terrain::MOUNTAIN || village2 { sight.max(2) } else { sight };
        // Retail walks the area column by column (x outer, y inner, the centre in its place) and queues one explore
        // action per tile from the last tile to the first; the action stack then runs them first tile first, and each
        // one queues its own meetings.
        let side = (2 * radius + 1) as usize;
        let mut left: Option<u8> = None;
        for k in start..side * side {
            if left == Some(0) {
                return Some(k);
            }
            let (dx, dy) = ((k / side) as i32 - radius, (k % side) as i32 - radius);
            let Some(i) = self.tile_index(Wc { x: at.x + dx, y: at.y + dy }) else { continue };
            let raised = self.reward_pending();
            self.explore_tile(player, i);
            match left {
                Some(n) => left = Some(n - 1),
                None if stoppable && !raised && self.reward_pending() => left = Some(1),
                None => {}
            }
        }
        None
    }

    pub fn explore_unit(&mut self, unit_id: u32) {
        let Some(u) = self.unit(unit_id) else { return };
        let (owner, at) = (u.owner, u.coordinates);
        let sight = self.sight_range(u);
        self.explore_from(owner, at, sight);
    }
}
