//! A centipede head pushed aside by a unit that appears on its tile: the followers follow.
//!
//! Retail queues the pushed head's move; that move queues a move of its follower (when the follower is not next to the
//! head's new tile) onto the tile the head left. The queue runs last in, first out, so that follower move runs before
//! the train that caused the push. The follower thus lands on the tile the new unit is about to take, and the train
//! pushes it aside in turn, by the direction to its leader.
//!
//! Evidence from the game: game 3043a480 (Cymanti against Cymanti, version 123), command 260 (turn 13). Player 2's city
//! reward appeared on (11,11), where player 1's head 14 stood with its segment 34 on (12,10). Head 14 was pushed to
//! (11,12); the server ended with 34 on (10,12) facing 2, next to its head, the engine had left it on (12,10).
use polytopia_sim::engine::Engine;
use polytopia_sim::json::Json;
use polytopia_sim::WorldCoordinates as Wc;

const WARRIOR: i64 = 2;
const CENTIPEDE: i64 = 33;
const SEGMENT: i64 = 34;

fn wc(x: i32, y: i32) -> Wc {
    Wc { x, y }
}

/// An 8x8 plain with a capital for each player. Player 1's head 1 stands on (3,3) with its segment 2 on (4,2); player 2
/// has warriors on every neighbour of the head except (3,4) and (2,4), which the pushed units may take.
fn board() -> Engine {
    let mut tiles = Vec::new();
    for y in 0..8 {
        for x in 0..8 {
            let mut t = format!(r#""x":{x},"y":{y},"terrain":3,"climate":2"#);
            match (x, y) {
                (0, 0) => t.push_str(r#","city":true,"capital":true,"owner":1,"ruling":[0,0]"#),
                (7, 7) => t.push_str(r#","city":true,"capital":true,"owner":2,"ruling":[7,7]"#),
                _ => {}
            }
            tiles.push(format!("{{{t}}}"));
        }
    }
    let blockers = [(3, 2), (4, 3), (2, 2), (4, 4), (2, 3)];
    let mut units = vec![
        format!(r#"{{"id":1,"type":{CENTIPEDE},"owner":1,"x":3,"y":3,"health":200}}"#),
        format!(r#"{{"id":2,"type":{SEGMENT},"owner":1,"x":4,"y":2,"health":100}}"#),
    ];
    for (i, (x, y)) in blockers.iter().enumerate() {
        units.push(format!(r#"{{"id":{},"type":{WARRIOR},"owner":2,"x":{x},"y":{y},"health":100}}"#, 10 + i));
    }
    let text = format!(
        r#"{{"format":"polytopia-position","game":{{"turn":6,"width":8,"height":8,"current_player":2}},
           "players":[{{"id":1,"tribe":17,"human":true,"stars":5,"tech":[0]}},{{"id":2,"tribe":3,"stars":5}}],
           "tiles":[{}],
           "improvements":[{{"x":0,"y":0,"level":1}},{{"x":7,"y":7,"level":1}}],
           "units":[{}]}}"#,
        tiles.join(","),
        units.join(",")
    );
    let mut e = Engine::from_position(&Json::parse(&text).unwrap()).unwrap();
    e.s.units.iter_mut().find(|u| u.id == 1).unwrap().follower = 2;
    e.s.units.iter_mut().find(|u| u.id == 2).unwrap().leader = 1;
    e
}

#[test]
fn a_pushed_head_keeps_its_segment_next_to_it() {
    let mut e = board();
    let new = e.train_on_occupied(2, WARRIOR as i32, wc(3, 3), wc(7, 7)).expect("the warrior is trained");
    assert_eq!(e.unit(new).unwrap().coordinates, wc(3, 3), "the new unit takes the head's tile");
    assert_eq!(e.unit(1).unwrap().coordinates, wc(3, 4), "the head steps aside");
    // The segment followed onto (3,3), then the new unit pushed it by the direction to its leader.
    assert_eq!(e.unit(2).unwrap().coordinates, wc(2, 4));
    let (head, seg) = (e.unit(1).unwrap().coordinates, e.unit(2).unwrap().coordinates);
    assert!((head.x - seg.x).abs() <= 1 && (head.y - seg.y).abs() <= 1, "the chain is unbroken");
    let mut tiles: Vec<Wc> = e.s.units.iter().map(|u| u.coordinates).collect();
    tiles.sort_by_key(|c| (c.x, c.y));
    tiles.dedup();
    assert_eq!(tiles.len(), e.s.units.len(), "no two units share a tile");
}

#[test]
fn a_segment_still_next_to_the_pushed_head_stays_where_it_is() {
    let mut e = board();
    // The segment moves to (2,3) and the other warriors go: the head steps to a neighbour of (3,3) that touches (2,3).
    e.s.units.retain(|u| u.owner != 2);
    e.s.units.iter_mut().find(|u| u.id == 2).unwrap().coordinates = wc(2, 3);
    e.train_on_occupied(2, WARRIOR as i32, wc(3, 3), wc(7, 7)).expect("the warrior is trained");
    let head = e.unit(1).unwrap().coordinates;
    assert_ne!(head, wc(3, 3), "the head stepped aside");
    assert_eq!(e.unit(2).unwrap().coordinates, wc(2, 3), "the segment did not follow");
}
