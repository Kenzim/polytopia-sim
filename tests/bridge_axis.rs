//! A land unit can step onto a bridge over water only along the bridge or diagonally: a bridge whose two vertical
//! neighbours are land cannot be entered from its left or right, and one whose horizontal neighbours are land cannot
//! be entered from above or below. When both pairs are land the bridge runs vertically.
//!
//! These tests need no files.
use polytopia_sim::engine::{Command, Engine};
use polytopia_sim::json::Json;
use polytopia_sim::WorldCoordinates as Wc;

const WATER: i32 = 1;
const FIELD: i32 = 3;
const BRIDGE: i32 = 48;
const WARRIOR: i32 = 2;

/// A 7 by 7 board of water with a bridge at (3,3), land on `land` and a warrior of player 1 at `from`.
fn board(land: &[(i32, i32)], from: (i32, i32)) -> Engine {
    let mut tiles = Vec::new();
    for y in 0..7 {
        for x in 0..7 {
            let terrain = if land.contains(&(x, y)) || (x, y) == from || (x, y) == (0, 0) || (x, y) == (6, 6) {
                FIELD
            } else {
                WATER
            };
            let mut t = format!(r#""x":{x},"y":{y},"terrain":{terrain},"climate":2,"explored":[1,2]"#);
            if (x, y) == (3, 3) {
                t.push_str(&format!(r#","improvement":{BRIDGE}"#));
            }
            if (x, y) == (0, 0) {
                t.push_str(r#","city":true,"capital":true,"owner":1,"ruling":[0,0]"#);
            } else if (x, y) == (6, 6) {
                t.push_str(r#","city":true,"capital":true,"owner":2,"ruling":[6,6]"#);
            }
            tiles.push(format!("{{{t}}}"));
        }
    }
    let (x, y) = from;
    let text = format!(
        r#"{{"format":"polytopia-position","game":{{"turn":3,"width":7,"height":7,"current_player":1}},
           "players":[{{"id":1,"tribe":2,"human":true,"stars":5,"known":[2]}},{{"id":2,"tribe":3,"stars":5,"known":[1]}}],
           "tiles":[{}],
           "improvements":[{{"x":0,"y":0,"level":1}},{{"x":6,"y":6,"level":1}}],
           "units":[{{"id":1,"type":{WARRIOR},"owner":1,"x":{x},"y":{y},"created":1}}]}}"#,
        tiles.join(",")
    );
    Engine::from_position(&Json::parse(&text).unwrap()).unwrap()
}

fn can_step_on_bridge(land: &[(i32, i32)], from: (i32, i32)) -> bool {
    board(land, from).is_legal(&Command::Move { unit: 1, to: Wc { x: 3, y: 3 } })
}

const VERTICAL: [(i32, i32); 2] = [(3, 2), (3, 4)];
const HORIZONTAL: [(i32, i32); 2] = [(2, 3), (4, 3)];

#[test]
fn a_vertical_bridge_is_entered_from_above_or_diagonally_but_not_from_the_side() {
    assert!(can_step_on_bridge(&VERTICAL, (3, 2)));
    assert!(can_step_on_bridge(&VERTICAL, (4, 2)));
    assert!(!can_step_on_bridge(&VERTICAL, (4, 3)));
    assert!(!can_step_on_bridge(&VERTICAL, (2, 3)));
}

#[test]
fn a_horizontal_bridge_is_entered_from_the_side_but_not_from_above() {
    assert!(can_step_on_bridge(&HORIZONTAL, (4, 3)));
    assert!(!can_step_on_bridge(&HORIZONTAL, (3, 2)));
    assert!(!can_step_on_bridge(&HORIZONTAL, (3, 4)));
}

#[test]
fn a_bridge_with_land_on_all_four_sides_runs_vertically() {
    let all = [VERTICAL, HORIZONTAL].concat();
    assert!(can_step_on_bridge(&all, (3, 2)));
    assert!(!can_step_on_bridge(&all, (4, 3)));
}
