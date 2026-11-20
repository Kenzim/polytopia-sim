//! Start a game, let the built-in AI play every seat to the turn limit and print the result.
//!
//! ```text
//! cargo run --release --example play_game [seed] [turns]      # defaults: seed 1, 30 turns
//! ```
use polytopia_sim::game::{tribe_name, NewGame};
use polytopia_sim::MapSize;

fn main() {
    let seed: i32 = std::env::args().nth(1).and_then(|s| s.parse().ok()).unwrap_or(1);
    let turns: i32 = std::env::args().nth(2).and_then(|s| s.parse().ok()).unwrap_or(30);
    let mut game = NewGame::new()
        .seed(seed)
        .size(MapSize::Small)
        .ai("bardur")
        .ai("imperius")
        .ai("oumaji")
        .turn_limit(turns)
        .build()
        .expect("new game");

    let mut commands = 0u32;
    let mut commands_this_turn = 0u32;
    while game.step_ai(&mut commands_this_turn) {
        commands += 1;
    }

    println!(
        "seed {seed}: game over at turn {} after {commands} commands, winners {:?}",
        game.s.current_turn,
        game.game_winners()
    );
    for (id, score) in game.scores() {
        let player = game.player(id).expect("seat");
        let tribe = tribe_name(game.s.version, player.tribe).unwrap_or_default();
        println!("player {id} ({tribe}): score {score}, cities {}, stars {}", game.city_count(id), player.currency);
    }
}
