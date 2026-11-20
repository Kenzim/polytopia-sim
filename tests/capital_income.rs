//! A human player's original capital makes 2 stars a turn at the start, as in the game: city level 1 plus the
//! human handicap. Every human seat in the retail regression saves has handicap 1.

use polytopia_sim::engine::{Engine, GameOptions, PlayerSetup};
use polytopia_sim::json::Json;
use polytopia_sim::{BotDifficulty, HUMAN_HANDICAP};

fn capital_income(e: &Engine, player: u8) -> i32 {
    let start = e.player(player).unwrap().start_tile;
    e.tile_work(start)
}

#[test]
fn a_new_human_capital_makes_two_stars() {
    for difficulty in [BotDifficulty::Easy, BotDifficulty::Normal, BotDifficulty::Hard, BotDifficulty::Crazy] {
        let opts = GameOptions {
            players: vec![PlayerSetup { tribe: 2, human: true }, PlayerSetup { tribe: 3, human: false }],
            difficulty,
            ..GameOptions::default()
        };
        let e = Engine::new_game(&opts);
        assert_eq!(e.player(1).unwrap().handicap, HUMAN_HANDICAP);
        assert_eq!(capital_income(&e, 1), 2, "human capital at {difficulty:?}");
        assert_eq!(e.income(1), 2);
    }
}

#[test]
fn bot_capital_income_follows_the_difficulty() {
    for (difficulty, stars) in
        [(BotDifficulty::Easy, 1), (BotDifficulty::Normal, 2), (BotDifficulty::Hard, 3), (BotDifficulty::Crazy, 5)]
    {
        let opts = GameOptions {
            players: vec![PlayerSetup { tribe: 2, human: true }, PlayerSetup { tribe: 3, human: false }],
            difficulty,
            ..GameOptions::default()
        };
        let e = Engine::new_game(&opts);
        assert_eq!(capital_income(&e, 2), stars, "bot capital at {difficulty:?}");
    }
}

#[test]
fn a_position_without_a_handicap_gives_humans_the_human_one() {
    let text = r#"{"format":"polytopia-position","game":{"turn":1,"width":4,"height":4,"current_player":1},
        "players":[{"id":1,"tribe":2,"human":true},{"id":2,"tribe":3},{"id":3,"tribe":4,"handicap":4}],
        "tiles":[{"x":0,"y":0,"terrain":3,"climate":2,"city":true,"capital":true,"owner":1},
                 {"x":3,"y":3,"terrain":3,"climate":3,"city":true,"capital":true,"owner":2},
                 {"x":0,"y":3,"terrain":3,"climate":4,"city":true,"capital":true,"owner":3}]}"#;
    let e = Engine::from_position(&Json::parse(text).unwrap()).unwrap();
    assert_eq!(e.player(1).unwrap().handicap, HUMAN_HANDICAP);
    assert_eq!(e.player(2).unwrap().handicap, 0);
    assert_eq!(e.player(3).unwrap().handicap, 4);
    assert_eq!(capital_income(&e, 1), 2);
}
