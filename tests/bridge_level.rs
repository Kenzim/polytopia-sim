//! A bridge's runtime record keeps its orientation in the level field: 1 when the bridge runs horizontally, else 0.
//! Retail's build action stores whether the bridge's direction is 5, and its direction is 3 when the tiles above and
//! below are anchors (a vertical bridge, checked first), else 5 when the tiles left and right are, else 8. In the
//! saved final states of the corpus, 1,270 of 2,707 bridge records have level 1.
use polytopia_sim::engine::{Command, Engine};
use polytopia_sim::json::Json;
use polytopia_sim::WorldCoordinates as Wc;

const BRIDGE: i32 = 48;

fn wc(x: i32, y: i32) -> Wc {
    Wc { x, y }
}

/// Player 1 (to move) and player 2 on a 5x3 board. Row 1 is field except for water at (1,1); the tiles above and
/// below (1,1) are water when `water_above_below`, else field.
fn board(water_above_below: bool) -> Engine {
    let mut tiles = Vec::new();
    for y in 0..3 {
        for x in 0..5 {
            let water = x == 1 && (y == 1 || water_above_below);
            let mut t = format!(r#""x":{x},"y":{y},"terrain":{},"climate":2"#, if water { 1 } else { 3 });
            match (x, y) {
                (0, 0) => t.push_str(r#","city":true,"capital":true,"owner":1,"ruling":[0,0]"#),
                (4, 2) => t.push_str(r#","city":true,"capital":true,"owner":2,"ruling":[4,2]"#),
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

fn built_level(water_above_below: bool) -> u16 {
    let mut e = board(water_above_below);
    e.apply(Command::Build { at: wc(1, 1), improvement: BRIDGE }).unwrap();
    e.improvement_rt(wc(1, 1)).expect("the bridge has a record").level
}

#[test]
fn a_bridge_between_land_left_and_right_has_level_1() {
    assert_eq!(built_level(true), 1);
}

#[test]
fn a_bridge_with_land_above_and_below_has_level_0() {
    assert_eq!(built_level(false), 0);
}
