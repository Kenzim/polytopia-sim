//! Decompose sent again for an improvement that is already decomposing.
//!
//! Retail's decompose command is valid whenever the tile holds an improvement; its action adds the Decomposing effect
//! only if the improvement lacks it. A player who clicks the button twice therefore sends a command the game accepts and
//! that changes nothing, although the improvement's ability list no longer offers it. Real games hold runs of dozens of
//! such repeats (Cymanti, version 123, e.g. game 48e3c348 command 121 onwards).
use polytopia_sim::engine::{Command, Engine};
use polytopia_sim::json::Json;
use polytopia_sim::WorldCoordinates as Wc;

const FUNGI: i32 = 37;
const DECOMPOSE_TECH: i32 = 36;

fn board(tech: &str) -> Engine {
    let mut tiles = Vec::new();
    for y in 0..4 {
        for x in 0..4 {
            let mut t = format!(r#""x":{x},"y":{y},"terrain":3,"climate":2,"owner":1"#);
            match (x, y) {
                (0, 0) => t.push_str(r#","city":true,"capital":true,"ruling":[0,0]"#),
                (2, 2) => t.push_str(&format!(r#","improvement":{FUNGI}"#)),
                _ => {}
            }
            tiles.push(format!("{{{t}}}"));
        }
    }
    let text = format!(
        r#"{{"format":"polytopia-position","game":{{"turn":6,"width":4,"height":4,"current_player":1}},
           "players":[{{"id":1,"tribe":17,"human":true,"stars":5,"tech":[{tech}]}},{{"id":2,"tribe":3,"stars":5}}],
           "tiles":[{}],"improvements":[{{"x":0,"y":0,"level":1}}],"units":[]}}"#,
        tiles.join(",")
    );
    Engine::from_position(&Json::parse(&text).unwrap()).unwrap()
}

#[test]
fn a_second_decompose_is_accepted_and_changes_nothing() {
    let mut e = board(&format!("0,{DECOMPOSE_TECH}"));
    let cmd = Command::Decompose { at: Wc { x: 2, y: 2 } };
    assert!(e.legal_commands().contains(&cmd));
    e.apply(cmd).expect("the first decompose");
    assert!(!e.legal_commands().contains(&cmd), "the ability list no longer offers it");
    assert!(e.accepts(&cmd), "the game still accepts it");
    let before = format!("{:?}", e.improvement_rt(Wc { x: 2, y: 2 }));
    e.apply(cmd).expect("the repeat is accepted");
    assert!(e.improvement_rt(Wc { x: 2, y: 2 }).unwrap().decomposing);
    assert_eq!(before, format!("{:?}", e.improvement_rt(Wc { x: 2, y: 2 })), "the improvement is unchanged");
}

#[test]
fn a_decompose_without_the_ability_is_still_refused() {
    let e = board("0");
    let cmd = Command::Decompose { at: Wc { x: 2, y: 2 } };
    assert!(!e.accepts(&cmd));
}
