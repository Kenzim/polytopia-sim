//! The command-line tool: start games, list and play moves, replay and check saves, print the version table, and
//! measure speed. Run with no arguments for help.

use polytopia_sim::game::{record_save, replay_save, GameMode, NewGame};
use polytopia_sim::json::Json;
use polytopia_sim::version::{support_level, GameVersion, Support, MILESTONES};
use polytopia_sim::{BotDifficulty, Command, Engine, MapPreset, MapSize};
use std::hint::black_box;
use std::process::ExitCode;
use std::time::Instant;

const HELP: &str = "\
Usage: polytopia-sim <command> [arguments]

Commands:
  new [game options] [-o FILE]          Start a game; print its position (JSON) or write it to FILE.
  moves FILE [--json]                   The player to move, their legal commands and the AI's choice.
  apply FILE STEP... [-o FILE]          Play steps and print the new position. A step is a command's text as
                                        listed by `moves`, `ai` (one AI command) or `ai-turn` (the AI plays out
                                        the current player's turn).
  play [game options] [--max-turns N]   The built-in AI plays every seat until the game ends.
  replay SAVE [--ai]                    Replay a save from the game, or a recorded game from `record`, and compare
                                        the end with the file. --ai also compares every recorded AI move with the
                                        engine's AI.
  record SAVE [-o FILE]                 A save from the game as a recorded game (JSON): positions and commands only,
                                        without player names or account ids.
  check FILE...                         Load each game file, write it as a position, load that again, and
                                        check both agree (state and legal commands).
  versions                              The supported game versions and how well each is tested.
  bench [game options] [--games N]      Random playouts per second, and the time to list and apply moves. Playout i
                                        uses map seed --seed + i.

Game files are positions (.json), saves from the game (.state), or two server records given as
initial.bin+current.bin.

Game options:
  --human TRIBE / --ai TRIBE   Add a seat (in turn order). Default: a human Imperius and an AI Bardur
                               (both AI for `play` and `bench`).
  --seed N                     Map seed (default 1).
  --size NAME                  tiny, small, normal (default), large, huge, massive.
  --map NAME                   dryland, lakes, continents (default), archipelago, waterworld, pangea.
  --mode NAME                  domination (default), might, perfection, glory.
  --turn-limit N               End after N turns (0 = none).
  --score-limit N              End when a player reaches N points (0 = none).
  --difficulty NAME            easy, normal (default), hard, crazy.
  --version N                  Game version (default: the newest).
  --map-attempts N             0 (default) = the game's own search for a fair map; 1 = first map, much faster.
";

type Res<T> = Result<T, String>;

/// The arguments after the command, read in order.
struct Args {
    items: Vec<String>,
}

impl Args {
    /// Remove `--name VALUE` and return the value.
    fn value(&mut self, name: &str) -> Res<Option<String>> {
        match self.items.iter().position(|a| a == name) {
            None => Ok(None),
            Some(i) if i + 1 < self.items.len() => {
                let v = self.items.remove(i + 1);
                self.items.remove(i);
                Ok(Some(v))
            }
            Some(_) => Err(format!("{name} needs a value")),
        }
    }

    fn number<T: std::str::FromStr>(&mut self, name: &str) -> Res<Option<T>> {
        self.value(name)?.map(|v| v.parse::<T>().map_err(|_| format!("{name}: not a number: {v}"))).transpose()
    }

    fn parsed<T: std::str::FromStr>(&mut self, name: &str) -> Res<Option<T>>
    where
        T::Err: std::fmt::Display,
    {
        self.value(name)?.map(|v| v.parse::<T>().map_err(|e| e.to_string())).transpose()
    }

    /// Remove a `--name` switch and say whether it was there.
    fn flag(&mut self, name: &str) -> bool {
        let before = self.items.len();
        self.items.retain(|a| a != name);
        self.items.len() != before
    }

    /// Every `--name VALUE` pair of a repeated option, with its position.
    fn repeated(&mut self, names: &[&str]) -> Res<Vec<(String, String)>> {
        let mut out = Vec::new();
        let mut i = 0;
        while i < self.items.len() {
            if names.contains(&self.items[i].as_str()) {
                if i + 1 >= self.items.len() {
                    return Err(format!("{} needs a value", self.items[i]));
                }
                let v = self.items.remove(i + 1);
                out.push((self.items.remove(i), v));
            } else {
                i += 1;
            }
        }
        Ok(out)
    }

    /// The remaining arguments; an unknown option is an error.
    fn rest(self) -> Res<Vec<String>> {
        if let Some(a) = self.items.iter().find(|a| a.starts_with("--")) {
            return Err(format!("unknown option {a}"));
        }
        Ok(self.items)
    }
}

/// Read the game options. `all_ai` makes every seat an AI seat (and the default seats AI).
fn new_game(args: &mut Args, all_ai: bool) -> Res<NewGame> {
    let mut g = NewGame::new();
    if let Some(v) = args.number("--seed")? {
        g = g.seed(v);
    }
    if let Some(v) = args.parsed::<MapSize>("--size")? {
        g = g.size(v);
    }
    if let Some(v) = args.parsed::<MapPreset>("--map")? {
        g = g.map_type(v);
    }
    if let Some(v) = args.parsed::<GameMode>("--mode")? {
        g = g.mode(v);
    }
    if let Some(v) = args.number("--turn-limit")? {
        g = g.turn_limit(v);
    }
    if let Some(v) = args.number("--score-limit")? {
        g = g.score_limit(v);
    }
    if let Some(v) = args.parsed::<BotDifficulty>("--difficulty")? {
        g = g.difficulty(v);
    }
    if let Some(v) = args.number("--version")? {
        g = g.version(v);
    }
    if let Some(v) = args.number("--map-attempts")? {
        g = g.map_attempts(v);
    }
    let mut seats = args.repeated(&["--human", "--ai"])?;
    if seats.is_empty() {
        seats = vec![("--human".into(), "imperius".into()), ("--ai".into(), "bardur".into())];
    }
    for (kind, tribe) in seats {
        g = if kind == "--human" && !all_ai { g.human(&tribe) } else { g.ai(&tribe) };
    }
    Ok(g)
}

fn write_or_print(text: &str, out: Option<String>) -> Res<()> {
    match out {
        Some(path) => std::fs::write(&path, text).map_err(|e| format!("{path}: {e}")),
        None => {
            println!("{text}");
            Ok(())
        }
    }
}

fn one_file(rest: Vec<String>, command: &str) -> Res<String> {
    match <[String; 1]>::try_from(rest) {
        Ok([f]) => Ok(f),
        Err(_) => Err(format!("{command} takes one game file")),
    }
}

fn cmd_new(mut args: Args) -> Res<()> {
    let out = args.value("-o")?;
    let g = new_game(&mut args, false)?;
    args.rest()?;
    let e = g.build().map_err(|e| e.to_string())?;
    write_or_print(&e.to_position().to_string(), out)
}

fn cmd_moves(mut args: Args) -> Res<()> {
    let json = args.flag("--json");
    let e = Engine::open_save(&one_file(args.rest()?, "moves")?)?;
    let legal = e.legal_commands();
    let ai = e.clone().ai_get_move();
    if json {
        let out = polytopia_sim::json::obj()
            .set("turn", e.s.current_turn)
            .set("player", e.current_player())
            .set("over", e.is_over())
            .set("commands", Json::Arr(legal.iter().map(|c| e.command_json(c)).collect()))
            .set("ai", e.command_json(&ai))
            .build();
        println!("{out}");
    } else {
        println!(
            "turn {}, player {} to move, {} legal commands{}",
            e.s.current_turn,
            e.current_player(),
            legal.len(),
            if e.is_over() { " (game over)" } else { "" }
        );
        for c in &legal {
            println!("  {c}");
        }
        println!("AI would play: {ai}");
    }
    Ok(())
}

fn cmd_apply(mut args: Args) -> Res<()> {
    let out = args.value("-o")?;
    let mut rest = args.rest()?;
    if rest.len() < 2 {
        return Err("apply takes a game file and at least one step".into());
    }
    let file = rest.remove(0);
    let mut e = Engine::open_save(&file)?;
    let mut per_turn = 0;
    for step in &rest {
        match step.as_str() {
            "ai" => {
                let c = e.step_ai_cmd(&mut per_turn).ok_or("the game is over")?;
                eprintln!("played {c}");
            }
            "ai-turn" => {
                let (player, turn) = (e.current_player(), e.s.current_turn);
                while e.current_player() == player && e.s.current_turn == turn {
                    let Some(c) = e.step_ai_cmd(&mut per_turn) else { break };
                    eprintln!("played {c}");
                }
            }
            text => {
                let c = e.parse_command(text)?;
                e.apply(c).map_err(|r| format!("{text}: refused ({r:?})"))?;
                per_turn = 0;
            }
        }
    }
    write_or_print(&e.to_position().to_string(), out)
}

fn cmd_play(mut args: Args) -> Res<()> {
    let max_turns: u32 = args.number("--max-turns")?.unwrap_or(200);
    let g = new_game(&mut args, true)?;
    args.rest()?;
    let mut e = g.build().map_err(|e| e.to_string())?;
    let mut per_turn = 0;
    let mut commands = 0u64;
    let start = Instant::now();
    while !e.is_over() && e.s.current_turn < max_turns {
        if e.step_ai_cmd(&mut per_turn).is_none() {
            break;
        }
        commands += 1;
    }
    let scores: Vec<String> = e.scores().iter().map(|(id, s)| format!("player {id}: {s}")).collect();
    println!(
        "{} after turn {} ({commands} commands, {:.2} s)",
        if e.is_over() { "game over" } else { "stopped" },
        e.s.current_turn,
        start.elapsed().as_secs_f64()
    );
    println!("scores: {}", scores.join(", "));
    if e.is_over() {
        println!("winners: {:?}", e.game_winners());
    }
    Ok(())
}

fn cmd_replay(mut args: Args) -> Res<bool> {
    let ai = args.flag("--ai");
    let file = one_file(args.rest()?, "replay")?;
    let r = replay_save(&file, ai)?;
    println!(
        "{} commands replayed to turn {}; {} not legal on the engine, {} bookkeeping records skipped",
        r.commands,
        r.engine.s.current_turn,
        r.illegal.len(),
        r.skipped
    );
    for (i, c) in r.illegal.iter().take(20) {
        println!("  record {i}: {c}");
    }
    if ai {
        let n = r.ai_agreed + r.ai_differed;
        println!("AI moves: the engine's AI chose {} of {n} recorded moves", r.ai_agreed);
    }
    if r.differences.is_empty() {
        println!("final state: every player's stars, score, cities, units and techs match the file");
    } else {
        for d in &r.differences {
            println!("differs: {d}");
        }
    }
    Ok(r.illegal.is_empty() && r.differences.is_empty())
}

fn cmd_record(mut args: Args) -> Res<()> {
    let out = args.value("-o")?;
    let file = one_file(args.rest()?, "record")?;
    write_or_print(&record_save(&file)?.to_string(), out)
}

fn cmd_check(args: Args) -> Res<bool> {
    let files = args.rest()?;
    if files.is_empty() {
        return Err("check takes one or more game files".into());
    }
    let mut ok = true;
    for f in files {
        let e = Engine::open_save(&f)?;
        let text = e.to_position().to_string();
        let back = Engine::from_position(&Json::parse(&text)?)?;
        let same_state = back.to_position().to_string() == text;
        let same_moves = back.legal_commands() == e.legal_commands();
        ok &= same_state && same_moves;
        println!(
            "{f}: state {}, legal commands {}",
            if same_state { "same" } else { "DIFFERS" },
            if same_moves { "same" } else { "DIFFER" }
        );
    }
    Ok(ok)
}

fn cmd_versions() {
    let rules = |v: i32| GameVersion::for_save(v).map(|g| g.logic_data_version()).unwrap_or(0);
    let light = |v: i32| match support_level(v) {
        Support::Tested => "green (replays of real games)",
        Support::Supported => "amber (runs, not checked against real games)",
        Support::Unsupported => "red",
    };
    println!("{:<12} {:<6} {:<46} milestones", "versions", "rules", "status");
    let all: Vec<i32> = GameVersion::supported().map(|v| v.as_i32()).collect();
    let mut i = 0;
    while i < all.len() {
        let mut j = i;
        while j + 1 < all.len()
            && rules(all[j + 1]) == rules(all[i])
            && support_level(all[j + 1]) == support_level(all[i])
        {
            j += 1;
        }
        let names: Vec<&str> =
            MILESTONES.iter().filter(|m| m.version >= all[i] && m.version <= all[j]).map(|m| m.name).collect();
        let range = if i == j { all[i].to_string() } else { format!("{}-{}", all[i], all[j]) };
        println!("{:<12} v{:<5} {:<46} {}", range, rules(all[i]), light(all[i]), names.join(", "));
        i = j + 1;
    }
    println!(
        "New games start at {} or later; the newest is {}.",
        polytopia_sim::MIN_NEW_GAME_VERSION,
        polytopia_sim::GAME_VERSION
    );
}

fn cmd_bench(mut args: Args) -> Res<()> {
    let games: usize = args.number("--games")?.unwrap_or(200);
    // Playout i starts from map seed `--seed` + i.
    let first_seed: i32 = args.number("--seed")?.unwrap_or(1);
    let g = new_game(&mut args, true)?;
    args.rest()?;
    if games == 0 {
        return Err("--games must be at least 1".into());
    }
    let t = Instant::now();
    let mut starts = Vec::with_capacity(games);
    for i in 0..games {
        starts.push(g.clone().seed(first_seed.wrapping_add(i as i32)).build().map_err(|e| e.to_string())?);
    }
    let setup = t.elapsed().as_secs_f64() / games as f64;
    let mut rng = 0x9E37_79B9_7F4A_7C15u64;
    let mut next = || {
        rng ^= rng << 13;
        rng ^= rng >> 7;
        rng ^= rng << 17;
        rng
    };
    let (mut legal_t, mut apply_t, mut clone_t, mut moves, mut lists) = (0.0, 0.0, 0.0, 0u64, 0u64);
    let t = Instant::now();
    for start in &starts {
        let tc = Instant::now();
        let mut e = black_box(start.clone());
        clone_t += tc.elapsed().as_secs_f64();
        let mut n = 0;
        while !e.is_over() && n < 20_000 {
            let tl = Instant::now();
            let legal = e.legal_commands();
            legal_t += tl.elapsed().as_secs_f64();
            lists += 1;
            let others: Vec<Command> = legal.iter().copied().filter(|c| *c != Command::EndTurn).collect();
            let can_end = legal.contains(&Command::EndTurn);
            let cmd = if can_end && (others.is_empty() || next() % 10 == 0) {
                Command::EndTurn
            } else if !others.is_empty() {
                others[(next() % others.len() as u64) as usize]
            } else {
                break;
            };
            let ta = Instant::now();
            e.apply_unchecked(cmd);
            apply_t += ta.elapsed().as_secs_f64();
            n += 1;
        }
        moves += n;
    }
    let total = t.elapsed().as_secs_f64();
    println!("{games} random playouts (90% a random move, else end the turn), one thread");
    println!("  playouts per second   {:.1}", games as f64 / total);
    println!("  commands per second   {:.0}", moves as f64 / total);
    println!("  commands per playout  {:.0}", moves as f64 / games as f64);
    println!("  list legal commands   {:.2} us", legal_t * 1e6 / lists as f64);
    println!("  apply one command     {:.2} us", apply_t * 1e6 / moves.max(1) as f64);
    println!("  copy a starting game  {:.2} us", clone_t * 1e6 / games as f64);
    println!("  set up a new game     {:.2} ms", setup * 1e3);
    Ok(())
}

fn main() -> ExitCode {
    let mut argv: Vec<String> = std::env::args().skip(1).collect();
    if argv.is_empty() || matches!(argv[0].as_str(), "-h" | "--help" | "help") {
        print!("{HELP}");
        return ExitCode::SUCCESS;
    }
    let command = argv.remove(0);
    let args = Args { items: argv };
    let result: Res<bool> = match command.as_str() {
        "new" => cmd_new(args).map(|_| true),
        "moves" => cmd_moves(args).map(|_| true),
        "apply" => cmd_apply(args).map(|_| true),
        "play" => cmd_play(args).map(|_| true),
        "replay" => cmd_replay(args),
        "record" => cmd_record(args).map(|_| true),
        "check" => cmd_check(args),
        "versions" => {
            cmd_versions();
            Ok(true)
        }
        "bench" => cmd_bench(args).map(|_| true),
        other => Err(format!("unknown command {other}; run with --help")),
    };
    match result {
        Ok(true) => ExitCode::SUCCESS,
        Ok(false) => ExitCode::from(1),
        Err(e) => {
            eprintln!("error: {e}");
            ExitCode::from(2)
        }
    }
}
