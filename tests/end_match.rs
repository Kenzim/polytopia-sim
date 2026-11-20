//! The two ways out of `FinalTurn`, which retail keeps apart.
//!
//! An end-of-turn command (raw 15) in `FinalTurn` runs the end-of-turn effects, healing included, and then the game
//! ends where it stands. The end-match command (raw 22, `Engine::end_match`) is valid only in `FinalTurn`, and sets
//! the game `Ended` with nothing else, so a unit that did not move stays as hurt as it was.
use polytopia_sim::engine::{Command, Engine, GameOptions};
use polytopia_sim::GameStatus;

fn final_turn_game() -> (Engine, u32) {
    let mut e = Engine::new_game(&GameOptions::default());
    let me = e.current_player();
    let i =
        e.s.units.iter().position(|u| u.owner == me && u.passenger.is_none()).expect("a unit of the player to move");
    e.s.units[i].health = 40;
    let id = e.s.units[i].id;
    let from = e.s.units[i].coordinates;
    let beside = e.neighbors(from).into_iter().find(|c| e.unit_at(*c).is_none()).expect("a free tile");
    let other = e.spawn_unit(me, 2, beside, from, false).expect("a second unit");
    e.s.units.iter_mut().find(|u| u.id == other).unwrap().moved = true;
    e.s.current_state = GameStatus::FinalTurn;
    (e, id)
}

#[test]
fn an_end_of_turn_in_the_final_turn_runs_the_effects_then_ends_the_game() {
    let (mut e, id) = final_turn_game();
    let before = e.unit(id).unwrap().health;
    e.apply(Command::EndTurn).expect("an end of turn is legal in FinalTurn");
    assert!(e.is_over());
    let u = e.unit(id).unwrap();
    assert!(u.health > before, "the end of turn healed the idle unit: {before} to {}", u.health);
    let me = e.current_player();
    assert!(e.s.units.iter().filter(|u| u.owner == me).all(|u| !u.moved), "and cleared the moved flags");
}

#[test]
fn the_end_match_command_ends_the_game_with_nothing_else() {
    let (mut e, id) = final_turn_game();
    let counter = e.s.current_command;
    e.end_match().expect("valid in FinalTurn");
    assert!(e.is_over());
    assert_eq!(e.s.current_command, counter + 1);
    let u = e.unit(id).unwrap();
    assert_eq!(u.health, 40, "no healing ran");
    let me = e.current_player();
    assert!(e.s.units.iter().any(|u| u.owner == me && u.moved), "nor did the flags clear");
}

#[test]
fn the_end_match_command_is_only_valid_in_the_final_turn() {
    let mut e = Engine::new_game(&GameOptions::default());
    let counter = e.s.current_command;
    assert!(e.end_match().is_err());
    assert!(!e.is_over());
    assert_eq!(e.s.current_command, counter, "a refused command does not count");
    let (mut e, _) = final_turn_game();
    e.end_match().unwrap();
    assert!(e.end_match().is_err(), "nor once the game is over");
}
