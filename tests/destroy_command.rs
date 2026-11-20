//! The destroy and clear-effect commands are checked on their own, not through the unit or improvement ability lists.
//!
//! Retail's destroy command is valid for the current player when no city reward is pending and the tile holds an
//! improvement. The Destroy ability and the destructable test only decide what the improvement's ability list
//! offers, so a recorded game can contain a Destroy that the list would not have shown.
use polytopia_sim::engine::{Command, Engine};
use polytopia_sim::json::Json;

const LUMBER_HUT: i64 = 12;
const LIGHTHOUSE: i64 = 47;

fn board(improvement: i64) -> Engine {
    board_with(improvement, 0)
}

fn board_with(improvement: i64, effect: i64) -> Engine {
    let mut tiles = Vec::new();
    for y in 0..3 {
        for x in 0..5 {
            let mut t = format!(r#""x":{x},"y":{y},"terrain":3,"climate":3,"explored":[1,2]"#);
            match (x, y) {
                (0, 0) => t.push_str(r#","city":true,"capital":true,"owner":1,"ruling":[0,0]"#),
                (4, 2) => t.push_str(r#","city":true,"capital":true,"owner":2,"ruling":[4,2]"#),
                (1, 1) => t.push_str(&format!(r#","improvement":{improvement},"owner":1,"ruling":[0,0]"#)),
                (2, 1) if effect != 0 => t.push_str(&format!(r#","effects":[{effect}],"owner":1,"ruling":[0,0]"#)),
                _ => {}
            }
            tiles.push(format!("{{{t}}}"));
        }
    }
    let text = format!(
        r#"{{"format":"polytopia-position","game":{{"turn":6,"width":5,"height":3,"current_player":1}},
           "players":[{{"id":1,"tribe":3,"human":true,"stars":5,"tech":[0],"skin":17}},{{"id":2,"tribe":4,"stars":5}}],
           "tiles":[{}],
           "improvements":[{{"x":0,"y":0,"level":1}},{{"x":4,"y":2,"level":1}},{{"x":1,"y":1,"level":0}}],
           "units":[]}}"#,
        tiles.join(",")
    );
    Engine::from_position(&Json::parse(&text).unwrap()).unwrap()
}

fn at(x: i32, y: i32) -> polytopia_sim::WorldCoordinates {
    polytopia_sim::WorldCoordinates { x, y }
}

#[test]
fn a_player_without_the_destroy_ability_is_not_offered_the_command() {
    let e = board(LUMBER_HUT);
    assert!(!e.legal_commands().contains(&Command::Destroy { at: at(1, 1) }));
}

#[test]
fn the_game_still_accepts_it_and_removes_the_improvement() {
    let mut e = board(LUMBER_HUT);
    e.apply(Command::Destroy { at: at(1, 1) }).unwrap();
    assert_eq!(e.tile(at(1, 1)).unwrap().improvement, 0);
}

#[test]
fn a_lighthouse_is_accepted_although_the_list_never_offers_it() {
    let e = board(LIGHTHOUSE);
    assert!(!e.legal_commands().contains(&Command::Destroy { at: at(1, 1) }));
    assert!(e.accepts(&Command::Destroy { at: at(1, 1) }));
}

#[test]
fn an_empty_tile_is_still_refused() {
    let e = board(LUMBER_HUT);
    assert!(!e.accepts(&Command::Destroy { at: at(2, 1) }));
}

#[test]
fn a_tile_effect_can_be_cleared_without_a_unit_on_it() {
    let mut e = board_with(0, i64::from(Engine::EFFECT_ALGAE));
    let cmd = Command::ClearTileEffect { at: at(2, 1), effect: Engine::EFFECT_ALGAE };
    assert!(!e.legal_commands().contains(&cmd));
    assert!(e.accepts(&cmd));
    e.apply(cmd).unwrap();
    assert!(!e.has_effect(at(2, 1), Engine::EFFECT_ALGAE));
}

#[test]
fn clearing_an_effect_the_tile_lacks_is_still_refused() {
    let e = board_with(0, i64::from(Engine::EFFECT_ALGAE));
    assert!(!e.accepts(&Command::ClearTileEffect { at: at(2, 1), effect: Engine::EFFECT_FLOODED }));
}
