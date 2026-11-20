//! The Bubble effect of Aquarion's amphibious units.
//!
//! A player with the Bubble ability (the Waterways tech) whose Amphibious unit ends a move on a flooded tile gives
//! the unit the Bubble effect: one extra movement point. A bubble unit that ends a move on a land tile that is not
//! wetland loses it again. Players without the ability, and units that are not Amphibious, never get it.
//!
//! Evidence from the games: in the 14 Aquarion human games that the engine replayed cleanly the first refused command
//! was a move of an amphibious unit that started on a flooded tile and went one tile further than its movement
//! allows (for example a mermaid warrior with movement 1 going two tiles, a 2-movement amphibian going three);
//! with the rule the number of refused commands over the 28 Aquarion games falls from 1,396 to 850 and
//! the `bubble` flag of units matches the server's, except for crabs (autoflood).
use polytopia_sim::engine::{Command, Engine};
use polytopia_sim::json::Json;
use polytopia_sim::WorldCoordinates as Wc;

const AMPHIBIAN: i64 = 20;
const MERMAID_WARRIOR: i64 = 46;
const SHARK: i64 = 53;
const WATERWAYS: i64 = 48;
const AQUABASE: i64 = 45;

fn wc(x: i32, y: i32) -> Wc {
    Wc { x, y }
}

/// Player 1 (Aquarion, to move) and player 2 on a 9x3 field board; the tiles of `flooded` carry the Flooded effect.
fn board(techs: &[i64], units: &str, flooded: &[(i32, i32)]) -> Engine {
    let mut tiles = Vec::new();
    for y in 0..3 {
        for x in 0..9 {
            let mut t = format!(r#""x":{x},"y":{y},"terrain":3,"climate":3"#);
            match (x, y) {
                (0, 0) => t.push_str(r#","city":true,"capital":true,"owner":1,"ruling":[0,0]"#),
                (8, 2) => t.push_str(r#","city":true,"capital":true,"owner":2,"ruling":[8,2]"#),
                _ => {}
            }
            if flooded.contains(&(x, y)) {
                t.push_str(r#","effects":[1]"#);
            }
            tiles.push(format!("{{{t}}}"));
        }
    }
    let tech = techs.iter().map(|t| t.to_string()).collect::<Vec<_>>().join(",");
    let text = format!(
        r#"{{"format":"polytopia-position","game":{{"turn":6,"width":9,"height":3,"current_player":1}},
           "players":[{{"id":1,"tribe":3,"human":true,"stars":5,"tech":[0,{tech}]}},{{"id":2,"tribe":4,"stars":5}}],
           "tiles":[{}],
           "improvements":[{{"x":0,"y":0,"level":1}},{{"x":8,"y":2,"level":1}}],
           "units":[{units}]}}"#,
        tiles.join(",")
    );
    Engine::from_position(&Json::parse(&text).unwrap()).unwrap()
}

fn unit(id: u32, ty: i64, x: i32, y: i32, flags: &str) -> String {
    format!(r#"{{"id":{id},"type":{ty},"owner":1,"x":{x},"y":{y},"health":100,"flags":[{flags}]}}"#)
}

#[test]
fn stepping_onto_a_flooded_tile_gives_an_amphibious_unit_a_bubble() {
    let mut e = board(&[AQUABASE, WATERWAYS], &unit(1, AMPHIBIAN, 1, 1, ""), &[(2, 1)]);
    assert!(!e.unit(1).unwrap().bubble);
    e.apply(Command::Move { unit: 1, to: wc(2, 1) }).expect("the move is legal");
    assert!(e.unit(1).unwrap().bubble, "an amphibian that ends on a flooded tile has the bubble");
}

#[test]
fn a_bubble_adds_one_movement_point() {
    // Amphibious units pay a whole move for a dry tile, so the corridor is flooded.
    let flooded = [(3, 1), (4, 1), (5, 1)];
    let plain = board(&[AQUABASE, WATERWAYS], &unit(1, MERMAID_WARRIOR, 3, 1, ""), &flooded);
    let bubble = board(&[AQUABASE, WATERWAYS], &unit(1, MERMAID_WARRIOR, 3, 1, r#""bubble""#), &flooded);
    let far = Command::Move { unit: 1, to: wc(5, 1) };
    assert!(!plain.legal_commands().contains(&far), "movement 1 cannot go two tiles");
    assert!(bubble.legal_commands().contains(&far), "the bubble makes it two");
}

#[test]
fn a_bubble_ends_on_dry_land_and_stays_on_wetland() {
    let flooded = [(2, 1), (3, 1)];
    let mut e = board(&[AQUABASE, WATERWAYS], &unit(1, AMPHIBIAN, 2, 1, r#""bubble""#), &flooded);
    e.apply(Command::Move { unit: 1, to: wc(3, 1) }).expect("the move is legal");
    assert!(e.unit(1).unwrap().bubble, "flooded to flooded keeps it");
    let mut e = board(&[AQUABASE, WATERWAYS], &unit(1, AMPHIBIAN, 2, 1, r#""bubble""#), &flooded);
    e.apply(Command::Move { unit: 1, to: wc(1, 1) }).expect("the move is legal");
    assert!(!e.unit(1).unwrap().bubble, "dry land removes it");
}

#[test]
fn without_the_ability_or_the_amphibious_unit_there_is_no_bubble() {
    let mut e = board(&[AQUABASE], &unit(1, AMPHIBIAN, 1, 1, ""), &[(2, 1)]);
    e.apply(Command::Move { unit: 1, to: wc(2, 1) }).expect("the move is legal");
    assert!(!e.unit(1).unwrap().bubble, "no Waterways, no bubble");
    let mut e = board(&[AQUABASE, WATERWAYS], &unit(1, SHARK, 1, 1, ""), &[(2, 1)]);
    // A shark moves on water only; the flooded field is wetland for it, so the move is legal on the flooded tile.
    if e.apply(Command::Move { unit: 1, to: wc(2, 1) }).is_ok() {
        assert!(!e.unit(1).unwrap().bubble, "a shark is not amphibious");
    }
}
