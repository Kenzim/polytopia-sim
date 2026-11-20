//! Retail's freeze-area action queues a route update and the city-connection update behind its tile freezes, so
//! ice that now links two route openers carries a route at once (an auto-freezing Mooni's move raised none in the engine).
use polytopia_sim::engine::{Command, Engine};
use polytopia_sim::json::Json;
use polytopia_sim::WorldCoordinates as Wc;

const MOONI: i64 = 22;
const MYCELIUM: i64 = 39;

/// Player 1's cities at (0,0) and (4,0) with mycelium (a route opener that routes over ice) on the two land tiles at (1,0) and (3,0) and water between at (2,0); the
/// Mooni (id 1) stands on land at (2,2), two tiles from the water.
fn board() -> Engine {
    let mut tiles = Vec::new();
    for y in 0..3 {
        for x in 0..6 {
            let extra = match (x, y) {
                (0, 0) | (4, 0) => r#","city":true,"owner":1,"ruling":[0,0]"#.to_string(),
                (1, 0) | (3, 0) => format!(r#","owner":1,"ruling":[0,0],"improvement":{MYCELIUM}"#),
                (5, 2) => r#","city":true,"capital":true,"owner":2,"ruling":[5,2]"#.to_string(),
                _ => String::new(),
            };
            let terrain = if (x, y) == (2, 0) { 1 } else { 3 };
            tiles.push(format!(r#"{{"x":{x},"y":{y},"terrain":{terrain},"climate":2{extra}}}"#));
        }
    }
    let text = format!(
        r#"{{"format":"polytopia-position","game":{{"turn":9,"width":6,"height":3,"current_player":1}},
           "players":[{{"id":1,"tribe":3,"human":true,"stars":5}},{{"id":2,"tribe":3,"human":true,"stars":5}}],
           "tiles":[{}],
           "improvements":[{{"x":0,"y":0,"level":1}},{{"x":4,"y":0,"level":1}},{{"x":5,"y":2,"level":1}}],
           "units":[{{"id":1,"type":{MOONI},"owner":1,"x":2,"y":2}}]}}"#,
        tiles.join(",")
    );
    Engine::from_position(&Json::parse(&text).unwrap()).unwrap()
}

#[test]
fn an_autofreeze_move_updates_the_routes() {
    let mut e = board();
    let water = Wc { x: 2, y: 0 };
    assert!(!e.tile(water).unwrap().has_route);
    e.apply_unchecked(Command::Move { unit: 1, to: Wc { x: 2, y: 1 } });
    assert_eq!(e.tile(water).unwrap().terrain, 6, "the water froze");
    assert!(e.tile(water).unwrap().has_route, "the ice between the two mycelia carries a route");
}
