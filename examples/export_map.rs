//! Write a generated map as JSON (terrain, climate, resources and owners per tile).
//!
//! ```text
//! cargo run --release --example export_map -- <seed> <preset> <size> <tribes-csv>   # preset: dry|lakes|cont|arch|ww|pangea
//! ```
use polytopia_sim::map::generate::generate_internal;
use polytopia_sim::map::shape::create_from_preset;
use polytopia_sim::{Map, MapPreset, PlayerState, WorldCoordinates, GAME_VERSION};

fn main() {
    let a: Vec<String> = std::env::args().collect();
    let seed: i32 = a[1].parse().unwrap();
    let preset = match a[2].as_str() {
        "dry" => MapPreset::Dryland,
        "lakes" => MapPreset::Lakes,
        "cont" => MapPreset::Continents,
        "arch" => MapPreset::Archipelago,
        "ww" => MapPreset::WaterWorld,
        _ => MapPreset::Pangea,
    };
    let w: u16 = a[3].parse().unwrap();
    let tribes: Vec<i32> = a[4].split(',').map(|s| s.parse().unwrap()).collect();
    let mut players: Vec<PlayerState> = tribes
        .iter()
        .enumerate()
        .map(|(i, t)| PlayerState {
            id: i as u8 + 1,
            handicap: if i == 0 { 1 } else { 0 },
            currency: 0,
            score: 0,
            tribe: *t,
            auto_play: i != 0,
            start_tile: WorldCoordinates { x: -1, y: -1 },
        })
        .collect();
    // the game adds Nature (id 255) after the seats; it changes the random draws
    players.push(PlayerState {
        id: 255,
        handicap: 1,
        currency: 0,
        score: 0,
        tribe: 1,
        auto_play: true,
        start_tile: WorldCoordinates { x: -1, y: -1 },
    });
    let mut map = Map::new(w, w);
    generate_internal(GAME_VERSION, seed, &mut map, &create_from_preset(preset), &mut players, 0, None);
    let tiles: Vec<String> = map
        .tiles
        .iter()
        .map(|t| {
            format!(
                "{{\"x\":{},\"y\":{},\"terrain\":{},\"climate\":{},\"resource\":{},\"improvement\":{},\"owner\":{},\"capital\":{},\"city\":{}}}",
                t.coordinates.x, t.coordinates.y, t.terrain, t.climate, t.resource, t.improvement, t.owner, t.is_capital, t.is_city
            )
        })
        .collect();
    let starts: Vec<String> = players
        .iter()
        .filter(|p| p.id != 255)
        .map(|p| format!("{{\"id\":{},\"tribe\":{},\"x\":{},\"y\":{}}}", p.id, p.tribe, p.start_tile.x, p.start_tile.y))
        .collect();
    println!("{{\"size\":{},\"players\":[{}],\"tiles\":[{}]}}", w, starts.join(","), tiles.join(","));
}
