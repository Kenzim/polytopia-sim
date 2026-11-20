//! Wiping a player out. The game's wipe action records, on the wiped player, the turn, the wiper's id and the index of
//! the command that did it (the last entry of the command stack, so one less than the number of commands run so far),
//! and counts one more wipe-out on the wiper. Retail evidence: all 14 wipes in the regression saves (8) and the weekly
//! replays of weeks 10-19 and 10-26 (6), for example weekly game 5af11d91 (player 1 wiped player 2 in turn 6,
//! at command 250). The replay tests skip without the files.
use polytopia_sim::engine::replay::SaveFile;
use polytopia_sim::engine::{Command, Engine};
use polytopia_sim::json::Json;
use polytopia_sim::WorldCoordinates as Wc;

const C1: Wc = Wc { x: 1, y: 1 };
const C2: Wc = Wc { x: 8, y: 1 };
const C3: Wc = Wc { x: 4, y: 8 };

/// Player 1 to move in turn 5, one city each and a warrior of player 1 on each enemy city, ready to capture it.
fn board() -> Engine {
    let cities = [(C1, 1u8), (C2, 2), (C3, 3)];
    let mut tiles = Vec::new();
    for y in 0..10 {
        for x in 0..10 {
            let mut t = format!(r#""x":{x},"y":{y},"terrain":3,"climate":2"#);
            if let Some((_, owner)) = cities.iter().find(|(c, _)| c.x == x && c.y == y) {
                t.push_str(&format!(r#","city":true,"capital":true,"owner":{owner}"#));
            }
            tiles.push(format!("{{{t}}}"));
        }
    }
    let text = format!(
        r#"{{"format":"polytopia-position","game":{{"turn":5,"width":10,"height":10,"current_player":1}},
           "players":[{{"id":1,"tribe":2,"human":true}},{{"id":2,"tribe":3}},{{"id":3,"tribe":4}}],
           "tiles":[{}],
           "units":[{{"type":2,"owner":1,"x":{},"y":{}}},{{"type":2,"owner":1,"x":{},"y":{}}}]}}"#,
        tiles.join(","),
        C2.x,
        C2.y,
        C3.x,
        C3.y
    );
    Engine::from_position(&Json::parse(&text).unwrap()).unwrap()
}

/// Capture the city at `at` with the warrior standing there; returns the command counter before the command.
fn capture(e: &mut Engine, at: Wc) -> i32 {
    let unit = e.s.units.iter().find(|u| u.coordinates == at && u.owner == 1).unwrap().id;
    let before = i32::from(e.s.current_command);
    e.apply(Command::Capture { unit, at }).expect("capture is legal");
    before
}

/// (wiped at command, wipe-outs, killer, turn of the wipe) of a player.
fn wipe_fields(e: &Engine, id: u8) -> (i32, u32, u8, u32) {
    let r = e.player_runtime(id).unwrap();
    (r.wiped_at_command, r.wipe_outs, r.killer_id, r.killed_turn)
}

#[test]
fn a_wipe_records_the_wiper_the_turn_and_the_command() {
    let mut e = board();
    for id in 1..=3 {
        assert_eq!(wipe_fields(&e, id), (-1, 0, 0, 0), "nobody is wiped yet");
    }

    let first = capture(&mut e, C2);
    assert!(!e.is_alive(2));
    assert_eq!(
        wipe_fields(&e, 2),
        (first, 0, 1, 5),
        "wiped player: the command's index, no wipe-outs, wiper 1, turn 5"
    );
    assert_eq!(wipe_fields(&e, 1).1, 1, "the wiper counts a wipe-out");
    assert_eq!(wipe_fields(&e, 3), (-1, 0, 0, 0), "player 3 is untouched");

    let second = capture(&mut e, C3);
    assert!(second > first);
    assert_eq!(wipe_fields(&e, 3), (second, 0, 1, 5));
    assert_eq!(wipe_fields(&e, 2), (first, 0, 1, 5), "a later command leaves the first wipe as it was");
    assert_eq!(wipe_fields(&e, 1), (-1, 2, 0, 0), "the wiper has two wipe-outs and is not wiped itself");
}

/// Replays a game's command stack and compares the wipe fields of every player with the game's final state; returns
/// the number of wiped players, None when the files are unreadable.
fn replay_wipes(init: &str, cur: &str) -> Option<usize> {
    let save = SaveFile::open(&format!("{init}+{cur}")).ok()?;
    let mut e = Engine::from_state(save.initial.clone());
    if save.commands.iter().any(|c| c.kind == 20) {
        e.start_match();
    }
    for c in &save.commands {
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
    let mut wipes = 0;
    for theirs in &save.current.player_runtime {
        let ours = e.player_runtime(theirs.player).expect("player in the engine");
        assert_eq!(
            (ours.wiped_at_command, ours.wipe_outs, ours.killer_id, ours.killed_turn),
            (theirs.wiped_at_command, theirs.wipe_outs, theirs.killer_id, theirs.killed_turn),
            "{cur}: player {} (wiped at command, wipe-outs, killer, turn)",
            theirs.player
        );
        wipes += usize::from(theirs.wiped_at_command >= 0);
    }
    Some(wipes)
}

fn pairs_in(dir: &str) -> Vec<(String, String)> {
    let Ok(rd) = std::fs::read_dir(dir) else { return Vec::new() };
    let mut v: Vec<(String, String)> = rd
        .filter_map(|d| d.ok())
        .filter_map(|d| d.file_name().to_string_lossy().strip_suffix("_init.bin").map(|s| s.to_string()))
        .map(|s| (format!("{dir}/{s}_init.bin"), format!("{dir}/{s}_cur.bin")))
        .collect();
    v.sort();
    v
}

/// The six weekly games of weeks 10-19 and 10-26 (`POLYTOPIA_WEEKLY_RAW`).
#[test]
fn weekly_replays_have_the_servers_wipe_fields() {
    let dir = std::env::var("POLYTOPIA_WEEKLY_RAW").unwrap_or_default();
    let games = pairs_in(&dir);
    if games.is_empty() {
        eprintln!("skipped: no weekly replays in {dir}");
        return;
    }
    let wipes: usize = games.iter().filter_map(|(i, c)| replay_wipes(i, c)).sum();
    assert!(wipes > 0, "no wipe in the weekly replays");
}

/// The regression games, by the same check (`POLYTOPIA_REGRESS`).
#[test]
fn regression_replays_have_the_servers_wipe_fields() {
    let dir = std::env::var("POLYTOPIA_REGRESS").unwrap_or_default();
    let games = pairs_in(&dir);
    if games.is_empty() {
        eprintln!("skipped: no regression data in {dir}");
        return;
    }
    let wipes: usize = games.iter().filter_map(|(i, c)| replay_wipes(i, c)).sum();
    assert!(wipes > 0, "no wipe in the regression replays");
}
