use polytopia_sim::map::{self, shape};
use polytopia_sim::{MapPreset, MapSize, PlayerState, WorldCoordinates};

/// Capital indexes from this port for seed 42, `MapSize::Normal` (width 16),
/// `MapPreset::Continents`, and 5 players.
///
/// This checks the ported generator. It is not a captured retail map.
#[test]
fn same_seed_capital_indexes() {
    let preset = MapPreset::Continents;
    let settings = shape::create_from_preset(preset);
    assert_eq!(settings.map_type, preset);
    assert_eq!(settings.wetness.to_bits(), 0x3f000000);
    assert_eq!(settings.equality_iterations, 50);

    let width = i32::from(MapSize::Normal.width());
    let mut rng = polytopia_sim::rng::DotNetRandom::new(42);
    let capitals = shape::generate_player_capital_positions(&mut rng, width, 5);
    assert_eq!(capitals, vec![211, 56, 61, 216, 51]);
}

// Retail comparisons live in `tests/map_capture.rs` (maps recorded from a live game).

/// `generate` for versions 98 and 123 returns a map of the requested size.
///
/// Version 98 does not take the island attempt. This does not compare terrain
/// with the retail game.
#[test]
fn generate_version_98_and_123_return_the_requested_size() {
    let size = MapSize::Normal;
    let side = size.width();
    let tiles = usize::from(side) * usize::from(side);

    assert!(!map::generate::tries_island_generator(98, MapPreset::Continents));
    assert!(!map::generate::tries_island_generator(98, MapPreset::Pangea));
    assert!(map::generate::tries_island_generator(123, MapPreset::Continents));
    assert!(!map::generate::tries_island_generator(123, MapPreset::Lakes));

    let mut players = five_players();
    let older = map::generate::generate(98, 42, size, MapPreset::Continents, &mut players, -1);
    assert_eq!(older.width, side);
    assert_eq!(older.height, side);
    assert_eq!(older.tiles.len(), tiles);

    let mut players = five_players();
    let newer = map::generate::generate(123, 42, size, MapPreset::Continents, &mut players, -1);
    assert_eq!(newer.width, side);
    assert_eq!(newer.height, side);
    assert_eq!(newer.tiles.len(), tiles);
}

fn five_players() -> Vec<PlayerState> {
    (1..=5)
        .map(|id| PlayerState {
            id,
            handicap: 0,
            currency: 0,
            score: 0,
            tribe: i32::from(id),
            auto_play: false,
            start_tile: WorldCoordinates { x: 0, y: 0 },
        })
        .collect()
}
