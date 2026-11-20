use polytopia_sim::map::generate::generate_internal;
use polytopia_sim::map::shape::create_from_preset;
use polytopia_sim::search::{search, starting_view_from_json, window_at, SearchError, StartingView, CAPITAL_CELLS};
use polytopia_sim::{Map, MapPreset, PlayerState, WorldCoordinates, GAME_VERSION};

const EXAMPLE_VIEW: &str = include_str!("../examples/normal-dryland-9.json");

#[test]
fn example_json_parses_as_a_normal_dryland_view() {
    let view = starting_view_from_json(EXAMPLE_VIEW).unwrap();
    assert_eq!(view.seat, Some(0));
    assert_eq!(view.tribes.len(), 9);
    assert!(view.tribes.iter().all(|tribe| tribe.is_some()));
    assert_eq!((view.capital.x, view.capital.y), (3, 13));
    assert_eq!(view.window.len(), 25);
    let (map, players) = dryland_nine(42);
    assert_eq!(view.capital, players[0].start_tile);
    assert_eq!(
        view.tribes,
        players.iter().filter(|player| player.id != 255).map(|player| Some(player.tribe)).collect::<Vec<_>>()
    );
    assert_eq!(view.window, window_at(&map, view.capital));
}

#[test]
fn four_unknown_tribes_are_rejected() {
    let mut tribes = vec![Some(2); 9];
    tribes[5] = None;
    tribes[6] = None;
    tribes[7] = None;
    tribes[8] = None;
    let view = StartingView { seat: Some(0), tribes, capital: WorldCoordinates { x: 3, y: 3 }, window: vec![None; 25] };
    let error = search(&view, 0..1).unwrap_err();
    assert!(matches!(error, SearchError::TooManyUnknownTribes { hidden: 4 }));
    assert!(error.to_string().contains("too many unknown tribes"));
}

#[test]
fn round_trip_finds_the_generated_seed() {
    let seed = 42;
    let (map, players) = dryland_nine(seed);
    let capital = players[0].start_tile;
    assert!(CAPITAL_CELLS.contains(&(capital.x, capital.y)));
    assert!(map.tiles.iter().any(|tile| tile.climate != players[0].tribe));

    let view = StartingView {
        seat: Some(0),
        tribes: players.iter().filter(|player| player.id != 255).map(|player| Some(player.tribe)).collect(),
        capital,
        window: window_at(&map, capital),
    };
    let hits = search(&view, 40..50).expect("lineup is fully known");
    assert!(hits.contains(&seed), "hits in 40..50: {hits:?}");
}

#[test]
fn one_hidden_tribe_still_returns_the_seed() {
    let seed = 42;
    let (map, players) = dryland_nine(seed);
    let capital = players[0].start_tile;
    let mut tribes: Vec<Option<i32>> =
        players.iter().filter(|player| player.id != 255).map(|player| Some(player.tribe)).collect();
    tribes[8] = None;
    let view = StartingView { seat: Some(0), tribes, capital, window: window_at(&map, capital) };
    let hits = search(&view, seed..seed + 1).expect("one hidden tribe is searchable");
    assert_eq!(hits, vec![seed]);
}

#[test]
#[ignore = "retail Normal Dryland 9-player capture is not available"]
fn same_seed_matches_captured_map() {}

fn dryland_nine(seed: i32) -> (Map, Vec<PlayerState>) {
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
    let settings = create_from_preset(MapPreset::Dryland);
    let mut map = Map::new(16, 16);
    generate_internal(GAME_VERSION, seed, &mut map, &settings, &mut players, 0, None);
    (map, players)
}

#[test]
fn null_tile_fields_parse_as_wildcards() {
    use polytopia_sim::search::WILDCARD;
    let mut text = EXAMPLE_VIEW.to_string();
    let first = text.find("\"terrain\"").expect("example has tiles");
    let end = text[first..].find('}').unwrap() + first;
    text.replace_range(first..end, "\"terrain\": 3, \"resource\": null, \"climate\": null ");
    let view = starting_view_from_json(&text).unwrap();
    let tile = view.window[0].as_ref().unwrap();
    assert_eq!((tile.terrain, tile.resource, tile.climate), (3, WILDCARD, WILDCARD));
}

#[test]
fn wildcard_fields_keep_the_true_seed_and_a_wrong_field_rejects_it() {
    use polytopia_sim::search::WILDCARD;
    let (map, players) = dryland_nine(42);
    let mut view = StartingView {
        seat: Some(0),
        tribes: players.iter().filter(|p| p.id != 255).map(|p| Some(p.tribe)).collect(),
        capital: players[0].start_tile,
        window: window_at(&map, players[0].start_tile),
    };
    // blank one field of three different tiles: the true seed must still match
    for (k, field) in [(0usize, 0), (6, 1), (12, 2)] {
        if let Some(tile) = view.window[k].as_mut() {
            match field {
                0 => tile.terrain = WILDCARD,
                1 => tile.resource = WILDCARD,
                _ => tile.climate = WILDCARD,
            }
        }
    }
    assert!(search(&view, 40..45).unwrap().contains(&42));
    // a confidently wrong value (not a wildcard) rejects it
    let mut wrong = view.clone();
    if let Some(tile) = wrong.window[8].as_mut() {
        tile.terrain = if tile.terrain == 3 { 5 } else { 3 };
    }
    assert!(!search(&wrong, 40..45).unwrap().contains(&42));
}
