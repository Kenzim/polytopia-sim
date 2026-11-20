//! The rules end a game by storing `FinalTurn`, not `Ended`: the player to move keeps acting, and only the end-match
//! command (`Command::EndTurn` in that state) ends the game. Nothing runs in between: no end of turn, no new turn, no
//! income. The scenario test is in `might.rs`; these replays are retail games that kept accepting commands after the
//! capture of the last city:
//!
//! * a live game that ended on the capture of the last capital (turn 19): nine commands followed, the last an end
//!   turn, and the server's final turn and stars are those before it (the engine used to start turn 20 and pay income);
//! * a human Polaris game, Oumaji 8470 v Polaris 1650 (turn 14): nine commands after the last capture;
//! * a human Polaris game, Polaris 5370 v Polaris 6040 (turn 24): a city reward after the last capture.
//!
//! Each replay reads the game from `POLYTOPIA_CLIENT_GAMES` (the first) or `POLYTOPIA_HUMAN_RAW` (the others), and
//! skips without it.
use polytopia_sim::engine::replay::SaveFile;
use polytopia_sim::engine::Engine;
use polytopia_sim::{GameStatus, NATURE};

fn client_games() -> String {
    std::env::var("POLYTOPIA_CLIENT_GAMES").unwrap_or_default()
}

fn human_raw() -> String {
    std::env::var("POLYTOPIA_HUMAN_RAW").unwrap_or_default()
}

/// Replays every command as the server recorded it; returns the engine, the number of commands the engine would not
/// have allowed once the game was in `FinalTurn`, and whether it got there.
fn replay(save: &SaveFile) -> (Engine, usize, bool) {
    let mut e = Engine::from_state(save.initial.clone());
    if save.commands.iter().any(|c| c.kind == 20) {
        e.start_match();
    }
    let (mut refused, mut final_turn) = (0, false);
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
        if e.s.current_state == GameStatus::FinalTurn {
            final_turn = true;
            if !e.legal_commands().contains(&cmd) {
                refused += 1;
            }
        }
        e.apply_unchecked(cmd);
    }
    (e, refused, final_turn)
}

fn check(dir: String, game: &str) {
    let (Ok(init), Ok(cur)) =
        (std::fs::read(format!("{dir}/{game}_init.bin")), std::fs::read(format!("{dir}/{game}_cur.bin")))
    else {
        eprintln!("skipped: blobs of {game} absent in {dir}");
        return;
    };
    let save = SaveFile::parse_server(&init, &cur).unwrap();
    let (e, refused, final_turn) = replay(&save);
    assert!(final_turn, "the rules put the game into FinalTurn before it ends");
    assert_eq!(refused, 0, "every command the server accepted in the final turn is legal");
    assert!(e.is_over(), "the end-match command ends the game");
    assert_eq!(e.s.current_turn, save.current.current_turn, "no turn starts after the game-over");
    for theirs in save.current.players.iter().filter(|p| p.id != NATURE) {
        let ours = e.s.players.iter().find(|p| p.id == theirs.id).unwrap();
        assert_eq!((ours.currency, ours.score), (theirs.currency, theirs.score), "player {} (stars, score)", theirs.id);
    }
}

#[test]
fn a_capital_capture_game_ends_without_a_new_turn_or_income() {
    check(client_games(), "c994887c-2f6d-4a98-f949-08df37a3787d");
}

#[test]
fn oumaji_8470_v_polaris_1650_plays_on_after_the_last_capture() {
    check(human_raw(), "71ce7323-53ed-49ce-0300-08df34296509");
}

#[test]
fn polaris_5370_v_polaris_6040_accepts_the_reward_after_the_last_capture() {
    check(human_raw(), "9266e1df-10f2-4d97-4adf-08df29efb9b0");
}
