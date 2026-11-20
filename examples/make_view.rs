//! Print a StartingView JSON for a generated Normal Dryland 9-player map.
//!
//! ```text
//! cargo run --release --example make_view [seed]              # default seed 42
//! ```
use polytopia_sim::map::generate::generate_internal;
use polytopia_sim::map::shape::create_from_preset;
use polytopia_sim::search::window_at;
use polytopia_sim::{Map, MapPreset, PlayerState, WorldCoordinates, GAME_VERSION};
fn main() {
    let seed: i32 = std::env::args().nth(1).map(|s| s.parse().unwrap()).unwrap_or(42);
    let mut players: Vec<PlayerState> = (0..9)
        .map(|seat| PlayerState {
            id: seat as u8 + 1,
            handicap: if seat == 0 { 1 } else { 0 },
            currency: 0,
            score: 0,
            tribe: seat + 2,
            auto_play: seat != 0,
            start_tile: WorldCoordinates { x: -1, y: -1 },
        })
        .collect();
    players.push(PlayerState {
        id: 255,
        handicap: 1,
        currency: 0,
        score: 0,
        tribe: 1,
        auto_play: true,
        start_tile: WorldCoordinates { x: -1, y: -1 },
    });
    let mut map = Map::new(16, 16);
    generate_internal(GAME_VERSION, seed, &mut map, &create_from_preset(MapPreset::Dryland), &mut players, 0, None);
    let capital = players[0].start_tile;
    println!("{{\n  \"width\": 16,\n  \"players\": 9,\n  \"preset\": \"dryland\",\n  \"seat\": 0,\n  \"tribes\": [2, 3, 4, 5, 6, 7, 8, 9, 10],\n  \"capital\": {{ \"x\": {}, \"y\": {} }},\n  \"window\": [", capital.x, capital.y);
    let window = window_at(&map, capital);
    for (i, t) in window.iter().enumerate() {
        let t = t.expect("capital cells are inside the map");
        println!(
            "    {{ \"terrain\": {}, \"resource\": {}, \"climate\": {} }}{}",
            t.terrain,
            t.resource,
            t.climate,
            if i + 1 < window.len() { "," } else { "" }
        );
    }
    println!("  ]\n}}");
}
