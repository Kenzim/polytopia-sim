//! A sanctuary's level follows the game on the tiles around it. Retail's build action ends with the surrounding-area
//! check, so hunting the game next to a sanctuary takes its level down at once.
use polytopia_sim::engine::{Command, Engine};
use polytopia_sim::json::Json;
use polytopia_sim::WorldCoordinates as Wc;

const HUNTING: i32 = 9;

fn wc(x: i32, y: i32) -> Wc {
    Wc { x, y }
}

/// A 5x3 board owned by player 1, whose capital is at (0,0); a sanctuary at (1,1) stored at level 1 and game on (2,1).
fn board() -> Engine {
    let mut tiles = Vec::new();
    for y in 0..3 {
        for x in 0..5 {
            let mut t = format!(r#""x":{x},"y":{y},"terrain":3,"climate":2,"owner":1,"ruling":[0,0]"#);
            match (x, y) {
                (0, 0) => t.push_str(r#","city":true,"capital":true"#),
                (1, 1) => t.push_str(r#","improvement":32"#),
                (2, 1) => t.push_str(r#","resource":1"#),
                (4, 2) => t.push_str(r#","city":true,"capital":true,"owner":2,"ruling":[4,2]"#),
                _ => {}
            }
            tiles.push(format!("{{{t}}}"));
        }
    }
    let text = format!(
        r#"{{"format":"polytopia-position","game":{{"turn":6,"width":5,"height":3,"current_player":1}},
           "players":[{{"id":1,"tribe":3,"human":true,"stars":50,"tech":[1,2,3,4,5,6,7,8,9,10,11,12,13,14,15,16,17,18,19,20,21,22,23,24,25,26,27,28,29,30]}},{{"id":2,"tribe":2,"stars":5}}],
           "tiles":[{}],
           "improvements":[{{"x":0,"y":0,"level":1}},{{"x":4,"y":2,"level":1}},{{"x":1,"y":1,"level":1,"founded":3}}],
           "units":[]}}"#,
        tiles.join(",")
    );
    Engine::from_position(&Json::parse(&text).unwrap()).unwrap()
}

#[test]
fn hunting_the_game_next_to_a_sanctuary_takes_its_level_down() {
    let mut e = board();
    assert_eq!(e.improvement_rt(wc(1, 1)).unwrap().level, 1);
    e.apply(Command::Build { at: wc(2, 1), improvement: HUNTING }).unwrap();
    assert_eq!(e.improvement_rt(wc(1, 1)).unwrap().level, 0);
}

#[test]
fn a_level_down_does_nothing_while_the_neighbours_still_give_the_level() {
    // Retail's own check of a level-down: the stored level must be above the calculated one.
    let mut e = board();
    e.improvement_level_down(wc(1, 1));
    assert_eq!(e.improvement_rt(wc(1, 1)).unwrap().level, 1);
}

const SAWMILL: i32 = 13;
const MARKET: i32 = 50;

/// Retail's level-down action ends with the surrounding-area check: a market that took its level from a sawmill comes
/// down with it, in the same run of the action stack.
#[test]
fn a_level_down_checks_the_neighbours_that_took_their_level_from_it() {
    let mut e = board();
    // Sawmill at (2,1) with no lumber hut near it, market beside it; the stored levels were left over from a hut
    // that is gone.
    for (at, improvement, level) in [(wc(2, 1), SAWMILL, 3), (wc(3, 1), MARKET, 3)] {
        let i = e.tile(at).unwrap().index as usize;
        e.s.map.tiles[i].improvement = improvement;
        e.s.map.tiles[i].resource = 0;
        e.s.improvements.retain(|r| r.tile as usize != i);
        e.s.improvements.push(polytopia_sim::ImprovementRuntime {
            tile: i as _,
            owner: 1,
            founder: 1,
            level,
            ..Default::default()
        });
    }
    e.improvement_level_down(wc(2, 1));
    e.run_deferred();
    assert_eq!(e.improvement_rt(wc(3, 1)).unwrap().level, 1);
}
