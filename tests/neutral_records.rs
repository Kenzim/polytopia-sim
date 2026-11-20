//! A neutral improvement record (a ruin, a village, a corner lighthouse) belongs to nobody until its tile is taken
//! into a border; then it takes the tile's owner. Retail evidence: in all 462 records of villages, ruins and
//! lighthouses on owned tiles in the regression saves, the fetched games and the weekly replays, the record's owner is
//! its tile's owner; weekly games 8f69e437 and f112aa5d (week 10-19) end with the corner lighthouse of player 1's
//! border recorded to player 1.
//!
//! The board is a hand-made 10x10 position; the replay test reads the weekly games from `POLYTOPIA_WEEKLY_RAW`, and
//! skips without them.
use polytopia_sim::engine::replay::SaveFile;
use polytopia_sim::engine::tables::improvement;
use polytopia_sim::engine::Engine;
use polytopia_sim::json::Json;
use polytopia_sim::CityReward;
use polytopia_sim::WorldCoordinates as Wc;

const LIGHTHOUSE: i32 = 47;
const CAPITAL: Wc = Wc { x: 2, y: 2 };
const CORNER: Wc = Wc { x: 0, y: 0 };
const RUIN: Wc = Wc { x: 4, y: 3 };
const VILLAGE: Wc = Wc { x: 3, y: 4 };
const FAR: Wc = Wc { x: 6, y: 6 };

/// Player 1's capital at 2,2 with a border of one; a neutral village, a ruin and the corner lighthouse lie two tiles
/// out, and another ruin lies beyond the border that growth reaches.
fn board() -> Engine {
    let mut tiles = Vec::new();
    for y in 0..10 {
        for x in 0..10 {
            let mut t = format!(r#""x":{x},"y":{y},"terrain":3,"climate":2"#);
            if (x, y) == (CAPITAL.x, CAPITAL.y) {
                t.push_str(r#","city":true,"capital":true,"owner":1"#);
            } else if (x, y) == (8, 8) {
                t.push_str(r#","city":true,"capital":true,"owner":2"#);
            } else if (x, y) == (VILLAGE.x, VILLAGE.y) {
                t.push_str(r#","city":true"#);
            } else if (x, y) == (RUIN.x, RUIN.y) || (x, y) == (FAR.x, FAR.y) {
                t.push_str(&format!(r#","improvement":{}"#, improvement::RUIN));
            } else if (x, y) == (CORNER.x, CORNER.y) {
                t.push_str(&format!(r#","improvement":{LIGHTHOUSE}"#));
            }
            tiles.push(format!("{{{t}}}"));
        }
    }
    let text = format!(
        r#"{{"format":"polytopia-position","game":{{"turn":2,"width":10,"height":10,"current_player":1}},
           "players":[{{"id":1,"tribe":2,"human":true}},{{"id":2,"tribe":3}}],
           "tiles":[{}]}}"#,
        tiles.join(",")
    );
    Engine::from_position(&Json::parse(&text).unwrap()).unwrap()
}

/// (tile owner, record owner) of the tile at `at`.
fn owners(e: &Engine, at: Wc) -> (u8, Option<u8>) {
    let t = e.tile(at).unwrap();
    (t.owner, e.improvement_rt(at).map(|r| r.owner))
}

#[test]
fn a_record_on_a_tile_taken_into_a_border_takes_the_tiles_owner() {
    let mut e = board();
    for at in [CORNER, RUIN, VILLAGE, FAR] {
        assert_eq!(owners(&e, at), (0, Some(0)), "{at:?} is neutral to begin with");
    }
    e.apply_city_reward(CAPITAL, CityReward::BorderGrowth);
    for at in [CORNER, RUIN, VILLAGE] {
        assert_eq!(
            owners(&e, at),
            (1, Some(1)),
            "{at:?} is inside the grown border: tile and record belong to player 1"
        );
    }
    assert_eq!(owners(&e, FAR), (0, Some(0)), "a tile the border does not reach stays neutral");
}

/// Replays a weekly game and checks every improvement record's owner against the game's final state; returns whether
/// the game has a corner lighthouse on an owned tile.
fn replay_record_owners(init: &str, cur: &str) -> Option<bool> {
    let save = SaveFile::open(&format!("{init}+{cur}")).ok()?;
    let mut e = Engine::from_state(save.initial.clone());
    if save.commands.iter().any(|c| c.kind == 20) {
        e.start_match();
    }
    for c in &save.commands {
        if c.kind == 20 || c.player == 255 {
            continue;
        }
        if c.kind == 22 {
            if e.end_match().is_err() {
                e.s.current_command += 1;
            }
            continue;
        }
        let Some(cmd) = e.command_from_raw(c) else {
            e.s.current_command += 1;
            continue;
        };
        e.apply_unchecked(cmd);
    }
    let mut lighthouse = false;
    for theirs in &save.current.improvements {
        let Some(ours) = e.s.improvements.iter().find(|r| r.tile == theirs.tile) else { continue };
        assert_eq!(ours.owner, theirs.owner, "{cur}: owner of the record on tile {}", theirs.tile);
        let tile = e.s.map.tiles.iter().find(|t| t.index == theirs.tile).unwrap();
        lighthouse |= tile.improvement == LIGHTHOUSE && tile.owner != 0;
    }
    Some(lighthouse)
}

#[test]
fn weekly_replays_have_the_servers_record_owners() {
    let dir = std::env::var("POLYTOPIA_WEEKLY_RAW").unwrap_or_default();
    let Ok(rd) = std::fs::read_dir(&dir) else {
        eprintln!("skipped: no weekly replays in {dir}");
        return;
    };
    let mut names: Vec<String> = rd
        .filter_map(|d| d.ok())
        .filter_map(|d| d.file_name().to_string_lossy().strip_suffix("_init.bin").map(|s| s.to_string()))
        .collect();
    names.sort();
    if names.is_empty() {
        eprintln!("skipped: no weekly replays in {dir}");
        return;
    }
    let lighthouses = names
        .iter()
        .filter(|n| replay_record_owners(&format!("{dir}/{n}_init.bin"), &format!("{dir}/{n}_cur.bin")) == Some(true))
        .count();
    assert!(lighthouses > 0, "no owned corner lighthouse in the weekly replays");
}
