//! A unit that examines a ruin stands visible afterwards.
//!
//! In the game, examining a ruin queues a reveal of the examining unit when it carries the Invisible effect, so a Cloak
//! or Cloak boat that hid on an earlier move is visible after the examination. The next move then hides it again,
//! and that hide action marks the unit attacked.
//!
//! The replay test is live game 355af191 (Might, game version 123, 17 November 2026): player 2's Cloak boat 609 examines
//! a ruin, is pushed off the tile by the battleship reward, and moves again in the next turn. The server has the boat
//! attacked after that move; the engine kept it invisible through the examination and left it unspent. The record is
//! read from `POLYTOPIA_CLIENT_GAMES`; the replay test skips without it.
use polytopia_sim::engine::replay::SaveFile;
use polytopia_sim::engine::{Command, Engine};
use polytopia_sim::json::Json;
use polytopia_sim::WorldCoordinates as Wc;

const CLOAK: i64 = 38;

/// A 5x3 field of land with a ruin on (2,1) and player 1's invisible cloak standing on it.
fn board() -> Engine {
    let mut tiles = Vec::new();
    for y in 0..3 {
        for x in 0..5 {
            let mut t = format!(r#""x":{x},"y":{y},"terrain":3,"climate":2,"explored":[1,2]"#);
            match (x, y) {
                (0, 0) => t.push_str(r#","city":true,"capital":true,"owner":1,"ruling":[0,0]"#),
                (4, 0) => t.push_str(r#","city":true,"capital":true,"owner":2,"ruling":[4,0]"#),
                (2, 1) => t.push_str(r#","improvement":2"#),
                _ => {}
            }
            tiles.push(format!("{{{t}}}"));
        }
    }
    let text = format!(
        r#"{{"format":"polytopia-position","game":{{"turn":6,"width":5,"height":3,"current_player":1}},
           "players":[{{"id":1,"tribe":2,"human":true,"stars":5,"known":[2]}},{{"id":2,"tribe":3,"stars":5,"known":[1]}}],
           "tiles":[{}],
           "improvements":[{{"x":0,"y":0,"level":1}},{{"x":4,"y":0,"level":1}}],
           "units":[{{"id":1,"type":{CLOAK},"owner":1,"x":2,"y":1,"flags":["invisible"]}}]}}"#,
        tiles.join(",")
    );
    Engine::from_position(&Json::parse(&text).unwrap()).unwrap()
}

#[test]
fn an_invisible_unit_is_revealed_by_examining_a_ruin() {
    let mut e = board();
    assert!(e.unit(1).unwrap().invisible);
    assert!(e.tile(Wc { x: 2, y: 1 }).unwrap().has_ruin);
    e.apply_unchecked(Command::ExamineRuins { unit: 1 });
    let unit = e.unit(1).unwrap();
    assert!(!unit.invisible, "the examiner is visible");
    assert!(unit.moved && unit.attacked, "and exhausted");
}

#[test]
fn a_moving_unit_hides_again_and_is_marked_attacked() {
    let mut e = board();
    e.apply_unchecked(Command::ExamineRuins { unit: 1 });
    e.s.units[0].moved = false;
    e.s.units[0].attacked = false;
    e.apply_unchecked(Command::Move { unit: 1, to: Wc { x: 2, y: 0 } });
    let unit = e.unit(1).unwrap();
    assert!(unit.invisible, "the move hides it");
    assert!(unit.attacked, "the hide action marks it attacked");
}

fn games_dir() -> String {
    std::env::var("POLYTOPIA_CLIENT_GAMES").unwrap_or_default()
}

#[test]
fn replay_of_355af191_marks_the_cloak_boat_attacked_like_the_server() {
    let dir = games_dir();
    let Some(init_path) = std::fs::read_dir(&dir).ok().and_then(|d| {
        d.filter_map(|e| e.ok().map(|e| e.path())).find(|p| {
            p.file_name().is_some_and(|n| {
                n.to_string_lossy().starts_with("355af191") && n.to_string_lossy().ends_with("_init.bin")
            })
        })
    }) else {
        eprintln!("skipped: record 355af191 absent in {dir}");
        return;
    };
    let cur_path = init_path.to_string_lossy().replace("_init.bin", "_cur.bin");
    let (Ok(init), Ok(cur)) = (std::fs::read(&init_path), std::fs::read(&cur_path)) else {
        eprintln!("skipped: record 355af191 incomplete in {dir}");
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
            (ours.coordinates, ours.moved, ours.attacked, ours.invisible),
            (theirs.coordinates, theirs.moved, theirs.attacked, theirs.invisible),
            "unit {} (tile, moved, attacked, invisible)",
            theirs.id
        );
    }
}
