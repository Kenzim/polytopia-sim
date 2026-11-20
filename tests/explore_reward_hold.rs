//! An explore stops where a city reward is raised, until the reward is answered, and an area is explored nearest tile
//! first.
//!
//! Revealing a lighthouse adds a population to a city, and that can level it up. Retail queues one explore action per
//! tile of the area, ordered by distance (Manhattan distance from the centre, sorted with .NET's unstable
//! introsort), and its action stack stops at the pending city reward: the tiles still queued behind the one that raised
//! it stay unexplored until the reward is answered. The engine used to explore the area column by column and to let one
//! more tile run after the reward.
//!
//! The replay tests are two live games (game version 123, November 2026) that end with a reward still pending: after
//! an ExamineRuins and after an Attack, the server has not explored the tile queued behind the lighthouse. The files
//! come from `POLYTOPIA_DIVERGENCE`. The tests skip without them.
use polytopia_sim::engine::replay::SaveFile;
use polytopia_sim::engine::Engine;

fn divergence_dir() -> String {
    std::env::var("POLYTOPIA_DIVERGENCE").unwrap_or_default()
}

fn replay_matches_server(name: &str) {
    let dir = divergence_dir();
    let (Ok(init), Ok(cur)) =
        (std::fs::read(format!("{dir}/{name}_init.bin")), std::fs::read(format!("{dir}/{name}_cur.bin")))
    else {
        eprintln!("skipped: game {name} blobs absent in {dir}");
        return;
    };
    let save = SaveFile::parse_server(&init, &cur).unwrap();
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
    for p in save.current.players.iter().filter(|p| p.id != 255) {
        let seen = |s: &polytopia_sim::GameState| s.fog.iter().map(|f| f.explorers.contains(&p.id)).collect::<Vec<_>>();
        assert_eq!(seen(&e.s), seen(&save.current), "the tiles player {} has explored", p.id);
        let live = Engine::from_state(save.current.clone());
        assert_eq!(e.score(p.id), live.score(p.id), "player {}'s score", p.id);
    }
}

#[test]
fn an_examine_ruins_stops_exploring_at_the_lighthouse_reward() {
    replay_matches_server("82f99676-65a9-4e4a-9cc1-08df401f00f0-1795009453");
}

#[test]
fn an_attack_stops_exploring_at_the_lighthouse_reward() {
    replay_matches_server("d3bb48d8-000b-40f6-1cd2-08df401f00f0-1794811727");
}

#[test]
fn a_held_explore_survives_a_position_round_trip() {
    use polytopia_sim::engine::{Deferred, ExploreRun, EXPLORE_PATH_MAX};
    use polytopia_sim::json::Json;
    let text = r#"{"format":"polytopia-position","game":{"turn":6,"width":4,"height":4,"current_player":1},
        "players":[{"id":1,"tribe":2,"human":true,"stars":5},{"id":2,"tribe":3,"stars":5}],
        "tiles":[{"x":0,"y":0,"terrain":3,"climate":2,"city":true,"capital":true,"owner":1,"ruling":[0,0]},{"x":3,"y":3,"terrain":3,"climate":2,"city":true,"capital":true,"owner":2,"ruling":[3,3]}],
        "improvements":[{"x":0,"y":0,"level":1},{"x":3,"y":3,"level":1}],
        "units":[]}"#;
    let mut e = Engine::from_position(&Json::parse(text).unwrap()).unwrap();
    let mut path = [0u16; EXPLORE_PATH_MAX];
    path[..3].copy_from_slice(&[5, 6, 10]);
    let held = Deferred::Explore(ExploreRun { player: 1, sight: 1, len: 3, next: 1, area: 4, path });
    e.deferred.push(held);
    let back = Engine::from_position(&e.to_position()).unwrap();
    assert_eq!(back.deferred, vec![held]);
}
