//! Retail's heal action removes the poison effect from the unit on the tile and from its passenger, and the poison
//! action adds it to both, so resting (or Recover) cures a carried unit too. The engine used to cure only the carrier,
//! leaving a poisoned passenger (39 units in the corpus final-state comparison).
use polytopia_sim::engine::{Command, Engine, GameOptions};

fn poisoned_carrier() -> (Engine, u32, u32) {
    let mut e = Engine::new_game(&GameOptions::default());
    let me = e.current_player();
    let i =
        e.s.units.iter().position(|u| u.owner == me && u.passenger.is_none()).expect("a unit of the player to move");
    let id = e.s.units[i].id;
    e.embark_pub(id);
    let boat = e.s.units.iter().position(|u| u.passenger.as_ref().is_some_and(|p| p.id == id)).expect("the boat");
    e.s.units[boat].poisoned = true;
    e.s.units[boat].passenger.as_mut().unwrap().poisoned = true;
    e.s.units[boat].moved = false;
    e.s.units[boat].attacked = false;
    let boat_id = e.s.units[boat].id;
    (e, boat_id, id)
}

fn passenger_poisoned(e: &Engine, boat: u32) -> bool {
    e.unit(boat).and_then(|b| b.passenger.as_ref()).is_some_and(|p| p.poisoned)
}

#[test]
fn recover_cures_the_passenger_too() {
    let (mut e, boat, _) = poisoned_carrier();
    e.apply_unchecked(Command::Recover { unit: boat });
    assert!(!e.unit(boat).unwrap().poisoned);
    assert!(!passenger_poisoned(&e, boat));
}

#[test]
fn resting_through_the_end_of_turn_cures_the_passenger_too() {
    let (mut e, boat, _) = poisoned_carrier();
    e.apply_unchecked(Command::EndTurn);
    assert!(!e.unit(boat).unwrap().poisoned);
    assert!(!passenger_poisoned(&e, boat));
}
