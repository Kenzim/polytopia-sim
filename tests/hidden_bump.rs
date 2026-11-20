//! A move onto a tile holding a unit the mover cannot see.
//!
//! Retail's move command first checks the destination tile: when it holds a unit (the path search lets a unit walk
//! towards an invisible enemy, but the tile is not free) the mover stays where it is and is not spent, the hidden unit
//! loses its Invisible effect, the mover turns to face it, and an owner the mover's owner has not met yet is met.
//!
//! The replay tests are eight live games of 16 and 17 November 2026 (Might mode, game version 123) in which a
//! player's move ended on a hidden Cloak or Cloak boat: the server's state kept the unit on its origin tile with its
//! move unspent, where the engine had put the two units on one tile. The records come from `POLYTOPIA_CLIENT_GAMES`.
//! The replay tests skip without them.
use polytopia_sim::engine::replay::SaveFile;
use polytopia_sim::engine::{Command, Engine};
use polytopia_sim::json::Json;
use polytopia_sim::WorldCoordinates as Wc;

const WARRIOR: i64 = 2;
const CLOAK: i64 = 38;

fn wc(x: i32, y: i32) -> Wc {
    Wc { x, y }
}

/// A 5x3 field of land. Player 1's warrior stands on (1,1) and player 2's hidden Cloak on (2,1); the capitals are in
/// the corners. `known` is whether the two players have met.
fn board(known: bool) -> Engine {
    let mut tiles = Vec::new();
    for y in 0..3 {
        for x in 0..5 {
            let mut t = format!(r#""x":{x},"y":{y},"terrain":3,"climate":2,"explored":[1,2]"#);
            match (x, y) {
                (0, 0) => t.push_str(r#","city":true,"capital":true,"owner":1,"ruling":[0,0]"#),
                (4, 0) => t.push_str(r#","city":true,"capital":true,"owner":2,"ruling":[4,0]"#),
                _ => {}
            }
            tiles.push(format!("{{{t}}}"));
        }
    }
    let known = if known { "[2]" } else { "[]" };
    let text = format!(
        r#"{{"format":"polytopia-position","game":{{"turn":6,"width":5,"height":3,"current_player":1}},
           "players":[{{"id":1,"tribe":2,"human":true,"stars":5,"known":{known}}},{{"id":2,"tribe":3,"stars":5,"known":{known}}}],
           "tiles":[{}],
           "improvements":[{{"x":0,"y":0,"level":1}},{{"x":4,"y":0,"level":1}}],
           "units":[{{"id":1,"type":{WARRIOR},"owner":1,"x":1,"y":1}},
                    {{"id":2,"type":{CLOAK},"owner":2,"x":2,"y":1,"flags":["invisible"]}}]}}"#,
        tiles.join(",")
    );
    Engine::from_position(&Json::parse(&text).unwrap()).unwrap()
}

#[test]
fn a_move_onto_a_hidden_unit_leaves_the_mover_where_it_was() {
    let mut e = board(true);
    assert!(e.unit(2).unwrap().invisible);
    e.apply_unchecked(Command::Move { unit: 1, to: wc(2, 1) });
    let (mover, hidden) = (e.unit(1).unwrap(), e.unit(2).unwrap());
    assert_eq!(mover.coordinates, wc(1, 1), "the mover stays on its tile");
    assert!(!mover.moved, "the move is not spent");
    assert_eq!(hidden.coordinates, wc(2, 1));
    assert!(!hidden.invisible, "the unit is revealed");
    assert_eq!(mover.direction, Engine::direction_of(wc(1, 1), wc(2, 1)), "the mover turns to face it");
    assert_eq!(e.s.units.iter().filter(|u| u.coordinates == wc(2, 1)).count(), 1, "one unit on the tile");
}

#[test]
fn the_mover_can_move_again_after_the_bump() {
    let mut e = board(true);
    e.apply_unchecked(Command::Move { unit: 1, to: wc(2, 1) });
    e.apply(Command::Move { unit: 1, to: wc(1, 2) }).expect("the unit still has its move");
    assert_eq!(e.unit(1).unwrap().coordinates, wc(1, 2));
}

#[test]
fn the_bump_introduces_an_unmet_owner() {
    let mut e = board(false);
    let stars = e.s.players.iter().find(|p| p.id == 1).unwrap().currency;
    e.apply_unchecked(Command::Move { unit: 1, to: wc(2, 1) });
    assert!(e.knows_player(1, 2), "player 1 has met player 2");
    assert!(e.s.players.iter().find(|p| p.id == 1).unwrap().currency > stars, "the meeting pays");
}

#[test]
fn the_bump_pays_nothing_between_players_that_have_met() {
    let mut e = board(true);
    let stars = e.s.players.iter().find(|p| p.id == 1).unwrap().currency;
    e.apply_unchecked(Command::Move { unit: 1, to: wc(2, 1) });
    assert_eq!(e.s.players.iter().find(|p| p.id == 1).unwrap().currency, stars);
}

/// Live games and the order of their commands: the id prefix of each record.
const GAMES: [&str; 8] =
    ["43cd83e2", "4b9b9877", "61645907", "80b78617", "8ce579a0", "d91c7861", "d9856064", "dd4bcdb2"];

fn games_dir() -> String {
    std::env::var("POLYTOPIA_CLIENT_GAMES").unwrap_or_default()
}

/// Replays the server's own command list of one game and compares every unit's tile, move and attack flags with the
/// server's final state. Returns false when the files are absent.
fn replay_keeps_units_where_the_server_does(prefix: &str) -> bool {
    let dir = games_dir();
    let Ok(entries) = std::fs::read_dir(&dir) else { return false };
    let Some(init_path) = entries.filter_map(|e| e.ok().map(|e| e.path())).find(|p| {
        let name = p.file_name().unwrap().to_string_lossy().into_owned();
        name.starts_with(prefix) && name.ends_with("_init.bin")
    }) else {
        return false;
    };
    let cur_path = init_path.to_string_lossy().replace("_init.bin", "_cur.bin");
    let (Ok(init), Ok(cur)) = (std::fs::read(&init_path), std::fs::read(&cur_path)) else { return false };
    let save = SaveFile::parse_server(&init, &cur).unwrap();
    let mut e = Engine::from_state(save.initial.clone());
    if save.commands.iter().any(|c| c.kind == 20) {
        e.start_match();
    }
    for c in &save.commands {
        if c.kind == 20 || c.player == 255 {
            continue;
        }
        let Some(cmd) = e.command_from_raw(c) else {
            e.s.current_command += 1;
            continue;
        };
        e.apply_unchecked(cmd);
    }
    for theirs in &save.current.units {
        let ours =
            e.s.units
                .iter()
                .find(|u| u.id == theirs.id)
                .unwrap_or_else(|| panic!("game {prefix}: unit {} is missing", theirs.id));
        assert_eq!(
            (ours.coordinates, ours.moved, ours.attacked),
            (theirs.coordinates, theirs.moved, theirs.attacked),
            "game {prefix}: unit {} (tile, moved, attacked)",
            theirs.id
        );
    }
    true
}

#[test]
fn live_games_keep_the_mover_off_the_hidden_unit_s_tile() {
    let mut run = 0;
    for prefix in GAMES {
        if replay_keeps_units_where_the_server_does(prefix) {
            run += 1;
        }
    }
    if run == 0 {
        eprintln!("skipped: no game records in {}", games_dir());
    }
}
