//! The Polaris spread: a Polaris player that takes a city freezes the tiles the city rules, a Polaris city that
//! grows its border freezes the new area, and a player of another tribe takes the Polaris climate back from the
//! tiles it owns.
//!
//! Evidence from the game: in the saved Polaris replays the 3x3 round a city a Polaris player owns ends as climate
//! 16 with its water turned to Ice (379 of 384 water tiles), and a city taken from Polaris gives its tiles back to
//! the new owner's climate. The replay case skips without the human game data.
use polytopia_sim::engine::{Command, Engine};
use polytopia_sim::json::Json;
use polytopia_sim::{CityReward, WorldCoordinates as Wc};

const WARRIOR: i64 = 2;
const WATER: i32 = 1;
const FIELD: i32 = 3;
const ICE: i32 = 6;
const POLARIS_CLIMATE: i32 = 16;
const OTHER_CLIMATE: i32 = 2;

fn wc(x: i32, y: i32) -> Wc {
    Wc { x, y }
}

/// A 9x5 board. Player 1 is Tribe `t1`, player 2 is Tribe `t2`. Player 2's city is at (6,2) and rules the 3x3
/// round it, where (5,3) and (6,3) are water. `owned_climate` is the climate of the tiles player 2's city rules.
fn board(t1: i32, t2: i32, owned_climate: i32, current: u8, units: &str) -> Engine {
    let mut tiles = Vec::new();
    for y in 0..5 {
        for x in 0..9 {
            let ruled = (5..=7).contains(&x) && (1..=3).contains(&y);
            let water = y == 3 && (5..=6).contains(&x);
            let climate = if ruled { owned_climate } else { OTHER_CLIMATE };
            let mut t =
                format!(r#""x":{x},"y":{y},"terrain":{},"climate":{climate}"#, if water { WATER } else { FIELD });
            if ruled {
                t.push_str(r#","owner":2,"ruling":[6,2]"#);
            }
            match (x, y) {
                (6, 2) => t.push_str(r#","city":true,"capital":true"#),
                (0, 0) => t.push_str(r#","city":true,"capital":true,"owner":1,"ruling":[0,0]"#),
                _ => {}
            }
            tiles.push(format!("{{{t}}}"));
        }
    }
    let text = format!(
        r#"{{"format":"polytopia-position","game":{{"turn":6,"width":9,"height":5,"current_player":{current}}},
           "players":[{{"id":1,"tribe":{t1},"human":true,"stars":5}},{{"id":2,"tribe":{t2},"stars":5}}],
           "tiles":[{}],
           "improvements":[{{"x":0,"y":0,"level":1}},{{"x":6,"y":2,"level":1}}],
           "units":[{units}]}}"#,
        tiles.join(",")
    );
    Engine::from_position(&Json::parse(&text).unwrap()).unwrap()
}

fn unit(id: u32, kind: i64, owner: u8, x: i32, y: i32) -> String {
    format!(r#"{{"id":{id},"type":{kind},"owner":{owner},"x":{x},"y":{y},"health":100}}"#)
}

/// Player 1's warrior stands in player 2's empty city and takes it.
fn capture(e: &mut Engine) {
    e.apply(Command::Capture { unit: 1, at: wc(6, 2) }).expect("the capture is legal");
    assert_eq!(e.tile(wc(6, 2)).unwrap().owner, 1, "the city changed hands");
}

#[test]
fn a_polaris_capture_freezes_the_tiles_the_city_rules() {
    let mut e = board(16, 3, OTHER_CLIMATE, 1, &unit(1, WARRIOR, 1, 6, 2));
    capture(&mut e);
    for y in 1..=3 {
        for x in 5..=7 {
            assert_eq!(e.tile(wc(x, y)).unwrap().climate, POLARIS_CLIMATE, "({x},{y}) takes the Polaris climate");
        }
    }
    for x in 5..=6 {
        assert_eq!(e.tile(wc(x, 3)).unwrap().terrain, ICE, "the water at ({x},3) turns to Ice");
    }
    assert_eq!(e.tile(wc(5, 2)).unwrap().terrain, FIELD, "land stays land");
    assert_eq!(e.tile(wc(4, 2)).unwrap().climate, OTHER_CLIMATE, "a tile outside the city is left alone");
}

#[test]
fn a_capture_by_another_tribe_takes_the_polaris_climate_back_from_land_only() {
    // Player 2 here is Polaris-climate land and water; player 1 (tribe 3) takes the city.
    let mut e = board(3, 16, POLARIS_CLIMATE, 1, &unit(1, WARRIOR, 1, 6, 2));
    e.tile_mut(wc(5, 3)).unwrap().terrain = ICE;
    capture(&mut e);
    let own = e.tile(wc(7, 1)).unwrap().climate;
    assert_ne!(own, POLARIS_CLIMATE, "land takes the captor's climate");
    assert_eq!(e.tile(wc(5, 3)).unwrap().climate, POLARIS_CLIMATE, "ice keeps the Polaris climate");
    assert_eq!(e.tile(wc(6, 3)).unwrap().terrain, WATER, "water stays water");
    assert_eq!(e.tile(wc(6, 3)).unwrap().climate, POLARIS_CLIMATE, "water keeps the Polaris climate");
}

#[test]
fn at_the_start_of_the_turn_a_player_takes_the_polaris_climate_back_from_its_own_land() {
    // Player 2 owns Polaris-climate land and water and is not Polaris. Player 1 ends its turn, so player 2 starts.
    let mut e = board(16, 3, POLARIS_CLIMATE, 1, &unit(1, WARRIOR, 1, 0, 1));
    e.apply(Command::EndTurn).unwrap();
    assert_ne!(e.tile(wc(7, 1)).unwrap().climate, POLARIS_CLIMATE, "land is restored");
    assert_eq!(e.tile(wc(6, 3)).unwrap().climate, POLARIS_CLIMATE, "water is left");
}

#[test]
fn a_polaris_border_growth_freezes_the_area_it_owns() {
    // Player 2 is Polaris and its city grows its border from 1 to 2.
    let mut e = board(3, 16, OTHER_CLIMATE, 2, &unit(1, WARRIOR, 1, 0, 1));
    e.apply_unchecked(Command::CityReward { city: wc(6, 2), reward: CityReward::BorderGrowth });
    for x in 5..=6 {
        assert_eq!(e.tile(wc(x, 3)).unwrap().terrain, ICE, "the water at ({x},3) turns to Ice");
    }
    assert_eq!(e.tile(wc(4, 2)).unwrap().climate, POLARIS_CLIMATE, "the grown border is frozen too");
    assert_eq!(e.tile(wc(3, 2)).unwrap().climate, OTHER_CLIMATE, "a tile outside the border is left alone");
}

#[test]
fn a_border_growth_by_another_tribe_takes_the_polaris_climate_back_from_the_new_land() {
    // Player 2 is not Polaris. A neutral tile just outside its border carries the Polaris climate (a Mooni walked
    // past); the city grows its border from 1 to 2 and rules it, so the tile takes the owner's climate.
    let mut e = board(16, 3, OTHER_CLIMATE, 2, &unit(1, WARRIOR, 1, 0, 1));
    e.tile_mut(wc(4, 2)).unwrap().climate = POLARIS_CLIMATE;
    e.tile_mut(wc(3, 2)).unwrap().climate = POLARIS_CLIMATE;
    e.apply_unchecked(Command::CityReward { city: wc(6, 2), reward: CityReward::BorderGrowth });
    assert_eq!(e.tile(wc(4, 2)).unwrap().owner, 2, "the grown border rules (4,2)");
    assert_ne!(e.tile(wc(4, 2)).unwrap().climate, POLARIS_CLIMATE, "the new land takes the owner's climate");
    assert_eq!(e.tile(wc(3, 2)).unwrap().climate, POLARIS_CLIMATE, "a tile outside the border is left alone");
}
