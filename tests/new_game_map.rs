//! `Engine::new_game` must generate the same map as the generator called the way the game calls it: seats 1..n in
//! order, then Nature (255) last in the list. Without Nature the climate pass gives the last seat no region.

use polytopia_sim::map::generate::generate;
use polytopia_sim::{
    Engine, GameOptions, MapPreset, MapSize, NewGameError, PlayerSetup, PlayerState, WorldCoordinates, NATURE,
};

const PRESETS: [MapPreset; 6] = [
    MapPreset::Dryland,
    MapPreset::Lakes,
    MapPreset::Continents,
    MapPreset::Archipelago,
    MapPreset::WaterWorld,
    MapPreset::Pangea,
];
const SIZES: [MapSize; 6] =
    [MapSize::Tiny, MapSize::Small, MapSize::Normal, MapSize::Large, MapSize::Huge, MapSize::Massive];

/// The 12 normal tribes, cycled.
fn tribe(i: usize) -> i32 {
    2 + (i % 12) as i32
}

fn options(seed: i32, size: MapSize, preset: MapPreset, count: usize) -> GameOptions {
    GameOptions {
        seed,
        size,
        preset,
        players: (0..count).map(|i| PlayerSetup { tribe: tribe(i), human: i == 0 }).collect(),
        ..GameOptions::default()
    }
}

fn seat(id: u8, tribe: i32) -> PlayerState {
    PlayerState {
        id,
        handicap: 0,
        currency: 0,
        score: 0,
        tribe,
        auto_play: true,
        start_tile: WorldCoordinates { x: -1, y: -1 },
    }
}

fn generated(opts: &GameOptions, with_nature: bool) -> polytopia_sim::Map {
    let mut list: Vec<PlayerState> = opts.players.iter().enumerate().map(|(i, p)| seat(i as u8 + 1, p.tribe)).collect();
    if with_nature {
        list.push(seat(NATURE, 1));
    }
    generate(opts.version, opts.seed, opts.size, opts.preset, &mut list, 0)
}

/// Every tile field the generator writes (`finish_setup` adds the ruling city afterwards).
fn same_tiles(a: &polytopia_sim::Map, b: &polytopia_sim::Map) -> bool {
    a.width == b.width
        && a.tiles.len() == b.tiles.len()
        && a.tiles.iter().zip(&b.tiles).all(|(x, y)| {
            let (mut x, mut y) = (*x, *y);
            x.ruling_city = WorldCoordinates { x: -1, y: -1 };
            y.ruling_city = WorldCoordinates { x: -1, y: -1 };
            x == y
        })
}

fn cases() -> Vec<GameOptions> {
    let mut out = Vec::new();
    let mut seed = 7;
    for (i, preset) in PRESETS.iter().enumerate() {
        for (j, size) in SIZES.iter().enumerate() {
            let count = 2 + (i + j) % size.max_players().min(8).saturating_sub(1);
            out.push(options(seed, *size, *preset, count));
            seed = seed.wrapping_mul(1_103_515_245).wrapping_add(12_345) & 0x3fff_ffff;
        }
    }
    out
}

#[test]
fn new_game_map_matches_the_generator_called_with_nature() {
    let mut differs_without_nature = 0;
    for opts in cases() {
        let e = Engine::new_game(&opts);
        let label = format!("seed {} {:?} {:?} {} players", opts.seed, opts.size, opts.preset, opts.players.len());
        assert!(same_tiles(&e.s.map, &generated(&opts, true)), "{label}: map differs from the generator with Nature");
        if !same_tiles(&e.s.map, &generated(&opts, false)) {
            differs_without_nature += 1;
        }

        // Seat ids and order unchanged; Nature last with no capital or start tile.
        let ids: Vec<u8> = e.s.players.iter().map(|p| p.id).collect();
        let mut want: Vec<u8> = (1..=opts.players.len() as u8).collect();
        want.push(NATURE);
        assert_eq!(ids, want, "{label}: player ids");
        let nature = e.s.players.last().unwrap();
        assert_eq!(nature.start_tile, WorldCoordinates { x: -1, y: -1 }, "{label}: Nature start tile");
        assert!(
            !e.s.map.tiles.iter().any(|t| t.owner == NATURE || t.capital_of == NATURE),
            "{label}: Nature owns a tile"
        );
        for p in &e.s.players[..opts.players.len()] {
            assert!(p.start_tile.x >= 0, "{label}: player {} has no start tile", p.id);
            assert!(
                e.s.map.tiles.iter().any(|t| t.is_capital && t.owner == p.id),
                "{label}: player {} has no capital",
                p.id
            );
        }
    }
    // The comparison must be able to see the bug: leaving Nature out changes the map.
    assert!(differs_without_nature > 0, "no case tells the two player lists apart");
}

#[test]
fn try_new_game_refuses_what_the_size_cannot_hold() {
    let limits: Vec<usize> = SIZES.iter().map(|s| s.max_players()).collect();
    assert_eq!(limits, [9, 16, 16, 25, 36, 81]);
    for size in SIZES {
        let max = size.max_players();
        // At the limit every seed generates.
        for seed in 0..4 {
            assert!(
                Engine::try_new_game(&options(seed, size, MapPreset::Continents, max)).is_ok(),
                "{size:?}: {max} players"
            );
        }
        let err = Engine::try_new_game(&options(3, size, MapPreset::Continents, max + 1)).err();
        assert_eq!(err, Some(NewGameError::TooManyPlayers { size, requested: max + 1, max }));
    }
    assert_eq!(
        Engine::try_new_game(&options(3, MapSize::Tiny, MapPreset::Lakes, 0)).err(),
        Some(NewGameError::NoPlayers)
    );
}

/// One past the limit, the generator itself fails on some seed, so the limit is not stricter than it must be.
#[test]
fn one_past_the_limit_the_generator_fails() {
    for size in SIZES {
        let n = size.max_players() + 1;
        let fails = (0..40).any(|seed| {
            let opts = options(seed, size, MapPreset::Continents, n);
            std::panic::catch_unwind(|| generated(&opts, true)).is_err()
        });
        assert!(fails, "{size:?}: {n} players generated on every seed tried");
    }
}
