//! The infiltration reward tests each spawn tile for a land unit as a move from the tile onto itself. A bridge on
//! water passes that test as long as it still has land on two opposite sides, so a dagger can be placed on it.
//!
//! These tests need no files.
use polytopia_sim::engine::{Command, Engine, GameOptions};
use polytopia_sim::WorldCoordinates as Wc;

const CITY: i32 = 1;
const WATER: i32 = 1;
const FIELD: i32 = 3;
const BRIDGE: i32 = 48;

fn type_of(e: &Engine, id: &str) -> i32 {
    (0..200).find(|i| e.t.unit(*i).is_some_and(|u| u.data.id == id)).unwrap_or_else(|| panic!("unit type {id}"))
}

/// Player 1's daggers after a cloak of its own infiltrated a city of player 2 whose territory is water apart from the
/// city, the cloak's tile and the tile two below the city, with a bridge on the water tile between them. The city
/// is level 3, so there are more daggers than land tiles to put them on.
fn daggers_after_infiltration() -> (Vec<Wc>, Wc) {
    let mut e = Engine::new_game(&GameOptions::default());
    let size = e.s.map.width as i32;
    let city =
        e.s.map
            .tiles
            .iter()
            .find(|t| t.owner == 2 && t.improvement == CITY && t.coordinates.y + 2 < size && t.coordinates.x > 0)
            .expect("a city of player 2")
            .coordinates;
    let beside = Wc { x: city.x - 1, y: city.y };
    let bridge = Wc { x: city.x, y: city.y + 1 };
    let far = Wc { x: city.x, y: city.y + 2 };
    e.s.units.retain(|u| u.coordinates != city && u.coordinates != beside);
    for t in e.s.map.tiles.iter_mut().filter(|t| t.coordinates != city) {
        let c = t.coordinates;
        if t.ruling_city == city || c == beside || c == bridge || c == far {
            t.improvement = 0;
            t.has_ruin = false;
            t.resource = 0;
            t.terrain = if c == beside || c == far { FIELD } else { WATER };
        }
    }
    e.s.map.tiles.iter_mut().find(|t| t.coordinates == bridge).unwrap().improvement = BRIDGE;
    e.improvement_rt_mut(city).expect("the city").level = 3;
    e.s.units.retain(|u| u.coordinates != far && u.coordinates != bridge);
    let cloak = type_of(&e, "cloak");
    let id = e.spawn_unit(1, cloak, beside, beside, false).expect("the cloak");
    let giant = type_of(&e, "giant");
    e.spawn_unit(2, giant, city, city, false).expect("the giant");
    e.apply(Command::Attack { unit: id, target: city }).unwrap();
    let dagger = type_of(&e, "dagger");
    (e.s.units.iter().filter(|u| u.owner == 1 && u.type_idx == dagger).map(|u| u.coordinates).collect(), bridge)
}

#[test]
fn an_infiltration_places_a_dagger_on_a_bridge_over_the_citys_water() {
    let (daggers, bridge) = daggers_after_infiltration();
    assert!(daggers.contains(&bridge), "daggers at {daggers:?}, bridge at {bridge:?}");
}
