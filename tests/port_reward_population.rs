//! The AI adds an improvement's reward population to its score only when the improvement already scores above zero;
//! growth population is added either way. A tile whose position rating is negative (an embark point beside a unit
//! in danger) pulls a port's score below zero, and the population bonus must not lift it back into the candidate
//! list: the extra candidate shifted the position-based noise of every later candidate and changed the Zebasi
//! bot's pick in the weekly challenge of 2026-11-03.
//!
//! The built position for this lives in `src/engine/ai_improve.rs`. This test reads the weekly rehearsal dumps from
//! `POLYTOPIA_WEEKLY_SCRATCH`, and skips without them.
use polytopia_sim::engine::replay::SaveFile;
use polytopia_sim::engine::{Command, Engine};
use polytopia_sim::json::Json;
use polytopia_sim::WorldCoordinates as Wc;

const PORT: i32 = 8;
const FARM: i32 = 5;

fn scratch_dir() -> String {
    std::env::var("POLYTOPIA_WEEKLY_SCRATCH").unwrap_or_default()
}

/// The weekly rehearsal of 2026-11-03: at command 212 (turn 7) the Zebasi bot has a tile-command list that retail
/// reported in `scratch_ai_t7.json`. The engine's list used to carry one more entry, a port beside Zebasi's own
/// unit that scores below zero, which shifted the noise index of every later entry and flipped a three-way tie
/// between farms: retail builds the farm at (2,10), the engine the one at (1,11).
#[test]
fn weekly_rehearsal_zebasi_candidates_and_pick_match_retail_at_command_212() {
    let dir = scratch_dir();
    let (Ok(init), Ok(cur)) =
        (std::fs::read(format!("{dir}/init.bin")), std::fs::read(format!("{dir}/24_t7_after_end.bin")))
    else {
        eprintln!("skipped: no rehearsal dumps in {dir}");
        return;
    };
    let save = SaveFile::parse_server(&init, &cur).unwrap();
    let mut e = Engine::from_state(save.initial.clone());
    if save.commands.iter().any(|c| c.kind == 20) {
        e.start_match();
    }
    for c in save.commands.iter().filter(|c| c.kind != 20 && c.player != 255) {
        let cmd = e.command_from_raw(c).expect("a supported command");
        e.apply_unchecked(cmd);
    }
    while e.s.current_command < 212 {
        let c = e.ai_get_move();
        e.apply_unchecked(c);
    }
    assert_eq!((e.s.current_command, e.current_player()), (212, 4));

    let me = e.current_player();
    let mut ahead = e.clone();
    e.refresh_map_data_pub(me);
    e.analyze_situation_pub(me);
    let list = e.tile_command_candidates(me);
    let at = |x, y| Wc { x, y };
    let ports: Vec<Wc> = list
        .iter()
        .filter_map(|(c, _)| if let Command::Build { at, improvement: PORT } = c { Some(*at) } else { None })
        .collect();
    assert_eq!(ports, vec![at(1, 8), at(1, 12)], "the port at (2,7) is not offered");
    let farms: Vec<(Wc, f32)> = list
        .iter()
        .filter_map(|(c, s)| if let Command::Build { at, improvement: FARM } = c { Some((*at, *s)) } else { None })
        .collect();
    assert_eq!(farms.iter().map(|f| f.0).collect::<Vec<_>>(), vec![at(2, 10), at(1, 11), at(3, 12)]);
    assert!(farms.iter().all(|(_, s)| (*s - 68.4).abs() < 1e-4), "three farms tie at 68.4: {farms:?}");

    // Retail's own list, when its dump is at hand: same length, same scores in the same order.
    if let Ok(text) = std::fs::read_to_string(format!("{dir}/../scratch_ai_t7.json")) {
        let retail = Json::parse(&text).unwrap();
        let theirs: Vec<f64> = retail.get("lists").unwrap().as_arr()[0]
            .as_arr()
            .iter()
            .map(|o| o.get("score").unwrap().as_f64().unwrap())
            .collect();
        let ours: Vec<f64> = list.iter().map(|(_, s)| f64::from(*s)).collect();
        assert_eq!(ours.len(), theirs.len(), "the lists have the same length: {ours:?} against {theirs:?}");
        for (i, (a, b)) in ours.iter().zip(&theirs).enumerate() {
            assert!((a - b).abs() < 1e-3, "entry {i}: engine {a}, retail {b}");
        }
    }

    let pick = ahead.ai_get_move();
    assert_eq!(pick, Command::Build { at: at(2, 10), improvement: FARM }, "retail picks the farm at (2,10)");
}
