//! A build's surrounding-area level-ups run from the action stack.
//!
//! Retail's surrounding-area check pushes one improvement level-up per level gained straight onto the action
//! stack in area order (`dx` outer, `dy` inner), under the build's own actions, and the stack runs from the top. Each
//! level-up pushes its population and then the surrounding-area check around its own tile, so a neighbour it raises
//! runs before that population; a level-up whose tile is no longer below its level is dropped unrun.
use polytopia_sim::engine::{Command, Deferred, Engine};
use polytopia_sim::json::Json;
use polytopia_sim::WorldCoordinates as Wc;

const MINE: i32 = 21;
const FORGE: i32 = 22;
const CYMANTI: i32 = 17;

const HOME: Wc = Wc { x: 6, y: 6 };
const CITY_A: Wc = Wc { x: 0, y: 0 };
const CITY_B: Wc = Wc { x: 6, y: 0 };
const FORGE_A: Wc = Wc { x: 2, y: 2 };
const SITE: Wc = Wc { x: 3, y: 3 };

/// Player 1 owns a high-level capital that rules the mine site and two level-1 cities one point short of a level-up:
/// city A rules the forge on (2,2), city B the forge on `forge_b`. Both forges touch the site.
fn board(home_xp: i16, forge_b: Wc) -> Engine {
    let mut tiles = Vec::new();
    for y in 0..7 {
        for x in 0..7 {
            let mut t = format!(r#""x":{x},"y":{y},"terrain":3,"climate":7"#);
            let c = Wc { x, y };
            if c == HOME {
                t.push_str(r#","city":true,"capital":true,"owner":1,"ruling":[6,6]"#);
            } else if c == CITY_A {
                t.push_str(r#","city":true,"owner":1,"ruling":[0,0]"#);
            } else if c == CITY_B {
                t.push_str(r#","city":true,"owner":1,"ruling":[6,0]"#);
            } else if c == FORGE_A {
                t.push_str(&format!(r#","owner":1,"ruling":[0,0],"improvement":{FORGE}"#));
            } else if c == forge_b {
                t.push_str(&format!(r#","owner":1,"ruling":[6,0],"improvement":{FORGE}"#));
            } else if c == SITE {
                t.push_str(r#","terrain":4,"owner":1,"ruling":[6,6]"#);
            } else if (x, y) == (0, 6) {
                t.push_str(r#","city":true,"capital":true,"owner":2,"ruling":[0,6]"#);
            }
            tiles.push(format!("{{{t}}}"));
        }
    }
    let text = format!(
        r#"{{"format":"polytopia-position","game":{{"turn":5,"width":7,"height":7,"current_player":1}},
           "players":[{{"id":1,"tribe":{CYMANTI},"human":true,"stars":20}},{{"id":2,"tribe":3}}],
           "tiles":[{}],
           "improvements":[{{"x":6,"y":6,"level":6,"xp":{home_xp},"population":{home_xp}}},
                           {{"x":0,"y":0,"level":1,"xp":1,"population":1}},
                           {{"x":6,"y":0,"level":1,"xp":1,"population":1}},
                           {{"x":{},"y":{},"level":0}},{{"x":{},"y":{},"level":0}}]}}"#,
        tiles.join(","),
        FORGE_A.x,
        FORGE_A.y,
        forge_b.x,
        forge_b.y
    );
    Engine::from_position(&Json::parse(&text).unwrap()).unwrap()
}

fn triggers(e: &Engine) -> Vec<(i32, i32)> {
    e.triggers.iter().map(|t| (t.city.x, t.city.y)).collect()
}

fn level(e: &Engine, at: Wc) -> u16 {
    e.improvement_rt(at).map(|r| r.level).unwrap()
}

#[test]
fn the_last_neighbour_in_area_order_raises_its_reward_first() {
    // The forges do not touch each other: (4,2) comes after (2,2) in area order, so its level-up runs first.
    let mut e = board(0, Wc { x: 4, y: 2 });
    e.apply_unchecked(Command::Build { at: SITE, improvement: MINE });
    assert_eq!(triggers(&e), [(CITY_B.x, CITY_B.y)], "the later forge's city asks first");
    assert!(e.deferred.contains(&Deferred::ImprovementLevelUp(FORGE_A)));

    e.apply(Command::CityReward { city: CITY_B, reward: polytopia_sim::CityReward::Workshop }).unwrap();
    assert_eq!(triggers(&e), [(CITY_A.x, CITY_A.y)]);
}

#[test]
fn a_level_up_raises_a_touching_neighbour_before_its_own_population() {
    // The forge on (3,2) runs first, and its surrounding-area check pushes a level-up for the touching forge on (2,2)
    // on top of its own population, so city A asks first after all. The level-up the build pushed for (2,2) is
    // dropped later: the forge is already at the level the mine gives.
    let forge_b = Wc { x: 3, y: 2 };
    let mut e = board(0, forge_b);
    e.apply_unchecked(Command::Build { at: SITE, improvement: MINE });
    assert_eq!(triggers(&e), [(CITY_A.x, CITY_A.y)]);
    assert_eq!((level(&e, FORGE_A), level(&e, forge_b)), (1, 1));

    e.apply(Command::CityReward { city: CITY_A, reward: polytopia_sim::CityReward::Workshop }).unwrap();
    assert_eq!(triggers(&e), [(CITY_B.x, CITY_B.y)]);
    e.apply(Command::CityReward { city: CITY_B, reward: polytopia_sim::CityReward::Workshop }).unwrap();
    assert!(e.triggers.is_empty());
    assert!(e.deferred.is_empty());
    assert_eq!((level(&e, FORGE_A), level(&e, forge_b)), (1, 1));
}

#[test]
fn the_buildings_own_population_runs_before_any_level_up() {
    // The mine's own two points raise the capital's reward; the forges' level-ups wait under it.
    let mut e = board(5, Wc { x: 4, y: 2 });
    e.apply_unchecked(Command::Build { at: SITE, improvement: MINE });
    assert_eq!(triggers(&e), [(HOME.x, HOME.y)]);
    assert_eq!(e.deferred.iter().filter(|d| matches!(d, Deferred::ImprovementLevelUp(_))).count(), 2);
    assert_eq!((level(&e, FORGE_A), level(&e, Wc { x: 4, y: 2 })), (0, 0));
}
