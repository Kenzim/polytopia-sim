//! Random-play benchmark of polytopia-sim, matching `bench-theirs` in games and policy.
//!
//!   bench-ours run SIZE THREADS SECONDS
//!   bench-ours ops SIZE SECONDS
//!   bench-ours maps SIZE SEEDS
//!
//! Games: 1v1, Imperius against Imperius (both seats take commands), Might, Lakes map, capped
//! at 1000 commands; a game that ends or reaches the cap is replaced by a new seed. Policy:
//! EndTurn one time in ten, otherwise a uniform pick among the other legal commands (the
//! engine has no Resign command). `run` prints decisions per second over the whole run (new
//! games included) and with new-game time taken out; `ops` times each operation on one thread.

use polytopia_sim::engine::{Command, Engine, GameOptions, PlayerSetup};
use polytopia_sim::{MapPreset, MapSize, GAME_MODE_MIGHT};
use std::time::{Duration, Instant};

const LIMIT: u64 = 1000;

struct Rng(u64);
impl Rng {
    fn next(&mut self) -> usize {
        self.0 = self.0.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        (self.0 >> 33) as usize
    }
}

fn options(seed: i32, size: i32) -> GameOptions {
    let size = match size {
        11 => MapSize::Tiny,
        14 => MapSize::Small,
        16 => MapSize::Normal,
        18 => MapSize::Large,
        _ => panic!("size 11, 14, 16 or 18"),
    };
    GameOptions {
        seed,
        size,
        preset: MapPreset::Lakes,
        players: vec![PlayerSetup { tribe: 7, human: true }, PlayerSetup { tribe: 7, human: true }],
        mode: GAME_MODE_MIGHT,
        ..GameOptions::default()
    }
}

/// Index of the command to play among `legal`.
fn pick(legal: &[Command], rng: &mut Rng) -> usize {
    let r = rng.next();
    let end = legal.iter().position(|c| matches!(c, Command::EndTurn));
    let others = legal.len() - usize::from(end.is_some());
    match end {
        Some(e) if others == 0 || r % 10 == 0 => e,
        _ => {
            let nth = (r / 10) % others.max(1);
            legal.iter().enumerate().filter(|(_, c)| !matches!(c, Command::EndTurn)).nth(nth).map_or(0, |(i, _)| i)
        }
    }
}

#[derive(Default)]
struct Tally {
    decisions: u64,
    games: u64,
    ended: u64,
    capped: u64,
    reset_time: Duration,
}

fn play(size: i32, seed0: i32, stride: i32, until: Instant) -> Tally {
    let mut n = Tally::default();
    let mut rng = Rng(0x9E37_79B9_7F4A_7C15 ^ seed0 as u64);
    let mut seed = seed0;
    while Instant::now() < until {
        let at = Instant::now();
        let mut e = Engine::new_game(&options(seed, size));
        n.reset_time += at.elapsed();
        seed += stride;
        n.games += 1;
        let mut commands = 0;
        loop {
            if e.is_over() {
                n.ended += 1;
                break;
            }
            if commands == LIMIT {
                n.capped += 1;
                break;
            }
            let legal = e.legal_commands();
            let i = pick(&legal, &mut rng);
            e.apply_unchecked(legal[i]);
            commands += 1;
        }
        n.decisions += commands;
    }
    n
}

fn run(a: &[String]) {
    let size: i32 = a[0].parse().unwrap();
    let threads: i32 = a[1].parse().unwrap();
    let secs: f64 = a[2].parse().unwrap();
    let start = Instant::now();
    let until = start + Duration::from_secs_f64(secs);
    let parts: Vec<Tally> = std::thread::scope(|s| {
        let hs: Vec<_> = (0..threads).map(|i| s.spawn(move || play(size, 1 + i, threads, until))).collect();
        hs.into_iter().map(|h| h.join().unwrap()).collect()
    });
    let wall = start.elapsed().as_secs_f64();
    let d: u64 = parts.iter().map(|p| p.decisions).sum();
    let games: u64 = parts.iter().map(|p| p.games).sum();
    let reset_s: f64 = parts.iter().map(|p| p.reset_time.as_secs_f64()).sum::<f64>() / threads as f64;
    println!(
        "{{\"engine\":\"ours\",\"size\":{size},\"threads\":{threads},\"decisions\":{d},\"games\":{games},\"ended\":{},\"capped\":{},\"decisions_per_game\":{:.0},\"dps\":{:.0},\"dps_no_reset\":{:.0}}}",
        parts.iter().map(|p| p.ended).sum::<u64>(),
        parts.iter().map(|p| p.capped).sum::<u64>(),
        d as f64 / games.max(1) as f64,
        d as f64 / wall,
        d as f64 / (wall - reset_s),
    );
}

fn ops(a: &[String]) {
    let size: i32 = a[0].parse().unwrap();
    let secs: f64 = a[1].parse().unwrap();
    let until = Instant::now() + Duration::from_secs_f64(secs);
    let mut rng = Rng(0x9E37_79B9_7F4A_7C15);
    let (mut t_new, mut t_legal, mut t_apply, mut t_clone) = (Duration::ZERO, Duration::ZERO, Duration::ZERO, Duration::ZERO);
    let (mut games, mut d, mut legal_len) = (0u64, 0u64, 0u64);
    let mut seed = 1;
    while Instant::now() < until {
        let at = Instant::now();
        let mut e = Engine::new_game(&options(seed, size));
        t_new += at.elapsed();
        seed += 1;
        games += 1;
        let mut commands = 0;
        while !e.is_over() && commands < LIMIT {
            let at = Instant::now();
            let c = e.clone();
            t_clone += at.elapsed();
            std::hint::black_box(&c);
            drop(c);
            let at = Instant::now();
            let legal = e.legal_commands();
            t_legal += at.elapsed();
            legal_len += legal.len() as u64;
            let i = pick(&legal, &mut rng);
            let at = Instant::now();
            e.apply_unchecked(legal[i]);
            t_apply += at.elapsed();
            commands += 1;
        }
        d += commands;
    }
    let at = Instant::now();
    for _ in 0..20 {
        std::hint::black_box(Engine::tables_for(polytopia_sim::GAME_VERSION));
    }
    let t_tables = at.elapsed();
    let us = |x: Duration, n: u64| x.as_secs_f64() * 1e6 / n.max(1) as f64;
    println!(
        "{{\"engine\":\"ours\",\"size\":{size},\"games\":{games},\"decisions\":{d},\"legal_moves_avg\":{:.1},\"new_game_us\":{:.1},\"of_which_rule_tables_us\":{:.1},\"legal_us\":{:.2},\"apply_us\":{:.2},\"clone_us\":{:.2}}}",
        legal_len as f64 / d.max(1) as f64,
        us(t_new, games), us(t_tables, 20), us(t_legal, d), us(t_apply, d), us(t_clone, d),
    );
}

/// One line per seed: the seed, then terrain.climate.resource.improvement of every tile in index order.
fn maps(a: &[String]) {
    let size: i32 = a[0].parse().unwrap();
    let seeds: i32 = a[1].parse().unwrap();
    for seed in 1..=seeds {
        let e = Engine::new_game(&options(seed, size));
        let tiles: Vec<String> =
            e.s.map.tiles.iter().map(|t| format!("{}.{}.{}.{}", t.terrain, t.climate, t.resource, t.improvement)).collect();
        println!("{seed} {}", tiles.join(" "));
    }
}

fn main() {
    let a: Vec<String> = std::env::args().skip(1).collect();
    match a[0].as_str() {
        "run" => run(&a[1..]),
        "ops" => ops(&a[1..]),
        "maps" => maps(&a[1..]),
        _ => panic!("mode: run, ops or maps"),
    }
}
