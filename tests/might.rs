//! Might mode: the game ends when one player holds every capital.
//!
//! The board is a hand-made 10x10 position with three players. Each has a capital; the `extra` list gives the
//! players who also own a second city, so that losing a capital does not wipe them out.

use polytopia_sim::engine::{Command, Engine, GameOptions, PlayerSetup};
use polytopia_sim::json::Json;
use polytopia_sim::{
    GameStatus, MapPreset, MapSize, WorldCoordinates as Wc, GAME_MODE_DOMINATION, GAME_MODE_MIGHT, GAME_MODE_PERFECTION,
};

const C1: Wc = Wc { x: 1, y: 1 };
const C2: Wc = Wc { x: 8, y: 1 };
const C3: Wc = Wc { x: 4, y: 8 };
/// Second cities, by player.
const S1: Wc = Wc { x: 1, y: 4 };
const S2: Wc = Wc { x: 8, y: 4 };
const S3: Wc = Wc { x: 7, y: 8 };

/// Player 1 (human) to move, with a warrior standing on each enemy capital, ready to capture it.
fn board(extra: &[u8]) -> Engine {
    let cities = [(C1, 1u8, true), (C2, 2, true), (C3, 3, true), (S1, 1, false), (S2, 2, false), (S3, 3, false)];
    let mut tiles = Vec::new();
    for y in 0..10 {
        for x in 0..10 {
            let mut t = format!(r#""x":{x},"y":{y},"terrain":3,"climate":2"#);
            if let Some((_, owner, capital)) =
                cities.iter().find(|(c, owner, capital)| c.x == x && c.y == y && (*capital || extra.contains(owner)))
            {
                t.push_str(&format!(r#","city":true,"capital":{capital},"owner":{owner}"#));
            }
            tiles.push(format!("{{{t}}}"));
        }
    }
    let text = format!(
        r#"{{"format":"polytopia-position","game":{{"turn":2,"width":10,"height":10,"current_player":1}},
           "players":[{{"id":1,"tribe":2,"human":true}},{{"id":2,"tribe":3}},{{"id":3,"tribe":4}}],
           "tiles":[{}],
           "units":[{{"type":2,"owner":1,"x":{},"y":{}}},{{"type":2,"owner":1,"x":{},"y":{}}}]}}"#,
        tiles.join(","),
        C2.x,
        C2.y,
        C3.x,
        C3.y
    );
    Engine::from_position(&Json::parse(&text).unwrap()).unwrap()
}

fn might(extra: &[u8]) -> Engine {
    let mut e = board(extra);
    e.s.settings.set_game_mode(GAME_MODE_MIGHT);
    e
}

fn unit_on(e: &Engine, at: Wc) -> u32 {
    e.s.units.iter().find(|u| u.coordinates == at && u.owner == 1).unwrap().id
}

fn capture(e: &mut Engine, at: Wc) {
    let unit = unit_on(e, at);
    e.apply(Command::Capture { unit, at }).expect("capture is legal");
}

/// The rules have ended the game: it is in `FinalTurn` (retail's game-over action), still being played, until the
/// end-match command, which is `Command::EndTurn` there, ends it.
fn end_match(e: &mut Engine) {
    assert_eq!(e.s.current_state, GameStatus::FinalTurn);
    assert!(!e.is_over(), "the game-over action leaves the game running until the end-match command");
    assert!(e.game_winners().is_empty());
    assert!(e.legal_commands().contains(&Command::EndTurn));
    e.apply(Command::EndTurn).expect("end-match is legal");
    assert!(e.is_over());
}

fn counts(e: &Engine) -> Vec<(u8, i32)> {
    e.capital_counts()
}

#[test]
fn the_game_ends_when_the_last_capital_is_taken() {
    let mut e = might(&[2, 3]);
    assert_eq!(e.capital_total(), 3);
    assert_eq!(counts(&e), vec![(1, 1), (2, 1), (3, 1)]);
    assert!(!e.is_over());

    capture(&mut e, C2);
    assert_eq!(counts(&e), vec![(1, 2), (2, 0), (3, 1)]);
    assert!(!e.is_over(), "two of three capitals is not enough");
    assert_eq!(e.capital_holder(), None);

    capture(&mut e, C3);
    end_match(&mut e);
    assert_eq!(e.winners, vec![1]);
    assert_eq!(e.game_winners(), vec![1]);
    assert_eq!(e.capital_holder(), Some(1));
    // the losers still have cities: this ended on capitals, not on extermination
    assert!(e.is_alive(2) && e.is_alive(3));
}

#[test]
fn no_win_while_another_player_holds_a_capital() {
    let mut e = might(&[1, 2, 3]);
    capture(&mut e, C2);
    e.check_game_over();
    assert!(!e.is_over());
    assert!(e.game_winners().is_empty());
    // player 3 holds a capital and the rest of the board changes hands: still not over
    e.capture_city_at(1, S2);
    e.capture_city_at(1, S3);
    e.check_game_over();
    assert!(!e.is_over());
    assert_eq!(counts(&e), vec![(1, 2), (2, 0), (3, 1)]);
}

#[test]
fn the_player_must_hold_their_own_capital_too() {
    // Player 3 takes player 1's capital; player 1 then takes both other capitals. Player 1 holds two of three, so
    // the game goes on until the own capital is retaken.
    let mut e = might(&[1, 2, 3]);
    e.capture_city_at(3, C1);
    e.capture_city_at(1, C2);
    e.capture_city_at(1, C3);
    e.check_game_over();
    assert_eq!(counts(&e), vec![(1, 2), (2, 0), (3, 1)]);
    assert!(!e.is_over());
    e.capture_city_at(1, C1);
    e.check_game_over();
    end_match(&mut e);
    assert_eq!(e.winners, vec![1]);
}

#[test]
fn an_eliminated_players_capital_counts_for_its_captor() {
    // Player 2 has only its capital: taking it wipes player 2 out. The game continues with players 1 and 3, and
    // ends when player 1 also takes player 3's capital.
    let mut e = might(&[1, 3]);
    capture(&mut e, C2);
    assert!(!e.is_alive(2));
    assert!(!e.is_over());
    assert_eq!(counts(&e), vec![(1, 2), (2, 0), (3, 1)]);
    capture(&mut e, C3);
    end_match(&mut e);
    assert_eq!(e.winners, vec![1]);
    assert!(e.is_alive(3));
}

#[test]
fn extermination_still_ends_a_might_game() {
    let mut e = might(&[1]);
    capture(&mut e, C2);
    assert!(!e.is_alive(2) && e.is_alive(3));
    e.capture_city_at(1, C3);
    e.check_eliminations();
    end_match(&mut e);
    assert_eq!(e.winners, vec![1]);
}

#[test]
fn domination_is_unchanged() {
    let mut e = board(&[2, 3]);
    e.s.settings.set_game_mode(GAME_MODE_DOMINATION);
    assert_eq!(e.s.settings, polytopia_sim::GameSettings::default());
    capture(&mut e, C2);
    capture(&mut e, C3);
    assert!(!e.is_over(), "holding every capital means nothing outside Might");
    assert_eq!(e.capital_holder(), Some(1));
    // it ends when the last opponent is gone, as before
    e.capture_city_at(1, S2);
    e.capture_city_at(1, S3);
    e.check_eliminations();
    end_match(&mut e);
    assert_eq!(e.winners, vec![1]);
}

#[test]
fn other_modes_keep_their_settings() {
    let mut s = polytopia_sim::GameSettings { turn_limit: 30, win_by_extermination: true, ..Default::default() };
    s.set_game_mode(GAME_MODE_PERFECTION);
    assert_eq!(
        (s.base_game_mode, s.rules_game_mode, s.turn_limit, s.win_by_capital, s.win_by_extermination),
        (1, 1, 30, false, true)
    );
    let mut m = polytopia_sim::GameSettings { turn_limit: 30, score_limit: 9, ..Default::default() };
    m.set_game_mode(GAME_MODE_MIGHT);
    assert_eq!((m.base_game_mode, m.rules_game_mode), (4, 4));
    assert!(m.win_by_capital && m.win_by_extermination);
    assert_eq!((m.turn_limit, m.score_limit), (0, 0));
}

#[test]
fn a_turn_limit_switches_the_capital_rule_off() {
    let mut e = might(&[2, 3]);
    e.s.settings.turn_limit = 5;
    capture(&mut e, C2);
    capture(&mut e, C3);
    assert_eq!(e.capital_holder(), Some(1));
    assert!(!e.is_over());
}

#[test]
fn the_mode_survives_a_position_round_trip() {
    let mut e = might(&[2, 3]);
    let text = e.to_position().to_string();
    let back = Engine::from_position(&Json::parse(&text).unwrap()).unwrap();
    assert_eq!(back.s.settings, e.s.settings);
    assert_eq!((back.s.settings.base_game_mode, back.s.settings.rules_game_mode), (GAME_MODE_MIGHT, GAME_MODE_MIGHT));
    assert!(back.s.settings.win_by_capital);

    // the reloaded game plays by the same rules
    let mut back = back;
    capture(&mut back, C2);
    capture(&mut back, C3);
    end_match(&mut back);

    // a finished game reloads as over; the winner is worked out again from the board
    capture(&mut e, C2);
    capture(&mut e, C3);
    end_match(&mut e);
    let done = Engine::from_position(&Json::parse(&e.to_position().to_string()).unwrap()).unwrap();
    assert!(done.is_over());
    assert!(done.winners.is_empty());
    assert_eq!(done.game_winners(), vec![1]);
}

#[test]
fn a_might_position_without_rule_flags_gets_the_preset() {
    let e = might(&[2, 3]);
    let mut json = e.to_position();
    let Json::Obj(top) = &mut json else { panic!("position is an object") };
    let game = top.iter_mut().find(|(k, _)| k == "game").map(|(_, v)| v).unwrap();
    let Json::Obj(game) = game else { panic!() };
    let settings = game.iter_mut().find(|(k, _)| k == "settings").map(|(_, v)| v).unwrap();
    let Json::Obj(settings) = settings else { panic!() };
    settings.retain(|(k, _)| k != "win_by_capital" && k != "win_by_extermination");
    let back = Engine::from_position(&Json::parse(&json.to_string()).unwrap()).unwrap();
    assert!(back.s.settings.win_by_capital && back.s.settings.win_by_extermination);
}

#[test]
fn new_game_starts_in_might_mode() {
    let opts = GameOptions {
        seed: 7,
        size: MapSize::Small,
        preset: MapPreset::Continents,
        players: vec![
            PlayerSetup { tribe: 4, human: true },
            PlayerSetup { tribe: 7, human: false },
            PlayerSetup { tribe: 10, human: false },
        ],
        mode: GAME_MODE_MIGHT,
        ..GameOptions::default()
    };
    let e = Engine::new_game(&opts);
    let s = e.s.settings;
    assert_eq!((s.base_game_mode, s.rules_game_mode), (4, 4));
    assert!(s.win_by_capital && s.win_by_extermination);
    assert_eq!((s.turn_limit, s.score_limit), (0, 0));
    assert_eq!(e.capital_total(), 3);
    assert_eq!(counts(&e), vec![(1, 1), (2, 1), (3, 1)]);
    assert!(!e.is_over());
    assert!(e.game_winners().is_empty());
}
