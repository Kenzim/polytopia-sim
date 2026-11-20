//! A carrier is promoted through its passenger.
//!
//! Retail's promotion check and the promote action both read the passenger of a carrier when it has one: the
//! passenger's type, experience and promotion count decide, and the promotion goes on the passenger while the
//! carrier takes the full health.
use polytopia_sim::engine::{Command, Engine};
use polytopia_sim::json::Json;

const BOAT: i64 = 43;
const WARRIOR: i64 = 2;

fn board(ship_xp: i64, passenger_xp: i64, passenger_promotion: i64) -> Engine {
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
           "units":[{{"id":1,"type":{BOAT},"owner":1,"x":2,"y":1,"health":10,"xp":{ship_xp},
                     "passenger":{{"id":2,"type":{WARRIOR},"owner":1,"health":10,"xp":{passenger_xp},"promotion":{passenger_promotion}}}}}]}}"#,
        tiles.join(",")
    );
    Engine::from_position(&Json::parse(&text).unwrap()).unwrap()
}

fn promotes(e: &Engine) -> bool {
    e.legal_commands().iter().any(|c| matches!(c, Command::Promote { unit: 1 }))
}

#[test]
fn a_ship_carrying_an_experienced_unit_can_be_promoted() {
    assert!(promotes(&board(0, 3, 0)));
}

#[test]
fn the_ships_own_experience_does_not_count() {
    assert!(!promotes(&board(3, 0, 0)));
}

#[test]
fn a_carried_unit_that_is_already_promoted_cannot_be_promoted_again() {
    assert!(!promotes(&board(0, 3, 1)));
}

#[test]
fn promoting_puts_the_promotion_on_the_passenger_and_fills_the_ship() {
    let mut e = board(0, 3, 0);
    e.apply(Command::Promote { unit: 1 }).unwrap();
    let ship = e.unit(1).unwrap();
    assert_eq!(ship.promotion, 0);
    let rider = ship.passenger.as_deref().unwrap();
    assert_eq!(rider.promotion, 1);
    assert_eq!(i32::from(ship.health), e.max_health(ship));
    assert!(i32::from(ship.health) > 10);
}
