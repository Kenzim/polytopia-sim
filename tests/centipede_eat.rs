//! Where the new segment of a centipede that ate stands.
//!
//! The segment appears on the first tile round the tail, starting opposite its facing, that the path finder lets a
//! segment of the owner stand on. That is not only a plain tile: a mountain counts once the owner knows Climbing, and
//! is skipped without it.
//!
//! The new segment is made like any new unit, so it faces the map centre, not the way its tail faces. A later Eat
//! and a push both read that facing.
//!
//! A pushed unit that belongs to a chain steps aside by the direction to its follower, to its leader, or between the
//! two (the facing it carries is not used).
//!
//! Evidence from the game: human game 03650cc5 (version 123, Cymanti against Elyrion), command 206 (turn 12). The
//! centipede attacked (3,6) from the mountain (4,6) and stepped in. The server put the new segment on (4,6), the tile the
//! head had left; the engine skipped the mountain and put it on (4,7), which later left the segment a tile away from
//! the server's one and cost the Elyrion player 15 points.
use polytopia_sim::engine::{Command, Engine};
use polytopia_sim::json::Json;
use polytopia_sim::WorldCoordinates as Wc;

const WARRIOR: i64 = 2;
const CENTIPEDE: i64 = 33;
const SEGMENT: i64 = 34;
const CLIMBING: i64 = 20;

fn wc(x: i32, y: i32) -> Wc {
    Wc { x, y }
}

/// Player 1 (Cymanti, to move, with `techs`) on a 6x3 plain whose tile (3,1) is a mountain; player 2 has a weak
/// warrior on (2,1). Player 1's centipede 1 stands on the mountain.
fn board(techs: &str) -> Engine {
    let mut tiles = Vec::new();
    for y in 0..3 {
        for x in 0..6 {
            let mut t = format!(r#""x":{x},"y":{y},"terrain":{},"climate":2"#, if (x, y) == (3, 1) { 4 } else { 3 });
            match (x, y) {
                (0, 0) => t.push_str(r#","city":true,"capital":true,"owner":1,"ruling":[0,0]"#),
                (5, 2) => t.push_str(r#","city":true,"capital":true,"owner":2,"ruling":[5,2]"#),
                _ => {}
            }
            tiles.push(format!("{{{t}}}"));
        }
    }
    let text = format!(
        r#"{{"format":"polytopia-position","game":{{"turn":6,"width":6,"height":3,"current_player":1}},
           "players":[{{"id":1,"tribe":17,"human":true,"stars":5,"tech":[0{techs}]}},{{"id":2,"tribe":3,"stars":5}}],
           "tiles":[{}],
           "improvements":[{{"x":0,"y":0,"level":1}},{{"x":5,"y":2,"level":1}}],
           "units":[{{"id":1,"type":{CENTIPEDE},"owner":1,"x":3,"y":1,"health":200}},
                    {{"id":4,"type":{WARRIOR},"owner":2,"x":2,"y":1,"health":10}}]}}"#,
        tiles.join(",")
    );
    Engine::from_position(&Json::parse(&text).unwrap()).unwrap()
}

fn segment(e: &Engine) -> Option<Wc> {
    e.s.units.iter().find(|u| u.type_idx == SEGMENT as i32).map(|u| u.coordinates)
}

#[test]
fn a_segment_may_take_the_mountain_the_head_left_once_the_owner_can_climb() {
    let mut e = board(&format!(",{CLIMBING}"));
    e.apply(Command::Attack { unit: 1, target: wc(2, 1) }).expect("the attack is legal");
    assert_eq!(e.unit(1).unwrap().coordinates, wc(2, 1), "the head stepped in");
    assert_eq!(segment(&e), Some(wc(3, 1)), "the new segment stands on the tile behind the head");
}

#[test]
fn a_segment_skips_the_mountain_without_climbing() {
    let mut e = board("");
    e.apply(Command::Attack { unit: 1, target: wc(2, 1) }).expect("the attack is legal");
    let seg = segment(&e).expect("a segment appeared");
    assert_ne!(seg, wc(3, 1), "a mountain is not open to the segment");
    assert_eq!(seg, wc(3, 2), "the next tile in the order is used");
}

#[test]
fn a_new_segment_faces_the_map_centre_not_its_tail() {
    let mut e = board("");
    // The head stands on (4,0) and eats south, so it faces south; the centre of the 6x3 map is (3,1).
    e.s.units.iter_mut().find(|u| u.id == 1).unwrap().coordinates = wc(4, 0);
    e.s.units.iter_mut().find(|u| u.id == 4).unwrap().coordinates = wc(4, 1);
    e.apply(Command::Attack { unit: 1, target: wc(4, 1) }).expect("the attack is legal");
    let seg = e.s.units.iter().find(|u| u.type_idx == SEGMENT as i32).expect("a segment appeared");
    assert_eq!(seg.coordinates, wc(4, 0));
    assert_eq!(e.unit(1).unwrap().direction, 3, "the head faces south");
    assert_eq!(seg.direction, 2, "the segment faces the centre, south west of (4,0)");
}

/// Head 1 on (1,1), its segment 2 on (2,1) facing east (not towards its leader), and a warrior trained onto (2,1).
fn push_chain(with_follower: bool) -> Wc {
    let mut e = board("");
    {
        let h = e.s.units.iter_mut().find(|u| u.id == 1).unwrap();
        h.coordinates = wc(1, 1);
        h.follower = 2;
    }
    let mut seg = e.s.units.iter().find(|u| u.id == 4).unwrap().clone();
    seg.id = 2;
    seg.type_idx = SEGMENT as i32;
    seg.owner = 1;
    seg.coordinates = wc(2, 1);
    seg.health = 100;
    seg.leader = 1;
    seg.direction = 5;
    if with_follower {
        let mut tail = seg.clone();
        tail.id = 3;
        tail.coordinates = wc(2, 2);
        tail.leader = 2;
        tail.follower = 0;
        seg.follower = 3;
        e.s.units.push(tail);
    }
    e.s.units.retain(|u| u.id != 4);
    e.s.units.push(seg);
    e.train_on_occupied(1, WARRIOR as i32, wc(2, 1), wc(0, 0)).expect("the warrior is trained");
    e.unit(2).unwrap().coordinates
}

#[test]
fn a_pushed_segment_steps_aside_by_the_direction_to_its_leader() {
    // Towards the leader (1,1) is taken by the head, so the next candidate is (1,0); the facing east would give (3,1).
    assert_eq!(push_chain(false), wc(1, 0));
}

#[test]
fn a_pushed_segment_between_two_neighbours_uses_the_average_direction() {
    // Leader to the west (1,1), follower to the south (2,2): the directions add up to south west, which is (1,2).
    assert_eq!(push_chain(true), wc(1, 2));
}
