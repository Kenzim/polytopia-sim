//! Replaying a retail save: the command stack of the file holds every command of the game.

use super::load::{lz4_block, read_game_state, Reader};
use super::*;

/// One serialised command (command type ids as the game writes them).
#[derive(Debug, Clone, Copy, Default)]
pub struct RawCmd {
    pub kind: u16,
    pub player: u8,
    pub a: Wc,
    pub b: Wc,
    pub unit: u32,
    pub n: i32,
    pub flag: bool,
}

pub struct SaveFile {
    pub initial: GameState,
    pub current: GameState,
    pub commands: Vec<RawCmd>,
}

fn read_commands(r: &mut Reader) -> Result<Vec<RawCmd>, String> {
    let triggers = r.u16()?;
    for _ in 0..triggers {
        r.u8()?;
        r.u16()?;
        r.wc()?;
        r.u8()?;
    }
    let n = r.u16()?;
    if crate::env_var!("LOAD_DEBUG").is_ok() {
        eprintln!("{} commands at pos {}", n, r.p);
    }
    let mut out = Vec::new();
    for ci in 0..n {
        if crate::env_var!("LOAD_DEBUG").is_ok() && ci + 12 > n {
            eprintln!("cmd {} at pos {}", ci, r.p);
        }
        out.push(read_one(r)?);
    }
    Ok(out)
}

fn read_one(r: &mut Reader) -> Result<RawCmd, String> {
    {
        let kind = r.u16()?;
        let mut c = RawCmd { kind, player: r.u8()?, ..RawCmd::default() };
        match kind {
            1 | 5 | 16 => {
                c.n = i32::from(r.u16()?);
                c.a = r.wc()?;
            }
            39 => {
                c.n = i32::from(r.u16()?);
                c.a = r.wc()?;
            }
            2 => {
                c.unit = r.u32()?;
                c.a = r.wc()?;
                c.b = r.wc()?;
            }
            3 | 4 | 9 | 10 | 13 | 14 | 17 | 18 | 21 | 24 | 25 | 26 | 33 | 36 | 37 | 38 => c.a = r.wc()?,
            6 => {
                c.a = r.wc()?;
                c.b = r.wc()?;
                c.unit = r.u32()?;
            }
            7 => {
                c.unit = r.u32()?;
                c.a = r.wc()?;
            }
            8 => c.n = i32::from(r.u16()?),
            11 => {
                c.a = r.wc()?;
                c.n = i32::from(r.u16()?);
            }
            15 | 20 | 22 => {}
            27 | 29 | 30 | 32 => {
                c.a = r.wc()?;
                c.n = i32::from(r.u8()?);
            }
            28 => {
                c.n = i32::from(r.u8()?);
                c.flag = r.bool()?;
            }
            35 => {
                c.n = i32::from(r.u8()?);
                c.unit = u32::from(r.u8()?);
                c.flag = r.bool()?;
            }
            other => return Err(format!("unknown command type {other} at {}", r.p)),
        }
        Ok(c)
    }
}

fn read_tail(r: &mut Reader, version: i32) -> Result<(), String> {
    let actions = r.u16()?;
    if actions != 0 {
        return Err(format!("{actions} pending actions in the save"));
    }
    if version > 0x61 {
        r.bool()?;
    }
    if version > 0x79 {
        r.i32()?;
    }
    Ok(())
}

impl SaveFile {
    /// The server's initial and current game state data (a share link or replay, dumped by
    /// `tools/grab_replay.py`): each is the `GameState` version, the serialised `GameState`, then its command stack.
    pub fn parse_server(initial: &[u8], current: &[u8]) -> Result<SaveFile, String> {
        let mut r = Reader { b: initial, p: 4 };
        let initial = read_game_state(&mut r)?;
        let mut r = Reader { b: current, p: 4 };
        let state = read_game_state(&mut r)?;
        if crate::env_var!("LOAD_DEBUG").is_ok() {
            eprintln!("current state parsed, pos {} of {}", r.p, current.len());
        }
        let commands = read_commands(&mut r)?;
        // A save of a live game can be paused mid-stack, e.g. on a city reward: the pending actions come
        // last, and replaying the commands rebuilds them (`Engine::deferred`), so stop reading there.
        if r.u16()? == 0 {
            r.p -= 2;
            read_tail(&mut r, state.version)?;
        }
        if crate::env_var!("LOAD_DEBUG").is_ok() {
            eprintln!("{} commands, end {} of {}", commands.len(), r.p, current.len());
        }
        Ok(SaveFile { initial, current: state, commands })
    }

    /// The pending command triggers saved after a live game's current `GameState`: (player, trigger kind, tile).
    pub fn live_triggers(current: &[u8]) -> Result<Vec<(u8, u16, Wc)>, String> {
        let mut r = Reader { b: current, p: 4 };
        read_game_state(&mut r)?;
        let mut out = Vec::new();
        for _ in 0..r.u16()? {
            let player = r.u8()?;
            let kind = r.u16()?;
            let at = r.wc()?;
            r.u8()?;
            out.push((player, kind, at));
        }
        Ok(out)
    }

    /// A `.state` file, or a pair of server blobs given as `initial.bin+current.bin`.
    pub fn open(path: &str) -> Result<SaveFile, String> {
        if let Some((a, b)) = path.split_once('+') {
            let ia = std::fs::read(a).map_err(|e| e.to_string())?;
            let cb = std::fs::read(b).map_err(|e| e.to_string())?;
            return SaveFile::parse_server(&ia, &cb);
        }
        SaveFile::parse(&std::fs::read(path).map_err(|e| e.to_string())?)
    }

    pub fn parse(bytes: &[u8]) -> Result<SaveFile, String> {
        let data = lz4_block(&bytes[3..])?;
        let mut r = Reader { b: &data, p: 0 };
        let _file_version = r.i32()?;
        let initial = read_game_state(&mut r)?;
        if crate::env_var!("LOAD_DEBUG").is_ok() {
            eprintln!("initial state parsed, pos {} of {}", r.p, data.len());
        }
        let _pending = read_commands(&mut r)?;
        read_tail(&mut r, initial.version)?;
        if r.p >= data.len() {
            return Ok(SaveFile { current: initial.clone(), initial, commands: Vec::new() });
        }
        if crate::env_var!("LOAD_DEBUG").is_ok() {
            eprintln!("tail parsed, pos {} of {}", r.p, data.len());
        }
        let current = read_game_state(&mut r)?;
        if crate::env_var!("LOAD_DEBUG").is_ok() {
            eprintln!("current state parsed, pos {}", r.p);
        }
        // Some saves carry a few extra bytes at the end of the state that the loader does not know yet:
        // resynchronise on the first offset whose command list parses and ends exactly at the end of the data.
        // The file ends with the last-seen command (u16). An empty history (a weekly challenge that has
        // not been played yet) is only accepted at the real offset, where a false match is not possible.
        let start = r.p;
        let mut last_err = String::new();
        for delta in 0..=12usize {
            let mut r2 = Reader { b: &data, p: start + delta };
            match read_commands(&mut r2).and_then(|c| read_tail(&mut r2, current.version).map(|_| c)) {
                Ok(commands)
                    if r2.p <= data.len() && data.len() - r2.p <= 2 && (!commands.is_empty() || delta == 0) =>
                {
                    if delta != 0 && crate::env_var!("LOAD_DEBUG").is_ok() {
                        eprintln!("resynchronised the command list at +{delta}");
                    }
                    return Ok(SaveFile { initial, current, commands });
                }
                Ok(c) => {
                    if crate::env_var!("LOAD_DEBUG").is_ok() {
                        eprintln!("delta {delta}: {} commands end at {} of {}", c.len(), r2.p, data.len());
                    }
                    last_err = "command list does not end at the end of the data".into()
                }
                Err(e) => {
                    if crate::env_var!("LOAD_DEBUG").is_ok() {
                        eprintln!("delta {delta}: {e}");
                    }
                    last_err = e
                }
            }
        }
        Err(last_err)
    }
}

impl RawCmd {
    /// Parse `to_bytes` output (no version prefix).
    pub fn from_bytes(b: &[u8]) -> Result<RawCmd, String> {
        read_one(&mut Reader { b, p: 0 })
    }

    /// The command as a command stack holds it: command type (u16), player byte, then the fields in the order
    /// `read_commands` reads them. Prefix the `GameState` version (i32) to get the form a command is read back from.
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut o = Vec::new();
        let wc = |o: &mut Vec<u8>, w: Wc| {
            o.extend_from_slice(&w.x.to_le_bytes());
            o.extend_from_slice(&w.y.to_le_bytes());
        };
        o.extend_from_slice(&self.kind.to_le_bytes());
        o.push(self.player);
        match self.kind {
            1 | 5 | 16 | 39 => {
                o.extend_from_slice(&(self.n as u16).to_le_bytes());
                wc(&mut o, self.a);
            }
            2 => {
                o.extend_from_slice(&self.unit.to_le_bytes());
                wc(&mut o, self.a);
                wc(&mut o, self.b);
            }
            6 => {
                wc(&mut o, self.a);
                wc(&mut o, self.b);
                o.extend_from_slice(&self.unit.to_le_bytes());
            }
            7 => {
                o.extend_from_slice(&self.unit.to_le_bytes());
                wc(&mut o, self.a);
            }
            8 => o.extend_from_slice(&(self.n as u16).to_le_bytes()),
            11 => {
                wc(&mut o, self.a);
                o.extend_from_slice(&(self.n as u16).to_le_bytes());
            }
            27 | 29 | 30 | 32 => {
                wc(&mut o, self.a);
                o.push(self.n as u8);
            }
            28 => {
                o.push(self.n as u8);
                o.push(u8::from(self.flag));
            }
            15 | 20 | 22 => {}
            _ => wc(&mut o, self.a),
        }
        o
    }
}

impl Engine {
    /// The serialised form of an engine command for the current player (the inverse of `command_from_raw`).
    /// Diplomacy commands carry the other player's capital tile, as retail's do.
    pub fn command_to_raw(&self, cmd: &Command) -> Option<RawCmd> {
        let player = self.current_player();
        let pos = |id: u32| self.unit(id).map(|u| u.coordinates);
        let raw = |kind: u16| RawCmd { kind, player, ..RawCmd::default() };
        let at = |kind: u16, a: Wc| RawCmd { a, ..raw(kind) };
        let with = |kind: u16, other: u8| RawCmd {
            a: self.capital_of(other).unwrap_or_default(),
            n: i32::from(other),
            ..raw(kind)
        };
        Some(match *cmd {
            Command::Build { at: a, improvement } => RawCmd { n: improvement, ..at(1, a) },
            Command::Attack { unit, target } => RawCmd { unit, b: target, ..at(2, pos(unit)?) },
            Command::Recover { unit } => at(3, pos(unit)?),
            Command::HealOthers { unit } => at(4, pos(unit)?),
            Command::Train { city, unit_type } => RawCmd { n: unit_type, ..at(5, city) },
            Command::Move { unit, to } => RawCmd { unit, b: to, ..at(6, pos(unit)?) },
            Command::Capture { unit, at: a } => RawCmd { unit, ..at(7, a) },
            Command::Research { tech } => RawCmd { n: tech, ..raw(8) },
            Command::Destroy { at: a } => at(9, a),
            Command::Disband { unit } => at(10, pos(unit)?),
            Command::CityReward { city, reward } => RawCmd { n: reward as i32, ..at(11, city) },
            Command::Promote { unit } => at(13, pos(unit)?),
            Command::FreezeArea { unit } => at(17, pos(unit)?),
            Command::BreakIce { unit } => at(18, pos(unit)?),
            Command::ExamineRuins { unit } => at(14, pos(unit)?),
            Command::EndTurn => raw(15),
            Command::Upgrade { unit, to } => RawCmd { n: to, ..at(16, pos(unit)?) },
            Command::Stay { unit } => at(21, pos(unit)?),
            Command::Explode { unit } => at(24, pos(unit)?),
            Command::Boost { unit } => at(25, pos(unit)?),
            Command::Decompose { at: a } => at(26, a),
            Command::PeaceTreaty { with: o } => with(27, o),
            Command::PeaceResponse { with: o, accept } => RawCmd { n: i32::from(o), flag: accept, ..raw(28) },
            Command::BreakPeace { with: o } => with(29, o),
            Command::EstablishEmbassy { with: o } => with(30, o),
            Command::Disembark { unit } => at(36, pos(unit)?),
            Command::Swarm { unit } => at(38, pos(unit)?),
            Command::ClearTileEffect { at: a, effect } => RawCmd { n: i32::from(effect), ..at(39, a) },
        })
    }

    /// Start the match: the starting techs are granted and the command counter moves to 1.
    pub fn start_match(&mut self) {
        let ids: Vec<(u8, i32)> = self.s.players.iter().map(|p| (p.id, p.tribe)).collect();
        for (id, tribe) in ids {
            let start: Vec<i32> = self
                .t
                .tribe(tribe)
                .map(|t| t.starting_tech.iter().filter_map(|n| self.t.tech_idx.get(n).copied()).collect())
                .unwrap_or_default();
            if let Some(row) = self.s.tech.iter_mut().find(|t| t.player == id) {
                row.tech = start;
            }
            self.enable_start_tasks(id);
        }
        self.s.current_state = crate::GameStatus::Started;
        self.s.current_command = 1;
        self.refresh_scores();
    }

    /// Translate a serialised command into an engine command (None when the engine has no such command).
    pub fn command_from_raw(&self, c: &RawCmd) -> Option<Command> {
        let unit_at = |w: Wc| self.unit_at(w).map(|u| u.id);
        Some(match c.kind {
            1 => Command::Build { at: c.a, improvement: c.n },
            2 => Command::Attack { unit: c.unit, target: c.b },
            3 => Command::Recover { unit: unit_at(c.a)? },
            4 => Command::HealOthers { unit: unit_at(c.a)? },
            5 => Command::Train { city: c.a, unit_type: c.n },
            6 => Command::Move { unit: c.unit, to: c.b },
            7 => Command::Capture { unit: c.unit, at: c.a },
            8 => Command::Research { tech: c.n },
            9 => Command::Destroy { at: c.a },
            10 => Command::Disband { unit: unit_at(c.a)? },
            11 => Command::CityReward {
                city: c.a,
                reward: match c.n {
                    1 => crate::CityReward::CityWall,
                    2 => crate::CityReward::Park,
                    3 => crate::CityReward::Workshop,
                    4 => crate::CityReward::Explorer,
                    5 => crate::CityReward::BorderGrowth,
                    6 => crate::CityReward::SuperUnit,
                    7 => crate::CityReward::Resources,
                    8 => crate::CityReward::PopulationGrowth,
                    _ => crate::CityReward::None,
                },
            },
            13 => Command::Promote { unit: unit_at(c.a)? },
            14 => Command::ExamineRuins { unit: unit_at(c.a)? },
            15 => Command::EndTurn,
            // The end-match command (22) is not a `Command`: apply it with [`Engine::end_match`].
            17 => Command::FreezeArea { unit: unit_at(c.a)? },
            18 => Command::BreakIce { unit: unit_at(c.a)? },
            16 => Command::Upgrade { unit: unit_at(c.a)?, to: c.n },
            21 => Command::Stay { unit: unit_at(c.a)? },
            24 => Command::Explode { unit: unit_at(c.a)? },
            25 => Command::Boost { unit: unit_at(c.a)? },
            26 => Command::Decompose { at: c.a },
            27 => Command::PeaceTreaty { with: c.n as u8 },
            28 => Command::PeaceResponse { with: c.n as u8, accept: c.flag },
            29 => Command::BreakPeace { with: c.n as u8 },
            30 => Command::EstablishEmbassy { with: c.n as u8 },
            38 => Command::Swarm { unit: unit_at(c.a)? },
            36 => Command::Disembark { unit: unit_at(c.a)? },
            39 => Command::ClearTileEffect { at: c.a, effect: c.n as u16 },
            _ => return None,
        })
    }
}
