//! What an attack does to the Pacifist task.
//!
//! An attack starts the Pacifist streak again, which is the `turns` counter of the task. Retail's `converts` counter
//! belongs to the Convert task, so an attack leaves it at zero. The engine once used it as an "attacked this turn"
//! flag, which showed in the state of a player who attacked on the last turn.
//!
//! Evidence from the game: weekly games 2e31718c and bf8049cf (week 2026-10-26). Player 1 attacked in the last turn,
//! and the server's final Pacifist task has `converts` 0 where the engine had 1. All six week 10-26 games now have the
//! server's task values for every player.
use polytopia_sim::engine::replay::SaveFile;
use polytopia_sim::engine::{Command, Engine};
use polytopia_sim::json::Json;
use polytopia_sim::{TaskKind, WorldCoordinates};

const BARDUR: i64 = 3;
const WARRIOR: i64 = 2;

/// Player 1 (to move) holds a warrior beside player 2's at 4 turns of the Pacifist streak; `defender_health` decides
/// whether the attack kills.
fn board(defender_health: u32) -> Engine {
    let mut tiles = Vec::new();
    for y in 0..3 {
        for x in 0..6 {
            let mut t = format!(r#""x":{x},"y":{y},"terrain":3,"climate":2"#);
            match (x, y) {
                (0, 0) => t.push_str(r#","city":true,"capital":true,"owner":1,"ruling":[0,0]"#),
                (5, 0) => t.push_str(r#","city":true,"capital":true,"owner":2,"ruling":[5,0]"#),
                _ => {}
            }
            tiles.push(format!("{{{t}}}"));
        }
    }
    let text = format!(
        r#"{{"format":"polytopia-position","game":{{"turn":6,"width":6,"height":3,"current_player":1}},
           "players":[{{"id":1,"tribe":{BARDUR},"human":true,"stars":5,"tasks":[{{"kind":1,"started":true,"completed":false,"value":4}}]}},{{"id":2,"tribe":{BARDUR},"stars":5}}],
           "tiles":[{}],
           "improvements":[{{"x":0,"y":0,"level":1}},{{"x":5,"y":0,"level":1}}],
           "units":[{{"id":1,"type":{WARRIOR},"owner":1,"x":1,"y":1,"health":100}},
                    {{"id":2,"type":{WARRIOR},"owner":2,"x":2,"y":1,"health":{defender_health}}}]}}"#,
        tiles.join(",")
    );
    Engine::from_position(&Json::parse(&text).unwrap()).unwrap()
}

fn pacifist(e: &Engine) -> (i32, i32) {
    let t = e.task(1, TaskKind::Pacifist).expect("player 1 has the Pacifist task");
    (t.turns, t.converts)
}

#[test]
fn an_attack_restarts_the_streak_and_leaves_converts_alone() {
    let mut e = board(100);
    assert_eq!(pacifist(&e), (4, 0));
    e.apply(Command::Attack { unit: 1, target: WorldCoordinates { x: 2, y: 1 } }).expect("the attack is legal");
    assert!(e.unit(2).is_some(), "the defender survives, so the attack is a plain one");
    assert_eq!(pacifist(&e), (0, 0));
}

#[test]
fn a_killing_attack_leaves_converts_alone_too() {
    let mut e = board(5);
    e.apply(Command::Attack { unit: 1, target: WorldCoordinates { x: 2, y: 1 } }).expect("the attack is legal");
    assert!(e.unit(2).is_none());
    let (_, converts) = pacifist(&e);
    assert_eq!(converts, 0);
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

/// Replays a weekly game and compares the value of every task of every player with the server's final state; skips
/// without the files. The list order is not compared: the server's order of the tasks of the AI players is a separate matter.
fn task_values_match_the_server(prefix: &str) {
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
    for server in &save.current.tasks {
        for t in &server.tasks {
            let mine = e
                .task(server.player, t.kind)
                .unwrap_or_else(|| panic!("{prefix}: player {} lacks task {:?}", server.player, t.kind));
            assert_eq!(mine, t, "{prefix}: player {} task {:?}", server.player, t.kind);
        }
    }
}

#[test]
fn the_weekly_games_have_the_servers_task_values() {
    for prefix in ["2e31718c", "bf8049cf", "cd47553c", "5af11d91", "8f69e437", "f112aa5d"] {
        task_values_match_the_server(prefix);
    }
}
