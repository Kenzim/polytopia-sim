//! Converting units completes the Convert task.
//!
//! Retail's convert action bumps the player's started Convert task for every converted unit that did not belong to
//! Nature, and the task is complete at 3. A centipede chain converts head and segments together, so a chain of three
//! completes it with one attack. Cymanti games hold the Convert monument build that follows, which the engine refused.
use polytopia_sim::engine::{Command, Engine};
use polytopia_sim::json::Json;
use polytopia_sim::{TaskKind, WorldCoordinates as Wc};

const MINDBENDER: i64 = 10;
const CENTIPEDE: i64 = 33;
const SEGMENT: i64 = 34;

fn board(units: &str) -> Engine {
    let mut tiles = Vec::new();
    for y in 0..3 {
        for x in 0..8 {
            let mut t = format!(r#""x":{x},"y":{y},"terrain":3,"climate":2"#);
            match (x, y) {
                (0, 0) => t.push_str(r#","city":true,"capital":true,"owner":1,"ruling":[0,0]"#),
                (7, 2) => t.push_str(r#","city":true,"capital":true,"owner":2,"ruling":[7,2]"#),
                _ => {}
            }
            tiles.push(format!("{{{t}}}"));
        }
    }
    let text = format!(
        r#"{{"format":"polytopia-position","game":{{"turn":6,"width":8,"height":3,"current_player":1}},
           "players":[{{"id":1,"tribe":3,"human":true,"stars":5,"tasks":[{{"kind":9,"started":true,"completed":false,"value":0}}]}},{{"id":2,"tribe":17,"stars":5}}],
           "tiles":[{}],
           "improvements":[{{"x":0,"y":0,"level":1}},{{"x":7,"y":2,"level":1}}],
           "units":[{units}]}}"#,
        tiles.join(",")
    );
    Engine::from_position(&Json::parse(&text).unwrap()).unwrap()
}

fn convert_task(e: &Engine) -> (bool, i32) {
    let t = e.task(1, TaskKind::Convert).expect("player 1 has the Convert task");
    (t.is_completed, t.converts)
}

#[test]
fn a_converted_unit_counts_once() {
    let mut e = board(
        r#"{"id":1,"type":10,"owner":1,"x":1,"y":1,"health":100},
           {"id":2,"type":2,"owner":2,"x":2,"y":1,"health":100}"#,
    );
    assert_eq!(convert_task(&e), (false, 0));
    e.apply(Command::Attack { unit: 1, target: Wc { x: 2, y: 1 } }).expect("the conversion is legal");
    assert_eq!(e.unit(2).unwrap().owner, 1);
    assert_eq!(convert_task(&e), (false, 1));
}

#[test]
fn a_converted_chain_counts_every_unit_and_the_third_completes_the_task() {
    let mut e = board(&format!(
        r#"{{"id":1,"type":{MINDBENDER},"owner":1,"x":3,"y":1,"health":100}},
               {{"id":2,"type":{CENTIPEDE},"owner":2,"x":2,"y":1,"health":100,"follower":3}},
               {{"id":3,"type":{SEGMENT},"owner":2,"x":1,"y":1,"health":100,"leader":2,"follower":4}},
               {{"id":4,"type":{SEGMENT},"owner":2,"x":0,"y":1,"health":100,"leader":3}}"#
    ));
    e.apply(Command::Attack { unit: 1, target: Wc { x: 2, y: 1 } }).expect("the conversion is legal");
    assert_eq!(e.unit(4).unwrap().owner, 1, "the whole chain changes owner");
    assert_eq!(convert_task(&e), (true, 3));
}
