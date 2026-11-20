//! A swimming unit may always enter shallow water.
//!
//! Retail's allowed-terrain list for a unit adds shallow water for a unit with the Swim ability whatever its owner has
//! researched. A boat converted by a player without Fishing still sails; the engine left it stranded.
use polytopia_sim::engine::Engine;
use polytopia_sim::json::Json;
use polytopia_sim::WorldCoordinates as Wc;

const TRANSPORTSHIP: i64 = 43;
const WARRIOR: i64 = 2;

fn board() -> Engine {
    let mut tiles = Vec::new();
    for y in 0..3 {
        for x in 0..6 {
            let mut t = format!(r#""x":{x},"y":{y},"terrain":{},"climate":2"#, if x >= 2 { 1 } else { 3 });
            match (x, y) {
                (0, 0) => t.push_str(r#","city":true,"capital":true,"owner":1,"ruling":[0,0]"#),
                (0, 2) => t.push_str(r#","city":true,"capital":true,"owner":2,"ruling":[0,2]"#),
                _ => {}
            }
            tiles.push(format!("{{{t}}}"));
        }
    }
    let text = format!(
        r#"{{"format":"polytopia-position","game":{{"turn":6,"width":6,"height":3,"current_player":1}},
           "players":[{{"id":1,"tribe":4,"human":true,"stars":5}},{{"id":2,"tribe":4,"stars":5}}],
           "tiles":[{}],
           "improvements":[{{"x":0,"y":0,"level":1}},{{"x":0,"y":2,"level":1}}],
           "units":[{{"id":1,"type":{TRANSPORTSHIP},"owner":1,"x":3,"y":1,"health":100}},
                    {{"id":2,"type":{WARRIOR},"owner":1,"x":1,"y":1,"health":100}}]}}"#,
        tiles.join(",")
    );
    Engine::from_position(&Json::parse(&text).unwrap()).unwrap()
}

#[test]
fn a_boat_sails_without_the_owner_having_fishing() {
    let e = board();
    let boat = e.unit(1).unwrap();
    assert!(e.allowed_terrain(boat).contains(&1));
    assert!(e.move_destinations(boat).contains(&Wc { x: 4, y: 1 }));
}

#[test]
fn a_land_unit_still_needs_the_tech_for_water() {
    let e = board();
    assert!(!e.allowed_terrain(e.unit(2).unwrap()).contains(&1));
}
