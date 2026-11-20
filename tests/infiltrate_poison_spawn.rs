//! Retail's train action, which the infiltration reward queues for each dagger it places, checks the tile for poison
//! the way a step does. A dagger placed on a tile with a poisonous improvement (a Clathrus) starts poisoned, which
//! takes the defence bonus off it and lets a retaliation kill what attacked it.
//!
//! These tests need no files.
use polytopia_sim::engine::{Command, Engine, GameOptions};
use polytopia_sim::WorldCoordinates as Wc;

const CITY: i32 = 1;
const FIELD: i32 = 3;

fn type_of(e: &Engine, id: &str) -> i32 {
    (0..200).find(|i| e.t.unit(*i).is_some_and(|u| u.data.id == id)).unwrap_or_else(|| panic!("unit type {id}"))
}

fn improvement_of(e: &Engine, id: &str) -> i32 {
    (0..200)
        .find(|i| e.t.improvement(*i).is_some_and(|x| x.data.id == id))
        .unwrap_or_else(|| panic!("improvement {id}"))
}

/// Player 1's daggers after a cloak of its own infiltrated a city of player 2 whose every other tile holds `improvement`.
fn infiltrate_over(improvement: &str) -> Vec<(Wc, bool)> {
    let mut e = Engine::new_game(&GameOptions::default());
    let city =
        e.s.map.tiles.iter().find(|t| t.owner == 2 && t.improvement == CITY).expect("a city of player 2").coordinates;
    let beside = Wc { x: if city.x > 0 { city.x - 1 } else { city.x + 1 }, y: city.y };
    e.s.units.retain(|u| u.coordinates != city && u.coordinates != beside);
    let kind = improvement_of(&e, improvement);
    for t in e.s.map.tiles.iter_mut().filter(|t| t.ruling_city == city && t.coordinates != city) {
        t.improvement = 0;
        t.has_ruin = false;
        if t.terrain != FIELD && t.terrain != 4 && t.terrain != 5 {
            continue;
        }
        t.improvement = kind;
    }
    let t = e.s.map.tiles.iter_mut().find(|t| t.coordinates == beside).unwrap();
    t.terrain = FIELD;
    t.improvement = 0;
    t.has_ruin = false;
    let cloak = type_of(&e, "cloak");
    let id = e.spawn_unit(1, cloak, beside, beside, false).expect("the cloak");
    // A defender that survives the probe keeps the city's own tile out of the spawn candidates.
    e.s.map.tiles.iter_mut().find(|t| t.coordinates == beside).unwrap().improvement = kind;
    let giant = type_of(&e, "giant");
    e.spawn_unit(2, giant, city, city, false).expect("the giant");
    e.apply(Command::Attack { unit: id, target: city }).unwrap();
    let dagger = type_of(&e, "dagger");
    e.s.units.iter().filter(|u| u.owner == 1 && u.type_idx == dagger).map(|u| (u.coordinates, u.poisoned)).collect()
}

#[test]
fn a_dagger_placed_on_a_clathrus_starts_poisoned() {
    let daggers = infiltrate_over("clathrus");
    assert!(!daggers.is_empty(), "the infiltration placed daggers");
    assert!(daggers.iter().all(|&(_, poisoned)| poisoned), "{daggers:?}");
}

#[test]
fn a_dagger_placed_on_any_other_tile_is_not_poisoned() {
    let daggers = infiltrate_over("windmill");
    assert!(!daggers.is_empty(), "the infiltration placed daggers");
    assert!(daggers.iter().all(|&(_, poisoned)| !poisoned), "{daggers:?}");
}
