//! A capture explores the captured city's area quietly (it meets no players), but a map corner it reveals is still a
//! lighthouse discovery: the lighthouse levels up and the capturer's capital gains one population and one xp.
//!
//! The replay test reads live game 4f260976 (turn 18, the capture at command 469 reveals the corner at 10,10) from
//! `POLYTOPIA_DIVERGENCE`, and skips without it.
use polytopia_sim::engine::replay::SaveFile;
use polytopia_sim::engine::Engine;

const GAME: &str = "4f260976-15ba-46a7-f94e-08df37a3787d-1793643883";
const CAPTURE: usize = 469;
const LIGHTHOUSE_TILE: usize = 120;

fn divergence_dir() -> String {
    std::env::var("POLYTOPIA_DIVERGENCE").unwrap_or_default()
}

fn replay(save: &SaveFile, stop_after: Option<usize>) -> Engine {
    let mut e = Engine::from_state(save.initial.clone());
    if save.commands.iter().any(|c| c.kind == 20) {
        e.start_match();
    }
    for (n, c) in save.commands.iter().enumerate() {
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
        if stop_after == Some(n) {
            break;
        }
    }
    e
}

fn lighthouse_level(e: &Engine) -> Option<i32> {
    e.s.improvements.iter().find(|r| r.tile as usize == LIGHTHOUSE_TILE).map(|r| i32::from(r.level))
}

#[test]
fn a_capture_that_reveals_a_corner_discovers_its_lighthouse() {
    let dir = divergence_dir();
    let (Ok(init), Ok(cur)) =
        (std::fs::read(format!("{dir}/{GAME}_init.bin")), std::fs::read(format!("{dir}/{GAME}_cur.bin")))
    else {
        eprintln!("skipped: blobs of {GAME} absent in {dir}");
        return;
    };
    let save = SaveFile::parse_server(&init, &cur).unwrap();
    let c = &save.commands[CAPTURE];
    assert_eq!((c.kind, c.player), (7, 2), "command {CAPTURE} is player 2's capture");

    let before = replay(&save, Some(CAPTURE - 1));
    assert_eq!(before.s.fog[LIGHTHOUSE_TILE].explorers, vec![1], "only player 1 had seen the corner");
    assert_eq!(lighthouse_level(&before), Some(2));

    let after = replay(&save, Some(CAPTURE));
    assert_eq!(after.s.fog[LIGHTHOUSE_TILE].explorers, vec![1, 2], "the capture explored the corner");
    assert_eq!(lighthouse_level(&after), Some(3), "the capture discovered the lighthouse");

    // At the end of the turn the players and improvements equal the server's.
    let e = replay(&save, None);
    for theirs in save.current.players.iter().filter(|p| p.id != polytopia_sim::NATURE) {
        let ours = e.s.players.iter().find(|p| p.id == theirs.id).unwrap();
        assert_eq!((ours.currency, ours.score), (theirs.currency, theirs.score), "player {} (stars, score)", theirs.id);
    }
    for theirs in &save.current.improvements {
        let Some(ours) = e.s.improvements.iter().find(|r| r.tile == theirs.tile) else { continue };
        assert_eq!(
            (ours.level, ours.population, ours.xp),
            (theirs.level, theirs.population, theirs.xp),
            "improvement at tile {} (level, population, xp)",
            theirs.tile
        );
    }
}
