//! The Polaris tribe end to end: its tech tree, its units and buildings, and the way its units move on ice.
//!
//! Polaris starts with Frostwork (which replaces Fishing), has Sledding, Ice Fishing, Polar Warfare and Polarism in
//! place of Aquaculture, Sailing, Navigation and Aquatism, trains an Ice Archer for the archer and a Gaami for the
//! giant, and builds an Ice Bank for the market. Its sled and fortress skate: they move only over ice. The Glide
//! ability (Polarism) makes ice cheaper to cross for every other unit.
use polytopia_sim::engine::{Command, Engine};
use polytopia_sim::json::Json;
use polytopia_sim::{CityReward, Engine as E2, GameOptions, MapPreset, MapSize, PlayerSetup, WorldCoordinates as Wc};

const POLARIS: i32 = 16;
const WARRIOR: i64 = 2;
const MOONI: i32 = 22;
const BATTLE_SLED: i32 = 23;
const ICE_FORTRESS: i32 = 24;
const ICE_ARCHER: i32 = 25;
const GAAMI: i32 = 27;
const ARCHER: i32 = 9;
const GIANT: i32 = 12;

const WATER: i32 = 1;
const FIELD: i32 = 3;
const ICE: i32 = 6;
const FISHING: i32 = 7;
const OUTPOST: i32 = 33;
const ICE_BANK: i32 = 34;
const ICE_TEMPLE: i32 = 35;
const MARKET: i32 = 50;
const STARFISHING: i32 = 46;
const FISH: i32 = 3;
const STARFISH: i32 = 8;

const TECH_BASIC: i32 = 0;
const TECH_TRADE: i32 = 5;
const TECH_ARCHERY: i32 = 18;
const TECH_FROSTWORK: i32 = 30;
const TECH_POLARWARFARE: i32 = 31;
const TECH_POLARISM: i32 = 32;
const TECH_SLEDDING: i32 = 41;
const TECH_ICEFISHING: i32 = 42;
const TECH_FISHING: i32 = 10;
const TECH_SAILING: i32 = 13;
const TECH_AQUACULTURE: i32 = 11;
const TECH_AQUATISM: i32 = 12;
const TECH_NAVIGATION: i32 = 14;

fn wc(x: i32, y: i32) -> Wc {
    Wc { x, y }
}

fn unit(id: u32, kind: i64, owner: u8, x: i32, y: i32) -> String {
    format!(r#"{{"id":{id},"type":{kind},"owner":{owner},"x":{x},"y":{y},"health":100}}"#)
}

/// A `w` x `h` board of field for Polaris (player 1, to move, capital at (0,0)) against player 2 (capital at the
/// top right). `tile` gives extra JSON for a tile as `(terrain, extra)`; `techs` are player 1's techs.
fn board(w: i32, h: i32, tile: impl Fn(i32, i32) -> (i32, String), techs: &[i32], stars: i32, units: &str) -> Engine {
    let mut tiles = Vec::new();
    let mut improvements =
        vec![r#"{"x":0,"y":0,"level":1}"#.to_string(), format!(r#"{{"x":{},"y":0,"level":1}}"#, w - 1)];
    for y in 0..h {
        for x in 0..w {
            let (terrain, extra) = tile(x, y);
            let climate = if terrain == ICE { POLARIS } else { 2 };
            let mut t = format!(r#""x":{x},"y":{y},"terrain":{terrain},"climate":{climate}{extra}"#);
            if (x, y) == (0, 0) {
                t.push_str(r#","city":true,"capital":true,"owner":1,"ruling":[0,0]"#);
            } else if (x, y) == (w - 1, 0) {
                t.push_str(&format!(r#","city":true,"capital":true,"owner":2,"ruling":[{},0]"#, w - 1));
            }
            if extra.contains(r#""city":true"#) && !t.contains("capital") {
                improvements.push(format!(r#"{{"x":{x},"y":{y},"level":1}}"#));
            }
            tiles.push(format!("{{{t}}}"));
        }
    }
    let tech: Vec<String> = techs.iter().map(i32::to_string).collect();
    let text = format!(
        r#"{{"format":"polytopia-position","game":{{"turn":6,"width":{w},"height":{h},"current_player":1}},
           "players":[{{"id":1,"tribe":{POLARIS},"human":true,"stars":{stars},"tech":[{}]}},{{"id":2,"tribe":3,"stars":5}}],
           "tiles":[{}],
           "improvements":[{}],
           "units":[{units}]}}"#,
        tech.join(","),
        tiles.join(","),
        improvements.join(",")
    );
    Engine::from_position(&Json::parse(&text).unwrap()).unwrap()
}

/// Land everywhere except `special`, which gives the terrain and extra JSON of chosen tiles. Land owned by player 1
/// is the 3x3 round its capital.
fn land(special: &'static [(i32, i32, i32, &'static str)]) -> impl Fn(i32, i32) -> (i32, String) {
    move |x, y| {
        let mut out = (FIELD, String::new());
        if x <= 1 && y <= 1 {
            out.1 = r#","owner":1,"ruling":[0,0]"#.to_string();
        }
        for &(sx, sy, terrain, extra) in special {
            if (sx, sy) == (x, y) {
                out =
                    (terrain, format!("{}{extra}", if x <= 1 && y <= 1 { r#","owner":1,"ruling":[0,0]"# } else { "" }));
            }
        }
        out
    }
}

fn can_train(e: &Engine, unit_type: i32) -> bool {
    e.legal_commands().contains(&Command::Train { city: wc(0, 0), unit_type })
}

fn can_build(e: &Engine, at: Wc, improvement: i32) -> bool {
    e.legal_commands().contains(&Command::Build { at, improvement })
}

fn stars(e: &Engine) -> i32 {
    e.player(1).unwrap().currency
}

fn new_polaris_game() -> E2 {
    let opts = GameOptions {
        seed: 11,
        size: MapSize::Small,
        preset: MapPreset::Continents,
        players: vec![PlayerSetup { tribe: POLARIS, human: true }, PlayerSetup { tribe: 3, human: false }],
        ..GameOptions::default()
    };
    E2::new_game(&opts)
}

// ---- the tech tree ----

#[test]
fn polaris_starts_with_frostwork_instead_of_fishing() {
    let e = new_polaris_game();
    let mut techs = e.techs(1).to_vec();
    techs.sort_unstable();
    assert_eq!(techs, vec![TECH_BASIC, TECH_FROSTWORK], "the start techs are Basic and Frostwork");
    let next = e.researchable(1);
    for tech in [TECH_SLEDDING, TECH_ICEFISHING] {
        assert!(next.contains(&tech), "{tech} follows Frostwork: {next:?}");
    }
    for tech in [TECH_FISHING, TECH_AQUACULTURE, TECH_SAILING, TECH_AQUATISM, TECH_NAVIGATION] {
        assert!(!next.contains(&tech), "{tech} is replaced for Polaris: {next:?}");
    }
}

#[test]
fn the_polaris_chain_is_sledding_to_polar_warfare_and_ice_fishing_to_polarism() {
    let mut e = board(5, 3, land(&[]), &[TECH_BASIC, TECH_FROSTWORK], 100, "");
    assert!(!e.researchable(1).contains(&TECH_POLARWARFARE), "Polar Warfare needs Sledding");
    assert!(!e.researchable(1).contains(&TECH_POLARISM), "Polarism needs Ice Fishing");
    let before = stars(&e);
    e.apply(Command::Research { tech: TECH_SLEDDING }).expect("Sledding is legal");
    // Sledding costs 2 and the price is the cost of the first tier plus 4 for the single city.
    assert_eq!(before - stars(&e), 2 + 4, "the price of Sledding with one city");
    assert!(e.researchable(1).contains(&TECH_POLARWARFARE), "Polar Warfare follows Sledding");
    assert!(!e.researchable(1).contains(&TECH_POLARISM));
    e.apply(Command::Research { tech: TECH_ICEFISHING }).expect("Ice Fishing is legal");
    assert!(e.researchable(1).contains(&TECH_POLARISM), "Polarism follows Ice Fishing");
    e.apply(Command::Research { tech: TECH_POLARWARFARE }).expect("Polar Warfare is legal");
    e.apply(Command::Research { tech: TECH_POLARISM }).expect("Polarism is legal");
    assert!(e.has_tech(1, TECH_POLARISM) && e.has_tech(1, TECH_POLARWARFARE));
}

#[test]
fn a_city_makes_the_next_polaris_tech_dearer() {
    let mut e = board(5, 3, land(&[]), &[TECH_BASIC, TECH_FROSTWORK], 100, "");
    let one_city = e.tech_price(1, TECH_POLARWARFARE);
    // Polar Warfare costs 3: 3 + 4 for one city.
    assert_eq!(one_city, 7);
    e.apply(Command::Research { tech: TECH_SLEDDING }).unwrap();
    assert_eq!(e.tech_price(1, TECH_POLARWARFARE), 7, "the price follows the city count, not the techs owned");
}

// ---- units ----

#[test]
fn each_polaris_unit_needs_its_tech() {
    let base = [TECH_BASIC, TECH_FROSTWORK];
    let e = board(5, 3, land(&[]), &base, 100, "");
    assert!(can_train(&e, MOONI), "Frostwork trains the Mooni");
    assert!(!can_train(&e, GAAMI) && !can_train(&e, GIANT), "the giant is hidden: no player trains it");
    assert!(!can_train(&e, BATTLE_SLED) && !can_train(&e, ICE_FORTRESS) && !can_train(&e, ICE_ARCHER));

    let e = board(5, 3, land(&[]), &[TECH_BASIC, TECH_FROSTWORK, TECH_SLEDDING], 100, "");
    assert!(can_train(&e, BATTLE_SLED), "Sledding trains the Battle Sled");
    assert!(!can_train(&e, ICE_FORTRESS));

    let e = board(5, 3, land(&[]), &[TECH_BASIC, TECH_FROSTWORK, TECH_SLEDDING, TECH_POLARWARFARE], 100, "");
    assert!(can_train(&e, ICE_FORTRESS), "Polar Warfare trains the Ice Fortress");

    let e = board(5, 3, land(&[]), &[TECH_BASIC, TECH_FROSTWORK, TECH_ARCHERY], 100, "");
    assert!(can_train(&e, ICE_ARCHER), "Archery trains the Ice Archer");
    assert!(!can_train(&e, ARCHER), "a Polaris player never trains the plain archer");
}

#[test]
fn polaris_units_cost_their_listed_price() {
    let techs = [TECH_BASIC, TECH_FROSTWORK, TECH_SLEDDING, TECH_POLARWARFARE, TECH_ARCHERY];
    for (unit_type, price) in [(MOONI, 5), (BATTLE_SLED, 5), (ICE_FORTRESS, 15), (ICE_ARCHER, 3)] {
        let mut e = board(5, 3, land(&[]), &techs, 100, "");
        e.apply(Command::Train { city: wc(0, 0), unit_type }).expect("the unit can be trained");
        assert_eq!(100 - stars(&e), price, "price of unit {unit_type}");
    }
}

// ---- buildings ----

#[test]
fn an_outpost_is_built_on_ice_only_and_needs_frostwork() {
    static SPECIAL: [(i32, i32, i32, &str); 1] = [(1, 1, ICE, "")];
    let e = board(5, 3, land(&SPECIAL), &[TECH_BASIC, TECH_FROSTWORK], 100, "");
    assert!(can_build(&e, wc(1, 1), OUTPOST), "an owned ice tile takes an outpost");
    assert!(!can_build(&e, wc(1, 0), OUTPOST), "a field tile does not");
    let e = board(5, 3, land(&SPECIAL), &[TECH_BASIC], 100, "");
    assert!(!can_build(&e, wc(1, 1), OUTPOST), "the outpost comes with Frostwork");
}

#[test]
fn an_outpost_costs_five_and_adds_a_population_to_its_city() {
    static SPECIAL: [(i32, i32, i32, &str); 1] = [(1, 1, ICE, "")];
    let mut e = board(5, 3, land(&SPECIAL), &[TECH_BASIC, TECH_FROSTWORK], 100, "");
    let pop = e.improvement_rt(wc(0, 0)).unwrap().population;
    e.apply(Command::Build { at: wc(1, 1), improvement: OUTPOST }).expect("legal");
    assert_eq!(100 - stars(&e), 5);
    assert_eq!(e.tile(wc(1, 1)).unwrap().improvement, OUTPOST);
    assert_eq!(e.improvement_rt(wc(0, 0)).unwrap().population, pop + 1, "an outpost is worth one population");
}

#[test]
fn outposts_on_ice_within_five_tiles_join_by_a_route_over_the_ice() {
    // A strip of ice along row 1 from x=1 to x=5; outposts at both ends.
    static SPECIAL: [(i32, i32, i32, &str); 5] =
        [(1, 1, ICE, ""), (2, 1, ICE, ""), (3, 1, ICE, ""), (4, 1, ICE, ""), (5, 1, ICE, "")];
    let units = "";
    let mut e = board(8, 3, land(&SPECIAL), &[TECH_BASIC, TECH_FROSTWORK], 100, units);
    // Make the strip owned: it lies outside the 3x3 of the capital, so own it for player 1.
    for x in 2..=5 {
        let t = e.tile_mut(wc(x, 1)).unwrap();
        t.owner = 1;
        t.ruling_city = wc(0, 0);
    }
    e.apply(Command::Build { at: wc(1, 1), improvement: OUTPOST }).expect("legal");
    e.apply(Command::Build { at: wc(5, 1), improvement: OUTPOST }).expect("legal");
    for x in 2..=4 {
        assert!(e.tile(wc(x, 1)).unwrap().has_route, "({x},1) lies on the route between the outposts");
    }
    assert!(!e.tile(wc(2, 2)).unwrap().has_route, "a field tile beside the ice is not on the route");
}

#[test]
fn an_outpost_does_not_route_across_land() {
    static SPECIAL: [(i32, i32, i32, &str); 2] = [(1, 1, ICE, ""), (4, 1, ICE, "")];
    let mut e = board(8, 3, land(&SPECIAL), &[TECH_BASIC, TECH_FROSTWORK], 100, "");
    for x in 2..=4 {
        let t = e.tile_mut(wc(x, 1)).unwrap();
        t.owner = 1;
        t.ruling_city = wc(0, 0);
    }
    e.apply(Command::Build { at: wc(1, 1), improvement: OUTPOST }).expect("legal");
    e.apply(Command::Build { at: wc(4, 1), improvement: OUTPOST }).expect("legal");
    assert!(
        !e.tile(wc(2, 1)).unwrap().has_route && !e.tile(wc(3, 1)).unwrap().has_route,
        "field between outposts carries no route"
    );
}

#[test]
fn an_ice_temple_needs_polarism_ice_and_costs_twenty() {
    static SPECIAL: [(i32, i32, i32, &str); 1] = [(1, 1, ICE, "")];
    let e = board(5, 3, land(&SPECIAL), &[TECH_BASIC, TECH_FROSTWORK], 100, "");
    assert!(!can_build(&e, wc(1, 1), ICE_TEMPLE), "no temple before Polarism");
    let mut e = board(5, 3, land(&SPECIAL), &[TECH_BASIC, TECH_FROSTWORK, TECH_ICEFISHING, TECH_POLARISM], 100, "");
    assert!(can_build(&e, wc(1, 1), ICE_TEMPLE), "Polarism allows a temple on ice");
    assert!(!can_build(&e, wc(1, 0), ICE_TEMPLE), "but not on field");
    let (score, pop) = (e.player(1).unwrap().score, e.improvement_rt(wc(0, 0)).unwrap().population);
    e.apply(Command::Build { at: wc(1, 1), improvement: ICE_TEMPLE }).expect("legal");
    assert_eq!(100 - stars(&e), 20);
    assert_eq!(
        e.player(1).unwrap().score - score,
        100 + 5,
        "a temple scores 100 on completion, and 5 for its population"
    );
    assert_eq!(e.improvement_rt(wc(0, 0)).unwrap().population, pop + 1);
}

#[test]
fn an_ice_bank_replaces_the_market_and_is_built_on_field_or_ice() {
    static SPECIAL: [(i32, i32, i32, &str); 1] = [(1, 1, ICE, "")];
    let mut e = board(5, 3, land(&SPECIAL), &[TECH_BASIC, TECH_FROSTWORK, TECH_TRADE], 100, "");
    assert!(can_build(&e, wc(1, 0), ICE_BANK), "Trade unlocks the Ice Bank on field");
    assert!(can_build(&e, wc(1, 1), ICE_BANK), "and on ice");
    assert!(!can_build(&e, wc(1, 0), MARKET) && !can_build(&e, wc(1, 1), MARKET), "the plain market is replaced");
    e.apply(Command::Build { at: wc(1, 0), improvement: ICE_BANK }).expect("legal");
    assert_eq!(100 - stars(&e), 20);
    assert!(!can_build(&e, wc(1, 1), ICE_BANK), "an Ice Bank is unique");
}

#[test]
fn ice_fishing_unlocks_the_fishing_improvement_and_fish_pay_a_population() {
    static SPECIAL: [(i32, i32, i32, &str); 1] = [(1, 1, WATER, r#","resource":3"#)];
    let e = board(5, 3, land(&SPECIAL), &[TECH_BASIC, TECH_FROSTWORK], 100, "");
    assert!(!can_build(&e, wc(1, 1), FISHING), "no fishing before Ice Fishing");
    let mut e = board(5, 3, land(&SPECIAL), &[TECH_BASIC, TECH_FROSTWORK, TECH_ICEFISHING], 100, "");
    assert!(can_build(&e, wc(1, 1), FISHING), "Ice Fishing allows fishing on a fish tile");
    let pop = e.improvement_rt(wc(0, 0)).unwrap().population;
    e.apply(Command::Build { at: wc(1, 1), improvement: FISHING }).expect("legal");
    assert_eq!(100 - stars(&e), 2, "fishing costs 2");
    assert_eq!(e.improvement_rt(wc(0, 0)).unwrap().population, pop + 1);
    let _ = FISH;
}

#[test]
fn starfishing_comes_with_polar_warfare_and_pays_eight_stars() {
    static SPECIAL: [(i32, i32, i32, &str); 1] = [(1, 1, WATER, r#","resource":8"#)];
    // The improvement is manual: it needs one of the player's units on the tile (a Polaris sled cannot swim, so this
    // unit stands in for a ship).
    let unit_on_tile = unit(1, WARRIOR, 1, 1, 1);
    let e = board(5, 3, land(&SPECIAL), &[TECH_BASIC, TECH_FROSTWORK, TECH_SLEDDING], 100, &unit_on_tile);
    assert!(!can_build(&e, wc(1, 1), STARFISHING), "no starfishing before Polar Warfare");
    let mut e = board(
        5,
        3,
        land(&SPECIAL),
        &[TECH_BASIC, TECH_FROSTWORK, TECH_SLEDDING, TECH_POLARWARFARE],
        100,
        &unit_on_tile,
    );
    assert!(can_build(&e, wc(1, 1), STARFISHING));
    e.apply(Command::Build { at: wc(1, 1), improvement: STARFISHING }).expect("legal");
    assert_eq!(stars(&e) - 100, 8, "a starfish pays 8 stars and costs nothing");
    assert_eq!(e.tile(wc(1, 1)).unwrap().resource, 0, "the starfish is consumed");
    let _ = STARFISH;
}

// ---- movement on ice ----

/// Row 1 of a 9x3 board is ice from x=1 to x=7; the rest is field.
fn ice_row(techs: &[i32], units: &str) -> Engine {
    static SPECIAL: [(i32, i32, i32, &str); 7] = [
        (1, 1, ICE, ""),
        (2, 1, ICE, ""),
        (3, 1, ICE, ""),
        (4, 1, ICE, ""),
        (5, 1, ICE, ""),
        (6, 1, ICE, ""),
        (7, 1, ICE, ""),
    ];
    board(9, 3, land(&SPECIAL), techs, 10, units)
}

fn reaches(e: &Engine, unit: u32, to: Wc) -> bool {
    e.legal_commands().contains(&Command::Move { unit, to })
}

#[test]
fn a_battle_sled_crosses_three_ice_tiles_and_can_attack_after() {
    let mut e = ice_row(&[TECH_BASIC, TECH_FROSTWORK, TECH_SLEDDING], &unit(1, BATTLE_SLED as i64, 1, 1, 1));
    assert!(reaches(&e, 1, wc(4, 1)), "three ice tiles at movement 3");
    assert!(!reaches(&e, 1, wc(5, 1)), "a fourth ice tile is out of reach");
    e.apply(Command::Move { unit: 1, to: wc(4, 1) }).expect("legal");
    let s = e.unit(1).unwrap();
    assert!(s.moved && !s.attacked, "a move that ends on ice leaves the attack: {s:?}");
}

#[test]
fn a_sled_that_steps_off_the_ice_has_spent_its_turn() {
    // From (3,1) on the ice the field tile (3,0) is one step away: a skater may take it as its last step.
    let mut e = ice_row(&[TECH_BASIC, TECH_FROSTWORK, TECH_SLEDDING], &unit(1, BATTLE_SLED as i64, 1, 3, 1));
    assert!(reaches(&e, 1, wc(3, 0)), "the step onto field is offered");
    e.apply(Command::Move { unit: 1, to: wc(3, 0) }).expect("legal");
    let s = e.unit(1).unwrap();
    assert!(s.moved && s.attacked, "stepping off the ice also uses the attack: {s:?}");
}

#[test]
fn a_sled_on_plain_land_moves_one_tile() {
    let e =
        board(9, 3, land(&[]), &[TECH_BASIC, TECH_FROSTWORK, TECH_SLEDDING], 10, &unit(1, BATTLE_SLED as i64, 1, 3, 1));
    assert!(reaches(&e, 1, wc(4, 1)), "one step of land");
    assert!(!reaches(&e, 1, wc(5, 1)), "a second step is more than the whole move");
}

#[test]
fn an_ice_fortress_on_ice_can_still_move_after_it_attacks() {
    let units = [unit(1, ICE_FORTRESS as i64, 1, 3, 1), unit(2, WARRIOR, 2, 5, 1)].join(",");
    let mut e = ice_row(&[TECH_BASIC, TECH_FROSTWORK, TECH_SLEDDING, TECH_POLARWARFARE], &units);
    e.apply(Command::Attack { unit: 1, target: wc(5, 1) }).expect("range 2");
    let f = e.unit(1).unwrap();
    assert!(f.attacked && !f.moved, "the escape ability on ice leaves the move: {f:?}");
    assert!(reaches(&e, 1, wc(2, 1)), "and the fortress can retreat");
}

#[test]
fn an_ice_fortress_on_field_cannot_move_after_it_attacks() {
    let units = [unit(1, ICE_FORTRESS as i64, 1, 3, 0), unit(2, WARRIOR, 2, 5, 0)].join(",");
    let mut e = ice_row(&[TECH_BASIC, TECH_FROSTWORK, TECH_SLEDDING, TECH_POLARWARFARE], &units);
    e.apply(Command::Attack { unit: 1, target: wc(5, 0) }).expect("range 2");
    let f = e.unit(1).unwrap();
    assert!(f.attacked && f.moved, "off ice the skater does not keep its move: {f:?}");
}

#[test]
fn glide_makes_ice_cheaper_for_a_walking_unit() {
    let units = unit(1, WARRIOR, 1, 0, 1);
    let plain = ice_row(&[TECH_BASIC, TECH_FROSTWORK], &units);
    let glide = ice_row(&[TECH_BASIC, TECH_FROSTWORK, TECH_ICEFISHING, TECH_POLARISM], &units);
    // A warrior has one move. Over ice it takes one tile without Glide and, since a step on ice costs less than a full
    // move, a second step as well with it.
    assert!(reaches(&plain, 1, wc(1, 1)) && !reaches(&plain, 1, wc(2, 1)), "one ice tile without Glide");
    assert!(reaches(&glide, 1, wc(1, 1)) && reaches(&glide, 1, wc(2, 1)), "two ice tiles with Glide");
}

#[test]
fn a_super_unit_reward_gives_polaris_a_gaami() {
    let mut e = board(5, 3, land(&[]), &[TECH_BASIC, TECH_FROSTWORK], 10, "");
    e.apply_unchecked(Command::CityReward { city: wc(0, 0), reward: CityReward::SuperUnit });
    let at_city = e.unit_at(wc(0, 0)).expect("a unit was placed in the city");
    assert_eq!(at_city.type_idx, GAAMI, "the giant of the reward is the Gaami for Polaris");
}

#[test]
fn an_ice_temple_grows_in_level_and_score_as_it_ages() {
    static SPECIAL: [(i32, i32, i32, &str); 1] = [(1, 1, ICE, "")];
    let mut e = board(5, 3, land(&SPECIAL), &[TECH_BASIC, TECH_FROSTWORK, TECH_ICEFISHING, TECH_POLARISM], 100, "");
    e.apply(Command::Build { at: wc(1, 1), improvement: ICE_TEMPLE }).expect("legal");
    assert_eq!(e.improvement_rt(wc(1, 1)).unwrap().level, 0);
    let score = e.player(1).unwrap().score;
    for _ in 0..4 {
        e.apply(Command::EndTurn).unwrap();
    }
    let level = e.improvement_rt(wc(1, 1)).unwrap().level;
    assert_eq!(level, 1, "one level after two turns (growth rate 2)");
    assert!(e.player(1).unwrap().score > score, "the level is scored through the buildings");
}

#[test]
fn an_ice_bank_earns_its_work_for_every_level_and_the_level_follows_the_frozen_tiles() {
    // Rows 2..=6 of a 9x7 board are ice: 45 frozen tiles, two levels of twenty.
    fn icy(x: i32, y: i32) -> (i32, String) {
        let _ = x;
        (
            if y >= 2 { ICE } else { FIELD },
            if x <= 1 && y <= 1 { r#","owner":1,"ruling":[0,0]"#.to_string() } else { String::new() },
        )
    }
    let mut e = board(9, 7, icy, &[TECH_BASIC, TECH_FROSTWORK, TECH_TRADE], 100, "");
    e.apply(Command::Build { at: wc(1, 0), improvement: ICE_BANK }).expect("legal");
    assert_eq!(e.improvement_rt(wc(1, 0)).unwrap().level, 2, "45 frozen tiles are two levels");
    assert_eq!(e.tile_work(wc(1, 0)), 2 * 2, "an Ice Bank earns 2 stars for each level");
    // The same bank on a board with fewer than twenty frozen tiles is level 0 and earns nothing.
    let mut e =
        board(9, 5, |x, y| icy(x, if y >= 4 { 2 } else { 0 }), &[TECH_BASIC, TECH_FROSTWORK, TECH_TRADE], 100, "");
    e.apply(Command::Build { at: wc(1, 0), improvement: ICE_BANK }).expect("legal");
    assert_eq!(e.improvement_rt(wc(1, 0)).unwrap().level, 0);
    assert_eq!(e.tile_work(wc(1, 0)), 0);
}

#[test]
fn glide_does_not_help_off_the_ice() {
    let e = board(
        9,
        3,
        land(&[]),
        &[TECH_BASIC, TECH_FROSTWORK, TECH_ICEFISHING, TECH_POLARISM],
        10,
        &unit(1, WARRIOR, 1, 3, 1),
    );
    assert!(reaches(&e, 1, wc(4, 1)) && !reaches(&e, 1, wc(5, 1)), "one tile of field, Glide or not");
}

#[test]
fn a_mooni_has_no_attack() {
    let units = [unit(1, MOONI as i64, 1, 3, 1), unit(2, WARRIOR, 2, 4, 1)].join(",");
    let e = ice_row(&[TECH_BASIC, TECH_FROSTWORK], &units);
    assert!(
        !e.legal_commands().iter().any(|c| matches!(c, Command::Attack { unit: 1, .. })),
        "a Mooni has attack 0 and only freezes by moving"
    );
}

#[test]
fn a_battle_sled_dashes_and_can_attack_after_moving_on_ice() {
    let units = [unit(1, BATTLE_SLED as i64, 1, 1, 1), unit(2, WARRIOR, 2, 4, 1)].join(",");
    let mut e = ice_row(&[TECH_BASIC, TECH_FROSTWORK, TECH_SLEDDING], &units);
    e.apply(Command::Move { unit: 1, to: wc(3, 1) }).expect("legal");
    assert!(e.legal_commands().contains(&Command::Attack { unit: 1, target: wc(4, 1) }), "Dash: attack after the move");
    e.apply(Command::Attack { unit: 1, target: wc(4, 1) }).expect("legal");
    let s = e.unit(1).unwrap();
    assert!(s.attacked && !s.moved, "the sled attacked from the ice and keeps Escape's move: {s:?}");
}

#[test]
fn outposts_connect_a_far_city_to_the_capital_over_ice() {
    // The capital (0,0) and a second city (7,1), joined by an ice strip with an outpost at each end.
    static SPECIAL: [(i32, i32, i32, &str); 6] =
        [(1, 1, ICE, ""), (2, 1, ICE, ""), (3, 1, ICE, ""), (4, 1, ICE, ""), (5, 1, ICE, ""), (6, 1, ICE, "")];
    let f = land(&SPECIAL);
    let mut e = board(
        9,
        3,
        |x, y| {
            let (t, extra) = f(x, y);
            if (x, y) == (7, 1) {
                (t, r#","city":true,"owner":1,"ruling":[7,1]"#.to_string())
            } else if (6..=7).contains(&x) && y <= 2 {
                (t, r#","owner":1,"ruling":[7,1]"#.to_string())
            } else if (2..=5).contains(&x) && y == 1 {
                (t, r#","owner":1,"ruling":[0,0]"#.to_string())
            } else {
                (t, extra)
            }
        },
        &[TECH_BASIC, TECH_FROSTWORK],
        100,
        "",
    );
    let connected = |e: &Engine| e.improvement_rt(wc(7, 1)).unwrap().connected_to_capital_of_player;
    assert_eq!(connected(&e), 0, "no connection before the outposts");
    e.apply(Command::Build { at: wc(1, 1), improvement: OUTPOST }).expect("legal");
    e.apply(Command::Build { at: wc(6, 1), improvement: OUTPOST }).expect("legal");
    assert_eq!(connected(&e), 1, "the far city is connected to the capital of player 1");
}
