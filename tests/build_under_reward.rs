//! Retail's build command does not look at a pending city reward: a player whose road build raised a reward can
//! send another build before answering it. Corpus games 2a9c6040 (command 373), 9d7af18b (279) and b04362b0 (662) did
//! exactly that and the server accepted it. The pending check lives in the client's action manager, not in the command.
use polytopia_sim::engine::{Command, Engine};
use polytopia_sim::json::Json;
use polytopia_sim::WorldCoordinates as Wc;

/// Player 1 owns a capital at 3,0 and cities at 0,0 and 6,0; the tile at 2,1 is owned and bare. A road at 1,0 joins the
/// outer cities to the capital, and the connection raises a city reward.
fn board(capital_xp: i16, roads: &[(i32, i32)]) -> Engine {
    let mut tiles = Vec::new();
    for y in 0..3 {
        for x in 0..7 {
            let mut t = format!(r#""x":{x},"y":{y},"terrain":3,"climate":2"#);
            match (x, y) {
                (3, 0) => t.push_str(r#","city":true,"capital":true,"owner":1,"ruling":[3,0]"#),
                (0, 0) => t.push_str(r#","city":true,"owner":1,"ruling":[0,0]"#),
                (6, 0) => t.push_str(r#","city":true,"owner":1,"ruling":[6,0]"#),
                (0, 2) => {
                    t = r#""x":0,"y":2,"terrain":3,"climate":2,"city":true,"capital":true,"owner":2,"ruling":[0,2]"#
                        .to_string()
                }
                _ => {}
            }
            if (x, y) == (2, 1) {
                t.push_str(r#","owner":1,"ruling":[3,0]"#);
            }
            if (y, roads.contains(&(x, y))) == (0, true) {
                t.push_str(r#","owner":1,"ruling":[3,0],"road":true"#);
            }
            tiles.push(format!("{{{t}}}"));
        }
    }
    let text = format!(
        r#"{{"format":"polytopia-position","game":{{"turn":5,"width":7,"height":3,"current_player":1}},
           "players":[{{"id":1,"tribe":2,"human":true,"stars":20,"tech":[0,4]}},{{"id":2,"tribe":3}}],
           "tiles":[{}],
           "improvements":[{{"x":3,"y":0,"level":1,"xp":{capital_xp},"population":{capital_xp}}},
                           {{"x":0,"y":0,"level":1,"xp":1,"population":1}},
                           {{"x":6,"y":0,"level":1,"xp":1,"population":1}}]}}"#,
        tiles.join(",")
    );
    Engine::from_position(&Json::parse(&text).unwrap()).unwrap()
}

#[test]
fn a_build_is_accepted_while_a_city_reward_is_pending() {
    let mut e = board(0, &[(2, 0), (4, 0), (5, 0)]);
    let next = Command::Build { at: Wc { x: 2, y: 1 }, improvement: 3 };
    assert!(e.accepts(&next), "the road is legal before the reward");
    e.apply_unchecked(Command::Build { at: Wc { x: 1, y: 0 }, improvement: 3 });
    assert!(e.reward_pending(), "the road raised a city reward");
    assert!(e.accepts(&next), "the build is still accepted with the reward pending");
    e.apply_unchecked(next);
    assert!(e.tile(Wc { x: 2, y: 1 }).unwrap().has_road);
}

#[test]
fn an_end_turn_is_still_refused_while_a_city_reward_is_pending() {
    let mut e = board(0, &[(2, 0), (4, 0), (5, 0)]);
    e.apply_unchecked(Command::Build { at: Wc { x: 1, y: 0 }, improvement: 3 });
    assert!(e.reward_pending());
    assert!(!e.accepts(&Command::EndTurn), "commands other than a build wait for the reward");
}
