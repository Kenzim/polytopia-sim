//! The step into a defeated unit's tile is a full move action.
//!
//! A melee unit that kills its target steps onto the tile. Retail queues a normal move for it, so everything a move
//! does on arrival happens: an Aquarion amphibian that lands on flooded ground gets the Bubble (and with it one more
//! movement point), a crab (Autoflood) floods the tile it steps onto, and an amphibian that lands on dry ground drops
//! its Bubble.
//!
//! Evidence from the games: in three Aquarion human games an amphibian or mermaid warrior made a longer move than its
//! movement allows right after a kill; the engine refused them because the unit had no Bubble after the step.
use polytopia_sim::engine::{Command, Engine};
use polytopia_sim::json::Json;
use polytopia_sim::WorldCoordinates as Wc;

const AMPHIBIAN: i64 = 20;
const CRAB: i64 = 26;
const WARRIOR: i64 = 2;
const FLOODED: u16 = Engine::EFFECT_FLOODED;

fn wc(x: i32, y: i32) -> Wc {
    Wc { x, y }
}

/// A 9x3 board of fields: player 1 (Aquarion) has `attacker` at (3,1), player 2 a weak warrior at (4,1).
/// `tile` adds json to single tiles.
fn board(attacker: i64, tile: &[(i32, i32, &str)]) -> Engine {
    let mut tiles = Vec::new();
    for y in 0..3 {
        for x in 0..9 {
            let mut t = format!(r#""x":{x},"y":{y},"terrain":3,"climate":3,"explored":[1,2]"#);
            match (x, y) {
                (0, 0) => t.push_str(r#","city":true,"capital":true,"owner":1,"ruling":[0,0]"#),
                (8, 2) => t.push_str(r#","city":true,"capital":true,"owner":2,"ruling":[8,2]"#),
                _ => {}
            }
            for (tx, ty, extra) in tile {
                if (*tx, *ty) == (x, y) {
                    t.push(',');
                    t.push_str(extra);
                }
            }
            tiles.push(format!("{{{t}}}"));
        }
    }
    let text = format!(
        r#"{{"format":"polytopia-position","game":{{"turn":6,"width":9,"height":3,"current_player":1}},
           "players":[{{"id":1,"tribe":3,"human":true,"stars":5,"tech":[0,45,48],"skin":17}},{{"id":2,"tribe":4,"stars":5}}],
           "tiles":[{}],
           "improvements":[{{"x":0,"y":0,"level":1}},{{"x":8,"y":2,"level":1}}],
           "units":[{{"id":1,"type":{attacker},"owner":1,"x":3,"y":1,"health":100}},
                    {{"id":2,"type":{WARRIOR},"owner":2,"x":4,"y":1,"health":10}}]}}"#,
        tiles.join(",")
    );
    Engine::from_position(&Json::parse(&text).unwrap()).unwrap()
}

fn attack(e: &mut Engine) {
    e.apply(Command::Attack { unit: 1, target: wc(4, 1) }).expect("the attack is legal");
    assert!(e.unit(2).is_none(), "the warrior is dead");
    assert_eq!(e.unit(1).unwrap().coordinates, wc(4, 1), "the attacker stepped in");
}

#[test]
fn an_amphibian_that_steps_onto_flooded_ground_gets_the_bubble() {
    let mut e = board(AMPHIBIAN, &[(4, 1, r#""effects":[1]"#)]);
    attack(&mut e);
    assert!(e.unit(1).unwrap().bubble);
}

#[test]
fn an_amphibian_that_steps_onto_dry_ground_has_no_bubble() {
    let mut e = board(AMPHIBIAN, &[]);
    attack(&mut e);
    assert!(!e.unit(1).unwrap().bubble);
}

#[test]
fn a_crab_floods_the_tile_it_steps_onto() {
    let mut e = board(CRAB, &[]);
    attack(&mut e);
    assert!(e.has_effect(wc(4, 1), FLOODED));
}
