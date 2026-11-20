//! Position files: a whole game state as JSON, written by the map editor and read back into an [`Engine`].
//!
//! `to_json` writes every field of the state, so a retail save converted to a position and loaded again is the
//! same engine state (`examples/position` checks that). `from_json` also accepts hand-made positions where most
//! fields are left out; the defaults are documented on each reader below and in `ENGINE.md` (Position files).
//!
//! Layout: `{"format": "polytopia-position", "version": 1, "game": {...}, "players": [...], "tiles": [...],
//! "improvements": [...], "units": [...], "triggers": [...], "peace_requests": [...], "deferred": [...]}`; `"handover_held": true` is written when the end of the
//! mover's turn waits for a city reward the end raised (see [`Engine::end_turn`]).

use super::infiltrate::INFILTRATE_SPAWN_MAX;
use super::scout::SCOUT_PATH_MAX;
use super::*;
use crate::json::{obj, Json};
use crate::{
    Aggression, CityReward, DiplomacyLink, DiplomacyRelation, DiplomacyRelationState, GameStatus, Map, OwnedTech,
    PlayerDiplomacy, PlayerRuntime, PlayerTask, PlayerTasks, TaskKind, TileExplorers, DIPLOMACY_DEFAULT_TURN,
    HUMAN_HANDICAP,
};

pub const FORMAT: &str = "polytopia-position";
const NO_ID: u32 = u32::MAX;
const UNIT_FLAGS: [&str; 9] =
    ["frozen", "poisoned", "boosted", "invisible", "bubble", "petrified", "swift", "double_ready", "charmed"];

fn wc(c: Wc) -> Json {
    Json::Arr(vec![c.x.into(), c.y.into()])
}

fn deferred_json(d: &Deferred) -> Json {
    let (kind, at) = match *d {
        Deferred::CityLevelUp(c) => ("city_level_up", c),
        Deferred::ImprovementLevelUp(c) => ("improvement_level_up", c),
        Deferred::ImprovementLevelDown(c) => ("improvement_level_down", c),
        Deferred::Income(player, amount) => {
            return obj().set("kind", "income").set("player", player).set("amount", amount).build()
        }
        Deferred::PeaceRequest { to, from } => {
            return obj().set("kind", "peace_request").set("player", to).set("from", from).build()
        }
        Deferred::Score(player, amount) => {
            return obj().set("kind", "score").set("player", player).set("amount", amount).build()
        }
        Deferred::Population(c, amount) => {
            return obj().set("kind", "population").set("x", c.x).set("y", c.y).set("amount", amount).build()
        }
        Deferred::Scout(r) => {
            let path = Json::Arr(r.path[..r.len as usize].iter().map(|c| wc(*c)).collect());
            return obj()
                .set("kind", "scout")
                .set("player", r.player)
                .set("moves", r.moves)
                .set("seed", r.seed)
                .set("area", r.area)
                .set("path", path)
                .build();
        }
        Deferred::Explore(r) => {
            let path = Json::Arr(r.path[..r.len as usize].iter().map(|i| Json::from(i64::from(*i))).collect());
            return obj()
                .set("kind", "explore")
                .set("player", r.player)
                .set("sight", r.sight)
                .set("next", r.next)
                .set("area", r.area)
                .set("path", path)
                .build();
        }
        Deferred::Train { owner, unit_type, at, home, veteran } => {
            return obj()
                .set("kind", "train")
                .set("player", owner)
                .set("type", unit_type)
                .set("x", at.x)
                .set("y", at.y)
                .set("home_x", home.x)
                .set("home_y", home.y)
                .set("veteran", veteran)
                .build();
        }
        Deferred::Infiltrate(r) => {
            let tiles = Json::Arr(r.tiles[..r.len as usize].iter().map(|c| wc(*c)).collect());
            return obj()
                .set("kind", "infiltrate")
                .set("player", r.player)
                .set("x", r.city.x)
                .set("y", r.city.y)
                .set("land", r.land)
                .set("water", r.water.unwrap_or(-1))
                .set("next", r.next)
                .set("pay", r.pay)
                .set("tiles", tiles)
                .set("spores_x", r.spores.map_or(-1, |c| c.x))
                .set("spores_y", r.spores.map_or(-1, |c| c.y))
                .build();
        }
    };
    obj().set("kind", kind).set("x", at.x).set("y", at.y).build()
}

fn deferred_from_json(v: &Json) -> Option<Deferred> {
    let at = Wc { x: v.int("x", -1) as i32, y: v.int("y", -1) as i32 };
    Some(match v.get("kind")?.as_str()? {
        "city_level_up" => Deferred::CityLevelUp(at),
        "improvement_level_up" => Deferred::ImprovementLevelUp(at),
        "improvement_level_down" => Deferred::ImprovementLevelDown(at),
        "income" => Deferred::Income(v.int("player", 0) as u8, v.int("amount", 0) as i32),
        "peace_request" => Deferred::PeaceRequest { to: v.int("player", 0) as u8, from: v.int("from", 0) as u8 },
        "score" => Deferred::Score(v.int("player", 0) as u8, v.int("amount", 0) as i32),
        "population" => Deferred::Population(at, v.int("amount", 0) as i32),
        "scout" => {
            let tiles: Vec<Wc> = v
                .get("path")?
                .as_arr()
                .iter()
                .filter_map(|p| match p.as_arr() {
                    [x, y] => Some(Wc { x: x.as_i64()? as i32, y: y.as_i64()? as i32 }),
                    _ => None,
                })
                .take(SCOUT_PATH_MAX)
                .collect();
            let first = *tiles.first()?;
            let mut path = [first; SCOUT_PATH_MAX];
            path[..tiles.len()].copy_from_slice(&tiles);
            Deferred::Scout(ScoutRun {
                player: v.int("player", 0) as u8,
                moves: v.int("moves", 0) as u8,
                seed: v.int("seed", 0) as u32,
                len: tiles.len() as u8,
                area: v.int("area", 0) as u8,
                path,
            })
        }
        "explore" => {
            let tiles: Vec<u16> = v
                .get("path")?
                .as_arr()
                .iter()
                .filter_map(|p| p.as_i64().and_then(|i| u16::try_from(i).ok()))
                .take(EXPLORE_PATH_MAX)
                .collect();
            if tiles.is_empty() {
                return None;
            }
            let mut path = [0; EXPLORE_PATH_MAX];
            path[..tiles.len()].copy_from_slice(&tiles);
            Deferred::Explore(ExploreRun {
                player: v.int("player", 0) as u8,
                sight: v.int("sight", 0) as u8,
                len: tiles.len() as u8,
                next: (v.int("next", 0) as u8).min(tiles.len() as u8),
                area: v.int("area", 0) as u16,
                path,
            })
        }
        "train" => Deferred::Train {
            owner: v.int("player", 0) as u8,
            unit_type: v.int("type", 0) as i32,
            at,
            home: Wc { x: v.int("home_x", -1) as i32, y: v.int("home_y", -1) as i32 },
            veteran: v.flag("veteran", false),
        },
        "infiltrate" => {
            let tiles: Vec<Wc> = v
                .get("tiles")?
                .as_arr()
                .iter()
                .filter_map(|p| match p.as_arr() {
                    [x, y] => Some(Wc { x: x.as_i64()? as i32, y: y.as_i64()? as i32 }),
                    _ => None,
                })
                .take(INFILTRATE_SPAWN_MAX)
                .collect();
            let mut list = [at; INFILTRATE_SPAWN_MAX];
            list[..tiles.len()].copy_from_slice(&tiles);
            let water = v.int("water", -1) as i32;
            Deferred::Infiltrate(InfiltrateRun {
                player: v.int("player", 0) as u8,
                city: at,
                land: v.int("land", 0) as i32,
                water: (water >= 0).then_some(water),
                len: tiles.len() as u8,
                next: (v.int("next", 0) as u8).min(tiles.len() as u8),
                pay: v.int("pay", 0) as i32,
                tiles: list,
                spores: Some(Wc { x: v.int("spores_x", -1) as i32, y: v.int("spores_y", -1) as i32 })
                    .filter(|c| c.x >= 0 && c.y >= 0),
            })
        }
        _ => return None,
    })
}

fn unit_flags(u: &UnitState) -> [bool; 9] {
    [u.frozen, u.poisoned, u.boosted, u.invisible, u.bubble, u.petrified, u.swift, u.double_ready, u.charmed]
}

fn unit_json(u: &UnitState) -> Json {
    let flags: Vec<&str> = UNIT_FLAGS.iter().zip(unit_flags(u)).filter(|(_, on)| *on).map(|(n, _)| *n).collect();
    obj()
        .set("id", u.id)
        .set("type", u.type_idx)
        .set("owner", u.owner)
        .set("x", u.coordinates.x)
        .set("y", u.coordinates.y)
        .set("health", u.health)
        .set("xp", u.xp)
        .set("promotion", u.promotion)
        .set("moved", u.moved)
        .set("attacked", u.attacked)
        .set("home", wc(u.home))
        .set("previous", wc(u.previous_turn_end))
        .set("created", u.created_turn)
        .set("direction", u.direction)
        .set("birth_climate", u.birth_climate)
        .set_if(u.leader != 0, "leader", u.leader)
        .set_if(u.follower != 0, "follower", u.follower)
        .set_if(u.temporary_explorer, "temporary_explorer", true)
        .set_if(!flags.is_empty(), "flags", flags)
        .set_if(u.passenger.is_some(), "passenger", u.passenger.as_deref().map(unit_json).unwrap_or(Json::Null))
        .build()
}

impl Engine {
    /// The whole state as a position file.
    pub fn to_position(&self) -> Json {
        let s = &self.s;
        let st = &s.settings;
        let game = obj()
            .set("game_version", s.version)
            .set("seed", s.seed)
            .set("turn", s.current_turn)
            .set("current_player", s.current_player_id().unwrap_or(0))
            .set("command", s.current_command)
            .set("unit_id", s.current_unit_id)
            .set("status", s.current_state as u8)
            .set("width", s.map.width)
            .set("height", s.map.height)
            .set(
                "settings",
                obj()
                    .set("base_game_mode", st.base_game_mode)
                    .set("rules_game_mode", st.rules_game_mode)
                    .set("turn_limit", st.turn_limit)
                    .set("score_limit", st.score_limit)
                    .set("win_by_capital", st.win_by_capital)
                    .set("win_by_extermination", st.win_by_extermination)
                    .set("allow_special_tribes", st.allow_special_tribes)
                    .set("allow_tech_sharing", st.allow_tech_sharing)
                    .set("player_death_condition", st.player_death_condition)
                    .set("map_preset", st.map_preset)
                    .set("difficulty", st.difficulty)
                    .set("opponent_count", st.opponent_count)
                    .set("game_type", st.game_type)
                    .build(),
            )
            .build();

        let players: Vec<Json> = s
            .players
            .iter()
            .map(|p| {
                let rt = s.player_runtime.iter().find(|r| r.player == p.id).cloned().unwrap_or_default();
                let tech = s.tech.iter().find(|t| t.player == p.id).map(|t| t.tech.clone()).unwrap_or_default();
                let tasks: Vec<Json> = s
                    .tasks
                    .iter()
                    .find(|t| t.player == p.id)
                    .map(|t| {
                        t.tasks
                            .iter()
                            .map(|k| {
                                obj()
                                    .set("kind", k.kind as i32)
                                    .set("started", k.is_started)
                                    .set("completed", k.is_completed)
                                    .set("value", k.converts + k.explored_tiles + k.kills + k.turns)
                                    .build()
                            })
                            .collect()
                    })
                    .unwrap_or_default();
                let diplomacy: Vec<Json> = s
                    .diplomacy
                    .iter()
                    .find(|d| d.player == p.id)
                    .map(|d| {
                        d.relations
                            .iter()
                            .map(|l| {
                                let r = &l.relation;
                                obj()
                                    .set("other", l.other)
                                    .set("state", r.state as i32)
                                    .set("last_attack", r.last_attack_turn)
                                    .set("embassy", r.embassy_level)
                                    .set("peace_broken", r.last_peace_broken_turn)
                                    .set("first_meet", r.first_meet)
                                    .set("embassy_built", r.embassy_build_turn)
                                    .set("previous_attack", r.previous_attack_turn)
                                    .build()
                            })
                            .collect()
                    })
                    .unwrap_or_default();
                obj()
                    .set("id", p.id)
                    .set("tribe", p.tribe)
                    .set("human", !p.auto_play)
                    .set("handicap", p.handicap)
                    .set("stars", p.currency)
                    .set("score", p.score)
                    .set("start", wc(p.start_tile))
                    .set("tech", tech)
                    .set("known", rt.known_players.clone())
                    .set("tasks", Json::Arr(tasks))
                    .set("diplomacy", Json::Arr(diplomacy))
                    .set(
                        "messages",
                        Json::Arr(rt.messages.iter().map(|(k, f)| Json::Arr(vec![(*k).into(), (*f).into()])).collect()),
                    )
                    .set("unique", rt.built_unique_improvements.clone())
                    .set(
                        "aggressions",
                        Json::Arr(
                            rt.aggressions.iter().map(|a| Json::Arr(vec![a.other.into(), a.count.into()])).collect(),
                        ),
                    )
                    .set("climate", rt.climate)
                    .set("tribe_mix", rt.tribe_mix)
                    .set("chosen_tribe", rt.has_chosen_tribe)
                    .set("skin", rt.skin)
                    .set("cities", rt.cities)
                    .set("kills", rt.kills)
                    .set("casualties", rt.casualties)
                    .set("wipe_outs", rt.wipe_outs)
                    .set("killer", rt.killer_id)
                    .set("killed_turn", rt.killed_turn)
                    .set("resigned_turn", rt.resigned_turn)
                    .set("resigned_at_command", rt.resigned_at_command)
                    .set("wiped_at_command", rt.wiped_at_command)
                    .set("end_score", rt.end_score)
                    .set("block_train_units", rt.block_train_units)
                    .build()
            })
            .collect();

        let tiles: Vec<Json> = s
            .map
            .tiles
            .iter()
            .map(|t| {
                let explorers = s.fog.get(t.index as usize).map(|f| f.explorers.to_vec()).unwrap_or_default();
                let effects: Vec<i32> = (0..16).filter(|b| t.effects & (1 << b) != 0).collect();
                obj()
                    .set("x", t.coordinates.x)
                    .set("y", t.coordinates.y)
                    .set("terrain", t.terrain)
                    .set("climate", t.climate)
                    .set_if(t.skin != 0, "skin", t.skin)
                    .set_if(t.resource != 0, "resource", t.resource)
                    .set_if(t.improvement != 0, "improvement", t.improvement)
                    .set_if(t.owner != 0, "owner", t.owner)
                    .set_if(t.is_capital, "capital", true)
                    .set_if(t.capital_of != t.owner && t.capital_of != 0, "capital_of", t.capital_of)
                    .set("altitude", t.altitude)
                    .set("ruling", wc(t.ruling_city))
                    .set_if(t.has_road, "road", true)
                    .set_if(t.has_route, "route", true)
                    .set_if(!effects.is_empty(), "effects", effects)
                    .set("explored", explorers)
                    .build()
            })
            .collect();

        let improvements: Vec<Json> = s
            .improvements
            .iter()
            .map(|r| {
                let c = s.map.tiles.get(r.tile as usize).map(|t| t.coordinates).unwrap_or(Wc { x: -1, y: -1 });
                obj()
                    .set("x", c.x)
                    .set("y", c.y)
                    .set("owner", r.owner)
                    .set("founder", r.founder)
                    .set("level", r.level)
                    .set("founded", r.founded)
                    .set("xp", r.xp)
                    .set("population", r.population)
                    .set("production", r.production)
                    .set("base_score", r.base_score)
                    .set("border_size", r.border_size)
                    .set("upgrade", r.upgrade)
                    .set("connected", r.connected_to_capital_of_player)
                    .set("rewards", r.rewards.iter().map(|x| *x as i32).collect::<Vec<_>>())
                    .set_if(r.decomposing, "decomposing", true)
                    .set_if(r.robbed, "robbed", true)
                    .set_if(
                        s.city_names.contains_key(&r.tile),
                        "name",
                        s.city_names.get(&r.tile).cloned().unwrap_or_default(),
                    )
                    .build()
            })
            .collect();

        obj()
            .set("format", FORMAT)
            .set("version", 1)
            .set("game", game)
            .set("players", Json::Arr(players))
            .set("tiles", Json::Arr(tiles))
            .set("improvements", Json::Arr(improvements))
            .set("units", Json::Arr(s.units.iter().map(unit_json).collect()))
            .set(
                "triggers",
                Json::Arr(
                    self.triggers
                        .iter()
                        .map(|t| {
                            obj()
                                .set("player", t.player)
                                .set("x", t.city.x)
                                .set("y", t.city.y)
                                .set("peace_from", t.peace_from.map_or(-1, i32::from))
                                .build()
                        })
                        .collect(),
                ),
            )
            .set(
                "peace_requests",
                Json::Arr(self.peace_requests.iter().map(|(a, b)| Json::Arr(vec![(*a).into(), (*b).into()])).collect()),
            )
            .set("deferred", Json::Arr(self.deferred.iter().map(deferred_json).collect()))
            .set_if(self.handover_held, "handover_held", true)
            .build()
    }

    /// Build an engine from a position file. Missing fields get defaults:
    /// * game: version 123, turn 1, the first player to move, Domination settings;
    /// * players: id = seat number, AI unless `human`, starting techs of the tribe, 5 stars, capital = their
    ///   capital city tile, everyone known to everyone, neutral relations met on turn 0;
    /// * tiles: altitude 1 on land, -1 on water; `ruling` = the nearest city of the tile's owner within its
    ///   border; `explored` = every player (no fog); `route` recomputed from roads, ports and cities;
    /// * improvements: one per city or building tile; a city defaults to level 1 with border 1 and production
    ///   1 + workshop + park, buildings start at the level their neighbours give;
    /// * units: next free id, full health, home = the ruling city of their tile, ready to move.
    pub fn from_position(v: &Json) -> Result<Engine, String> {
        if v.get("format").and_then(Json::as_str).is_some_and(|f| f != FORMAT) {
            return Err("not a polytopia-position file".into());
        }
        let game = v.get("game").cloned().unwrap_or(Json::Obj(vec![]));
        let version = game.int("game_version", 123) as i32;
        let tiles_in = v.get("tiles").map(Json::as_arr).unwrap_or(&[]);
        let width = game.int("width", -1);
        let width = if width > 0 {
            width
        } else {
            // a bare editor map: the size is the largest coordinate + 1, or `size`
            v.int("size", tiles_in.iter().map(|t| t.int("x", 0).max(t.int("y", 0)) + 1).max().unwrap_or(0))
        } as u16;
        let height = game.int("height", i64::from(width)) as u16;
        if width == 0 || height == 0 {
            return Err("position has no tiles".into());
        }
        let mut s = GameState::new(version, game.int("seed", 0) as i32);
        s.map = Map::new(width, height);
        s.current_turn = game.int("turn", 1) as u32;
        s.current_command = game.int("command", 0) as u16;
        s.current_state = match game.int("status", 2) {
            1 => GameStatus::Lobby,
            3 => GameStatus::FinalTurn,
            4 => GameStatus::Ended,
            _ => GameStatus::Started,
        };
        if let Some(st) = game.get("settings") {
            let d = crate::GameSettings::default();
            let x = &mut s.settings;
            x.base_game_mode = st.int("base_game_mode", i64::from(d.base_game_mode)) as i32;
            x.rules_game_mode = st.int("rules_game_mode", i64::from(x.base_game_mode)) as i32;
            x.turn_limit = st.int("turn_limit", 0) as i32;
            x.score_limit = st.int("score_limit", 0) as i32;
            // A Might position that does not list the rule flags gets the Might preset's.
            let might = x.rules_game_mode == crate::GAME_MODE_MIGHT;
            x.win_by_capital = st.flag("win_by_capital", d.win_by_capital || might);
            x.win_by_extermination = st.flag("win_by_extermination", d.win_by_extermination || might);
            x.allow_special_tribes = st.flag("allow_special_tribes", d.allow_special_tribes);
            x.allow_tech_sharing = st.flag("allow_tech_sharing", d.allow_tech_sharing);
            x.player_death_condition = st.int("player_death_condition", i64::from(d.player_death_condition)) as i32;
            x.map_preset = st.int("map_preset", 0) as i32;
            x.difficulty = st.int("difficulty", i64::from(d.difficulty)) as i32;
            x.opponent_count = st.int("opponent_count", 0) as i32;
            x.game_type = st.int("game_type", 0) as i32;
        }

        // ---- tiles ----
        let mut have_ruling = vec![false; s.map.tiles.len()];
        let mut have_route = false;
        let mut have_fog = false;
        s.fog =
            s.map.tiles.iter().map(|t| TileExplorers { tile: t.index, explorers: crate::Explorers::new() }).collect();
        for tj in tiles_in {
            let (x, y) = (tj.int("x", -1) as i32, tj.int("y", -1) as i32);
            if x < 0 || y < 0 || x >= i32::from(width) || y >= i32::from(height) {
                return Err(format!("tile ({x},{y}) is outside the {width}x{height} map"));
            }
            let i = (y * i32::from(width) + x) as usize;
            let t = &mut s.map.tiles[i];
            t.terrain = tj.int("terrain", 0) as i32;
            t.climate = tj.int("climate", 1) as i32;
            t.skin = tj.int("skin", 0) as i32;
            t.resource = tj.int("resource", 0) as i32;
            t.improvement = tj.int("improvement", 0) as i32;
            if tj.flag("city", false) {
                t.improvement = improvement::CITY;
            }
            t.owner = tj.int("owner", 0) as u8;
            t.is_capital = tj.flag("capital", false);
            t.capital_of = if t.is_capital { tj.int("capital_of", i64::from(t.owner)) as u8 } else { 0 };
            let land = !crate::rules::path::is_water(t.terrain) && t.terrain != 0;
            t.altitude = tj.int("altitude", if land { 1 } else { -1 }) as i32;
            if let Some((rx, ry)) = tj.pair("ruling") {
                t.ruling_city = Wc { x: rx, y: ry };
                have_ruling[i] = true;
            }
            t.has_road = tj.flag("road", false);
            if let Some(r) = tj.get("route") {
                t.has_route = r.as_bool().unwrap_or(false);
                have_route = true;
            }
            for e in tj.get("effects").map(Json::as_arr).unwrap_or(&[]) {
                if let Some(b) = e.as_i64().filter(|b| (0..16).contains(b)) {
                    t.effects |= 1 << b;
                }
            }
            t.is_city = t.improvement == improvement::CITY;
            t.has_ruin = t.improvement == improvement::RUIN;
            if let Some(ex) = tj.get("explored") {
                have_fog = true;
                s.fog[i].explorers = ex.as_arr().iter().filter_map(|p| p.as_i64().map(|p| p as u8)).collect();
            }
        }

        // ---- players ----
        let players_in = v.get("players").map(Json::as_arr).unwrap_or(&[]);
        let ids: Vec<u8> = players_in.iter().enumerate().map(|(k, p)| p.int("id", k as i64 + 1) as u8).collect();
        for (k, pj) in players_in.iter().enumerate() {
            let id = ids[k];
            let tribe = pj.int("tribe", 2) as i32;
            let start = pj.pair("start").map(|(x, y)| Wc { x, y }).unwrap_or_else(|| {
                s.map
                    .tiles
                    .iter()
                    .find(|t| t.owner == id && t.is_capital && t.is_city)
                    .map(|t| t.coordinates)
                    .unwrap_or(Wc { x: -1, y: -1 })
            });
            s.players.push(PlayerState {
                id,
                handicap: pj.int("handicap", if pj.flag("human", false) { HUMAN_HANDICAP as i64 } else { 0 }) as i32,
                currency: pj.int("stars", 5) as i32,
                score: pj.int("score", 0) as u32,
                tribe,
                auto_play: !pj.flag("human", false),
                start_tile: start,
            });
            let known: Vec<u8> = match pj.get("known") {
                Some(k) => k.as_arr().iter().filter_map(|p| p.as_i64().map(|p| p as u8)).collect(),
                None => ids.iter().copied().filter(|o| *o != id).collect(),
            };
            let pairs = |key: &str| -> Vec<(i64, i64)> {
                pj.get(key)
                    .map(Json::as_arr)
                    .unwrap_or(&[])
                    .iter()
                    .filter_map(|m| Some((m.as_arr().first()?.as_i64()?, m.as_arr().get(1)?.as_i64()?)))
                    .collect()
            };
            s.player_runtime.push(PlayerRuntime {
                player: id,
                tribe_mix: pj.int("tribe_mix", 0) as i32,
                climate: pj.int("climate", i64::from(tribe)) as i32,
                has_chosen_tribe: pj.flag("chosen_tribe", true),
                skin: pj.int("skin", 0) as i32,
                resigned_turn: pj.int("resigned_turn", -1) as i32,
                resigned_at_command: pj.int("resigned_at_command", -1) as i32,
                wiped_at_command: pj.int("wiped_at_command", -1) as i32,
                aggressions: pairs("aggressions")
                    .into_iter()
                    .map(|(o, c)| Aggression { other: o as u8, count: c as i32 })
                    .collect(),
                known_players: known.clone(),
                built_unique_improvements: pj
                    .get("unique")
                    .map(Json::as_arr)
                    .unwrap_or(&[])
                    .iter()
                    .filter_map(|x| x.as_i64().map(|x| x as i32))
                    .collect(),
                end_score: pj.int("end_score", 0) as u32,
                cities: pj.int("cities", -1) as i32,
                kills: pj.int("kills", 0) as u32,
                casualties: pj.int("casualties", 0) as u32,
                wipe_outs: pj.int("wipe_outs", 0) as u32,
                killer_id: pj.int("killer", 0) as u8,
                killed_turn: pj.int("killed_turn", 0) as u32,
                block_train_units: pj.flag("block_train_units", false),
                messages: pairs("messages").into_iter().map(|(a, b)| (a as u8, b as u8)).collect(),
            });
            let tech: Vec<i32> = pj
                .get("tech")
                .map(Json::as_arr)
                .unwrap_or(&[])
                .iter()
                .filter_map(|x| x.as_i64().map(|x| x as i32))
                .collect();
            s.tech.push(OwnedTech { player: id, tech });
            let tasks = pj
                .get("tasks")
                .map(Json::as_arr)
                .unwrap_or(&[])
                .iter()
                .map(|t| {
                    let kind = t.int("kind", 0);
                    let val = t.int("value", 0) as i32;
                    PlayerTask {
                        kind: task_kind(kind),
                        is_started: t.flag("started", false),
                        is_completed: t.flag("completed", false),
                        converts: if kind == 9 { val } else { 0 },
                        explored_tiles: if kind == 7 { val } else { 0 },
                        kills: if kind == 5 { val } else { 0 },
                        turns: if kind == 1 { val } else { 0 },
                    }
                })
                .collect();
            s.tasks.push(PlayerTasks { player: id, tasks });
            let relations = match pj.get("diplomacy") {
                Some(d) => d
                    .as_arr()
                    .iter()
                    .map(|l| DiplomacyLink {
                        other: l.int("other", 0) as u8,
                        relation: DiplomacyRelation {
                            state: match l.int("state", 0) {
                                1 => DiplomacyRelationState::Peace,
                                2 => DiplomacyRelationState::War,
                                3 => DiplomacyRelationState::CoolDown,
                                _ => DiplomacyRelationState::Neutral,
                            },
                            last_attack_turn: l.int("last_attack", i64::from(DIPLOMACY_DEFAULT_TURN)) as i32,
                            embassy_level: l.int("embassy", 0) as i32,
                            last_peace_broken_turn: l.int("peace_broken", i64::from(DIPLOMACY_DEFAULT_TURN)) as i32,
                            first_meet: l.int("first_meet", 0) as i32,
                            embassy_build_turn: l.int("embassy_built", i64::from(DIPLOMACY_DEFAULT_TURN)) as i32,
                            previous_attack_turn: l.int("previous_attack", i64::from(DIPLOMACY_DEFAULT_TURN)) as i32,
                        },
                    })
                    .collect(),
                None => known
                    .iter()
                    .map(|o| DiplomacyLink {
                        other: *o,
                        relation: DiplomacyRelation { first_meet: 0, ..DiplomacyRelation::default() },
                    })
                    .collect(),
            };
            s.diplomacy.push(PlayerDiplomacy { player: id, relations });
        }
        let current = game.int("current_player", i64::from(ids.first().copied().unwrap_or(1))) as u8;
        s.current_player_index = s.players.iter().position(|p| p.id == current).unwrap_or(0) as u8;
        if !have_fog {
            for f in &mut s.fog {
                f.explorers = ids.as_slice().into();
            }
        }

        // ---- improvements (runtime state of cities and buildings) ----
        let mut given = std::collections::BTreeSet::new();
        for r in v.get("improvements").map(Json::as_arr).unwrap_or(&[]) {
            let (x, y) = (r.int("x", -1) as i32, r.int("y", -1) as i32);
            if x < 0 || y < 0 || x >= i32::from(width) || y >= i32::from(height) {
                continue;
            }
            let idx = y * i32::from(width) + x;
            let tile = &s.map.tiles[idx as usize];
            let rewards: Vec<CityReward> = r
                .get("rewards")
                .map(Json::as_arr)
                .unwrap_or(&[])
                .iter()
                .filter_map(|x| x.as_i64())
                .map(|x| load::reward_from(x as u16))
                .collect();
            let is_city = tile.improvement == improvement::CITY;
            let level = r.int("level", 1) as u16;
            let production = 1 + rewards
                .iter()
                .filter(|w| matches!(w, CityReward::Workshop) || (version >= 99 && matches!(w, CityReward::Park)))
                .count();
            let xp = r.int("xp", 0) as i16;
            s.improvements.push(ImprovementRuntime {
                tile: idx,
                owner: r.int("owner", i64::from(tile.owner)) as u8,
                founder: r.int("founder", i64::from(tile.owner)) as u8,
                level,
                founded: r.int("founded", 0) as u16,
                xp,
                population: r.int("population", i64::from(xp)) as i16,
                production: r.int("production", production as i64) as u16,
                base_score: r.int(
                    "base_score",
                    if is_city {
                        250 * rewards.iter().filter(|w| matches!(w, CityReward::Park)).count() as i64
                    } else {
                        0
                    },
                ) as u16,
                border_size: r.int(
                    "border_size",
                    if is_city {
                        1 + rewards.iter().filter(|w| matches!(w, CityReward::BorderGrowth)).count() as i64
                    } else {
                        0
                    },
                ) as u16,
                upgrade: r.int("upgrade", 0) as u16,
                connected_to_capital_of_player: r.int("connected", 0) as u8,
                rewards,
                decomposing: r.flag("decomposing", false),
                robbed: r.flag("robbed", false),
            });
            if let Some(n) = r.get("name").and_then(Json::as_str) {
                s.city_names.insert(idx, n.to_string());
            }
            given.insert(idx);
        }

        // ---- units ----
        let mut units: Vec<UnitState> = Vec::new();
        for uj in v.get("units").map(Json::as_arr).unwrap_or(&[]) {
            units.push(read_unit(uj, &s)?);
        }
        // units without an id (NO_ID; retail does use id 0) get the next free ones
        let mut next = units
            .iter()
            .flat_map(|u| std::iter::once(u.id).chain(u.passenger.as_ref().map(|p| p.id)))
            .filter(|i| *i != NO_ID)
            .max()
            .unwrap_or(0);
        for u in &mut units {
            if u.id == NO_ID {
                next += 1;
                u.id = next;
            }
            if let Some(p) = u.passenger.as_mut() {
                if p.id == NO_ID {
                    next += 1;
                    p.id = next;
                }
            }
        }
        s.units = units;
        s.current_unit_id = match game.get("unit_id").and_then(Json::as_i64) {
            Some(n) => n.max(i64::from(next)) as u32,
            None => next,
        };

        let mut e = Engine::from_state(s);
        // derived state for hand-made positions
        let missing: Vec<(Wc, i32)> =
            e.s.map
                .tiles
                .iter()
                .filter(|t| t.improvement != 0 && !given.contains(&t.index))
                .map(|t| (t.coordinates, t.improvement))
                .collect();
        for (c, kind) in missing {
            e.default_runtime(c, kind);
        }
        e.fill_ruling(&have_ruling);
        let no_home: Vec<bool> =
            v.get("units").map(Json::as_arr).unwrap_or(&[]).iter().map(|u| u.get("home").is_none()).collect();
        for u in 0..e.s.units.len() {
            if no_home.get(u).copied().unwrap_or(false) {
                let c = e.s.units[u].coordinates;
                e.s.units[u].home = e.tile(c).map(|t| t.ruling_city).unwrap_or(Wc { x: -1, y: -1 });
            }
        }
        for k in 0..e.s.player_runtime.len() {
            if e.s.player_runtime[k].cities < 0 {
                let id = e.s.player_runtime[k].player;
                e.s.player_runtime[k].cities =
                    e.s.map.tiles.iter().filter(|t| t.is_city && t.owner == id).count() as i32;
            }
        }
        if !have_route {
            e.update_routes();
        }

        e.triggers = v
            .get("triggers")
            .map(Json::as_arr)
            .unwrap_or(&[])
            .iter()
            .map(|t| Trigger {
                player: t.int("player", 0) as u8,
                city: Wc { x: t.int("x", -1) as i32, y: t.int("y", -1) as i32 },
                peace_from: u8::try_from(t.int("peace_from", -1)).ok(),
            })
            .collect();
        e.peace_requests = v
            .get("peace_requests")
            .map(Json::as_arr)
            .unwrap_or(&[])
            .iter()
            .filter_map(|p| Some((p.as_arr().first()?.as_i64()? as u8, p.as_arr().get(1)?.as_i64()? as u8)))
            .collect();
        e.deferred = v.get("deferred").map(Json::as_arr).unwrap_or(&[]).iter().filter_map(deferred_from_json).collect();
        e.handover_held = v.get("handover_held").and_then(Json::as_bool).unwrap_or(false);
        if game.get("turn").is_none() || players_in.iter().any(|p| p.get("score").is_none()) {
            e.refresh_scores();
        }
        Ok(e)
    }

    /// Runtime state for an improvement placed without one (hand-made positions).
    fn default_runtime(&mut self, at: Wc, kind: i32) {
        let Some(t) = self.tile(at) else { return };
        let (idx, owner) = (t.index, t.owner);
        if kind == improvement::CITY {
            self.s.improvements.push(ImprovementRuntime {
                tile: idx,
                owner,
                founder: owner,
                level: 1,
                production: 1,
                border_size: 1,
                ..ImprovementRuntime::default()
            });
            return;
        }
        if owner == 0 {
            // What retail's map generation writes for a neutral ruin or corner lighthouse: level 1, no score or border,
            // and the lighthouse without production.
            self.s.improvements.push(ImprovementRuntime {
                tile: idx,
                level: 1,
                production: u16::from(kind != crate::rules::economy::LIGHTHOUSE_IMPROVEMENT),
                ..ImprovementRuntime::default()
            });
            return;
        }
        let tables = self.t.clone();
        let Some(info) = tables.improvement(kind) else { return };
        let base_score: i32 = info.data.rewards.iter().map(|r| r.score_or_default().max(0)).sum();
        self.s.improvements.push(ImprovementRuntime {
            tile: idx,
            owner,
            founder: owner,
            production: 1,
            base_score: base_score as u16,
            border_size: info.data.border_size.unwrap_or(0) as u16,
            ..ImprovementRuntime::default()
        });
        let level = self.improvement_level(at);
        if let Some(rt) = self.improvement_rt_mut(at) {
            rt.level = level.max(0) as u16;
        }
    }

    /// `ruling` for tiles that did not give one: the nearest city of the tile's owner whose border reaches it.
    #[allow(clippy::needless_range_loop)] // the index also addresses `self.s.map.tiles`, which is borrowed mutably below
    fn fill_ruling(&mut self, have: &[bool]) {
        let cities: Vec<(Wc, u8, i32)> = self
            .s
            .map
            .tiles
            .iter()
            .filter(|t| t.is_city)
            .map(|t| {
                (
                    t.coordinates,
                    t.owner,
                    self.improvement_rt(t.coordinates).map(|r| i32::from(r.border_size)).unwrap_or(1),
                )
            })
            .collect();
        for i in 0..self.s.map.tiles.len() {
            if have[i] {
                continue;
            }
            let t = &self.s.map.tiles[i];
            if t.owner == 0 {
                continue;
            }
            let best = cities
                .iter()
                .filter(|(c, o, r)| *o == t.owner && Engine::chebyshev(*c, t.coordinates) <= *r)
                .min_by_key(|(c, _, _)| Engine::chebyshev(*c, t.coordinates))
                .map(|(c, _, _)| *c);
            if let Some(c) = best {
                self.s.map.tiles[i].ruling_city = c;
            }
        }
    }
}

fn task_kind(k: i64) -> TaskKind {
    match k {
        1 => TaskKind::Pacifist,
        2 => TaskKind::Genius,
        3 => TaskKind::Network,
        4 => TaskKind::Wealth,
        5 => TaskKind::Killer,
        6 => TaskKind::Metropolis,
        7 => TaskKind::Explorer,
        8 => TaskKind::ExploreLighthouses,
        9 => TaskKind::Convert,
        _ => TaskKind::None,
    }
}

fn read_unit(uj: &Json, s: &GameState) -> Result<UnitState, String> {
    let at = Wc { x: uj.int("x", -1) as i32, y: uj.int("y", -1) as i32 };
    let type_idx = uj.int("type", 2) as i32;
    let tables = Engine::tables_for(s.version);
    let full = tables.unit(type_idx).map(|u| u.data.health).unwrap_or(100);
    let flags: Vec<&str> = uj.get("flags").map(Json::as_arr).unwrap_or(&[]).iter().filter_map(Json::as_str).collect();
    let has = |n: &str| flags.contains(&n);
    let passenger = match uj.get("passenger") {
        Some(p) if !p.is_null() => {
            let mut p = p.clone();
            if let Json::Obj(kv) = &mut p {
                // a passenger keeps the coordinates it had when it embarked; without them it is on the carrier's tile
                if !kv.iter().any(|(k, _)| k == "x") {
                    kv.push(("x".into(), at.x.into()));
                    kv.push(("y".into(), at.y.into()));
                }
            }
            Some(Box::new(read_unit(&p, s)?))
        }
        _ => None,
    };
    let (w, h) = (i32::from(s.map.width), i32::from(s.map.height));
    Ok(UnitState {
        id: uj.get("id").and_then(Json::as_i64).map(|i| i as u32).unwrap_or(NO_ID),
        leader: uj.int("leader", 0) as u32,
        follower: uj.int("follower", 0) as u32,
        owner: uj.int("owner", 1) as u8,
        birth_climate: uj.int(
            "birth_climate",
            s.map.tiles.get((at.y * w + at.x).max(0) as usize).map(|t| i64::from(t.climate)).unwrap_or(0),
        ) as i32,
        type_idx,
        previous_turn_end: uj.pair("previous").map(|(x, y)| Wc { x, y }).unwrap_or(at),
        coordinates: at,
        home: uj.pair("home").map(|(x, y)| Wc { x, y }).unwrap_or(Wc { x: -1, y: -1 }),
        passenger,
        health: uj.int("health", i64::from(full)) as u16,
        promotion: uj.int("promotion", 0) as u16,
        xp: uj.int("xp", 0) as u16,
        moved: uj.flag("moved", false),
        attacked: uj.flag("attacked", false),
        created_turn: uj.int("created", 0) as u16,
        frozen: has("frozen"),
        poisoned: has("poisoned"),
        boosted: has("boosted"),
        invisible: has("invisible"),
        bubble: has("bubble"),
        petrified: has("petrified"),
        swift: has("swift"),
        double_ready: has("double_ready"),
        charmed: has("charmed"),
        temporary_explorer: uj.flag("temporary_explorer", false),
        direction: uj.int("direction", i64::from(Engine::direction_from_vector(w / 2 - at.x, h / 2 - at.y))) as u8,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A position as the map editor writes one by hand: only the map, the players and the units.
    fn hand_made() -> Json {
        let mut tiles = Vec::new();
        for y in 0..6 {
            for x in 0..6 {
                let mut t = vec![
                    ("x".to_string(), Json::from(x)),
                    ("y".to_string(), Json::from(y)),
                    ("terrain".to_string(), Json::from(if x == 5 { 1 } else { 3 })),
                    ("climate".to_string(), Json::from(2)),
                ];
                if (x, y) == (2, 2) {
                    t.push(("city".into(), true.into()));
                    t.push(("capital".into(), true.into()));
                }
                if (1..=3).contains(&x) && (1..=3).contains(&y) {
                    t.push(("owner".into(), 1.into()));
                }
                tiles.push(Json::Obj(t));
            }
        }
        let text = format!(
            r#"{{"format":"polytopia-position","game":{{"turn":2,"width":6,"height":6}},
               "players":[{{"id":1,"tribe":2,"human":true,"stars":7}},{{"id":2,"tribe":3}}],
               "tiles":{},
               "units":[{{"type":2,"owner":1,"x":2,"y":2}},{{"type":43,"owner":1,"x":5,"y":3,"passenger":{{"type":9,"owner":1}}}}]}}"#,
            Json::Arr(tiles)
        );
        Json::parse(&text).unwrap()
    }

    #[test]
    fn hand_made_positions_get_their_defaults() {
        let e = Engine::from_position(&hand_made()).unwrap();
        assert_eq!(e.s.current_turn, 2);
        assert_eq!(e.player(1).unwrap().currency, 7);
        assert_eq!(e.player(2).unwrap().currency, 5);
        assert!(!e.player(1).unwrap().auto_play && e.player(2).unwrap().auto_play);
        // the city got a runtime record, units got ids, full health and the city as home
        let city = e.improvement_rt(Wc { x: 2, y: 2 }).unwrap();
        assert_eq!((city.level, city.border_size, city.owner), (1, 1, 1));
        let w = e.s.units.iter().find(|u| u.type_idx == 2).unwrap();
        assert_eq!((w.health, w.home), (100, Wc { x: 2, y: 2 }));
        let boat = e.s.units.iter().find(|u| u.type_idx == 43).unwrap();
        assert_eq!(boat.passenger.as_ref().unwrap().coordinates, Wc { x: 5, y: 3 });
        assert_ne!(boat.id, w.id);
        // the tribes' starting techs
        assert!(!e.s.tech.iter().find(|t| t.player == 1).unwrap().tech.is_empty());
    }

    #[test]
    fn positions_round_trip() {
        let e = Engine::from_position(&hand_made()).unwrap();
        let back = Engine::from_position(&Json::parse(&e.to_position().to_string()).unwrap()).unwrap();
        assert_eq!(format!("{:?}", e.s), format!("{:?}", back.s));
        assert_eq!(e.clone().legal_commands(), back.clone().legal_commands());
    }
}
