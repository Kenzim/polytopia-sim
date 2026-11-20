//! Position files (`engine/position.rs`), the map editor's game format.
//!
//! Run as `cargo run --release --example position -- <command> ...`; the commands are:
//!   position from-save <file.state> [out.json]   retail save (its current position) -> position file
//!   position check <file.state>...               save -> position -> engine: is it the same state?
//!   position moves <position.json|line.json> [--json]   the player to move: legal commands and the AI's choice
//!   position apply <request.json>                play a command or let the AI play; prints the new position
//!   position meta [version]                      the rules tables the editor needs (units, techs, tribes, improvements)
//!
//! A line file is `{"position": <position at the start of a player's turn>, "steps": [{"text", "ai"}]}`: the
//! commands played since, by their Debug text. The AI keeps state within a turn (its missions) that a position
//! file does not hold, so AI steps are replayed through `ai_get_move` to rebuild it. A request is a line plus
//! `"action"`: `{"cmd": "<Debug text of a legal command>"}` or `{"ai": "move" | "turn" | "human"}` (one AI
//! command; the AI plays out the current player's turn; the AI plays until a human player is to move).
use polytopia_sim::engine::{Command, Engine};
use polytopia_sim::json::{obj, Json};

fn load_save(path: &str) -> Engine {
    let bytes = std::fs::read(path).unwrap_or_else(|e| panic!("{path}: {e}"));
    Engine::from_save(&bytes).unwrap_or_else(|e| panic!("{path}: {e}"))
}

fn cmd_json(e: &Engine, c: &Command) -> Json {
    e.command_json(c)
}

/// The player to move, their legal commands and the AI's choice (worked out on a copy, so `e` is not touched).
fn moves_json(e: &Engine) -> Json {
    let me = e.current_player();
    let commands: Vec<Json> = e.legal_commands().iter().map(|c| cmd_json(e, c)).collect();
    let mut ai_engine = e.clone();
    let ai = ai_engine.ai_get_move();
    obj()
        .set("turn", e.s.current_turn)
        .set("player", me)
        .set("stars", e.player(me).map(|p| p.currency).unwrap_or(0))
        .set("human", e.player(me).is_some_and(|p| !p.auto_play))
        .set("over", e.is_over())
        .set("commands", Json::Arr(commands))
        .set("ai", cmd_json(e, &ai))
        .build()
}

/// One AI command for the current player, with the guards `Engine::step_ai` uses. `per_turn` counts the
/// commands of this player's turn.
fn ai_step(e: &mut Engine, per_turn: &mut u32) -> Command {
    let mut cmd = e.ai_get_move();
    *per_turn += 1;
    if *per_turn > 400
        || (!matches!(cmd, Command::EndTurn | Command::Stay { .. }) && !e.legal_commands().contains(&cmd))
    {
        cmd = Command::EndTurn;
    }
    cmd
}

/// A position being played forward: the engine, the position it started its current turn from, and the steps since.
struct Line {
    e: Engine,
    /// Set when an EndTurn was played: the position the steps now start from.
    base: Option<Json>,
    steps: Vec<(String, bool)>,
    per_turn: u32,
    warnings: Vec<String>,
}

impl Line {
    fn open(req: &Json) -> Result<Line, String> {
        let pos = req.get("position").ok_or("no position")?;
        let e = Engine::from_position(pos)?;
        let mut line = Line { e, base: None, steps: Vec::new(), per_turn: 0, warnings: Vec::new() };
        for s in req.get("steps").map(Json::as_arr).unwrap_or(&[]) {
            let text = s.get("text").and_then(Json::as_str).ok_or("step without text")?;
            if s.get("ai").and_then(Json::as_bool).unwrap_or(false) {
                let cmd = line.play_ai();
                if format!("{cmd:?}") != text {
                    line.warnings.push(format!("the AI now plays {cmd:?} where the line had {text}"));
                }
            } else {
                line.play_text(text)?;
            }
        }
        Ok(line)
    }

    fn play(&mut self, cmd: Command, ai: bool) {
        let player = self.e.current_player();
        self.e.apply_unchecked(cmd);
        self.steps.push((format!("{cmd:?}"), ai));
        if self.e.current_player() != player || cmd == Command::EndTurn {
            self.per_turn = 0;
        }
        if cmd == Command::EndTurn {
            // a fresh load at the start of a turn plays like the engine that got there (replay-check), so the
            // line can start again from here
            self.base = Some(self.e.to_position());
            self.steps.clear();
        }
    }

    fn play_ai(&mut self) -> Command {
        let cmd = ai_step(&mut self.e, &mut self.per_turn);
        self.play(cmd, true);
        cmd
    }

    fn play_text(&mut self, text: &str) -> Result<Command, String> {
        let cmd = self
            .e
            .legal_commands()
            .into_iter()
            .find(|c| format!("{c:?}") == text)
            .ok_or_else(|| format!("not a legal command now: {text}"))?;
        self.play(cmd, false);
        Ok(cmd)
    }
}

/// `apply`: play the request's action and report the commands played, the new position and its moves.
fn apply(req: &Json) -> Result<Json, String> {
    let mut line = Line::open(req)?;
    let action = req.get("action").ok_or("no action")?;
    let mut log = Vec::new();
    // each command is described in the position it was played in, so `from` is where the unit stood
    let mut record = |before: &Engine, cmd: &Command, ai: bool| {
        let Json::Obj(mut fields) = cmd_json(before, cmd) else { unreachable!() };
        fields.push(("player".into(), Json::from(before.current_player())));
        fields.push(("turn".into(), Json::from(before.s.current_turn)));
        fields.push(("ai".into(), Json::from(ai)));
        log.push(Json::Obj(fields));
    };
    if let Some(text) = action.get("cmd").and_then(Json::as_str) {
        let before = line.e.clone();
        let cmd = line.play_text(text)?;
        record(&before, &cmd, false);
    } else if let Some(mode) = action.get("ai").and_then(Json::as_str) {
        let start_player = line.e.current_player();
        let start_turn = line.e.s.current_turn;
        let is_human = |e: &Engine| e.player(e.current_player()).is_some_and(|p| !p.auto_play);
        let mut guard = 0;
        loop {
            if line.e.is_over() || guard >= 20_000 {
                break;
            }
            match mode {
                "move" if guard >= 1 => break,
                "turn" if line.e.current_player() != start_player || line.e.s.current_turn != start_turn => break,
                "human" if is_human(&line.e) && guard >= 1 => break,
                "human" if is_human(&line.e) => {
                    return Err(format!("player {} is human; use the AI for the turn instead", start_player))
                }
                "move" | "turn" | "human" => {}
                _ => return Err(format!("unknown AI mode {mode}")),
            }
            guard += 1;
            let before = line.e.clone();
            let cmd = line.play_ai();
            record(&before, &cmd, true);
        }
    } else {
        return Err("action needs cmd or ai".into());
    }
    let scores: Vec<Json> = line
        .e
        .s
        .players
        .iter()
        .filter(|p| p.id != polytopia_sim::engine::NATURE)
        .map(|p| Json::from(vec![Json::from(p.id), Json::from(line.e.score(p.id))]))
        .collect();
    let steps: Vec<Json> =
        line.steps.iter().map(|(t, ai)| obj().set("text", t.as_str()).set("ai", *ai).build()).collect();
    Ok(obj()
        .set("position", line.e.to_position())
        .set("base", line.base.take().unwrap_or(Json::Null))
        .set("steps", Json::Arr(steps))
        .set("log", Json::Arr(log))
        .set("moves", moves_json(&line.e))
        .set("scores", Json::Arr(scores))
        .set("warnings", line.warnings.iter().map(String::as_str).collect::<Vec<_>>())
        .build())
}

/// Field-by-field differences between two engines (the Debug text of each part).
fn diff(a: &Engine, b: &Engine) -> Vec<String> {
    let mut out = Vec::new();
    let (x, y) = (&a.s, &b.s);
    let mut part = |name: &str, l: String, r: String| {
        if l != r {
            let k = l.bytes().zip(r.bytes()).position(|(p, q)| p != q).unwrap_or(l.len().min(r.len()));
            let from = k.saturating_sub(80);
            out.push(format!(
                "{name}: ...{}... vs ...{}...",
                &l[from..(k + 80).min(l.len())],
                &r[from..(k + 80).min(r.len())]
            ));
        }
    };
    part(
        "header",
        format!(
            "{} {} {} {} {} {:?} {:?}",
            x.version,
            x.seed,
            x.current_turn,
            x.current_player_index,
            x.current_unit_id,
            x.current_state,
            x.current_command
        ),
        format!(
            "{} {} {} {} {} {:?} {:?}",
            y.version,
            y.seed,
            y.current_turn,
            y.current_player_index,
            y.current_unit_id,
            y.current_state,
            y.current_command
        ),
    );
    part("settings", format!("{:?}", x.settings), format!("{:?}", y.settings));
    part("tiles", format!("{:?}", x.map), format!("{:?}", y.map));
    part("players", format!("{:?}", x.players), format!("{:?}", y.players));
    part("player_runtime", format!("{:?}", x.player_runtime), format!("{:?}", y.player_runtime));
    part("units", format!("{:?}", x.units), format!("{:?}", y.units));
    part("improvements", format!("{:?}", x.improvements), format!("{:?}", y.improvements));
    part("tech", format!("{:?}", x.tech), format!("{:?}", y.tech));
    part("diplomacy", format!("{:?}", x.diplomacy), format!("{:?}", y.diplomacy));
    part("fog", format!("{:?}", x.fog), format!("{:?}", y.fog));
    part("tasks", format!("{:?}", x.tasks), format!("{:?}", y.tasks));
    part("city_names", format!("{:?}", x.city_names), format!("{:?}", y.city_names));
    part(
        "triggers",
        format!("{:?} {:?}", a.triggers, a.peace_requests),
        format!("{:?} {:?}", b.triggers, b.peace_requests),
    );
    out
}

fn main() {
    let a: Vec<String> = std::env::args().collect();
    match a.get(1).map(String::as_str) {
        Some("from-save") => {
            // the position the save is at now (the file also holds the start of the game and the commands since)
            let save = polytopia_sim::engine::replay::SaveFile::open(&a[2]).unwrap_or_else(|e| panic!("{}: {e}", a[2]));
            let e = Engine::from_state(save.current);
            let text = e.to_position().to_string();
            match a.get(3) {
                Some(out) => std::fs::write(out, text).expect("write"),
                None => println!("{text}"),
            }
        }
        Some("check") => {
            let mut bad = 0;
            for path in &a[2..] {
                let e = load_save(path);
                let text = e.to_position().to_string();
                let back = Engine::from_position(&Json::parse(&text).expect("parse")).expect("load");
                let d = diff(&e, &back);
                let same_moves = e.clone().legal_commands() == back.clone().legal_commands();
                println!(
                    "{path}: {} ({} bytes), legal commands {}",
                    if d.is_empty() { "identical" } else { "DIFFERS" },
                    text.len(),
                    if same_moves { "same" } else { "DIFFER" }
                );
                for line in &d {
                    println!("  {line}");
                }
                if !d.is_empty() || !same_moves {
                    bad += 1;
                }
            }
            std::process::exit(if bad > 0 { 1 } else { 0 });
        }
        Some("replay-check") => {
            // replay a recorded game; at the start of every player's turn, save the position, load it and check
            // that the state and the AI's next move are the same
            use polytopia_sim::engine::replay::SaveFile;
            use polytopia_sim::engine::Command;
            let save = SaveFile::open(&a[2]).expect("open");
            let cur = Engine::from_state(save.current.clone());
            let back = Engine::from_position(&cur.to_position()).expect("load");
            let d = diff(&cur, &back);
            println!("final state: {}", if d.is_empty() { "identical" } else { "DIFFERS" });
            for line in d.iter().take(5) {
                println!("  {line}");
            }
            let mut e = Engine::from_state(save.initial.clone());
            e.start_match();
            let (mut checked, mut bad) = (0, 0);
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
                if c.kind == 35 {
                    if e.resign(c.n as u8).is_err() {
                        e.s.current_command += 1;
                    }
                    continue;
                }
                let Some(cmd) = e.command_from_raw(c) else {
                    e.s.current_command += 1;
                    continue;
                };
                e.apply_unchecked(cmd);
                if cmd != Command::EndTurn || e.is_over() {
                    continue;
                }
                let back = Engine::from_position(&e.to_position()).expect("load");
                let d = diff(&e, &back);
                let (m1, m2) = (e.clone().ai_get_move(), back.clone().ai_get_move());
                checked += 1;
                if !d.is_empty() || m1 != m2 {
                    bad += 1;
                    if bad == 1 && std::env::var("UNITS").is_ok() {
                        for (u, w) in e.s.units.iter().zip(back.s.units.iter()) {
                            if u != w {
                                println!("  {u:?}\n  {w:?}");
                            }
                        }
                        println!("  counts {} {}", e.s.units.len(), back.s.units.len());
                    }
                    if bad <= 5 {
                        println!("turn {} player {}: AI {:?} vs {:?}", e.s.current_turn, e.current_player(), m1, m2);
                        for line in d.iter().take(5) {
                            println!("  {line}");
                        }
                    }
                }
            }
            println!("{checked} turn starts checked, {bad} differ");
        }
        Some("moves") => {
            let text = std::fs::read_to_string(&a[2]).unwrap_or_else(|e| panic!("{}: {e}", a[2]));
            let v = Json::parse(&text).unwrap_or_else(|e| panic!("{}: {e}", a[2]));
            let (mut e, warnings) = if v.get("position").is_some() {
                let line = Line::open(&v).unwrap_or_else(|e| panic!("{}: {e}", a[2]));
                (line.e, line.warnings)
            } else {
                (Engine::from_position(&v).unwrap_or_else(|e| panic!("{}: {e}", a[2])), Vec::new())
            };
            let me = e.current_player();
            if a.get(3).map(String::as_str) == Some("--json") {
                let Json::Obj(mut fields) = moves_json(&e) else { unreachable!() };
                fields.push(("warnings".into(), Json::from(warnings.iter().map(String::as_str).collect::<Vec<_>>())));
                println!("{}", Json::Obj(fields));
                return;
            }
            println!(
                "turn {} player {} ({} stars)",
                e.s.current_turn,
                me,
                e.player(me).map(|p| p.currency).unwrap_or(0)
            );
            for c in e.legal_commands() {
                println!("  {c:?}");
            }
            println!("AI move: {:?}", e.ai_get_move());
        }
        Some("apply") => {
            let text = std::fs::read_to_string(&a[2]).unwrap_or_else(|e| panic!("{}: {e}", a[2]));
            let req = Json::parse(&text).unwrap_or_else(|e| panic!("{}: {e}", a[2]));
            match apply(&req) {
                Ok(out) => println!("{out}"),
                Err(err) => {
                    eprintln!("{err}");
                    std::process::exit(1);
                }
            }
        }
        Some("meta") => {
            // the rules tables the editor needs: units, techs, tribes, improvements
            use polytopia_sim::json::obj;
            let version = a.get(2).and_then(|v| v.parse().ok()).unwrap_or(123);
            let t = Engine::tables_for(version);
            let l = &t.logic;
            let strs = |v: &[String]| Json::from(v.iter().map(|s| s.as_str()).collect::<Vec<_>>());
            let mut units: Vec<_> = l.units.values().collect();
            units.sort_by_key(|u| u.idx);
            let units: Vec<Json> = units
                .iter()
                .map(|u| {
                    obj()
                        .set("idx", u.idx)
                        .set("id", u.id.as_str())
                        .set("hidden", u.hidden)
                        .set("cost", u.cost)
                        .set("health", u.health)
                        .set("attack", u.attack)
                        .set("defence", u.defence)
                        .set("movement", u.movement)
                        .set("range", u.range)
                        .set("abilities", strs(&u.unit_abilities))
                        .set("upgrades_from", u.upgrades_from.clone().map(Json::from).unwrap_or(Json::Null))
                        .build()
                })
                .collect();
            let mut techs: Vec<_> = l.tech.values().collect();
            techs.sort_by_key(|x| x.idx);
            let techs: Vec<Json> = techs
                .iter()
                .map(|x| {
                    obj()
                        .set("idx", x.idx)
                        .set("id", x.id.as_str())
                        .set("cost", x.cost)
                        .set("unlocks", strs(&x.tech_unlocks))
                        .set("units", strs(&x.unit_unlocks))
                        .set("improvements", strs(&x.improvement_unlocks))
                        .set("abilities", strs(&x.ability_unlocks))
                        .build()
                })
                .collect();
            let mut tribes: Vec<_> = l.tribes.values().collect();
            tribes.sort_by_key(|x| x.idx);
            let tribes: Vec<Json> = tribes
                .iter()
                .map(|x| {
                    obj()
                        .set("idx", x.idx)
                        .set("id", x.id.as_str())
                        .set("color", x.color)
                        .set("stars", x.starting_stars.unwrap_or(5))
                        .set("starting_unit", x.starting_unit.as_str())
                        .set("starting_tech", strs(&x.starting_tech))
                        .set("abilities", strs(&x.tribe_abilities))
                        .build()
                })
                .collect();
            let mut imps: Vec<_> = l.improvements.values().collect();
            imps.sort_by_key(|x| x.idx);
            let imps: Vec<Json> = imps
                .iter()
                .map(|x| {
                    obj()
                        .set("idx", x.idx)
                        .set("id", x.id.as_str())
                        .set("hidden", x.hidden)
                        .set("cost", x.cost.unwrap_or(0))
                        .set("max_level", x.max_level.unwrap_or(0))
                        .set("abilities", strs(&x.improvement_abilities))
                        .build()
                })
                .collect();
            let tech_idx: Vec<Json> =
                t.tech_idx.iter().map(|(k, v)| Json::from(vec![Json::from(k.as_str()), Json::from(*v)])).collect();
            let out = obj()
                .set("version", version)
                .set("units", Json::Arr(units))
                .set("techs", Json::Arr(techs))
                .set("tribes", Json::Arr(tribes))
                .set("improvements", Json::Arr(imps))
                .set("tech_idx", Json::Arr(tech_idx))
                .build();
            println!("{out}");
        }
        _ => {
            eprintln!("usage: position from-save <file.state> [out.json] | check <file.state>... | moves <position.json|line.json> [--json] | apply <request.json> | meta [version]");
            std::process::exit(2);
        }
    }
}
