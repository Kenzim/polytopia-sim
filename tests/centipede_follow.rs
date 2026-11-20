//! Centipede segments in combat.
//!
//! Attacking and stepping in: a centipede that attacks and steps into the defeated unit's tile drags its tail along.
//! A segment that is no longer next to the head's new tile moves onto the tile the head left, and the next segment is
//! tested against that, down the chain. A segment that is still next to the new tile stays where it is.
//!
//! Moving: the attack steps in through an ordinary move, and so does every Move command, so the same rule applies to a
//! plain move of the head. The segments follow onto the tile the head left, not along its path.
//!
//! Conversion: a converted segment takes the rest of its chain with it, followers and leaders alike.
//!
//! Evidence from the game: weekly game cd47553c, command 881 (turn 18): centipede 75 attacked (12,11) from (13,10)
//! with segment 110 on (14,9). The server had the segment on (13,10) afterwards; the engine left it on (14,9). At
//! command 889 an enemy unit converted the head, and at commands 925 and 926 the Cymanti bot attacked both segments
//! as enemy units, which the engine refused while it kept them with their old owner.
use polytopia_sim::engine::replay::SaveFile;
use polytopia_sim::engine::{Command, Engine};
use polytopia_sim::json::Json;
use polytopia_sim::WorldCoordinates as Wc;

const WARRIOR: i64 = 2;
const MINDBENDER: i64 = 10;
const CENTIPEDE: i64 = 33;
const SEGMENT: i64 = 34;

fn wc(x: i32, y: i32) -> Wc {
    Wc { x, y }
}

/// Player 1 (Cymanti, to move) and player 2 on an 8x3 plain. `units` is the unit list as position JSON.
fn board(units: &str) -> Engine {
    let mut tiles = Vec::new();
    for y in 0..3 {
        for x in 0..8 {
            let mut t = format!(r#""x":{x},"y":{y},"terrain":3,"climate":2"#);
            match (x, y) {
                (0, 0) => t.push_str(r#","city":true,"capital":true,"owner":1,"ruling":[0,0]"#),
                (7, 2) => t.push_str(r#","city":true,"capital":true,"owner":2,"ruling":[7,2]"#),
                _ => {}
            }
            tiles.push(format!("{{{t}}}"));
        }
    }
    let text = format!(
        r#"{{"format":"polytopia-position","game":{{"turn":6,"width":8,"height":3,"current_player":1}},
           "players":[{{"id":1,"tribe":17,"human":true,"stars":5}},{{"id":2,"tribe":3,"stars":5}}],
           "tiles":[{}],
           "improvements":[{{"x":0,"y":0,"level":1}},{{"x":7,"y":2,"level":1}}],
           "units":[{units}]}}"#,
        tiles.join(",")
    );
    Engine::from_position(&Json::parse(&text).unwrap()).unwrap()
}

fn at(e: &Engine, id: u32) -> Wc {
    e.unit(id).unwrap_or_else(|| panic!("unit {id} is gone")).coordinates
}

fn owner(e: &Engine, id: u32) -> u8 {
    e.unit(id).unwrap_or_else(|| panic!("unit {id} is gone")).owner
}

/// Head 1 on (3,1) with segments 2 on (4,0) and 3 on (5,0); a weak enemy warrior 4 on (2,1).
fn chain() -> Engine {
    board(&format!(
        r#"{{"id":1,"type":{CENTIPEDE},"owner":1,"x":3,"y":1,"health":200,"follower":2}},
           {{"id":2,"type":{SEGMENT},"owner":1,"x":4,"y":0,"health":100,"leader":1,"follower":3}},
           {{"id":3,"type":{SEGMENT},"owner":1,"x":5,"y":0,"health":100,"leader":2}},
           {{"id":4,"type":{WARRIOR},"owner":2,"x":2,"y":1,"health":10}}"#
    ))
}

#[test]
fn the_tail_follows_a_head_that_attacks_and_steps_in() {
    let mut e = chain();
    e.apply(Command::Attack { unit: 1, target: wc(2, 1) }).expect("the attack is legal");
    assert!(e.unit(4).is_none(), "the warrior is dead");
    assert_eq!(at(&e, 1), wc(2, 1), "the head stepped in");
    assert_eq!(at(&e, 2), wc(3, 1), "the first segment took the head's old tile");
    assert_eq!(at(&e, 3), wc(4, 0), "the second segment took the first one's old tile");
    assert!(e.unit(2).unwrap().moved && e.unit(3).unwrap().moved);
}

#[test]
fn a_segment_still_next_to_the_head_stays() {
    let mut e = board(&format!(
        r#"{{"id":1,"type":{CENTIPEDE},"owner":1,"x":3,"y":1,"health":200,"follower":2}},
           {{"id":2,"type":{SEGMENT},"owner":1,"x":3,"y":0,"health":100,"leader":1}},
           {{"id":4,"type":{WARRIOR},"owner":2,"x":2,"y":1,"health":10}}"#
    ));
    e.apply(Command::Attack { unit: 1, target: wc(2, 1) }).expect("the attack is legal");
    assert_eq!(at(&e, 1), wc(2, 1));
    assert_eq!(at(&e, 2), wc(3, 0), "(3,0) is next to (2,1), so the segment stays");
}

#[test]
fn a_head_that_does_not_move_leaves_its_tail() {
    // The warrior survives the hit, so the head stays on its tile and so does the tail.
    let mut e = board(&format!(
        r#"{{"id":1,"type":{CENTIPEDE},"owner":1,"x":3,"y":1,"health":200,"follower":2}},
           {{"id":2,"type":{SEGMENT},"owner":1,"x":4,"y":0,"health":100,"leader":1}},
           {{"id":4,"type":{WARRIOR},"owner":2,"x":2,"y":1,"health":1000}}"#
    ));
    e.apply(Command::Attack { unit: 1, target: wc(2, 1) }).expect("the attack is legal");
    assert!(e.unit(4).is_some(), "the warrior survived");
    assert_eq!(at(&e, 1), wc(3, 1));
    assert_eq!(at(&e, 2), wc(4, 0));
}

/// Player 2's centipede 1 on (2,1) with segments 2 on (1,1) and 3 on (0,1), and player 1's mindbender 4 on (3,1).
fn enemy_chain() -> Engine {
    board(&format!(
        r#"{{"id":1,"type":{CENTIPEDE},"owner":2,"x":2,"y":1,"health":200,"follower":2}},
           {{"id":2,"type":{SEGMENT},"owner":2,"x":1,"y":1,"health":100,"leader":1,"follower":3}},
           {{"id":3,"type":{SEGMENT},"owner":2,"x":0,"y":1,"health":100,"leader":2}},
           {{"id":4,"type":{MINDBENDER},"owner":1,"x":3,"y":1,"health":100}}"#
    ))
}

#[test]
fn converting_the_head_converts_the_whole_chain() {
    let mut e = enemy_chain();
    e.apply(Command::Attack { unit: 4, target: wc(2, 1) }).expect("the conversion is legal");
    assert_eq!([owner(&e, 1), owner(&e, 2), owner(&e, 3)], [1, 1, 1]);
    assert!(e.unit(3).unwrap().moved && e.unit(3).unwrap().attacked, "a converted unit has acted");
}

#[test]
fn converting_a_middle_segment_converts_both_directions() {
    let mut e = board(&format!(
        r#"{{"id":1,"type":{CENTIPEDE},"owner":2,"x":2,"y":0,"health":200,"follower":2}},
           {{"id":2,"type":{SEGMENT},"owner":2,"x":2,"y":1,"health":100,"leader":1,"follower":3}},
           {{"id":3,"type":{SEGMENT},"owner":2,"x":1,"y":2,"health":100,"leader":2}},
           {{"id":4,"type":{MINDBENDER},"owner":1,"x":3,"y":1,"health":100}}"#
    ));
    e.apply(Command::Attack { unit: 4, target: wc(2, 1) }).expect("the conversion is legal");
    assert_eq!([owner(&e, 1), owner(&e, 2), owner(&e, 3)], [1, 1, 1]);
}

#[test]
fn the_tail_follows_a_head_that_moves() {
    let mut e = board(&format!(
        r#"{{"id":1,"type":{CENTIPEDE},"owner":1,"x":3,"y":1,"health":200,"follower":2}},
           {{"id":2,"type":{SEGMENT},"owner":1,"x":4,"y":0,"health":100,"leader":1,"follower":3}},
           {{"id":3,"type":{SEGMENT},"owner":1,"x":5,"y":0,"health":100,"leader":2}}"#
    ));
    e.apply(Command::Move { unit: 1, to: wc(1, 1) }).expect("the move is legal");
    assert_eq!(at(&e, 1), wc(1, 1));
    assert_eq!(at(&e, 2), wc(3, 1), "the first segment took the tile the head left");
    assert_eq!(at(&e, 3), wc(4, 0), "the second segment took the first one's old tile");
    assert!(e.unit(2).unwrap().moved && e.unit(3).unwrap().moved);
}

#[test]
fn a_segment_still_next_to_a_moved_head_stays() {
    let mut e = board(&format!(
        r#"{{"id":1,"type":{CENTIPEDE},"owner":1,"x":3,"y":1,"health":200,"follower":2}},
           {{"id":2,"type":{SEGMENT},"owner":1,"x":3,"y":0,"health":100,"leader":1}}"#
    ));
    e.apply(Command::Move { unit: 1, to: wc(2, 1) }).expect("the move is legal");
    assert_eq!(at(&e, 1), wc(2, 1));
    assert_eq!(at(&e, 2), wc(3, 0), "(3,0) is next to (2,1), so the segment stays");
}

fn weekly_game(prefix: &str) -> Option<String> {
    let dir = std::env::var("POLYTOPIA_WEEKLY_RAW").unwrap_or_default();
    let name = std::fs::read_dir(&dir)
        .ok()?
        .filter_map(|d| d.ok())
        .map(|d| d.file_name().to_string_lossy().into_owned())
        .find(|n| n.starts_with(prefix) && n.ends_with("_init.bin"))?;
    let stem = name.strip_suffix("_init.bin")?;
    Some(format!("{dir}/{stem}_init.bin+{dir}/{stem}_cur.bin"))
}

/// Replays the weekly game starting with `game` up to and including `last`, calling `check(command index, engine
/// before, engine after)` for each command from `from` on. Reads `POLYTOPIA_WEEKLY_RAW`;
/// returns false (the caller skips) without the file.
fn replay_weekly(
    game: &str,
    from: usize,
    last: usize,
    mut check: impl FnMut(usize, Option<Command>, &mut Engine, bool),
) -> bool {
    let Some(path) = weekly_game(game) else {
        eprintln!("skipped: no weekly replay files");
        return false;
    };
    let save = SaveFile::open(&path).unwrap_or_else(|e| panic!("{path}: {e}"));
    let mut e = Engine::from_state(save.initial.clone());
    for (i, c) in save.commands.iter().enumerate().take(last + 1) {
        if c.kind == 20 || c.player == 255 {
            continue;
        }
        let Some(cmd) = e.command_from_raw(c) else {
            e.s.current_command += 1;
            continue;
        };
        if i >= from {
            check(i, Some(cmd), &mut e, false);
        }
        e.apply_unchecked(cmd);
        if i >= from {
            check(i, None, &mut e, true);
        }
    }
    true
}

/// Command 881: centipede 75 attacks (12,11) from (13,10) with segment 110 on (14,9).
#[test]
fn the_weekly_game_moves_the_segment_after_the_attack() {
    replay_weekly("cd47553c", 881, 881, |i, cmd, e, after| {
        if i != 881 {
            return;
        }
        if !after {
            assert_eq!(cmd, Some(Command::Attack { unit: 75, target: wc(12, 11) }));
            assert_eq!((at(e, 75), at(e, 110)), (wc(13, 10), wc(14, 9)));
        } else {
            assert_eq!(at(e, 75), wc(12, 11), "the head stepped in");
            assert_eq!(at(e, 110), wc(13, 10), "the segment took the head's old tile");
            // The kill also makes the head eat: the new segment grows on the tile the old tail just left.
            let grown = e.unit_at(wc(14, 9)).expect("a new segment grows on (14,9)");
            assert_eq!((grown.id, grown.leader), (113, 110));
        }
    });
}

/// Command 889: player 1's unit 97 converts the head 75; the segments 110 and 113 go with it, so the bot's attacks on
/// them at commands 925 and 926 are attacks on enemies.
#[test]
fn the_weekly_game_converts_the_segments_with_the_head() {
    replay_weekly("cd47553c", 889, 889, |i, _, e, after| {
        if i == 889 && after {
            assert_eq!([owner(e, 75), owner(e, 110), owner(e, 113)], [1, 1, 1]);
        }
    });
}

/// A segment that grows after a kill has no home city, like any unit made on a tile that is not a city, so it does not
/// count against its owner's city's unit limit.
#[test]
fn a_grown_segment_has_no_home() {
    let mut e = board(&format!(
        r#"{{"id":1,"type":{CENTIPEDE},"owner":1,"x":3,"y":1,"health":200,"home":[0,0]}},
           {{"id":4,"type":{WARRIOR},"owner":2,"x":2,"y":1,"health":10}}"#
    ));
    assert_eq!(e.city_unit_count(wc(0, 0)), 1);
    e.apply(Command::Attack { unit: 1, target: wc(2, 1) }).expect("the attack is legal");
    let grown = e.s.units.iter().find(|u| u.type_idx == SEGMENT as i32).expect("the kill grows a segment");
    assert_eq!(grown.owner, 1);
    assert_eq!(grown.home, wc(-1, -1), "the segment has no home city");
    assert_eq!(e.city_unit_count(wc(0, 0)), 1, "only the head counts against the city");
}

/// Weekly game cd47553c: the server keeps no home on the segments 110 and 113 (the head 75 has its home), so the
/// city (10,2) of player 3 supported a seventh unit at command 942, a Train that the engine refused when the segments
/// counted against it.
#[test]
fn the_weekly_game_grows_segments_without_a_home() {
    replay_weekly("cd47553c", 881, 942, |i, cmd, e, after| {
        if i == 881 && after {
            assert_eq!(e.unit(113).unwrap().home, wc(-1, -1), "the grown segment has no home");
        }
        if i == 942 && !after {
            let cmd = cmd.expect("a command");
            assert!(e.legal_commands().contains(&cmd), "player 3's Train in (10,2) is legal: {cmd:?}");
        }
    });
}

/// Weekly game 2e31718c (week 10-26, player 3): at command 844 the centipede 79 moves from (10,3) to (9,2) with its
/// segment 139 on (10,4). The server had the segment on (10,3) afterwards; without it the engine refused the head's
/// move onto (9,3) at command 899 (the engine had kept the segment away and grown another one on that tile).
#[test]
fn the_weekly_game_moves_the_tail_after_a_move() {
    replay_weekly("2e31718c", 844, 844, |i, cmd, e, after| {
        if i != 844 {
            return;
        }
        if !after {
            assert_eq!(cmd, Some(Command::Move { unit: 79, to: wc(9, 2) }));
            assert_eq!((at(e, 79), at(e, 139)), (wc(10, 3), wc(10, 4)));
        } else {
            assert_eq!(at(e, 79), wc(9, 2));
            assert_eq!(at(e, 139), wc(10, 3), "the segment took the tile the head left");
        }
    });
}

/// Retail's move command checks the unit, its moved and frozen state and a path, never its leader, so a segment can be
/// sent on its own. Corpus: 11 games hold such a move; the server's final state only matches when the segment moved.
#[test]
fn a_segment_can_be_moved_on_its_own() {
    let mut e = chain();
    let cmd = Command::Move { unit: 2, to: wc(3, 0) };
    assert!(!e.legal_commands().contains(&cmd), "the move options leave a segment out");
    e.apply(cmd).expect("the move command accepts a segment");
    assert_eq!(at(&e, 2), wc(3, 0));
    assert!(e.unit(2).unwrap().moved);
    assert_eq!(at(&e, 3), wc(4, 0), "its own follower takes the tile it left");
}

#[test]
fn a_segment_move_still_needs_a_path() {
    let e = chain();
    assert!(!e.accepts(&Command::Move { unit: 2, to: wc(7, 1) }), "out of reach");
    assert!(!e.accepts(&Command::Move { unit: 2, to: wc(5, 0) }), "occupied by its follower");
}

/// Retail works out a splash hit while it builds the attack, before the damage runs. A segment next to the target
/// whose leader the attack kills becomes the new head, but still takes the half damage worked out for a segment
/// (80 / 2 = 40), not the one for a centipede. Corpus: Cymanti game 9bac2541, where the server's head died to two
/// bombership hits that the engine left it ten health to survive.
#[test]
fn a_splash_hit_on_a_segment_that_becomes_the_head_is_worked_out_for_the_segment() {
    const BOMBERSHIP: i64 = 41;
    let mut e = board(&format!(
        r#"{{"id":5,"type":{BOMBERSHIP},"owner":1,"x":5,"y":1,"health":100}},
           {{"id":2,"type":{SEGMENT},"owner":2,"x":3,"y":1,"health":5,"follower":3}},
           {{"id":3,"type":{SEGMENT},"owner":2,"x":3,"y":0,"health":100,"leader":2}}"#
    ));
    e.apply(Command::Attack { unit: 5, target: wc(3, 1) }).expect("the attack is legal");
    assert!(e.unit(2).is_none(), "the leader is dead");
    let head = e.unit(3).expect("the follower lives");
    assert_eq!(head.type_idx, CENTIPEDE as i32, "it became the head");
    assert_eq!(head.health, 160, "200 less the 40 lost as a segment");
}
