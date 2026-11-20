//! Embarking trains a new boat around the unit. As in the game, the boat starts unpromoted, with no xp and with
//! the current turn as its created turn; the passenger keeps its own promotion, xp and created turn. Retail
//! evidence: every boat in the regression saves has promotion 0 and xp 0, and its created turn is the embark turn
//! (6bcde: boat 163, created turn 25, carries veteran unit 26 from turn 7). The replay test reads the regression
//! games from `POLYTOPIA_REGRESS`, and skips without them.
use polytopia_sim::engine::replay::SaveFile;
use polytopia_sim::engine::{Engine, GameOptions, PlayerSetup};
use polytopia_sim::UnitState;

fn regress_dir() -> String {
    std::env::var("POLYTOPIA_REGRESS").unwrap_or_default()
}

fn regress_inputs() -> Vec<String> {
    let regress = regress_dir();
    let Ok(dir) = std::fs::read_dir(&regress) else { return Vec::new() };
    let mut names: Vec<String> =
        dir.filter_map(|d| d.ok()).map(|d| d.file_name().to_string_lossy().into_owned()).collect();
    names.sort();
    names
        .iter()
        .filter_map(|n| {
            if n.ends_with(".state") {
                Some(format!("{regress}/{n}"))
            } else {
                n.strip_suffix("_init.bin").map(|stem| format!("{regress}/{stem}_init.bin+{regress}/{stem}_cur.bin"))
            }
        })
        .collect()
}

#[test]
fn a_boat_made_by_embarking_starts_unpromoted_and_new() {
    let opts = GameOptions {
        players: vec![PlayerSetup { tribe: 2, human: true }, PlayerSetup { tribe: 3, human: false }],
        ..GameOptions::default()
    };
    let mut e = Engine::new_game(&opts);
    e.s.current_turn = 6;
    let i = e.s.units.iter().position(|u| u.owner == 1 && u.passenger.is_none()).expect("a unit of player 1");
    {
        let u = &mut e.s.units[i];
        u.promotion = 1;
        u.xp = 3;
        u.created_turn = 2;
        u.health = 70;
    }
    let before: UnitState = e.s.units[i].clone();
    e.embark_pub(before.id);

    let boat = e.s.units.iter().find(|u| u.passenger.as_ref().is_some_and(|p| p.id == before.id)).expect("the boat");
    assert_ne!(boat.id, before.id, "the boat is a new unit");
    assert_eq!(boat.promotion, 0, "the boat is not promoted");
    assert_eq!(boat.xp, 0);
    assert_eq!(boat.created_turn, 6, "the boat was created this turn");
    assert_eq!(boat.health, 70, "the boat starts with the passenger's health");
    assert_eq!((boat.owner, boat.home, boat.coordinates), (before.owner, before.home, before.coordinates));
    assert_eq!(boat.passenger.as_deref(), Some(&before), "the passenger is unchanged");
}

/// Replays every regression game and compares each loaded boat in the engine's final state with the same boat
/// (same id, same passenger) in the game's own final state.
#[test]
fn replayed_boats_match_the_game() {
    let inputs = regress_inputs();
    if inputs.is_empty() {
        eprintln!("skipped: no regression data in {}", regress_dir());
        return;
    }
    let mut compared = 0;
    let mut promoted_passengers = 0;
    for path in &inputs {
        let save = SaveFile::open(path).unwrap_or_else(|e| panic!("{path}: {e}"));
        let mut e = Engine::from_state(save.initial.clone());
        if save.commands.iter().any(|c| c.kind == 20) {
            e.start_match();
        }
        for c in &save.commands {
            if c.kind == 20 || c.player == 255 {
                continue;
            }
            let Some(cmd) = e.command_from_raw(c) else {
                e.s.current_command += 1;
                continue;
            };
            if e.current_player() != c.player {
                break;
            }
            e.apply_unchecked(cmd);
        }
        for ours in e.s.units.iter().filter(|u| u.passenger.is_some()) {
            let p = ours.passenger.as_deref().unwrap();
            let Some(theirs) = save
                .current
                .units
                .iter()
                .find(|u| u.id == ours.id && u.passenger.as_ref().is_some_and(|q| q.id == p.id))
            else {
                continue;
            };
            let q = theirs.passenger.as_deref().unwrap();
            assert_eq!(
                (ours.promotion, ours.xp, ours.created_turn),
                (theirs.promotion, theirs.xp, theirs.created_turn),
                "{path}: boat {} (promotion, xp, created turn)",
                ours.id
            );
            assert_eq!((p.promotion, p.created_turn), (q.promotion, q.created_turn), "{path}: passenger {}", p.id);
            compared += 1;
            promoted_passengers += usize::from(p.promotion > 0);
        }
    }
    eprintln!("{} files, {compared} boats compared, {promoted_passengers} with a promoted passenger", inputs.len());
    assert!(compared > 0, "no boat in the replays matched the game's");
}

/// A land unit that walks onto a port starts its turn elsewhere. In retail both the new boat and the passenger record
/// the tile of the boarding as their last turn-end tile (weekly game 5af11d91, archer 50 boarding boat 92 at (7,2) from
/// (8,1): the server's final state has (7,2) for both). Nothing but the end of turn updates the tile of a passenger.
#[test]
fn boat_and_passenger_remember_the_boarding_tile() {
    let opts = GameOptions {
        players: vec![PlayerSetup { tribe: 2, human: true }, PlayerSetup { tribe: 3, human: false }],
        ..GameOptions::default()
    };
    let mut e = Engine::new_game(&opts);
    let i = e.s.units.iter().position(|u| u.owner == 1 && u.passenger.is_none()).expect("a unit of player 1");
    let start = e.s.units[i].coordinates;
    let id = e.s.units[i].id;
    e.s.units[i].coordinates = polytopia_sim::WorldCoordinates { x: start.x + 1, y: start.y };
    assert_eq!(e.s.units[i].previous_turn_end, start, "the unit began the turn on its first tile");
    e.embark_pub(id);

    let boat = e.s.units.iter().find(|u| u.passenger.as_ref().is_some_and(|p| p.id == id)).expect("the boat");
    let passenger = boat.passenger.as_deref().unwrap();
    assert_eq!(boat.previous_turn_end, boat.coordinates, "the boat is new where it stands");
    assert_eq!(passenger.previous_turn_end, boat.coordinates, "the passenger remembers the boarding tile");
}

/// The week 10-26 weekly game 5af11d91: every boat of the engine's final state that is also in the server's final
/// state carries a passenger with the server's last turn-end tile. Skips without the replay files
/// (`POLYTOPIA_WEEKLY_RAW`).
#[test]
fn replayed_passengers_have_the_servers_turn_end_tile() {
    let dir = std::env::var("POLYTOPIA_WEEKLY_RAW").unwrap_or_default();
    let game = "5af11d91-9cae-4104-f2b1-08df2c4dfe0c";
    let Ok(save) = SaveFile::open(&format!("{dir}/{game}_init.bin+{dir}/{game}_cur.bin")) else {
        eprintln!("skipped: {game} absent in {dir}");
        return;
    };
    let mut e = Engine::from_state(save.initial.clone());
    if save.commands.iter().any(|c| c.kind == 20) {
        e.start_match();
    }
    for c in &save.commands {
        if c.kind == 20 || c.player == 255 {
            continue;
        }
        let Some(cmd) = e.command_from_raw(c) else {
            e.s.current_command += 1;
            continue;
        };
        e.apply_unchecked(cmd);
    }
    let mut compared = 0;
    for theirs in save.current.units.iter().filter(|u| u.passenger.is_some()) {
        let Some(ours) = e.s.units.iter().find(|u| u.id == theirs.id) else { continue };
        let (p, q) = (ours.passenger.as_deref().unwrap(), theirs.passenger.as_deref().unwrap());
        assert_eq!(p.id, q.id, "boat {}: the passenger", ours.id);
        assert_eq!(p.previous_turn_end, q.previous_turn_end, "boat {} passenger {}: last turn-end tile", ours.id, p.id);
        compared += 1;
    }
    assert!(compared > 0, "no boat to compare");
}
