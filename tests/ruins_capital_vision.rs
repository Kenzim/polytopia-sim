//! A free tech from ruins reveals the capitals of the players already met, as a researched tech does.
//!
//! Retail's ruins reward queues a research action for the tech, and the tech-learning step of that action reveals
//! every known capital when the tech carries the capital-vision ability (Diplomacy).
//!
//! The replay tests are three live games (Might, 14x14, game version 123, 16 and 17 November 2026) in which a
//! player found Diplomacy in a ruin. The server then held one more explored tile for that player (the opponent's
//! capital, 5 points) and the engine did not, so the server's score was 5 above the engine's. The files come from
//! `POLYTOPIA_DIVERGENCE`. The replay tests skip without them.
use polytopia_sim::engine::replay::SaveFile;
use polytopia_sim::engine::{Command, Engine, GameOptions};
use polytopia_sim::rules::path::is_water;
use polytopia_sim::WorldCoordinates as Wc;

const DIPLOMACY: i32 = 38;
const FIELD: i32 = 3;
const FREE_TECH: i32 = 4;

fn dist(a: Wc, b: Wc) -> i32 {
    (a.x - b.x).abs().max((a.y - b.y).abs())
}

/// Player 1 owns every tech but Diplomacy, so the ruin's free tech can only be Diplomacy, and has met player 2.
/// Player 1's first unit stands on a ruin whose reward is a free tech.
fn board() -> (Engine, u32) {
    let mut e = Engine::new_game(&GameOptions::default());
    let all: Vec<i32> = e.t.tech_idx.values().copied().filter(|&t| t != DIPLOMACY).collect();
    e.s.tech.iter_mut().find(|t| t.player == 1).unwrap().tech = all;
    assert_eq!(e.researchable(1), vec![DIPLOMACY]);
    let capital = e.player(2).unwrap().start_tile;
    let start = e.s.units.iter().find(|u| u.owner == 1).unwrap().clone();
    let at = e
        .s
        .map
        .tiles
        .iter()
        .map(|t| t.coordinates)
        .filter(|c| {
            e.tile(*c).is_some_and(|t| !is_water(t.terrain) && t.improvement == 0) && e.ruins_reward(1, *c) == FREE_TECH
        })
        .find(|c| e.s.units.iter().all(|u| dist(u.coordinates, *c) > 2) && dist(*c, capital) > 3)
        .expect("a land tile whose ruin gives a free tech");
    for t in e.s.map.tiles.iter_mut().filter(|t| dist(t.coordinates, at) <= 1) {
        t.terrain = FIELD;
        t.has_ruin = t.coordinates == at;
    }
    assert_eq!(e.ruins_reward(1, at), FREE_TECH);
    let i = e.s.units.iter().position(|u| u.id == start.id).unwrap();
    e.s.units[i].coordinates = at;
    e.s.units[i].moved = false;
    e.s.units[i].attacked = false;
    e.explore_unit(start.id);
    e.meet(1, 2);
    (e, start.id)
}

#[test]
fn a_free_tech_with_capital_vision_reveals_the_capitals_met() {
    let (mut e, unit) = board();
    let capital = e.player(2).unwrap().start_tile;
    assert!(!e.is_explored_at(1, capital), "the capital is out of sight at first");
    e.apply(Command::ExamineRuins { unit }).expect("examining the ruin is legal");
    assert!(e.techs(1).contains(&DIPLOMACY), "the ruin gave Diplomacy");
    assert!(e.is_explored_at(1, capital), "Diplomacy found in a ruin reveals the capital of the player met");
}

#[test]
fn a_free_tech_reveals_no_capital_of_a_player_not_met() {
    let (mut e, unit) = board();
    e.s.player_runtime.iter_mut().find(|r| r.player == 1).unwrap().known_players.retain(|&p| p != 2);
    let capital = e.player(2).unwrap().start_tile;
    e.apply(Command::ExamineRuins { unit }).expect("examining the ruin is legal");
    assert!(!e.is_explored_at(1, capital));
}

fn divergence_dir() -> String {
    std::env::var("POLYTOPIA_DIVERGENCE").unwrap_or_default()
}

/// Replays the pair of files and checks every player's stars and score against the server's state.
fn replay_matches_server(name: &str) {
    let dir = divergence_dir();
    let (Ok(init), Ok(cur)) =
        (std::fs::read(format!("{dir}/{name}_init.bin")), std::fs::read(format!("{dir}/{name}_cur.bin")))
    else {
        eprintln!("skipped: {name} blobs absent in {dir}");
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
        let Some(cmd) = e.command_from_raw(c) else {
            e.s.current_command += 1;
            continue;
        };
        e.apply_unchecked(cmd);
    }
    for theirs in save.current.players.iter().filter(|p| p.id != polytopia_sim::NATURE) {
        let ours = e.player(theirs.id).unwrap();
        assert_eq!(
            (ours.id, ours.currency, ours.score),
            (theirs.id, theirs.currency, theirs.score),
            "player {} (stars, score)",
            theirs.id
        );
    }
}

#[test]
fn game_0d56e799_player_2_score_matches_the_server() {
    replay_matches_server("0d56e799-58d2-43f2-3239-08df401f00f0-1794841481");
}

#[test]
fn game_4f0bfc90_player_2_score_matches_the_server() {
    replay_matches_server("4f0bfc90-4867-407c-1699-08df4019957b-1794907098");
}

#[test]
fn game_c555b89f_player_1_score_matches_the_server() {
    replay_matches_server("c555b89f-0108-45a2-3f14-08df401f00f0-1794863747");
}
