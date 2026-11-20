//! Retail prices techs (and sizes the AI's unit need and research scores) from the player record's own city counter,
//! not from the cities on the map. The counter is whatever the file records (a hand-made challenge can disagree with
//! its map), gains a city on each capture and loses one for the previous owner. A village has no previous owner.
//!
//! The board is a hand-made 10x10 position; the replay test reads this week's challenge rehearsal dumps from
//! `POLYTOPIA_WEEKLY_SCRATCH`, and skips without them.
use polytopia_sim::engine::replay::SaveFile;
use polytopia_sim::engine::tables::{improvement, player_ability};
use polytopia_sim::engine::{Command, Engine, GameOptions};
use polytopia_sim::json::Json;
use polytopia_sim::rules::economy::tech_price;
use polytopia_sim::WorldCoordinates as Wc;

const C1: Wc = Wc { x: 1, y: 1 };
const C2: Wc = Wc { x: 8, y: 1 };
const S2: Wc = Wc { x: 8, y: 4 };
const VILLAGE: Wc = Wc { x: 4, y: 8 };

/// Player 1 to move with a warrior on `warrior_at`. Player 2 owns a capital and a second city; `cities2` is the
/// counter its record claims (`None`: left out, so it is counted from the map).
fn board(warrior_at: Wc, cities2: Option<i32>) -> Engine {
    let owned = [(C1, 1, true), (C2, 2, true), (S2, 2, false)];
    let mut tiles = Vec::new();
    for y in 0..10 {
        for x in 0..10 {
            let mut t = format!(r#""x":{x},"y":{y},"terrain":3,"climate":2"#);
            if let Some((_, owner, capital)) = owned.iter().find(|(c, _, _)| c.x == x && c.y == y) {
                t.push_str(&format!(r#","city":true,"capital":{capital},"owner":{owner}"#));
            } else if x == VILLAGE.x && y == VILLAGE.y {
                t.push_str(r#","city":true"#);
            }
            tiles.push(format!("{{{t}}}"));
        }
    }
    let counter = cities2.map(|n| format!(r#","cities":{n}"#)).unwrap_or_default();
    let text = format!(
        r#"{{"format":"polytopia-position","game":{{"turn":2,"width":10,"height":10,"current_player":1}},
           "players":[{{"id":1,"tribe":2,"human":true}},{{"id":2,"tribe":3{counter}}}],
           "tiles":[{}],
           "units":[{{"type":2,"owner":1,"x":{},"y":{}}}]}}"#,
        tiles.join(","),
        warrior_at.x,
        warrior_at.y
    );
    Engine::from_position(&Json::parse(&text).unwrap()).unwrap()
}

fn counter(e: &Engine, player: u8) -> i32 {
    e.player_runtime(player).unwrap().cities
}

fn capture(e: &mut Engine, at: Wc) {
    let unit = e.s.units.iter().find(|u| u.coordinates == at && u.owner == 1).unwrap().id;
    e.apply(Command::Capture { unit, at }).expect("capture is legal");
}

/// A researchable tech of player 1, its base cost, and whether the player's price carries the literacy factor.
fn first_tech(e: &Engine) -> (i32, i32, bool) {
    let tech =
        e.researchable(1).into_iter().find(|t| e.t.tech(*t).is_some_and(|d| d.cost > 0)).expect("a researchable tech");
    (tech, e.t.tech(tech).unwrap().cost, e.has_player_ability(1, player_ability::LITERACY))
}

#[test]
fn a_tech_costs_what_the_recorded_counter_says_not_what_the_map_holds() {
    let mut e = board(C1, None);
    e.s.players.iter_mut().find(|p| p.id == 1).unwrap().currency = 100;
    assert_eq!((counter(&e, 1), e.city_count(1)), (1, 1), "a position without a counter counts the map");
    let (tech, base, lit) = first_tech(&e);
    assert_eq!(e.tech_price(1, tech), tech_price(base, 1, lit));

    // The record says three cities though the map holds one.
    e.s.player_runtime.iter_mut().find(|r| r.player == 1).unwrap().cities = 3;
    assert_eq!(e.city_count(1), 1);
    assert_eq!(e.tech_price(1, tech), tech_price(base, 3, lit));
    assert!(e.tech_price(1, tech) > tech_price(base, 1, lit));

    // Research charges that price.
    let price = e.tech_price(1, tech);
    e.apply(Command::Research { tech }).expect("research is legal");
    assert_eq!(e.player(1).unwrap().currency, 100 - price);
}

#[test]
fn a_capture_moves_one_city_between_the_counters_whatever_they_hold() {
    let mut e = board(S2, Some(5));
    assert_eq!((counter(&e, 1), counter(&e, 2), e.city_count(2)), (1, 5, 2));
    capture(&mut e, S2);
    assert_eq!((counter(&e, 1), counter(&e, 2)), (2, 4), "the capturer gains one, the previous owner loses one");
    assert_eq!((e.city_count(1), e.city_count(2)), (2, 1));
}

#[test]
fn capturing_a_village_costs_nobody_a_city() {
    let mut e = board(VILLAGE, None);
    assert_eq!((counter(&e, 1), counter(&e, 2)), (1, 2));
    capture(&mut e, VILLAGE);
    assert_eq!((counter(&e, 1), counter(&e, 2)), (2, 2));
}

#[test]
fn a_new_game_counts_each_generated_capital() {
    let e = Engine::new_game(&GameOptions::default());
    for p in e.s.players.iter().filter(|p| p.id != polytopia_sim::NATURE) {
        assert_eq!((counter(&e, p.id), e.city_count(p.id)), (1, 1), "player {}", p.id);
    }
    let (tech, base, lit) = first_tech(&e);
    assert_eq!(e.tech_price(1, tech), tech_price(base, 1, lit));
}

/// A border that grows over an unowned village takes it as a city of the owner, as a capture does; plain land and
/// ruling the same area again add nothing.
#[test]
fn a_border_that_claims_a_village_counts_it() {
    let mut e = Engine::new_game(&GameOptions::default());
    let capital = e.capital_of(1).expect("capital");
    e.rule_area(capital, 1);
    assert_eq!(counter(&e, 1), 1, "plain land adds nothing");
    let near = e.area(capital, 1).into_iter().find(|c| *c != capital).expect("a tile next to the capital");
    let i = e.tile_index(near).unwrap();
    e.s.map.tiles[i].owner = 0;
    e.s.map.tiles[i].improvement = improvement::CITY;
    e.rule_area(capital, 1);
    assert_eq!((counter(&e, 1), e.s.map.tiles[i].owner), (2, 1));
    e.rule_area(capital, 1);
    assert_eq!(counter(&e, 1), 2, "ruling the area again changes nothing");
}

fn scratch_dir() -> String {
    std::env::var("POLYTOPIA_WEEKLY_SCRATCH").unwrap_or_default()
}

/// The weekly challenge of 2026-11-03 records Vengir with one city on a map of two, and Luxidoor loses its capital on
/// turn 6. At every checkpoint of the retail 2.18 run the engine's stars and counters equal retail's.
#[test]
fn weekly_rehearsal_stars_and_counters_match_retail_through_turn_7() {
    let dir = scratch_dir();
    let Ok(init) = std::fs::read(format!("{dir}/init.bin")) else {
        eprintln!("skipped: no rehearsal dumps in {dir}");
        return;
    };
    let mut names: Vec<String> = std::fs::read_dir(&dir)
        .unwrap()
        .flatten()
        .map(|f| f.file_name().to_string_lossy().into_owned())
        .filter(|n| n.len() > 3 && n[..2].chars().all(|c| c.is_ascii_digit()) && n.ends_with(".bin"))
        .collect();
    names.sort();
    assert!(!names.is_empty());
    for name in names {
        let cur = std::fs::read(format!("{dir}/{name}")).unwrap();
        let save = SaveFile::parse_server(&init, &cur).unwrap();
        let mut e = Engine::from_state(save.initial.clone());
        if save.commands.iter().any(|c| c.kind == 20) {
            e.start_match();
        }
        for c in save.commands.iter().filter(|c| c.kind != 20 && c.player != 255) {
            let cmd = e.command_from_raw(c).expect("a supported command");
            e.apply_unchecked(cmd);
        }
        for theirs in save.current.players.iter().filter(|p| p.id != polytopia_sim::NATURE) {
            let ours = e.player(theirs.id).unwrap();
            assert_eq!(ours.currency, theirs.currency, "{name}: stars of player {}", theirs.id);
            let recorded = save.current.player_runtime.iter().find(|r| r.player == theirs.id).unwrap().cities;
            assert_eq!(counter(&e, theirs.id), recorded, "{name}: city counter of player {}", theirs.id);
        }
    }
}
