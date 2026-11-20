//! The Elyrion tribe end to end: Enchant Animal, the Sanctuary, the dragons and the effects they have on a game.
//!
//! The positions are built by hand on a small board of field (`board`), with player 1 Elyrion and player 2 Aquarion
//! far enough away to stay out of the way.
use polytopia_sim::engine::{Command, Engine};
use polytopia_sim::json::Json;
use polytopia_sim::WorldCoordinates as Wc;
use polytopia_sim::{CityReward, Engine as NewGame, GameOptions, MapPreset, MapSize, PlayerSetup};

const ELYRION: i32 = 5;
const FIELD: i32 = 3;
const FOREST: i32 = 5;
const GAME: i32 = 1;
const FRUIT: i32 = 6;
const BABY_DRAGON: i32 = 18;
const FIRE_DRAGON: i32 = 19;
const DRAGON_EGG: i32 = 17;
const SANCTUARY: i32 = 32;
const POLYTAUR: i32 = 15;
const WARRIOR: i32 = 2;
const CORNER: (i32, i32) = (6, 6);

fn wc(x: i32, y: i32) -> Wc {
    Wc { x, y }
}

/// A 7x7 board. Player 1 (Elyrion, to move on turn 6) has its capital on (0,0) and owns the tiles (0..=1, 0..=1).
/// `tile` gives a tile's terrain and extra JSON; the corner (6,6) is explored by player 2 only, so a unit of player 1
/// that sees it discovers a map corner. `xp` is the capital's xp.
fn board(tile: impl Fn(i32, i32) -> (i32, String), units: &str, xp: i32) -> Engine {
    board_with(tile, units, xp, None, 20)
}

/// [`board`] with player 1's techs (`None` leaves the position's default) and stars.
fn board_with(
    tile: impl Fn(i32, i32) -> (i32, String),
    units: &str,
    xp: i32,
    techs: Option<&[i32]>,
    stars: i32,
) -> Engine {
    let tech = techs
        .map(|t| format!(r#","tech":[{}]"#, t.iter().map(i32::to_string).collect::<Vec<_>>().join(",")))
        .unwrap_or_default();
    let mut tiles = Vec::new();
    for y in 0..7 {
        for x in 0..7 {
            let (terrain, extra) = tile(x, y);
            let explored = if (x, y) == CORNER { "[2]" } else { "[1,2]" };
            let mut t = format!(r#""x":{x},"y":{y},"terrain":{terrain},"climate":2,"explored":{explored}{extra}"#);
            if (x, y) == (0, 0) {
                t.push_str(r#","city":true,"capital":true,"owner":1,"ruling":[0,0]"#);
            } else if x <= 1 && y <= 1 {
                t.push_str(r#","owner":1,"ruling":[0,0]"#);
            } else if (x, y) == (6, 0) {
                t.push_str(r#","city":true,"capital":true,"owner":2,"ruling":[6,0]"#);
            }
            tiles.push(format!("{{{t}}}"));
        }
    }
    let text = format!(
        r#"{{"format":"polytopia-position","game":{{"turn":6,"width":7,"height":7,"current_player":1}},
           "players":[{{"id":1,"tribe":{ELYRION},"human":true,"stars":{stars},"known":[2]{tech}}},{{"id":2,"tribe":3,"stars":5,"known":[1]}}],
           "tiles":[{}],
           "improvements":[{{"x":0,"y":0,"level":1,"xp":{xp}}},{{"x":6,"y":0,"level":1}}],
           "units":[{units}]}}"#,
        tiles.join(",")
    );
    Engine::from_position(&Json::parse(&text).unwrap()).unwrap()
}

fn field(_: i32, _: i32) -> (i32, String) {
    (FIELD, String::new())
}

fn resource_at(e: &Engine, at: Wc) -> i32 {
    e.tile(at).unwrap().resource
}

fn games(e: &Engine) -> usize {
    e.s.map.tiles.iter().filter(|t| t.resource == GAME).count()
}

/// Both players end their turn, `rounds` times.
fn pass_rounds(e: &mut Engine, rounds: usize) {
    for _ in 0..rounds * 2 {
        e.apply_unchecked(Command::EndTurn);
    }
}

// ---- the Sanctuary ----

/// Player 1 owns (0..=1, 0..=1) and builds a Sanctuary on the field (1,1) on turn 6. `forests` are forest tiles with
/// extra JSON; the Sanctuary's growth rate is 2, so it is ready on turn 8.
fn sanctuary_board(forests: &'static [(i32, i32, &'static str)]) -> Engine {
    let tile = move |x: i32, y: i32| {
        for &(fx, fy, extra) in forests {
            if (fx, fy) == (x, y) {
                return (FOREST, extra.to_string());
            }
        }
        (FIELD, String::new())
    };
    let mut e = board(tile, "", 0);
    e.apply_unchecked(Command::Build { at: wc(1, 1), improvement: SANCTUARY });
    e
}

#[test]
fn a_sanctuary_fills_an_owned_empty_forest_with_game_every_second_turn() {
    // (2,1) is outside player 1's border.
    let mut e = sanctuary_board(&[(1, 0, ""), (2, 1, "")]);
    assert_eq!(games(&e), 0);
    pass_rounds(&mut e, 1);
    assert_eq!(games(&e), 0, "one turn old: its growth rate is 2");
    pass_rounds(&mut e, 1);
    assert_eq!(resource_at(&e, wc(1, 0)), GAME, "two turns old: the owned forest gets game");
    assert_eq!(resource_at(&e, wc(2, 1)), 0, "a forest the player does not own is left alone");
}

#[test]
fn a_sanctuary_skips_forests_that_hold_a_resource_or_an_improvement() {
    let mut e = sanctuary_board(&[(1, 0, r#","resource":6"#), (0, 1, ""), (0, 0, "")]);
    pass_rounds(&mut e, 2);
    assert_eq!(resource_at(&e, wc(1, 0)), FRUIT, "a forest with a resource keeps it");
    assert_eq!(resource_at(&e, wc(0, 1)), GAME, "the empty owned forest is the target");
    assert_eq!(games(&e), 1);
    pass_rounds(&mut e, 2);
    assert_eq!(games(&e), 1, "no empty forest is left, and the city's own tile is never the target");
}

#[test]
fn a_sanctuary_with_two_empty_forests_fills_only_the_first_one_of_its_area_each_time() {
    let mut e = sanctuary_board(&[(1, 0, ""), (0, 1, "")]);
    let first = e.area_iter(wc(1, 1), 1).find(|c| [wc(1, 0), wc(0, 1)].contains(c)).unwrap();
    pass_rounds(&mut e, 2);
    assert_eq!(games(&e), 1, "one game per attempt");
    assert_eq!(resource_at(&e, first), GAME, "the first tile of the area");
    pass_rounds(&mut e, 2);
    assert_eq!(games(&e), 2, "the next attempt takes the other forest");
}

// ---- dragons ----

/// A baby dragon of player 1 on (4,5), created on turn 3, which sees the corner (6,6) only once it is a fire dragon.
fn dragon_board(xp: i32) -> Engine {
    let units = format!(r#"{{"id":1,"type":{BABY_DRAGON},"owner":1,"x":4,"y":5,"created":3}}"#);
    board(field, &units, xp)
}

#[test]
fn a_dragon_grows_up_through_an_upgrade_that_uses_a_unit_id() {
    let mut e = dragon_board(0);
    let unit_id = e.s.current_unit_id;
    e.apply_unchecked(Command::EndTurn);
    assert_eq!(e.unit(1).unwrap().type_idx, FIRE_DRAGON, "the baby is three turns old: it grows into a fire dragon");
    assert_eq!(e.s.current_unit_id, unit_id + 1, "the growth is an upgrade action and takes an id like any other");
}

#[test]
fn a_dragon_too_young_to_grow_stays_as_it_is_and_takes_no_id() {
    let units = format!(
        r#"{{"id":1,"type":{BABY_DRAGON},"owner":1,"x":4,"y":5,"created":5}},{{"id":2,"type":{DRAGON_EGG},"owner":1,"x":3,"y":5,"created":5}}"#
    );
    let mut e = board(field, &units, 0);
    let unit_id = e.s.current_unit_id;
    e.apply_unchecked(Command::EndTurn);
    assert_eq!(e.unit(1).unwrap().type_idx, BABY_DRAGON);
    assert_eq!(e.unit(2).unwrap().type_idx, DRAGON_EGG);
    assert_eq!(e.s.current_unit_id, unit_id);
}

#[test]
fn a_growth_that_levels_a_city_up_keeps_the_turn_with_its_owner_until_the_reward_is_answered() {
    let mut e = dragon_board(1);
    e.apply_unchecked(Command::EndTurn);
    assert_eq!(e.unit(1).unwrap().type_idx, FIRE_DRAGON);
    assert!(e.reward_pending(), "the corner the fire dragon sees adds a population and levels the capital up");
    assert_eq!(e.current_player(), 1, "the turn waits for the answer");
    assert_eq!(e.s.current_turn, 6);
    let legal = e.legal_commands();
    assert!(
        legal.iter().all(|c| matches!(c, Command::CityReward { .. })),
        "only the reward can be answered: {legal:?}"
    );
    e.apply(legal[0]).unwrap();
    assert!(!e.reward_pending());
    assert_eq!(e.current_player(), 2, "the answer hands the turn on");
}

#[test]
fn a_held_end_of_turn_survives_a_position_round_trip() {
    let mut e = dragon_board(1);
    e.apply_unchecked(Command::EndTurn);
    let mut copy = Engine::from_position(&Json::parse(&e.to_position().to_string()).unwrap()).unwrap();
    assert_eq!(copy.current_player(), 1);
    let reward = copy.legal_commands()[0];
    copy.apply(reward).unwrap();
    assert_eq!(copy.current_player(), 2, "the copy passes the turn too");
}

#[test]
fn a_growth_that_raises_no_reward_passes_the_turn_at_once() {
    let mut e = dragon_board(0);
    e.apply_unchecked(Command::EndTurn);
    assert!(!e.reward_pending());
    assert_eq!(e.current_player(), 2);
}

// ---- the step onto a defeated enemy's tile ----

const CLATHRUS: i32 = 41;

/// A polytaur of player 1 on (3,3) beside a weak warrior of player 2 on (4,3). Player 2 owns that tile, which holds
/// `improvement` (0 for none).
fn duel_board(improvement: i32) -> Engine {
    let tile = move |x: i32, y: i32| {
        if (x, y) == (4, 3) {
            let imp = if improvement == 0 { String::new() } else { format!(r#","improvement":{improvement}"#) };
            return (FIELD, format!(r#"{imp},"owner":2,"ruling":[6,0]"#));
        }
        (FIELD, String::new())
    };
    let units = format!(
        r#"{{"id":1,"type":{POLYTAUR},"owner":1,"x":3,"y":3}},{{"id":2,"type":{WARRIOR},"owner":2,"x":4,"y":3,"health":10}}"#
    );
    board(tile, &units, 0)
}

#[test]
fn a_unit_that_steps_onto_a_poisonous_improvement_after_a_kill_is_poisoned() {
    let mut e = duel_board(CLATHRUS);
    e.apply(Command::Attack { unit: 1, target: wc(4, 3) }).expect("legal");
    assert!(e.unit(2).is_none(), "the warrior is dead");
    let stepper = e.unit(1).unwrap();
    assert_eq!(stepper.coordinates, wc(4, 3), "the polytaur steps onto the tile");
    assert!(stepper.poisoned, "and the poison on it gets the polytaur");
}

#[test]
fn a_unit_that_steps_onto_a_plain_tile_after_a_kill_is_not_poisoned() {
    let mut e = duel_board(0);
    e.apply(Command::Attack { unit: 1, target: wc(4, 3) }).expect("legal");
    let stepper = e.unit(1).unwrap();
    assert_eq!(stepper.coordinates, wc(4, 3));
    assert!(!stepper.poisoned);
}

#[test]
fn the_retail_ai_answers_a_reward_held_at_the_end_of_a_turn() {
    let mut e = dragon_board(1);
    e.apply_unchecked(Command::EndTurn);
    assert_eq!(e.current_player(), 1);
    let cmd = e.ai_get_move();
    assert!(matches!(cmd, Command::CityReward { .. }), "{cmd:?}");
    e.apply_unchecked(cmd);
    assert_eq!(e.current_player(), 2);
}

// ---- the tech tree ----

const TECH_BASIC: i32 = 0;
const TECH_HUNTING: i32 = 15;
const TECH_FORESTRY: i32 = 16;
const TECH_ARCHERY: i32 = 18;
const TECH_AQUATISM: i32 = 12;
const TECH_FOREST_MAGIC: i32 = 28;
const TECH_WATER_MAGIC: i32 = 29;
const HUNTING: i32 = 9;
const CLEAR_FOREST: i32 = 10;
const BURN_FOREST: i32 = 11;
const LUMBER_HUT: i32 = 12;
const HIDDEN_SANCTUARY: i32 = 42;
const ENCHANT_ANIMAL: i32 = 30;
const ENCHANT_WHALE: i32 = 31;
const ARCHER: i32 = 9;
const GIANT: i32 = 12;

fn new_elyrion_game(version: i32) -> NewGame {
    let opts = GameOptions {
        seed: 11,
        size: MapSize::Small,
        preset: MapPreset::Continents,
        version,
        players: vec![PlayerSetup { tribe: ELYRION, human: true }, PlayerSetup { tribe: 3, human: false }],
        ..GameOptions::default()
    };
    NewGame::new_game(&opts)
}

fn can_train(e: &Engine, unit_type: i32) -> bool {
    e.legal_commands().contains(&Command::Train { city: wc(0, 0), unit_type })
}

fn can_build(e: &Engine, at: Wc, improvement: i32) -> bool {
    e.legal_commands().contains(&Command::Build { at, improvement })
}

#[test]
fn elyrion_starts_with_forest_magic_instead_of_hunting() {
    for version in 121..=124 {
        let e = new_elyrion_game(version);
        let mut techs = e.techs(1).to_vec();
        techs.sort_unstable();
        assert_eq!(
            techs,
            vec![TECH_BASIC, TECH_FOREST_MAGIC],
            "version {version}: the start techs are Basic and Forest Magic"
        );
        let next = e.researchable(1);
        for tech in [TECH_ARCHERY, TECH_FORESTRY] {
            assert!(next.contains(&tech), "version {version}: {tech} follows Forest Magic: {next:?}");
        }
        assert!(!next.contains(&TECH_HUNTING), "version {version}: Hunting is replaced: {next:?}");
    }
}

#[test]
fn water_magic_is_in_no_tech_chain_so_it_can_never_be_researched() {
    for version in 121..=124 {
        let mut e = new_elyrion_game(version);
        let mut ever = Vec::new();
        for _ in 0..60 {
            let next = e.researchable(1);
            let Some(&first) = next.first() else { break };
            ever.extend(next.iter().copied());
            e.s.tech.iter_mut().find(|t| t.player == 1).unwrap().tech.push(first);
        }
        assert!(!ever.contains(&TECH_WATER_MAGIC), "version {version}: nothing unlocks Water Magic");
        assert!(ever.contains(&TECH_AQUATISM), "version {version}: the rest of the tree is open");
    }
}

#[test]
fn water_magic_unlocks_aquatism_and_no_improvement_because_enchant_whale_is_not_in_the_data() {
    let e = board_with(field, "", 0, Some(&[TECH_BASIC, TECH_FOREST_MAGIC, TECH_WATER_MAGIC]), 20);
    let unlocked = e.unlocked_improvements(1);
    assert!(!unlocked.contains(&ENCHANT_WHALE), "{unlocked:?}");
}

#[test]
fn forest_magic_unlocks_enchant_animal_and_forestry_the_sanctuary_and_lumber_hut() {
    let e = board_with(field, "", 0, Some(&[TECH_BASIC, TECH_FOREST_MAGIC]), 20);
    let unlocked = e.unlocked_improvements(1);
    assert!(unlocked.contains(&ENCHANT_ANIMAL), "{unlocked:?}");
    assert!(!unlocked.contains(&HUNTING), "Hunting is not an Elyrion improvement: {unlocked:?}");
    assert!(!unlocked.contains(&SANCTUARY) && !unlocked.contains(&LUMBER_HUT), "Forestry comes later");

    let e = board_with(field, "", 0, Some(&[TECH_BASIC, TECH_FOREST_MAGIC, TECH_FORESTRY]), 20);
    let unlocked = e.unlocked_improvements(1);
    assert!(unlocked.contains(&SANCTUARY) && unlocked.contains(&LUMBER_HUT), "{unlocked:?}");
    assert!(!unlocked.contains(&CLEAR_FOREST), "the Sanctuary replaces Clear Forest: {unlocked:?}");
    assert!(!unlocked.contains(&BURN_FOREST), "Elyrion cannot burn a forest: {unlocked:?}");
    assert!(
        !unlocked.contains(&HIDDEN_SANCTUARY) || !can_build(&e, wc(1, 1), HIDDEN_SANCTUARY),
        "the hidden sanctuary is never built"
    );
}

// ---- units ----

#[test]
fn elyrion_trains_a_warrior_and_an_archer_and_none_of_its_hidden_units() {
    let e = board_with(field, "", 0, Some(&[TECH_BASIC, TECH_FOREST_MAGIC]), 50);
    assert!(can_train(&e, WARRIOR));
    assert!(!can_train(&e, ARCHER), "the archer needs Archery");
    let e = board_with(field, "", 0, Some(&[TECH_BASIC, TECH_FOREST_MAGIC, TECH_ARCHERY]), 50);
    assert!(can_train(&e, ARCHER), "Archery trains the archer");
    for hidden in [POLYTAUR, DRAGON_EGG, BABY_DRAGON, FIRE_DRAGON, GIANT] {
        assert!(!can_train(&e, hidden), "unit {hidden} is hidden: no city trains it");
    }
}

#[test]
fn a_super_unit_reward_gives_elyrion_a_dragon_egg_instead_of_a_giant() {
    let mut e = board_with(field, "", 0, Some(&[TECH_BASIC, TECH_FOREST_MAGIC]), 20);
    e.apply_unchecked(Command::CityReward { city: wc(0, 0), reward: CityReward::SuperUnit });
    assert_eq!(e.unit_at(wc(0, 0)).expect("a unit was placed in the city").type_idx, DRAGON_EGG);
}

#[test]
fn a_dragon_egg_moves_one_tile_but_never_attacks_and_never_hits_back() {
    let units = format!(
        r#"{{"id":1,"type":{DRAGON_EGG},"owner":1,"x":3,"y":3,"created":5}},{{"id":2,"type":{WARRIOR},"owner":2,"x":4,"y":3}},
           {{"id":3,"type":{WARRIOR},"owner":1,"x":0,"y":3}},{{"id":4,"type":{DRAGON_EGG},"owner":2,"x":1,"y":3,"created":5}}"#
    );
    let mut e = board(field, &units, 0);
    let legal = e.legal_commands();
    assert!(legal.contains(&Command::Move { unit: 1, to: wc(3, 4) }), "movement 1");
    assert!(!legal.contains(&Command::Move { unit: 1, to: wc(3, 5) }), "no further than one tile");
    assert!(!legal.iter().any(|c| matches!(c, Command::Attack { unit: 1, .. })), "its attack is 0: it never attacks");
    let warrior_health = e.unit(3).unwrap().health;
    e.apply(Command::Attack { unit: 3, target: wc(1, 3) }).expect("legal");
    assert!(e.unit(4).unwrap().health < 100, "the egg takes the hit");
    assert_eq!(e.unit(3).unwrap().health, warrior_health, "and, being stiff, does not hit back");
}

#[test]
fn a_baby_dragon_flies_over_water_and_a_fire_dragon_strikes_from_two_tiles_and_splashes() {
    const WATER: i32 = 1;
    let tile = |x: i32, y: i32| if (x, y) == (4, 3) { (WATER, String::new()) } else { (FIELD, String::new()) };
    let units = format!(
        r#"{{"id":1,"type":{BABY_DRAGON},"owner":1,"x":3,"y":3,"created":5}},{{"id":2,"type":{FIRE_DRAGON},"owner":1,"x":3,"y":5,"created":0}},
           {{"id":3,"type":{WARRIOR},"owner":2,"x":5,"y":5}},{{"id":4,"type":{WARRIOR},"owner":2,"x":5,"y":6}}"#
    );
    let mut e = board(tile, &units, 0);
    let legal = e.legal_commands();
    assert!(legal.contains(&Command::Move { unit: 1, to: wc(4, 3) }), "a flier crosses the water tile");
    assert!(legal.contains(&Command::Move { unit: 1, to: wc(5, 3) }), "and a baby dragon moves two tiles");
    assert!(legal.contains(&Command::Attack { unit: 2, target: wc(5, 5) }), "a fire dragon has range 2");
    let before = e.unit(4).unwrap().health;
    e.apply(Command::Attack { unit: 2, target: wc(5, 5) }).expect("legal");
    assert!(e.unit(4).is_none_or(|u| u.health < before), "the splash hurts the enemy next to the target");
    assert_eq!(e.unit(2).unwrap().coordinates, wc(3, 5), "a ranged attacker does not step in");
}

// ---- Enchant Animal ----

/// A field board where (1,0) is player 1's tile with game on it; `units` stand on the board.
fn game_board(units: &str, stars: i32, techs: &[i32]) -> Engine {
    let tile = |x: i32, y: i32| {
        if (x, y) == (1, 0) {
            (FIELD, r#","resource":1"#.to_string())
        } else {
            (FIELD, String::new())
        }
    };
    board_with(tile, units, 0, Some(techs), stars)
}

const FM: [i32; 2] = [TECH_BASIC, TECH_FOREST_MAGIC];

#[test]
fn enchant_animal_is_built_on_game_only_and_needs_forest_magic_and_three_stars() {
    let e = game_board("", 20, &FM);
    assert!(can_build(&e, wc(1, 0), ENCHANT_ANIMAL));
    assert!(!can_build(&e, wc(0, 1), ENCHANT_ANIMAL), "a tile without game cannot be enchanted");
    let e = game_board("", 20, &[TECH_BASIC]);
    assert!(!can_build(&e, wc(1, 0), ENCHANT_ANIMAL), "Forest Magic is needed");
    let e = game_board("", 2, &FM);
    assert!(!can_build(&e, wc(1, 0), ENCHANT_ANIMAL), "it costs three stars");
    let e = game_board("", 3, &FM);
    assert!(can_build(&e, wc(1, 0), ENCHANT_ANIMAL));
}

#[test]
fn enchant_animal_uses_the_game_up_and_makes_a_polytaur_with_no_home() {
    let mut e = game_board("", 20, &FM);
    let unit_id = e.s.current_unit_id;
    e.apply(Command::Build { at: wc(1, 0), improvement: ENCHANT_ANIMAL }).expect("legal");
    assert_eq!(e.player(1).unwrap().currency, 17, "three stars");
    assert_eq!(resource_at(&e, wc(1, 0)), 0, "the game is used up");
    assert_eq!(e.tile(wc(1, 0)).unwrap().improvement, 0, "no building is left behind");
    let polytaur = e.unit_at(wc(1, 0)).expect("a unit stands where the game was");
    assert_eq!((polytaur.type_idx, polytaur.owner), (POLYTAUR, 1));
    assert_eq!(polytaur.home, wc(-1, -1), "an independent unit belongs to no city");
    assert_eq!(e.s.current_unit_id, unit_id + 1, "the polytaur is trained through a train action and takes an id");
}

#[test]
fn enchant_animal_under_the_builders_own_unit_pushes_that_unit_aside() {
    let units = format!(r#"{{"id":1,"type":{WARRIOR},"owner":1,"x":1,"y":0}}"#);
    let mut e = game_board(&units, 20, &FM);
    e.apply(Command::Build { at: wc(1, 0), improvement: ENCHANT_ANIMAL }).expect("legal");
    assert_eq!(e.unit_at(wc(1, 0)).unwrap().type_idx, POLYTAUR);
    assert_ne!(e.unit(1).expect("the warrior lives").coordinates, wc(1, 0));
}

#[test]
fn a_polytaur_dashes_so_it_can_move_and_then_attack_in_one_turn() {
    let units = format!(
        r#"{{"id":1,"type":{POLYTAUR},"owner":1,"x":3,"y":3}},{{"id":2,"type":{WARRIOR},"owner":2,"x":5,"y":3}}"#
    );
    let mut e = board(field, &units, 0);
    assert!(
        !e.legal_commands().contains(&Command::Attack { unit: 1, target: wc(5, 3) }),
        "two tiles away: out of reach until it moves"
    );
    e.apply(Command::Move { unit: 1, to: wc(4, 3) }).expect("legal");
    assert!(
        e.legal_commands().contains(&Command::Attack { unit: 1, target: wc(5, 3) }),
        "dash: it can still attack after moving"
    );
}

// ---- the Sanctuary ----

const FORESTRY: [i32; 3] = [TECH_BASIC, TECH_FOREST_MAGIC, TECH_FORESTRY];

#[test]
fn a_sanctuary_needs_forestry_and_costs_five_stars() {
    let tile = |x: i32, y: i32| if (x, y) == (1, 0) { (FOREST, String::new()) } else { (FIELD, String::new()) };
    let e = board_with(tile, "", 0, Some(&FM), 20);
    assert!(!can_build(&e, wc(1, 1), SANCTUARY), "Forestry is needed");
    let mut e = board_with(tile, "", 0, Some(&FORESTRY), 20);
    assert!(can_build(&e, wc(1, 1), SANCTUARY));
    assert!(!can_build(&e, wc(1, 0), CLEAR_FOREST), "the Sanctuary takes Clear Forest's place");
    e.apply(Command::Build { at: wc(1, 1), improvement: SANCTUARY }).expect("legal");
    assert_eq!(e.player(1).unwrap().currency, 15, "five stars");
    let poor = board_with(tile, "", 0, Some(&FORESTRY), 4);
    assert!(!can_build(&poor, wc(1, 1), SANCTUARY), "four stars are not enough");
}

#[test]
fn a_sanctuary_needs_a_forest_beside_it_and_only_one_stands_under_a_city() {
    let e = board_with(field, "", 0, Some(&FORESTRY), 50);
    assert!(!can_build(&e, wc(1, 1), SANCTUARY), "no forest in sight");
    let tile = |x: i32, y: i32| {
        if (x, y) == (1, 0) || (x, y) == (6, 5) {
            (FOREST, String::new())
        } else {
            (FIELD, String::new())
        }
    };
    let mut e = board_with(tile, "", 0, Some(&FORESTRY), 50);
    assert!(can_build(&e, wc(0, 1), SANCTUARY), "the second spot is fine until the first is built");
    e.apply(Command::Build { at: wc(1, 1), improvement: SANCTUARY }).expect("legal");
    assert!(!can_build(&e, wc(0, 1), SANCTUARY), "it is limited to one near a city");
}
