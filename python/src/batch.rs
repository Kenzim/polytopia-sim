//! `polytopia_sim.engine_fast`: many games at once behind numpy arrays, with the interface of polybots-engine's
//! `engine_fast` package (`Batch`, its methods and constants), so code written against that package runs on this
//! engine by changing the import. Only the CPU backend exists; the GPU-only methods raise `ValueError`.

use engine::rules::diplomacy::get_relation;
use engine::{
    BotDifficulty, CityReward, Command, Engine, GameOptions, MapPreset, MapSize, PlayerSetup, UnitState, NATURE,
};
use engine::{GAME_MODE_DOMINATION, GAME_MODE_GLORY, GAME_MODE_MIGHT, GAME_MODE_PERFECTION};
use numpy::{PyArray1, PyArrayMethods};
use pyo3::exceptions::PyValueError;
use pyo3::prelude::*;
use pyo3::types::PyDict;
use std::sync::{Mutex, MutexGuard};

type Wc = engine::WorldCoordinates;

/// The game version whose rules a batch plays.
const RULES: i32 = 124;

const MOVE_FIELDS: [&str; 6] = ["kind", "source", "target", "option", "opponent", "price"];
const PREVIEW_FIELDS: [&str; 4] = ["present", "damage_dealt", "damage_taken", "flags"];
const SCALAR_FIELDS: [&str; 3] = ["viewer", "turn", "stars"];
const PLAYER_FIELDS: [&str; 4] = ["seat", "tribe", "relation", "score"];
const STATUS: [&str; 6] = ["empty", "playing", "ended", "truncated", "refused", "failed"];
const APPLIED: [&str; 3] = ["skipped", "taken", "rejected"];
const OUTPUTS: [&str; 4] = ["previews", "defence", "income", "max_health"];
/// The outputs polybots-engine computes only on its cpu backend.
const CPU_OUTPUTS: [&str; 2] = ["income", "max_health"];
const DEVICE_ARRAYS: [&str; 9] =
    ["status", "winner", "counts", "applied", "moves", "tiles", "scalars", "players", "tech"];
const SKIP: i32 = -1;
const TILE_PLANES: [&str; 29] = [
    "explored",
    "terrain",
    "climate",
    "resource",
    "improvement",
    "improvement_level",
    "owner",
    "capital_of",
    "ruling_city",
    "road",
    "route",
    "effects",
    "ruin_hint",
    "unit_kind",
    "unit_owner",
    "unit_health",
    "unit_veteran",
    "unit_effects",
    "unit_passenger",
    "unit_moved",
    "unit_attacked",
    "unit_kills",
    "unit_home",
    "city_population",
    "city_connected",
    "city_progress",
    "city_wall",
    "city_parks",
    "city_super_units",
];
/// The game's command type numbers, as `KINDS` names them.
const KINDS: [(&str, i32); 23] = [
    ("Build", 1),
    ("Attack", 2),
    ("Recover", 3),
    ("HealOthers", 4),
    ("Train", 5),
    ("Move", 6),
    ("Capture", 7),
    ("Research", 8),
    ("Destroy", 9),
    ("Disband", 10),
    ("CityReward", 11),
    ("Promote", 13),
    ("ExamineRuins", 14),
    ("EndTurn", 15),
    ("Upgrade", 16),
    ("EndMatch", 22),
    ("Decompose", 26),
    ("PeaceTreaty", 27),
    ("PeaceRequestResponse", 28),
    ("BreakPeace", 29),
    ("EstablishEmbassy", 30),
    ("Resign", 35),
    ("Disembark", 36),
];
const RESIGN: i32 = 35;
const MAX_SEATS: usize = 4;

const EMPTY: u8 = 0;
const PLAYING: u8 = 1;
const ENDED: u8 = 2;
const TRUNCATED: u8 = 3;
const REFUSED: u8 = 4;
const FAILED: u8 = 5;

const CITY: i32 = 1;
const RUIN: i32 = 2;
const KILLS_TARGET: i16 = 1;
const ATTACKER_DIES: i16 = 2;
const RETALIATION: i16 = 4;

type Row = [i32; 6];

/// One game of the batch, with the moves offered to its player to move.
#[derive(Default)]
struct Slot {
    game: Option<Engine>,
    status: u8,
    winner: i32,
    reason: String,
    decisions: u32,
    action_limit: u32,
    /// The legal rows, sorted, and the command each stands for (`None`: Resign).
    rows: Vec<Row>,
    commands: Vec<Option<Command>>,
}

#[derive(Clone)]
struct Setup {
    seed: i32,
    side: u16,
    tribes: Vec<i32>,
    map: i32,
    mode: i32,
    action_limit: i64,
}

fn tile_index(e: &Engine, c: Wc) -> i32 {
    e.tile_index(c).map_or(-1, |i| i as i32)
}

/// The seat that wins when `loser` resigns a two-seat game.
fn other_seat(e: &Engine, loser: u8) -> i32 {
    e.s.players.iter().map(|p| p.id).find(|&id| id != loser && id != NATURE).map_or(0, i32::from)
}

/// The board side below the game's smallest that polybots-engine also plays (its "nano"); the game's generator runs
/// unchanged at that side.
const NANO: usize = 8;

/// The board sides a batch may have: 11 to 30 (the game's sizes and the sides between them) and [`NANO`].
fn side_supported(side: usize) -> bool {
    (11..=30).contains(&side) || side == NANO
}

/// The game's map size of this side, where there is one.
fn map_size(side: u16) -> Option<MapSize> {
    [MapSize::Tiny, MapSize::Small, MapSize::Normal, MapSize::Large, MapSize::Huge, MapSize::Massive]
        .into_iter()
        .find(|s| s.width() == side)
}

fn preset(map: i32) -> Option<MapPreset> {
    Some(match map {
        1 => MapPreset::Dryland,
        2 => MapPreset::Lakes,
        3 => MapPreset::Continents,
        4 => MapPreset::Archipelago,
        5 => MapPreset::WaterWorld,
        6 => MapPreset::Pangea,
        _ => return None,
    })
}

/// A new game for `setup`, or why it is outside the environment's scope.
fn new_game(setup: &Setup) -> Result<Engine, String> {
    let scope = || "configuration outside the environment's scope".to_string();
    if !(2..=MAX_SEATS).contains(&setup.tribes.len())
        || setup.tribes.iter().any(|t| !(2..=17).contains(t))
        || !(1..=10_000).contains(&setup.action_limit)
    {
        return Err(scope());
    }
    let preset = preset(setup.map).ok_or_else(scope)?;
    let (turn_limit, score_limit) = match setup.mode {
        GAME_MODE_PERFECTION => (30, 0),
        GAME_MODE_DOMINATION | GAME_MODE_MIGHT => (0, 0),
        GAME_MODE_GLORY => (0, 10_000),
        _ => return Err("game mode outside scope".into()),
    };
    if engine::game::tribe_name(RULES, setup.tribes[0]).is_none()
        || setup.tribes.iter().any(|&t| engine::game::tribe_name(RULES, t).is_none())
    {
        return Err(scope());
    }
    let options = GameOptions {
        version: RULES,
        seed: setup.seed,
        // Not read on this path (the side is passed on its own); set where the side is one of the game's sizes.
        size: map_size(setup.side).unwrap_or(MapSize::None),
        preset,
        players: setup.tribes.iter().map(|&tribe| PlayerSetup { tribe, human: true }).collect(),
        difficulty: BotDifficulty::Easy,
        mode: setup.mode,
        turn_limit,
        score_limit,
    };
    if options.players.len() > engine::max_players_on_side(setup.side) {
        return Err(scope());
    }
    // One .NET Random seeded with the slot's seed: its first draw is the game's random seed, the second would name
    // villages, and the map generator draws its attempt seeds from the rest. The same seed gives the same game as
    // polybots-engine's environment.
    let mut random = engine::rng::DotNetRandom::new(setup.seed);
    let game_seed = random.next();
    random.next();
    Engine::try_new_game_with_seeds(&options, setup.side, game_seed, &mut || random.next()).map_err(|e| e.to_string())
}

/// The row a command takes in `moves`.
fn row(e: &Engine, me: u8, c: &Command) -> Row {
    use Command::*;
    let unit_tile = |id: u32| e.unit(id).map_or(-1, |u| tile_index(e, u.coordinates));
    let unit_cost = |ty: i32| e.t.unit(ty).map_or(-1, |u| u.data.cost);
    let n = -1;
    match *c {
        Move { unit, to } => [6, unit_tile(unit), tile_index(e, to), n, n, n],
        Attack { unit, target } => [2, unit_tile(unit), tile_index(e, target), n, n, n],
        Capture { unit, .. } => [7, unit_tile(unit), n, n, n, n],
        Train { city, unit_type } => [5, n, tile_index(e, city), unit_type, n, unit_cost(unit_type)],
        Build { at, improvement } => {
            let price = e.t.improvement(improvement).map_or(0, |i| i.data.cost.unwrap_or(0));
            [1, n, tile_index(e, at), improvement, n, price]
        }
        Research { tech } => [8, n, n, tech, n, e.tech_price(me, tech)],
        CityReward { city, reward } => [11, n, tile_index(e, city), reward as i32, n, n],
        Promote { unit } => [13, unit_tile(unit), n, n, n, n],
        Recover { unit } => [3, unit_tile(unit), n, n, n, n],
        HealOthers { unit } => [4, unit_tile(unit), n, n, n, n],
        Explode { unit } => [24, unit_tile(unit), n, n, n, n],
        Disband { unit } => [10, unit_tile(unit), n, n, n, n],
        Destroy { at } => [9, n, tile_index(e, at), n, n, n],
        Decompose { at } => [26, n, tile_index(e, at), n, n, n],
        Upgrade { unit, to } => [16, unit_tile(unit), n, to, n, unit_cost(to)],
        ExamineRuins { unit } => [14, unit_tile(unit), n, n, n, n],
        Stay { unit } => [21, unit_tile(unit), n, n, n, n],
        Swarm { unit } => [38, unit_tile(unit), n, n, n, n],
        Boost { unit } => [25, unit_tile(unit), n, n, n, n],
        Disembark { unit } => [36, unit_tile(unit), n, n, n, n],
        FreezeArea { unit } => [17, unit_tile(unit), n, n, n, n],
        BreakIce { unit } => [18, unit_tile(unit), n, 0, n, n],
        ClearTileEffect { at, effect } => [39, n, tile_index(e, at), i32::from(effect), n, n],
        BreakPeace { with } => [29, n, n, n, i32::from(with), n],
        PeaceTreaty { with } => [27, n, n, n, i32::from(with), n],
        PeaceResponse { with, accept } => [28, n, n, i32::from(accept), i32::from(with), n],
        EstablishEmbassy { with } => [30, n, n, n, i32::from(with), e.embassy_cost(me)],
        EndTurn => [15, n, n, n, n, n],
    }
}

/// The order of rows within a slot: by the first five fields.
fn sort_key(r: &Row) -> [i32; 5] {
    [r[0], r[1], r[2], r[3], r[4]]
}

impl Slot {
    fn clear(&mut self) {
        self.game = None;
        self.winner = 0;
        self.rows.clear();
        self.commands.clear();
    }

    fn refuse(&mut self, reason: impl Into<String>) {
        self.clear();
        self.status = REFUSED;
        self.reason = reason.into();
    }

    fn reset(&mut self, setup: &Setup) {
        self.clear();
        self.decisions = 0;
        self.reason.clear();
        self.action_limit = setup.action_limit as u32;
        match std::panic::catch_unwind(|| new_game(setup)) {
            Ok(Ok(e)) => {
                self.game = Some(e);
                self.status = PLAYING;
                self.refresh();
            }
            Ok(Err(reason)) => self.refuse(reason),
            Err(_) => self.fail("the engine panicked setting up the game"),
        }
    }

    fn fail(&mut self, reason: &str) {
        self.clear();
        self.status = FAILED;
        self.reason = reason.into();
    }

    /// After a reset or a move: the game's state and the rows offered next.
    fn refresh(&mut self) {
        self.rows.clear();
        self.commands.clear();
        let Some(e) = self.game.as_ref() else { return };
        if e.is_over() {
            self.status = ENDED;
            self.winner = e.game_winners().first().map_or(0, |&w| i32::from(w));
            self.reason = "the game ended".into();
            return;
        }
        if self.decisions >= self.action_limit {
            self.status = TRUNCATED;
            self.reason = "the action limit was reached".into();
            return;
        }
        let me = e.current_player();
        // A row belongs to its source tile (the unit's), else its target tile; rows on a tile the mover has not
        // explored are left out, so the list does not reveal what is under the fog (the game would accept them).
        let explored = |r: &Row| {
            let anchor = if r[1] >= 0 { r[1] } else { r[2] };
            anchor < 0 || e.is_explored(me, anchor as usize)
        };
        let mut offered: Vec<(Row, Option<Command>)> =
            e.legal_commands().into_iter().map(|c| (row(e, me, &c), Some(c))).filter(|(r, _)| explored(r)).collect();
        let seats = e.s.players.iter().filter(|p| p.id != NATURE).count();
        if seats == 2 && !offered.is_empty() {
            offered.push(([RESIGN, -1, -1, -1, -1, -1], None));
        }
        offered.sort_by_key(|(r, _)| sort_key(r));
        offered.dedup_by_key(|(r, _)| sort_key(r));
        if offered.is_empty() {
            self.refuse("no legal move");
            return;
        }
        (self.rows, self.commands) = offered.into_iter().unzip();
    }

    /// Play row `pick`; 0 skipped, 1 taken, 2 rejected.
    fn step(&mut self, pick: i64) -> u8 {
        if pick == i64::from(SKIP) {
            return 0;
        }
        if self.status != PLAYING || pick < 0 || pick as usize >= self.rows.len() {
            return 2;
        }
        let command = self.commands[pick as usize];
        let e = self.game.as_mut().expect("a playing slot has a game");
        self.decisions += 1;
        match command {
            Some(c) => {
                if std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| e.apply_unchecked(c))).is_err() {
                    self.fail("the engine panicked applying a move");
                    return 1;
                }
                self.refresh();
            }
            None => {
                let winner = other_seat(e, e.current_player());
                self.rows.clear();
                self.commands.clear();
                self.status = ENDED;
                self.winner = winner;
                self.reason = "the player to move resigned".into();
            }
        }
        1
    }
}

/// The unit on `c` as `viewer` sees it: none on unexplored tiles, and an invisible foreign unit only at peace.
fn visible_unit(e: &Engine, c: Wc, viewer: u8) -> Option<&UnitState> {
    let i = e.tile_index(c)?;
    if !e.is_explored(viewer, i) {
        return None;
    }
    e.unit_at(c).filter(|u| u.owner == viewer || !u.invisible || e.at_peace(viewer, u.owner))
}

fn visible_root(e: &Engine, c: Wc, viewer: u8) -> Option<&UnitState> {
    visible_unit(e, c, viewer).filter(|u| u.health != 0 && !u.temporary_explorer)
}

fn unit_effect_bits(u: &UnitState) -> i16 {
    [u.frozen, u.poisoned, u.boosted, u.invisible, u.bubble, u.petrified, u.swift, u.double_ready, u.charmed]
        .iter()
        .enumerate()
        .fold(0, |acc, (bit, &on)| acc | (i16::from(on) << bit))
}

/// The attack preview of a row, as `PREVIEW_FIELDS`; zeros for rows without one.
fn preview(e: &Engine, me: u8, r: &Row, c: &Option<Command>) -> [i16; 4] {
    let Some(Command::Attack { unit, target }) = *c else { return [0; 4] };
    let _ = r;
    let Some(attacker) = e.unit(unit) else { return [0; 4] };
    let (Some(a), Some(d)) =
        (visible_root(e, attacker.coordinates, me), visible_root(e, target, me).filter(|d| d.owner != me))
    else {
        return [0; 4];
    };
    let q = e.battle_query_pub(a, d, false);
    let res = engine::rules::combat::get_battle_results(&e.combat_unit(a), &e.combat_unit(d), &q);
    let shown = |raw: i32, health: u16| (raw < i32::from(health)).then(|| (raw as f32 * 0.1).round_ties_even() as i32);
    let dealt = shown(res.attack_damage, d.health);
    let taken = (res.retaliation_damage != -1).then(|| shown(res.retaliation_damage, a.health));
    let flags = (if dealt.is_none() { KILLS_TARGET } else { 0 })
        | (if taken.is_some_and(|t| t.is_none()) { ATTACKER_DIES } else { 0 })
        | (if taken.is_some() { RETALIATION } else { 0 });
    [1, dealt.map_or(-1, |v| v as i16), taken.flatten().map_or(-1, |v| v as i16), flags]
}

/// The player to move's view: tile planes, scalars, player rows and techs, all -1 where unknown.
fn write_view(e: &Engine, tiles: &mut [i16], scalars: &mut [i32], players: &mut [i64], tech: &mut [i16]) {
    let viewer = e.current_player();
    let cells = e.s.map.tiles.len();
    let plane = |k: usize| k * cells;
    let rainbow = e.has_tribe_ability(viewer, "rainbowvision");
    for (i, x) in e.s.map.tiles.iter().enumerate() {
        let mut set = |k: usize, v: i16| tiles[plane(k) + i] = v;
        if !e.is_explored(viewer, i) {
            set(0, 0);
            set(12, i16::from(rainbow && (x.improvement == RUIN || x.has_ruin)));
            continue;
        }
        set(0, 1);
        set(12, 0);
        set(1, x.terrain as i16);
        set(2, x.climate as i16);
        set(3, e.get_resource(x.coordinates, viewer).map_or(0, |r| r as i16));
        set(6, i16::from(x.owner));
        set(7, i16::from(x.capital_of));
        set(8, tile_index(e, x.ruling_city) as i16);
        set(9, i16::from(x.has_road));
        set(10, i16::from(x.has_route));
        set(11, x.effects as i16);
        let known = x.improvement != 0 && e.t.improvement(x.improvement).is_some();
        set(4, if known { x.improvement as i16 } else { 0 });
        set(5, 0);
        if known && x.improvement == CITY {
            if let Some(m) = e.improvement_rt(x.coordinates) {
                set(5, m.level as i16);
                set(27, m.rewards.iter().filter(|&&r| r == CityReward::Park).count() as i16);
                set(28, m.rewards.iter().filter(|&&r| r == CityReward::SuperUnit).count() as i16);
                if x.owner == viewer {
                    set(23, m.population);
                    set(24, i16::from(m.connected_to_capital_of_player));
                    set(25, m.xp);
                    set(26, i16::from(m.rewards.contains(&CityReward::CityWall)));
                }
            }
        }
        set(13, 0);
        set(14, 0);
        let Some(u) = visible_unit(e, x.coordinates, viewer) else { continue };
        set(13, u.type_idx as i16);
        set(14, i16::from(u.owner));
        set(15, u.health.div_ceil(10) as i16);
        set(16, i16::from(u.promotion != 0));
        set(21, u.xp as i16);
        if u.owner == viewer {
            set(17, unit_effect_bits(u));
            set(18, u.passenger.as_ref().map_or(0, |p| p.type_idx as i16));
            set(19, i16::from(u.moved));
            set(20, i16::from(u.attacked));
            set(22, tile_index(e, u.home) as i16);
        }
    }
    let me = e.s.players.iter().find(|p| p.id == viewer);
    scalars[0] = i32::from(viewer);
    scalars[1] = e.s.current_turn as i32;
    scalars[2] = me.map_or(0, |p| p.currency);
    let known: Vec<u8> = e.player_runtime(viewer).map(|r| r.known_players.clone()).unwrap_or_default();
    let mut rows: Vec<&engine::PlayerState> = me.into_iter().collect();
    let mut met: Vec<&engine::PlayerState> =
        e.s.players.iter().filter(|p| p.id != viewer && p.id != NATURE && known.contains(&p.id)).collect();
    met.sort_by_key(|p| p.id);
    rows.extend(met);
    for (k, p) in rows.iter().take(MAX_SEATS).enumerate() {
        let relation = if p.id == viewer { 0 } else { get_relation(&e.s, viewer, p.id).state as i64 };
        players[k * 4..k * 4 + 4].copy_from_slice(&[i64::from(p.id), i64::from(p.tribe), relation, i64::from(p.score)]);
    }
    let mut techs: Vec<i16> = e.techs(viewer).iter().map(|&t| t as i16).collect();
    techs.sort_unstable();
    for (slot, t) in tech.iter_mut().zip(techs) {
        *slot = t;
    }
}

/// The defence bonus (`max_health == false`) or displayed maximum health of every unit the mover sees.
fn unit_plane(e: &Engine, max_health: bool) -> Vec<i16> {
    let viewer = e.current_player();
    e.s.map
        .tiles
        .iter()
        .map(|x| {
            visible_root(e, x.coordinates, viewer).map_or(-1, |u| {
                if max_health {
                    (e.max_health(u) as u32).div_ceil(10) as i16
                } else {
                    e.defence_bonus(u) as i16
                }
            })
        })
        .collect()
}

/// Runs `f` on every slot named in `work` (each at most once), spread over up to `workers` threads, and returns
/// each result with its slot's index.
fn par_each<T: Sync, R: Send>(
    slots: &mut [Slot],
    work: &[(usize, T)],
    workers: usize,
    f: impl Fn(&mut Slot, &T) -> R + Sync,
) -> Vec<(usize, R)> {
    let mut by_slot: Vec<Option<&T>> = vec![None; slots.len()];
    for (s, t) in work {
        by_slot[*s] = Some(t);
    }
    let mut jobs: Vec<(usize, &mut Slot, &T)> =
        slots.iter_mut().zip(by_slot).enumerate().filter_map(|(i, (slot, t))| t.map(|t| (i, slot, t))).collect();
    let workers = workers.clamp(1, jobs.len().max(1));
    if workers == 1 {
        return jobs.into_iter().map(|(i, slot, t)| (i, f(slot, t))).collect();
    }
    let chunk = jobs.len().div_ceil(workers);
    std::thread::scope(|scope| {
        let handles: Vec<_> = jobs
            .chunks_mut(chunk)
            .map(|part| {
                let f = &f;
                scope.spawn(move || part.iter_mut().map(|(i, slot, t)| (*i, f(slot, t))).collect::<Vec<_>>())
            })
            .collect();
        handles.into_iter().flat_map(|h| h.join().expect("a batch worker panicked")).collect()
    })
}

/// Many games at once; every array covers all slots.
#[pyclass(module = "polytopia_sim.engine_fast")]
struct Batch {
    slots: Mutex<Vec<Slot>>,
    size: usize,
    workers: usize,
    outputs: Vec<&'static str>,
    tech_width: usize,
}

fn refuse(msg: impl Into<String>) -> PyErr {
    PyValueError::new_err(msg.into())
}

impl Batch {
    fn need(&self, name: &str) -> PyResult<()> {
        if self.outputs.contains(&name) {
            Ok(())
        } else {
            Err(refuse(format!("the batch was made without outputs=(\"{name}\",)")))
        }
    }

    /// The slots; a batch in use by another Python thread (during a reset or step) refuses rather than waits.
    fn lock(&self) -> PyResult<MutexGuard<'_, Vec<Slot>>> {
        self.slots.try_lock().map_err(|_| refuse("the batch is in use by another thread"))
    }
}

fn playing(slots: &[Slot], slot: usize) -> Option<&Engine> {
    let s = &slots[slot];
    (s.status == PLAYING).then_some(s.game.as_ref()).flatten()
}

#[pymethods]
impl Batch {
    /// `games` slots of `size` x `size` maps (11 to 30, or 8). `rules` is accepted for compatibility and ignored: this
    /// engine carries its own rules data.
    #[new]
    #[pyo3(signature = (games, size=11, workers=1, backend="cpu", outputs=Vec::new(), rules=None))]
    fn new(
        games: usize,
        size: i64,
        workers: usize,
        backend: &str,
        outputs: Vec<String>,
        rules: Option<PyObject>,
    ) -> PyResult<Self> {
        let _ = rules;
        let size = usize::try_from(size).ok().filter(|&s| side_supported(s));
        let Some(size) = size else {
            return Err(refuse("board side is neither 11 to 30 nor nano (8)"));
        };
        if let Some(o) = outputs.iter().find(|o| !OUTPUTS.contains(&o.as_str())) {
            return Err(refuse(format!("unknown output {o:?}: one of {OUTPUTS:?}")));
        }
        let wanted: Vec<&'static str> = OUTPUTS.into_iter().filter(|n| outputs.iter().any(|o| o == n)).collect();
        match backend {
            "cpu" => {}
            "gpu" => {
                if let Some(o) = wanted.iter().find(|o| CPU_OUTPUTS.contains(o)) {
                    return Err(refuse(format!("output {o:?}: the cpu backend only")));
                }
                return Err(refuse("this engine has no gpu backend; use backend=\"cpu\""));
            }
            _ => return Err(refuse("backend \"cpu\" or \"gpu\"")),
        }
        let tables = Engine::tables_for(RULES);
        let tech_width = tables.logic.tech.values().map(|t| t.idx + 1).max().unwrap_or(0).max(0) as usize;
        Ok(Batch {
            slots: Mutex::new((0..games).map(|_| Slot::default()).collect()),
            size,
            workers: workers.max(1),
            outputs: wanted,
            tech_width,
        })
    }

    fn __len__(&self) -> PyResult<usize> {
        Ok(self.lock()?.len())
    }

    #[getter]
    fn size(&self) -> usize {
        self.size
    }

    /// Start new games in `slots`, one seed each, all with the seats `tribes` (the game's tribe numbers).
    #[pyo3(signature = (slots, seeds, tribes, map=2, mode=4, action_limit=1000))]
    #[allow(clippy::too_many_arguments)]
    fn reset(
        &mut self,
        py: Python<'_>,
        slots: Vec<i64>,
        seeds: Vec<i64>,
        tribes: Vec<i32>,
        map: i32,
        mode: i32,
        action_limit: i64,
    ) -> PyResult<()> {
        if slots.len() != seeds.len() {
            return Err(refuse("one seed per slot"));
        }
        let side = self.size as u16;
        let workers = self.workers;
        let mut guard = self.lock()?;
        let mut seen = vec![false; guard.len()];
        let mut work = Vec::with_capacity(slots.len());
        for (&slot, &seed) in slots.iter().zip(&seeds) {
            let seed = i32::try_from(seed).map_err(|_| refuse("seed out of range"))?;
            let s =
                usize::try_from(slot).ok().filter(|&s| s < guard.len()).ok_or_else(|| refuse("slot out of range"))?;
            if std::mem::replace(&mut seen[s], true) {
                return Err(refuse(format!("slot {slot} named twice")));
            }
            work.push((s, Setup { seed, side, tribes: tribes.clone(), map, mode, action_limit }));
        }
        let slots = &mut *guard;
        py.allow_threads(|| par_each(slots, &work, workers, |slot, setup| slot.reset(setup)));
        Ok(())
    }

    /// Play one row per slot (`SKIP` leaves a slot alone); per slot 0 skipped, 1 taken, 2 rejected.
    fn step<'py>(&mut self, py: Python<'py>, moves: Vec<i64>) -> PyResult<Bound<'py, PyArray1<u8>>> {
        let mut slots = self.lock()?;
        if moves.len() != slots.len() {
            return Err(refuse("one move and one result per slot"));
        }
        let work: Vec<(usize, i64)> = moves.into_iter().enumerate().collect();
        let mut applied = vec![0u8; slots.len()];
        let workers = self.workers;
        let slots = &mut *slots;
        for (i, r) in py.allow_threads(|| par_each(slots, &work, workers, |slot, &pick| slot.step(pick))) {
            applied[i] = r;
        }
        Ok(PyArray1::from_vec(py, applied))
    }

    fn status<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyArray1<u8>>> {
        let slots = self.lock()?;
        Ok(PyArray1::from_vec(py, slots.iter().map(|s| s.status).collect()))
    }

    fn winner<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyArray1<i32>>> {
        let slots = self.lock()?;
        Ok(PyArray1::from_vec(py, slots.iter().map(|s| if s.status == ENDED { s.winner } else { 0 }).collect()))
    }

    /// Why a slot is not playing (empty while it is).
    fn reason(&self, slot: i64) -> PyResult<String> {
        let slots = self.lock()?;
        let s = usize::try_from(slot).ok().and_then(|s| slots.get(s)).ok_or_else(|| refuse("slot out of range"))?;
        Ok(if s.status == EMPTY { String::new() } else { s.reason.clone() })
    }

    /// `(offsets, rows)`: slot `s`'s legal rows are `rows[offsets[s]:offsets[s + 1]]`, in `MOVE_FIELDS` layout.
    #[allow(clippy::type_complexity)]
    fn moves<'py>(&self, py: Python<'py>) -> PyResult<(Bound<'py, PyArray1<i64>>, Bound<'py, numpy::PyArray2<i32>>)> {
        let slots = self.lock()?;
        let mut offsets = Vec::with_capacity(slots.len() + 1);
        let mut flat = Vec::new();
        offsets.push(0i64);
        for s in &*slots {
            if s.status == PLAYING {
                for r in &s.rows {
                    flat.extend_from_slice(r);
                }
            }
            offsets.push((flat.len() / MOVE_FIELDS.len()) as i64);
        }
        let n = flat.len() / MOVE_FIELDS.len();
        let rows = PyArray1::from_vec(py, flat).reshape([n, MOVE_FIELDS.len()])?;
        Ok((PyArray1::from_vec(py, offsets), rows))
    }

    /// The view of each slot's player to move: `(tiles, scalars, players, tech)`.
    #[allow(clippy::type_complexity)]
    fn views<'py>(
        &self,
        py: Python<'py>,
    ) -> PyResult<(
        Bound<'py, numpy::PyArray3<i16>>,
        Bound<'py, numpy::PyArray2<i32>>,
        Bound<'py, numpy::PyArray3<i64>>,
        Bound<'py, numpy::PyArray2<i16>>,
    )> {
        let slots = self.lock()?;
        let n = slots.len();
        let cells = self.size * self.size;
        let planes = TILE_PLANES.len();
        let mut tiles = vec![-1i16; n * planes * cells];
        let mut scalars = vec![-1i32; n * SCALAR_FIELDS.len()];
        let mut players = vec![-1i64; n * MAX_SEATS * PLAYER_FIELDS.len()];
        let mut tech = vec![-1i16; n * self.tech_width];
        let width = self.tech_width;
        let parts = tiles
            .chunks_mut(planes * cells)
            .zip(scalars.chunks_mut(SCALAR_FIELDS.len()))
            .zip(players.chunks_mut(MAX_SEATS * PLAYER_FIELDS.len()))
            .zip(tech.chunks_mut(width.max(1)));
        for (slot, (((t, s), p), k)) in slots.iter().zip(parts) {
            if let Some(e) = (slot.status == PLAYING).then_some(slot.game.as_ref()).flatten() {
                write_view(e, t, s, p, &mut k[..width]);
            }
        }
        Ok((
            PyArray1::from_vec(py, tiles).reshape([n, planes, cells])?,
            PyArray1::from_vec(py, scalars).reshape([n, SCALAR_FIELDS.len()])?,
            PyArray1::from_vec(py, players).reshape([n, MAX_SEATS, PLAYER_FIELDS.len()])?,
            PyArray1::from_vec(py, tech).reshape([n, width])?,
        ))
    }

    /// The attack preview of every row of `moves`, in `PREVIEW_FIELDS` layout.
    fn previews<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, numpy::PyArray2<i16>>> {
        let slots = self.lock()?;
        self.need("previews")?;
        let mut flat = Vec::new();
        for (i, s) in slots.iter().enumerate() {
            let Some(e) = playing(&slots, i) else { continue };
            let me = e.current_player();
            for (r, c) in s.rows.iter().zip(&s.commands) {
                flat.extend_from_slice(&preview(e, me, r, c));
            }
        }
        let n = flat.len() / PREVIEW_FIELDS.len();
        PyArray1::from_vec(py, flat).reshape([n, PREVIEW_FIELDS.len()])
    }

    /// The defence bonus, in tenths, of every unit the player to move sees; -1 elsewhere.
    fn defence<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, numpy::PyArray2<i16>>> {
        self.need("defence")?;
        self.unit_planes(py, false)
    }

    /// The displayed maximum health of every unit the player to move sees; -1 elsewhere.
    fn max_health<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, numpy::PyArray2<i16>>> {
        self.need("max_health")?;
        self.unit_planes(py, true)
    }

    /// The stars per turn of each slot's player to move; -1 for slots not playing.
    fn income<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyArray1<i32>>> {
        let slots = self.lock()?;
        self.need("income")?;
        let v = (0..slots.len()).map(|i| playing(&slots, i).map_or(-1, |e| e.income(e.current_player()))).collect();
        Ok(PyArray1::from_vec(py, v))
    }

    #[pyo3(signature = (*_args, **_kwargs))]
    fn queue(&self, _args: &Bound<'_, pyo3::types::PyTuple>, _kwargs: Option<&Bound<'_, PyDict>>) -> PyResult<()> {
        Err(refuse("queue needs the gpu backend"))
    }

    fn queue_depth(&self) -> PyResult<usize> {
        Err(refuse("queue_depth needs the gpu backend"))
    }

    #[pyo3(signature = (*_args, **_kwargs))]
    fn restart_finished(
        &self,
        _args: &Bound<'_, pyo3::types::PyTuple>,
        _kwargs: Option<&Bound<'_, PyDict>>,
    ) -> PyResult<()> {
        Err(refuse("restart_finished needs the gpu backend"))
    }

    fn device_arrays(&self) -> PyResult<()> {
        Err(refuse("device_arrays: the gpu backend only"))
    }

    #[pyo3(signature = (*_args, **_kwargs))]
    fn step_device(
        &self,
        _args: &Bound<'_, pyo3::types::PyTuple>,
        _kwargs: Option<&Bound<'_, PyDict>>,
    ) -> PyResult<()> {
        Err(refuse("step_device: the gpu backend only"))
    }

    /// Bytes moved to and from a device: always `(0, 0)` on the CPU.
    fn traffic(&self) -> (u64, u64) {
        (0, 0)
    }
}

impl Batch {
    fn unit_planes<'py>(&self, py: Python<'py>, max_health: bool) -> PyResult<Bound<'py, numpy::PyArray2<i16>>> {
        let slots = self.lock()?;
        let cells = self.size * self.size;
        let mut flat = vec![-1i16; slots.len() * cells];
        for (i, part) in flat.chunks_mut(cells.max(1)).enumerate().take(slots.len()) {
            if let Some(e) = playing(&slots, i) {
                part.copy_from_slice(&unit_plane(e, max_health));
            }
        }
        PyArray1::from_vec(py, flat).reshape([slots.len(), cells])
    }
}

/// Adds the `engine_fast` submodule to `parent` and makes `import polytopia_sim.engine_fast` work.
pub fn register(parent: &Bound<'_, PyModule>) -> PyResult<()> {
    let py = parent.py();
    let m = PyModule::new(py, "engine_fast")?;
    m.add_class::<Batch>()?;
    m.add("RULES", RULES)?;
    m.add("MOVE_FIELDS", MOVE_FIELDS.to_vec())?;
    m.add("PREVIEW_FIELDS", PREVIEW_FIELDS.to_vec())?;
    m.add("SCALAR_FIELDS", SCALAR_FIELDS.to_vec())?;
    m.add("PLAYER_FIELDS", PLAYER_FIELDS.to_vec())?;
    m.add("TILE_PLANES", TILE_PLANES.to_vec())?;
    m.add("STATUS", STATUS.to_vec())?;
    m.add("APPLIED", APPLIED.to_vec())?;
    m.add("OUTPUTS", OUTPUTS.to_vec())?;
    m.add("DEVICE_ARRAYS", DEVICE_ARRAYS.to_vec())?;
    m.add("SKIP", SKIP)?;
    let kinds = PyDict::new(py);
    for (name, id) in KINDS {
        kinds.set_item(name, id)?;
    }
    m.add("KINDS", kinds)?;
    parent.add_submodule(&m)?;
    py.import("sys")?.getattr("modules")?.set_item("polytopia_sim.engine_fast", &m)?;
    Ok(())
}
