//! The autofreeze of a Mooni (the Polaris tribe): a move command freezes the tiles and units round the
//! destination.
//!
//! The square of radius 1 round the tile the unit reaches is looked at. Water in it turns to Ice, and every tile
//! that can be frozen takes the climate of the unit's owner. An enemy unit in it is frozen: it counts as moved and
//! attacked, so it can neither move nor attack until its owner has ended a turn. The unit's own side, and units
//! of a player at peace, are left alone. A unit without the ability freezes nothing.
//!
//! Evidence from the game: in the saved Polaris replays a Mooni step is refused by an engine without the freeze,
//! because the server's board has Ice where the Mooni walked next.
use polytopia_sim::engine::replay::SaveFile;
use polytopia_sim::engine::{Command, Engine};
use polytopia_sim::json::Json;
use polytopia_sim::WorldCoordinates as Wc;

const MOONI: i64 = 22;
const WARRIOR: i64 = 2;
const GAAMI: i64 = 27;
const WATER: i32 = 1;
const ICE: i32 = 6;
const POLARIS_CLIMATE: i32 = 16;

fn wc(x: i32, y: i32) -> Wc {
    Wc { x, y }
}

/// Player 1 (Polaris, to move) and player 2 on a 7x3 board: row 2 is water from x=2 to x=4, the rest is field.
fn board(units: &str) -> Engine {
    let mut tiles = Vec::new();
    for y in 0..3 {
        for x in 0..7 {
            let water = y == 2 && (2..=4).contains(&x);
            let mut t = format!(r#""x":{x},"y":{y},"terrain":{},"climate":2"#, if water { 1 } else { 3 });
            match (x, y) {
                (0, 0) => t.push_str(r#","city":true,"capital":true,"owner":1,"ruling":[0,0]"#),
                (6, 0) => t.push_str(r#","city":true,"capital":true,"owner":2,"ruling":[6,0]"#),
                _ => {}
            }
            tiles.push(format!("{{{t}}}"));
        }
    }
    let text = format!(
        r#"{{"format":"polytopia-position","game":{{"turn":6,"width":7,"height":3,"current_player":1}},
           "players":[{{"id":1,"tribe":16,"human":true,"stars":5}},{{"id":2,"tribe":3,"stars":5}}],
           "tiles":[{}],
           "improvements":[{{"x":0,"y":0,"level":1}},{{"x":6,"y":0,"level":1}}],
           "units":[{units}]}}"#,
        tiles.join(",")
    );
    Engine::from_position(&Json::parse(&text).unwrap()).unwrap()
}

fn unit(id: u32, kind: i64, owner: u8, x: i32, y: i32) -> String {
    format!(r#"{{"id":{id},"type":{kind},"owner":{owner},"x":{x},"y":{y},"health":100}}"#)
}

#[test]
fn a_mooni_move_freezes_the_enemy_unit_beside_the_destination() {
    let units = [unit(1, MOONI, 1, 1, 1), unit(2, WARRIOR, 2, 3, 1)].join(",");
    let mut e = board(&units);
    e.apply(Command::Move { unit: 1, to: wc(2, 1) }).expect("the move is legal");
    let foe = e.unit(2).unwrap();
    assert!(foe.frozen && foe.moved && foe.attacked, "the enemy warrior is frozen: {foe:?}");
    assert!(!e.unit(1).unwrap().frozen, "the Mooni itself is not frozen");
}

#[test]
fn the_water_beside_the_destination_turns_to_ice_and_the_rest_stays() {
    let units = unit(1, MOONI, 1, 1, 1);
    let mut e = board(&units);
    e.apply(Command::Move { unit: 1, to: wc(2, 1) }).expect("the move is legal");
    let terrain = |e: &Engine, x, y| e.tile(wc(x, y)).unwrap().terrain;
    for x in 2..=3 {
        assert_eq!(terrain(&e, x, 2), ICE, "({x},2) is next to the destination");
    }
    assert_eq!(terrain(&e, 4, 2), WATER, "(4,2) is two tiles away");
    assert_eq!(e.tile(wc(2, 2)).unwrap().climate, POLARIS_CLIMATE);
}

#[test]
fn own_units_are_not_frozen() {
    let units = [unit(1, MOONI, 1, 1, 1), unit(2, WARRIOR, 1, 3, 1)].join(",");
    let mut e = board(&units);
    e.apply(Command::Move { unit: 1, to: wc(2, 1) }).expect("the move is legal");
    assert!(!e.unit(2).unwrap().frozen);
}

#[test]
fn a_unit_without_the_ability_freezes_nothing() {
    let units = [unit(1, WARRIOR, 1, 1, 1), unit(2, WARRIOR, 2, 3, 1)].join(",");
    let mut e = board(&units);
    e.apply(Command::Move { unit: 1, to: wc(2, 1) }).expect("the move is legal");
    assert!(!e.unit(2).unwrap().frozen);
    assert_eq!(e.tile(wc(2, 2)).unwrap().terrain, WATER);
}

#[test]
fn a_frozen_unit_cannot_act_until_its_owner_has_ended_a_turn() {
    let units = [unit(1, MOONI, 1, 1, 1), unit(2, WARRIOR, 2, 3, 1)].join(",");
    let mut e = board(&units);
    e.apply(Command::Move { unit: 1, to: wc(2, 1) }).unwrap();
    e.apply(Command::EndTurn).unwrap();
    assert!(e.unit(2).unwrap().frozen, "still frozen on its owner's turn");
    assert!(e
        .legal_commands()
        .iter()
        .all(|c| !matches!(c, Command::Move { unit: 2, .. } | Command::Attack { unit: 2, .. })));
    e.apply(Command::EndTurn).unwrap();
    e.apply(Command::EndTurn).unwrap();
    assert!(!e.unit(2).unwrap().frozen, "thawed once its owner has ended a turn");
}

/// The step a melee unit takes into the tile of the unit it killed is a move that is not a push, so a Gaami that
/// kills freezes round the tile it steps into.
#[test]
fn a_gaami_that_kills_freezes_round_the_tile_it_steps_into() {
    let units = [unit(1, GAAMI, 1, 1, 1), unit(2, WARRIOR, 2, 2, 1), unit(3, WARRIOR, 2, 3, 1)].join(",");
    let mut e = board(&units);
    e.s.units.iter_mut().find(|u| u.id == 2).unwrap().health = 30;
    e.apply(Command::Attack { unit: 1, target: wc(2, 1) }).expect("the attack is legal");
    assert!(e.unit(2).is_none(), "the first warrior is dead: {:?}", e.unit(2));
    assert_eq!(e.unit(1).unwrap().coordinates, wc(2, 1), "the Gaami stepped into its tile");
    let far = e.unit(3).unwrap();
    assert!(far.frozen && far.moved && far.attacked, "the second warrior is frozen: {far:?}");
    assert_eq!(e.tile(wc(3, 2)).unwrap().terrain, ICE, "water beside the tile it stepped into turned to ice");
}

/// A Gaami that does not kill stays where it is and freezes nothing: only a move freezes.
#[test]
fn a_gaami_attack_that_does_not_kill_freezes_nothing() {
    let units = [unit(1, GAAMI, 1, 1, 1), unit(2, WARRIOR, 2, 2, 1), unit(3, WARRIOR, 2, 3, 1)].join(",");
    let mut e = board(&units);
    e.s.units.iter_mut().find(|u| u.id == 2).unwrap().health = 100;
    e.apply(Command::Attack { unit: 1, target: wc(2, 1) }).expect("the attack is legal");
    assert!(e.unit(2).is_some(), "the first warrior survived");
    assert_eq!(e.unit(1).unwrap().coordinates, wc(1, 1), "the Gaami did not move");
    assert!(!e.unit(3).unwrap().frozen);
    assert_eq!(e.tile(wc(3, 2)).unwrap().terrain, WATER);
}

/// Replays the saved Polaris games of the human corpus and checks that a Mooni move is seldom refused. Skips
/// when the private replay files are absent.
#[test]
fn replayed_polaris_games_accept_mooni_moves() {
    let dir = std::env::var("POLYTOPIA_HUMAN_RAW").unwrap_or_default();
    let Ok(files) = std::fs::read_dir(&dir) else {
        eprintln!("skipped: no replays in {dir}");
        return;
    };
    let mut names: Vec<String> = files.flatten().map(|f| f.file_name().to_string_lossy().into_owned()).collect();
    names.sort();
    let (mut games, mut moves, mut refused) = (0, 0, 0);
    for init in names.iter().filter(|n| n.ends_with("_init.bin")) {
        let cur = init.replace("_init.bin", "_cur.bin");
        let Ok(save) = SaveFile::open(&format!("{dir}/{init}+{dir}/{cur}")) else { continue };
        if !save.initial.players.iter().any(|p| p.tribe == 16) {
            continue;
        }
        games += 1;
        let mut e = Engine::from_state(save.initial.clone());
        for c in &save.commands {
            if c.kind == 20 || c.player == 255 {
                continue;
            }
            let Some(cmd) = e.command_from_raw(c) else {
                e.s.current_command += 1;
                continue;
            };
            if e.current_player() != c.player {
                break;
            }
            if matches!(cmd, Command::Move { unit, .. } if e.unit(unit).is_some_and(|u| u.type_idx == MOONI as i32)) {
                moves += 1;
                if e.clone().apply(cmd).is_err() {
                    refused += 1;
                }
            }
            e.apply_unchecked(cmd);
        }
        if games >= 40 {
            break;
        }
    }
    eprintln!("{games} Polaris games, {moves} Mooni moves, {refused} refused");
    if games > 0 {
        assert!(refused * 2 <= moves, "{refused} of {moves} Mooni moves refused");
    }
}
