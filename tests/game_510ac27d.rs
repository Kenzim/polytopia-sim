//! Live game 510ac27d (an online game, game version 123): the replay must keep the server's stars, scores, city records and
//! pending city rewards the whole way.
//!
//! At command 1502 player 2 builds a road that joins the city at (11,8) to the capital at (11,11). The connection's
//! population runs the city's point first and the capital's second (retail pushes the capital's change, then the city's,
//! onto its action stack, which runs last in, first out), so the city's level-up reward is raised first. The engine
//! raised the capital's first, which changed the order the rewards were answered in: player 2 had 61 stars against the
//! server's 91 at the 1,539 position, and -5 against 50 at the end of the game.
//!
//! The final unit health is not compared: three of player 2's idle units are healed on the server by the final end
//! turn, which the engine ends without end-of-turn effects (the end-match command and a real end turn share one command).
//!
//! The files come from `POLYTOPIA_CLIENT_GAMES`. The mid-game pair is the
//! server's state with 1,539 commands (`rejected/..-1443_*`), the other the finished game. Both tests skip without them.
use polytopia_sim::engine::replay::SaveFile;
use polytopia_sim::engine::Engine;

const GAME: &str = "510ac27d-72d5-4daa-251a-08df37a3787e";

fn games_dir() -> String {
    std::env::var("POLYTOPIA_CLIENT_GAMES").unwrap_or_default()
}

/// Replays every command of the pair on its initial state; `None` when the files are absent.
fn replay(init: &str, cur: &str) -> Option<(Engine, SaveFile, Vec<u8>)> {
    let dir = games_dir();
    let (Ok(init), Ok(cur)) = (std::fs::read(format!("{dir}/{init}")), std::fs::read(format!("{dir}/{cur}"))) else {
        eprintln!("skipped: game {GAME} blobs absent in {dir}");
        return None;
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
    }
    Some((e, save, cur))
}

fn assert_matches_server(e: &Engine, save: &SaveFile, cur: &[u8]) {
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
    // The record owner is the tile's owner (retail's own field is obsolete): the bridges built at turns 39 and 40 outside
    // every border keep 0, not the builder.
    for theirs in save.current.improvements.iter().filter(|r| r.founder != polytopia_sim::NATURE) {
        let ours = e.s.improvements.iter().find(|r| r.tile == theirs.tile).expect("the same improvements");
        assert_eq!(
            (ours.owner, ours.founder),
            (theirs.owner, theirs.founder),
            "record at tile {} (owner, founder)",
            theirs.tile
        );
    }
    let server: Vec<_> = SaveFile::live_triggers(cur).unwrap().into_iter().map(|(_, _, at)| (at.x, at.y)).collect();
    let ours: Vec<_> = e.triggers.iter().map(|t| (t.city.x, t.city.y)).collect();
    assert_eq!(ours, server, "pending city rewards");
}

/// The server's state with 1,539 commands (turn 42): player 2 has 91 stars and no pending reward.
#[test]
fn the_mid_game_position_matches_the_server() {
    let Some((e, save, cur)) =
        replay(&format!("rejected/{GAME}-1443_init.bin"), &format!("rejected/{GAME}-1443_cur.bin"))
    else {
        return;
    };
    assert_eq!(save.commands.len(), 1539);
    assert_eq!(e.s.players.iter().find(|p| p.id == 2).unwrap().currency, 91, "player 2's stars");
    assert!(e.triggers.is_empty(), "no pending reward: {:?}", e.triggers);
    assert_matches_server(&e, &save, &cur);
}

/// The finished game: stars and scores of both players match (50 stars for player 2).
#[test]
fn the_finished_game_ends_with_the_servers_stars() {
    let Some((e, save, cur)) = replay(&format!("{GAME}_init.bin"), &format!("{GAME}_cur.bin")) else { return };
    assert_eq!(save.commands.len(), 2138);
    assert_eq!(e.s.players.iter().find(|p| p.id == 2).unwrap().currency, 50, "player 2's stars");
    assert_matches_server(&e, &save, &cur);
}
