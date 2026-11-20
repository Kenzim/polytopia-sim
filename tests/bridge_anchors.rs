//! A bridge needs a tile that is not water on both sides of the water tile, left and right or above and below.
//! Another bridge is no anchor: it sits on water. Retail's anchor check looks only at the terrain of the two tiles.
//!
//! Evidence from the game: online game 510ac27d (game version 123, Might, small map). The server refused
//! `Build Bridge` at (8,5) for player 1 with "Can't build" on turns 40, 41 and 42, while the engine listed it. The
//! tile had a bridge on its left (7,5) and a mountain on its right (9,5), a mountain above (8,4) and a port on
//! water below (8,6). The replay test reads the saved position from `POLYTOPIA_REJECTED`, and skips
//! without it.
use polytopia_sim::engine::replay::SaveFile;
use polytopia_sim::engine::{Command, Engine};
use polytopia_sim::json::Json;
use polytopia_sim::WorldCoordinates as Wc;

const BRIDGE: i32 = 48;

fn wc(x: i32, y: i32) -> Wc {
    Wc { x, y }
}

fn bridge(x: i32, y: i32) -> Command {
    Command::Build { at: wc(x, y), improvement: BRIDGE }
}

/// Player 1 (to move) and player 2 on a 5x3 board. Row 1 is field, water at (1,1) and (2,1), field again from x=3.
/// (1,1) carries a bridge (field above and below it); (2,1) has water below it at (2,2) when `water_below`.
fn board(bridge_at_1_1: bool, water_below: bool) -> Engine {
    let mut tiles = Vec::new();
    for y in 0..3 {
        for x in 0..5 {
            let water = (y == 1 && (x == 1 || x == 2)) || (y == 2 && x == 2 && water_below);
            let mut t = format!(r#""x":{x},"y":{y},"terrain":{},"climate":2"#, if water { 1 } else { 3 });
            match (x, y) {
                (0, 0) => t.push_str(r#","city":true,"capital":true,"owner":1,"ruling":[0,0]"#),
                (4, 2) => t.push_str(r#","city":true,"capital":true,"owner":2,"ruling":[4,2]"#),
                (1, 1) if bridge_at_1_1 => t.push_str(&format!(r#","improvement":{BRIDGE}"#)),
                _ => {}
            }
            tiles.push(format!("{{{t}}}"));
        }
    }
    let text = format!(
        r#"{{"format":"polytopia-position","game":{{"turn":6,"width":5,"height":3,"current_player":1}},
           "players":[{{"id":1,"tribe":3,"human":true,"stars":50,"tech":[1,2,3,4,5,6,7,8,9,10,11,12,13,14,15,16,17,18,19,20,21,22,23,24,25,26,27,28,29,30]}},{{"id":2,"tribe":2,"stars":5}}],
           "tiles":[{}],
           "improvements":[{{"x":0,"y":0,"level":1}},{{"x":4,"y":2,"level":1}}],
           "units":[]}}"#,
        tiles.join(",")
    );
    Engine::from_position(&Json::parse(&text).unwrap()).unwrap()
}

#[test]
fn a_bridge_is_built_between_two_land_tiles() {
    let e = board(false, true);
    assert!(e.legal_commands().contains(&bridge(1, 1)), "(1,1) has field above and below");
}

#[test]
fn a_bridge_does_not_anchor_another_bridge() {
    // (2,1) has a bridge on its left and field on its right, and water below: only a bridge anchors it.
    let e = board(true, true);
    assert!(!e.legal_commands().contains(&bridge(2, 1)), "a bridge is no anchor");
    // With field below (2,1) instead of water, the vertical pair is field again and the bridge is legal.
    let e = board(true, false);
    assert!(e.legal_commands().contains(&bridge(2, 1)), "field above and below");
}

fn rejected_dir() -> String {
    std::env::var("POLYTOPIA_REJECTED").unwrap_or_default()
}

#[test]
fn replay_of_the_refused_bridge_does_not_offer_it() {
    const GAME: &str = "510ac27d-72d5-4daa-251a-08df37a3787e-1443";
    let dir = rejected_dir();
    let (Ok(init), Ok(cur)) =
        (std::fs::read(format!("{dir}/{GAME}_init.bin")), std::fs::read(format!("{dir}/{GAME}_cur.bin")))
    else {
        eprintln!("skipped: blobs of {GAME} absent in {dir}");
        return;
    };
    let save = SaveFile::parse_server(&init, &cur).unwrap();
    // The first 1443 commands; the server refused the bridge at index 1443 (turn 40), so that slot holds an EndTurn.
    let mut e = Engine::from_state(save.initial.clone());
    if save.commands.iter().any(|c| c.kind == 20) {
        e.start_match();
    }
    for c in &save.commands[..1443] {
        if c.kind == 20 || c.player == 255 {
            continue;
        }
        if c.kind == 22 {
            if e.end_match().is_err() {
                e.s.current_command += 1;
            }
            continue;
        }
        let Some(cmd) = e.command_from_raw(c) else {
            e.s.current_command += 1;
            continue;
        };
        e.apply_unchecked(cmd);
    }
    assert_eq!(e.current_player(), 1);
    let at = |x, y| e.tile(wc(x, y)).unwrap();
    assert_eq!(at(7, 5).improvement, BRIDGE, "a bridge on the left of the tile");
    assert_eq!(at(9, 5).terrain, 4, "a mountain on its right");
    assert!(!e.legal_commands().contains(&bridge(8, 5)), "the server refused this bridge");
}
