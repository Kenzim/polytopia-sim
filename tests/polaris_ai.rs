//! The Polaris pieces of the AI: what counts as freezable, how an ice bank is scored, and the scores of the
//! freeze-area and break-ice commands.
//!
//! The AI scores a freeze-area command at 50 for every freezable tile in the square round the unit (its own tile
//! included) and a break-ice command at 50 for every breakable ice tile beside it. A Polaris player never breaks ice.
//! An ice bank is worth more the more frozen tiles the player owns, one extra level for every twenty.
use polytopia_sim::engine::{Command, Engine};
use polytopia_sim::json::Json;
use polytopia_sim::WorldCoordinates as Wc;

const WARRIOR: i64 = 2;
const RIDER: i64 = 3;
const GAAMI: i64 = 27;
const WATER: i32 = 1;
const FIELD: i32 = 3;
const ICE: i32 = 6;
const ICE_BANK: i32 = 34;
const POLARIS: i32 = 16;
const BARDUR: i32 = 3;

fn wc(x: i32, y: i32) -> Wc {
    Wc { x, y }
}

fn unit(id: u32, kind: i64, owner: u8, x: i32, y: i32, extra: &str) -> String {
    format!(r#"{{"id":{id},"type":{kind},"owner":{owner},"x":{x},"y":{y},"health":100{extra}}}"#)
}

/// A `w` x `h` board with player 1 (the given tribe, played by the AI, to move) against player 2. `tile` gives the
/// terrain and climate of a tile; `owned` says whether player 1 owns it; the capitals sit in the top corners.
fn board(
    tribe: i32,
    w: i32,
    h: i32,
    tile: impl Fn(i32, i32) -> (i32, i32),
    owned: impl Fn(i32, i32) -> bool,
    units: &str,
    stars: i32,
) -> Engine {
    board_in(123, tribe, w, h, tile, owned, units, stars)
}

#[allow(clippy::too_many_arguments)]
fn board_in(
    version: i32,
    tribe: i32,
    w: i32,
    h: i32,
    tile: impl Fn(i32, i32) -> (i32, i32),
    owned: impl Fn(i32, i32) -> bool,
    units: &str,
    stars: i32,
) -> Engine {
    let mut tiles = Vec::new();
    let improvements = [r#"{"x":0,"y":0,"level":1}"#.to_string(), format!(r#"{{"x":{},"y":0,"level":1}}"#, w - 1)];
    for y in 0..h {
        for x in 0..w {
            let (terrain, climate) = tile(x, y);
            let mut t = format!(r#""x":{x},"y":{y},"terrain":{terrain},"climate":{climate}"#);
            if (x, y) == (0, 0) {
                t.push_str(r#","city":true,"capital":true,"owner":1,"ruling":[0,0]"#);
            } else if (x, y) == (w - 1, 0) {
                t.push_str(&format!(r#","city":true,"capital":true,"owner":2,"ruling":[{},0]"#, w - 1));
            } else if owned(x, y) {
                t.push_str(r#","owner":1,"ruling":[0,0]"#);
            }
            tiles.push(format!("{{{t}}}"));
        }
    }
    let text = format!(
        r#"{{"format":"polytopia-position","game":{{"game_version":{version},"turn":6,"width":{w},"height":{h},"current_player":1}},
           "players":[{{"id":1,"tribe":{tribe},"stars":{stars}}},{{"id":2,"tribe":3,"stars":5}}],
           "tiles":[{}],
           "improvements":[{}],
           "units":[{units}]}}"#,
        tiles.join(","),
        improvements.join(",")
    );
    Engine::from_position(&Json::parse(&text).unwrap()).unwrap()
}

fn score_of(e: &Engine, u: u32, wanted: impl Fn(&Command) -> bool) -> Option<f32> {
    let unit = e.unit(u).unwrap().clone();
    e.unit_move_candidates(&unit).into_iter().find(|(c, _)| wanted(c)).map(|c| c.1)
}

/// 7x3 in the Polaris climate, with water in row 2 from x=2 to x=4.
fn water_row(tribe: i32, units: &str) -> Engine {
    board(
        tribe,
        7,
        3,
        |x, y| if y == 2 && (2..=4).contains(&x) { (WATER, POLARIS) } else { (FIELD, POLARIS) },
        |_, _| false,
        units,
        5,
    )
}

#[test]
fn only_water_and_other_climates_are_freezable_to_a_polaris_player() {
    let e = board(
        POLARIS,
        5,
        3,
        |x, _| {
            if x == 0 {
                (WATER, POLARIS)
            } else if x == 4 {
                (FIELD, 2)
            } else {
                (FIELD, POLARIS)
            }
        },
        |_, _| false,
        &unit(1, WARRIOR, 1, 2, 1, ""),
        5,
    );
    assert!(e.is_freezable(wc(0, 1), 1), "water");
    assert!(e.is_freezable(wc(4, 1), 1), "land of another climate");
    assert!(!e.is_freezable(wc(1, 1), 1), "land already in the Polaris climate");
    assert!(!e.is_freezable(wc(2, 1), 1), "the player's own unit does not make a tile freezable");
}

#[test]
fn an_enemy_unit_makes_a_polaris_tile_freezable() {
    let e = board(
        POLARIS,
        5,
        3,
        |_, _| (FIELD, POLARIS),
        |_, _| false,
        &[unit(1, WARRIOR, 1, 1, 1, ""), unit(2, WARRIOR, 2, 3, 1, "")].join(","),
        5,
    );
    assert!(e.is_freezable(wc(3, 1), 1));
    assert!(!e.is_freezable(wc(2, 1), 1));
}

#[test]
fn the_ai_scores_freeze_area_at_fifty_per_freezable_tile() {
    // Three water tiles beside the Gaami and nothing else to freeze.
    let e = water_row(POLARIS, &unit(1, GAAMI, 1, 3, 1, ""));
    let s = score_of(&e, 1, |c| matches!(c, Command::FreezeArea { unit: 1 })).expect("the AI considers freezing");
    assert_eq!(s, 150.0);
}

#[test]
fn an_enemy_unit_adds_to_the_freeze_area_score() {
    let e = water_row(POLARIS, &[unit(1, GAAMI, 1, 3, 1, ""), unit(2, WARRIOR, 2, 4, 1, "")].join(","));
    let s = score_of(&e, 1, |c| matches!(c, Command::FreezeArea { unit: 1 })).unwrap();
    assert_eq!(s, 200.0, "three water tiles and the tile of the enemy warrior");
}

#[test]
fn a_polaris_player_never_scores_break_ice() {
    let tile = |x: i32, y: i32| if y == 2 && (2..=4).contains(&x) { (ICE, POLARIS) } else { (FIELD, 2) };
    let e = board(POLARIS, 7, 3, tile, |_, _| false, &unit(1, WARRIOR, 1, 3, 1, ""), 5);
    assert!(e.legal_commands().contains(&Command::BreakIce { unit: 1 }), "the command is legal");
    assert!(score_of(&e, 1, |c| matches!(c, Command::BreakIce { .. })).is_none());
}

#[test]
fn another_tribe_scores_break_ice_at_fifty_per_ice_tile_beside_the_unit() {
    let tile = |x: i32, y: i32| if y == 2 && (2..=4).contains(&x) { (ICE, POLARIS) } else { (FIELD, 2) };
    let e = board(BARDUR, 7, 3, tile, |_, _| false, &unit(1, WARRIOR, 1, 3, 1, ""), 5);
    let s = score_of(&e, 1, |c| matches!(c, Command::BreakIce { unit: 1 })).expect("the AI considers breaking ice");
    assert_eq!(s, 150.0);
}

#[test]
fn a_dash_unit_that_has_moved_still_breaks_ice() {
    // A moved rider cannot move or attack, so only the "can perform any ability" check lets the AI look at it.
    let tile = |x: i32, y: i32| if y == 2 && (2..=4).contains(&x) { (ICE, POLARIS) } else { (FIELD, 2) };
    let mut e = board(BARDUR, 7, 3, tile, |_, _| false, &unit(1, RIDER, 1, 3, 1, r#","moved":true"#), 0);
    assert_eq!(e.ai_get_move(), Command::BreakIce { unit: 1 });
}

#[test]
fn an_ice_bank_is_worth_more_with_more_frozen_tiles() {
    // Forty owned tiles, all frozen or none of them. Only the rules of game versions 100 to 103 give the ice bank
    // the Polaris climate as an adjacency.
    let bank_score = |climate: i32| {
        let e =
            board_in(100, POLARIS, 8, 6, move |_, _| (FIELD, climate), |_, _| true, &unit(1, WARRIOR, 1, 4, 3, ""), 50);
        e.ai_improvement_score(1, wc(4, 4), ICE_BANK)
    };
    let (cold, warm) = (bank_score(POLARIS), bank_score(2));
    assert!(warm > 0.0, "the bank can be built here");
    assert!(cold > warm, "frozen tiles raise the score: {cold} against {warm}");
}
