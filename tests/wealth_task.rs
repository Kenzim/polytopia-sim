//! The Wealth task and a disband refund.
//!
//! Retail's disband action pays the refund as one currency increase action per star, and each of those runs the
//! Wealth check, so a refund that takes the player to 100 stars completes the task and unlocks its monument.
//!
//! Evidence from the games: the first refused command of a corpus game was the Wealth monument (a Park of Fortune
//! style build) after a disband took the player past 100 stars.
use polytopia_sim::engine::{Command, Engine};
use polytopia_sim::json::Json;
use polytopia_sim::TaskKind;

const BARDUR: i64 = 3;
const WARRIOR: i64 = 2;
/// The tech that gives the Disband ability.
const FREE_SPIRIT: i32 = 2;

/// A 6x3 board: player 1 (`tribe`, to move) has the capital at (0,0), player 2 (Bardur) at (5,0) and a city at (3,1)
/// of `city_level`.
fn board(tribe: i64, tasks: &str, units: &str, city_level: u32) -> Engine {
    let mut tiles = Vec::new();
    for y in 0..3 {
        for x in 0..6 {
            let mut t = format!(r#""x":{x},"y":{y},"terrain":3,"climate":2"#);
            match (x, y) {
                (0, 0) => t.push_str(r#","city":true,"capital":true,"owner":1,"ruling":[0,0]"#),
                (5, 0) => t.push_str(r#","city":true,"capital":true,"owner":2,"ruling":[5,0]"#),
                (3, 1) => t.push_str(r#","city":true,"owner":2,"ruling":[3,1]"#),
                _ => {}
            }
            tiles.push(format!("{{{t}}}"));
        }
    }
    let text = format!(
        r#"{{"format":"polytopia-position","game":{{"turn":6,"width":6,"height":3,"current_player":1}},
           "players":[{{"id":1,"tribe":{tribe},"human":true,"stars":5,"tasks":[{tasks}]}},{{"id":2,"tribe":{BARDUR},"stars":5}}],
           "tiles":[{}],
           "improvements":[{{"x":0,"y":0,"level":1}},{{"x":5,"y":0,"level":1}},{{"x":3,"y":1,"level":{city_level}}}],
           "units":[{units}]}}"#,
        tiles.join(",")
    );
    Engine::from_position(&Json::parse(&text).unwrap()).unwrap()
}

fn with_stars(stars: i32) -> Engine {
    let units = format!(r#"{{"id":1,"type":{WARRIOR},"owner":1,"x":1,"y":1,"health":100}}"#);
    let mut e = board(BARDUR, r#"{"kind":4,"started":true,"completed":false,"value":0}"#, &units, 1);
    e.grant_tech(1, FREE_SPIRIT);
    e.s.players.iter_mut().find(|p| p.id == 1).unwrap().currency = stars;
    e
}

#[test]
fn a_disband_refund_that_reaches_100_stars_completes_wealth() {
    let mut e = with_stars(99);
    e.apply(Command::Disband { unit: 1 }).expect("the disband is legal");
    assert_eq!(e.s.players.iter().find(|p| p.id == 1).unwrap().currency, 100);
    assert!(e.task(1, TaskKind::Wealth).unwrap().is_completed);
}

#[test]
fn a_disband_refund_short_of_100_stars_leaves_wealth() {
    let mut e = with_stars(98);
    e.apply(Command::Disband { unit: 1 }).expect("the disband is legal");
    assert!(!e.task(1, TaskKind::Wealth).unwrap().is_completed);
}
