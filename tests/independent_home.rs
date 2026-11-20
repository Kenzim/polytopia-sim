//! Units with the `independent` ability (a Polytaur, a dagger) belong to no city. Capturing a village or city does not
//! make it their home, and the daggers an infiltration places start with no home, so none of them takes one of a
//! city's unit slots. In the recorded games every such unit ends with no home on the server, and a level 1 city
//! holding a Polytaur that had captured it could still train (an Elyrion player's Warrior on turn 5 of a 7-player
//! game).
//!
//! These tests need no files.
use polytopia_sim::engine::{Command, Engine, GameOptions};
use polytopia_sim::WorldCoordinates as Wc;

const CITY: i32 = 1;
const FIELD: i32 = 3;
const NO_HOME: Wc = Wc { x: -1, y: -1 };

fn type_of(e: &Engine, id: &str) -> i32 {
    (0..200).find(|i| e.t.unit(*i).is_some_and(|u| u.data.id == id)).unwrap_or_else(|| panic!("unit type {id}"))
}

/// Player 1 to move with a unit of `unit` type, home `home`, standing on a village nobody owns.
fn on_a_village(unit: &str, home: Wc) -> (Engine, u32, Wc) {
    let mut e = Engine::new_game(&GameOptions::default());
    assert_eq!(e.current_player(), 1);
    let village = e.s.map.tiles.iter().find(|t| t.owner == 0 && t.improvement == CITY).expect("a village").coordinates;
    e.s.units.retain(|u| u.coordinates != village);
    let ty = type_of(&e, unit);
    let id = e.spawn_unit(1, ty, village, home, false).expect("the capturer");
    (e, id, village)
}

fn capture(e: &mut Engine, unit: u32, at: Wc) {
    let cmd = Command::Capture { unit, at };
    assert!(e.legal_commands().contains(&cmd), "the capture is offered");
    e.apply(cmd).unwrap();
}

#[test]
fn a_capturing_polytaur_keeps_no_home() {
    let (mut e, id, village) = on_a_village("polytaur", NO_HOME);
    capture(&mut e, id, village);
    assert_eq!(e.tile(village).map(|t| t.owner), Some(1));
    assert_eq!(e.unit(id).map(|u| u.home), Some(NO_HOME));
    assert_eq!(e.city_unit_count(village), 0);
}

#[test]
fn a_capturing_warrior_moves_in() {
    let capital = Engine::new_game(&GameOptions::default()).capital_of(1).expect("a capital");
    let (mut e, id, village) = on_a_village("warrior", capital);
    capture(&mut e, id, village);
    assert_eq!(e.unit(id).map(|u| u.home), Some(village));
    assert_eq!(e.city_unit_count(village), 1);
}

#[test]
fn infiltration_daggers_have_no_home() {
    let mut e = Engine::new_game(&GameOptions::default());
    let city =
        e.s.map.tiles.iter().find(|t| t.owner == 2 && t.improvement == CITY).expect("a city of player 2").coordinates;
    let beside = Wc { x: if city.x > 0 { city.x - 1 } else { city.x + 1 }, y: city.y };
    e.s.units.retain(|u| u.coordinates != city && u.coordinates != beside);
    let t = e.s.map.tiles.iter_mut().find(|t| t.coordinates == beside).unwrap();
    t.terrain = FIELD;
    t.improvement = 0;
    t.has_ruin = false;
    let cloak = type_of(&e, "cloak");
    let id = e.spawn_unit(1, cloak, beside, beside, false).expect("the cloak");
    e.apply(Command::Attack { unit: id, target: city }).unwrap();

    let dagger = type_of(&e, "dagger");
    let daggers: Vec<Wc> = e.s.units.iter().filter(|u| u.owner == 1 && u.type_idx == dagger).map(|u| u.home).collect();
    assert!(!daggers.is_empty(), "the infiltration placed daggers");
    assert!(daggers.iter().all(|&h| h == NO_HOME), "{daggers:?}");
    assert_eq!(e.city_unit_count(city), 0);
}
