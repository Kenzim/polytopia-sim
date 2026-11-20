//! A defender retaliates only against a tile its owner has explored.
//!
//! Retail decides whether a defender hits back by scanning the tiles the defender could attack from where it stands,
//! and that scan skips every tile its owner has never explored. A ranged unit attacked from such a tile (two tiles
//! away, outside what its owner has ever seen) takes the damage and does not answer it.
//!
//! The replay tests are live game 5b870336 (an online game, game version 123, 15 November 2026): at
//! its last command the first file has player 2's archer shooting player 1's archer in its city from a tile player 1
//! had never seen. The server's archer kept its full health; the engine gave it a retaliation of 20.
//!
//! The files come from `POLYTOPIA_DIVERGENCE`. The replay tests skip without them.
use polytopia_sim::engine::replay::SaveFile;
use polytopia_sim::engine::{Command, Engine};
use polytopia_sim::json::Json;
use polytopia_sim::WorldCoordinates as Wc;

const ARCHER: i64 = 9;

fn wc(x: i32, y: i32) -> Wc {
    Wc { x, y }
}

/// Two archers on a 7x3 field of land: player 1's at (1,1) shoots player 2's at (3,1), two tiles away. Every tile is
/// explored by both players except (1,1), which player 2 has explored only when `shooter_seen_by_target`.
fn board(shooter_seen_by_target: bool) -> Engine {
    let mut tiles = Vec::new();
    for y in 0..3 {
        for x in 0..7 {
            let mut t = format!(r#""x":{x},"y":{y},"terrain":3,"climate":2"#);
            match (x, y) {
                (0, 0) => t.push_str(r#","city":true,"capital":true,"owner":1,"ruling":[0,0]"#),
                (6, 0) => t.push_str(r#","city":true,"capital":true,"owner":2,"ruling":[6,0]"#),
                _ => {}
            }
            let seen = if (x, y) == (1, 1) && !shooter_seen_by_target { "[1]" } else { "[1,2]" };
            t.push_str(&format!(r#","explored":{seen}"#));
            tiles.push(format!("{{{t}}}"));
        }
    }
    let text = format!(
        r#"{{"format":"polytopia-position","game":{{"turn":6,"width":7,"height":3,"current_player":1}},
           "players":[{{"id":1,"tribe":2,"human":true,"stars":5}},{{"id":2,"tribe":3,"stars":5}}],
           "tiles":[{}],
           "improvements":[{{"x":0,"y":0,"level":1}},{{"x":6,"y":0,"level":1}}],
           "units":[{{"id":1,"type":{ARCHER},"owner":1,"x":1,"y":1,"health":100}},
                    {{"id":2,"type":{ARCHER},"owner":2,"x":3,"y":1,"health":100}}]}}"#,
        tiles.join(",")
    );
    Engine::from_position(&Json::parse(&text).unwrap()).unwrap()
}

#[test]
fn a_target_that_has_explored_the_shooters_tile_retaliates() {
    let mut e = board(true);
    e.apply(Command::Attack { unit: 1, target: wc(3, 1) }).expect("the attack is legal");
    assert!(e.unit(2).unwrap().health < 100, "the target is hit");
    assert!(e.unit(1).unwrap().health < 100, "the target answers the shot: {:?}", e.unit(1));
}

#[test]
fn a_target_that_has_never_explored_the_shooters_tile_does_not_retaliate() {
    let mut e = board(false);
    e.apply(Command::Attack { unit: 1, target: wc(3, 1) }).expect("the attack is legal");
    assert!(e.unit(2).unwrap().health < 100, "the target is hit");
    assert_eq!(e.unit(1).unwrap().health, 100, "no answer from a tile its owner has never seen");
}

const GAME: &str = "5b870336-e71d-46c7-c364-08df4019957a";

fn divergence_dir() -> String {
    std::env::var("POLYTOPIA_DIVERGENCE").unwrap_or_default()
}

/// Replays the pair of files and checks every unit's tile, health and xp against the server's state.
fn replay_matches_server(stamp: &str) {
    let dir = divergence_dir();
    let (Ok(init), Ok(cur)) = (
        std::fs::read(format!("{dir}/{GAME}-{stamp}_init.bin")),
        std::fs::read(format!("{dir}/{GAME}-{stamp}_cur.bin")),
    ) else {
        eprintln!("skipped: game {GAME} blobs absent in {dir}");
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
        let Some(cmd) = e.command_from_raw(c) else {
            e.s.current_command += 1;
            continue;
        };
        e.apply_unchecked(cmd);
    }
    assert_eq!(e.s.units.len(), save.current.units.len(), "the same units");
    for theirs in &save.current.units {
        let ours =
            e.s.units.iter().find(|u| u.id == theirs.id).unwrap_or_else(|| panic!("unit {} is missing", theirs.id));
        assert_eq!(
            (ours.coordinates, ours.health, ours.xp),
            (theirs.coordinates, theirs.health, theirs.xp),
            "unit {} (tile, health, xp)",
            theirs.id
        );
    }
}

/// Right after player 2's archer (unit 12, moved and attacked) shot player 1's archer in its city.
#[test]
fn the_position_after_the_shot_matches_the_server() {
    replay_matches_server("1794783057");
}

/// The same game with more commands (unit 12 has not moved or attacked since).
#[test]
fn the_later_position_matches_the_server() {
    replay_matches_server("1794787550");
}
