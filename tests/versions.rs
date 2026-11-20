//! Game versions against the code and the README.
//!
//! * every supported version starts a game and plays a few turns (the evidence behind the amber light);
//! * the regression replays, when present, cover exactly the tested versions (the evidence behind the green light);
//! * the traffic-light and rules-data tables in `README.md` match `support_level` and `RULES_LADDER`.

use polytopia_sim::engine::replay::SaveFile;
use polytopia_sim::engine::{Command, Engine, GameOptions, PlayerSetup};
use polytopia_sim::{
    support_level, BotDifficulty, GameVersion, MapPreset, MapSize, Support, GAME_VERSION, MAX_TESTED_GAME_VERSION,
    MIN_SUPPORTED_GAME_VERSION, MIN_TESTED_GAME_VERSION, RULES_LADDER,
};
use std::collections::BTreeSet;

/// The 12 normal tribes, Cymanti, Polaris, Elyrion and Aquarion.
const TRIBES: [i32; 16] = [2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17];

#[test]
fn every_supported_version_starts_and_plays_a_few_turns() {
    for version in GameVersion::supported() {
        let n = version.as_i32();
        let first = ((n * 3) as usize) % TRIBES.len();
        let second = (first + 1 + (n as usize) % 5) % TRIBES.len();
        let opts = GameOptions {
            version: n,
            seed: n,
            size: MapSize::Tiny,
            preset: MapPreset::Continents,
            players: vec![
                PlayerSetup { tribe: TRIBES[first], human: false },
                PlayerSetup { tribe: TRIBES[second], human: false },
            ],
            difficulty: BotDifficulty::Normal,
            ..GameOptions::default()
        };
        let mut e = Engine::new_game(&opts);
        assert_eq!(e.s.version, n);
        let mut rng = n as u64 * 2862933555777941757 + 3037000493;
        let mut next = || {
            rng ^= rng << 13;
            rng ^= rng >> 7;
            rng ^= rng << 17;
            rng
        };
        let mut commands = 0;
        while !e.is_over() && e.s.current_turn < 4 && commands < 3000 {
            let legal = e.legal_commands();
            assert!(!legal.is_empty(), "game version {n}: no legal command at turn {}", e.s.current_turn);
            let non_end: Vec<&Command> = legal.iter().filter(|c| !matches!(c, Command::EndTurn)).collect();
            let cmd = if !non_end.is_empty() && next() % 8 != 0 {
                *non_end[(next() % non_end.len() as u64) as usize]
            } else {
                Command::EndTurn
            };
            e.apply_unchecked(cmd);
            commands += 1;
        }
        assert!(
            e.is_over() || e.s.current_turn >= 4,
            "game version {n} stalled at turn {} after {commands} commands",
            e.s.current_turn
        );
    }
}

fn regress_dir() -> String {
    std::env::var("POLYTOPIA_REGRESS").unwrap_or_default()
}

/// Version of every regression game: `(name, version)`.
fn regress_versions() -> Vec<(String, i32)> {
    let dir = regress_dir();
    let Ok(read) = std::fs::read_dir(&dir) else { return Vec::new() };
    let mut names: Vec<String> =
        read.filter_map(|d| d.ok()).map(|d| d.file_name().to_string_lossy().into_owned()).collect();
    names.sort();
    let mut out = Vec::new();
    for n in &names {
        let path = if n.ends_with(".state") {
            format!("{dir}/{n}")
        } else if let Some(stem) = n.strip_suffix("_init.bin") {
            format!("{dir}/{stem}_init.bin+{dir}/{stem}_cur.bin")
        } else {
            continue;
        };
        let save = SaveFile::open(&path).unwrap_or_else(|e| panic!("{path}: {e}"));
        out.push((n.clone(), save.initial.version));
    }
    out
}

/// Needs the regression data (not in git); skips with a note when it is missing.
#[test]
fn regression_replays_cover_exactly_the_tested_versions() {
    let games = regress_versions();
    if games.is_empty() {
        eprintln!("skipped: no regression data in {}", regress_dir());
        return;
    }
    let seen: BTreeSet<i32> = games.iter().map(|(_, v)| *v).collect();
    let tested: BTreeSet<i32> =
        (MIN_SUPPORTED_GAME_VERSION..=GAME_VERSION).filter(|&v| support_level(v) == Support::Tested).collect();
    assert_eq!(seen, tested, "regression game versions {games:?} against the green versions");
    assert_eq!(*tested.iter().next().unwrap(), MIN_TESTED_GAME_VERSION);
}

/// The map captures in `tests/data` are all of the newest tested game version (123). Version 124 changed no map
/// generation rule, so they stand for it too.
#[test]
fn map_captures_are_of_the_newest_tested_version() {
    let dir = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/data");
    let mut checked = 0;
    for entry in std::fs::read_dir(dir).unwrap().flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        if !(name.starts_with("capture_") && name.ends_with(".txt")) {
            continue;
        }
        let text = std::fs::read_to_string(entry.path()).unwrap();
        let header = text.lines().next().unwrap();
        assert!(header.contains(&format!("version {MAX_TESTED_GAME_VERSION}")), "{name}: {header}");
        checked += 1;
    }
    assert_eq!(checked, 10);
}

/// Table rows of the README section under `heading`: the cells of each `|` line after the header and rule rows.
fn readme_table(heading: &str) -> Vec<Vec<String>> {
    let text = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/README.md")).expect("README.md");
    let mut lines = text.lines().skip_while(|l| l.trim() != heading);
    assert!(lines.next().is_some(), "README.md has no heading {heading:?}");
    let rows: Vec<Vec<String>> = lines
        .skip_while(|l| !l.starts_with('|'))
        .take_while(|l| l.starts_with('|'))
        .skip(2)
        .map(|l| l.trim().trim_matches('|').split('|').map(|c| c.trim().to_string()).collect())
        .collect();
    assert!(!rows.is_empty(), "no table under {heading:?}");
    rows
}

/// `a..=b`, `a`, `below a` or `above b`, in backticks or not, as an inclusive range over `i32`.
fn parse_range(cell: &str) -> (i32, i32) {
    let cell = cell.trim_matches('`');
    if let Some(n) = cell.strip_prefix("below ") {
        return (i32::MIN, n.trim().parse::<i32>().unwrap() - 1);
    }
    if let Some(n) = cell.strip_prefix("above ") {
        return (n.trim().parse::<i32>().unwrap() + 1, i32::MAX);
    }
    match cell.split_once("..=") {
        Some((a, b)) => (a.trim().parse().unwrap(), b.trim().parse().unwrap()),
        None => {
            let n = cell.parse().unwrap_or_else(|_| panic!("cannot read game versions {cell:?}"));
            (n, n)
        }
    }
}

#[test]
fn readme_traffic_light_matches_the_code() {
    let rows = readme_table("## Supported game versions");
    let mut next = i32::MIN;
    let mut done = false;
    for row in &rows {
        assert!(!done, "rows after the last range: {row:?}");
        let (first, last) = parse_range(&row[0]);
        assert_eq!(first, next, "ranges must follow each other without gaps, at {row:?}");
        let want = match row[1].as_str() {
            "green" => Support::Tested,
            "amber" => Support::Supported,
            "red" => Support::Unsupported,
            other => panic!("unknown light {other:?}"),
        };
        // Every version of a small range; the edges and a few inner points of a range that reaches i32::MIN or i32::MAX.
        let probes: Vec<i32> = if i64::from(last) - i64::from(first) < 400 {
            (first..=last).collect()
        } else {
            vec![first, first + 1, last - 1, last, 0, 1000]
        };
        for v in probes.into_iter().filter(|v| (first..=last).contains(v)) {
            assert_eq!(
                support_level(v),
                want,
                "game version {v} is {:?} in the code, README says {}",
                support_level(v),
                row[1]
            );
        }
        done = last == i32::MAX;
        next = last.wrapping_add(1);
    }
    assert!(done, "the table must run up to the newest versions");
}

#[test]
fn readme_rules_table_matches_the_ladder() {
    let rows = readme_table("### Rules data per game version");
    let mut expected = Vec::new();
    for (i, step) in RULES_LADDER.iter().enumerate() {
        let end = RULES_LADDER.get(i + 1).map_or(i32::MAX, |n| n.first_game_version - 1);
        let first = step.first_game_version.max(MIN_SUPPORTED_GAME_VERSION);
        let last = end.min(GAME_VERSION);
        if first <= last {
            expected.push((first, last, step.rules_version));
        }
    }
    let readme: Vec<(i32, i32, i32)> = rows
        .iter()
        .map(|row| {
            let (first, last) = parse_range(&row[0]);
            (
                first,
                last,
                row[1]
                    .trim_matches('`')
                    .trim_start_matches('v')
                    .parse()
                    .unwrap_or_else(|_| panic!("rules file {:?}", row[1])),
            )
        })
        .collect();
    assert_eq!(readme, expected);
}
