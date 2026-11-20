//! A unit put on a poisoned tile (trained, or a centipede's new segment) is poisoned at once.
//!
//! Retail's train action places the unit and then checks it for poison, as a move does, so the unit
//! stands Poisoned straight away. A flying unit and a tribe that resists poison are exempt, as for a move.
//!
//! These tests need no files.
use polytopia_sim::engine::tables::{improvement_ability, unit_ability};
use polytopia_sim::engine::{Engine, GameOptions};
use polytopia_sim::WorldCoordinates as Wc;

fn spawned_poisoned(flying: bool) -> bool {
    let mut e = Engine::new_game(&GameOptions::default());
    let poison = (0..400)
        .find(|i| e.t.improvement(*i).is_some_and(|d| d.abilities.contains(&improvement_ability::POISON)))
        .expect("a poison improvement");
    let city =
        e.s.map.tiles.iter().find(|t| t.owner == 1 && t.improvement == 1).expect("a city of player 1").coordinates;
    let at = Wc { x: if city.x > 0 { city.x - 1 } else { city.x + 1 }, y: city.y };
    e.s.units.retain(|u| u.coordinates != at);
    let t = e.s.map.tiles.iter_mut().find(|t| t.coordinates == at).unwrap();
    t.terrain = 3;
    t.improvement = poison;
    t.has_ruin = false;
    let kind = (0..200)
        .find(|i| e.t.unit(*i).is_some_and(|u| u.abilities.contains(&unit_ability::FLY) == flying && u.data.attack > 0))
        .expect("a unit type");
    let id = e.spawn_unit(1, kind, at, city, false).expect("the unit");
    e.unit(id).unwrap().poisoned
}

#[test]
fn a_warrior_spawned_on_a_poisoned_tile_is_poisoned() {
    assert!(spawned_poisoned(false));
}

#[test]
fn a_flying_unit_spawned_on_a_poisoned_tile_is_not() {
    assert!(!spawned_poisoned(true));
}
