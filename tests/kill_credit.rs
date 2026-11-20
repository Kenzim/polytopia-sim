//! Who is credited with a kill.
//!
//! The kill count of a player goes up when an attack takes the last health of a unit. The player credited is the owner
//! of the unit on the attack's origin tile, except when the attack starts and ends on one tile: an explosion hits each
//! victim with an attack whose origin and target are the victim's own tile, and there the acting player (the exploder)
//! gets the kill, not the victim's owner. A kill of a Nature unit counts for nobody.
//!
//! Evidence from the game: weekly games 2e31718c and bf8049cf (week 2026-10-26). The Cymanti bots' segments exploded
//! among enemy units, and the engine paid those kills to the victims' owners. The server's kill counts at the end of
//! both games, one per player, now match the engine's.
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

fn kills(e: &Engine, player: u8) -> u32 {
    e.s.player_runtime.iter().find(|r| r.player == player).map(|r| r.kills).unwrap_or(0)
}

fn casualties(e: &Engine, player: u8) -> u32 {
    e.s.player_runtime.iter().find(|r| r.player == player).map(|r| r.casualties).unwrap_or(0)
}

#[test]
fn an_explosion_pays_its_kill_to_the_exploder() {
    let mut e = board(&format!(
        r#"{{"id":1,"type":{DOOMUX},"owner":1,"x":1,"y":1,"health":100}},
           {{"id":2,"type":{WARRIOR},"owner":2,"x":2,"y":1,"health":10}}"#
    ));
    e.apply(Command::Explode { unit: 1 }).expect("the explode is legal");
    assert!(e.unit(2).is_none(), "the warrior beside the explosion is dead");
    assert_eq!(kills(&e, 1), 1, "the exploder is credited");
    assert_eq!(kills(&e, 2), 0, "the victim's owner is not");
    assert_eq!(casualties(&e, 2), 1, "the victim's owner has the casualty");
}

#[test]
fn an_ordinary_attack_pays_its_kill_to_the_attacker() {
    let mut e = board(&format!(
        r#"{{"id":1,"type":{WARRIOR},"owner":1,"x":1,"y":1,"health":100}},
           {{"id":2,"type":{WARRIOR},"owner":2,"x":2,"y":1,"health":10}}"#
    ));
    e.apply(Command::Attack { unit: 1, target: polytopia_sim::WorldCoordinates { x: 2, y: 1 } })
        .expect("the attack is legal");
    assert!(e.unit(2).is_none());
    assert_eq!((kills(&e, 1), kills(&e, 2)), (1, 0));
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

/// Replays a weekly game and compares every player's kill count with the server's final state. Reads
/// `POLYTOPIA_WEEKLY_RAW`; skips without the files.
fn kills_match_the_server(prefix: &str) {
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
    for r in &save.current.player_runtime {
        assert_eq!(kills(&e, r.player), r.kills, "kills of player {} in {prefix}", r.player);
    }
}

#[test]
fn the_weekly_games_have_the_servers_kill_counts() {
    kills_match_the_server("bf8049cf");
    kills_match_the_server("2e31718c");
}
