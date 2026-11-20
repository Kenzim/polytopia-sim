//! The resign command (raw 35): the resigning player goes on auto-play and keeps the turn and command index it resigned
//! at; the winner check then runs, so resigning the last opponent of a multiplayer game sends it to `FinalTurn`.
use polytopia_sim::engine::Engine;
use polytopia_sim::json::Json;
use polytopia_sim::GameStatus;

/// Player 1 (to move) holds a city and a warrior; player 2 only has a city when `second_city` is set.
fn board(second_city: bool) -> Engine {
    let mut tiles = Vec::new();
    for y in 0..3 {
        for x in 0..6 {
            let mut t = format!(r#""x":{x},"y":{y},"terrain":3,"climate":2"#);
            match (x, y) {
                (0, 0) => t.push_str(r#","city":true,"capital":true,"owner":1,"ruling":[0,0]"#),
                (5, 2) if second_city => t.push_str(r#","city":true,"capital":true,"owner":2,"ruling":[5,2]"#),
                _ => {}
            }
            tiles.push(format!("{{{t}}}"));
        }
    }
    let city2 = if second_city { r#",{"x":5,"y":2,"level":1}"# } else { "" };
    let text = format!(
        r#"{{"format":"polytopia-position","game":{{"turn":9,"width":6,"height":3,"current_player":1}},
           "players":[{{"id":1,"tribe":3,"human":true,"stars":5}},{{"id":2,"tribe":3,"human":true,"stars":5}}],
           "tiles":[{}],
           "improvements":[{{"x":0,"y":0,"level":1}}{city2}],
           "units":[]}}"#,
        tiles.join(",")
    );
    let mut e = Engine::from_position(&Json::parse(&text).unwrap()).unwrap();
    e.s.settings.game_type = 1;
    e
}

#[test]
fn resigning_puts_the_player_on_auto_play_with_the_turn_and_command() {
    let mut e = board(true);
    let command = e.s.current_command;
    e.resign(2).unwrap();
    assert!(e.s.players.iter().find(|p| p.id == 2).unwrap().auto_play);
    assert!(!e.s.players.iter().find(|p| p.id == 1).unwrap().auto_play);
    let rt = e.s.player_runtime.iter().find(|r| r.player == 2).unwrap();
    assert_eq!((rt.resigned_turn, rt.resigned_at_command), (9, i32::from(command)));
    assert_eq!(e.s.current_command, command + 1);
}

#[test]
fn resigning_the_last_opponent_ends_a_multiplayer_game() {
    let mut e = board(false);
    assert_eq!(e.s.current_state, GameStatus::Started);
    e.resign(2).unwrap();
    assert_eq!(e.s.current_state, GameStatus::FinalTurn);
}

#[test]
fn resigning_an_unknown_player_changes_nothing() {
    let mut e = board(true);
    let command = e.s.current_command;
    assert!(e.resign(7).is_err());
    assert_eq!(e.s.current_command, command);
}
