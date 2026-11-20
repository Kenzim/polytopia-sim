//! The tentacles of Aquarion's jelly.
//!
//! A unit with the Tentacles ability puts the Tentacle effect on the eight tiles around it (not while frozen). A unit that
//! ends a move on a Tentacle tile takes a hit from the strongest unit with Tentacles next to it that belongs to another
//! player that is not at peace with it: the full battle damage of that unit against it. The tiles are recomputed after
//! a move, a train, a disband, a freeze, a kill and the end of a turn that concerns a unit with Tentacles. A jelly that
//! has just been trained also hits the units next to its city (full battle damage, as a move of a jelly does), and a jelly
//! that a train pushes aside moves like any other (it hits its new neighbours and takes its ring with it). A ship that
//! lands its passenger on a Tentacle tile has the hit fixed against the ship and landed on the passenger.
//!
//! Evidence: in the Aquarion human games the server's boards show tentacle tiles (effect bit 3) in rings round jellies
//! that moved, were trained or died; before this the engine never set the effect and every game with a jelly differed on
//! those tiles.
use polytopia_sim::engine::{Command, Engine};
use polytopia_sim::json::Json;
use polytopia_sim::WorldCoordinates as Wc;

const JELLY: i64 = 52;
const WARRIOR: i64 = 2;
const MERMAID_WARRIOR: i64 = 46;
const TENTACLE: u16 = 1 << 3;

fn wc(x: i32, y: i32) -> Wc {
    Wc { x, y }
}

/// Player 1 (Aquarion, to move) and player 2 (Bardur) on a 9x5 water board; `units` is `(id, type, owner, x, y, flags)`.
fn board(units: &[(u32, i64, u8, i32, i32, &str)]) -> Engine {
    let mut tiles = Vec::new();
    for y in 0..5 {
        for x in 0..9 {
            let mut t = format!(r#""x":{x},"y":{y},"terrain":1,"climate":3"#);
            let ringed = units.iter().any(|(_, ty, _, ux, uy, flags)| {
                *ty == JELLY
                    && !flags.contains("frozen")
                    && (ux - x).abs() <= 1
                    && (uy - y).abs() <= 1
                    && (*ux, *uy) != (x, y)
            });
            if ringed {
                t.push_str(r#","effects":[3]"#);
            }
            match (x, y) {
                (0, 0) => t.push_str(r#","terrain":3,"city":true,"capital":true,"owner":1,"ruling":[0,0]"#),
                (8, 4) => t.push_str(r#","terrain":3,"city":true,"capital":true,"owner":2,"ruling":[8,4]"#),
                _ => {}
            }
            tiles.push(format!("{{{t}}}"));
        }
    }
    let us: Vec<String> = units
        .iter()
        .map(|(id, ty, owner, x, y, flags)| {
            format!(
                r#"{{"id":{id},"type":{ty},"owner":{owner},"x":{x},"y":{y},"health":{},"flags":[{flags}]}}"#,
                if flags.contains("weak") {
                    10
                } else if *ty == JELLY {
                    200
                } else {
                    100
                }
            )
        })
        .collect();
    let text = format!(
        r#"{{"format":"polytopia-position","game":{{"turn":6,"width":9,"height":5,"current_player":1}},
           "players":[{{"id":1,"tribe":3,"human":true,"stars":30,"tech":[0,45,46,14]}},{{"id":2,"tribe":4,"stars":5}}],
           "tiles":[{}],
           "improvements":[{{"x":0,"y":0,"level":1}},{{"x":8,"y":4,"level":1}}],
           "units":[{}]}}"#,
        tiles.join(","),
        us.join(",")
    );
    Engine::from_position(&Json::parse(&text).unwrap()).unwrap()
}

fn tentacle_tiles(e: &Engine) -> Vec<(i32, i32)> {
    let mut v = Vec::new();
    for y in 0..5 {
        for x in 0..9 {
            if e.tile(wc(x, y)).unwrap().effects & TENTACLE != 0 {
                v.push((x, y));
            }
        }
    }
    v
}

#[test]
fn a_jelly_puts_tentacles_on_the_eight_tiles_round_it() {
    let e = board(&[(1, JELLY, 2, 4, 2, "")]);
    let t = tentacle_tiles(&e);
    assert_eq!(t.len(), 8);
    assert!(!t.contains(&(4, 2)), "not under the jelly itself");
}

#[test]
fn a_frozen_jelly_has_no_tentacles() {
    let e = board(&[(1, JELLY, 2, 4, 2, r#""frozen""#)]);
    assert!(tentacle_tiles(&e).is_empty());
}

#[test]
fn a_unit_that_steps_next_to_a_jelly_is_hit() {
    let mut e = board(&[(1, JELLY, 2, 4, 2, ""), (2, MERMAID_WARRIOR, 1, 2, 2, "")]);
    e.apply(Command::Move { unit: 2, to: wc(3, 2) }).expect("the move is legal");
    let hp = e.unit(2).unwrap().health;
    assert!(hp < 100, "the warrior took a tentacle hit, health {hp}");
    assert_eq!(e.unit(1).unwrap().health, 200, "the jelly is not hurt (no retaliation)");
}

#[test]
fn a_unit_that_stays_out_of_the_ring_is_not_hit() {
    let mut e = board(&[(1, JELLY, 2, 5, 2, ""), (2, MERMAID_WARRIOR, 1, 2, 2, "")]);
    e.apply(Command::Move { unit: 2, to: wc(3, 2) }).expect("the move is legal");
    assert_eq!(e.unit(2).unwrap().health, 100);
}

#[test]
fn a_jelly_that_moves_takes_its_ring_with_it() {
    let mut e = board(&[(1, JELLY, 1, 4, 2, ""), (2, WARRIOR, 2, 8, 0, "")]);
    assert!(tentacle_tiles(&e).contains(&(3, 1)));
    e.apply(Command::Move { unit: 1, to: wc(5, 2) }).expect("the move is legal");
    let t = tentacle_tiles(&e);
    assert!(!t.contains(&(3, 1)) && t.contains(&(6, 3)));
}

#[test]
fn a_jelly_that_dies_takes_its_ring_with_it() {
    // "weak" is a test marker for a jelly with 10 health
    let mut e = board(&[(1, JELLY, 2, 4, 2, r#""weak""#), (2, MERMAID_WARRIOR, 1, 3, 2, "")]);
    assert_eq!(tentacle_tiles(&e).len(), 8);
    e.apply(Command::Attack { unit: 2, target: wc(4, 2) }).expect("the attack is legal");
    assert!(e.unit(1).is_none(), "the jelly died");
    assert!(tentacle_tiles(&e).is_empty());
}

#[test]
fn a_jelly_that_is_trained_hits_the_enemy_units_next_to_its_city() {
    let mut e = board(&[
        (1, MERMAID_WARRIOR, 2, 1, 1, ""),
        (2, MERMAID_WARRIOR, 2, 3, 3, ""),
        (3, MERMAID_WARRIOR, 1, 1, 0, ""),
    ]);
    e.apply(Command::Train { city: wc(0, 0), unit_type: JELLY as i32 }).expect("the train is legal");
    assert!(e.unit(1).unwrap().health < 100, "the enemy next to the city was hit");
    assert_eq!(e.unit(2).unwrap().health, 100, "an enemy further away is not");
    assert_eq!(e.unit(3).unwrap().health, 100, "a unit of the same player is not");
}

#[test]
fn a_jelly_that_is_trained_can_kill_a_weak_enemy_next_to_its_city() {
    let mut e = board(&[(1, MERMAID_WARRIOR, 2, 1, 1, r#""weak""#)]);
    e.apply(Command::Train { city: wc(0, 0), unit_type: JELLY as i32 }).expect("the train is legal");
    assert!(e.unit(1).is_none(), "the weak enemy died");
}

#[test]
fn a_jelly_that_is_pushed_aside_moves_like_any_other_move() {
    // A train onto the tile of an enemy jelly pushes it one step away (here to (5, 3)).
    let mut e = board(&[(1, JELLY, 2, 4, 2, ""), (2, MERMAID_WARRIOR, 1, 6, 3, ""), (3, MERMAID_WARRIOR, 1, 8, 0, "")]);
    let sw = e.tribe_unit(1, "swordsman").unwrap();
    e.train_on_occupied(1, sw, wc(4, 2), wc(0, 0));
    assert_eq!(e.unit_at(wc(5, 3)).map(|u| u.id), Some(1), "the jelly stepped aside");
    assert!(e.unit(2).unwrap().health < 100, "the warrior next to the jelly's new tile was hit");
    assert_eq!(e.unit(3).unwrap().health, 100, "a warrior further away was not");
    let t = tentacle_tiles(&e);
    assert!(t.contains(&(6, 4)) && !t.contains(&(3, 1)), "the tentacles are round the new tile");
}

const TRANSPORT_SHIP: i64 = 43;

/// Water everywhere but the land column x >= 5; a player 1 transport ship at (4, 2) carries a warrior, and a jelly of
/// player 2 at (4, 1) puts its ring on the landing tile (5, 2).
fn landing_board() -> Engine {
    let mut tiles = Vec::new();
    for y in 0..5i32 {
        for x in 0..9i32 {
            let mut t = format!(r#""x":{x},"y":{y},"terrain":{},"climate":3"#, if x >= 5 { 3 } else { 1 });
            if (x - 4).abs() <= 1 && (y - 1).abs() <= 1 && (x, y) != (4, 1) {
                t.push_str(r#","effects":[3]"#);
            }
            match (x, y) {
                (0, 0) => t.push_str(r#","city":true,"capital":true,"owner":1,"ruling":[0,0]"#),
                (8, 4) => t.push_str(r#","city":true,"capital":true,"owner":2,"ruling":[8,4]"#),
                _ => {}
            }
            tiles.push(format!("{{{t}}}"));
        }
    }
    let text = format!(
        r#"{{"format":"polytopia-position","game":{{"turn":6,"width":9,"height":5,"current_player":1}},
           "players":[{{"id":1,"tribe":3,"human":true,"stars":5}},{{"id":2,"tribe":4,"stars":5}}],
           "tiles":[{}],
           "improvements":[{{"x":0,"y":0,"level":1}},{{"x":8,"y":4,"level":1}}],
           "units":[{{"id":1,"type":{TRANSPORT_SHIP},"owner":1,"x":4,"y":2,"health":100,"passenger":{{"id":2,"type":{MERMAID_WARRIOR},"owner":1,"health":100}}}},
                    {{"id":3,"type":{JELLY},"owner":2,"x":4,"y":1,"health":200}}]}}"#,
        tiles.join(",")
    );
    Engine::from_position(&Json::parse(&text).unwrap()).unwrap()
}

#[test]
fn a_ship_that_lands_its_passenger_on_a_tentacle_tile_hits_the_passenger_with_the_damage_against_the_ship() {
    let mut e = landing_board();
    let ship = e.unit(1).unwrap().clone();
    let jelly = e.unit(3).unwrap().clone();
    let query = e.battle_query_pub(&jelly, &ship, false);
    let against_ship =
        polytopia_sim::rules::combat::get_battle_results(&e.combat_unit(&jelly), &e.combat_unit(&ship), &query)
            .attack_damage;
    e.apply(Command::Move { unit: 1, to: wc(5, 2) }).expect("the ship may land");
    let warrior = e.unit_at(wc(5, 2)).expect("the passenger is on the landing tile");
    assert_eq!(warrior.id, 2);
    assert_eq!(
        i32::from(warrior.health),
        100 - against_ship,
        "the hit was fixed against the ship and landed on the warrior"
    );
}

#[test]
fn the_tentacle_hit_on_a_jelly_that_steps_into_the_ring_is_fixed_before_its_own_area_damage() {
    // Both hits are queued by the move and their damage fixed then, so the jelly in whose ring the mover lands deals
    // the damage of its health before the mover's own hit on it.
    let mut e = board(&[(1, JELLY, 1, 2, 2, ""), (2, JELLY, 2, 4, 2, "")]);
    let hurt = e.s.units.iter().position(|u| u.id == 2).unwrap();
    e.s.units[hurt].health = 80;
    let mut landed = e.unit(1).unwrap().clone();
    landed.coordinates = wc(3, 2);
    let query = e.battle_query_pub(e.unit(2).unwrap(), &landed, false);
    let hit = polytopia_sim::rules::combat::get_battle_results(
        &e.combat_unit(e.unit(2).unwrap()),
        &e.combat_unit(&landed),
        &query,
    )
    .attack_damage;
    e.apply(Command::Move { unit: 1, to: wc(3, 2) }).expect("the move is legal");
    assert!(e.unit(2).is_none_or(|u| u.health < 80), "the mover hit the jelly next to it");
    assert_eq!(i32::from(e.unit(1).unwrap().health), 200 - hit, "the hit was fixed with the other jelly at 80 health");
}

#[test]
fn a_jelly_gets_no_experience_for_a_kill_but_a_warrior_does() {
    for (unit_type, xp) in [(JELLY, 0), (MERMAID_WARRIOR, 1)] {
        let mut e = board(&[(1, unit_type, 1, 2, 2, ""), (2, MERMAID_WARRIOR, 2, 3, 2, r#""weak""#)]);
        e.apply(Command::Attack { unit: 1, target: wc(3, 2) }).expect("the attack is legal");
        assert!(e.unit(2).is_none(), "the weak enemy died");
        assert_eq!(e.unit(1).unwrap().xp, xp, "experience of unit type {unit_type}");
    }
}

#[test]
fn before_version_105_a_jelly_still_gets_experience_for_a_kill() {
    let mut e = board(&[(1, JELLY, 1, 2, 2, ""), (2, MERMAID_WARRIOR, 2, 3, 2, r#""weak""#)]);
    e.s.version = 104;
    e.apply(Command::Attack { unit: 1, target: wc(3, 2) }).expect("the attack is legal");
    assert_eq!(e.unit(1).unwrap().xp, 1);
}
