//! Every player driven by the built-in AI, with a score line per turn and optional command trace.
//!
//! ```text
//! cargo run --release --example play_ai [seed] [turns]        # defaults: seed 1, 30 turns
//! TRACE=1 cargo run --release --example play_ai               # also print every command
//! ```
use polytopia_sim::engine::{Command, Engine, GameOptions, PlayerSetup};
use polytopia_sim::{MapPreset, MapSize};

fn main() {
    let seed: i32 = std::env::args().nth(1).and_then(|s| s.parse().ok()).unwrap_or(1);
    let turns: i32 = std::env::args().nth(2).and_then(|s| s.parse().ok()).unwrap_or(30);
    let opts = GameOptions {
        seed,
        size: MapSize::Small,
        preset: MapPreset::Continents,
        players: vec![
            PlayerSetup { tribe: 4, human: false },
            PlayerSetup { tribe: 7, human: false },
            PlayerSetup { tribe: 10, human: false },
        ],
        turn_limit: turns,
        ..GameOptions::default()
    };
    let mut e = Engine::new_game(&opts);
    let mut commands = 0;
    let mut per_turn = 0;
    let mut last_turn = -1;
    while !e.is_over() && commands < 200000 {
        if e.s.current_turn as i32 != last_turn {
            last_turn = e.s.current_turn as i32;
            per_turn = 0;
            println!(
                "turn {} scores {:?}",
                last_turn,
                e.s.players.iter().filter(|p| p.id != 255).map(|p| (p.id, e.score(p.id))).collect::<Vec<_>>()
            );
        }
        let mut cmd = e.ai_get_move();
        per_turn += 1;
        if per_turn > 400 {
            cmd = Command::EndTurn;
        }
        if std::env::var("TRACE").is_ok() {
            println!("  p{} {:?}", e.current_player(), cmd);
        }
        if !matches!(cmd, Command::EndTurn | Command::Stay { .. }) && !e.legal_commands().contains(&cmd) {
            println!("ILLEGAL {:?}", cmd);
            cmd = Command::EndTurn;
        }
        e.apply_unchecked(cmd);
        commands += 1;
    }
    println!("done: over={} turn={} commands={} winners={:?}", e.is_over(), e.s.current_turn, commands, e.winners);
    for p in &e.s.players {
        println!(
            "player {} tribe {} score {} stars {} cities {} units {}",
            p.id,
            p.tribe,
            e.score(p.id),
            p.currency,
            e.city_count(p.id),
            e.s.units.iter().filter(|u| u.owner == p.id).count()
        );
    }
}
