//! Ruin rewards against retail: which reward a ruin gives, and what each reward does.

use polytopia_sim::engine::{Command, Engine, GameOptions};
use polytopia_sim::rules::path::is_water;
use polytopia_sim::{WorldCoordinates as Wc, NATURE};

const CITY: i32 = 1;
const FIELD: i32 = 3;
const SWORDSMAN: &str = "swordsman";

fn game() -> Engine {
    Engine::new_game(&GameOptions::default())
}

/// Takes every city from `player`, so that it is no longer alive.
fn wipe_cities(e: &mut Engine, player: u8) {
    for t in e.s.map.tiles.iter_mut().filter(|t| t.owner == player && t.improvement == CITY) {
        t.improvement = 0;
        t.owner = 0;
    }
}

fn dist(a: Wc, b: Wc) -> i32 {
    (a.x - b.x).abs().max((a.y - b.y).abs())
}

/// The 3x3 block around `c`.
fn near(c: Wc) -> Vec<Wc> {
    (-1..=1).flat_map(|dy| (-1..=1).map(move |dx| Wc { x: c.x + dx, y: c.y + dy })).collect()
}

fn land_tiles(e: &Engine) -> Vec<Wc> {
    e.s.map.tiles.iter().filter(|t| !is_water(t.terrain)).map(|t| t.coordinates).collect()
}

/// Retail counts every other living player, and nature (id 255, always
/// among the players) is always alive. So on land the swordsman stays eligible after every opponent is gone.
#[test]
fn swordsman_needs_no_living_opponent_because_nature_counts() {
    let mut e = game();
    assert!(e.s.players.iter().any(|p| p.id == NATURE));
    let tiles = land_tiles(&e);
    let alive: Vec<i32> = tiles.iter().map(|c| e.ruins_reward(1, *c)).collect();
    assert!(alive.contains(&8));
    wipe_cities(&mut e, 2);
    assert!(!e.is_alive(2));
    let wiped: Vec<i32> = tiles.iter().map(|c| e.ruins_reward(1, *c)).collect();
    assert_eq!(alive, wiped);
}

/// The swordsman takes the ruin tile, the examining unit steps aside and is exhausted, and the new unit is a
/// veteran at full health with no home city.
#[test]
fn swordsman_reward_spawns_a_veteran_on_the_ruin() {
    let mut e = game();
    let start = e.s.units.iter().find(|u| u.owner == 1).unwrap().clone();
    let at = land_tiles(&e)
        .into_iter()
        .filter(|c| e.ruins_reward(1, *c) == 8 && e.tile(*c).is_some_and(|t| t.improvement == 0))
        .filter(|c| near(*c).iter().all(|n| e.tile(*n).is_some_and(|t| t.improvement == 0)))
        .find(|c| e.s.units.iter().all(|u| dist(u.coordinates, *c) > 2))
        .expect("a land tile whose ruin gives a swordsman");
    // Open fields around the ruin, so that the push has somewhere to go.
    let around = near(at);
    for t in e.s.map.tiles.iter_mut().filter(|t| around.contains(&t.coordinates)) {
        t.terrain = FIELD;
        t.has_ruin = t.coordinates == at;
    }
    assert_eq!(e.ruins_reward(1, at), 8);
    let i = e.s.units.iter().position(|u| u.id == start.id).unwrap();
    e.s.units[i].coordinates = at;
    e.s.units[i].moved = false;
    e.s.units[i].attacked = false;
    e.explore_unit(start.id);
    assert_eq!(e.ruins_reward(1, at), 8);
    let next = e.s.current_unit_id + 1;
    e.apply(Command::ExamineRuins { unit: start.id }).expect("examining the ruin is legal");

    let sw = e.s.units.iter().find(|u| u.coordinates == at).expect("a unit on the ruin");
    assert_eq!(sw.id, next);
    assert_eq!(sw.owner, 1);
    assert_eq!(e.t.unit(sw.type_idx).unwrap().data.id, SWORDSMAN);
    assert_eq!(sw.promotion, 1);
    assert_eq!(i32::from(sw.health), e.max_health(sw));
    assert_eq!(sw.home, Wc { x: -1, y: -1 });
    assert_eq!(e.s.current_unit_id, next);

    let examiner = e.s.units.iter().find(|u| u.id == start.id).expect("the examining unit was pushed, not killed");
    assert_ne!(examiner.coordinates, at);
    assert!(examiner.moved && examiner.attacked);
    assert_eq!(examiner.promotion, start.promotion);
    assert!(!e.tile(at).unwrap().has_ruin);
}
