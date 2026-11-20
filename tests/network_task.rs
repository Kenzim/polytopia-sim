//! The Network task and the order of the city connection changes.
//!
//! Retail's connect action (and its counterpart for a city that switches capital) sets the city's connection flag
//! and then runs the Network check, once per city. The game runs the queued city actions last in, first out, so the
//! last city's connection is checked while an earlier city that is about to be disconnected still counts. A capture
//! that connects one city and cuts another therefore completes the task at the moment the count touches 5, although
//! the count after the whole update is 4.
//!
//! Evidence from the games: a monument build (Emperor's Tomb style, the Network task's) accepted by the server in the
//! command after a capture, with the engine at 4 connected cities after the update.
use polytopia_sim::engine::Engine;
use polytopia_sim::json::Json;
use polytopia_sim::TaskKind;

const NETWORK: &str = r#"{"kind":3,"started":true,"completed":false,"value":0}"#;

/// A 12x3 board for player 1 with the capital at (0,0): road along the top row to the cities (2,0), (4,0) and (6,0),
/// a road down the left column to a city at (0,2), and a city at (9,1) with no road that still carries the
/// connected flag. Player 2 is far away at (11,2). City order is row by row, so the cut city (9,1) comes before the new one (0,2) and the
/// game checks the new one first.
fn board(tasks: &str) -> Engine {
    let mut tiles = Vec::new();
    let mut improvements = Vec::new();
    for y in 0..3 {
        for x in 0..12 {
            let mut t = format!(r#""x":{x},"y":{y},"terrain":3,"climate":2"#);
            let city = matches!((x, y), (0, 0) | (2, 0) | (4, 0) | (6, 0) | (0, 2) | (9, 1) | (11, 2));
            let road = matches!((x, y), (1, 0) | (3, 0) | (5, 0) | (0, 1));
            match (x, y) {
                (0, 0) => t.push_str(r#","city":true,"capital":true,"owner":1,"ruling":[0,0]"#),
                (11, 2) => t.push_str(r#","city":true,"capital":true,"owner":2,"ruling":[11,2]"#),
                _ if city => t.push_str(&format!(r#","city":true,"owner":1,"ruling":[{x},{y}]"#)),
                _ => {}
            }
            if road {
                t.push_str(r#","road":true"#);
            }
            if city {
                let flag = if matches!((x, y), (2, 0) | (4, 0) | (6, 0) | (9, 1)) { 1 } else { 0 };
                improvements.push(format!(r#"{{"x":{x},"y":{y},"level":1,"connected":{flag}}}"#));
            }
            tiles.push(format!("{{{t}}}"));
        }
    }
    let text = format!(
        r#"{{"format":"polytopia-position","game":{{"turn":6,"width":12,"height":3,"current_player":1}},
           "players":[{{"id":1,"tribe":3,"human":true,"stars":5,"tasks":[{tasks}]}},{{"id":2,"tribe":3,"stars":5}}],
           "tiles":[{}],
           "improvements":[{}],
           "units":[]}}"#,
        tiles.join(","),
        improvements.join(",")
    );
    Engine::from_position(&Json::parse(&text).unwrap()).unwrap()
}

#[test]
fn a_connection_checked_before_a_disconnection_counts_the_city_about_to_be_cut() {
    let mut e = board(NETWORK);
    e.update_city_connections(1);
    let flag = |e: &Engine, x, y| {
        e.improvement_rt(polytopia_sim::WorldCoordinates { x, y }).unwrap().connected_to_capital_of_player
    };
    assert_eq!((flag(&e, 0, 2), flag(&e, 9, 1)), (1, 0), "the new city is connected and the road-less one is cut");
    assert!(e.task(1, TaskKind::Network).unwrap().is_completed);
}

#[test]
fn without_the_flagged_city_the_task_stays_open() {
    let mut e = board(NETWORK);
    e.improvement_rt_mut(polytopia_sim::WorldCoordinates { x: 9, y: 1 }).unwrap().connected_to_capital_of_player = 0;
    e.update_city_connections(1);
    assert!(!e.task(1, TaskKind::Network).unwrap().is_completed);
}
