//! When a road joins several cities to the capital at once, retail queues one connection per city, in city order, on
//! the game's action stack. The stack runs last in, first out, so the last city's connection runs first. Order
//! matters because the first level-up raises a reward and holds the rest of the command's actions behind it. Within
//! one connection the city's population runs before the capital's (retail queues the capital's, then the city's).
//!
//! The constructed test uses a hand-made board. The replay test reads weekly game 2e31718c (week 2026-10-26), where
//! command 644 answers a city reward in a city that only levels up when the connections run in this order; it reads
//! `POLYTOPIA_WEEKLY_RAW`, and skips without the file.
use polytopia_sim::engine::replay::SaveFile;
use polytopia_sim::engine::{Command, Deferred, Engine};
use polytopia_sim::json::Json;
use polytopia_sim::WorldCoordinates as Wc;

/// Player 1 owns a capital at 3,0 and cities at 0,0 and 6,0, with roads from the capital to within one tile of each.
/// Both outer cities have one xp of the two they need, so a connection raises a level-up; the capital has `capital_xp`.
/// `roads` lists the tiles of row 0 that already carry a road.
fn board(capital_xp: i16, roads: &[(i32, i32)]) -> Engine {
    let mut tiles = Vec::new();
    for y in 0..3 {
        for x in 0..7 {
            let mut t = format!(r#""x":{x},"y":{y},"terrain":3,"climate":2"#);
            match (x, y) {
                (3, 0) => t.push_str(r#","city":true,"capital":true,"owner":1,"ruling":[3,0]"#),
                (0, 0) => t.push_str(r#","city":true,"owner":1,"ruling":[0,0]"#),
                (6, 0) => t.push_str(r#","city":true,"owner":1,"ruling":[6,0]"#),
                (0, 2) => {
                    t = r#""x":0,"y":2,"terrain":3,"climate":2,"city":true,"capital":true,"owner":2,"ruling":[0,2]"#
                        .to_string()
                }
                _ => {}
            }
            if (y, roads.contains(&(x, y))) == (0, true) {
                t.push_str(r#","owner":1,"ruling":[3,0],"road":true"#);
            }
            tiles.push(format!("{{{t}}}"));
        }
    }
    let text = format!(
        r#"{{"format":"polytopia-position","game":{{"turn":5,"width":7,"height":3,"current_player":1}},
           "players":[{{"id":1,"tribe":2,"human":true,"stars":20}},{{"id":2,"tribe":3}}],
           "tiles":[{}],
           "improvements":[{{"x":3,"y":0,"level":1,"xp":{capital_xp},"population":{capital_xp}}},
                           {{"x":0,"y":0,"level":1,"xp":1,"population":1}},
                           {{"x":6,"y":0,"level":1,"xp":1,"population":1}}]}}"#,
        tiles.join(",")
    );
    Engine::from_position(&Json::parse(&text).unwrap()).unwrap()
}

fn triggers(e: &Engine) -> Vec<(i32, i32)> {
    e.triggers.iter().map(|t| (t.city.x, t.city.y)).collect()
}

/// The held population changes in the order they will run (the stack pops from the end).
fn held_population(e: &Engine) -> Vec<(i32, i32)> {
    e.deferred
        .iter()
        .rev()
        .filter_map(|d| if let Deferred::Population(c, _) = d { Some((c.x, c.y)) } else { None })
        .collect()
}

/// Two cities join at once: the last city's connection runs first, so its reward comes first.
#[test]
fn the_last_city_in_the_list_is_connected_first() {
    // Roads reach both outer cities from the capital, except for the tile at 1,0 that joins the west city. The east
    // city's road is complete already, but nothing has refreshed the connections yet, so building 1,0 joins both.
    let mut e = board(0, &[(2, 0), (4, 0), (5, 0)]);
    e.apply_unchecked(Command::Build { at: Wc { x: 1, y: 0 }, improvement: 3 });
    assert_eq!(triggers(&e), [(6, 0)], "the east city, last in the list, is connected first");
    // Behind the east city's reward wait the east connection's capital point, then the west connection's city point
    // and capital point.
    assert_eq!(held_population(&e), [(3, 0), (0, 0), (3, 0)]);
}

/// Weekly game 2e31718c (week 2026-10-26): before command 644 the engine had the city at (11,14) one population too
/// high and refused the Park reward the server accepted, because several connections ran in city order. With the
/// stack order (last city first) the command is legal.
#[test]
fn the_weekly_game_accepts_the_city_reward_at_command_644() {
    let dir = std::env::var("POLYTOPIA_WEEKLY_RAW").unwrap_or_default();
    let Some(stem) = std::fs::read_dir(&dir)
        .ok()
        .and_then(|d| {
            d.filter_map(|d| d.ok())
                .map(|d| d.file_name().to_string_lossy().into_owned())
                .find(|n| n.starts_with("2e31718c") && n.ends_with("_init.bin"))
        })
        .and_then(|n| n.strip_suffix("_init.bin").map(str::to_string))
    else {
        eprintln!("skipped: no weekly replay files");
        return;
    };
    let save = SaveFile::open(&format!("{dir}/{stem}_init.bin+{dir}/{stem}_cur.bin")).unwrap();
    let mut e = Engine::from_state(save.initial.clone());
    if save.commands.iter().any(|c| c.kind == 20) {
        e.start_match();
    }
    for (i, c) in save.commands.iter().enumerate().take(645) {
        if c.kind == 20 || c.player == 255 {
            continue;
        }
        let Some(cmd) = e.command_from_raw(c) else {
            e.s.current_command += 1;
            continue;
        };
        if i == 644 {
            assert!(matches!(cmd, Command::CityReward { .. }), "command 644 is a city reward: {cmd:?}");
            assert!(e.legal_commands().contains(&cmd), "the server's city reward is refused: {cmd:?}");
        }
        e.apply_unchecked(cmd);
    }
}
