//! A copy of a game is a full, independent copy.
//!
//! `GameState::clone` copies units, improvements and fog as blocks of bytes and clones only the items that own
//! memory (a unit carrying a passenger, a city with rewards, a tile explored by more players than fit in place).
//! These games reach all three, and the copy must equal the original, stay equal to it under the same commands, and
//! be untouched by what happens to the original afterwards.
use polytopia_sim::engine::{Command, Engine, GameOptions, PlayerSetup};
use polytopia_sim::{MapPreset, MapSize};

fn game(seed: i32, players: usize, size: MapSize, preset: MapPreset) -> Engine {
    let seats = (0..players).map(|i| PlayerSetup { tribe: 2 + (i as i32 * 3 + seed) % 12, human: false }).collect();
    Engine::new_game(&GameOptions {
        seed,
        size,
        preset,
        players: seats,
        turn_limit: 30,
        mode: 1,
        ..GameOptions::default()
    })
}

/// Play up to `commands` AI commands; false once the game is over.
fn advance(e: &mut Engine, commands: usize, per_turn: &mut u32, turn: &mut u32) -> bool {
    for _ in 0..commands {
        if e.is_over() {
            return false;
        }
        if e.s.current_turn != *turn {
            *turn = e.s.current_turn;
            *per_turn = 0;
        }
        let mut c = e.ai_get_move();
        *per_turn += 1;
        if *per_turn > 400 {
            c = Command::EndTurn;
        }
        e.apply_unchecked(c);
    }
    true
}

#[test]
fn a_copy_is_equal_and_independent() {
    let (mut passengers, mut rewards, mut copies) = (0, 0, 0);
    for (seed, players, size, preset) in [
        (11, 4, MapSize::Small, MapPreset::Archipelago),
        (12, 3, MapSize::Tiny, MapPreset::Continents),
        (13, 2, MapSize::Tiny, MapPreset::WaterWorld),
    ] {
        let mut e = game(seed, players, size, preset);
        let (mut per_turn, mut turn) = (0, u32::MAX);
        while advance(&mut e, 40, &mut per_turn, &mut turn) {
            passengers += e.s.units.iter().filter(|u| u.passenger.is_some()).count();
            rewards += e.s.improvements.iter().filter(|i| !i.rewards.is_empty()).count();
            let before = format!("{:?}", e.s);
            let mut copy = e.clone();
            assert_eq!(copy.s, e.s);
            assert_eq!(format!("{:?}", copy.s), before);
            // The same commands on both give the same game.
            let (mut pt, mut t) = (per_turn, turn);
            advance(&mut copy, 40, &mut pt, &mut t);
            let kept = e.clone();
            let more = advance(&mut e, 40, &mut per_turn, &mut turn);
            assert_eq!(copy.s, e.s, "seed {seed}: the copy played on like the original");
            // And the copy taken before them is as it was.
            assert_eq!(format!("{:?}", kept.s), before, "seed {seed}: the earlier copy changed");
            copies += 1;
            if !more {
                break;
            }
        }
    }
    assert!(copies > 20, "{copies} copies checked");
    assert!(passengers > 0 && rewards > 0, "the games reach carried units ({passengers}) and city rewards ({rewards})");
}

#[test]
fn a_tile_explored_by_many_players_copies() {
    let mut e = game(21, 2, MapSize::Tiny, MapPreset::Dryland);
    let ids: Vec<u8> = (1..=40).collect();
    for f in e.s.fog.iter_mut().step_by(3) {
        f.explorers = ids.iter().copied().collect();
    }
    let copy = e.s.clone();
    assert_eq!(copy, e.s);
    for f in e.s.fog.iter_mut() {
        f.explorers.clear();
    }
    assert!(copy.fog.iter().step_by(3).all(|f| f.explorers[..] == ids[..]));
}
