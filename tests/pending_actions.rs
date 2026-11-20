//! A command's actions run as a queue that stops at the first city level-up: the reward must be answered before the
//! rest of the command's actions run. A port that connects a new city to the capital adds a population to the connected
//! city, one to the capital (which can level up) and, last, one to the port's own city; when the capital levels up, the
//! port's population waits, and is added (possibly raising a second reward) once the first reward is answered.
//!
//! `a_port_connecting_a_city_pauses_at_the_capitals_level_up` builds that on a small hand-made board. The replay test
//! reads live game 4b25d7c0 (turn 23, command 462) from `POLYTOPIA_DIVERGENCE`, and skips without it.
use polytopia_sim::engine::replay::SaveFile;
use polytopia_sim::engine::{Command, Deferred, Engine};
use polytopia_sim::json::Json;
use polytopia_sim::{CityReward, WorldCoordinates as Wc};

const CAPITAL: Wc = Wc { x: 0, y: 0 };
const CITY: Wc = Wc { x: 4, y: 0 };
const PORT: Wc = Wc { x: 3, y: 0 };

/// Player 1 owns a capital at 0,0, roads at 1,0 and 2,0 and a new city at 4,0 across the water tile 3,0. A port on
/// 3,0 joins the roads to the city, which is the first connection. The capital has `capital_xp` of the 2 it needs.
fn board(capital_xp: i16) -> Engine {
    let mut tiles = Vec::new();
    for y in 0..3 {
        for x in 0..5 {
            let mut t = format!(r#""x":{x},"y":{y},"terrain":3,"climate":2"#);
            match (x, y) {
                (0, 0) => t.push_str(r#","city":true,"capital":true,"owner":1,"ruling":[0,0]"#),
                (1, 0) | (2, 0) => t.push_str(r#","owner":1,"ruling":[0,0],"road":true"#),
                (3, 0) => t = r#""x":3,"y":0,"terrain":1,"climate":2,"owner":1,"ruling":[4,0]"#.to_string(),
                (4, 0) => t.push_str(r#","city":true,"owner":1,"ruling":[4,0]"#),
                (0, 2) => t.push_str(r#","city":true,"capital":true,"owner":2,"ruling":[0,2]"#),
                _ => {}
            }
            tiles.push(format!("{{{t}}}"));
        }
    }
    let text = format!(
        r#"{{"format":"polytopia-position","game":{{"turn":5,"width":5,"height":3,"current_player":1}},
           "players":[{{"id":1,"tribe":2,"human":true,"stars":20}},{{"id":2,"tribe":3}}],
           "tiles":[{}],
           "improvements":[{{"x":0,"y":0,"level":1,"xp":{capital_xp},"population":{capital_xp}}},
                           {{"x":4,"y":0,"level":1,"xp":0,"population":0}}]}}"#,
        tiles.join(",")
    );
    Engine::from_position(&Json::parse(&text).unwrap()).unwrap()
}

fn city(e: &Engine, at: Wc) -> (u16, i16, i16, u8) {
    let tile = e.tile(at).unwrap().index;
    let r = e.s.improvements.iter().find(|r| r.tile == tile).unwrap();
    (r.level, r.population, r.xp, r.connected_to_capital_of_player)
}

fn triggers(e: &Engine) -> Vec<(i32, i32)> {
    e.triggers.iter().map(|t| (t.city.x, t.city.y)).collect()
}

fn build_port(e: &mut Engine) {
    e.apply_unchecked(Command::Build { at: PORT, improvement: 8 });
}

#[test]
fn a_port_connecting_a_city_pauses_at_the_capitals_level_up() {
    let mut e = board(1);
    assert_eq!(city(&e, CAPITAL), (1, 1, 1, 0));
    build_port(&mut e);

    // The connection ran first (the city's point, then the capital's): the capital levelled up and its reward is
    // pending; the port's own population waits behind it.
    assert_eq!(triggers(&e), [(0, 0)], "only the capital's reward is pending");
    assert_eq!(city(&e, CAPITAL), (2, 2, 0, 0), "capital levelled up");
    assert_eq!(city(&e, CITY), (1, 1, 1, 1), "city is connected but has only the connection's population");
    assert_eq!(e.deferred.iter().filter(|d| matches!(d, Deferred::Population(..))).count(), 1);
    assert!(e.deferred.contains(&Deferred::Population(CITY, 1)));
    assert!(e.legal_commands().iter().all(|c| matches!(c, Command::CityReward { city, .. } if *city == CAPITAL)));

    // Answering the capital's reward runs the held population: the city levels up and raises its own reward.
    e.apply(Command::CityReward { city: CAPITAL, reward: CityReward::Workshop }).unwrap();
    assert_eq!(triggers(&e), [(4, 0)]);
    assert_eq!(city(&e, CITY), (2, 2, 0, 1));
    assert!(e.deferred.iter().all(|d| !matches!(d, Deferred::Population(..))));
    assert!(e.legal_commands().iter().all(|c| matches!(c, Command::CityReward { city, .. } if *city == CITY)));

    e.apply(Command::CityReward { city: CITY, reward: CityReward::Explorer }).unwrap();
    assert!(e.triggers.is_empty());
    assert!(e.deferred.is_empty());
    assert!(e.legal_commands().contains(&Command::EndTurn));
}

#[test]
fn one_level_up_holds_nothing() {
    // The capital does not level up, so every action of the command runs at once and only the city's reward is pending.
    let mut e = board(0);
    build_port(&mut e);
    assert_eq!(triggers(&e), [(4, 0)]);
    assert_eq!(city(&e, CAPITAL), (1, 1, 1, 0));
    assert_eq!(city(&e, CITY), (2, 2, 0, 1));
    assert!(e.deferred.iter().all(|d| !matches!(d, Deferred::Population(..))));
}

fn divergence_dir() -> String {
    std::env::var("POLYTOPIA_DIVERGENCE").unwrap_or_default()
}

fn replay(save: &SaveFile, stop_after: Option<usize>) -> Engine {
    let mut e = Engine::from_state(save.initial.clone());
    if save.commands.iter().any(|c| c.kind == 20) {
        e.start_match();
    }
    for (n, c) in save.commands.iter().enumerate() {
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
        if stop_after == Some(n) {
            break;
        }
    }
    e
}

#[test]
fn game_4b25d7c0_pauses_after_the_ports_first_level_up() {
    let dir = divergence_dir();
    let game = "4b25d7c0-bab7-499c-3ba2-08df30e80ec3-1792940615";
    let (Ok(init), Ok(cur)) =
        (std::fs::read(format!("{dir}/{game}_init.bin")), std::fs::read(format!("{dir}/{game}_cur.bin")))
    else {
        eprintln!("skipped {game}: blobs absent in {dir}");
        return;
    };
    let save = SaveFile::parse_server(&init, &cur).unwrap();
    assert_eq!(save.commands.len(), 463);
    let c = &save.commands[462];
    assert_eq!(
        (c.kind, c.player, c.n, c.a.x, c.a.y),
        (1, 2, 8, 2, 1),
        "command 462 is player 2 building a port at 2,1"
    );

    // Straight after the build: one reward (the capital's), the new city at population 1, as the server holds it.
    let e = replay(&save, Some(462));
    assert_eq!(triggers(&e), [(2, 6)]);
    let server: Vec<_> = SaveFile::live_triggers(&cur).unwrap().into_iter().map(|(p, _, at)| (p, at.x, at.y)).collect();
    assert_eq!(server, [(2, 2, 6)]);
    assert_eq!(city(&e, Wc { x: 2, y: 2 }), (1, 1, 1, 2), "level, population, xp, connected");
    assert!(e.deferred.contains(&Deferred::Population(Wc { x: 2, y: 2 }, 1)));
    for theirs in save.current.improvements.iter().filter(|r| r.owner != polytopia_sim::NATURE) {
        let ours = e.s.improvements.iter().find(|r| r.tile == theirs.tile).expect("the same improvements");
        assert_eq!(
            (ours.level, ours.population, ours.xp, ours.connected_to_capital_of_player),
            (theirs.level, theirs.population, theirs.xp, theirs.connected_to_capital_of_player),
            "improvement at tile {} (level, population, xp, connected)",
            theirs.tile
        );
    }
    for theirs in save.current.players.iter().filter(|p| p.id != polytopia_sim::NATURE) {
        let ours = e.s.players.iter().find(|p| p.id == theirs.id).unwrap();
        assert_eq!((ours.currency, ours.score), (theirs.currency, theirs.score), "player {} (stars, score)", theirs.id);
    }

    // Answering the capital's reward runs the held population: the new city levels up and raises its own reward.
    let mut e = e;
    e.apply_unchecked(Command::CityReward { city: Wc { x: 2, y: 6 }, reward: CityReward::Park });
    assert_eq!(triggers(&e), [(2, 2)]);
    assert_eq!(city(&e, Wc { x: 2, y: 2 }).1, 2);
    assert!(e.deferred.iter().all(|d| !matches!(d, Deferred::Population(..))));
}
