//! Algae built by a player with the Cute skin also puts Foam on the tile; any other skin leaves just the Algae.
//! (The game's build adds the build's effect, and when that is Algae adds Foam if the building player's visual skin
//! is 19.) Found in the corpus final-state comparison: 224 tiles ended with Algae and Foam where the
//! engine had only Algae.
use polytopia_sim::engine::{Command, Engine};
use polytopia_sim::json::Json;
use polytopia_sim::WorldCoordinates as Wc;

const ALGAE: i32 = 38;

fn built_effects(skin: i32) -> u16 {
    let mut tiles = Vec::new();
    for y in 0..3 {
        for x in 0..6 {
            let mut t = format!(r#""x":{x},"y":{y},"terrain":{},"climate":2"#, if (x, y) == (3, 1) { 1 } else { 3 });
            match (x, y) {
                (0, 0) => t.push_str(r#","city":true,"capital":true,"owner":1,"ruling":[0,0]"#),
                (5, 2) => t.push_str(r#","city":true,"capital":true,"owner":2,"ruling":[5,2]"#),
                (3, 1) => t.push_str(r#","owner":1"#),
                _ => {}
            }
            tiles.push(format!("{{{t}}}"));
        }
    }
    let text = format!(
        r#"{{"format":"polytopia-position","game":{{"turn":6,"width":6,"height":3,"current_player":1}},
           "players":[{{"id":1,"tribe":2,"human":true,"stars":5}},{{"id":2,"tribe":3,"stars":5}}],
           "tiles":[{}],
           "improvements":[{{"x":0,"y":0,"level":1}},{{"x":5,"y":2,"level":1}}],
           "units":[]}}"#,
        tiles.join(",")
    );
    let mut e = Engine::from_position(&Json::parse(&text).unwrap()).unwrap();
    e.s.player_runtime.iter_mut().find(|r| r.player == 1).expect("runtime of player 1").skin = skin;
    let at = Wc { x: 3, y: 1 };
    e.apply_unchecked(Command::Build { at, improvement: ALGAE });
    e.tile(at).unwrap().effects
}

#[test]
fn a_cute_skin_builds_algae_with_foam() {
    assert_eq!(built_effects(19), (1 << Engine::EFFECT_ALGAE) | (1 << Engine::EFFECT_FOAM));
}

#[test]
fn another_skin_builds_plain_algae() {
    assert_eq!(built_effects(0), 1 << Engine::EFFECT_ALGAE);
}
