//! A poison infiltration poisons the defender that survives the hit.
//!
//! Retail's infiltration reward (game version 118 and later) fights the defender of the city with a probe of the
//! infiltrator's type; when the defender survives and the reward is the poison one, it queues a poison action on the
//! defender. The rebellion reward does not.
//!
//! These tests need no files.
use polytopia_sim::engine::{Command, Engine, GameOptions};
use polytopia_sim::WorldCoordinates as Wc;

const CITY: i32 = 1;
const FIELD: i32 = 3;

fn type_of(e: &Engine, id: &str) -> i32 {
    (0..200).find(|i| e.t.unit(*i).is_some_and(|u| u.data.id == id)).unwrap_or_else(|| panic!("unit type {id}"))
}

/// Whether the giant defending a city of player 2 ends poisoned after player 1's `infiltrator` attacks the city.
fn defender_poisoned_after(infiltrator: &str, version: i32) -> bool {
    let mut e = Engine::new_game(&GameOptions::default());
    e.s.version = version;
    let city =
        e.s.map.tiles.iter().find(|t| t.owner == 2 && t.improvement == CITY).expect("a city of player 2").coordinates;
    let beside = Wc { x: if city.x > 0 { city.x - 1 } else { city.x + 1 }, y: city.y };
    e.s.units.retain(|u| u.coordinates != city && u.coordinates != beside);
    let t = e.s.map.tiles.iter_mut().find(|t| t.coordinates == beside).unwrap();
    t.terrain = FIELD;
    t.improvement = 0;
    t.has_ruin = false;
    let kind = type_of(&e, infiltrator);
    let id = e.spawn_unit(1, kind, beside, beside, false).expect("the infiltrator");
    let giant = type_of(&e, "giant");
    e.spawn_unit(2, giant, city, city, false).expect("the giant");
    e.apply(Command::Attack { unit: id, target: city }).unwrap();
    let giant = e.unit_at(city).expect("the giant survives the hit");
    giant.poisoned
}

#[test]
fn a_poison_infiltration_poisons_the_surviving_defender() {
    assert!(defender_poisoned_after("moth", 124));
}

#[test]
fn a_rebellion_infiltration_leaves_the_defender_unpoisoned() {
    assert!(!defender_poisoned_after("cloak", 124));
}
