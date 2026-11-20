//! Regression tests against maps recorded from the retail game.
//!
//! `tests/data/capture_*.txt` were written from traces of live Large games (Continents, Dryland for c and d, Lakes for e, Archipelago for f, Water World for g, Pangea for h; c is Large, d and e Normal, f Massive, g Huge, h Small),
//! 5-opponent game: the 50 attempt seeds, the attempt the game kept,
//! the fairness score of each attempt (capture b only), and the full tile data of some attempts.
//! The map generator must reproduce every recorded tile, and the retry loop must keep the
//! same attempt.

use polytopia_sim::map::contents::get_inequality;
use polytopia_sim::map::generate::{generate_internal, generate_with_seed_source};
use polytopia_sim::map::shape::create_from_preset;
use polytopia_sim::{Map, MapPreset, MapSize, PlayerState, WorldCoordinates, GAME_VERSION};

struct Capture {
    tribes: Vec<i32>,
    winner: usize,
    seeds: Vec<i32>,
    ineq: Vec<f32>,
    /// (attempt, seed, rows of "terrain climate owner improvement resource")
    maps: Vec<(usize, i32, Vec<[i32; 5]>)>,
}

fn load(text: &str) -> Capture {
    let mut capture = Capture { tribes: vec![], winner: 0, seeds: vec![], ineq: vec![], maps: vec![] };
    let mut lines = text.lines().filter(|l| !l.starts_with('#')).peekable();
    while let Some(line) = lines.next() {
        let mut parts = line.split_whitespace();
        match parts.next() {
            Some("tribes") => capture.tribes = parts.next().unwrap().split(',').map(|v| v.parse().unwrap()).collect(),
            Some("winner") => capture.winner = parts.next().unwrap().parse().unwrap(),
            Some("seeds") => capture.seeds = parts.map(|v| v.parse().unwrap()).collect(),
            Some("ineq") => capture.ineq = parts.map(|v| v.parse().unwrap()).collect(),
            Some("map") => {
                let attempt = parts.next().unwrap().parse().unwrap();
                let seed = parts.next().unwrap().parse().unwrap();
                // Rows run until the next `map` line or the end of the file (16x16 and 18x18 maps).
                let mut rows = Vec::new();
                while let Some(row) = lines.next_if(|l| !l.starts_with("map")) {
                    let v: Vec<i32> = row.split_whitespace().map(|v| v.parse().unwrap()).collect();
                    rows.push([v[0], v[1], v[2], v[3], v[4]]);
                }
                capture.maps.push((attempt, seed, rows));
            }
            _ => {}
        }
    }
    capture
}

/// Players as the game builds them: ids 1..n in seat order, then Nature (id 255).
fn players(tribes: &[i32]) -> Vec<PlayerState> {
    let mut list: Vec<PlayerState> = tribes
        .iter()
        .enumerate()
        .map(|(i, tribe)| PlayerState {
            id: i as u8 + 1,
            handicap: if i == 0 { 1 } else { 0 },
            currency: 0,
            score: 0,
            tribe: *tribe,
            auto_play: i != 0,
            start_tile: WorldCoordinates { x: -1, y: -1 },
        })
        .collect();
    list.push(PlayerState {
        id: 255,
        handicap: 1,
        currency: 0,
        score: 0,
        tribe: 1,
        auto_play: true,
        start_tile: WorldCoordinates { x: -1, y: -1 },
    });
    list
}

fn assert_map(map: &Map, expected: &[[i32; 5]], label: &str) {
    assert_eq!(map.tiles.len(), expected.len(), "{label}: tile count");
    for (tile, want) in map.tiles.iter().zip(expected) {
        let got = [
            tile.terrain,
            tile.climate,
            i32::from(tile.owner),
            if tile.improvement == 0 { -1 } else { tile.improvement },
            if tile.resource == 0 { -1 } else { tile.resource },
        ];
        assert_eq!(
            &got, want,
            "{label}: tile ({}, {}) terrain/climate/owner/improvement/resource",
            tile.coordinates.x, tile.coordinates.y
        );
    }
}

fn check(text: &str, name: &str, preset: MapPreset, size: MapSize) {
    let side = i32::from(size.width());
    let capture = load(text);
    assert_eq!(capture.seeds.len(), 50);
    let settings = create_from_preset(preset);

    // Every recorded attempt: `generate_internal` with that seed reproduces the retail tiles.
    for (attempt, seed, rows) in &capture.maps {
        let mut list = players(&capture.tribes);
        let mut map = Map::new(side as u16, side as u16);
        generate_internal(GAME_VERSION, *seed, &mut map, &settings, &mut list, 0, None);
        assert_map(&map, rows, &format!("{name} attempt {attempt}"));
    }

    // Fairness scores, where the capture has them (players carry over between attempts, as in the game).
    if !capture.ineq.is_empty() {
        let mut list = players(&capture.tribes);
        for (i, seed) in capture.seeds.iter().enumerate() {
            let mut map = Map::new(side as u16, side as u16);
            generate_internal(GAME_VERSION, *seed, &mut map, &settings, &mut list, 0, None);
            let got = get_inequality(&map, &list);
            let want = capture.ineq[i];
            assert!(
                (got - want).abs() <= want.abs() * 1e-5,
                "{name} attempt {}: inequality {got} vs retail {want}",
                i + 1
            );
        }
    }

    // The retry loop keeps the attempt the game kept, and regenerates it as the final map.
    let (_, _, winner_rows) =
        capture.maps.iter().find(|(attempt, _, _)| *attempt == capture.winner).expect("capture stores the winning map");
    let mut list = players(&capture.tribes);
    let mut seeds = capture.seeds.clone().into_iter();
    let map = generate_with_seed_source(GAME_VERSION, size, preset, &mut list, -1, &mut || {
        seeds.next().expect("loop draws at most 50 seeds")
    });
    assert_map(&map, winner_rows, &format!("{name} winner (attempt {})", capture.winner));
}

#[test]
fn capture_a_tribes_13_9_4_12_7_15() {
    check(include_str!("data/capture_a.txt"), "capture a", MapPreset::Continents, MapSize::Large);
}

#[test]
fn capture_b_tribes_13_2_10_15_11_12() {
    check(include_str!("data/capture_b.txt"), "capture b", MapPreset::Continents, MapSize::Large);
}

#[test]
fn capture_c_dryland_tribes_13_9_11_14_6_12() {
    check(include_str!("data/capture_c.txt"), "capture c", MapPreset::Dryland, MapSize::Large);
}

#[test]
fn capture_d_normal_dryland_tribes_13_2_12_7_11_10() {
    check(include_str!("data/capture_d.txt"), "capture d", MapPreset::Dryland, MapSize::Normal);
}

#[test]
fn capture_e_lakes_tribes_13_8_4_15_10_9() {
    check(include_str!("data/capture_e.txt"), "capture e", MapPreset::Lakes, MapSize::Normal);
}

#[test]
fn capture_f_archipelago_massive_tribes_9_6_15_8_10_4() {
    check(include_str!("data/capture_f.txt"), "capture f", MapPreset::Archipelago, MapSize::Massive);
}

#[test]
fn capture_g_water_world_huge_tribes_7_9_6_2_11_14() {
    check(include_str!("data/capture_g.txt"), "capture g", MapPreset::WaterWorld, MapSize::Huge);
}

#[test]
fn capture_h_pangea_small_tribes_14_12_7_13_11_15_4() {
    check(include_str!("data/capture_h.txt"), "capture h", MapPreset::Pangea, MapSize::Small);
}

#[test]
fn capture_i_dryland_tiny_tribes_12_2_13_10_11_8_9_14_4() {
    check(include_str!("data/capture_i.txt"), "capture i", MapPreset::Dryland, MapSize::Tiny);
}

#[test]
fn capture_j_weekly_challenge_tiny_continents() {
    check(include_str!("data/capture_j.txt"), "capture j", MapPreset::Continents, MapSize::Tiny);
}
