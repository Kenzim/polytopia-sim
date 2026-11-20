//! Training a water unit in a city that has only flooded land next to it.
//!
//! A unit with movement terrain can be trained in a city when a tile next to the city matches its terrain list. A tile
//! matches when its terrain is in the list, or when it is wetland (the Flooded or Algae effect) and the list holds wetland.
//! Aquarion's jelly and shark list wetland, so a city on flooded land trains them without any water beside it.
//!
//! Evidence from the games: in an Aquarion human game the capital stood among flooded fields and the player trained a
//! jelly there on two turns; the engine refused both and then lacked the unit for every later command.
use polytopia_sim::engine::{Command, Engine};
use polytopia_sim::json::Json;
use polytopia_sim::WorldCoordinates as Wc;

const JELLY: i32 = 52;
const SHARK: i32 = 53;
const AQUABASE: i64 = 45;
const MARINELIFE: i64 = 46;
const NAVIGATION: i64 = 14;

fn wc(x: i32, y: i32) -> Wc {
    Wc { x, y }
}

/// A 5x3 board of fields with player 1 (Aquarion) in the capital at (0,0); `extra` is json added to single tiles.
fn board(extra: &[(i32, i32, &str)]) -> Engine {
    let mut tiles = Vec::new();
    for y in 0..3 {
        for x in 0..5 {
            let terrain = if extra.iter().any(|e| (e.0, e.1) == (x, y) && e.2 == "water") { 1 } else { 3 };
            let mut t = format!(r#""x":{x},"y":{y},"terrain":{terrain},"climate":3"#);
            match (x, y) {
                (0, 0) => t.push_str(r#","city":true,"capital":true,"owner":1,"ruling":[0,0]"#),
                (4, 2) => t.push_str(r#","city":true,"capital":true,"owner":2,"ruling":[4,2]"#),
                _ => {}
            }
            for (ex, ey, frag) in extra {
                if (*ex, *ey) == (x, y) && *frag != "water" {
                    t.push(',');
                    t.push_str(frag);
                }
            }
            tiles.push(format!("{{{t}}}"));
        }
    }
    let text = format!(
        r#"{{"format":"polytopia-position","game":{{"turn":6,"width":5,"height":3,"current_player":1}},
           "players":[{{"id":1,"tribe":3,"human":true,"stars":20,"tech":[0,{AQUABASE},{MARINELIFE},{NAVIGATION}],"skin":17}},{{"id":2,"tribe":4,"stars":5}}],
           "tiles":[{}],
           "improvements":[{{"x":0,"y":0,"level":1}},{{"x":4,"y":2,"level":1}}],
           "units":[]}}"#,
        tiles.join(",")
    );
    Engine::from_position(&Json::parse(&text).unwrap()).unwrap()
}

fn can_train(e: &Engine, unit_type: i32) -> bool {
    e.legal_commands().contains(&Command::Train { city: wc(0, 0), unit_type })
}

#[test]
fn a_city_next_to_flooded_land_trains_water_units() {
    let e = board(&[(1, 0, r#""effects":[1]"#)]);
    assert!(can_train(&e, JELLY));
    assert!(can_train(&e, SHARK));
    let mut e = e;
    e.apply(Command::Train { city: wc(0, 0), unit_type: JELLY }).expect("the train is legal");
}

#[test]
fn algae_next_to_the_city_counts_as_wetland_too() {
    let e = board(&[(0, 1, r#""effects":[4]"#)]);
    assert!(can_train(&e, JELLY));
}

#[test]
fn dry_land_next_to_the_city_does_not() {
    let e = board(&[]);
    assert!(!can_train(&e, JELLY));
    assert!(!can_train(&e, SHARK));
}

#[test]
fn shallow_water_next_to_the_city_still_does() {
    let e = board(&[(1, 1, "water")]);
    assert!(can_train(&e, JELLY));
}
