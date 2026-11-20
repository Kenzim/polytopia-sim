//! `Engine::is_legal` answers as `legal_commands().contains()` does.
//!
//! `is_legal` generates only the commands of the unit, city or kind a command names. Over AI games on land and water
//! maps it must agree with the full list for every command of the list and for the commands that were legal in
//! earlier positions of the same game, many of which no longer are.
use polytopia_sim::engine::{Command, Engine, GameOptions, PlayerSetup};
use polytopia_sim::{MapPreset, MapSize};

#[test]
fn is_legal_agrees_with_the_legal_list() {
    let (mut yes, mut no, mut positions) = (0u32, 0u32, 0u32);
    for (seed, players, size, preset) in [
        (31, 4, MapSize::Small, MapPreset::Archipelago),
        (32, 3, MapSize::Tiny, MapPreset::Continents),
        (33, 2, MapSize::Tiny, MapPreset::Dryland),
    ] {
        let seats = (0..players).map(|i| PlayerSetup { tribe: 2 + (i * 5 + seed) % 12, human: false }).collect();
        let mut e = Engine::new_game(&GameOptions {
            seed,
            size,
            preset,
            players: seats,
            turn_limit: 30,
            mode: 1,
            ..GameOptions::default()
        });
        let mut earlier: Vec<Command> = Vec::new();
        let (mut per_turn, mut turn, mut step) = (0, u32::MAX, 0u32);
        while !e.is_over() {
            if e.s.current_turn != turn {
                turn = e.s.current_turn;
                per_turn = 0;
            }
            let pick = e.ai_get_move();
            if step % 3 == 0 {
                let legal = e.legal_commands();
                for c in &legal {
                    assert!(e.is_legal(c), "seed {seed} command {}: {c:?} is in the list", e.s.current_command);
                }
                for c in earlier.iter().chain([&pick, &Command::Stay { unit: 1 }]) {
                    let listed = legal.contains(c);
                    assert_eq!(e.is_legal(c), listed, "seed {seed} command {}: {c:?}", e.s.current_command);
                    if listed {
                        yes += 1;
                    } else {
                        no += 1;
                    }
                }
                // Keep a spread of this position's commands for the next ones.
                earlier.extend(legal.iter().step_by(1 + legal.len() / 40).copied());
                if earlier.len() > 400 {
                    earlier.drain(..earlier.len() - 400);
                }
                positions += 1;
            }
            step += 1;
            per_turn += 1;
            e.apply_unchecked(if per_turn > 400 { Command::EndTurn } else { pick });
        }
    }
    assert!(
        positions > 300 && yes > 5000 && no > 5000,
        "{positions} positions, {yes} legal and {no} illegal commands checked"
    );
}
