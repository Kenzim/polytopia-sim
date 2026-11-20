//! Who embarks onto a port.
//!
//! A unit that is part of a centipede chain never embarks, and the port must belong to the unit's owner or to a player
//! at peace with it. A land unit stepping onto an enemy port stays a land unit on the tile; the engine used to put it
//! in a boat, which the server's later commands for that unit contradict. The owner must also know a technology that
//! lets units onto water (a land unit can reach an algae port without one).
use polytopia_sim::engine::Engine;
use polytopia_sim::json::Json;
use polytopia_sim::WorldCoordinates as Wc;

const PORT: i32 = 8;

/// Player 1 (tribe Bardur) has a warrior on (2,1) beside a port on (3,1) that belongs to `port_owner`; `extra` adds
/// units. Row 1 is water to the east, so the port counts as an embarking tile.
fn board(port_owner: u8, extra: &str) -> Engine {
    let mut tiles = Vec::new();
    for y in 0..3 {
        for x in 0..6 {
            let water = y == 1 && x >= 3;
            let mut t = format!(r#""x":{x},"y":{y},"terrain":{},"climate":2"#, if water { 1 } else { 3 });
            match (x, y) {
                (0, 0) => t.push_str(r#","city":true,"capital":true,"owner":1,"ruling":[0,0]"#),
                (5, 2) => t.push_str(r#","city":true,"capital":true,"owner":2,"ruling":[5,2]"#),
                (3, 1) => t.push_str(&format!(r#","improvement":{PORT},"owner":{port_owner}"#)),
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
           "units":[{{"id":1,"type":2,"owner":1,"x":2,"y":1,"health":100}}{extra}]}}"#,
        tiles.join(",")
    );
    let mut e = Engine::from_position(&Json::parse(&text).unwrap()).unwrap();
    let fishing = (0..200).find(|i| e.t.tech(*i).is_some_and(|t| t.id == "fishing")).expect("the fishing tech");
    e.s.tech.iter_mut().find(|t| t.player == 1).unwrap().tech.push(fishing);
    e
}

fn embarks(e: &Engine) -> i32 {
    e.will_embark(e.unit(1).unwrap(), Wc { x: 3, y: 1 })
}

#[test]
fn a_unit_embarks_onto_its_own_port() {
    assert_eq!(embarks(&board(1, "")), 1);
}

#[test]
fn a_unit_does_not_embark_onto_an_enemy_port() {
    assert_eq!(embarks(&board(2, "")), 0);
}

#[test]
fn a_chained_unit_does_not_embark() {
    let mut e = board(1, "");
    e.s.units[0].follower = 9;
    assert_eq!(embarks(&e), 0);
    e.s.units[0].follower = 0;
    e.s.units[0].leader = 9;
    assert_eq!(embarks(&e), 0);
}

#[test]
fn a_unit_without_a_water_technology_does_not_embark() {
    let mut e = board(1, "");
    e.s.tech.iter_mut().find(|t| t.player == 1).unwrap().tech.clear();
    assert_eq!(embarks(&e), 0);
}

#[test]
fn only_a_technology_that_unlocks_a_port_lets_units_embark() {
    let mut e = board(1, "");
    let freediving =
        (0..200).find(|i| e.t.tech(*i).is_some_and(|t| t.id == "freediving")).expect("the freediving tech");
    let techs = &mut e.s.tech.iter_mut().find(|t| t.player == 1).unwrap().tech;
    techs.clear();
    techs.push(freediving);
    assert_eq!(embarks(&e), 0, "freediving lets a unit onto ocean but unlocks no port");
}
