//! Promotion needs the player's experience limit, not a limit in the unit's data.
//!
//! A unit can be promoted once its experience reaches 3 (2 for a tribe with Discipline) and it has not been promoted.
//! The rules data lists a promotion limit per unit and the shark, jelly and others list 0; retail never reads it
//! in the promotion check, so those units promote like any other.
//!
//! Evidence from the games: an Aquarion human game promoted a shark that had three experience points; the engine
//! refused because the shark's data limit is 0.
use polytopia_sim::engine::{Command, Engine};
use polytopia_sim::json::Json;

const SHARK: i64 = 53;

fn board(xp: i64) -> Engine {
    let mut tiles = Vec::new();
    for y in 0..3 {
        for x in 0..5 {
            let terrain = if (x, y) == (2, 1) { 1 } else { 3 };
            let mut t = format!(r#""x":{x},"y":{y},"terrain":{terrain},"climate":3,"explored":[1,2]"#);
            match (x, y) {
                (0, 0) => t.push_str(r#","city":true,"capital":true,"owner":1,"ruling":[0,0]"#),
                (4, 2) => t.push_str(r#","city":true,"capital":true,"owner":2,"ruling":[4,2]"#),
                _ => {}
            }
            tiles.push(format!("{{{t}}}"));
        }
    }
    let text = format!(
        r#"{{"format":"polytopia-position","game":{{"turn":6,"width":5,"height":3,"current_player":1}},
           "players":[{{"id":1,"tribe":3,"human":true,"stars":5,"tech":[0,45],"skin":17}},{{"id":2,"tribe":4,"stars":5}}],
           "tiles":[{}],
           "improvements":[{{"x":0,"y":0,"level":1}},{{"x":4,"y":2,"level":1}}],
           "units":[{{"id":1,"type":{SHARK},"owner":1,"x":2,"y":1,"health":100,"xp":{xp}}}]}}"#,
        tiles.join(",")
    );
    Engine::from_position(&Json::parse(&text).unwrap()).unwrap()
}

fn promotes(e: &Engine) -> bool {
    e.legal_commands().iter().any(|c| matches!(c, Command::Promote { unit: 1 }))
}

#[test]
fn a_shark_with_three_experience_points_can_be_promoted() {
    assert!(promotes(&board(3)));
}

#[test]
fn a_shark_with_two_cannot() {
    assert!(!promotes(&board(2)));
}
