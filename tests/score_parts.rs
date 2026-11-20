//! `Engine::score_parts` sums to `Engine::score` for every player: at every command of every regression replay, and
//! at every command of the AI baseline run on this week's challenge. The data is read in place (it is not in git):
//! the regression games from `POLYTOPIA_REGRESS`; missing files are skipped with a note.
use polytopia_sim::engine::replay::SaveFile;
use polytopia_sim::engine::Engine;

mod common;
use common::weekly_file;

fn regress_dir() -> String {
    std::env::var("POLYTOPIA_REGRESS").unwrap_or_default()
}

fn check_all(e: &Engine, what: &str) -> usize {
    let mut n = 0;
    for p in e.s.players.iter().filter(|p| p.id != 255) {
        let parts = e.score_parts(p.id);
        assert_eq!(parts.total(), e.score(p.id), "{what}: player {} parts {:?}", p.id, parts);
        n += 1;
    }
    n
}

fn regress_inputs() -> Vec<String> {
    let mut out = Vec::new();
    let regress = regress_dir();
    let Ok(dir) = std::fs::read_dir(&regress) else { return out };
    let mut names: Vec<String> =
        dir.filter_map(|d| d.ok()).map(|d| d.file_name().to_string_lossy().into_owned()).collect();
    names.sort();
    for n in &names {
        if n.ends_with(".state") {
            out.push(format!("{regress}/{n}"));
        } else if let Some(stem) = n.strip_suffix("_init.bin") {
            out.push(format!("{regress}/{stem}_init.bin+{regress}/{stem}_cur.bin"));
        }
    }
    out
}

#[test]
fn parts_sum_on_regression_replays() {
    let inputs = regress_inputs();
    if inputs.is_empty() {
        eprintln!("skipped: no regression data in {}", regress_dir());
        return;
    }
    let mut checks = 0;
    for path in &inputs {
        let save = SaveFile::open(path).unwrap_or_else(|e| panic!("{path}: {e}"));
        checks += check_all(&Engine::from_state(save.current.clone()), &format!("{path} saved current"));
        let mut e = Engine::from_state(save.initial.clone());
        if save.commands.iter().any(|c| c.kind == 20) {
            e.start_match();
        }
        checks += check_all(&e, &format!("{path} start"));
        for (i, c) in save.commands.iter().enumerate() {
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
            checks += check_all(&e, &format!("{path} after #{i} {cmd:?}"));
        }
    }
    eprintln!("{} files, {checks} player checks", inputs.len());
    assert!(inputs.len() >= 23, "expected the 23 regression games, found {}", inputs.len());
}

#[test]
fn parts_sum_on_weekly_ai_baseline() {
    let Some(path) = weekly_file() else {
        eprintln!("skipped: weekly challenge file not found");
        return;
    };
    let mut e = Engine::from_save(&std::fs::read(&path).unwrap()).unwrap();
    let mut checks = check_all(&e, "weekly start");
    let mut per_turn = 0u32;
    let mut turns = std::collections::BTreeSet::new();
    while !e.is_over() {
        turns.insert(e.s.current_turn);
        let what = format!("weekly turn {} cmd {}", e.s.current_turn, e.s.current_command);
        e.step_ai(&mut per_turn);
        checks += check_all(&e, &what);
    }
    eprintln!("weekly: turns {:?}, {checks} player checks, final {:?}", turns, e.score_parts(1));
    assert_eq!(turns.len(), 10);
    assert_eq!((e.score(1), e.score(2)), (7985, 6005));
}
