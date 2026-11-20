//! The attack command sets the attacker's moved and attacked flags itself, before the queued attack actions run. A centipede
//! head killed by the retaliation therefore hands a follower that takes over as the new head a spent unit: it does not
//! rest-heal at its owner's turn end, as the engine had it do.
use polytopia_sim::engine::{Command, Engine};
use polytopia_sim::json::Json;
use polytopia_sim::WorldCoordinates as Wc;

const CENTIPEDE: i64 = 33;
const SEGMENT: i64 = 34;
const GIANT: i64 = 12;

/// Player 1's head (id 1, 10 health) with a 100 health segment (id 2) behind it faces player 2's giant (id 3).
fn board() -> Engine {
    let mut tiles = Vec::new();
    for y in 0..3 {
        for x in 0..6 {
            let city = match (x, y) {
                (0, 0) => r#","city":true,"capital":true,"owner":1,"ruling":[0,0]"#,
                (5, 2) => r#","city":true,"capital":true,"owner":2,"ruling":[5,2]"#,
                _ => "",
            };
            tiles.push(format!(r#"{{"x":{x},"y":{y},"terrain":3,"climate":2{city}}}"#));
        }
    }
    let text = format!(
        r#"{{"format":"polytopia-position","game":{{"turn":9,"width":6,"height":3,"current_player":1}},
           "players":[{{"id":1,"tribe":17,"human":true,"stars":5}},{{"id":2,"tribe":3,"human":true,"stars":5}}],
           "tiles":[{}],
           "improvements":[{{"x":0,"y":0,"level":1}},{{"x":5,"y":2,"level":1}}],
           "units":[
             {{"id":1,"type":{CENTIPEDE},"owner":1,"x":1,"y":1,"health":10,"follower":2}},
             {{"id":2,"type":{SEGMENT},"owner":1,"x":0,"y":1,"health":100,"leader":1}},
             {{"id":3,"type":{GIANT},"owner":2,"x":2,"y":1,"health":200}}]}}"#,
        tiles.join(",")
    );
    Engine::from_position(&Json::parse(&text).unwrap()).unwrap()
}

#[test]
fn a_follower_promoted_after_a_retaliation_kill_is_spent() {
    let mut e = board();
    let r = e.apply(Command::Attack { unit: 1, target: Wc { x: 2, y: 1 } });
    r.unwrap();
    assert!(e.s.units.iter().all(|u| u.id != 1), "the head died to the retaliation");
    let head = e.s.units.iter().find(|u| u.id == 2).unwrap();
    assert_eq!(head.type_idx, CENTIPEDE as i32);
    assert!(head.moved && head.attacked);
    let health = head.health;
    e.apply(Command::EndTurn).unwrap();
    assert_eq!(e.s.units.iter().find(|u| u.id == 2).unwrap().health, health, "no rest heal for a spent unit");
}
