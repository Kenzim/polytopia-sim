//! An infiltrator may not target a city that is being captured: a city with an owner on whose tile stands a unit of
//! another player (an invader that has moved in and not yet taken it). Retail answers such an attack with error
//! 1013 "Invalid attack target". The tile's own owner decides; a unit of the city's owner standing in it is no
//! invasion.
//!
//! The constructed test needs no files. The replay test reads live game 1d9ec9f1 (command 761) from
//! `POLYTOPIA_DIVERGENCE` or `POLYTOPIA_CLIENT_GAMES`, and skips without it.
use polytopia_sim::engine::replay::SaveFile;
use polytopia_sim::engine::{Command, Engine, GameOptions};
use polytopia_sim::WorldCoordinates as Wc;

const CITY: i32 = 1;
const FIELD: i32 = 3;

fn type_of(e: &Engine, id: &str) -> i32 {
    (0..200).find(|i| e.t.unit(*i).is_some_and(|u| u.data.id == id)).unwrap_or_else(|| panic!("unit type {id}"))
}

/// Player 1 to move with a cloak beside a player 2 city. Returns the engine, the cloak's id, and the city tile.
fn setup() -> (Engine, u32, Wc) {
    let mut e = Engine::new_game(&GameOptions::default());
    assert_eq!(e.current_player(), 1);
    let city =
        e.s.map.tiles.iter().find(|t| t.owner == 2 && t.improvement == CITY).expect("a city of player 2").coordinates;
    let beside = Wc { x: if city.x > 0 { city.x - 1 } else { city.x + 1 }, y: city.y };
    e.s.units.retain(|u| u.coordinates != city && u.coordinates != beside);
    let t = e.s.map.tiles.iter_mut().find(|t| t.coordinates == beside).unwrap();
    t.terrain = FIELD;
    t.improvement = 0;
    t.has_ruin = false;
    let cloak = type_of(&e, "cloak");
    let id = e.spawn_unit(1, cloak, beside, beside, false).expect("the cloak");
    (e, id, city)
}

fn offers_infiltration(e: &Engine, unit: u32, city: Wc) -> bool {
    e.legal_commands().contains(&Command::Attack { unit, target: city })
}

#[test]
fn infiltrate_is_refused_while_a_foreign_unit_stands_in_the_city_and_allowed_for_the_owners_unit() {
    let (mut e, cloak, city) = setup();
    assert!(offers_infiltration(&e, cloak, city), "an empty enemy city can be infiltrated");

    let warrior = type_of(&e, "warrior");
    // The city owner's own unit stands in it: nothing is being captured.
    let own = e.spawn_unit(2, warrior, city, city, false).expect("player 2 unit");
    assert_eq!(e.unit_at(city).map(|u| u.id), Some(own));
    assert!(offers_infiltration(&e, cloak, city), "a unit of the city's owner is no invasion");
    assert!(e.attack_targets(e.unit(cloak).unwrap()).contains(&city));

    // An invader of player 1 stands in it instead: the city is being captured.
    e.s.units.retain(|u| u.coordinates != city);
    let invader = e.spawn_unit(1, warrior, city, city, true).expect("player 1 unit");
    assert_eq!(e.unit_at(city).map(|u| u.id), Some(invader));
    assert!(!offers_infiltration(&e, cloak, city), "retail: invalid attack target");
    assert!(!e.attack_targets(e.unit(cloak).unwrap()).contains(&city));
    assert!(!e.attack_targets_with_range(e.unit(cloak).unwrap(), 1, false).contains(&city));

    // A third player's unit counts as an invader too: only the tile's owner is exempt.
    e.s.units.retain(|u| u.coordinates != city);
    e.spawn_unit(3, warrior, city, city, true).expect("player 3 unit");
    assert!(!offers_infiltration(&e, cloak, city));
}

fn divergence_dirs() -> Vec<String> {
    ["POLYTOPIA_DIVERGENCE", "POLYTOPIA_CLIENT_GAMES"].iter().filter_map(|v| std::env::var(v).ok()).collect()
}

/// Game 1d9ec9f1, turn 34: the cloak (unit 166) at 5,3 may not infiltrate the city at 5,4 that player 1's own
/// warrior (unit 150) has just moved into. Retail rejected command 761 (that attack) with error 1013.
#[test]
fn live_game_1d9ec9f1_command_761_is_not_offered() {
    let game = "1d9ec9f1-ca63-42db-a96c-08df3291b15a";
    let blobs = divergence_dirs().into_iter().find_map(|dir| {
        Some((
            std::fs::read(format!("{dir}/{game}_init.bin")).ok()?,
            std::fs::read(format!("{dir}/{game}_cur.bin")).ok()?,
        ))
    });
    let Some((init, cur)) = blobs else {
        eprintln!("skipped {game}: blobs absent");
        return;
    };
    let save = SaveFile::parse_server(&init, &cur).unwrap();
    let mut e = Engine::from_state(save.initial.clone());
    if save.commands.iter().any(|c| c.kind == 20) {
        e.start_match();
    }
    for c in save.commands.iter().take(761) {
        if c.kind == 20 || c.player == 255 {
            continue;
        }
        if c.kind == 22 {
            if e.end_match().is_err() {
                e.s.current_command += 1;
            }
            continue;
        }
        let Some(cmd) = e.command_from_raw(c) else {
            e.s.current_command += 1;
            continue;
        };
        e.apply_unchecked(cmd);
    }
    let (city, cloak) = (Wc { x: 5, y: 4 }, 166);
    assert_eq!((e.s.current_turn, e.current_player()), (34, 1));
    assert_eq!(e.tile(city).map(|t| t.owner), Some(2));
    let invader = e.unit_at(city).expect("a unit in the city");
    assert_eq!((invader.id, invader.owner), (150, 1));
    assert_eq!(e.unit(cloak).map(|u| (u.owner, u.coordinates)), Some((1, Wc { x: 5, y: 3 })));
    assert!(!offers_infiltration(&e, cloak, city), "retail: 1013 Invalid attack target");
}
