//! New games and loading a prepared state.

use super::*;
use crate::map::generate::generate;
use crate::{BotDifficulty, GameStatus, MapPreset, MapSize, PlayerRuntime, PlayerState, HUMAN_HANDICAP};

/// One seat of a new game.
#[derive(Debug, Clone, Copy)]
pub struct PlayerSetup {
    /// Tribe number in the rules data.
    pub tribe: i32,
    /// A human seat waits for commands; any other seat is played by the built-in AI.
    pub human: bool,
}

/// What [`Engine::new_game`] needs. `GameOptions::default()` is a two-player Normal Continents game at the current
/// game version; change the fields you care about.
#[derive(Debug, Clone)]
pub struct GameOptions {
    /// The game version to play ([`crate::GAME_VERSION`] by default).
    pub version: i32,
    /// Map seed; the same options always give the same game.
    pub seed: i32,
    pub size: MapSize,
    pub preset: MapPreset,
    /// The seats in turn order.
    pub players: Vec<PlayerSetup>,
    /// Strength of the AI seats.
    pub difficulty: BotDifficulty,
    /// `GameMode`: 1 Perfection, 2 Domination, 3 Glory, 4 Might (`GAME_MODE_MIGHT`: the game ends when one player
    /// holds every capital). Leave the limits at 0 for Might.
    pub mode: i32,
    /// The game ends after this many turns; 0 for no limit.
    pub turn_limit: i32,
    /// The game ends when a player reaches this score; 0 for no limit.
    pub score_limit: i32,
}

impl Default for GameOptions {
    fn default() -> Self {
        Self {
            version: crate::GAME_VERSION,
            seed: 1,
            size: MapSize::Normal,
            preset: MapPreset::Continents,
            players: vec![PlayerSetup { tribe: 4, human: true }, PlayerSetup { tribe: 7, human: false }],
            difficulty: BotDifficulty::Normal,
            mode: crate::GAME_MODE_DOMINATION,
            turn_limit: 0,
            score_limit: 0,
        }
    }
}

/// Why [`Engine::try_new_game`] could not make a game.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NewGameError {
    /// `GameOptions::players` is empty.
    NoPlayers,
    /// More seats than the map size holds.
    TooManyPlayers { size: MapSize, requested: usize, max: usize },
    /// More seats than a map of `side` tiles a side holds ([`Engine::try_new_game_with_seeds`]).
    TooManyPlayersOnSide { side: u16, requested: usize, max: usize },
}

impl std::fmt::Display for NewGameError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            NewGameError::NoPlayers => write!(f, "a new game needs at least one player"),
            NewGameError::TooManyPlayers { size, requested, max } => {
                write!(f, "{requested} players do not fit a {size:?} map (at most {max})")
            }
            NewGameError::TooManyPlayersOnSide { side, requested, max } => {
                write!(f, "{requested} players do not fit a {side}x{side} map (at most {max})")
            }
        }
    }
}

impl std::error::Error for NewGameError {}

impl Engine {
    /// Rule tables for a game version.
    pub fn tables_for(version: i32) -> Arc<Tables> {
        // Built once per version and shared: the tables are read-only after construction.
        static CACHE: std::sync::Mutex<Vec<(i32, Arc<Tables>)>> = std::sync::Mutex::new(Vec::new());
        if let Some((_, t)) = CACHE.lock().unwrap().iter().find(|(v, _)| *v == version) {
            return Arc::clone(t);
        }
        let logic = crate::data::load_game_logic(version)
            .or_else(|_| crate::data::load_embedded_game_logic())
            .expect("embedded rules");
        let t = Arc::new(Tables::new(logic));
        CACHE.lock().unwrap().push((version, Arc::clone(&t)));
        t
    }

    /// Generate a map and set up players, techs, fog and starting units.
    ///
    /// Panics when the options cannot make a game (see [`Engine::try_new_game`]).
    pub fn new_game(opts: &GameOptions) -> Self {
        Self::try_new_game(opts).unwrap_or_else(|e| panic!("{e}"))
    }

    /// [`Engine::new_game`], returning an error instead of panicking when there are no seats or more seats than the
    /// map size holds ([`MapSize::max_players`]).
    pub fn try_new_game(opts: &GameOptions) -> Result<Self, NewGameError> {
        Self::try_new_game_with_map_attempts(opts, 0)
    }

    /// [`Engine::try_new_game`] with a cap on the map generator's attempts at a fair map. 0 runs the game's own search
    /// (the map the game makes for this seed); 1 keeps the first map, which is much faster but a different map.
    pub(crate) fn try_new_game_with_map_attempts(opts: &GameOptions, map_attempts: i32) -> Result<Self, NewGameError> {
        if opts.players.is_empty() {
            return Err(NewGameError::NoPlayers);
        }
        let (requested, max) = (opts.players.len(), opts.size.max_players());
        if requested > max {
            return Err(NewGameError::TooManyPlayers { size: opts.size, requested, max });
        }
        Self::build_new_game(opts, opts.seed, |players| {
            generate(opts.version, opts.seed, opts.size, opts.preset, players, map_attempts)
        })
    }

    /// [`Engine::try_new_game`] on a map of `side` tiles a side (any side the generator can place the players on, not
    /// only the game's sizes; `opts.size` is not used), with the seeds given separately: the map generator draws each
    /// attempt's seed from `next_map_seed`, and `game_seed` is the game's own random seed (ruin rewards, explorers, AI
    /// noise). `opts.seed` is not used.
    pub fn try_new_game_with_seeds(
        opts: &GameOptions,
        side: u16,
        game_seed: i32,
        next_map_seed: &mut dyn FnMut() -> i32,
    ) -> Result<Self, NewGameError> {
        if opts.players.is_empty() {
            return Err(NewGameError::NoPlayers);
        }
        let (requested, max) = (opts.players.len(), crate::max_players_on_side(side));
        if requested > max {
            return Err(NewGameError::TooManyPlayersOnSide { side, requested, max });
        }
        Self::build_new_game(opts, game_seed, |players| {
            crate::map::generate::generate_side_with_seed_source(
                opts.version,
                side,
                opts.preset,
                players,
                0,
                next_map_seed,
            )
        })
    }

    /// A new game on the map `make_map` generates; the callers have checked the player count.
    fn build_new_game(
        opts: &GameOptions,
        game_seed: i32,
        make_map: impl FnOnce(&mut [PlayerState]) -> crate::Map,
    ) -> Result<Self, NewGameError> {
        let tables = Self::tables_for(opts.version);
        let handicap = crate::ai::decide::handicap_from_difficulty(opts.difficulty);
        let mut players: Vec<PlayerState> = opts
            .players
            .iter()
            .enumerate()
            .map(|(i, p)| PlayerState {
                id: i as u8 + 1,
                handicap: if p.human { HUMAN_HANDICAP } else { handicap },
                currency: tables.tribe(p.tribe).and_then(|t| t.starting_stars).unwrap_or(5),
                score: 0,
                tribe: p.tribe,
                auto_play: !p.human,
                start_tile: WorldCoordinates { x: -1, y: -1 },
            })
            .collect();
        // Nature is the last entry of the player list, as in retail. It must be there when the map is generated: the
        // climate pass gives one region to every list entry but the last, so without Nature the last seat gets none.
        players.push(PlayerState {
            id: NATURE,
            handicap: 0,
            currency: 0,
            score: 0,
            tribe: 1,
            auto_play: true,
            start_tile: WorldCoordinates { x: -1, y: -1 },
        });
        let map = make_map(&mut players);
        let mut s = GameState::new(opts.version, game_seed);
        s.map = map;
        s.players = players;
        s.settings.set_game_mode(opts.mode);
        s.settings.turn_limit = opts.turn_limit;
        s.settings.score_limit = opts.score_limit;
        s.settings.difficulty = opts.difficulty as i32;
        s.settings.opponent_count = opts.players.len() as i32 - 1;
        s.settings.map_preset = opts.preset as i32;
        let mut e = Engine::new(tables, s);
        e.finish_setup();
        Ok(e)
    }

    /// Build everything derived from the map and players of `self.s`.
    pub fn finish_setup(&mut self) {
        self.init_fog();
        // Ruling cities for the tiles the generator already assigned.
        let cities: Vec<(Wc, u8)> = self
            .s
            .map
            .tiles
            .iter()
            .filter(|t| t.improvement == improvement::CITY && t.owner != 0)
            .map(|t| (t.coordinates, t.owner))
            .collect();
        for (c, owner) in &cities {
            self.new_city_runtime(*c, *owner);
            if let Some(i) = self.tile_index(*c) {
                self.s.map.tiles[i].ruling_city = *c;
            }
            for n in self.area(*c, 1) {
                if let Some(i) = self.tile_index(n) {
                    if self.s.map.tiles[i].owner == *owner {
                        self.s.map.tiles[i].ruling_city = *c;
                    }
                }
            }
        }
        // Every other improvement the generator placed (neutral villages, ruins, the corner lighthouses) carries a record
        // from the start, as in the game's starting saves.
        let unrecorded: Vec<(i32, i32)> = self
            .s
            .map
            .tiles
            .iter()
            .filter(|t| t.improvement != 0 && !self.s.improvements.iter().any(|r| r.tile == t.index))
            .map(|t| (t.index, t.improvement))
            .collect();
        for (tile, kind) in unrecorded {
            let (level, production, border_size) = match kind {
                improvement::CITY => (1, 1, 1),
                crate::map::contents::IMPROVEMENT_LIGHTHOUSE => (1, 0, 0),
                crate::map::contents::IMPROVEMENT_RUIN => (1, 1, 0),
                _ => (0, 0, 0),
            };
            self.s.improvements.push(ImprovementRuntime {
                tile,
                level,
                production,
                border_size,
                ..ImprovementRuntime::default()
            });
        }
        // Villages rule their own tile for unit-support purposes only after capture.
        let ids: Vec<(u8, i32)> = self.s.players.iter().map(|p| (p.id, p.tribe)).collect();
        for (id, tribe) in ids {
            // Placing the capital sets the player's city counter to one.
            let counter = self.city_count(id);
            self.s.player_runtime.push(PlayerRuntime {
                player: id,
                climate: tribe,
                cities: counter,
                ..PlayerRuntime::default()
            });
            let start: Vec<i32> = self
                .t
                .tribe(tribe)
                .map(|t| t.starting_tech.iter().filter_map(|n| self.t.tech_idx.get(n).copied()).collect())
                .unwrap_or_default();
            self.s.tech.push(crate::OwnedTech { player: id, tech: start });
            self.enable_start_tasks(id);
            // Start tile: the capital the generator picked.
            if id == NATURE {
                continue;
            }
            let cap = cities
                .iter()
                .find(|(c, o)| *o == id && self.tile(*c).is_some_and(|t| t.is_capital))
                .map(|(c, _)| *c)
                .or_else(|| cities.iter().find(|(_, o)| *o == id).map(|(c, _)| *c));
            if let Some(cap) = cap {
                // A tribe with the city level bonus (Luxidoor) starts with its capital at level 3; nothing else about
                // the city changes.
                if self.t.tribe(tribe).is_some_and(|t| t.bonus == crate::map::contents::BONUS_CITY_LEVEL_3) {
                    if let Some(rt) = self.improvement_rt_mut(cap) {
                        rt.level = 3;
                    }
                }
                if let Some(p) = self.player_mut(id) {
                    p.start_tile = cap;
                }
                // The game's generator explores the capital's radius 2 and then clears the explorers of the four map
                // corners, so a lighthouse in that area starts unexplored and earns nothing.
                let (w, h) = (self.width() - 1, self.height() - 1);
                let start: Vec<usize> = (-2..=2)
                    .flat_map(|dx| (-2..=2).map(move |dy| Wc { x: cap.x + dx, y: cap.y + dy }))
                    .filter(|c| !((c.x == 0 || c.x == w) && (c.y == 0 || c.y == h)))
                    .filter_map(|c| self.tile_index(c))
                    .collect();
                for i in start {
                    self.explore_tile(id, i);
                }
                let unit_name = self.t.tribe(tribe).map(|t| t.starting_unit.clone()).unwrap_or_default();
                if let Some(&uidx) = self.t.unit_idx.get(&unit_name) {
                    if uidx != 0 {
                        self.spawn_unit(id, uidx, cap, cap, false);
                    }
                }
            }
        }
        self.s.current_state = GameStatus::Started;
        self.s.current_player_index = 0;
        self.s.current_turn = 0;
        self.refresh_scores();
    }
}
