//! The facing of a unit after an explosion.
//!
//! Retail's attack routine sets the facing of the unit on the origin tile towards the target and of the unit on the target
//! tile towards the origin, with no test that the two tiles differ. An explosion hits each victim with an attack whose
//! origin and target are the victim's own tile, and the direction of two equal tiles is "none" (8), so a surviving victim
//! ends up facing nowhere, whatever way it faced before.
//!
//! Evidence from the game: weekly game 2e31718c (week 2026-10-26). Three of the Cymanti player's units were hit by
//! enemy explosions late on and the server stores direction 8 for each; the engine kept the facing from their own
//! earlier attacks.
use polytopia_sim::engine::replay::SaveFile;
use polytopia_sim::engine::{Command, Engine};
use polytopia_sim::json::Json;

const CYMANTI: i64 = 17;
const BARDUR: i64 = 3;
const DOOMUX: i64 = 29;
const WARRIOR: i64 = 2;

/// Player 1 (Cymanti, to move) and player 2 on a 7x3 field board; `units` is the unit list as position JSON.
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
           "players":[{{"id":1,"tribe":{CYMANTI},"human":true,"stars":5}},{{"id":2,"tribe":{BARDUR},"stars":5}}],
           "tiles":[{}],
           "improvements":[{{"x":0,"y":0,"level":1}},{{"x":6,"y":0,"level":1}}],
           "units":[{units}]}}"#,
        tiles.join(",")
    );
    Engine::from_position(&Json::parse(&text).unwrap()).unwrap()
}

fn weekly_game(prefix: &str) -> Option<String> {
    let dir = std::env::var("POLYTOPIA_WEEKLY_RAW").unwrap_or_default();
    let name = std::fs::read_dir(&dir)
        .ok()?
        .filter_map(|d| d.ok())
        .map(|d| d.file_name().to_string_lossy().into_owned())
        .find(|n| n.starts_with(prefix) && n.ends_with("_init.bin"))?;
    let stem = name.strip_suffix("_init.bin")?;
    Some(format!("{dir}/{stem}_init.bin+{dir}/{stem}_cur.bin"))
}

#[test]
fn an_explosion_leaves_the_victim_facing_nowhere() {
    let mut e = board(&format!(
        r#"{{"id":1,"type":{DOOMUX},"owner":1,"x":1,"y":1,"health":100}},
           {{"id":2,"type":{WARRIOR},"owner":2,"x":2,"y":1,"health":100,"direction":3}}"#
    ));
    assert_eq!(e.unit(2).map(|u| u.direction), Some(3), "the victim starts facing north");
    e.apply(Command::Explode { unit: 1 }).expect("the explode is legal");
    let victim = e.unit(2).expect("the victim survives at full health");
    assert!(victim.health < 100, "the explosion hurt it");
    assert_eq!(victim.direction, 8, "and it now faces nowhere");
}

#[test]
fn an_ordinary_attack_turns_both_units() {
    let mut e = board(&format!(
        r#"{{"id":1,"type":{WARRIOR},"owner":1,"x":1,"y":1,"health":100}},
           {{"id":2,"type":{WARRIOR},"owner":2,"x":2,"y":1,"health":100,"direction":3}}"#
    ));
    e.apply(Command::Attack { unit: 1, target: polytopia_sim::WorldCoordinates { x: 2, y: 1 } })
        .expect("the attack is legal");
    assert_eq!(e.unit(1).map(|u| u.direction), Some(5), "the attacker faces east");
    assert_eq!(e.unit(2).map(|u| u.direction), Some(1), "the defender faces west, back at the attacker");
}

/// Replays a weekly game and compares every unit's facing with the server's final state; skips without the files.
fn facings_match_the_server(prefix: &str) {
    let Some(path) = weekly_game(prefix) else {
        eprintln!("skipped: no weekly replay files");
        return;
    };
    let save = SaveFile::open(&path).unwrap_or_else(|e| panic!("{path}: {e}"));
    let mut e = Engine::from_state(save.initial.clone());
    for c in &save.commands {
        if c.kind == 20 || c.player == 255 {
            continue;
        }
        let Some(cmd) = e.command_from_raw(c) else {
            e.s.current_command += 1;
            continue;
        };
        e.apply_unchecked(cmd);
    }
    for u in &save.current.units {
        let mine = e.unit(u.id).unwrap_or_else(|| panic!("unit {} of {prefix} is missing", u.id));
        assert_eq!(mine.direction, u.direction, "facing of unit {} in {prefix}", u.id);
    }
}

#[test]
fn the_weekly_games_have_the_servers_unit_facings() {
    for prefix in ["2e31718c", "bf8049cf", "cd47553c", "5af11d91", "8f69e437", "f112aa5d"] {
        facings_match_the_server(prefix);
    }
}
