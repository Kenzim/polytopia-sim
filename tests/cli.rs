//! The command-line tool, run as a program.

use std::process::{Command, Output};

fn tool(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_polytopia-sim")).args(args).output().expect("run polytopia-sim")
}

fn text(o: &Output) -> String {
    String::from_utf8_lossy(&o.stdout).into_owned()
}

#[test]
fn new_moves_apply_check() {
    let dir = std::env::temp_dir().join(format!("polytopia-sim-cli-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let start = dir.join("start.json");
    let after = dir.join("after.json");
    let (start, after) = (start.to_str().unwrap(), after.to_str().unwrap());

    let o = tool(&[
        "new", "--seed", "5", "--size", "tiny", "--mode", "might", "--human", "xin-xi", "--ai", "cymanti", "-o", start,
    ]);
    assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));

    let o = tool(&["moves", start]);
    assert!(o.status.success());
    let listing = text(&o);
    assert!(listing.starts_with("turn 0, player 1 to move"), "{listing}");
    let first = listing.lines().nth(1).unwrap().trim().to_string();

    let o = tool(&["moves", start, "--json"]);
    let json = polytopia_sim::json::Json::parse(text(&o).trim()).unwrap();
    assert_eq!(json.get("commands").unwrap().as_arr()[0].get("text").and_then(|t| t.as_str()), Some(first.as_str()));

    let o = tool(&["apply", start, &first, "EndTurn", "ai-turn", "-o", after]);
    assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
    assert!(text(&tool(&["moves", after])).starts_with("turn 1, player 1 to move"));

    let o = tool(&["check", start, after]);
    assert!(o.status.success(), "{}", text(&o));
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn play_versions_bench() {
    let o = tool(&["play", "--size", "tiny", "--turn-limit", "5", "--seed", "3"]);
    assert!(o.status.success());
    assert!(text(&o).contains("game over after turn"), "{}", text(&o));

    let o = tool(&["versions"]);
    assert!(text(&o).contains("green"));

    let o = tool(&["bench", "--size", "tiny", "--turn-limit", "3", "--games", "2"]);
    assert!(o.status.success());
    assert!(text(&o).contains("playouts per second"));
}

#[test]
fn mistakes_exit_with_2() {
    for args in [
        &["new", "--size", "enormous"][..],
        &["new", "--human", "atlantis"],
        &["new", "--frob", "1"],
        &["frobnicate"],
        &["moves"],
    ] {
        let o = tool(args);
        assert_eq!(o.status.code(), Some(2), "{args:?}");
        assert!(String::from_utf8_lossy(&o.stderr).starts_with("error: "), "{args:?}");
    }
    assert!(tool(&["--help"]).status.success());
}
