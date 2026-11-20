//! The freeze on attack of an Ice Archer (the Polaris tribe): the unit it hits is frozen if it survives the damage.
//!
//! The damage is dealt as for any attack. A surviving target is then frozen: it counts as moved and attacked, so it
//! can neither move nor attack until its owner has ended a turn. The tile is not frozen, the attacker does not
//! step into the tile or take a retaliation, and a unit without the ability freezes nothing.
//!
//! Evidence from the game: in the saved Polaris replays, a unit that an Ice Archer hit is not moved or used again
//! on its owner's next turn.
use polytopia_sim::engine::replay::SaveFile;
use polytopia_sim::engine::{Command, Engine};
use polytopia_sim::json::Json;
use polytopia_sim::WorldCoordinates as Wc;

const ICE_ARCHER: i64 = 25;
const WARRIOR: i64 = 2;
const POLARIS_CLIMATE: i32 = 16;

fn wc(x: i32, y: i32) -> Wc {
    Wc { x, y }
}

/// Player 1 (Polaris, to move) and player 2 on a 7x3 field of land.
fn board(units: &str) -> Engine {
    let mut tiles = Vec::new();
    for y in 0..3 {
        for x in 0..7 {
            let mut t = format!(r#""x":{x},"y":{y},"terrain":3,"climate":2"#);
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

fn unit(id: u32, kind: i64, owner: u8, x: i32, y: i32, health: u32) -> String {
    format!(r#"{{"id":{id},"type":{kind},"owner":{owner},"x":{x},"y":{y},"health":{health}}}"#)
}

#[test]
fn an_ice_archer_freezes_the_unit_it_hit() {
    let units = [unit(1, ICE_ARCHER, 1, 1, 1, 100), unit(2, WARRIOR, 2, 3, 1, 100)].join(",");
    let mut e = board(&units);
    e.apply(Command::Attack { unit: 1, target: wc(3, 1) }).expect("the attack is legal");
    let foe = e.unit(2).unwrap();
    assert!(foe.health < 100, "the arrow does its damage: {foe:?}");
    assert!(foe.frozen && foe.moved && foe.attacked, "the warrior is frozen: {foe:?}");
    let archer = e.unit(1).unwrap();
    assert!(!archer.frozen, "the archer itself is not frozen");
    assert_eq!(archer.coordinates, wc(1, 1), "the archer stays where it was");
    assert_eq!(archer.health, 100, "a frozen unit does not retaliate");
}

#[test]
fn a_unit_without_the_ability_freezes_nothing() {
    let units = [unit(1, WARRIOR, 1, 2, 1, 100), unit(2, WARRIOR, 2, 3, 1, 100)].join(",");
    let mut e = board(&units);
    e.apply(Command::Attack { unit: 1, target: wc(3, 1) }).expect("the attack is legal");
    assert!(!e.unit(2).unwrap().frozen);
}

#[test]
fn only_the_unit_is_frozen_not_its_tile() {
    let units = [unit(1, ICE_ARCHER, 1, 1, 1, 100), unit(2, WARRIOR, 2, 3, 1, 100)].join(",");
    let mut e = board(&units);
    e.apply(Command::Attack { unit: 1, target: wc(3, 1) }).unwrap();
    assert_eq!(e.tile(wc(3, 1)).unwrap().climate, 2, "the tile keeps its climate");
    assert_ne!(e.tile(wc(3, 1)).unwrap().climate, POLARIS_CLIMATE);
}

#[test]
fn a_unit_that_dies_is_not_frozen_and_the_archer_does_not_step_in() {
    let units = [unit(1, ICE_ARCHER, 1, 1, 1, 100), unit(2, WARRIOR, 2, 3, 1, 5)].join(",");
    let mut e = board(&units);
    e.apply(Command::Attack { unit: 1, target: wc(3, 1) }).unwrap();
    assert!(e.unit(2).is_none(), "the warrior is dead");
    let archer = e.unit(1).unwrap();
    assert_eq!(archer.coordinates, wc(1, 1));
    assert_eq!(archer.xp, 1, "the kill still pays xp");
    assert!(archer.attacked);
}

#[test]
fn a_frozen_unit_cannot_act_until_its_owner_has_ended_a_turn() {
    let units = [unit(1, ICE_ARCHER, 1, 1, 1, 100), unit(2, WARRIOR, 2, 3, 1, 100)].join(",");
    let mut e = board(&units);
    e.apply(Command::Attack { unit: 1, target: wc(3, 1) }).unwrap();
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

/// Replays the saved Polaris games of the human corpus: every unit an Ice Archer hit and left alive must be frozen,
/// and a frozen unit must seldom be the one a player commands. Skips when the private replay files are absent.
#[test]
fn replayed_polaris_games_freeze_the_units_an_ice_archer_hit() {
    let dir = std::env::var("POLYTOPIA_HUMAN_RAW").unwrap_or_default();
    let Ok(files) = std::fs::read_dir(&dir) else {
        eprintln!("skipped: no replays in {dir}");
        return;
    };
    let mut names: Vec<String> = files.flatten().map(|f| f.file_name().to_string_lossy().into_owned()).collect();
    names.sort();
    let (mut games, mut hits, mut frozen, mut commands, mut while_frozen) = (0, 0, 0, 0, 0);
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
            let actor = match cmd {
                Command::Move { unit, .. } | Command::Attack { unit, .. } => Some(unit),
                _ => None,
            };
            if let Some(u) = actor.and_then(|u| e.unit(u)) {
                commands += 1;
                if u.frozen {
                    while_frozen += 1;
                }
            }
            let archer_target = match cmd {
                Command::Attack { unit, target } if e.unit(unit).is_some_and(|u| u.type_idx == ICE_ARCHER as i32) => {
                    Some(target)
                }
                _ => None,
            };
            e.apply_unchecked(cmd);
            if let Some(unit) = archer_target.and_then(|t| e.unit_at(t)) {
                hits += 1;
                if unit.frozen {
                    frozen += 1;
                }
            }
        }
        if games >= 40 {
            break;
        }
    }
    eprintln!("{games} Polaris games, {hits} Ice Archer hits that left the unit alive, {frozen} frozen; {commands} moves and attacks, {while_frozen} by a frozen unit");
    assert_eq!(hits, frozen, "every surviving target is frozen");
    assert!(while_frozen * 50 <= commands, "{while_frozen} of {commands} moves and attacks were by a frozen unit");
}
