//! A new game holds an improvement record for every improvement on the map, neutral villages, ruins and corner
//! lighthouses included, as retail's map generation writes them. So a fresh game's position is what loading and saving
//! it again gives, and a record the loader makes up for a hand-made position is the one a new game has.
//!
//! The retail check reads the initial states of the online games in `POLYTOPIA_CLIENT_GAMES` (`<id>_init.bin` with
//! `<id>_cur.bin`) and skips without them.
use polytopia_sim::engine::replay::SaveFile;
use polytopia_sim::engine::tables::improvement;
use polytopia_sim::engine::Engine;
use polytopia_sim::{GameOptions, ImprovementRuntime, MapSize, PlayerSetup};

fn game(seed: i32, size: MapSize, tribes: &[i32]) -> Engine {
    let players = tribes.iter().map(|&tribe| PlayerSetup { tribe, human: true }).collect();
    Engine::new_game(&GameOptions { seed, size, players, ..GameOptions::default() })
}

/// What retail writes for a neutral improvement of `kind`.
fn retail_neutral(tile: i32, kind: i32) -> ImprovementRuntime {
    ImprovementRuntime {
        tile,
        level: 1,
        production: u16::from(kind != 47),
        border_size: u16::from(kind == improvement::CITY),
        ..ImprovementRuntime::default()
    }
}

#[test]
fn a_fresh_games_position_survives_a_load_and_save() {
    // Imperius v Bardur on Tiny with seed 11 is the case the game API's file test opens; the others vary size, tribes and seats.
    let cases: [(i32, MapSize, &[i32]); 6] = [
        (11, MapSize::Tiny, &[7, 4]),
        (3, MapSize::Small, &[2, 3, 5]),
        (42, MapSize::Normal, &[2, 7, 8, 9]),
        (7, MapSize::Tiny, &[13, 16]),
        (99, MapSize::Small, &[17, 4]),
        (123, MapSize::Large, &[2, 4, 6, 7]),
    ];
    let (mut villages, mut ruins, mut lighthouses) = (0, 0, 0);
    for (seed, size, tribes) in cases {
        let g = game(seed, size, tribes);
        let tag = format!("seed {seed} {size:?} {tribes:?}");
        for t in g.s.map.tiles.iter().filter(|t| t.improvement != 0) {
            let rt =
                g.improvement_rt(t.coordinates).unwrap_or_else(|| panic!("{tag}: no record at {:?}", t.coordinates));
            if t.owner == 0 {
                // Every player's first sight of a corner lighthouse raises its level by one.
                let seen =
                    g.s.fog
                        .iter()
                        .find(|f| f.tile == t.index)
                        .map_or(0, |f| f.explorers.iter().filter(|&&p| p != polytopia_sim::NATURE).count());
                let mut want = retail_neutral(t.index, t.improvement);
                if t.improvement == 47 {
                    want.level += seen as u16;
                }
                assert_eq!(*rt, want, "{tag}: neutral record at {:?}", t.coordinates);
                match t.improvement {
                    improvement::CITY => villages += 1,
                    improvement::RUIN => ruins += 1,
                    _ => lighthouses += 1,
                }
            }
        }
        let first = g.to_position();
        let back = Engine::from_position(&first).unwrap();
        assert_eq!(back.to_position().to_string(), first.to_string(), "{tag}: first load changed the position");
        assert_eq!(back.legal_commands(), g.legal_commands(), "{tag}");
    }
    assert!(
        villages > 0 && ruins > 0 && lighthouses > 0,
        "the cases cover villages ({villages}), ruins ({ruins}) and lighthouses ({lighthouses})"
    );
}

#[test]
fn neutral_records_match_retail_initial_states() {
    let dir = std::env::var("POLYTOPIA_CLIENT_GAMES").unwrap_or_default();
    let Ok(entries) = std::fs::read_dir(&dir) else {
        eprintln!("skipped: no {dir}");
        return;
    };
    let mut names: Vec<String> =
        entries.filter_map(|e| e.ok()?.file_name().into_string().ok()).filter(|n| n.ends_with("_init.bin")).collect();
    names.sort();
    let (mut games, mut records) = (0, 0);
    for n in names.iter().take(200) {
        let Ok(save) = SaveFile::open(&format!("{dir}/{n}+{dir}/{}", n.replace("_init", "_cur"))) else { continue };
        games += 1;
        for t in save.initial.map.tiles.iter().filter(|t| t.improvement != 0 && t.owner == 0) {
            let rt = save
                .initial
                .improvements
                .iter()
                .find(|r| r.tile == t.index)
                .unwrap_or_else(|| panic!("{n}: no record at {:?}", t.coordinates));
            assert_eq!(*rt, retail_neutral(t.index, t.improvement), "{n}: {:?}", t.coordinates);
            records += 1;
        }
    }
    eprintln!("{games} retail games, {records} neutral records");
}
