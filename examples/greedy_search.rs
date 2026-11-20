//! A small search player against the built-in AI, to show copying games for lookahead.
//!
//! Player 1 looks one command ahead: it tries every legal command on a copy of the game and keeps the one that most
//! raises its score over the opponent's, ending its turn when nothing helps. Player 2 is the built-in AI.
//!
//! ```text
//! cargo run --release --example greedy_search [seed] [turns]  # defaults: seed 1, 20 turns
//! ```
use polytopia_sim::game::NewGame;
use polytopia_sim::{Command, Engine, MapSize};

/// The searching player's score minus the best other score.
fn margin(game: &Engine, me: u8) -> i32 {
    let scores = game.scores();
    let mine = scores.iter().find(|(id, _)| *id == me).map_or(0, |s| s.1);
    let best_other = scores.iter().filter(|(id, _)| *id != me).map(|s| s.1).max().unwrap_or(0);
    mine - best_other
}

fn main() {
    let seed: i32 = std::env::args().nth(1).and_then(|s| s.parse().ok()).unwrap_or(1);
    let turns: i32 = std::env::args().nth(2).and_then(|s| s.parse().ok()).unwrap_or(20);
    let mut game = NewGame::new()
        .seed(seed)
        .size(MapSize::Tiny)
        .human("imperius")
        .ai("bardur")
        .turn_limit(turns)
        .build()
        .expect("new game");
    let me = 1;
    let (mut ai_commands, mut tried) = (0u32, 0u64);
    while !game.is_over() {
        if game.current_player() != me {
            game.step_ai(&mut ai_commands);
            continue;
        }
        let now = margin(&game, me);
        let mut best = (now, Command::EndTurn);
        let legal = game.legal_commands();
        for &cmd in legal.iter().filter(|c| **c != Command::EndTurn) {
            let mut trial = game.clone();
            trial.apply_unchecked(cmd);
            tried += 1;
            let m = margin(&trial, me);
            if m > best.0 {
                best = (m, cmd);
            }
        }
        // EndTurn is not legal while a choice (a city reward) is pending; then take the first option.
        if best.1 == Command::EndTurn && !legal.contains(&Command::EndTurn) {
            best.1 = legal[0];
        }
        game.apply(best.1).expect("legal");
    }
    println!("game over at turn {}: {tried} positions tried", game.s.current_turn);
    for (id, score) in game.scores() {
        println!("player {id} ({}): score {score}", if id == me { "greedy search" } else { "built-in AI" });
    }
    println!("winners {:?}", game.game_winners());
}
