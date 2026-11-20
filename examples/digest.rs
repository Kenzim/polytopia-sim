//! Determinism digest: plays a fixed set of games and prints a hash of everything observable, so an optimisation
//! that changes no behaviour leaves the output identical.
//!
//!   cargo run --release --example digest [quick]
//!
//! Random games (uniform legal commands, End Turn one time in ten) over versions, sizes, presets, modes, tribes
//! and player counts, plus games played by the built-in AI. Hashed: every legal command list in order, every
//! chosen command, and every 25 commands and at the end the game state, triggers, deferred actions, peace
//! requests, winners, AI states and scores. One line per game, then the total.

use polytopia_sim::engine::{Command, Engine, GameOptions, PlayerSetup};
use polytopia_sim::{BotDifficulty, MapPreset, MapSize};

struct Fnv(u64);
impl Fnv {
    fn new() -> Self {
        Fnv(0xcbf2_9ce4_8422_2325)
    }
    fn bytes(&mut self, b: &[u8]) {
        for &x in b {
            self.0 = (self.0 ^ x as u64).wrapping_mul(0x0100_0000_01b3);
        }
    }
    fn debug<T: std::fmt::Debug>(&mut self, v: &T) {
        self.bytes(format!("{v:?}").as_bytes());
    }
}

struct Rng(u64);
impl Rng {
    fn next(&mut self) -> usize {
        self.0 = self.0.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        (self.0 >> 33) as usize
    }
}

fn snapshot(h: &mut Fnv, e: &Engine) {
    h.debug(&e.s);
    h.debug(&e.triggers);
    h.debug(&e.deferred);
    h.debug(&e.peace_requests);
    h.debug(&e.winners);
    h.debug(&e.ai_states);
    for p in &e.s.players {
        h.debug(&(p.id, e.score(p.id)));
    }
}

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

fn random_game(opts: &GameOptions, limit: usize) -> Option<u64> {
    let mut e = Engine::try_new_game(opts).ok()?;
    let mut h = Fnv::new();
    let mut rng = Rng(opts.seed as u64 ^ 0x9E37_79B9_7F4A_7C15);
    snapshot(&mut h, &e);
    for n in 0..limit {
        if e.is_over() {
            break;
        }
        let legal = e.legal_commands();
        h.debug(&legal);
        let c = legal[pick(&legal, &mut rng)];
        e.apply_unchecked(c);
        if n % 25 == 24 {
            snapshot(&mut h, &e);
        }
    }
    snapshot(&mut h, &e);
    Some(h.0)
}

fn ai_game(opts: &GameOptions, limit: usize) -> Option<u64> {
    let mut e = Engine::try_new_game(opts).ok()?;
    let mut h = Fnv::new();
    snapshot(&mut h, &e);
    let (mut per_turn, mut turn) = (0, u32::MAX);
    for n in 0..limit {
        if e.is_over() {
            break;
        }
        if e.s.current_turn != turn {
            turn = e.s.current_turn;
            per_turn = 0;
        }
        let mut c = e.ai_get_move();
        per_turn += 1;
        if per_turn > 400 {
            c = Command::EndTurn;
        }
        h.debug(&c);
        e.apply_unchecked(c);
        if n % 25 == 24 {
            snapshot(&mut h, &e);
        }
    }
    snapshot(&mut h, &e);
    Some(h.0)
}

fn main() {
    let quick = std::env::args().nth(1).as_deref() == Some("quick");
    let versions: &[i32] = if quick { &[124] } else { &[60, 87, 100, 110, 121, 122, 123, 124] };
    let sizes = [MapSize::Tiny, MapSize::Small, MapSize::Normal];
    let presets = [
        MapPreset::Lakes,
        MapPreset::Continents,
        MapPreset::Dryland,
        MapPreset::Archipelago,
        MapPreset::WaterWorld,
        MapPreset::Pangea,
    ];
    let tribes: Vec<i32> = (2..=17).collect();
    let mut total = Fnv::new();
    let mut games = 0;
    let mut k = 0usize;
    for &version in versions {
        for (si, &size) in sizes.iter().enumerate() {
            for players in 2..=4usize {
                for mode in 1..=4 {
                    k += 1;
                    let seed = (k * 7919 % 100_000) as i32 + 1;
                    let seats = (0..players)
                        .map(|i| PlayerSetup { tribe: tribes[(k * 5 + i * 3) % tribes.len()], human: true })
                        .collect();
                    let opts = GameOptions {
                        version,
                        seed,
                        size,
                        preset: presets[(k + si) % presets.len()],
                        players: seats,
                        mode,
                        turn_limit: if mode == 4 { 0 } else { 30 },
                        ..GameOptions::default()
                    };
                    let limit = if quick { 300 } else { 1000 };
                    let line = random_game(&opts, limit);
                    println!(
                        "random v{version} {size:?} {players}p mode{mode} seed{seed}: {}",
                        line.map_or("refused".into(), |x| format!("{x:016x}"))
                    );
                    total.debug(&line);
                    games += 1;
                }
            }
        }
        for (i, players) in [2usize, 3, 4].into_iter().enumerate() {
            k += 1;
            let seed = (k * 104_729 % 100_000) as i32 + 1;
            let seats = (0..players)
                .map(|j| PlayerSetup { tribe: tribes[(k * 3 + j * 5) % tribes.len()], human: false })
                .collect();
            let opts = GameOptions {
                version,
                seed,
                size: sizes[i],
                preset: presets[k % presets.len()],
                players: seats,
                difficulty: [BotDifficulty::Normal, BotDifficulty::Hard, BotDifficulty::Crazy][i],
                turn_limit: 30,
                mode: 1,
                ..GameOptions::default()
            };
            let limit = if quick { 1500 } else { 6000 };
            let line = ai_game(&opts, limit);
            println!(
                "ai v{version} {:?} {players}p seed{seed}: {}",
                sizes[i],
                line.map_or("refused".into(), |x| format!("{x:016x}"))
            );
            total.debug(&line);
            games += 1;
        }
    }
    println!("total {games} games: {:016x}", total.0);
}
