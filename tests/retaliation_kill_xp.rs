//! A kill earns the killer an xp point unless the killer is hidden, carries, or (from version 105) is Static. The
//! engine applied the Static exclusion to the attacker's kill but not to a defender whose retaliation kills the attacker.
use polytopia_sim::engine::{Command, Engine};
use polytopia_sim::json::Json;
use polytopia_sim::WorldCoordinates as Wc;

const WARRIOR: i64 = 2;
const KNIGHT: i64 = 4;
const DOOMUX: i64 = 29;

/// Player 1's warrior (id 1, 10 health) attacks player 2's unit (id 2) next to it; the retaliation kills the warrior.
fn retaliation_kill(defender_type: i64) -> Engine {
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
           "players":[{{"id":1,"tribe":3,"human":true,"stars":5}},{{"id":2,"tribe":3,"human":true,"stars":5}}],
           "tiles":[{}],
           "improvements":[{{"x":0,"y":0,"level":1}},{{"x":5,"y":2,"level":1}}],
           "units":[
             {{"id":1,"type":{WARRIOR},"owner":1,"x":1,"y":1,"health":10}},
             {{"id":2,"type":{defender_type},"owner":2,"x":2,"y":1}}]}}"#,
        tiles.join(",")
    );
    let mut e = Engine::from_position(&Json::parse(&text).unwrap()).unwrap();
    e.apply_unchecked(Command::Attack { unit: 1, target: Wc { x: 2, y: 1 } });
    e
}

#[test]
fn a_static_defender_gets_no_xp_for_a_retaliation_kill() {
    let e = retaliation_kill(DOOMUX);
    assert!(e.s.units.iter().all(|u| u.id != 1), "the retaliation killed the attacker");
    assert_eq!(e.s.units.iter().find(|u| u.id == 2).unwrap().xp, 0);
}

#[test]
fn another_defender_gets_xp_for_a_retaliation_kill() {
    let e = retaliation_kill(KNIGHT);
    assert!(e.s.units.iter().all(|u| u.id != 1), "the retaliation killed the attacker");
    assert_eq!(e.s.units.iter().find(|u| u.id == 2).unwrap().xp, 1);
}
