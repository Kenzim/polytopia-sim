//! The order in which a player meets the tribes it sees for the first time.
//!
//! Retail's explore walks the sight area column by column (x outer, y inner, both ascending), queues one explore action
//! per tile from the last tile to the first, and the action stack runs them in reverse, so the tiles are explored first
//! tile first. Each explore queues the meeting for the unit or city on its tile, so the tribes are met, and recorded in
//! the player's known list, in that column order.
//!
//! The check at the start of a turn queues the meetings of every explored tile in tile order and the stack runs them
//! last tile first, so a player who has seen several unknown tribes meets the one on the highest tile first.
//!
//! Within one tile retail queues the meeting with the owner of a unit and then with the owner of a city, and the stack
//! runs the city's first.
//!
//! Evidence from the game: weekly games 2e31718c and bf8049cf (week 2026-10-26). The server lists the tribes each
//! player met in the order the stack runs them; the engine listed the players of one turn-start check in tile order. The
//! six week 10-26 games now have the same known list as the server for every player.
use polytopia_sim::engine::replay::SaveFile;
use polytopia_sim::engine::{Command, Engine};
use polytopia_sim::json::Json;
use polytopia_sim::WorldCoordinates;

const BARDUR: i64 = 3;
const WARRIOR: i64 = 2;

/// Player 1 (to move) has a warrior at (1, 1) and has seen only the left column, (1, 0), (1, 1) and (2, 1); player 2's warrior stands at
/// `p2` and player 3's at `p3`, both on tiles the first move will explore. The 6x3 board is otherwise empty.
fn board(p2: (i32, i32), p3: (i32, i32), current: u8) -> Engine {
    board_with_capital(p2, p3, (5, 2), current)
}

/// [`board`] with player 3's capital at `cap3`.
fn board_with_capital(p2: (i32, i32), p3: (i32, i32), cap3: (i32, i32), current: u8) -> Engine {
    let mut tiles = Vec::new();
    for y in 0..3 {
        for x in 0..6 {
            let mut t = format!(r#""x":{x},"y":{y},"terrain":3,"climate":2"#);
            match (x, y) {
                (0, 0) => t.push_str(r#","city":true,"capital":true,"owner":1,"ruling":[0,0]"#),
                (5, 0) => t.push_str(r#","city":true,"capital":true,"owner":2,"ruling":[5,0]"#),
                _ => {}
            }
            if (x, y) == cap3 {
                t.push_str(&format!(r#","city":true,"capital":true,"owner":3,"ruling":[{x},{y}]"#));
            }
            t.push_str(if x == 0 || (x == 1 && y < 2) || (x, y) == (2, 1) {
                r#","explored":[1]"#
            } else {
                r#","explored":[]"#
            });
            tiles.push(format!("{{{t}}}"));
        }
    }
    let text = format!(
        r#"{{"format":"polytopia-position","game":{{"turn":6,"width":6,"height":3,"current_player":{current}}},
           "players":[{{"id":1,"tribe":{BARDUR},"human":true,"stars":5,"known":[]}},{{"id":2,"tribe":{BARDUR},"stars":5,"known":[]}},{{"id":3,"tribe":{BARDUR},"stars":5,"known":[]}}],
           "tiles":[{}],
           "improvements":[{{"x":0,"y":0,"level":1}},{{"x":5,"y":0,"level":1}},{{"x":{},"y":{},"level":1}}],
           "units":[{{"id":1,"type":{WARRIOR},"owner":1,"x":1,"y":1,"health":100}},
                    {{"id":2,"type":{WARRIOR},"owner":2,"x":{},"y":{},"health":100}},
                    {{"id":3,"type":{WARRIOR},"owner":3,"x":{},"y":{},"health":100}}]}}"#,
        tiles.join(","),
        cap3.0,
        cap3.1,
        p2.0,
        p2.1,
        p3.0,
        p3.1
    );
    Engine::from_position(&Json::parse(&text).unwrap()).unwrap()
}

/// The board of [`board`], but the whole map is already seen by player 1 and player 3 is to move, so ending its turn
/// starts player 1's with two unknown tribes in sight.
fn seen_board(p2: (i32, i32), p3: (i32, i32)) -> Engine {
    let mut e = board(p2, p3, 3);
    for f in &mut e.s.fog {
        f.explorers = vec![1].into();
    }
    e
}

fn known(e: &Engine, player: u8) -> Vec<u8> {
    e.s.player_runtime.iter().find(|r| r.player == player).map(|r| r.known_players.clone()).unwrap_or_default()
}

fn step_east(e: &mut Engine) {
    e.apply(Command::Move { unit: 1, to: WorldCoordinates { x: 2, y: 1 } }).expect("the move is legal");
}

#[test]
fn tiles_are_explored_column_by_column() {
    // Player 2's warrior is in the first new column but on the bottom row, player 3's in the last column on the top
    // row: column order meets player 2 first, row order would meet player 3 first.
    let mut e = board((1, 2), (3, 0), 1);
    step_east(&mut e);
    assert_eq!(known(&e, 1), vec![2, 3]);
}

#[test]
fn the_column_order_holds_the_other_way_round() {
    let mut e = board((3, 0), (1, 2), 1);
    step_east(&mut e);
    assert_eq!(known(&e, 1), vec![3, 2]);
}

#[test]
fn within_a_column_the_upper_tile_goes_first() {
    let mut e = board((3, 2), (3, 0), 1);
    step_east(&mut e);
    assert_eq!(known(&e, 1), vec![3, 2]);
}

#[test]
fn a_unit_and_a_city_on_one_tile_meet_the_city_owner_first() {
    // Player 2's warrior stands in player 3's capital: retail queues the unit's owner and then the city's, and the stack
    // runs the city's meeting first.
    let mut e = board_with_capital((3, 1), (5, 1), (3, 1), 1);
    step_east(&mut e);
    assert_eq!(known(&e, 1), vec![3, 2]);
}

#[test]
fn the_start_of_a_turn_meets_the_highest_tile_first() {
    // Player 2's warrior is on a lower tile than player 3's, so the stack runs player 3's meeting first.
    let mut e = seen_board((4, 0), (4, 2));
    e.apply(Command::EndTurn).expect("player 3 may end its turn");
    assert_eq!(known(&e, 1), vec![3, 2]);
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

/// Replays a weekly game and compares every player's known list with the server's final state; skips without the files.
fn known_lists_match_the_server(prefix: &str) {
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
        assert_eq!(known(&e, r.player), r.known_players, "known players of player {} in {prefix}", r.player);
    }
}

#[test]
fn the_weekly_games_have_the_servers_known_player_lists() {
    for prefix in ["2e31718c", "bf8049cf", "cd47553c", "5af11d91", "8f69e437", "f112aa5d"] {
        known_lists_match_the_server(prefix);
    }
}
