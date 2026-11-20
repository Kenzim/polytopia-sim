//! A super unit reward waits behind a city reward raised by the push of the unit on its city.
//!
//! Retail queues the push of an occupant (a move that explores from the new tile) before the train of the giant. When
//! that exploration reveals a map corner, the lighthouse population levels the capital up and raises a city reward;
//! the action stack stops there, so the giant appears only after that reward is answered, and takes the unit id after
//! the one the answering reward's own unit takes.
//!
//! The replay test is live game 17f83d72 (Might, game version 123, 17 November 2026): two giants from two cities'
//! rewards in consecutive commands. The server numbers the second city's giant 67 and the first city's 68; the engine
//! had them the other way round. The record is read from `POLYTOPIA_CLIENT_GAMES`; the replay test skips without it.
use polytopia_sim::engine::replay::SaveFile;
use polytopia_sim::engine::{Command, Deferred, Engine};
use polytopia_sim::json::Json;
use polytopia_sim::{CityReward, WorldCoordinates as Wc};

const WARRIOR: i64 = 2;
const GIANT: i32 = 12;
const SIZE: i32 = 7;

fn giants(e: &Engine) -> Vec<(u32, Wc)> {
    e.s.units.iter().filter(|u| u.owner == 1 && u.type_idx == GIANT).map(|u| (u.id, u.coordinates)).collect()
}

/// A 7x7 field. Player 1's capital (0,0) is one population short of level 2; its second city (5,5) holds a warrior
/// that can only step aside onto a tile beside the corner (6,6), which player 1 has explored only when `seen`.
fn board(seen: bool) -> Engine {
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
            } else if (x, y) == (5, 5) {
                t.push_str(r#","city":true,"owner":1,"ruling":[5,5]"#);
            }
            tiles.push(format!("{{{t}}}"));
        }
    }
    let text = format!(
        r#"{{"format":"polytopia-position","game":{{"turn":6,"width":{SIZE},"height":{SIZE},"current_player":1}},
           "players":[{{"id":1,"tribe":2,"human":true,"stars":5}},{{"id":2,"tribe":3,"stars":5}}],
           "tiles":[{}],
           "improvements":[{{"x":0,"y":0,"level":1,"xp":1}},{{"x":5,"y":5,"level":5}}],
           "units":[{{"id":1,"type":{WARRIOR},"owner":1,"x":5,"y":5}}]}}"#,
        tiles.join(",")
    );
    Engine::from_position(&Json::parse(&text).unwrap()).unwrap()
}

fn reward(e: &mut Engine) {
    e.apply_unchecked(Command::CityReward { city: Wc { x: 5, y: 5 }, reward: CityReward::SuperUnit });
}

#[test]
fn the_giant_waits_for_the_reward_the_push_raised() {
    let mut e = board(false);
    reward(&mut e);
    assert_ne!(e.unit(1).unwrap().coordinates, Wc { x: 5, y: 5 }, "the warrior stepped aside");
    assert!(e.reward_pending(), "its sight revealed the corner and levelled the capital up");
    assert!(giants(&e).is_empty(), "the giant is still queued");

    let answer =
        e.legal_commands().into_iter().find(|c| matches!(c, Command::CityReward { .. })).expect("a reward to answer");
    e.apply(answer).unwrap();
    assert_eq!(giants(&e), vec![(2, Wc { x: 5, y: 5 })], "the giant appears once the reward is answered");
}

#[test]
fn a_held_giant_survives_a_position_round_trip() {
    let mut e = board(false);
    reward(&mut e);
    assert!(e.deferred.iter().any(|d| matches!(d, Deferred::Train { .. })));
    let mut copy = Engine::from_position(&e.to_position()).unwrap();
    assert_eq!(copy.deferred, e.deferred);
    let answer = copy.legal_commands().into_iter().find(|c| matches!(c, Command::CityReward { .. })).unwrap();
    copy.apply(answer).unwrap();
    assert_eq!(giants(&copy).len(), 1);
}

#[test]
fn without_a_raised_reward_the_giant_appears_at_once() {
    let mut e = board(true);
    reward(&mut e);
    assert!(!e.reward_pending());
    assert_eq!(giants(&e), vec![(2, Wc { x: 5, y: 5 })]);
}

fn games_dir() -> String {
    std::env::var("POLYTOPIA_CLIENT_GAMES").unwrap_or_default()
}

#[test]
fn replay_of_17f83d72_numbers_the_giants_like_the_server() {
    let dir = games_dir();
    let Some(init_path) = std::fs::read_dir(&dir).ok().and_then(|d| {
        d.filter_map(|e| e.ok().map(|e| e.path())).find(|p| {
            p.file_name().is_some_and(|n| {
                n.to_string_lossy().starts_with("17f83d72") && n.to_string_lossy().ends_with("_init.bin")
            })
        })
    }) else {
        eprintln!("skipped: record 17f83d72 absent in {dir}");
        return;
    };
    let cur_path = init_path.to_string_lossy().replace("_init.bin", "_cur.bin");
    let (Ok(init), Ok(cur)) = (std::fs::read(&init_path), std::fs::read(&cur_path)) else {
        eprintln!("skipped: record 17f83d72 incomplete in {dir}");
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
    for theirs in &save.current.units {
        let ours =
            e.s.units.iter().find(|u| u.id == theirs.id).unwrap_or_else(|| panic!("unit {} is missing", theirs.id));
        assert_eq!(
            (ours.coordinates, ours.type_idx),
            (theirs.coordinates, theirs.type_idx),
            "unit {} (tile, type)",
            theirs.id
        );
    }
}
