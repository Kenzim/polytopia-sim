//! The stable way in: set up a game by name, play it, and read and write commands as text or JSON.
//!
//! ```
//! use polytopia_sim::game::{GameMode, NewGame};
//! use polytopia_sim::MapSize;
//!
//! let mut game = NewGame::new().seed(7).size(MapSize::Tiny).mode(GameMode::Might).human("imperius").ai("bardur").build().unwrap();
//! let moves = game.legal_commands();
//! let text = moves[0].to_string();
//! let cmd = game.parse_command(&text).unwrap();
//! game.apply(cmd).unwrap();
//! ```
//!
//! Positions are JSON ([`Engine::to_position`], [`Engine::from_position`]); saves from the game load with
//! [`Engine::open_save`]. The items in this module and those methods are the release's public surface. The rest of
//! the crate is public too, for tools that need the rules in detail, but may change between minor releases.

use crate::engine::replay::RawCmd;
use crate::engine::{Command, Engine, GameOptions, NewGameError, PlayerSetup, Reject, Wc};
use crate::json::{obj, Json};
use crate::version::{GameVersion, VersionError};
use crate::GameState;
use crate::{BotDifficulty, MapPreset, MapSize, NATURE};
use std::fmt;
use std::str::FromStr;

/// How a game is won.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum GameMode {
    /// The highest score after the turn limit (30 by default in the game).
    Perfection,
    /// The last tribe standing, or the highest score at the turn or score limit if one is set.
    Domination,
    /// The first to the score limit.
    Glory,
    /// The first to hold every capital.
    Might,
}

impl GameMode {
    /// The game's own number for the mode (`GameOptions::mode`).
    pub fn code(self) -> i32 {
        match self {
            GameMode::Perfection => crate::GAME_MODE_PERFECTION,
            GameMode::Domination => crate::GAME_MODE_DOMINATION,
            GameMode::Glory => crate::GAME_MODE_GLORY,
            GameMode::Might => crate::GAME_MODE_MIGHT,
        }
    }
}

/// Lower case with spaces, hyphens and underscores dropped, so "Water World", "water-world" and "waterworld" agree.
fn normalise(name: &str) -> String {
    name.chars().filter(|c| c.is_alphanumeric()).flat_map(char::to_lowercase).collect()
}

/// A name [`FromStr`] did not recognise.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnknownName {
    pub kind: &'static str,
    pub name: String,
}

impl fmt::Display for UnknownName {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "unknown {}: {:?}", self.kind, self.name)
    }
}

impl std::error::Error for UnknownName {}

macro_rules! from_names {
    ($ty:ty, $kind:literal, $($name:literal => $value:expr),+ $(,)?) => {
        impl FromStr for $ty {
            type Err = UnknownName;
            fn from_str(s: &str) -> Result<Self, UnknownName> {
                match normalise(s).as_str() {
                    $($name => Ok($value),)+
                    _ => Err(UnknownName { kind: $kind, name: s.to_string() }),
                }
            }
        }
    };
}

from_names!(GameMode, "game mode",
    "perfection" => GameMode::Perfection, "domination" => GameMode::Domination, "glory" => GameMode::Glory, "might" => GameMode::Might);
from_names!(MapSize, "map size",
    "tiny" => MapSize::Tiny, "small" => MapSize::Small, "normal" => MapSize::Normal, "large" => MapSize::Large,
    "huge" => MapSize::Huge, "massive" => MapSize::Massive);
from_names!(MapPreset, "map type",
    "dryland" => MapPreset::Dryland, "lakes" => MapPreset::Lakes, "continents" => MapPreset::Continents,
    "archipelago" => MapPreset::Archipelago, "waterworld" => MapPreset::WaterWorld, "pangea" => MapPreset::Pangea);
from_names!(BotDifficulty, "AI difficulty",
    "easy" => BotDifficulty::Easy, "normal" => BotDifficulty::Normal, "hard" => BotDifficulty::Hard,
    "crazy" => BotDifficulty::Crazy);

/// The tribe number for a tribe name ("Xin-xi", "xinxi", "Imperius", ...) in a game version's rules.
pub fn tribe_index(version: i32, name: &str) -> Option<i32> {
    let rules = Engine::tables_for(version);
    let key = normalise(name);
    rules.logic.tribes.values().find(|t| t.idx > 1 && normalise(&t.id) == key).map(|t| t.idx)
}

/// The name of a tribe number in a game version's rules.
pub fn tribe_name(version: i32, tribe: i32) -> Option<String> {
    Engine::tables_for(version).tribe(tribe).filter(|t| t.idx > 1).map(|t| t.id.clone())
}

/// Why [`NewGame::build`] could not make a game.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SetupError {
    UnknownTribe(String),
    Version(VersionError),
    NewGame(NewGameError),
}

impl fmt::Display for SetupError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SetupError::UnknownTribe(name) => write!(f, "unknown tribe: {name:?}"),
            SetupError::Version(e) => write!(f, "{e}"),
            SetupError::NewGame(e) => write!(f, "{e}"),
        }
    }
}

impl std::error::Error for SetupError {}

impl fmt::Display for Reject {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Reject::NotYourTurn => write!(f, "not this player's turn"),
            Reject::PendingReward => write!(f, "a choice (such as a city reward) must be made first"),
            Reject::NoSuchUnit => write!(f, "no such unit"),
            Reject::NotAllowed(why) => write!(f, "not allowed: {why}"),
        }
    }
}

impl std::error::Error for Reject {}

/// A new game, set up by name. The defaults are the game's: the current game version, a Normal map of Continents,
/// Domination with no limits, Normal AI. Seats play in the order they are added.
#[derive(Debug, Clone)]
pub struct NewGame {
    opts: GameOptions,
    seats: Vec<(String, bool)>,
    map_attempts: i32,
}

impl Default for NewGame {
    fn default() -> Self {
        Self::new()
    }
}

impl NewGame {
    pub fn new() -> Self {
        NewGame {
            opts: GameOptions { players: Vec::new(), ..GameOptions::default() },
            seats: Vec::new(),
            map_attempts: 0,
        }
    }

    /// The game version (see the README's version table; new games start at 108 or later).
    pub fn version(mut self, version: i32) -> Self {
        self.opts.version = version;
        self
    }

    /// Map seed: the same settings and seed always give the same game.
    pub fn seed(mut self, seed: i32) -> Self {
        self.opts.seed = seed;
        self
    }

    pub fn size(mut self, size: MapSize) -> Self {
        self.opts.size = size;
        self
    }

    pub fn map_type(mut self, preset: MapPreset) -> Self {
        self.opts.preset = preset;
        self
    }

    pub fn mode(mut self, mode: GameMode) -> Self {
        self.opts.mode = mode.code();
        self
    }

    /// The game ends after this many turns; 0 for no limit.
    pub fn turn_limit(mut self, turns: i32) -> Self {
        self.opts.turn_limit = turns;
        self
    }

    /// The game ends when a player reaches this score; 0 for no limit.
    pub fn score_limit(mut self, score: i32) -> Self {
        self.opts.score_limit = score;
        self
    }

    /// How strongly the AI seats play.
    pub fn difficulty(mut self, difficulty: BotDifficulty) -> Self {
        self.opts.difficulty = difficulty;
        self
    }

    /// A seat that waits for commands.
    pub fn human(mut self, tribe: &str) -> Self {
        self.seats.push((tribe.to_string(), true));
        self
    }

    /// A seat the built-in AI plays when [`Engine::step_ai`] is called for it.
    pub fn ai(mut self, tribe: &str) -> Self {
        self.seats.push((tribe.to_string(), false));
        self
    }

    /// How many maps the generator may try in its search for a fair one. 0, the default, runs the game's own search
    /// and gives the same map as the game for the seed. 1 keeps the first map: about twenty times faster to set up,
    /// still a valid map, but not the game's.
    pub fn map_attempts(mut self, attempts: u32) -> Self {
        self.map_attempts = attempts.min(i32::MAX as u32) as i32;
        self
    }

    /// The low-level options these settings stand for.
    pub fn options(&self) -> Result<GameOptions, SetupError> {
        GameVersion::for_new_match(self.opts.version).map_err(SetupError::Version)?;
        let mut opts = self.opts.clone();
        opts.players = self
            .seats
            .iter()
            .map(|(name, human)| {
                tribe_index(opts.version, name)
                    .map(|tribe| PlayerSetup { tribe, human: *human })
                    .ok_or_else(|| SetupError::UnknownTribe(name.clone()))
            })
            .collect::<Result<_, _>>()?;
        Ok(opts)
    }

    pub fn build(&self) -> Result<Engine, SetupError> {
        let opts = self.options()?;
        Engine::try_new_game_with_map_attempts(&opts, self.map_attempts).map_err(SetupError::NewGame)
    }
}

/// The text form of a command, e.g. `Move { unit: 3, to: WorldCoordinates { x: 4, y: 5 } }` or `EndTurn`.
/// [`Engine::parse_command`] reads back the text of any command in the current [`Engine::legal_commands`] list.
/// The text names units by id and tiles by coordinates, so it only means something in the position it was listed in.
impl fmt::Display for Command {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Debug::fmt(self, f)
    }
}

fn wc_json(c: Wc) -> Json {
    Json::from(vec![c.x, c.y])
}

impl Engine {
    /// A game file: a position (`.json`), a save from the game (`.state`), or a pair of server records given as
    /// `initial.bin+current.bin`. A save loads at the position it was saved in.
    pub fn open_save(path: &str) -> Result<Engine, String> {
        if path.ends_with(".json") {
            let text = std::fs::read_to_string(path).map_err(|e| format!("{path}: {e}"))?;
            return Engine::from_position(&Json::parse(&text).map_err(|e| format!("{path}: {e}"))?);
        }
        let save = crate::engine::replay::SaveFile::open(path).map_err(|e| format!("{path}: {e}"))?;
        Ok(Engine::from_state(save.current))
    }

    /// The legal command whose text ([`Command`]'s `Display`) is `text`.
    pub fn parse_command(&self, text: &str) -> Result<Command, String> {
        let text = text.trim();
        self.legal_commands()
            .into_iter()
            .find(|c| c.to_string() == text)
            .ok_or_else(|| format!("not a legal command now: {text}"))
    }

    /// A command as JSON, with what a viewer needs to name it and mark it on the board: `text` and `kind` always;
    /// `unit`, `unit_type`, `from` (the acting unit's tile) and `to` (the tile acted on, the unit's own tile when
    /// there is none) when they apply; and the command's own fields (`train`, `improvement`, `tech`, `reward`,
    /// `upgrade_to`, `with`, `accept`, `target_type`, `target_owner`). Describe a command in the position it is
    /// played from.
    pub fn command_json(&self, c: &Command) -> Json {
        use Command::*;
        let text = c.to_string();
        let kind: String = text.chars().take_while(|ch| ch.is_alphanumeric()).collect();
        let mut o = obj().set("text", text.as_str()).set("kind", kind);
        let mut to = None;
        let unit = match *c {
            Move { unit, .. }
            | Attack { unit, .. }
            | Capture { unit, .. }
            | Promote { unit }
            | Recover { unit }
            | HealOthers { unit }
            | Explode { unit }
            | Disband { unit }
            | Upgrade { unit, .. }
            | ExamineRuins { unit }
            | Stay { unit }
            | Swarm { unit }
            | Disembark { unit }
            | FreezeArea { unit }
            | BreakIce { unit } => Some(unit),
            _ => None,
        };
        if let Some(id) = unit {
            o = o.set("unit", id);
            if let Some(u) = self.unit(id) {
                o = o.set("unit_type", u.type_idx).set("from", wc_json(u.coordinates));
                to = Some(u.coordinates);
            }
        }
        match *c {
            Move { to: t, .. } => to = Some(t),
            Attack { target, .. } => {
                to = Some(target);
                if let Some(t) = self.unit_at(target) {
                    o = o.set("target_type", t.type_idx).set("target_owner", t.owner);
                }
            }
            Capture { at, .. } | Build { at, .. } | Destroy { at } | Decompose { at } => to = Some(at),
            Train { city, .. } | CityReward { city, .. } => to = Some(city),
            _ => {}
        }
        if let Some(t) = to {
            o = o.set("to", wc_json(t));
        }
        match *c {
            Train { unit_type, .. } => o = o.set("train", unit_type),
            Build { improvement, .. } => o = o.set("improvement", improvement),
            Research { tech } => o = o.set("tech", tech),
            CityReward { reward, .. } => o = o.set("reward", format!("{reward:?}")),
            Upgrade { to, .. } => o = o.set("upgrade_to", to),
            BreakPeace { with } | PeaceTreaty { with } | EstablishEmbassy { with } => o = o.set("with", with),
            PeaceResponse { with, accept } => o = o.set("with", with).set("accept", accept),
            _ => {}
        }
        o.build()
    }

    /// Every player's score ([`Engine::score`]), in seat order (Nature left out).
    pub fn scores(&self) -> Vec<(u8, i32)> {
        self.s.players.iter().filter(|p| p.id != NATURE).map(|p| (p.id, self.score(p.id))).collect()
    }
}

/// What [`replay_save`] found.
#[derive(Clone)]
pub struct ReplayReport {
    /// Recorded commands the engine played.
    pub commands: usize,
    /// Recorded commands the engine would not have allowed (position in the record, command). They are played anyway.
    pub illegal: Vec<(usize, Command)>,
    /// Records with no engine command (the server's own bookkeeping, such as the end of a game).
    pub skipped: usize,
    /// With the AI check on: recorded moves of AI seats the engine's AI chose too, and those it did not.
    pub ai_agreed: usize,
    pub ai_differed: usize,
    /// Where the final state differs from the one in the file, per player (stars, score, cities, units, techs).
    pub differences: Vec<String>,
    /// The game after the last command.
    pub engine: Engine,
}

/// The `format` of a recorded game file.
pub const RECORDED_GAME_FORMAT: &str = "polytopia-recorded-game";

/// A recorded game: the state before the first command, the commands as the game recorded them, and the state the
/// record ends in.
struct Recording {
    initial: Engine,
    commands: Vec<RawCmd>,
    end: GameState,
}

fn open_recording(path: &str) -> Result<Recording, String> {
    if !path.ends_with(".json") {
        let save = crate::engine::replay::SaveFile::open(path).map_err(|e| format!("{path}: {e}"))?;
        return Ok(Recording { initial: Engine::from_state(save.initial), commands: save.commands, end: save.current });
    }
    let text = std::fs::read_to_string(path).map_err(|e| format!("{path}: {e}"))?;
    let j = Json::parse(&text).map_err(|e| format!("{path}: {e}"))?;
    if j.get("format").and_then(Json::as_str) != Some(RECORDED_GAME_FORMAT) {
        return Err(format!("{path}: not a recorded game (format {RECORDED_GAME_FORMAT:?})"));
    }
    let position = |key: &str| {
        j.get(key)
            .ok_or_else(|| format!("{path}: no {key:?}"))
            .and_then(|p| Engine::from_position(p).map_err(|e| format!("{path}: {key}: {e}")))
    };
    let mut commands = Vec::new();
    for (i, c) in j.get("commands").map(Json::as_arr).unwrap_or_default().iter().enumerate() {
        let n: Vec<i64> = c.as_arr().iter().filter_map(Json::as_i64).collect();
        let [kind, player, ax, ay, bx, by, unit, value, flag] = n[..] else {
            return Err(format!("{path}: command {i} is not nine numbers"));
        };
        commands.push(RawCmd {
            kind: kind as u16,
            player: player as u8,
            a: Wc { x: ax as i32, y: ay as i32 },
            b: Wc { x: bx as i32, y: by as i32 },
            unit: unit as u32,
            n: value as i32,
            flag: flag != 0,
        });
    }
    Ok(Recording { initial: position("initial")?, commands, end: position("end")?.s })
}

/// A save from the game (a `.state` file or `initial.bin+current.bin`) as a recorded game in the engine's own JSON
/// format: `{"format": "polytopia-recorded-game", "version": 1, "initial", "commands", "end"}`, where `initial` and
/// `end` are positions and each command is nine numbers as the game records it (`kind, player, a.x, a.y, b.x, b.y,
/// unit, value, flag`). Only what a position holds is kept, so player names and account ids are not carried over.
/// [`replay_save`] replays it like the save.
pub fn record_save(path: &str) -> Result<Json, String> {
    let save = crate::engine::replay::SaveFile::open(path).map_err(|e| format!("{path}: {e}"))?;
    let commands: Vec<Json> = save
        .commands
        .iter()
        .map(|c| {
            Json::from(vec![
                i64::from(c.kind),
                i64::from(c.player),
                c.a.x.into(),
                c.a.y.into(),
                c.b.x.into(),
                c.b.y.into(),
                i64::from(c.unit),
                c.n.into(),
                i64::from(c.flag),
            ])
        })
        .collect();
    Ok(obj()
        .set("format", RECORDED_GAME_FORMAT)
        .set("version", 1)
        .set("initial", Engine::from_state(save.initial).to_position())
        .set("commands", Json::Arr(commands))
        .set("end", Engine::from_state(save.current).to_position())
        .build())
}

/// Replay every recorded command of a game from its start, and compare the end with the state the file holds. The
/// file is a save from the game (a `.state` file or `initial.bin+current.bin`) or a recorded game from
/// [`record_save`] (`.json`). With `check_ai`, also ask the engine's AI for each move an AI seat made.
pub fn replay_save(path: &str, check_ai: bool) -> Result<ReplayReport, String> {
    let Recording { initial, commands, end } = open_recording(path)?;
    let mut e = initial;
    if commands.iter().any(|c| c.kind == 20) {
        e.start_match();
    }
    let mut report = ReplayReport {
        commands: 0,
        illegal: Vec::new(),
        skipped: 0,
        ai_agreed: 0,
        ai_differed: 0,
        differences: Vec::new(),
        engine: e.clone(),
    };
    for (i, c) in commands.iter().enumerate() {
        if c.kind == 20 || c.player == NATURE {
            continue;
        }
        // The end-match (22) and resign (35) records are not `Command`s; they have engine calls of their own.
        if c.kind == 22 {
            if e.end_match().is_err() {
                report.skipped += 1;
                e.s.current_command += 1;
            }
            continue;
        }
        if c.kind == 35 {
            if e.resign(c.n as u8).is_err() {
                e.s.current_command += 1;
            }
            continue;
        }
        let Some(cmd) = e.command_from_raw(c) else {
            report.skipped += 1;
            e.s.current_command += 1;
            continue;
        };
        if e.current_player() != c.player {
            return Err(format!(
                "{path}: record {i} is by player {} but the engine expects player {}",
                c.player,
                e.current_player()
            ));
        }
        if check_ai && e.player(c.player).is_some_and(|p| p.auto_play) {
            if e.ai_get_move() == cmd {
                report.ai_agreed += 1;
            } else {
                report.ai_differed += 1;
            }
        }
        // diplomacy answers and Stay are not in the legal command list but are always accepted
        let always = matches!(
            cmd,
            Command::EndTurn
                | Command::Stay { .. }
                | Command::PeaceTreaty { .. }
                | Command::PeaceResponse { .. }
                | Command::BreakPeace { .. }
                | Command::EstablishEmbassy { .. }
        );
        if !always && !e.legal_commands().contains(&cmd) {
            report.illegal.push((i, cmd));
        }
        e.apply_unchecked(cmd);
        report.commands += 1;
    }
    for p in end.players.iter().filter(|p| p.id != NATURE) {
        let file = (
            p.currency,
            p.score as i64,
            end.map
                .tiles
                .iter()
                .filter(|t| t.owner == p.id && t.improvement == crate::engine::tables::improvement::CITY)
                .count(),
            end.units.iter().filter(|u| u.owner == p.id).count(),
            end.tech.iter().find(|t| t.player == p.id).map_or(0, |t| t.tech.len()),
        );
        let ours = (
            e.player(p.id).map_or(-1, |m| m.currency),
            e.score(p.id) as i64,
            e.city_count(p.id) as usize,
            e.s.units.iter().filter(|u| u.owner == p.id).count(),
            e.techs(p.id).len(),
        );
        if file != ours {
            report.differences.push(format!(
                "player {}: stars {} / {}, score {} / {}, cities {} / {}, units {} / {}, techs {} / {} (file / engine)",
                p.id, file.0, ours.0, file.1, ours.1, file.2, ours.2, file.3, ours.3, file.4, ours.4
            ));
        }
    }
    report.engine = e;
    Ok(report)
}
