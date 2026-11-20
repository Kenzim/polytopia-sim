//! A scout's walk stops where a city reward is raised, until the reward is answered.
//!
//! Revealing a map corner (the lighthouse) adds a population to the capital, and that can level it up. Retail's action
//! stack stops at the pending city reward, so the explore actions and scout steps still queued behind it wait: the
//! explorer walks no further in that command.
//!
//! The replay test is live game 45dd7bec (an online game, game version 123, 16 November 2026): at its last command player 2
//! takes an Explorer reward. The server's walk ends on the step that reaches the lighthouse corner, with one tile of
//! that step still unexplored, and the player's stars stay at 7. The engine used to walk all 12 steps, meet player 1
//! on the way and score 14.
//!
//! The files come from `POLYTOPIA_DIVERGENCE`. The test skips without them.
use polytopia_sim::engine::replay::SaveFile;
use polytopia_sim::engine::Engine;

const GAME: &str = "45dd7bec-a3c6-4df2-c2d1-08df4019957a";
const STAMP: &str = "1794781271";
const PLAYER: u8 = 2;

fn divergence_dir() -> String {
    std::env::var("POLYTOPIA_DIVERGENCE").unwrap_or_default()
}

#[test]
fn the_explorer_walk_stops_at_the_lighthouse_reward_like_the_server() {
    let dir = divergence_dir();
    let (Ok(init), Ok(cur)) = (
        std::fs::read(format!("{dir}/{GAME}-{STAMP}_init.bin")),
        std::fs::read(format!("{dir}/{GAME}-{STAMP}_cur.bin")),
    ) else {
        eprintln!("skipped: game {GAME} blobs absent in {dir}");
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
    let stars = |s: &polytopia_sim::GameState| s.players.iter().find(|p| p.id == PLAYER).unwrap().currency;
    assert_eq!(stars(&e.s), stars(&save.current), "player 2's stars");
    let seen = |s: &polytopia_sim::GameState| s.fog.iter().map(|f| f.explorers.contains(&PLAYER)).collect::<Vec<_>>();
    assert_eq!(seen(&e.s), seen(&save.current), "the tiles player 2 has explored");
}
