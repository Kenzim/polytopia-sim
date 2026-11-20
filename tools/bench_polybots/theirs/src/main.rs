//! Random-play benchmark of engine-fast, matching `bench-ours` move for move in policy.
//!
//!   bench-theirs RULES.json run SIZE THREADS SECONDS [RESIGN [VIEW]]
//!   bench-theirs RULES.json ops SIZE SECONDS
//!   bench-theirs RULES.json maps SIZE SEEDS
//!
//! Games: 1v1, Imperius against Imperius, Might, Lakes map, capped at 1000 decisions; a game
//! that ends or reaches the cap is replaced by a new seed. Policy: EndTurn one time in ten,
//! otherwise a uniform pick among the other legal moves (Resign among them only when
//! RESIGN is 1; default 0). VIEW 1 also writes the mover's view before each decision, as
//! the batch does (default 0).
//! `run` prints decisions per second over the whole run (new games included) and with
//! new-game time taken out; `ops` times each operation on one thread.

use engine_fast::{menu, queries, reset, state::Game, step, tables::Tables, view};
use std::time::{Duration, Instant};

#[cfg(feature = "mimalloc")]
#[global_allocator]
static ALLOC: mimalloc::MiMalloc = mimalloc::MiMalloc;

const END_TURN: i32 = 15;
const RESIGN: i32 = 35;
const LIMIT: u32 = 1000;

struct Rng(u64);
impl Rng {
    fn next(&mut self) -> usize {
        self.0 = self.0.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        (self.0 >> 33) as usize
    }
}

fn config(seed: i32, size: i32) -> reset::Config {
    reset::Config { rules: 124, seed, size, map: 2, mode: 4, tribes: vec![7, 7], action_limit: LIMIT }
}

/// Index of the move to play among `kinds`.
fn pick(kinds: &[i32], resign: bool, rng: &mut Rng) -> usize {
    let r = rng.next();
    let end = kinds.iter().position(|&k| k == END_TURN);
    let other = |k: i32| k != END_TURN && (resign || k != RESIGN);
    let others = kinds.iter().filter(|&&k| other(k)).count();
    match end {
        Some(e) if others == 0 || r % 10 == 0 => e,
        _ => {
            let nth = (r / 10) % others.max(1);
            kinds.iter().enumerate().filter(|(_, &k)| other(k)).nth(nth).map_or(0, |(i, _)| i)
        }
    }
}

fn over(g: &Game) -> bool {
    matches!(queries::winner(g), Ok(Some(_)) | Err(_)) || g.decisions == g.fixed.action_limit
}

fn mover(g: &Game) -> i32 {
    g.players[g.current_player_index as usize].action.id
}

#[derive(Default)]
struct Tally {
    decisions: u64,
    games: u64,
    ended: u64,
    capped: u64,
    refused: u64,
    reset_time: Duration,
}

fn play(catalog: &str, t: &Tables, size: i32, seed0: i32, stride: i32, until: Instant, resign: bool, with_view: bool) -> Tally {
    let mut n = Tally::default();
    let mut rng = Rng(0x9E37_79B9_7F4A_7C15 ^ seed0 as u64);
    let mut seed = seed0;
    let mut v = view::View::default();
    let mut kinds = Vec::new();
    while Instant::now() < until {
        let at = Instant::now();
        let mut g = match reset::reset(&config(seed, size), catalog, t) {
            Ok(g) => g,
            Err(_) => {
                n.refused += 1;
                seed += stride;
                continue;
            }
        };
        n.reset_time += at.elapsed();
        seed += stride;
        n.games += 1;
        let mut kept = view::Kept::default();
        loop {
            if over(&g) {
                if g.decisions == g.fixed.action_limit { n.capped += 1 } else { n.ended += 1 }
                break;
            }
            if with_view && view::write_kept(&g, t, mover(&g), &mut v, &mut kept).is_err() {
                n.refused += 1;
                break;
            }
            let Ok(cands) = menu::menu(&g, t) else { n.refused += 1; break };
            kinds.clear();
            kinds.extend(cands.iter().map(|c| c.action.kind));
            let i = pick(&kinds, resign, &mut rng);
            if step::advance(&mut g, t, &cands[i].action).is_err() {
                n.refused += 1;
                break;
            }
            n.decisions += 1;
        }
    }
    n
}

fn run(catalog: &str, t: &Tables, a: &[String]) {
    let size: i32 = a[0].parse().unwrap();
    let threads: i32 = a[1].parse().unwrap();
    let secs: f64 = a[2].parse().unwrap();
    let resign = a.get(3).is_some_and(|x| x == "1");
    let with_view = a.get(4).is_some_and(|x| x == "1");
    let start = Instant::now();
    let until = start + Duration::from_secs_f64(secs);
    let parts: Vec<Tally> = std::thread::scope(|s| {
        let hs: Vec<_> = (0..threads)
            .map(|i| s.spawn(move || play(catalog, t, size, 1 + i, threads, until, resign, with_view)))
            .collect();
        hs.into_iter().map(|h| h.join().unwrap()).collect()
    });
    let wall = start.elapsed().as_secs_f64();
    let d: u64 = parts.iter().map(|p| p.decisions).sum();
    let games: u64 = parts.iter().map(|p| p.games).sum();
    let reset_s: f64 = parts.iter().map(|p| p.reset_time.as_secs_f64()).sum::<f64>() / threads as f64;
    println!(
        "{{\"engine\":\"theirs\",\"size\":{size},\"threads\":{threads},\"resign\":{resign},\"view\":{with_view},\"decisions\":{d},\"games\":{games},\"ended\":{},\"capped\":{},\"refused\":{},\"decisions_per_game\":{:.0},\"dps\":{:.0},\"dps_no_reset\":{:.0}}}",
        parts.iter().map(|p| p.ended).sum::<u64>(),
        parts.iter().map(|p| p.capped).sum::<u64>(),
        parts.iter().map(|p| p.refused).sum::<u64>(),
        d as f64 / games.max(1) as f64,
        d as f64 / wall,
        d as f64 / (wall - reset_s),
    );
}

fn ops(catalog: &str, t: &Tables, a: &[String]) {
    let size: i32 = a[0].parse().unwrap();
    let secs: f64 = a[1].parse().unwrap();
    let until = Instant::now() + Duration::from_secs_f64(secs);
    let mut rng = Rng(0x9E37_79B9_7F4A_7C15);
    let (mut t_new, mut t_legal, mut t_apply, mut t_clone, mut t_view) = (Duration::ZERO, Duration::ZERO, Duration::ZERO, Duration::ZERO, Duration::ZERO);
    let (mut games, mut d, mut legal_len) = (0u64, 0u64, 0u64);
    let mut v = view::View::default();
    let mut seed = 1;
    while Instant::now() < until {
        let at = Instant::now();
        let Ok(mut g) = reset::reset(&config(seed, size), catalog, t) else { seed += 1; continue };
        t_new += at.elapsed();
        seed += 1;
        games += 1;
        while !over(&g) {
            let at = Instant::now();
            let c = g.clone();
            t_clone += at.elapsed();
            std::hint::black_box(&c);
            drop(c);
            let at = Instant::now();
            view::write(&g, t, mover(&g), &mut v).unwrap();
            t_view += at.elapsed();
            let at = Instant::now();
            let cands = menu::menu(&g, t).unwrap();
            t_legal += at.elapsed();
            legal_len += cands.len() as u64;
            let kinds: Vec<i32> = cands.iter().map(|c| c.action.kind).collect();
            let i = pick(&kinds, false, &mut rng);
            let at = Instant::now();
            if step::advance(&mut g, t, &cands[i].action).is_err() {
                break;
            }
            t_apply += at.elapsed();
            d += 1;
        }
    }
    let us = |x: Duration, n: u64| x.as_secs_f64() * 1e6 / n.max(1) as f64;
    println!(
        "{{\"engine\":\"theirs\",\"size\":{size},\"games\":{games},\"decisions\":{d},\"legal_moves_avg\":{:.1},\"new_game_us\":{:.1},\"legal_us\":{:.2},\"apply_us\":{:.2},\"clone_us\":{:.2},\"view_full_us\":{:.2}}}",
        legal_len as f64 / d.max(1) as f64,
        us(t_new, games), us(t_legal, d), us(t_apply, d), us(t_clone, d), us(t_view, d),
    );
}

/// One line per seed: the seed, then terrain.climate.resource.improvement of every tile in
/// index order (-1 for no resource or improvement).
fn maps(catalog: &str, t: &Tables, a: &[String]) {
    let size: i32 = a[0].parse().unwrap();
    let seeds: i32 = a[1].parse().unwrap();
    for seed in 1..=seeds {
        let g = reset::reset(&config(seed, size), catalog, t).unwrap();
        let tiles: Vec<String> = g
            .tiles
            .iter()
            .map(|x| {
                let imp = x.improvement.as_ref().map_or(-1, |i| i.kind);
                format!("{}.{}.{}.{}", x.terrain, x.climate, x.resource.unwrap_or(-1), imp)
            })
            .collect();
        println!("{seed} {}", tiles.join(" "));
    }
}

fn main() {
    let a: Vec<String> = std::env::args().skip(1).collect();
    let catalog = std::fs::read_to_string(&a[0]).expect("rules file");
    let t = Tables::compile(&catalog).expect("rules");
    match a[1].as_str() {
        "run" => run(&catalog, &t, &a[2..]),
        "ops" => ops(&catalog, &t, &a[2..]),
        "maps" => maps(&catalog, &t, &a[2..]),
        _ => panic!("mode: run, ops or maps"),
    }
}
