//! Two live online games (game version 123) saved on 16 November 2026 whose replays used to drift from the server.
//!
//! The game of 01:17 (133 commands): player 2's build at (7,7) gives the city at (8,8) 6 population at once, 2 xp
//! below level 3 and 3 below level 4. Retail adds the points one at a time, so the city levels up twice and the
//! server holds a second reward for it; the engine levelled it once, so it had no reward and 5 more score.
//!
//! The game of 01:19 (747 commands): player 1 captures the city at (8,2) with the last command. The capture's
//! connection update pushes the connection and population changes of the capturer, then of the previous owner, and the
//! stack runs them last in, first out. The city loses a connection point (previous owner) before it gains one
//! (capturer); the engine ran the gain first, which took the city over its level and raised a reward the server does
//! not have, with 5 score for the point.
//!
//! The files come from `POLYTOPIA_DIVERGENCE`. The tests skip without them.
use polytopia_sim::engine::replay::SaveFile;
use polytopia_sim::engine::Engine;

fn divergence_dir() -> String {
    std::env::var("POLYTOPIA_DIVERGENCE").unwrap_or_default()
}

/// Replays the pair and checks stars, scores, cities and pending city rewards against the server's state.
fn replay_matches_server(stem: &str, commands: usize) {
    let dir = divergence_dir();
    let (Ok(init), Ok(cur)) =
        (std::fs::read(format!("{dir}/{stem}_init.bin")), std::fs::read(format!("{dir}/{stem}_cur.bin")))
    else {
        eprintln!("skipped: game {stem} blobs absent in {dir}");
        return;
    };
    let save = SaveFile::parse_server(&init, &cur).unwrap();
    assert_eq!(save.commands.len(), commands);
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
        let ours = e.s.players.iter().find(|p| p.id == theirs.id).unwrap();
        assert_eq!((ours.currency, ours.score), (theirs.currency, theirs.score), "player {} (stars, score)", theirs.id);
    }
    for theirs in save.current.improvements.iter().filter(|r| r.owner != polytopia_sim::NATURE && r.level > 0) {
        let ours = e.s.improvements.iter().find(|r| r.tile == theirs.tile).expect("the same cities");
        assert_eq!(
            (ours.level, ours.population, ours.xp, ours.connected_to_capital_of_player, &ours.rewards),
            (theirs.level, theirs.population, theirs.xp, theirs.connected_to_capital_of_player, &theirs.rewards),
            "city at tile {} (level, population, xp, connected, rewards)",
            theirs.tile
        );
    }
    let server: Vec<_> = SaveFile::live_triggers(&cur).unwrap().into_iter().map(|(p, _, at)| (p, at.x, at.y)).collect();
    let ours: Vec<_> = e.triggers.iter().map(|t| (t.player, t.city.x, t.city.y)).collect();
    assert_eq!(ours, server, "pending city rewards");
}

/// Saved 16 November 01:17: player 2 has 7 stars and score 2605, and a pending reward at (8,8).
#[test]
fn the_game_saved_at_0117_matches_the_server() {
    replay_matches_server("16d7a3bc-dbf6-4860-d239-08df4314f1b7-1794791846", 133);
}

/// Saved 16 November 01:19: scores 7225 and 2735, and no pending reward.
#[test]
fn the_game_saved_at_0119_matches_the_server() {
    replay_matches_server("74509ec1-beef-4111-d1cd-08df4314f1b7-1794791948", 747);
}
