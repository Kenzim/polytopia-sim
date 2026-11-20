//! Who wins a finished game with several players alive: Domination ranks them by cities held and then by score,
//! every other mode except Might by score alone.
//!
//! The board is a hand-made 12x12 position with three players; `cities` gives how many cities each one owns.

use polytopia_sim::engine::Engine;
use polytopia_sim::json::Json;
use polytopia_sim::{GameStatus, GAME_MODE_DOMINATION, GAME_MODE_GLORY, GAME_MODE_PERFECTION};

/// A finished game in which players 1 to 3 own `cities[0..3]` cities and have the given stored `scores`.
fn finished(cities: [usize; 3], scores: [u32; 3], mode: i32) -> Engine {
    let mut tiles = Vec::new();
    for y in 0..12 {
        for x in 0..12 {
            let mut t = format!(r#""x":{x},"y":{y},"terrain":3,"climate":2"#);
            // player n's cities run along row 3n, the first being the capital
            for (i, &count) in cities.iter().enumerate() {
                if y == 3 * i + 1 && x % 2 == 0 && x / 2 < count {
                    t.push_str(&format!(r#","city":true,"capital":{},"owner":{}"#, x == 0, i + 1));
                }
            }
            tiles.push(format!("{{{t}}}"));
        }
    }
    let text = format!(
        r#"{{"format":"polytopia-position","game":{{"turn":9,"width":12,"height":12,"current_player":1}},
           "players":[{{"id":1,"tribe":2}},{{"id":2,"tribe":3}},{{"id":3,"tribe":4}}],
           "tiles":[{}],"units":[]}}"#,
        tiles.join(",")
    );
    let mut e = Engine::from_position(&Json::parse(&text).unwrap()).unwrap();
    e.s.settings.set_game_mode(mode);
    for (i, score) in scores.into_iter().enumerate() {
        e.player_mut(i as u8 + 1).unwrap().score = score;
    }
    e.s.current_state = GameStatus::Ended;
    e
}

#[test]
fn domination_ranks_by_cities_before_score() {
    let e = finished([1, 3, 2], [900, 300, 600], GAME_MODE_DOMINATION);
    assert_eq!((1..=3).map(|p| e.recorded_city_count(p)).collect::<Vec<_>>(), vec![1, 3, 2]);
    assert_eq!(e.game_winners(), vec![2], "the most cities wins although player 1 has the best score");
}

#[test]
fn domination_breaks_a_city_tie_by_score() {
    let e = finished([2, 2, 1], [400, 450, 900], GAME_MODE_DOMINATION);
    assert_eq!(
        e.game_winners(),
        vec![2],
        "players 1 and 2 hold two cities each; player 2 has more score, player 3 fewer cities"
    );
    let e = finished([2, 2, 1], [450, 400, 900], GAME_MODE_DOMINATION);
    assert_eq!(e.game_winners(), vec![1]);
}

#[test]
fn other_modes_rank_by_score_alone() {
    for mode in [GAME_MODE_PERFECTION, GAME_MODE_GLORY] {
        let e = finished([1, 3, 2], [900, 300, 600], mode);
        assert_eq!(e.game_winners(), vec![1], "mode {mode}");
    }
}
