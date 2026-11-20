//! Where tiles flood and drain: the crab's autoflood, the canal, and the capture of a city.
//!
//! A tile floods only if it is floodable: land that is not a mountain and not already flooded (water, ocean, mountains
//! and ice never flood; an improvement or a city on it does not matter). A crab (autoflood) floods the tile it moves
//! onto, a canal floods the tile it is built on, and taking a city floods the city's tile for a Flooded tribe (Aquarion)
//! and drains it for every other tribe, whoever the city belonged to.
//!
//! Evidence from the games: in the 28 Aquarion human games the number of tiles whose effects differ from the
//! server's final grid falls from 153 (a crab that floods every tile it enters, water included) to 91 with these
//! rules, and the routes of the flooded tiles match in every game.
use polytopia_sim::engine::{Command, Engine};
use polytopia_sim::json::Json;
use polytopia_sim::WorldCoordinates as Wc;

const AQUARION: i64 = 3;
const BARDUR: i64 = 4;
const CRAB: i64 = 26;
const WARRIOR: i64 = 2;
const MERMAID_WARRIOR: i64 = 46;
const CANAL: i32 = 52;
const WATERWAYS: i64 = 48;
const AQUABASE: i64 = 45;
const FLOODED: u16 = Engine::EFFECT_FLOODED;
const SWAMPED: u16 = Engine::EFFECT_SWAMPED;

fn wc(x: i32, y: i32) -> Wc {
    Wc { x, y }
}

/// A 9x3 board. Player 1 (`tribe`, to move) owns the capital at (0,0) and the tiles beside it; player 2 (Bardur) has
/// the capital at (8,2). `tile` gives extra json for single tiles (or `terrain:N`), `techs` the player's techs.
fn board(tribe: i64, techs: &[i64], units: &str, tile: &[(i32, i32, &str)]) -> Engine {
    let mut tiles = Vec::new();
    for y in 0..3 {
        for x in 0..9 {
            let terrain = tile
                .iter()
                .find(|e| (e.0, e.1) == (x, y) && e.2.starts_with("terrain:"))
                .map_or(3, |e| e.2[8..].parse::<i32>().unwrap());
            let mut t = format!(r#""x":{x},"y":{y},"terrain":{terrain},"climate":3"#);
            match (x, y) {
                (0, 0) => t.push_str(r#","city":true,"capital":true,"owner":1,"ruling":[0,0]"#),
                (8, 2) => t.push_str(r#","city":true,"capital":true,"owner":2,"ruling":[8,2]"#),
                (1, 0) | (0, 1) | (1, 1) => t.push_str(r#","owner":1,"ruling":[0,0]"#),
                _ => {}
            }
            for (tx, ty, extra) in tile {
                if (*tx, *ty) == (x, y) && !extra.starts_with("terrain:") {
                    t.push(',');
                    t.push_str(extra);
                }
            }
            tiles.push(format!("{{{t}}}"));
        }
    }
    let tech = std::iter::once(0).chain(techs.iter().copied()).map(|t| t.to_string()).collect::<Vec<_>>().join(",");
    let skin = if tribe == AQUARION { 17 } else { 0 };
    let text = format!(
        r#"{{"format":"polytopia-position","game":{{"turn":6,"width":9,"height":3,"current_player":1}},
           "players":[{{"id":1,"tribe":{tribe},"human":true,"stars":5,"tech":[{tech}],"skin":{skin}}},{{"id":2,"tribe":4,"stars":5}}],
           "tiles":[{}],
           "improvements":[{{"x":0,"y":0,"level":1}},{{"x":8,"y":2,"level":1}}],
           "units":[{units}]}}"#,
        tiles.join(",")
    );
    Engine::from_position(&Json::parse(&text).unwrap()).unwrap()
}

fn unit(id: u32, ty: i64, x: i32, y: i32) -> String {
    format!(r#"{{"id":{id},"type":{ty},"owner":1,"x":{x},"y":{y},"health":100}}"#)
}

fn crab_board(tile: &[(i32, i32, &str)]) -> Engine {
    board(AQUARION, &[AQUABASE], &unit(1, CRAB, 3, 1), tile)
}

#[test]
fn a_crab_floods_the_field_it_moves_onto() {
    let mut e = crab_board(&[]);
    e.apply(Command::Move { unit: 1, to: wc(4, 1) }).expect("the move is legal");
    assert!(e.has_effect(wc(4, 1), FLOODED), "the destination floods");
    assert!(e.has_effect(wc(4, 1), SWAMPED), "and takes the swamp skin's effect");
    assert!(!e.has_effect(wc(3, 1), FLOODED), "the tile the crab left does not");
}

#[test]
fn a_crab_floods_a_forest_and_a_tile_with_an_improvement() {
    let mut e = crab_board(&[(4, 1, "terrain:5"), (2, 1, r#""improvement":3"#)]);
    e.apply(Command::Move { unit: 1, to: wc(4, 1) }).expect("the move is legal");
    assert!(e.has_effect(wc(4, 1), FLOODED), "forest floods");
    let mut e = crab_board(&[(2, 1, r#""improvement":3"#)]);
    e.apply(Command::Move { unit: 1, to: wc(2, 1) }).expect("the move is legal");
    assert!(e.has_effect(wc(2, 1), FLOODED), "an improvement does not stop the flood");
}

#[test]
fn a_crab_does_not_flood_water() {
    let mut e = crab_board(&[(4, 1, "terrain:1")]);
    e.apply(Command::Move { unit: 1, to: wc(4, 1) }).expect("the move is legal");
    assert!(!e.has_effect(wc(4, 1), FLOODED), "shallow water is not floodable");
}

#[test]
fn a_crab_on_a_flooded_tile_with_waterways_gets_a_bubble() {
    let mut e = board(AQUARION, &[AQUABASE, WATERWAYS], &unit(1, CRAB, 3, 1), &[]);
    e.apply(Command::Move { unit: 1, to: wc(4, 1) }).expect("the move is legal");
    assert!(e.has_effect(wc(4, 1), FLOODED));
    assert!(e.unit(1).unwrap().bubble, "the tile flooded under an amphibious unit");
}

#[test]
fn an_aquarion_capture_floods_the_city_tile() {
    let village = [(4, 1, r#""city":true"#)];
    let mut e = board(AQUARION, &[AQUABASE], &unit(1, MERMAID_WARRIOR, 4, 1), &village);
    e.apply(Command::Capture { unit: 1, at: wc(4, 1) }).expect("the capture is legal");
    assert!(e.has_effect(wc(4, 1), FLOODED));
    assert!(e.has_effect(wc(4, 1), SWAMPED));
}

#[test]
fn another_tribe_drains_a_flooded_neutral_village_it_takes() {
    let village = [(4, 1, r#""city":true,"effects":[1]"#)];
    let mut e = board(BARDUR, &[], &unit(1, WARRIOR, 4, 1), &village);
    assert!(e.has_effect(wc(4, 1), FLOODED));
    e.apply(Command::Capture { unit: 1, at: wc(4, 1) }).expect("the capture is legal");
    assert_eq!(e.tile(wc(4, 1)).unwrap().owner, 1);
    assert!(!e.has_effect(wc(4, 1), FLOODED), "the village tile drains when it is captured");
}

#[test]
fn building_a_canal_floods_its_tile() {
    let mut e = board(AQUARION, &[AQUABASE, WATERWAYS], &unit(1, MERMAID_WARRIOR, 4, 1), &[]);
    e.apply(Command::Build { at: wc(4, 1), improvement: CANAL }).expect("the canal can be built");
    assert!(e.has_effect(wc(4, 1), FLOODED));
    assert!(e.unit(1).unwrap().bubble, "the builder stands on a tile that flooded under it");
}

#[test]
fn mountains_and_ice_are_not_floodable() {
    for terrain in [4, 6] {
        let mut e = crab_board(&[(4, 1, &format!("terrain:{terrain}"))]);
        e.apply_unchecked(Command::Move { unit: 1, to: wc(4, 1) });
        assert!(!e.has_effect(wc(4, 1), FLOODED), "terrain {terrain} never floods");
    }
}
