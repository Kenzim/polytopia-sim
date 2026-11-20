//! The occupant of a city that a reward trains onto steps aside to the first neighbour, in retail's order, that it
//! can reach with a path of cost one; there is no terrain pre-check, so a land unit may take a bridge over water.
//!
//! Live game c23fbea6 (Might, game version 123): player 2 took the giant reward in a city whose rider faced the
//! north-west. The tile there is shallow water with a bridge; the server pushed the rider onto it, the engine skipped it
//! (water is not a terrain a rider walks on) and used the next direction, a mountain. The record comes from
//! `POLYTOPIA_CLIENT_GAMES`; the replay test skips without it.
use polytopia_sim::engine::replay::SaveFile;
use polytopia_sim::engine::{Command, Engine};
use polytopia_sim::json::Json;
use polytopia_sim::{CityReward, WorldCoordinates as Wc};

const RIDER: i64 = 3;
const BRIDGE: i64 = 48;
const GIANT: i32 = 12;

fn wc(x: i32, y: i32) -> Wc {
    Wc { x, y }
}

/// A 5x5 field. Player 1's city (2,2) holds a rider facing the north-west; (1,1) is water with a bridge when
/// `bridge` is set and a field otherwise.
fn board(bridge: bool) -> Engine {
    let mut tiles = Vec::new();
    for y in 0..5 {
        for x in 0..5 {
            let water = bridge && (x, y) == (1, 1);
            let mut t =
                format!(r#""x":{x},"y":{y},"terrain":{},"climate":2,"explored":[1,2]"#, if water { 1 } else { 3 });
            match (x, y) {
                (2, 2) => t.push_str(r#","city":true,"capital":true,"owner":1,"ruling":[2,2]"#),
                (4, 4) => t.push_str(r#","city":true,"capital":true,"owner":2,"ruling":[4,4]"#),
                (1, 1) if bridge => t.push_str(&format!(r#","improvement":{BRIDGE},"owner":1"#)),
                _ => {}
            }
            tiles.push(format!("{{{t}}}"));
        }
    }
    let text = format!(
        r#"{{"format":"polytopia-position","game":{{"turn":6,"width":5,"height":5,"current_player":1}},
           "players":[{{"id":1,"tribe":2,"human":true,"stars":5}},{{"id":2,"tribe":3,"stars":5}}],
           "tiles":[{}],
           "improvements":[{{"x":2,"y":2,"level":1}},{{"x":4,"y":4,"level":1}}],
           "units":[{{"id":1,"type":{RIDER},"owner":1,"x":2,"y":2,"direction":0}}]}}"#,
        tiles.join(",")
    );
    Engine::from_position(&Json::parse(&text).unwrap()).unwrap()
}

fn reward(e: &mut Engine) {
    e.apply_unchecked(Command::CityReward { city: wc(2, 2), reward: CityReward::SuperUnit });
}

#[test]
fn the_occupant_steps_onto_a_bridge_in_its_first_direction() {
    let mut e = board(true);
    reward(&mut e);
    assert_eq!(e.unit(1).unwrap().coordinates, wc(1, 1), "it took the bridge, not the tile after it");
    assert!(e.s.units.iter().any(|u| u.type_idx == GIANT && u.coordinates == wc(2, 2)), "the giant took the city");
}

#[test]
fn on_dry_land_the_occupant_still_takes_its_first_direction() {
    let mut e = board(false);
    reward(&mut e);
    assert_eq!(e.unit(1).unwrap().coordinates, wc(1, 1));
}

fn games_dir() -> String {
    std::env::var("POLYTOPIA_CLIENT_GAMES").unwrap_or_default()
}

#[test]
fn replay_of_c23fbea6_pushes_the_rider_like_the_server() {
    let dir = games_dir();
    let Some(init_path) = std::fs::read_dir(&dir).ok().and_then(|d| {
        d.filter_map(|e| e.ok().map(|e| e.path())).find(|p| {
            p.file_name().is_some_and(|n| {
                n.to_string_lossy().starts_with("c23fbea6") && n.to_string_lossy().ends_with("_init.bin")
            })
        })
    }) else {
        eprintln!("skipped: record c23fbea6 absent in {dir}");
        return;
    };
    let cur_path = init_path.to_string_lossy().replace("_init.bin", "_cur.bin");
    let (Ok(init), Ok(cur)) = (std::fs::read(&init_path), std::fs::read(&cur_path)) else {
        eprintln!("skipped: record c23fbea6 incomplete in {dir}");
        return;
    };
    let save = SaveFile::parse_server(&init, &cur).unwrap();
    let mut e = Engine::from_state(save.initial.clone());
    if save.commands.iter().any(|c| c.kind == 20) {
        e.start_match();
    }
    for c in &save.commands {
        if c.kind == 20 || c.player == 255 {
            continue;
        }
        match e.command_from_raw(c) {
            Some(cmd) => e.apply_unchecked(cmd),
            None => e.s.current_command += 1,
        }
    }
    for theirs in &save.current.units {
        let ours =
            e.s.units.iter().find(|u| u.id == theirs.id).unwrap_or_else(|| panic!("unit {} is missing", theirs.id));
        assert_eq!(
            (ours.coordinates, ours.type_idx),
            (theirs.coordinates, theirs.type_idx),
            "unit {} (tile, type)",
            theirs.id
        );
    }
}
