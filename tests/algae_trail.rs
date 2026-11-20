//! The Algae trail of a unit with the Algae ability (the Cymanti island).
//!
//! Every water tile of the path of a move command, the tile the unit leaves and the tile it reaches included,
//! that has no Algae yet gets the effect, as a free build of the algae spawn improvement for the unit's owner.
//! Land tiles on the path get nothing, nor do tiles that already have Algae, and units without the ability leave
//! no trail. A push does not count as a move command and leaves no trail.
//!
//! Evidence from the game: weekly game 2e31718c, command 18 (turn 0): the Cymanti island 50 moved from (3,2) to
//! (2,3) and the server had Algae on both tiles. Replaying the three Cymanti weekly games of week 2026-10-26
//! with the trail, the Algae tiles at the end of each game equal the server's.
use polytopia_sim::engine::replay::SaveFile;
use polytopia_sim::engine::{Command, Engine};
use polytopia_sim::json::Json;
use polytopia_sim::WorldCoordinates as Wc;

const ALGAE: u16 = 1 << 4;
const ISLAND: i64 = 57;
const SHARK: i64 = 53;

fn wc(x: i32, y: i32) -> Wc {
    Wc { x, y }
}

/// Player 1 (Cymanti, to move) and player 2 on a 7x3 board: row 1 is water except (4,1), which is field.
fn board(units: &str, algae_on: &[(i32, i32)]) -> Engine {
    let mut tiles = Vec::new();
    for y in 0..3 {
        for x in 0..7 {
            let water = y == 1 && x != 4;
            let mut t = format!(r#""x":{x},"y":{y},"terrain":{},"climate":2"#, if water { 1 } else { 3 });
            match (x, y) {
                (0, 0) => t.push_str(r#","city":true,"capital":true,"owner":1,"ruling":[0,0]"#),
                (6, 2) => t.push_str(r#","city":true,"capital":true,"owner":2,"ruling":[6,2]"#),
                _ => {}
            }
            if algae_on.contains(&(x, y)) {
                t.push_str(r#","effects":[4]"#);
            }
            tiles.push(format!("{{{t}}}"));
        }
    }
    let text = format!(
        r#"{{"format":"polytopia-position","game":{{"turn":6,"width":7,"height":3,"current_player":1}},
           "players":[{{"id":1,"tribe":17,"human":true,"stars":5}},{{"id":2,"tribe":3,"stars":5}}],
           "tiles":[{}],
           "improvements":[{{"x":0,"y":0,"level":1}},{{"x":6,"y":2,"level":1}}],
           "units":[{units}]}}"#,
        tiles.join(",")
    );
    Engine::from_position(&Json::parse(&text).unwrap()).unwrap()
}

fn algae_tiles(e: &Engine) -> Vec<(i32, i32)> {
    e.s.map.tiles.iter().filter(|t| t.effects & ALGAE != 0).map(|t| (t.coordinates.x, t.coordinates.y)).collect()
}

#[test]
fn an_island_leaves_algae_on_every_water_tile_of_its_path() {
    let mut e = board(&format!(r#"{{"id":1,"type":{ISLAND},"owner":1,"x":1,"y":1,"health":200}}"#), &[]);
    e.apply(Command::Move { unit: 1, to: wc(3, 1) }).expect("the move is legal");
    assert_eq!(e.unit(1).unwrap().coordinates, wc(3, 1));
    assert_eq!(
        algae_tiles(&e),
        vec![(1, 1), (2, 1), (3, 1)],
        "the tile it left, the one between and the one it reached"
    );
}

#[test]
fn the_trail_skips_tiles_that_already_have_algae_and_land() {
    let mut e = board(&format!(r#"{{"id":1,"type":{ISLAND},"owner":1,"x":1,"y":1,"health":200}}"#), &[(2, 1)]);
    e.apply(Command::Move { unit: 1, to: wc(3, 1) }).expect("the move is legal");
    assert_eq!(algae_tiles(&e), vec![(1, 1), (2, 1), (3, 1)]);
    assert!(e.s.map.tiles.iter().filter(|t| t.terrain == 3).all(|t| t.effects & ALGAE == 0), "land gets none");
}

#[test]
fn a_unit_without_the_ability_leaves_no_trail() {
    let mut e = board(&format!(r#"{{"id":1,"type":{SHARK},"owner":1,"x":1,"y":1,"health":100}}"#), &[]);
    e.apply(Command::Move { unit: 1, to: wc(3, 1) }).expect("the move is legal");
    assert!(algae_tiles(&e).is_empty());
}

/// The step into the tile of a unit the island has killed is a move, so it leaves the same trail (corpus game
/// 2704b9f5: the server had Algae on the port tile the island stepped into).
#[test]
fn an_island_that_kills_and_steps_in_leaves_algae_on_both_tiles() {
    let units = format!(
        r#"{{"id":1,"type":{ISLAND},"owner":1,"x":1,"y":1,"health":200}},{{"id":2,"type":{SHARK},"owner":2,"x":2,"y":1,"health":1}}"#
    );
    let mut e = board(&units, &[]);
    e.apply(Command::Attack { unit: 1, target: wc(2, 1) }).expect("the attack is legal");
    assert!(e.unit(2).is_none(), "the shark died");
    assert_eq!(e.unit(1).unwrap().coordinates, wc(2, 1), "the island stepped in");
    assert_eq!(algae_tiles(&e), vec![(1, 1), (2, 1)]);
}

/// Replays the Cymanti games of the weekly challenge of 2026-10-26 and compares the Algae tiles at the end with
/// the server's. Skips when the private replay files are absent.
#[test]
fn replayed_weekly_games_end_with_the_servers_algae_tiles() {
    let dir = std::env::var("POLYTOPIA_WEEKLY_RAW").unwrap_or_default();
    let Ok(files) = std::fs::read_dir(&dir) else {
        eprintln!("skipped: no weekly replays in {dir}");
        return;
    };
    let mut names: Vec<String> = files.flatten().map(|f| f.file_name().to_string_lossy().into_owned()).collect();
    names.sort();
    let mut compared = 0;
    for stem in ["2e31718c", "bf8049cf", "cd47553c"] {
        let Some(init) = names.iter().find(|n| n.starts_with(stem) && n.ends_with("_init.bin")) else { continue };
        let cur = init.replace("_init.bin", "_cur.bin");
        let save = SaveFile::open(&format!("{dir}/{init}+{dir}/{cur}")).expect("open the replay");
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
            e.apply_unchecked(cmd);
        }
        assert_eq!(algae_tiles(&e), algae_tiles_of(&save.current), "{stem}: Algae tiles at the end");
        compared += 1;
    }
    eprintln!("{compared} games compared");
}

fn algae_tiles_of(s: &polytopia_sim::GameState) -> Vec<(i32, i32)> {
    s.map.tiles.iter().filter(|t| t.effects & ALGAE != 0).map(|t| (t.coordinates.x, t.coordinates.y)).collect()
}
