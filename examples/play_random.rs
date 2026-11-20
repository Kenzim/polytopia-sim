//! Random legal play until the game ends or a command budget runs out.
//!
//! ```text
//! cargo run --release --example play_random [seed]            # default seed 1
//! ```
use polytopia_sim::game::NewGame;
use polytopia_sim::{Command, MapSize};

fn main() {
    let seed: i32 = std::env::args().nth(1).and_then(|s| s.parse().ok()).unwrap_or(1);
    let mut e = NewGame::new()
        .seed(seed)
        .size(MapSize::Small)
        .human("bardur")
        .ai("imperius")
        .ai("oumaji")
        .turn_limit(30)
        .build()
        .expect("new game");
    let mut rng = seed as u64 * 2862933555777941757 + 3037000493;
    let mut next = || {
        rng ^= rng << 13;
        rng ^= rng >> 7;
        rng ^= rng << 17;
        rng
    };
    let mut commands = 0;
    while !e.is_over() && commands < 20000 {
        let legal = e.legal_commands();
        // Nine times in ten play something other than EndTurn, so turns do things. EndTurn is not legal while a
        // choice is pending (a city reward, say), so then a random other command is played instead.
        let others: Vec<Command> = legal.iter().copied().filter(|c| *c != Command::EndTurn).collect();
        let can_end = legal.contains(&Command::EndTurn);
        let cmd = if can_end && (others.is_empty() || next() % 10 == 0) {
            Command::EndTurn
        } else if !others.is_empty() {
            others[(next() % others.len() as u64) as usize]
        } else {
            break;
        };
        e.apply(cmd).expect("a listed command is legal");
        commands += 1;
        if commands % 500 == 0 {
            println!("turn {} commands {} scores {:?}", e.s.current_turn, commands, e.scores());
        }
    }
    println!(
        "done: over={} turn={} commands={} winners={:?}",
        e.is_over(),
        e.s.current_turn,
        commands,
        e.game_winners()
    );
    for (id, score) in e.scores() {
        let stars = e.player(id).map_or(0, |p| p.currency);
        println!(
            "player {id} score {score} stars {stars} cities {} units {}",
            e.city_count(id),
            e.s.units.iter().filter(|u| u.owner == id).count()
        );
    }
}
