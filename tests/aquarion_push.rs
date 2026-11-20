//! A unit that a train pushes aside takes the rest of a move, not only the step: a crab floods the tile it lands on, an
//! amphibious unit gets the Bubble on a flooded tile and loses it on dry land, and a unit that lands on a poisonous
//! improvement is poisoned. (The tentacle part of the push is in `aquarion_tentacles.rs`.)
//!
//! The boards are 9x3 with the player 1 capital at (0, 0). A mermaid warrior is trained onto the tile of the unit at
//! (3, 1) and the unit steps to whichever neighbour the push picks, so every neighbour of that tile is made the same.
use polytopia_sim::engine::Engine;
use polytopia_sim::json::Json;
use polytopia_sim::WorldCoordinates as Wc;

const AQUARION: i64 = 3;
const CRAB: i64 = 26;
const MERMAID_WARRIOR: i64 = 46;
const AQUABASE: i64 = 45;
const WATERWAYS: i64 = 48;
const FLOODED: u16 = Engine::EFFECT_FLOODED;
const FUNGI: i64 = 37;

fn wc(x: i32, y: i32) -> Wc {
    Wc { x, y }
}

/// `around` is the extra json of the eight neighbours of (3, 1); `unit` is the json of the unit standing on (3, 1).
fn board(techs: &[i64], unit: &str, around: &str) -> Engine {
    let mut tiles = Vec::new();
    for y in 0..3i32 {
        for x in 0..9i32 {
            let mut t = format!(r#""x":{x},"y":{y},"terrain":3,"climate":3"#);
            match (x, y) {
                (0, 0) => t.push_str(r#","city":true,"capital":true,"owner":1,"ruling":[0,0]"#),
                (8, 2) => t.push_str(r#","city":true,"capital":true,"owner":2,"ruling":[8,2]"#),
                (1, 0) | (0, 1) | (1, 1) => t.push_str(r#","owner":1,"ruling":[0,0]"#),
                _ if (x - 3).abs() <= 1 && (y - 1).abs() <= 1 && (x, y) != (3, 1) => {
                    if !around.is_empty() {
                        t.push(',');
                        t.push_str(around);
                    }
                }
                _ => {}
            }
            tiles.push(format!("{{{t}}}"));
        }
    }
    let tech = std::iter::once(0).chain(techs.iter().copied()).map(|t| t.to_string()).collect::<Vec<_>>().join(",");
    let text = format!(
        r#"{{"format":"polytopia-position","game":{{"turn":6,"width":9,"height":3,"current_player":1}},
           "players":[{{"id":1,"tribe":{AQUARION},"human":true,"stars":5,"tech":[{tech}],"skin":17}},{{"id":2,"tribe":4,"stars":5}}],
           "tiles":[{}],
           "improvements":[{{"x":0,"y":0,"level":1}},{{"x":8,"y":2,"level":1}}],
           "units":[{unit}]}}"#,
        tiles.join(",")
    );
    Engine::from_position(&Json::parse(&text).unwrap()).unwrap()
}

fn unit(ty: i64, flags: &str) -> String {
    format!(r#"{{"id":1,"type":{ty},"owner":1,"x":3,"y":1,"health":100,"flags":[{flags}]}}"#)
}

/// Train a mermaid warrior onto (3, 1) and return where the unit that stood there went.
fn push(e: &mut Engine) -> Wc {
    e.train_on_occupied(1, MERMAID_WARRIOR as i32, wc(3, 1), wc(0, 0)).expect("the warrior is trained");
    let moved = e.unit(1).expect("the pushed unit survives").coordinates;
    assert_ne!(moved, wc(3, 1), "the unit stepped aside");
    moved
}

#[test]
fn a_crab_that_is_pushed_floods_the_tile_it_lands_on() {
    let mut e = board(&[AQUABASE], &unit(CRAB, ""), "");
    let to = push(&mut e);
    assert!(e.has_effect(to, FLOODED), "the destination floods");
    assert!(!e.has_effect(wc(3, 1), FLOODED), "the tile it left does not");
}

#[test]
fn an_amphibious_unit_that_is_pushed_onto_a_flooded_tile_gets_a_bubble() {
    let mut e = board(&[AQUABASE, WATERWAYS], &unit(MERMAID_WARRIOR, ""), r#""effects":[1]"#);
    let to = push(&mut e);
    assert!(e.has_effect(to, FLOODED));
    assert!(e.unit(1).unwrap().bubble);
}

#[test]
fn a_bubble_is_lost_when_the_unit_is_pushed_onto_dry_land() {
    let mut e = board(&[AQUABASE, WATERWAYS], &unit(MERMAID_WARRIOR, r#""bubble""#), "");
    push(&mut e);
    assert!(!e.unit(1).unwrap().bubble);
}

#[test]
fn a_unit_that_is_pushed_onto_a_poisonous_improvement_is_poisoned() {
    let around = format!(r#""improvement":{FUNGI},"owner":2,"ruling":[8,2]"#);
    let mut e = board(&[AQUABASE], &unit(MERMAID_WARRIOR, ""), &around);
    push(&mut e);
    assert!(e.unit(1).unwrap().poisoned);
}
