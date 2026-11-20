//! What follows an explode command.
//!
//! Retail 2.18 (game version 124) runs the explode through the same command wrapper as every other command: the
//! base command execute queues the end-of-command action (which marks every opinion for recomputing) and the
//! run-deferred action (which pays out the actions held back for the player) under the explosion. The retail
//! build before that skipped both after an explode, so from version 124 on they happen and before it they do not.
//!
//! The board is built here: a doomux next to a warrior, with an income of `PENDING_INCOME` stars still waiting on
//! the action stack. Without the version gate in `apply_unchecked` the 123 tests (or the 124 ones) fail.
use polytopia_sim::engine::{Command, Engine};
use polytopia_sim::json::Json;

const CYMANTI: i64 = 17;
const BARDUR: i64 = 3;
const DOOMUX: i64 = 29;
const WARRIOR: i64 = 2;
const PENDING_INCOME: i32 = 7;

/// Player 1 (Cymanti, to move) and player 2 on a 7x3 field board, game version `version`.
fn board(version: i32) -> Engine {
    let mut tiles = Vec::new();
    for y in 0..3 {
        for x in 0..7 {
            let mut t = format!(r#""x":{x},"y":{y},"terrain":3,"climate":2"#);
            match (x, y) {
                (0, 0) => t.push_str(r#","city":true,"capital":true,"owner":1,"ruling":[0,0]"#),
                (6, 0) => t.push_str(r#","city":true,"capital":true,"owner":2,"ruling":[6,0]"#),
                _ => {}
            }
            tiles.push(format!("{{{t}}}"));
        }
    }
    let text = format!(
        r#"{{"format":"polytopia-position","game":{{"turn":6,"width":7,"height":3,"current_player":1,"game_version":{version}}},
           "players":[{{"id":1,"tribe":{CYMANTI},"human":true,"stars":5}},{{"id":2,"tribe":{BARDUR},"stars":5}}],
           "tiles":[{}],
           "improvements":[{{"x":0,"y":0,"level":1}},{{"x":6,"y":0,"level":1}}],
           "units":[{{"id":1,"type":{DOOMUX},"owner":1,"x":1,"y":1,"health":100}},
                    {{"id":2,"type":{WARRIOR},"owner":2,"x":2,"y":1,"health":100}}]}}"#,
        tiles.join(",")
    );
    let mut e = Engine::from_position(&Json::parse(&text).unwrap()).unwrap();
    assert_eq!(e.s.version, version);
    e.deferred.push(polytopia_sim::engine::Deferred::Income(1, PENDING_INCOME));
    e
}

fn stars(e: &Engine) -> i32 {
    e.player(1).unwrap().currency
}

/// The opinion key player 2's manager was last computed for, if it has one.
fn opinion_key(e: &Engine) -> Option<(u16, u32)> {
    e.opinions.borrow().managers.get(&2).and_then(|m| m.clean_at)
}

fn explode(e: &mut Engine) {
    e.apply(Command::Explode { unit: 1 }).expect("the explode is legal");
    assert!(e.unit(1).is_none(), "the doomux is gone");
    assert!(e.unit(2).is_some_and(|w| w.health < 100), "the warrior beside it was hurt");
}

#[test]
fn from_124_an_explode_runs_the_held_back_actions() {
    let mut e = board(124);
    let before = stars(&e);
    explode(&mut e);
    assert!(e.deferred.is_empty(), "the run-deferred step emptied the stack");
    assert_eq!(stars(&e), before + PENDING_INCOME, "and the held income was paid");
}

#[test]
fn version_123_explode_leaves_the_held_back_actions_alone() {
    let mut e = board(123);
    let before = stars(&e);
    explode(&mut e);
    assert_eq!(e.deferred.len(), 1, "no run-deferred action follows the explode");
    assert_eq!(stars(&e), before, "so the income is still waiting");
}

#[test]
fn from_124_an_explode_marks_the_opinions_for_recomputing() {
    let mut e = board(124);
    let _ = e.opinion(2, 1);
    let computed_for = opinion_key(&e);
    assert_eq!(computed_for, Some((e.s.current_command, e.s.current_turn)));
    explode(&mut e);
    assert_eq!(opinion_key(&e), computed_for, "the key is left behind, so the next query recomputes");
    assert_ne!(opinion_key(&e), Some((e.s.current_command, e.s.current_turn)));
}

#[test]
fn version_123_explode_keeps_the_opinions_as_they_were() {
    let mut e = board(123);
    let _ = e.opinion(2, 1);
    explode(&mut e);
    assert_eq!(
        opinion_key(&e),
        Some((e.s.current_command, e.s.current_turn)),
        "no end-of-command action, so nothing marks the opinions"
    );
}

#[test]
fn other_commands_run_the_held_back_actions_in_every_version() {
    for version in [123, 124] {
        let mut e = board(version);
        let before = stars(&e);
        e.apply_unchecked(Command::Stay { unit: 1 });
        assert!(e.deferred.is_empty(), "version {version}: the stack is emptied after an ordinary command");
        assert_eq!(stars(&e), before + PENDING_INCOME, "version {version}");
    }
}
