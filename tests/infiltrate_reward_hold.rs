//! An infiltration reward's spawns and payment stop at a city reward until it is answered.
//!
//! The reward queues one spawn action per unit and then the payment. A spawned unit explores the tiles around it, and
//! revealing a map corner (the lighthouse) adds a population to the capital, which can level it up. Retail's action
//! stack stops at the pending city reward, so the spawns and the payment still queued behind it wait for the answer.
//!
//! The replay test is live game d3bb48d8 (Might, game version 123, 16 to 17 November 2026): at its last command
//! player 2's cloak infiltrates a level 5 city. The fourth dagger's sight reveals a corner and levels a city up; the
//! server has four daggers and no payment, where the engine placed a fifth dagger and paid 7 stars. The records are
//! read from `POLYTOPIA_CLIENT_GAMES`; the replay test skips without them.
use polytopia_sim::engine::replay::SaveFile;
use polytopia_sim::engine::{Command, Deferred, Engine};
use polytopia_sim::json::Json;
use polytopia_sim::WorldCoordinates as Wc;

const CLOAK: i64 = 38;
const DAGGER: i32 = 37;
const SIZE: i32 = 7;

fn stars(e: &Engine, player: u8) -> i32 {
    e.s.players.iter().find(|p| p.id == player).unwrap().currency
}

fn daggers(e: &Engine) -> usize {
    e.s.units.iter().filter(|u| u.owner == 1 && u.type_idx == DAGGER).count()
}

/// A 7x7 field. Player 1's capital (0,0) is one population short of level 2. Player 2's level 2 capital stands on
/// (5,5), beside the corner (6,6), which player 1 has not explored; player 1's cloak waits on (4,5).
fn board() -> Engine {
    let mut tiles = Vec::new();
    for y in 0..SIZE {
        for x in 0..SIZE {
            let explored = if (x, y) == (SIZE - 1, SIZE - 1) { "[2]" } else { "[1,2]" };
            let mut t = format!(r#""x":{x},"y":{y},"terrain":3,"climate":2,"explored":{explored}"#);
            if (x, y) == (0, 0) {
                t.push_str(r#","city":true,"capital":true,"owner":1,"ruling":[0,0]"#);
            } else if (x - 5).abs() <= 1 && (y - 5).abs() <= 1 {
                t.push_str(r#","owner":2,"ruling":[5,5]"#);
                if (x, y) == (5, 5) {
                    t.push_str(r#","city":true,"capital":true"#);
                }
            }
            tiles.push(format!("{{{t}}}"));
        }
    }
    let text = format!(
        r#"{{"format":"polytopia-position","game":{{"turn":6,"width":{SIZE},"height":{SIZE},"current_player":1}},
           "players":[{{"id":1,"tribe":2,"human":true,"stars":5,"known":[2]}},{{"id":2,"tribe":3,"stars":5,"known":[1]}}],
           "tiles":[{}],
           "improvements":[{{"x":0,"y":0,"level":1,"xp":1}},{{"x":5,"y":5,"level":2,"border_size":1}}],
           "units":[{{"id":1,"type":{CLOAK},"owner":1,"x":4,"y":5}}]}}"#,
        tiles.join(",")
    );
    Engine::from_position(&Json::parse(&text).unwrap()).unwrap()
}

#[test]
fn a_spawn_that_raises_a_city_reward_holds_the_rest_of_the_infiltration_reward() {
    let mut e = board();
    let before = stars(&e, 1);
    e.apply_unchecked(Command::Attack { unit: 1, target: Wc { x: 5, y: 5 } });
    assert!(e.reward_pending(), "the corner's population levelled player 1's capital up");
    assert_eq!(daggers(&e), 1, "the second spawn waits for the reward");
    assert_eq!(stars(&e, 1), before, "so does the payment");

    let reward =
        e.legal_commands().into_iter().find(|c| matches!(c, Command::CityReward { .. })).expect("a reward to answer");
    e.apply(reward).unwrap();
    assert!(!e.reward_pending());
    assert_eq!(daggers(&e), 2, "the second spawn runs once the reward is answered");
    assert!(stars(&e, 1) > before, "and the payment is made");
}

#[test]
fn a_held_infiltration_reward_survives_a_position_round_trip() {
    let mut e = board();
    e.apply_unchecked(Command::Attack { unit: 1, target: Wc { x: 5, y: 5 } });
    let mut copy = Engine::from_position(&e.to_position()).unwrap();
    assert!(e.deferred.iter().any(|d| matches!(d, Deferred::Infiltrate(_))), "the rest of the reward is held");
    assert_eq!(copy.deferred, e.deferred);
    let reward = copy.legal_commands().into_iter().find(|c| matches!(c, Command::CityReward { .. })).unwrap();
    copy.apply(reward).unwrap();
    assert_eq!(daggers(&copy), 2);
}

fn games_dir() -> String {
    std::env::var("POLYTOPIA_CLIENT_GAMES").unwrap_or_default()
}

#[test]
fn replay_of_d3bb48d8_has_four_daggers_and_no_payment_like_the_server() {
    let dir = games_dir();
    let Some(init_path) = std::fs::read_dir(&dir).ok().and_then(|d| {
        d.filter_map(|e| e.ok().map(|e| e.path())).find(|p| {
            p.file_name().is_some_and(|n| {
                n.to_string_lossy().starts_with("d3bb48d8") && n.to_string_lossy().ends_with("_init.bin")
            })
        })
    }) else {
        eprintln!("skipped: record d3bb48d8 absent in {dir}");
        return;
    };
    let cur_path = init_path.to_string_lossy().replace("_init.bin", "_cur.bin");
    let (Ok(init), Ok(cur)) = (std::fs::read(&init_path), std::fs::read(&cur_path)) else {
        eprintln!("skipped: record d3bb48d8 incomplete in {dir}");
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
    assert_eq!(stars(&e, 2), stars_of(&save.current, 2), "player 2's stars");
    let units = |s: &polytopia_sim::GameState| {
        s.units
            .iter()
            .filter(|u| u.owner == 2 && u.type_idx == DAGGER)
            .map(|u| u.id)
            .collect::<std::collections::BTreeSet<_>>()
    };
    assert_eq!(units(&e.s), units(&save.current), "player 2's daggers");
}

fn stars_of(s: &polytopia_sim::GameState, player: u8) -> i32 {
    s.players.iter().find(|p| p.id == player).unwrap().currency
}
