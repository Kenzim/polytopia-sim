//! A land unit embarks when it steps onto a port, wherever it stands.
//!
//! Whether a unit embarks depends on the unit and the tile it steps onto (a port it may use), not on the tile it
//! leaves. A land unit standing on an Algae tile (water that land units may enter) therefore embarks stepping
//! onto a port, like one stepping off dry land.
//!
//! Evidence from the game: weekly game bf8049cf, command 582 (turn 14): warrior 51 moved from (8,11) to (8,12), and
//! game 2e31718c, command 585: swordsman 65 moved from (6,9) to (5,9). Both started on an Algae tile and ended on a
//! port, and the next turn the server upgraded them to scout ships, which needs a boat.
use polytopia_sim::engine::replay::SaveFile;
use polytopia_sim::engine::{Command, Engine};
use polytopia_sim::json::Json;
use polytopia_sim::WorldCoordinates as Wc;

const WARRIOR: i64 = 2;
const TRANSPORT_SHIP: i64 = 43;
const PORT: i32 = 8;

fn wc(x: i32, y: i32) -> Wc {
    Wc { x, y }
}

/// Player 1 to move on a 6x3 board. Row 1 is water with a port on (3,1) owned by player 1; (2,1) has Algae when
/// `algae` is set and is a field otherwise.
fn board(algae: bool) -> Engine {
    let mut tiles = Vec::new();
    for y in 0..3 {
        for x in 0..6 {
            let water = y == 1 && (x >= 3 || algae && x == 2);
            let mut t = format!(r#""x":{x},"y":{y},"terrain":{},"climate":2"#, if water { 1 } else { 3 });
            match (x, y) {
                (0, 0) => t.push_str(r#","city":true,"capital":true,"owner":1,"ruling":[0,0]"#),
                (5, 2) => t.push_str(r#","city":true,"capital":true,"owner":2,"ruling":[5,2]"#),
                (3, 1) => t.push_str(&format!(r#","improvement":{PORT},"owner":1"#)),
                (2, 1) if algae => t.push_str(r#","effects":[4],"owner":1"#),
                _ => {}
            }
            tiles.push(format!("{{{t}}}"));
        }
    }
    let text = format!(
        r#"{{"format":"polytopia-position","game":{{"turn":6,"width":6,"height":3,"current_player":1}},
           "players":[{{"id":1,"tribe":2,"human":true,"stars":5}},{{"id":2,"tribe":3,"stars":5}}],
           "tiles":[{}],
           "improvements":[{{"x":0,"y":0,"level":1}},{{"x":5,"y":2,"level":1}}],
           "units":[{{"id":1,"type":{WARRIOR},"owner":1,"x":2,"y":1,"health":100}}]}}"#,
        tiles.join(",")
    );
    let mut e = Engine::from_position(&Json::parse(&text).unwrap()).unwrap();
    let fishing = (0..200).find(|i| e.t.tech(*i).is_some_and(|t| t.id == "fishing")).expect("the fishing tech");
    e.s.tech.iter_mut().find(|t| t.player == 1).unwrap().tech.push(fishing);
    e
}

fn boat_of(e: &Engine, passenger: u32) -> Option<u32> {
    e.s.units.iter().find(|u| u.passenger.as_ref().is_some_and(|p| p.id == passenger)).map(|u| u.id)
}

#[test]
fn a_land_unit_steps_off_dry_land_onto_a_port_and_embarks() {
    let mut e = board(false);
    assert_eq!(e.will_embark(e.unit(1).unwrap(), wc(3, 1)), 1);
    e.apply(Command::Move { unit: 1, to: wc(3, 1) }).expect("the move is legal");
    let boat = boat_of(&e, 1).expect("the warrior is carried by a boat");
    assert_eq!(e.unit(boat).unwrap().coordinates, wc(3, 1));
}

#[test]
fn a_land_unit_steps_off_an_algae_tile_onto_a_port_and_embarks() {
    let mut e = board(true);
    assert_eq!(e.will_embark(e.unit(1).unwrap(), wc(3, 1)), 1, "the tile it stands on does not matter");
    e.apply(Command::Move { unit: 1, to: wc(3, 1) }).expect("the move is legal");
    let boat = boat_of(&e, 1).expect("the warrior is carried by a boat");
    assert_eq!(e.unit(boat).unwrap().coordinates, wc(3, 1));
    assert_eq!(e.unit(boat).unwrap().created_turn, 6);
}

/// A transport ship on a 5x3 board: water on row 1 up to x = 2, then a field (3,1) that is flooded when `flooded` is set.
fn ship_board(flooded: bool) -> Engine {
    let mut tiles = Vec::new();
    for y in 0..3 {
        for x in 0..5 {
            let water = y == 1 && x <= 2;
            let mut t = format!(r#""x":{x},"y":{y},"terrain":{},"climate":2"#, if water { 1 } else { 3 });
            match (x, y) {
                (0, 0) => t.push_str(r#","city":true,"capital":true,"owner":1,"ruling":[0,0]"#),
                (4, 2) => t.push_str(r#","city":true,"capital":true,"owner":2,"ruling":[4,2]"#),
                (3, 1) if flooded => t.push_str(r#","effects":[1]"#),
                _ => {}
            }
            tiles.push(format!("{{{t}}}"));
        }
    }
    let text = format!(
        r#"{{"format":"polytopia-position","game":{{"turn":6,"width":5,"height":3,"current_player":1}},
           "players":[{{"id":1,"tribe":2,"human":true,"stars":5}},{{"id":2,"tribe":3,"stars":5}}],
           "tiles":[{}],
           "improvements":[{{"x":0,"y":0,"level":1}},{{"x":4,"y":2,"level":1}}],
           "units":[{{"id":1,"type":{TRANSPORT_SHIP},"owner":1,"x":2,"y":1,"health":100}}]}}"#,
        tiles.join(",")
    );
    Engine::from_position(&Json::parse(&text).unwrap()).unwrap()
}

#[test]
fn a_transport_ship_lands_on_a_field_but_not_on_flooded_wetland() {
    let dry = ship_board(false);
    assert_eq!(dry.will_embark(dry.unit(1).unwrap(), wc(3, 1)), 2, "a field is a landing");
    let wet = ship_board(true);
    assert_eq!(wet.will_embark(wet.unit(1).unwrap(), wc(3, 1)), 0, "a flooded field is wetland, not a landing");
}

/// Replays the Cymanti games of the weekly challenge of 2026-10-26 and checks that both units end up as the
/// server has them: carried by a boat after their move. Skips when the private replay files are absent.
#[test]
fn replayed_weekly_games_embark_the_units_that_step_from_algae_onto_a_port() {
    let dir = std::env::var("POLYTOPIA_WEEKLY_RAW").unwrap_or_default();
    let Ok(files) = std::fs::read_dir(&dir) else {
        eprintln!("skipped: no weekly replays in {dir}");
        return;
    };
    let names: Vec<String> = files.flatten().map(|f| f.file_name().to_string_lossy().into_owned()).collect();
    let mut checked = 0;
    for (stem, command, unit) in [("bf8049cf", 582usize, 51u32), ("2e31718c", 585, 65)] {
        let Some(init) = names.iter().find(|n| n.starts_with(stem) && n.ends_with("_init.bin")) else { continue };
        let cur = init.replace("_init.bin", "_cur.bin");
        let save = SaveFile::open(&format!("{dir}/{init}+{dir}/{cur}")).expect("open the replay");
        let mut e = Engine::from_state(save.initial.clone());
        for (i, c) in save.commands.iter().enumerate() {
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
            if i == command {
                assert!(
                    boat_of(&e, unit).is_some(),
                    "{stem}: unit {unit} is carried by a boat after command {command}"
                );
                checked += 1;
                break;
            }
        }
    }
    eprintln!("{checked} games checked");
}

/// Retail's embark test asks whether the target tile is water or wetland, so a port on land under the Flooded effect
/// (Aquarion) takes a land unit aboard like a port on water. Corpus: games 1e8583f3 and f66e39e7, where the server
/// accepts a Disembark for a giant that stepped onto such a port.
#[test]
fn a_land_unit_steps_onto_a_port_on_flooded_land_and_embarks() {
    let mut e = board(false);
    let t = e.s.map.tiles.iter_mut().find(|t| t.coordinates == wc(3, 1)).unwrap();
    t.terrain = 3;
    t.effects |= 1 << 1;
    assert_eq!(e.will_embark(e.unit(1).unwrap(), wc(3, 1)), 1);
    e.apply(Command::Move { unit: 1, to: wc(3, 1) }).expect("the move is legal");
    let boat = boat_of(&e, 1).expect("the warrior is carried by a boat");
    assert!(e.legal_commands().contains(&Command::Disembark { unit: boat }));
}
