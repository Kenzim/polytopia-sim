//! A ruin's ship reward carries a veteran passenger only when the ship can carry one.
//!
//! The Cymanti ship is the boomchi, which cannot carry. Retail then burns no second unit id, so the next unit made
//! gets the id straight after the ship. The engine burned one for a passenger that was never on the board, and every
//! later command naming a unit id in those games was off by one.
use polytopia_sim::engine::{Command, Engine};
use polytopia_sim::json::Json;
use polytopia_sim::WorldCoordinates as Wc;

const RAYCHI: i64 = 35;
const RUIN: i64 = 2;
const RUIN_AT: Wc = Wc { x: 3, y: 1 };

/// A strip of water with a ruin on (3,1) and the examiner, a swimming unit, on it. `tribe` is player 1's.
fn board(tribe: i64, seed: i32) -> Engine {
    let mut tiles = Vec::new();
    for y in 0..3 {
        for x in 0..6 {
            let land = x == 0 || (x, y) == (5, 2);
            let mut t = format!(r#""x":{x},"y":{y},"terrain":{},"climate":2"#, if land { 3 } else { 2 });
            match (x, y) {
                (0, 0) => t.push_str(r#","city":true,"capital":true,"owner":1,"ruling":[0,0]"#),
                (5, 2) => t.push_str(r#","city":true,"capital":true,"owner":2,"ruling":[5,2]"#),
                (3, 1) => t.push_str(&format!(r#","improvement":{RUIN}"#)),
                _ => {}
            }
            tiles.push(format!("{{{t}}}"));
        }
    }
    let text = format!(
        r#"{{"format":"polytopia-position","game":{{"turn":6,"width":6,"height":3,"current_player":1,"seed":{seed}}},
           "players":[{{"id":1,"tribe":{tribe},"human":true,"stars":5}},{{"id":2,"tribe":3,"stars":5}}],
           "tiles":[{}],
           "improvements":[{{"x":0,"y":0,"level":1}},{{"x":5,"y":2,"level":1}}],
           "units":[{{"id":1,"type":{RAYCHI},"owner":1,"x":3,"y":1}}]}}"#,
        tiles.join(",")
    );
    let mut e = Engine::from_position(&Json::parse(&text).unwrap()).unwrap();
    for t in 0..200 {
        if e.t.tech_info(t).is_some() && !e.unlocked_movement_terrain(1).contains(&2) {
            e.s.tech.iter_mut().find(|p| p.player == 1).unwrap().tech.push(t);
        }
    }
    e
}

/// Player 1's board once its examiner has opened a ruin that gives the ship.
fn examined(tribe: i64) -> Engine {
    let seed = (0..3000).find(|&s| board(tribe, s).ruins_reward(1, RUIN_AT) == 6).expect("a seed that gives the ship");
    let mut e = board(tribe, seed);
    e.apply(Command::ExamineRuins { unit: 1 }).expect("the examine");
    e
}

#[test]
fn a_boomchi_reward_burns_no_second_id() {
    let e = examined(17);
    let ship = e.s.units.iter().find(|u| u.owner == 1 && u.id != 1).expect("the ship");
    assert!(ship.passenger.is_none());
    assert_eq!(e.s.current_unit_id, ship.id);
}

#[test]
fn a_rammership_reward_still_carries_a_passenger() {
    let e = examined(4);
    let ship = e.s.units.iter().find(|u| u.owner == 1 && u.id != 1).expect("the ship");
    assert!(ship.passenger.is_some());
    assert_eq!(e.s.current_unit_id, ship.id + 1);
}
