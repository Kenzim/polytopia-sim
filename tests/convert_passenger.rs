//! Converting a boat converts the unit it carries.
//!
//! Retail's convert action sets the owner of the carried unit as well as the carrier's, and gives it the new capital as
//! its home. The engine left the passenger with its old owner, so when the converted boat later cleared a land tile
//! and let it off, it landed as an enemy of its own side and exerted a zone of control on it.
use polytopia_sim::engine::{Command, Engine};
use polytopia_sim::json::Json;
use polytopia_sim::WorldCoordinates as Wc;

const JUGGERNAUT: i64 = 45;
const GIANT: i64 = 12;
const MINDBENDER: i64 = 10;

fn board() -> Engine {
    let mut tiles = Vec::new();
    for y in 0..3 {
        for x in 0..6 {
            let mut t = format!(r#""x":{x},"y":{y},"terrain":{},"climate":2"#, if x >= 3 { 2 } else { 3 });
            match (x, y) {
                (0, 0) => t.push_str(r#","city":true,"capital":true,"owner":1,"ruling":[0,0]"#),
                (5, 2) => t.push_str(r#","city":true,"capital":true,"owner":2,"ruling":[5,2]"#),
                _ => {}
            }
            tiles.push(format!("{{{t}}}"));
        }
    }
    let text = format!(
        r#"{{"format":"polytopia-position","game":{{"turn":6,"width":6,"height":3,"current_player":1}},
           "players":[{{"id":1,"tribe":3,"human":true,"stars":5}},{{"id":2,"tribe":3,"stars":5}}],
           "tiles":[{}],
           "improvements":[{{"x":0,"y":0,"level":1}},{{"x":5,"y":2,"level":1}}],
           "units":[{{"id":1,"type":{MINDBENDER},"owner":1,"x":2,"y":1,"health":100}},
                    {{"id":2,"type":{JUGGERNAUT},"owner":2,"x":3,"y":1,"health":400,
                      "passenger":{{"id":3,"type":{GIANT},"owner":2,"x":3,"y":1,"health":400}}}}]}}"#,
        tiles.join(",")
    );
    Engine::from_position(&Json::parse(&text).unwrap()).unwrap()
}

#[test]
fn a_converted_boat_brings_its_passenger_over() {
    let mut e = board();
    e.apply(Command::Attack { unit: 1, target: Wc { x: 3, y: 1 } }).expect("the conversion is legal");
    let boat = e.unit(2).unwrap();
    assert_eq!(boat.owner, 1);
    let passenger = boat.passenger.as_deref().expect("the passenger is still aboard");
    assert_eq!((passenger.owner, passenger.home), (1, Wc { x: 0, y: 0 }));
}
