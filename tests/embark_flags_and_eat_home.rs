//! Three details of units that the corpus final-state comparison showed the engine had wrong.
//!
//! * The boat that embarking makes is a new unit made like any trained one, so its birth climate is 0 whatever the
//!   passenger's is; the passenger keeps its own.
//! * A land unit that kills a unit on a port and steps in carries the attack's flags onto the boat: the passenger is
//!   already moved and attacked when it boards.
//! * A segment that a centipede's Eat makes has no home (-1, -1), also when it appears on a city tile.
use polytopia_sim::engine::{Command, Engine, GameOptions, PlayerSetup};
use polytopia_sim::json::Json;
use polytopia_sim::WorldCoordinates as Wc;

const WARRIOR: i64 = 2;
const CENTIPEDE: i64 = 33;
const SEGMENT: i32 = 34;
const PORT: i32 = 8;
const CITY: i32 = 1;

fn wc(x: i32, y: i32) -> Wc {
    Wc { x, y }
}

fn position(terrain: impl Fn(i32, i32) -> i32, extra: impl Fn(i32, i32) -> String, units: &str, tribe: i64) -> Engine {
    let mut tiles = Vec::new();
    for y in 0..3 {
        for x in 0..6 {
            let mut t = format!(r#""x":{x},"y":{y},"terrain":{},"climate":2"#, terrain(x, y));
            match (x, y) {
                (0, 0) => t.push_str(r#","city":true,"capital":true,"owner":1,"ruling":[0,0]"#),
                (5, 2) => t.push_str(r#","city":true,"capital":true,"owner":2,"ruling":[5,2]"#),
                _ => t.push_str(&extra(x, y)),
            }
            tiles.push(format!("{{{t}}}"));
        }
    }
    let text = format!(
        r#"{{"format":"polytopia-position","game":{{"turn":6,"width":6,"height":3,"current_player":1}},
           "players":[{{"id":1,"tribe":{tribe},"human":true,"stars":5}},{{"id":2,"tribe":3,"stars":5}}],
           "tiles":[{}],
           "improvements":[{{"x":0,"y":0,"level":1}},{{"x":5,"y":2,"level":1}}],
           "units":[{units}]}}"#,
        tiles.join(",")
    );
    Engine::from_position(&Json::parse(&text).unwrap()).unwrap()
}

fn boat_of(e: &Engine, passenger: u32) -> Option<&polytopia_sim::UnitState> {
    e.s.units.iter().find(|u| u.passenger.as_ref().is_some_and(|p| p.id == passenger))
}

#[test]
fn an_embarking_boat_has_birth_climate_zero_and_the_passenger_keeps_its_own() {
    let opts = GameOptions {
        players: vec![PlayerSetup { tribe: 2, human: true }, PlayerSetup { tribe: 3, human: false }],
        ..GameOptions::default()
    };
    let mut e = Engine::new_game(&opts);
    let i = e.s.units.iter().position(|u| u.owner == 1 && u.passenger.is_none()).expect("a unit of player 1");
    e.s.units[i].birth_climate = 5;
    let id = e.s.units[i].id;
    e.embark_pub(id);
    let boat = boat_of(&e, id).expect("the boat");
    assert_eq!(boat.birth_climate, 0);
    assert_eq!(boat.passenger.as_ref().map(|p| p.birth_climate), Some(5));
}

#[test]
fn a_unit_that_kills_on_a_port_and_steps_in_boards_already_moved_and_attacked() {
    let mut e = position(
        |x, y| if y == 1 && x >= 3 { 1 } else { 3 },
        |x, y| if (x, y) == (3, 1) { format!(r#","improvement":{PORT},"owner":1"#) } else { String::new() },
        &format!(
            r#"{{"id":1,"type":{WARRIOR},"owner":1,"x":2,"y":1,"health":100}},
               {{"id":2,"type":{WARRIOR},"owner":2,"x":3,"y":1,"health":1}}"#
        ),
        2,
    );
    let fishing = (0..200).find(|i| e.t.tech(*i).is_some_and(|t| t.id == "fishing")).expect("the fishing tech");
    e.s.tech.iter_mut().find(|t| t.player == 1).unwrap().tech.push(fishing);
    e.apply(Command::Attack { unit: 1, target: wc(3, 1) }).expect("the attack is legal");
    let boat = boat_of(&e, 1).expect("the warrior stepped onto the port and is carried");
    assert_eq!(boat.coordinates, wc(3, 1));
    let p = boat.passenger.as_deref().unwrap();
    assert!(p.moved && p.attacked, "the passenger boards with the attack's flags");
}

#[test]
fn a_segment_made_by_eating_has_no_home_even_on_a_city_tile() {
    let mut e = position(
        |_, _| 3,
        |x, y| if (x, y) == (4, 0) { format!(r#","improvement":{CITY},"owner":1"#) } else { String::new() },
        &format!(
            r#"{{"id":1,"type":{CENTIPEDE},"owner":1,"x":4,"y":0,"health":200}},
               {{"id":4,"type":{WARRIOR},"owner":2,"x":4,"y":1,"health":10}}"#
        ),
        17,
    );
    e.apply(Command::Attack { unit: 1, target: wc(4, 1) }).expect("the attack is legal");
    let seg = e.s.units.iter().find(|u| u.type_idx == SEGMENT).expect("a segment appeared");
    assert_eq!(seg.coordinates, wc(4, 0), "on the head's old tile, a city");
    assert_eq!(seg.home, wc(-1, -1));
}
