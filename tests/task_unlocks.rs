//! Pacifist, Metropolis and the tasks a starting tech lists.
//!
//! Retail's attack command starts the Pacifist streak again only when the battle result says so (damage to a visible
//! unit), so a mindbender's conversion, which does no damage, leaves it. A captured city above level 1 enables and
//! checks the capturer's Metropolis task, so taking a level 5 city completes it. Starting a match learns the tribe's
//! starting techs through the normal learn routine, so a tribe that starts with Philosophy has the Genius task.
//!
//! Evidence from the games: first refused commands of 41 monument builds in the human corpus, each the monument of a
//! task the engine had not completed (an Altar of Peace after a mindbender conversion, a Park of Fortune after taking a
//! level 5 city, a Tower of Wisdom for a Philosophy tribe).
use polytopia_sim::engine::{Command, Engine};
use polytopia_sim::json::Json;
use polytopia_sim::{TaskKind, WorldCoordinates as Wc};

const BARDUR: i64 = 3;
const LUXIDOOR: i64 = 2;
const WARRIOR: i64 = 2;
const MINDBENDER: i64 = 10;

fn wc(x: i32, y: i32) -> Wc {
    Wc { x, y }
}

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

#[test]
fn a_conversion_leaves_the_pacifist_streak() {
    let units = format!(
        r#"{{"id":1,"type":{MINDBENDER},"owner":1,"x":1,"y":1,"health":100}},{{"id":2,"type":{WARRIOR},"owner":2,"x":2,"y":1,"health":100}}"#
    );
    let mut e = board(BARDUR, r#"{"kind":1,"started":true,"completed":false,"value":4}"#, &units, 1);
    e.apply(Command::Attack { unit: 1, target: wc(2, 1) }).expect("the conversion is legal");
    assert_eq!(e.task(1, TaskKind::Pacifist).unwrap().turns, 4);
}

#[test]
fn capturing_a_level_5_city_completes_metropolis() {
    let units = format!(r#"{{"id":1,"type":{WARRIOR},"owner":1,"x":3,"y":1,"health":100}}"#);
    let mut e = board(BARDUR, "", &units, 5);
    assert!(e.task(1, TaskKind::Metropolis).is_none());
    e.apply(Command::Capture { unit: 1, at: wc(3, 1) }).expect("the capture is legal");
    let t = e.task(1, TaskKind::Metropolis).expect("the capture enables the task");
    assert!(t.is_completed);
}

#[test]
fn capturing_a_level_1_city_starts_no_task() {
    let units = format!(r#"{{"id":1,"type":{WARRIOR},"owner":1,"x":3,"y":1,"health":100}}"#);
    let mut e = board(BARDUR, "", &units, 1);
    e.apply(Command::Capture { unit: 1, at: wc(3, 1) }).expect("the capture is legal");
    assert!(e.task(1, TaskKind::Metropolis).is_none());
}

#[test]
fn a_philosophy_tribe_has_the_genius_task_from_the_start() {
    let mut e = board(LUXIDOOR, "", "", 1);
    e.start_match();
    let t = e.task(1, TaskKind::Genius).expect("the starting tech enables it");
    assert!(t.is_started && !t.is_completed);
    let mut other = board(BARDUR, "", "", 1);
    other.start_match();
    assert!(other.task(1, TaskKind::Genius).is_none());
}
