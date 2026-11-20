//! A unit that captures a city is no longer invisible.
//!
//! Retail's capture command queues a reveal of the capturer when it is Invisible (from game version 113). A cloak that
//! takes a city therefore stands visible afterwards, the same as after examining ruins. In the Discord corpus six games
//! ended with a cloak the engine still had invisible after a capture.
use polytopia_sim::engine::{Command, Engine};
use polytopia_sim::json::Json;
use polytopia_sim::WorldCoordinates as Wc;

const CLOAK: i64 = 11;

fn board(version: i64) -> Engine {
    let mut tiles = Vec::new();
    for y in 0..3 {
        for x in 0..9 {
            let mut t = format!(r#""x":{x},"y":{y},"terrain":3,"climate":3"#);
            match (x, y) {
                (0, 0) => t.push_str(r#","city":true,"capital":true,"owner":1,"ruling":[0,0]"#),
                (8, 2) => t.push_str(r#","city":true,"capital":true,"owner":2,"ruling":[8,2]"#),
                (4, 1) => t.push_str(r#","city":true"#),
                _ => {}
            }
            tiles.push(format!("{{{t}}}"));
        }
    }
    let text = format!(
        r#"{{"format":"polytopia-position","game":{{"turn":6,"width":9,"height":3,"current_player":1,"version":{version}}},
           "players":[{{"id":1,"tribe":0,"human":true,"stars":5}},{{"id":2,"tribe":4,"stars":5}}],
           "tiles":[{}],
           "improvements":[{{"x":0,"y":0,"level":1}},{{"x":8,"y":2,"level":1}}],
           "units":[{{"id":1,"type":{CLOAK},"owner":1,"x":4,"y":1,"health":100,"flags":["invisible"]}}]}}"#,
        tiles.join(",")
    );
    Engine::from_position(&Json::parse(&text).unwrap()).unwrap()
}

#[test]
fn a_cloak_that_captures_a_city_is_revealed() {
    let mut e = board(124);
    assert!(e.unit(1).unwrap().invisible, "the cloak starts invisible");
    e.apply(Command::Capture { unit: 1, at: Wc { x: 4, y: 1 } }).expect("the capture is legal");
    assert_eq!(e.tile(Wc { x: 4, y: 1 }).unwrap().owner, 1);
    assert!(!e.unit(1).unwrap().invisible, "capturing reveals the cloak");
}
