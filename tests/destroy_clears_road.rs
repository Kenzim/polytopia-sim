//! Retail's destroy action clears the tile's road as well as its improvement, but only for a route opener or a bridge.
use polytopia_sim::engine::{Command, Engine};
use polytopia_sim::json::Json;
use polytopia_sim::WorldCoordinates as Wc;

const FARM: i64 = 5;
const MYCELIUM: i64 = 39;

fn board(improvement: i64) -> Engine {
    let mut tiles = Vec::new();
    for y in 0..3 {
        for x in 0..6 {
            let extra = match (x, y) {
                (0, 0) => r#","city":true,"capital":true,"owner":1,"ruling":[0,0]"#.to_string(),
                (5, 2) => r#","city":true,"capital":true,"owner":2,"ruling":[5,2]"#.to_string(),
                (1, 1) => format!(r#","owner":1,"ruling":[0,0],"improvement":{improvement},"road":true"#),
                _ => String::new(),
            };
            tiles.push(format!(r#"{{"x":{x},"y":{y},"terrain":3,"climate":2{extra}}}"#));
        }
    }
    let text = format!(
        r#"{{"format":"polytopia-position","game":{{"turn":9,"width":6,"height":3,"current_player":1}},
           "players":[{{"id":1,"tribe":3,"human":true,"stars":5}},{{"id":2,"tribe":3,"human":true,"stars":5}}],
           "tiles":[{}],
           "improvements":[{{"x":0,"y":0,"level":1}},{{"x":5,"y":2,"level":1}}],
           "units":[]}}"#,
        tiles.join(",")
    );
    Engine::from_position(&Json::parse(&text).unwrap()).unwrap()
}

fn destroy(improvement: i64) -> bool {
    let mut e = board(improvement);
    let at = Wc { x: 1, y: 1 };
    assert!(e.tile(at).unwrap().has_road && e.tile(at).unwrap().improvement == improvement as i32);
    e.apply_unchecked(Command::Destroy { at });
    let t = e.tile(at).unwrap();
    assert_eq!(t.improvement, 0);
    t.has_road
}

#[test]
fn destroying_a_route_opener_clears_the_road_under_it() {
    assert!(!destroy(MYCELIUM));
}

#[test]
fn destroying_another_improvement_leaves_the_road() {
    assert!(destroy(FARM));
}
