//! A unit's movement budget counts Boosted, Swift and Bubble on top of its data movement, and a unit may step
//! whenever that budget is above zero. An egg has no movement of its own, but one a Swarm has made Swift (or
//! Boosted, in older versions) can step one tile.
//!
//! These tests need no files.
use polytopia_sim::engine::{Command, Engine, GameOptions};

fn type_of(e: &Engine, id: &str) -> i32 {
    (0..200).find(|i| e.t.unit(*i).is_some_and(|u| u.data.id == id)).unwrap_or_else(|| panic!("unit type {id}"))
}

/// The move commands the engine offers player 1's egg, made Swift or not.
fn egg_moves(swift: bool) -> usize {
    let mut e = Engine::new_game(&GameOptions::default());
    let me = e.current_player();
    let at = e.s.units.iter().find(|u| u.owner == me).expect("a unit of the current player").coordinates;
    e.s.units.retain(|u| u.owner != me);
    let egg = type_of(&e, "bugegg");
    assert_eq!(e.t.unit(egg).unwrap().data.movement, 0, "an egg has no movement of its own");
    let id = e.spawn_unit(me, egg, at, at, false).expect("the egg");
    e.s.units.iter_mut().find(|u| u.id == id).unwrap().swift = swift;
    e.legal_commands().iter().filter(|c| matches!(c, Command::Move { unit, .. } if *unit == id)).count()
}

#[test]
fn a_swift_egg_can_step() {
    assert!(egg_moves(true) > 0);
}

#[test]
fn an_egg_without_a_bonus_cannot_step() {
    assert_eq!(egg_moves(false), 0);
}
