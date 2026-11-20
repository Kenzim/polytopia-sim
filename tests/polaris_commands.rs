//! The two Polaris unit commands: the freeze-area command of a Gaami and the break-ice command of any unit beside
//! breakable ice, and the ice bank levels they keep up to date.
//!
//! Freeze-area spends the unit and freezes the tiles and enemy units round it. Break-ice spends the unit and turns the
//! ice round it back to water (shallow when land that is not ice lies beside it, deep otherwise) and clears its skin.
//! The level of an ice bank follows the number of frozen tiles on the map, one level for every twenty.
use polytopia_sim::engine::{Command, Engine};
use polytopia_sim::json::Json;
use polytopia_sim::WorldCoordinates as Wc;

const WARRIOR: i64 = 2;
const GAAMI: i64 = 27;
const WATER: i32 = 1;
const OCEAN: i32 = 2;
const FIELD: i32 = 3;
const ICE: i32 = 6;
const ICE_BANK: i32 = 34;

fn wc(x: i32, y: i32) -> Wc {
    Wc { x, y }
}

fn unit(id: u32, kind: i64, owner: u8, x: i32, y: i32) -> String {
    format!(r#"{{"id":{id},"type":{kind},"owner":{owner},"x":{x},"y":{y},"health":100}}"#)
}

/// A `w` x `h` board for Polaris (player 1, to move) against player 2. `tile` gives the terrain and climate of a
/// tile; the capitals sit in the top corners; an ice bank stands on `bank` when given, at the level `bank_level`.
fn board(
    w: i32,
    h: i32,
    tile: impl Fn(i32, i32) -> (i32, i32),
    bank: Option<Wc>,
    bank_level: u32,
    units: &str,
) -> Engine {
    let mut tiles = Vec::new();
    let mut improvements =
        vec![r#"{"x":0,"y":0,"level":1}"#.to_string(), format!(r#"{{"x":{},"y":0,"level":1}}"#, w - 1)];
    for y in 0..h {
        for x in 0..w {
            let (terrain, climate) = tile(x, y);
            let mut t = format!(r#""x":{x},"y":{y},"terrain":{terrain},"climate":{climate}"#);
            if (x, y) == (0, 0) {
                t.push_str(r#","city":true,"capital":true,"owner":1,"ruling":[0,0]"#);
            } else if (x, y) == (w - 1, 0) {
                t.push_str(&format!(r#","city":true,"capital":true,"owner":2,"ruling":[{},0]"#, w - 1));
            } else if bank == Some(wc(x, y)) {
                t.push_str(&format!(r#","improvement":{ICE_BANK},"owner":1,"ruling":[0,0]"#));
                improvements.push(format!(r#"{{"x":{x},"y":{y},"level":{bank_level}}}"#));
            }
            tiles.push(format!("{{{t}}}"));
        }
    }
    let text = format!(
        r#"{{"format":"polytopia-position","game":{{"turn":6,"width":{w},"height":{h},"current_player":1}},
           "players":[{{"id":1,"tribe":16,"human":true,"stars":5}},{{"id":2,"tribe":3,"stars":5}}],
           "tiles":[{}],
           "improvements":[{}],
           "units":[{units}]}}"#,
        tiles.join(","),
        improvements.join(",")
    );
    Engine::from_position(&Json::parse(&text).unwrap()).unwrap()
}

/// 7x3: row 2 is water from x=2 to x=4, the rest field in the climate of the second tribe.
fn water_row(units: &str) -> Engine {
    board(7, 3, |x, y| if y == 2 && (2..=4).contains(&x) { (WATER, 2) } else { (FIELD, 2) }, None, 0, units)
}

fn terrain(e: &Engine, x: i32, y: i32) -> i32 {
    e.tile(wc(x, y)).unwrap().terrain
}

#[test]
fn a_gaami_freezes_the_area_round_it_and_is_spent() {
    let units = [unit(1, GAAMI, 1, 3, 1), unit(2, WARRIOR, 2, 4, 1)].join(",");
    let mut e = water_row(&units);
    assert!(e.legal_commands().contains(&Command::FreezeArea { unit: 1 }), "a fresh Gaami beside water can freeze");
    e.apply(Command::FreezeArea { unit: 1 }).expect("the command is legal");
    for x in 2..=4 {
        assert_eq!(terrain(&e, x, 2), ICE, "({x},2) was water beside the Gaami");
    }
    let foe = e.unit(2).unwrap();
    assert!(foe.frozen && foe.moved && foe.attacked, "the enemy warrior is frozen: {foe:?}");
    let gaami = e.unit(1).unwrap();
    assert!(!gaami.frozen && gaami.moved && gaami.attacked, "the Gaami is spent but not frozen: {gaami:?}");
    assert_eq!(e.tile(wc(2, 1)).unwrap().climate, 16, "a field tile takes the Polaris climate");
    assert!(!e.legal_commands().contains(&Command::FreezeArea { unit: 1 }));
}

#[test]
fn freeze_area_is_only_for_a_unit_that_has_it_and_has_not_moved() {
    let units = [unit(1, WARRIOR, 1, 3, 1), unit(2, GAAMI, 1, 2, 1)].join(",");
    let mut e = water_row(&units);
    assert!(
        !e.legal_commands().iter().any(|c| matches!(c, Command::FreezeArea { unit: 1 })),
        "a warrior cannot freeze"
    );
    assert!(e.legal_commands().contains(&Command::FreezeArea { unit: 2 }));
    e.apply(Command::Move { unit: 2, to: wc(1, 1) }).expect("the Gaami moves");
    assert!(
        !e.legal_commands().iter().any(|c| matches!(c, Command::FreezeArea { unit: 2 })),
        "a unit that has moved cannot freeze"
    );
}

#[test]
fn break_ice_turns_the_ice_beside_the_unit_back_to_shallow_water_and_clears_the_skin() {
    let mut e = board(
        7,
        3,
        |x, y| if y == 2 && (2..=4).contains(&x) { (ICE, 16) } else { (FIELD, 2) },
        None,
        0,
        &unit(1, WARRIOR, 1, 3, 1),
    );
    e.tile_mut(wc(3, 2)).unwrap().skin = 5;
    assert!(e.legal_commands().contains(&Command::BreakIce { unit: 1 }));
    e.apply(Command::BreakIce { unit: 1 }).expect("the command is legal");
    for x in 2..=4 {
        assert_eq!(terrain(&e, x, 2), WATER, "({x},2) is ice with land beside it, so it breaks to shallow water");
    }
    assert_eq!(e.tile(wc(3, 2)).unwrap().skin, 0);
    let u = e.unit(1).unwrap();
    assert!(u.moved && u.attacked, "the unit is spent");
    assert!(!e.legal_commands().iter().any(|c| matches!(c, Command::BreakIce { .. })), "no ice is left beside it");
}

#[test]
fn ice_with_no_land_beside_it_breaks_to_deep_water() {
    // A lone warrior on a field in the middle of a board of ice: the ice above it touches its field, the ice at
    // the diagonal has no land beside it.
    let mut e =
        board(5, 5, |x, y| if (x, y) == (2, 2) { (FIELD, 2) } else { (ICE, 16) }, None, 0, &unit(1, WARRIOR, 1, 2, 2));
    e.apply(Command::BreakIce { unit: 1 }).expect("the command is legal");
    assert_eq!(terrain(&e, 2, 1), WATER, "ice above the unit touches its field");
    assert_eq!(terrain(&e, 1, 1), OCEAN, "ice at the diagonal has no land beside it");
    assert_eq!(terrain(&e, 0, 2), ICE, "two tiles away is untouched");
}

#[test]
fn break_ice_is_not_offered_without_ice_or_after_a_move() {
    let mut e = board(
        7,
        3,
        |x, y| if y == 2 && (2..=4).contains(&x) { (ICE, 16) } else { (FIELD, 2) },
        None,
        0,
        &unit(1, WARRIOR, 1, 3, 1),
    );
    e.apply(Command::Move { unit: 1, to: wc(3, 0) }).expect("the warrior moves");
    assert!(!e.legal_commands().iter().any(|c| matches!(c, Command::BreakIce { .. })), "moved, and no ice beside it");
    let e = water_row(&unit(1, WARRIOR, 1, 3, 1));
    assert!(!e.legal_commands().iter().any(|c| matches!(c, Command::BreakIce { .. })), "water is not ice");
}

/// 8x5 with 19 frozen tiles (climate 16 on land counts), an ice bank in the corner.
fn bank_board(units: &str) -> Engine {
    let polar = |x: i32, y: i32| x < 3 || (x == 3 && y < 4);
    board(8, 5, move |x, y| (FIELD, if polar(x, y) { 16 } else { 2 }), Some(wc(0, 4)), 0, units)
}

#[test]
fn freezing_tiles_raises_the_ice_bank_level() {
    let mut e = bank_board(&unit(1, GAAMI, 1, 5, 2));
    assert_eq!(e.improvement_rt(wc(0, 4)).unwrap().level, 0);
    e.apply(Command::FreezeArea { unit: 1 }).expect("the command is legal");
    assert_eq!(e.improvement_rt(wc(0, 4)).unwrap().level, 1, "more than twenty tiles are frozen now");
}

#[test]
fn breaking_ice_lowers_the_ice_bank_level() {
    // Twenty frozen tiles: eighteen of Polaris climate, two of ice beside the warrior.
    let polar = |x: i32, y: i32| x < 3 || (x == 3 && y < 3);
    let mut e = board(
        8,
        5,
        move |x, y| {
            if (x, y) == (5, 2) || (x, y) == (5, 3) {
                (ICE, 2)
            } else {
                (FIELD, if polar(x, y) { 16 } else { 2 })
            }
        },
        Some(wc(0, 4)),
        1,
        &unit(1, WARRIOR, 1, 6, 2),
    );
    e.apply(Command::BreakIce { unit: 1 }).expect("the command is legal");
    assert_eq!(terrain(&e, 5, 2), WATER);
    assert_eq!(e.improvement_rt(wc(0, 4)).unwrap().level, 0, "fewer than twenty tiles are frozen now");
}
