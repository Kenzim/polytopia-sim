//! A border growth reward re-runs the route pass and the city connections of every other player, not the growing
//! player's own (the game's border growth queues the city-connection update for every player but Nature and the
//! reward's owner). A city of the growing player whose connection the next update would drop keeps
//! it, and its population, until then.
use polytopia_sim::engine::{Command, Engine, Trigger};
use polytopia_sim::json::Json;
use polytopia_sim::{CityReward, WorldCoordinates as Wc};

const GROWING: Wc = Wc { x: 6, y: 0 };
const LINKED: Wc = Wc { x: 4, y: 6 };

/// Player 1: capital (0,0), a level-4 city on (6,0) with a border growth to choose, and a city on (4,6) marked as
/// connected to the capital with nothing joining them.
fn board() -> Engine {
    let mut tiles = Vec::new();
    for y in 0..7 {
        for x in 0..7 {
            let mut t = format!(r#""x":{x},"y":{y},"terrain":3,"climate":2"#);
            match (x, y) {
                (0, 0) => t.push_str(r#","city":true,"capital":true,"owner":1,"ruling":[0,0]"#),
                (6, 0) => t.push_str(r#","city":true,"owner":1,"ruling":[6,0]"#),
                (4, 6) => t.push_str(r#","city":true,"owner":1,"ruling":[4,6]"#),
                (0, 6) => t.push_str(r#","city":true,"capital":true,"owner":2,"ruling":[0,6]"#),
                _ => {}
            }
            tiles.push(format!("{{{t}}}"));
        }
    }
    let text = format!(
        r#"{{"format":"polytopia-position","game":{{"turn":5,"width":7,"height":7,"current_player":1}},
           "players":[{{"id":1,"tribe":2,"human":true,"stars":20}},{{"id":2,"tribe":3}}],
           "tiles":[{}],
           "improvements":[{{"x":0,"y":0,"level":2,"xp":0,"population":3}},
                           {{"x":6,"y":0,"level":4,"xp":0,"population":5}},
                           {{"x":4,"y":6,"level":1,"xp":1,"population":1,"connected":1}}]}}"#,
        tiles.join(",")
    );
    let mut e = Engine::from_position(&Json::parse(&text).unwrap()).unwrap();
    e.triggers.push(Trigger { player: 1, city: GROWING, peace_from: None });
    e
}

#[test]
fn border_growth_leaves_the_growing_players_own_connections() {
    let mut e = board();
    e.apply(Command::CityReward { city: GROWING, reward: CityReward::BorderGrowth }).unwrap();
    let rt = e.improvement_rt(LINKED).unwrap();
    assert_eq!((rt.connected_to_capital_of_player, rt.population), (1, 1));
    assert_eq!(e.improvement_rt(GROWING).unwrap().border_size, 2);
}
