//! The engine's rules files against the game's own logic data JSON files.
//!
//! Needs the game files, which are not part of this repository: set
//! `POLYTOPIA_ORIGINAL_GAMEDATA` to a directory holding `gamedata-v9.json` ..
//! `gamedata-v28.json`. Without it the test says so and passes.

use polytopia_sim::data::convert;

#[test]
fn rules_files_match_the_game_files_for_every_version() {
    let Some(dir) = std::env::var_os("POLYTOPIA_ORIGINAL_GAMEDATA") else {
        eprintln!("skipped: POLYTOPIA_ORIGINAL_GAMEDATA is not set (round trip against the game files not run)");
        return;
    };
    let game_files = convert::read_game_files(std::path::Path::new(&dir)).expect("game files");
    let report = convert::check_round_trip(&game_files).unwrap_or_else(|err| panic!("{err}"));
    assert_eq!(report.len(), 20);
    for line in report {
        eprintln!("{line}");
    }
}
