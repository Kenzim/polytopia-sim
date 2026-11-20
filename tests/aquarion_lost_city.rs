//! The Lost City: an Aquarion unit that examines a ruin on open ocean founds a city there.
//!
//! Retail's ruins reward (game version above 119) checks first whether the ruin tile is not land, the player's tribe has
//! the Flooded ability and the tile is a valid Lost City location (an unowned ocean tile, off the map edge, with no city
//! or ruin within two tiles); then the reward is always the Lost City. The four tiles next to the ruin turn to shallow
//! water (ice stays), the tile becomes a flooded field with a new city that the player captures, and resources are drawn
//! for the free tiles around it with the player's tribe weights (every tile next to it, a third of the others within two).
//!
//! Evidence: in Aquarion human games (version 123, 11x11 and 14x14) the server's board after examining an ocean ruin
//! shows exactly this; replaying them, the engine now matches the server's terrain, resources and improvements on every
//! tile of the games that held a Lost City, and the refused commands that came from the missing city are gone. The
//! examiner moves into the new city as a capturer does: its home is the city (so it no longer counts against the city it
//! was trained in, which let that city train again).
use polytopia_sim::engine::{Command, Engine};
use polytopia_sim::json::Json;
use polytopia_sim::WorldCoordinates as Wc;

const AMPHIBIAN: i64 = 20;
const AQUARION: i64 = 3;
const BARDUR: i64 = 4;

fn wc(x: i32, y: i32) -> Wc {
    Wc { x, y }
}

/// Player 1 (`tribe`) with an amphibian standing on a ruin at (5,5) of a 11x11 ocean (version `version`); extra
/// improvements and tile changes go in `extra`, a list of `(x, y, json fragment)`.
fn board(tribe: i64, version: i64, extra: &[(i32, i32, &str)]) -> Engine {
    let mut tiles = Vec::new();
    for y in 0..11 {
        for x in 0..11 {
            let land = (x, y) == (0, 0) || (x, y) == (10, 10);
            let terrain = if land {
                3
            } else {
                extra
                    .iter()
                    .find(|e| (e.0, e.1) == (x, y) && e.2.starts_with(r#""terrain":"#))
                    .map_or(2, |e| e.2[10..].parse::<i32>().unwrap())
            };
            let mut t = format!(r#""x":{x},"y":{y},"terrain":{terrain},"climate":3,"explored":[1,2]"#);
            match (x, y) {
                (0, 0) => t.push_str(r#","city":true,"capital":true,"owner":1,"ruling":[0,0]"#),
                (10, 10) => t.push_str(r#","city":true,"capital":true,"owner":2,"ruling":[10,10]"#),
                (5, 5) => t.push_str(r#","improvement":2"#),
                _ => {}
            }
            for (ex, ey, frag) in extra {
                if (*ex, *ey) == (x, y) && !frag.starts_with(r#""terrain":"#) {
                    t.push(',');
                    t.push_str(frag);
                }
            }
            tiles.push(format!("{{{t}}}"));
        }
    }
    let text = format!(
        r#"{{"format":"polytopia-position","game":{{"turn":6,"width":11,"height":11,"current_player":1,"game_version":{version},"seed":7}},
           "players":[{{"id":1,"tribe":{tribe},"human":true,"stars":5,"known":[2]}},{{"id":2,"tribe":{BARDUR},"stars":5,"known":[1]}}],
           "tiles":[{}],
           "improvements":[{{"x":0,"y":0,"level":1}},{{"x":10,"y":10,"level":1}}],
           "units":[{{"id":1,"type":{AMPHIBIAN},"owner":1,"x":5,"y":5}}]}}"#,
        tiles.join(",")
    );
    Engine::from_position(&Json::parse(&text).unwrap()).unwrap()
}

#[test]
fn an_aquarion_ruin_on_open_ocean_is_a_lost_city() {
    let mut e = board(AQUARION, 123, &[]);
    assert_eq!(e.ruins_reward(1, wc(5, 5)), 9);
    e.apply_unchecked(Command::ExamineRuins { unit: 1 });
    let city = e.tile(wc(5, 5)).unwrap();
    assert_eq!((city.improvement, city.terrain, city.owner), (1, 3, 1), "a field with a city of the player's");
    assert!(city.is_city && city.effects & (1 << 1) != 0, "flooded");
    assert_eq!(e.improvement_rt(wc(5, 5)).map(|r| (r.level, r.border_size)), Some((1, 1)));
    for (x, y) in [(5, 4), (5, 6), (4, 5), (6, 5)] {
        assert_eq!(e.tile(wc(x, y)).unwrap().terrain, 1, "({x},{y}) next to the ruin is shallow water now");
    }
    for (x, y) in [(4, 4), (6, 6), (4, 6), (6, 4)] {
        assert_eq!(e.tile(wc(x, y)).unwrap().terrain, 2, "({x},{y}) diagonal to it stays ocean");
        assert_eq!(e.tile(wc(x, y)).unwrap().owner, 1, "but inside the new border");
    }
    assert!(!e.tile(wc(5, 5)).unwrap().has_ruin);
}

#[test]
fn the_examiner_of_a_lost_city_takes_it_as_its_home() {
    let mut e = board(AQUARION, 123, &[]);
    assert_eq!(e.unit(1).unwrap().home, wc(-1, -1));
    e.apply_unchecked(Command::ExamineRuins { unit: 1 });
    assert_eq!(e.unit(1).unwrap().home, wc(5, 5));
    assert_eq!(e.city_unit_count(wc(5, 5)), 1);
}

#[test]
fn ice_next_to_the_ruin_stays_ice() {
    let mut e = board(AQUARION, 123, &[(5, 4, r#""terrain":6"#)]);
    e.apply_unchecked(Command::ExamineRuins { unit: 1 });
    assert_eq!(e.tile(wc(5, 4)).unwrap().terrain, 6);
    assert_eq!(e.tile(wc(5, 6)).unwrap().terrain, 1);
}

#[test]
fn other_tribes_never_get_a_lost_city() {
    let e = board(BARDUR, 123, &[]);
    assert_ne!(e.ruins_reward(1, wc(5, 5)), 9);
}

#[test]
fn an_old_game_version_has_no_lost_city() {
    let e = board(AQUARION, 119, &[]);
    assert_ne!(e.ruins_reward(1, wc(5, 5)), 9);
}

#[test]
fn the_location_must_be_free_ocean_off_the_edge() {
    // another ruin, a city, or an owner within the rule; each one alone rules it out
    for extra in [
        &[(7, 5, r#""improvement":2"#)][..],
        &[(3, 3, r#""improvement":1,"city":true"#)][..],
        &[(5, 5, r#""owner":2"#)][..],
        &[(5, 5, r#""terrain":1"#)][..],
    ] {
        let e = board(AQUARION, 123, extra);
        assert_ne!(e.ruins_reward(1, wc(5, 5)), 9, "{extra:?}");
    }
    // two tiles away is too close, three is far enough
    assert_ne!(board(AQUARION, 123, &[(7, 7, r#""improvement":2"#)]).ruins_reward(1, wc(5, 5)), 9);
    assert_eq!(board(AQUARION, 123, &[(8, 5, r#""improvement":2"#)]).ruins_reward(1, wc(5, 5)), 9);
}
