//! Convert between the game's own `GameLogicData` JSON files and the engine's rules files.
//!
//! ```text
//! convert_gamedata to-rules <game-files-dir> <rules-dir>   gamedata-v9..v28.json -> v28.json + change files
//! convert_gamedata to-game  <rules-dir> <game-files-dir>   rules files -> gamedata-v9..v28.json (game layout)
//! convert_gamedata check    <game-files-dir>               round-trip proof against the built-in rules
//! ```
//!
//! `check` without a directory uses `POLYTOPIA_ORIGINAL_GAMEDATA`.

use std::path::{Path, PathBuf};
use std::process::ExitCode;

use polytopia_sim::data::convert;
use polytopia_sim::data::rules;

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let result = match args.iter().map(String::as_str).collect::<Vec<_>>().as_slice() {
        ["to-rules", from, to] => to_rules(Path::new(from), Path::new(to)),
        ["to-game", from, to] => to_game(Path::new(from), Path::new(to)),
        ["check", dir] => check(Path::new(dir)),
        ["check"] => match std::env::var_os("POLYTOPIA_ORIGINAL_GAMEDATA") {
            Some(dir) => check(&PathBuf::from(dir)),
            None => Err("pass a directory or set POLYTOPIA_ORIGINAL_GAMEDATA".to_string()),
        },
        _ => Err(include_str!("convert_gamedata.rs")
            .lines()
            .skip(2)
            .take_while(|line| line.starts_with("//!"))
            .map(|line| line.trim_start_matches("//!").trim_start_matches(' '))
            .collect::<Vec<_>>()
            .join("\n")),
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(message) => {
            eprintln!("{message}");
            ExitCode::FAILURE
        }
    }
}

fn to_rules(from: &Path, to: &Path) -> Result<(), String> {
    let game_files = convert::read_game_files(from)?;
    let files = convert::rules_files(&game_files).map_err(|err| err.to_string())?;
    std::fs::create_dir_all(to).map_err(|err| format!("{}: {err}", to.display()))?;
    for (version, text) in &files {
        let path = to.join(rules::file_name(*version));
        std::fs::write(&path, text).map_err(|err| format!("{}: {err}", path.display()))?;
        println!("{}: {} bytes", path.display(), text.len());
    }
    Ok(())
}

fn to_game(from: &Path, to: &Path) -> Result<(), String> {
    std::fs::create_dir_all(to).map_err(|err| format!("{}: {err}", to.display()))?;
    for version in rules::OLDEST_VERSION..=rules::BASE_VERSION {
        let data = polytopia_sim::data::load_game_logic_data_from_dir(from, version)
            .map_err(|err| format!("v{version}: {err}"))?;
        let path = to.join(convert::game_file_name(version));
        let text = convert::game_json_text(&data);
        std::fs::write(&path, &text).map_err(|err| format!("{}: {err}", path.display()))?;
        println!("{}: {} bytes", path.display(), text.len());
    }
    Ok(())
}

fn check(dir: &Path) -> Result<(), String> {
    let game_files = convert::read_game_files(dir)?;
    for line in convert::check_round_trip(&game_files)? {
        println!("{line}");
    }
    println!("round trip ok for versions {}..={}", rules::OLDEST_VERSION, rules::BASE_VERSION);
    Ok(())
}
