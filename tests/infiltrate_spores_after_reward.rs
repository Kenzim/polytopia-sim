//! A poisoned infiltrator leaves spores on its tile, and on water a free algae build. Both are queued by the kill
//! and the infiltration reward is queued above them, so the reward chooses its spawn tiles while the infiltrator's
//! water tile is still plain water: no unit can be placed there. The algae and the spores land after the reward.
//!
//! These tests need no files.
use polytopia_sim::engine::{Command, Engine, GameOptions};
use polytopia_sim::WorldCoordinates as Wc;

const CITY: i32 = 1;
const OCEAN: i32 = 2;
const WATER: i32 = 1;

fn type_of(e: &Engine, id: &str) -> i32 {
    (0..200).find(|i| e.t.unit(*i).is_some_and(|u| u.data.id == id)).unwrap_or_else(|| panic!("unit type {id}"))
}

/// A poisoned cloak of player 1 on a water tile beside a city of player 2 whose other tiles are all ocean, and a
/// giant in the city that survives the probe, so the cloak's tile is the only spawn candidate it could make.
/// Returns the engine after the infiltration and the cloak's tile.
fn infiltrate_from_water(poisoned: bool) -> (Engine, Wc) {
    let mut e = Engine::new_game(&GameOptions::default());
    let city =
        e.s.map.tiles.iter().find(|t| t.owner == 2 && t.improvement == CITY).expect("a city of player 2").coordinates;
    let beside = Wc { x: if city.x > 0 { city.x - 1 } else { city.x + 1 }, y: city.y };
    e.s.units.retain(|u| u.coordinates != city && u.coordinates != beside);
    for t in e.s.map.tiles.iter_mut().filter(|t| t.ruling_city == city && t.coordinates != city) {
        t.terrain = if t.coordinates == beside { WATER } else { OCEAN };
        t.improvement = 0;
        t.resource = 0;
        t.effects = 0;
        t.has_ruin = false;
    }
    e.s.units.retain(|u| {
        e.s.map.tiles.iter().all(|t| t.coordinates != u.coordinates || t.ruling_city != city || u.owner == 1)
    });
    let cloak = type_of(&e, "cloak");
    let id = e.spawn_unit(1, cloak, beside, beside, false).expect("the cloak");
    e.s.units.iter_mut().find(|u| u.id == id).unwrap().poisoned = poisoned;
    let giant = type_of(&e, "giant");
    e.spawn_unit(2, giant, city, city, false).expect("the giant");
    e.apply(Command::Attack { unit: id, target: city }).unwrap();
    (e, beside)
}

#[test]
fn a_poisoned_infiltrators_water_tile_gets_no_spawn() {
    let (e, beside) = infiltrate_from_water(true);
    let dagger = type_of(&e, "dagger");
    let placed: Vec<Wc> =
        e.s.units.iter().filter(|u| u.owner == 1 && u.type_idx == dagger).map(|u| u.coordinates).collect();
    assert!(placed.is_empty(), "daggers placed at {placed:?} (the cloak stood at {beside:?})");
}

#[test]
fn the_algae_and_spores_still_grow_after_the_reward() {
    let (e, beside) = infiltrate_from_water(true);
    let t = e.s.map.tiles.iter().find(|t| t.coordinates == beside).unwrap();
    assert_ne!(t.effects & (1 << Engine::EFFECT_ALGAE), 0, "{t:?}");
    assert_eq!(t.resource, 7, "{t:?}");
}

#[test]
fn a_healthy_infiltrator_leaves_no_spores() {
    let (e, beside) = infiltrate_from_water(false);
    let t = e.s.map.tiles.iter().find(|t| t.coordinates == beside).unwrap();
    assert_eq!(t.effects & (1 << Engine::EFFECT_ALGAE), 0, "{t:?}");
    assert_eq!(t.resource, 0, "{t:?}");
}
