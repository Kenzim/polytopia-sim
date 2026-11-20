//! An attack whose damage rounds to nothing still poisons the unit it hits. The game queues the poison for any
//! attack from a unit with the poison ability whenever the target is still alive afterwards, and does not look at how
//! much damage was done. A retaliation is only queued when
//! it does damage, so a retaliation that rounds to nothing poisons nobody; the engine already did that.
//!
//! Evidence from the game: weekly game cd47553c, command 694 (turn 15): a Cymanti `phychi` (50 hp, attack 7) hit a
//! warrior with 90 hp in its own walled capital for 0 damage, and the server had the warrior poisoned afterwards.
use polytopia_sim::engine::replay::SaveFile;
use polytopia_sim::engine::{Command, Engine};
use polytopia_sim::json::Json;
use polytopia_sim::WorldCoordinates as Wc;

const WARRIOR: i64 = 2;
const PHYCHI: i64 = 30;
const CAPITAL: Wc = Wc { x: 5, y: 1 };

/// Player 1 (Cymanti, to move) and player 2 on an 8x3 plain. Both capitals (0,0 and 5,1) have the City Wall reward,
/// so a warrior standing in one defends with a large bonus. `units` is the unit list as position JSON.
fn board(units: &str) -> Engine {
    let mut tiles = Vec::new();
    for y in 0..3 {
        for x in 0..8 {
            let mut t = format!(r#""x":{x},"y":{y},"terrain":3,"climate":2"#);
            match (x, y) {
                (0, 0) => t.push_str(r#","city":true,"capital":true,"owner":1,"ruling":[0,0]"#),
                (5, 1) => t.push_str(r#","city":true,"capital":true,"owner":2,"ruling":[5,1]"#),
                (4..=6, _) => t.push_str(r#","owner":2,"ruling":[5,1]"#),
                _ => {}
            }
            tiles.push(format!("{{{t}}}"));
        }
    }
    let text = format!(
        r#"{{"format":"polytopia-position","game":{{"turn":6,"width":8,"height":3,"current_player":1}},
           "players":[{{"id":1,"tribe":17,"human":true,"stars":5}},{{"id":2,"tribe":3,"stars":5}}],
           "tiles":[{}],
           "improvements":[{{"x":0,"y":0,"level":1,"rewards":[1]}},{{"x":5,"y":1,"level":1,"rewards":[1]}}],
           "units":[{units}]}}"#,
        tiles.join(",")
    );
    Engine::from_position(&Json::parse(&text).unwrap()).unwrap()
}

fn unit_at(e: &Engine, at: Wc) -> (u16, bool) {
    let u = e.unit_at(at).expect("unit");
    (u.health, u.poisoned)
}

#[test]
fn a_poisoner_poisons_a_unit_it_hits_for_no_damage() {
    let mut e = board(&format!(
        r#"{{"id":1,"type":{PHYCHI},"owner":1,"x":3,"y":1,"health":50}},{{"id":2,"type":{WARRIOR},"owner":2,"x":5,"y":1,"health":90}}"#
    ));
    assert_eq!(unit_at(&e, CAPITAL), (90, false));
    e.apply(Command::Attack { unit: 1, target: CAPITAL }).expect("the attack is legal");
    assert_eq!(unit_at(&e, CAPITAL), (90, true), "no damage, but poisoned");
}

#[test]
fn a_poisoner_still_poisons_when_the_damage_is_positive() {
    let mut e = board(&format!(
        r#"{{"id":1,"type":{PHYCHI},"owner":1,"x":4,"y":2,"health":50}},{{"id":2,"type":{WARRIOR},"owner":2,"x":6,"y":2,"health":90}}"#
    ));
    e.apply(Command::Attack { unit: 1, target: Wc { x: 6, y: 2 } }).expect("the attack is legal");
    let (health, poisoned) = unit_at(&e, Wc { x: 6, y: 2 });
    assert!(health < 90, "damage was done");
    assert!(poisoned);
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

/// Replays weekly game cd47553c (week 2026-10-26) to command 694, where Cymanti `phychi` 53 hits warrior 63 for no
/// damage. The server poisons the warrior. Reads `POLYTOPIA_WEEKLY_RAW`;
/// skips without the file.
#[test]
fn the_weekly_game_poisons_at_zero_damage() {
    let Some(path) = weekly_game("cd47553c") else {
        eprintln!("skipped: no weekly replay files");
        return;
    };
    let save = SaveFile::open(&path).unwrap_or_else(|e| panic!("{path}: {e}"));
    let mut e = Engine::from_state(save.initial.clone());
    for (i, c) in save.commands.iter().enumerate() {
        if c.kind == 20 || c.player == 255 {
            continue;
        }
        let Some(cmd) = e.command_from_raw(c) else {
            e.s.current_command += 1;
            continue;
        };
        if i == 694 {
            assert_eq!(cmd, Command::Attack { unit: 53, target: Wc { x: 13, y: 11 } });
            assert_eq!(e.unit(63).map(|u| (u.health, u.poisoned)), Some((90, false)));
        }
        e.apply_unchecked(cmd);
        if i == 694 {
            assert_eq!(
                e.unit(63).map(|u| (u.health, u.poisoned)),
                Some((90, true)),
                "the warrior took no damage and is poisoned"
            );
            return;
        }
    }
    panic!("the game has no command 694");
}
