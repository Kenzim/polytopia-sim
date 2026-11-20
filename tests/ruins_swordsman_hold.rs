//! The swordsman of a ruin reward waits behind a city reward raised by the push of the examiner.
//!
//! Retail trains the reward's swordsman onto the ruin's tile like a giant onto a city: the unit standing there (the
//! examiner) is pushed aside by a queued move that explores from its new tile. When that exploration reveals a map
//! corner, the lighthouse population levels the capital up and raises a city reward; the action stack stops there, so
//! the swordsman appears only after that reward is answered.
//!
//! The replay test is live game 82f99676 (Might, game version 123): player 2 examined a ruin at (8, 10), then resigned.
//! The server has no swordsman; the engine spawned one beside the pending reward. The record comes from
//! `POLYTOPIA_CLIENT_GAMES`; the replay test skips without it.
use polytopia_sim::engine::replay::SaveFile;
use polytopia_sim::engine::{Command, Deferred, Engine};
use polytopia_sim::json::Json;
use polytopia_sim::WorldCoordinates as Wc;

const WARRIOR: i64 = 2;
const RUIN: i64 = 2;
const SIZE: i32 = 7;
const RUIN_AT: Wc = Wc { x: 5, y: 5 };

/// A 7x7 field. Player 1's capital (0,0) is one population short of level 2; a ruin on (5,5) holds a warrior that can
/// only step aside onto a tile beside the corner (6,6), which player 1 has explored only when `seen`. The game seed is
/// one for which that ruin gives the swordsman.
fn board(seen: bool, seed: i32) -> Engine {
    let mut tiles = Vec::new();
    for y in 0..SIZE {
        for x in 0..SIZE {
            let near_corner = (x, y) == (6, 6) || (x, y) == (6, 5) || (x, y) == (5, 6);
            let explored = if (x, y) == (6, 6) && !seen { "[2]" } else { "[1,2]" };
            let water = (x - 5).abs() <= 1 && (y - 5).abs() <= 1 && (x, y) != (5, 5) && !near_corner;
            let mut t =
                format!(r#""x":{x},"y":{y},"terrain":{},"climate":2,"explored":{explored}"#, if water { 2 } else { 3 });
            if (x, y) == (0, 0) {
                t.push_str(r#","city":true,"capital":true,"owner":1,"ruling":[0,0]"#);
            } else if (x, y) == (6, 0) {
                t.push_str(r#","city":true,"capital":true,"owner":2,"ruling":[6,0]"#);
            } else if (x, y) == (5, 5) {
                t.push_str(&format!(r#","improvement":{RUIN}"#));
            }
            tiles.push(format!("{{{t}}}"));
        }
    }
    let text = format!(
        r#"{{"format":"polytopia-position","game":{{"turn":6,"width":{SIZE},"height":{SIZE},"current_player":1,"seed":{seed}}},
           "players":[{{"id":1,"tribe":2,"human":true,"stars":5}},{{"id":2,"tribe":3,"stars":5}}],
           "tiles":[{}],
           "improvements":[{{"x":0,"y":0,"level":1,"xp":1}},{{"x":6,"y":0,"level":1}}],
           "units":[{{"id":1,"type":{WARRIOR},"owner":1,"x":5,"y":5}}]}}"#,
        tiles.join(",")
    );
    Engine::from_position(&Json::parse(&text).unwrap()).unwrap()
}

fn swordsman_seed() -> i32 {
    (0..2000).find(|&seed| board(true, seed).ruins_reward(1, RUIN_AT) == 8).expect("a seed that gives the swordsman")
}

fn swordsmen(e: &Engine) -> Vec<(u32, Wc, u16)> {
    let sword = e.t.unit_idx.get(e.tribe_unit_name(1, "swordsman").as_str()).copied().expect("a swordsman type");
    e.s.units.iter().filter(|u| u.type_idx == sword).map(|u| (u.id, u.coordinates, u.promotion)).collect()
}

#[test]
fn the_swordsman_waits_for_the_reward_the_push_raised() {
    let mut e = board(false, swordsman_seed());
    e.apply_unchecked(Command::ExamineRuins { unit: 1 });
    assert_ne!(e.unit(1).unwrap().coordinates, RUIN_AT, "the examiner stepped aside");
    assert!(e.reward_pending(), "its sight revealed the corner and levelled the capital up");
    assert!(swordsmen(&e).is_empty(), "the swordsman is still queued");

    let answer =
        e.legal_commands().into_iter().find(|c| matches!(c, Command::CityReward { .. })).expect("a reward to answer");
    e.apply(answer).unwrap();
    assert_eq!(swordsmen(&e), vec![(2, RUIN_AT, 1)], "the swordsman appears, promoted, once the reward is answered");
}

#[test]
fn a_held_swordsman_survives_a_position_round_trip() {
    let mut e = board(false, swordsman_seed());
    e.apply_unchecked(Command::ExamineRuins { unit: 1 });
    assert!(e.deferred.iter().any(|d| matches!(d, Deferred::Train { veteran: true, .. })));
    let mut copy = Engine::from_position(&e.to_position()).unwrap();
    assert_eq!(copy.deferred, e.deferred);
    let answer = copy.legal_commands().into_iter().find(|c| matches!(c, Command::CityReward { .. })).unwrap();
    copy.apply(answer).unwrap();
    assert_eq!(swordsmen(&copy), vec![(2, RUIN_AT, 1)]);
}

#[test]
fn without_a_raised_reward_the_swordsman_appears_at_once() {
    let mut e = board(true, swordsman_seed());
    e.apply_unchecked(Command::ExamineRuins { unit: 1 });
    assert!(!e.reward_pending());
    assert_eq!(swordsmen(&e), vec![(2, RUIN_AT, 1)]);
}

fn games_dir() -> String {
    std::env::var("POLYTOPIA_CLIENT_GAMES").unwrap_or_default()
}

#[test]
fn replay_of_82f99676_holds_the_swordsman_like_the_server() {
    let dir = games_dir();
    let Some(init_path) = std::fs::read_dir(&dir).ok().and_then(|d| {
        d.filter_map(|e| e.ok().map(|e| e.path())).find(|p| {
            p.file_name().is_some_and(|n| {
                n.to_string_lossy().starts_with("82f99676") && n.to_string_lossy().ends_with("_init.bin")
            })
        })
    }) else {
        eprintln!("skipped: record 82f99676 absent in {dir}");
        return;
    };
    let cur_path = init_path.to_string_lossy().replace("_init.bin", "_cur.bin");
    let (Ok(init), Ok(cur)) = (std::fs::read(&init_path), std::fs::read(&cur_path)) else {
        eprintln!("skipped: record 82f99676 incomplete in {dir}");
        return;
    };
    let save = SaveFile::parse_server(&init, &cur).unwrap();
    let mut e = Engine::from_state(save.initial.clone());
    if save.commands.iter().any(|c| c.kind == 20) {
        e.start_match();
    }
    for c in &save.commands {
        if c.kind == 20 || c.player == 255 {
            continue;
        }
        match e.command_from_raw(c) {
            Some(cmd) => e.apply_unchecked(cmd),
            None => e.s.current_command += 1,
        }
    }
    let mut ours: Vec<u32> = e.s.units.iter().map(|u| u.id).collect();
    let mut theirs: Vec<u32> = save.current.units.iter().map(|u| u.id).collect();
    ours.sort_unstable();
    theirs.sort_unstable();
    assert_eq!(ours, theirs, "the unit ids");
}
