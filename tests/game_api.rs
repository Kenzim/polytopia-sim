//! The stable surface in `polytopia_sim::game`: setting up games by name, command text and JSON, opening game files.

use polytopia_sim::game::{tribe_index, tribe_name, GameMode, NewGame, SetupError};
use polytopia_sim::json::Json;
use polytopia_sim::{
    BotDifficulty, Command, Engine, GameOptions, MapPreset, MapSize, PlayerSetup, VersionError, GAME_MODE_MIGHT,
};

fn tiny() -> NewGame {
    NewGame::new().seed(11).size(MapSize::Tiny).human("Imperius").human("bardur")
}

#[test]
fn names_parse_loosely() {
    assert_eq!("Water World".parse::<MapPreset>(), Ok(MapPreset::WaterWorld));
    assert_eq!("water-world".parse::<MapPreset>(), Ok(MapPreset::WaterWorld));
    assert_eq!("TINY".parse::<MapSize>(), Ok(MapSize::Tiny));
    assert_eq!("might".parse::<GameMode>(), Ok(GameMode::Might));
    assert_eq!("crazy".parse::<BotDifficulty>(), Ok(BotDifficulty::Crazy));
    assert!("enormous".parse::<MapSize>().is_err());
    assert_eq!(GameMode::Might.code(), GAME_MODE_MIGHT);
}

#[test]
fn tribes_by_name_both_ways() {
    let v = polytopia_sim::GAME_VERSION;
    for (name, idx) in [("Xin-xi", 13), ("xinxi", 13), ("Imperius", 7), ("Bardur", 4), ("Polaris", 16), ("Cymanti", 17)]
    {
        assert_eq!(tribe_index(v, name), Some(idx), "{name}");
    }
    assert_eq!(tribe_name(v, 7).as_deref(), Some("imperius"));
    assert_eq!(tribe_index(v, "nature"), None);
    assert_eq!(tribe_index(v, "none"), None);
    assert_eq!(tribe_index(v, "atlantis"), None);
}

#[test]
fn new_game_matches_the_low_level_options() {
    let built = tiny().mode(GameMode::Might).build().unwrap();
    let opts = GameOptions {
        seed: 11,
        size: MapSize::Tiny,
        players: vec![PlayerSetup { tribe: 7, human: true }, PlayerSetup { tribe: 4, human: true }],
        mode: GAME_MODE_MIGHT,
        ..GameOptions::default()
    };
    let direct = Engine::new_game(&opts);
    assert_eq!(built.to_position().to_string(), direct.to_position().to_string());
}

#[test]
fn setup_errors() {
    assert_eq!(tiny().human("atlantis").build().err(), Some(SetupError::UnknownTribe("atlantis".into())));
    assert_eq!(tiny().version(50).build().err(), Some(SetupError::Version(VersionError::Unsupported(50))));
    assert_eq!(tiny().version(100).build().err(), Some(SetupError::Version(VersionError::BeforeNewMatch(100))));
    assert!(matches!(NewGame::new().build(), Err(SetupError::NewGame(_))));
    assert!(tiny().version(108).build().is_ok());
}

#[test]
fn one_map_attempt_is_a_valid_but_different_game() {
    let fair = tiny().build().unwrap();
    let quick = tiny().map_attempts(1).build().unwrap();
    assert_eq!(quick.s.players.len(), fair.s.players.len());
    for e in [&fair, &quick] {
        for id in [1u8, 2] {
            assert!(e.capital_of(id).is_some(), "player {id} has a capital");
        }
        assert!(!e.legal_commands().is_empty());
    }
    assert_eq!(tiny().map_attempts(1).build().unwrap().to_position().to_string(), quick.to_position().to_string());
}

#[test]
fn every_legal_command_reads_back_from_its_text() {
    let mut game = tiny().build().unwrap();
    let mut rng = 0x2545F4914F6CDD1Du64;
    for _ in 0..300 {
        if game.is_over() {
            break;
        }
        let legal = game.legal_commands();
        for c in &legal {
            assert_eq!(game.parse_command(&c.to_string()), Ok(*c));
            let j = game.command_json(c);
            assert_eq!(j.get("text").and_then(Json::as_str), Some(c.to_string().as_str()));
            let Json::Obj(fields) = &j else { panic!("command JSON is not an object: {j}") };
            let mut keys: Vec<&str> = fields.iter().map(|(k, _)| k.as_str()).collect();
            keys.sort_unstable();
            assert!(keys.windows(2).all(|w| w[0] != w[1]), "repeated key in {j}");
            if let Command::Move { to, .. } = c {
                assert_eq!(j.get("to"), Some(&Json::from(vec![to.x, to.y])));
            }
        }
        rng ^= rng << 13;
        rng ^= rng >> 7;
        rng ^= rng << 17;
        let pick = legal[(rng % legal.len() as u64) as usize];
        game.apply(pick).unwrap();
    }
    assert!(game.parse_command("Move { unit: 9999, to: WorldCoordinates { x: 0, y: 0 } }").is_err());
    assert_eq!(game.parse_command("  EndTurn \n"), Ok(Command::EndTurn));
}

#[test]
fn positions_open_from_files() {
    let game = tiny().build().unwrap();
    let path = std::env::temp_dir().join(format!("polytopia-sim-game-api-{}.json", std::process::id()));
    std::fs::write(&path, game.to_position().to_string()).unwrap();
    let back = Engine::open_save(path.to_str().unwrap()).unwrap();
    std::fs::write(&path, back.to_position().to_string()).unwrap();
    let again = Engine::open_save(path.to_str().unwrap()).unwrap();
    std::fs::remove_file(&path).ok();
    // A new game holds no records for neutral improvements; loading fills in defaults, so compare from the first load.
    assert_eq!(back.legal_commands(), game.legal_commands());
    assert_eq!(again.to_position().to_string(), back.to_position().to_string());
    assert!(Engine::open_save("/nonexistent/game.state").is_err());
}

#[test]
fn scores_cover_the_seats() {
    let game = tiny().build().unwrap();
    let scores = game.scores();
    assert_eq!(scores.iter().map(|(id, _)| *id).collect::<Vec<_>>(), vec![1, 2]);
    assert!(scores.iter().all(|&(id, s)| s == game.score(id)));
}

/// The recorded games shipped in `tests/data/games` replay with every command legal, end where the file does, and
/// the engine's AI chooses every move the game's AI made.
#[test]
fn shipped_recorded_games_replay_exactly() {
    let dir = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/data/games");
    let Ok(entries) = std::fs::read_dir(dir) else {
        eprintln!("skipped: no {dir}");
        return;
    };
    let mut files: Vec<String> =
        entries.filter_map(|e| e.ok()?.file_name().into_string().ok()).filter(|n| n.ends_with(".json")).collect();
    files.sort();
    for name in files {
        let r = polytopia_sim::game::replay_save(&format!("{dir}/{name}"), true).unwrap();
        assert!(r.commands > 0, "{name}: no commands");
        assert!(r.illegal.is_empty(), "{name}: {:?}", r.illegal);
        assert!(r.differences.is_empty(), "{name}: {:?}", r.differences);
        assert_eq!(r.ai_differed, 0, "{name}: the engine's AI chose differently {} times", r.ai_differed);
    }
}

/// A recorded game written by `record_save` replays exactly like the save it came from.
#[test]
fn recorded_games_replay_like_their_saves() {
    let dir = std::env::var("POLYTOPIA_REGRESS").unwrap_or_default();
    let path = format!("{dir}/cym14.state");
    if !std::path::Path::new(&path).exists() {
        eprintln!("skipped: {path} not found");
        return;
    }
    let file = std::env::temp_dir().join(format!("polytopia-sim-record-{}.json", std::process::id()));
    std::fs::write(&file, polytopia_sim::game::record_save(&path).unwrap().to_string()).unwrap();
    let a = polytopia_sim::game::replay_save(&path, true).unwrap();
    let b = polytopia_sim::game::replay_save(file.to_str().unwrap(), true).unwrap();
    std::fs::remove_file(&file).ok();
    assert_eq!(
        (b.commands, b.illegal.len(), b.ai_agreed, b.ai_differed),
        (a.commands, a.illegal.len(), a.ai_agreed, a.ai_differed)
    );
    assert!(b.differences.is_empty(), "{:?}", b.differences);
    assert_eq!(b.engine.to_position().to_string(), a.engine.to_position().to_string());
}

/// Every regression game (`POLYTOPIA_REGRESS`) replays with every command legal and ends where the file does,
/// including the home city of every unit still on the board.
#[test]
fn replay_save_matches_the_regression_games() {
    let dir = std::env::var("POLYTOPIA_REGRESS").unwrap_or_default();
    let Ok(entries) = std::fs::read_dir(&dir) else {
        eprintln!("skipped: POLYTOPIA_REGRESS not set or unreadable");
        return;
    };
    let mut names: Vec<String> = entries.filter_map(|e| e.ok()?.file_name().into_string().ok()).collect();
    names.sort();
    let mut games = 0;
    for name in names {
        let path = if let Some(game) = name.strip_suffix("_init.bin") {
            format!("{dir}/{name}+{dir}/{game}_cur.bin")
        } else if name.ends_with(".state") {
            format!("{dir}/{name}")
        } else {
            continue;
        };
        let r = polytopia_sim::game::replay_save(&path, false).unwrap();
        games += 1;
        assert!(r.differences.is_empty(), "{name}: {:?}", r.differences);
        assert!(r.illegal.is_empty(), "{name}: {:?}", r.illegal);
        let save = polytopia_sim::engine::replay::SaveFile::open(&path).unwrap();
        for theirs in &save.current.units {
            if let Some(ours) = r.engine.s.units.iter().find(|u| u.id == theirs.id) {
                assert_eq!(ours.home, theirs.home, "{name}: home of unit {} (type {})", theirs.id, theirs.type_idx);
            }
        }
    }
    assert!(games > 0, "no games in {dir}");
}
