//! Loading a retail `.state` save (game version 121 layout).

use super::*;
use crate::{
    Aggression, CityReward, DiplomacyLink, DiplomacyRelation, DiplomacyRelationState, GameStatus, ImprovementRuntime,
    Map, OwnedTech, PlayerDiplomacy, PlayerRuntime, PlayerState, TileExplorers, UnitState,
};

/// LZ4 block decompression (no frame header).
pub fn lz4_block(src: &[u8]) -> Result<Vec<u8>, String> {
    let mut out: Vec<u8> = Vec::with_capacity(src.len() * 6);
    let mut i = 0usize;
    while i < src.len() {
        let token = src[i];
        i += 1;
        let mut n = (token >> 4) as usize;
        if n == 15 {
            loop {
                let b = *src.get(i).ok_or("lz4: truncated length")?;
                i += 1;
                n += b as usize;
                if b != 255 {
                    break;
                }
            }
        }
        out.extend_from_slice(src.get(i..i + n).ok_or("lz4: truncated literals")?);
        i += n;
        if i >= src.len() {
            break;
        }
        let off = *src.get(i).ok_or("lz4: truncated offset")? as usize
            | (*src.get(i + 1).ok_or("lz4: truncated offset")? as usize) << 8;
        i += 2;
        let mut m = (token & 15) as usize;
        if m == 15 {
            loop {
                let b = *src.get(i).ok_or("lz4: truncated match length")?;
                i += 1;
                m += b as usize;
                if b != 255 {
                    break;
                }
            }
        }
        if off == 0 || off > out.len() {
            return Err("lz4: bad offset".into());
        }
        for _ in 0..m + 4 {
            let b = out[out.len() - off];
            out.push(b);
        }
    }
    Ok(out)
}

pub(crate) struct Reader<'a> {
    pub(crate) b: &'a [u8],
    pub(crate) p: usize,
}

impl<'a> Reader<'a> {
    pub(crate) fn take(&mut self, n: usize) -> Result<&'a [u8], String> {
        let s = self.b.get(self.p..self.p + n).ok_or_else(|| format!("read past end at {}", self.p))?;
        self.p += n;
        Ok(s)
    }
    pub(crate) fn u8(&mut self) -> Result<u8, String> {
        Ok(self.take(1)?[0])
    }
    pub(crate) fn bool(&mut self) -> Result<bool, String> {
        Ok(self.u8()? != 0)
    }
    pub(crate) fn u16(&mut self) -> Result<u16, String> {
        Ok(u16::from_le_bytes(self.take(2)?.try_into().unwrap()))
    }
    pub(crate) fn i16(&mut self) -> Result<i16, String> {
        Ok(i16::from_le_bytes(self.take(2)?.try_into().unwrap()))
    }
    pub(crate) fn i32(&mut self) -> Result<i32, String> {
        Ok(i32::from_le_bytes(self.take(4)?.try_into().unwrap()))
    }
    pub(crate) fn u32(&mut self) -> Result<u32, String> {
        Ok(u32::from_le_bytes(self.take(4)?.try_into().unwrap()))
    }
    pub(crate) fn f32(&mut self) -> Result<f32, String> {
        Ok(f32::from_le_bytes(self.take(4)?.try_into().unwrap()))
    }
    pub(crate) fn string(&mut self) -> Result<String, String> {
        let mut n = 0usize;
        let mut shift = 0;
        loop {
            let c = self.u8()?;
            n |= ((c & 0x7f) as usize) << shift;
            shift += 7;
            if c & 0x80 == 0 {
                break;
            }
        }
        Ok(String::from_utf8_lossy(self.take(n)?).into_owned())
    }
    pub(crate) fn wc(&mut self) -> Result<Wc, String> {
        Ok(Wc { x: self.i32()?, y: self.i32()? })
    }
}

fn read_unit(r: &mut Reader, v: i32) -> Result<UnitState, String> {
    let mut u = UnitState::default();
    u.id = r.u32()?;
    u.owner = r.u8()?;
    u.type_idx = i32::from(r.u16()?);
    u.follower = r.u32()?;
    u.leader = r.u32()?;
    u.coordinates = r.wc()?;
    u.home = r.wc()?;
    u.previous_turn_end = u.coordinates;
    u.health = r.u16()?;
    u.promotion = r.u16()?;
    u.xp = r.u16()?;
    u.moved = r.bool()?;
    u.attacked = r.bool()?;
    let _flipped = r.bool()?;
    u.created_turn = r.u16()?;
    if r.bool()? {
        u.passenger = Some(Box::new(read_unit(r, v)?));
    }
    for _ in 0..r.u16()? {
        match r.u16()? {
            0 => u.frozen = true,
            1 => u.poisoned = true,
            2 => u.boosted = true,
            3 => u.invisible = true,
            4 => u.bubble = true,
            5 => u.petrified = true,
            6 => u.swift = true,
            7 => u.double_ready = true,
            8 => u.charmed = true,
            _ => {}
        }
    }
    u.birth_climate = i32::from(r.i16()?);
    u.direction = r.u8()?;
    if v > 0x56 {
        let _skin = r.i16()?;
    }
    Ok(u)
}

pub(crate) fn reward_from(v: u16) -> CityReward {
    match v {
        1 => CityReward::CityWall,
        2 => CityReward::Park,
        3 => CityReward::Workshop,
        4 => CityReward::Explorer,
        5 => CityReward::BorderGrowth,
        6 => CityReward::SuperUnit,
        7 => CityReward::Resources,
        8 => CityReward::PopulationGrowth,
        11 => CityReward::Rebellion,
        13 => CityReward::Poison,
        100 => CityReward::TutorialExplorer,
        _ => CityReward::None,
    }
}

/// The game state up to and including the player list (version 121 layout).
pub(crate) fn read_game_state(r: &mut Reader) -> Result<GameState, String> {
    {
        let _ = 0;
        let version = r.i32()?;
        if !(121..=124).contains(&version) {
            return Err(format!("unsupported save version {version} (parser follows the version 121-124 layouts)"));
        }
        let mut s = GameState::new(version, 0);
        s.current_command = r.u16()?;
        s.current_turn = r.u32()?;
        s.current_player_index = r.u8()?;
        s.current_unit_id = r.u32()?;
        let state = r.u8()?;
        s.current_state = match state {
            1 => GameStatus::Lobby,
            3 => GameStatus::FinalTurn,
            4 => GameStatus::Ended,
            _ => GameStatus::Started,
        };
        s.seed = r.i32()?;
        // GameSettings.
        s.settings.turn_limit = r.i32()?;
        s.settings.score_limit = r.i32()?;
        s.settings.win_by_capital = r.bool()?;
        let _mirror = r.bool()?;
        s.settings.allow_special_tribes = r.bool()?;
        s.settings.win_by_extermination = r.bool()?;
        s.settings.allow_tech_sharing = r.bool()?;
        s.settings.player_death_condition = i32::from(r.u16()?);
        s.settings.base_game_mode = i32::from(r.u8()?);
        s.settings.rules_game_mode = i32::from(r.u8()?);
        let _name = r.string()?;
        let _map_size = r.i32()?;
        for _ in 0..r.u16()? {
            r.u16()?;
        }
        for _ in 0..r.u16()? {
            r.u16()?;
        }
        s.settings.difficulty = i32::from(r.u16()?);
        s.settings.opponent_count = r.i32()?;
        s.settings.game_type = i32::from(r.u16()?);
        s.settings.map_preset = i32::from(r.u8()?);
        let _time_limit = r.i32()?;
        for _ in 0..3 {
            r.f32()?;
        }
        for _ in 0..4 {
            r.bool()?;
        }
        if version >= 0x56 {
            for _ in 0..r.i32()? {
                r.i16()?;
                r.i16()?;
            }
        }
        if version >= 122 {
            // Game settings (version >= 122): a string, empty in local saves, a GUID
            // in online games.
            let _extra = r.string()?;
        }
        // Map.
        let (w, h) = (r.u16()?, r.u16()?);
        let mut map = Map::new(w, h);
        let mut improvements: Vec<ImprovementRuntime> = Vec::new();
        let mut units: Vec<UnitState> = Vec::new();
        let mut fog: Vec<TileExplorers> = Vec::new();
        let mut city_names = std::collections::BTreeMap::new();
        for i in 0..usize::from(w) * usize::from(h) {
            let coords = r.wc()?;
            let t = &mut map.tiles[i];
            if t.coordinates != coords {
                return Err(format!("tile {i}: coordinates {coords:?} out of order"));
            }
            t.terrain = i32::from(r.u16()?);
            t.climate = i32::from(r.i16()?);
            t.altitude = i32::from(r.i16()?);
            t.owner = r.u8()?;
            t.capital_of = r.u8()?;
            t.is_capital = t.capital_of != 0;
            t.ruling_city = r.wc()?;
            if r.bool()? {
                t.resource = i32::from(r.u16()?);
            }
            if r.bool()? {
                let kind = i32::from(r.u16()?);
                t.improvement = kind;
                let mut rt = ImprovementRuntime { tile: t.index, owner: t.owner, ..ImprovementRuntime::default() };
                rt.level = r.u16()?;
                rt.founded = r.u16()?;
                rt.xp = r.i16()?;
                rt.population = r.i16()?;
                rt.production = r.u16()?;
                rt.base_score = r.u16()?;
                rt.border_size = r.u16()?;
                rt.upgrade = r.u16()?;
                rt.connected_to_capital_of_player = r.u8()?;
                if r.bool()? {
                    let name = r.string()?;
                    city_names.insert(t.index, name);
                }
                rt.founder = r.u8()?;
                rt.rewards = (0..r.u16()?).map(|_| r.u16().map(reward_from)).collect::<Result<_, _>>()?;
                // Improvement effect: decomposing = 0, robbed = 1.
                for _ in 0..r.u16()? {
                    match r.u16()? {
                        0 => rt.decomposing = true,
                        1 => rt.robbed = true,
                        _ => {}
                    }
                }
                improvements.push(rt);
            }
            if r.bool()? {
                units.push(read_unit(r, version)?);
            }
            let ne = r.u8()?;
            let mut ex = Vec::new();
            for _ in 0..ne {
                ex.push(r.u8()?);
            }
            fog.push(TileExplorers { tile: t.index, explorers: ex.into() });
            t.has_road = r.bool()?;
            t.has_route = r.bool()?;
            t.skin = r.i32()?;
            for _ in 0..r.u8()? {
                let e = r.i32()?;
                if (0..16).contains(&e) {
                    t.effects |= 1 << e;
                }
            }
            t.is_city = t.improvement == improvement::CITY;
            t.has_ruin = t.improvement == improvement::RUIN;
        }
        s.map = map;
        s.improvements = improvements;
        s.city_names = city_names;
        // Players.
        let n = r.u16()?;
        for _ in 0..n {
            let id = r.u8()?;
            let _name = r.string()?;
            let _account = r.string()?;
            let auto_play = r.bool()?;
            let start_tile = r.wc()?;
            let tribe = i32::from(r.u16()?);
            let chosen = r.bool()?;
            let handicap = r.i32()?;
            let currency = r.i32()?;
            let score = r.u32()?;
            let end_score = r.u32()?;
            let cities = r.u16()?;
            let tech: Vec<i32> = (0..r.u16()?).map(|_| r.u16().map(i32::from)).collect::<Result<_, _>>()?;
            let known: Vec<u8> = (0..r.u16()?).map(|_| r.u8()).collect::<Result<_, _>>()?;
            let mut tasks = Vec::new();
            for _ in 0..r.u16()? {
                let kind = r.u16()?;
                let started = r.bool()?;
                let done = r.bool()?;
                let val = if matches!(kind, 1 | 5 | 7 | 9) { r.i32()? } else { 0 };
                tasks.push(crate::PlayerTask {
                    kind: match kind {
                        1 => crate::TaskKind::Pacifist,
                        2 => crate::TaskKind::Genius,
                        3 => crate::TaskKind::Network,
                        4 => crate::TaskKind::Wealth,
                        5 => crate::TaskKind::Killer,
                        6 => crate::TaskKind::Metropolis,
                        7 => crate::TaskKind::Explorer,
                        8 => crate::TaskKind::ExploreLighthouses,
                        9 => crate::TaskKind::Convert,
                        _ => crate::TaskKind::None,
                    },
                    is_started: started,
                    is_completed: done,
                    converts: if kind == 9 { val } else { 0 },
                    explored_tiles: if kind == 7 { val } else { 0 },
                    kills: if kind == 5 { val } else { 0 },
                    turns: if kind == 1 { val } else { 0 },
                });
            }
            s.tasks.push(crate::PlayerTasks { player: id, tasks });
            let kills = r.u32()?;
            let casualties = r.u32()?;
            let wipe_outs = r.u32()?;
            let color = r.i32()?;
            let tribe_mix = r.u8()?;
            let unique: Vec<i32> = (0..r.u16()?).map(|_| r.i16().map(i32::from)).collect::<Result<_, _>>()?;
            let mut links = Vec::new();
            for _ in 0..r.u16()? {
                let other = r.u8()?;
                let st = r.u8()?;
                let last_attack = r.i32()?;
                let embassy = r.u8()?;
                let last_peace_broken = r.i32()?;
                let first_meet = r.i32()?;
                let embassy_build = r.i32()?;
                let prev = if version > 0x50 { r.i32()? } else { crate::DIPLOMACY_DEFAULT_TURN };
                links.push(DiplomacyLink {
                    other,
                    relation: DiplomacyRelation {
                        state: match st {
                            1 => DiplomacyRelationState::Peace,
                            2 => DiplomacyRelationState::War,
                            3 => DiplomacyRelationState::CoolDown,
                            _ => DiplomacyRelationState::Neutral,
                        },
                        last_attack_turn: last_attack,
                        embassy_level: i32::from(embassy),
                        last_peace_broken_turn: last_peace_broken,
                        first_meet,
                        embassy_build_turn: embassy_build,
                        previous_attack_turn: prev,
                    },
                });
            }
            let nmsg = r.u16()?;
            if nmsg != 0 && crate::env_var!("LOAD_DEBUG").is_ok() {
                eprintln!("player {} has {} diplomacy messages", id, nmsg);
            }
            let mut messages = Vec::new();
            for _ in 0..nmsg {
                // Diplomacy message: type byte, sender byte.
                let kind = r.u8()?;
                let sender = r.u8()?;
                messages.push((kind, sender));
            }
            let killer_id = r.u8()?;
            let killed_turn = r.u32()?;
            let resigned_at = r.i32()?;
            let wiped_at = r.i32()?;
            let skin = i32::from(r.u16()?);
            let mut resigned_turn = -1;
            let mut climate = tribe;
            // Player serialisation gates these on the version only (a local game's Nature has no colour)
            if version >= 0x5d {
                resigned_turn = r.i32()?;
            }
            if version >= 0x79 {
                climate = i32::from(r.u8()?);
            }
            if crate::env_var!("LOAD_DEBUG").is_ok() {
                eprintln!(
                    "player {} done at pos {} (tasks {:?})",
                    id,
                    r.p,
                    s.tasks.last().map(|t| t.tasks.iter().map(|x| x.kind as i32).collect::<Vec<_>>())
                );
            }
            s.players.push(PlayerState { id, handicap, currency, score, tribe, auto_play, start_tile });
            s.player_runtime.push(PlayerRuntime {
                player: id,
                tribe_mix: i32::from(tribe_mix),
                climate,
                has_chosen_tribe: chosen,
                skin,
                resigned_turn,
                resigned_at_command: resigned_at,
                wiped_at_command: wiped_at,
                aggressions: Vec::<Aggression>::new(),
                known_players: known,
                built_unique_improvements: unique,
                end_score,
                cities: i32::from(cities),
                kills,
                casualties,
                wipe_outs,
                killer_id,
                killed_turn,
                block_train_units: false,
                messages,
            });
            s.tech.push(OwnedTech { player: id, tech });
            s.diplomacy.push(PlayerDiplomacy { player: id, relations: links });
            let _ = color;
        }
        s.units = units;
        s.fog = fog;
        Ok(s)
    }
}

impl Engine {
    /// Build an engine from the bytes of a retail `.state` file.
    ///
    /// A fresh challenge file carries a start-match command whose payload is the post-start snapshot
    /// (starting techs granted, command counter 1); that snapshot is the state play continues from.
    pub fn from_save(bytes: &[u8]) -> Result<Engine, String> {
        if bytes.len() < 4 {
            return Err("file too short".into());
        }
        let data = lz4_block(&bytes[3..])?;
        let mut r = Reader { b: &data, p: 0 };
        let _file_version = r.i32()?;
        let mut s = read_game_state(&mut r)?;
        let triggers = r.u16()?;
        for _ in 0..triggers {
            r.u8()?;
            r.u16()?;
            r.wc()?;
            r.u8()?;
        }
        let commands = r.u16()?;
        if commands >= 1 && r.u16()? == 20 {
            r.take(4)?;
            s = read_game_state(&mut r)?;
        }
        Ok(Engine::from_state(s))
    }

    /// Wrap a deserialised state; tribes without techs get their starting techs.
    pub fn from_state(s: GameState) -> Engine {
        let version = s.version;
        let t = Engine::tables_for(version);
        let mut e = Engine::new(t, s);
        // Players start with the techs of their tribe when the save lists none (fresh challenge state).
        let ids: Vec<(u8, i32)> = e.s.players.iter().map(|p| (p.id, p.tribe)).collect();
        for (id, tribe) in ids {
            let empty = e.s.tech.iter().find(|t| t.player == id).is_none_or(|t| t.tech.is_empty());
            if empty {
                let start: Vec<i32> =
                    e.t.tribe(tribe)
                        .map(|t| t.starting_tech.iter().filter_map(|n| e.t.tech_idx.get(n).copied()).collect())
                        .unwrap_or_default();
                if let Some(row) = e.s.tech.iter_mut().find(|t| t.player == id) {
                    row.tech = start;
                }
                e.enable_start_tasks(id);
            }
        }
        e.refresh_scores();
        e
    }
}
