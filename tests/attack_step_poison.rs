//! A boat that kills the unit on a Fungi tile steps onto it and lets its passenger off. The step is a move, so the
//! poison of the Fungi hits the unit then standing on the tile: the passenger. The engine looked the mover up by its id,
//! which a disembark has replaced by the passenger's, so it skipped the check and left the passenger healthy.
use polytopia_sim::engine::{Command, Engine};
use polytopia_sim::json::Json;
use polytopia_sim::WorldCoordinates as Wc;

const FUNGI: i64 = 37;
const WARRIOR: i64 = 2;
const SHIP: i32 = 44;

/// Player 1's warrior (id 1) stands on water at (1,1) and takes a boat there; player 2's warrior (id 2, 1 health)
/// stands on the Fungi at (2,1).
fn board() -> (Engine, u32) {
    let mut tiles = Vec::new();
    for y in 0..3 {
        for x in 0..6 {
            let extra = match (x, y) {
                (0, 0) => r#","city":true,"capital":true,"owner":1,"ruling":[0,0]"#.to_string(),
                (5, 2) => r#","city":true,"capital":true,"owner":2,"ruling":[5,2]"#.to_string(),
                (2, 1) => format!(r#","owner":2,"ruling":[5,2],"improvement":{FUNGI}"#),
                _ => String::new(),
            };
            let terrain = if (x, y) == (1, 1) { 1 } else { 3 };
            tiles.push(format!(r#"{{"x":{x},"y":{y},"terrain":{terrain},"climate":2{extra}}}"#));
        }
    }
    let text = format!(
        r#"{{"format":"polytopia-position","game":{{"turn":9,"width":6,"height":3,"current_player":1}},
           "players":[{{"id":1,"tribe":3,"human":true,"stars":5}},{{"id":2,"tribe":3,"human":true,"stars":5}}],
           "tiles":[{}],
           "improvements":[{{"x":0,"y":0,"level":1}},{{"x":5,"y":2,"level":1}}],
           "units":[
             {{"id":1,"type":{WARRIOR},"owner":1,"x":1,"y":1}},
             {{"id":2,"type":{WARRIOR},"owner":2,"x":2,"y":1,"health":1}}]}}"#,
        tiles.join(",")
    );
    let mut e = Engine::from_position(&Json::parse(&text).unwrap()).unwrap();
    e.embark_pub(1);
    let i = e.s.units.iter().position(|u| u.passenger.is_some()).unwrap();
    e.s.units[i].type_idx = SHIP;
    e.s.units[i].health = 100;
    let boat = e.s.units[i].id;
    (e, boat)
}

#[test]
fn a_passenger_let_off_by_a_kill_onto_a_fungi_is_poisoned() {
    let (mut e, boat) = board();
    e.apply_unchecked(Command::Attack { unit: boat, target: Wc { x: 2, y: 1 } });
    let landed = e.s.units.iter().find(|u| u.id == 1).expect("the passenger was let off");
    assert_eq!(landed.coordinates, Wc { x: 2, y: 1 });
    assert!(landed.poisoned);
}
