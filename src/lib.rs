//! A rules engine for The Battle of Polytopia.
//!
//! The crate (`polytopia-sim`) holds everything needed to play the game without a screen: the game state, the
//! commands a player can issue and their legality, turn flow and scoring, the map generator, the built-in computer
//! player, readers for the game's save files and the version tables that say which game versions are supported.
//! Given the same seed and the same commands, a game plays out the same way every time.
//!
//! # Playing a game
//!
//! [`game::NewGame`] sets up a game by tribe name ([`Engine::new_game`] does the same from [`GameOptions`], with tribe
//! numbers). [`Engine::legal_commands`] lists what the current player may do, [`Engine::apply`] plays one
//! [`Command`] and rejects it with a [`Reject`] if it is not legal, and [`Engine::step_ai`] plays whatever the
//! built-in AI would do next. Commands print as text and read back with [`Engine::parse_command`].
//!
//! ```
//! use polytopia_sim::game::NewGame;
//! use polytopia_sim::{Command, MapSize};
//!
//! let mut game = NewGame::new().seed(7).size(MapSize::Tiny).human("bardur").ai("imperius").turn_limit(3).build().unwrap();
//! assert!(!game.is_over());
//! let me = game.current_player();
//! assert!(game.legal_commands().contains(&Command::EndTurn));
//!
//! // Hand the human's turn over, then let the built-in AI play until the turn limit ends the game.
//! game.apply(Command::EndTurn).unwrap();
//! let mut commands_this_turn = 0;
//! while game.step_ai(&mut commands_this_turn) {}
//! assert!(game.is_over());
//! assert!(game.s.current_turn >= 3);
//! println!("player {me} scored {}", game.score(me));
//! ```
//!
//! # Where things are
//!
//! - [`game`]: the stable surface: setting up games by name, command text and JSON, opening game files. Everything
//!   else is public for tools that need the rules in detail, but may change between minor releases.
//! - [`engine`]: [`Engine`] itself, commands, the AI driver, save loading, replay and position files.
//! - [`rules`]: the rule formulas (combat, movement, economy, diplomacy) the engine calls.
//! - [`map`]: map generation from a seed, size and preset.
//! - [`ai`]: the scoring and decision rules behind the built-in AI.
//! - [`types`]: plain data (the game state, tiles, units, players), also re-exported at the crate root.
//! - [`version`]: [`GAME_VERSION`], the supported window and [`GameVersion`] queries; also re-exported at the root.
//! - [`data`]: the rules data per game version, embedded in the crate.
//! - [`search`]: the map-seed search from a starting view.
//! - [`rng`], [`hash`], [`dotnet_sort`], [`json`]: the supporting pieces (the game's random number generator and hash,
//!   its sort order, and a small JSON reader and writer; the crate has no dependencies).
//!
//! Every tribe is covered: the 12 normal tribes, Cymanti, Polaris, Elyrion and Aquarion. See the README for the
//! supported game versions.

// The ported rules functions take their inputs as separate arguments, one per value the original reads.
#![allow(clippy::too_many_arguments)]

pub mod ai;
pub mod data;
pub mod dotnet_sort;
pub mod engine;
pub mod game;
pub mod hash;
pub mod json;
pub mod map;
pub mod rng;
pub mod rules;
pub mod search;
pub mod types;
pub mod version;

/// Compiles the Rust examples in the README as doctests.
#[cfg(doctest)]
#[doc = include_str!("../README.md")]
struct ReadmeDoctests;

/// Compiles the Rust examples in docs/usage.md as doctests.
#[cfg(doctest)]
#[doc = include_str!("../docs/usage.md")]
struct UsageDoctests;

pub use engine::{Command, Engine, GameOptions, NewGameError, PlayerSetup, Reject, Tables, NATURE};
pub use types::*;
pub use version::*;

#[cfg(feature = "mimalloc")]
#[global_allocator]
static GLOBAL: mimalloc::MiMalloc = mimalloc::MiMalloc;

/// `std::env::var(name)` read once per call site and kept for the life of the process (nothing here sets
/// variables), so the debug switches in the hot paths cost no lock, scan or allocation. Gives
/// `Result<&'static str, std::env::VarError>`.
#[doc(hidden)]
#[macro_export]
macro_rules! env_var {
    ($name:literal) => {{
        static CELL: std::sync::OnceLock<Result<String, std::env::VarError>> = std::sync::OnceLock::new();
        match CELL.get_or_init(|| std::env::var($name)) {
            Ok(v) => Ok(v.as_str()),
            Err(e) => Err(e.clone()),
        }
    }};
}

#[doc(hidden)]
/// `env_var!` for a name that is not a literal: the environment as it was at the first call.
pub fn env_var(name: &str) -> Result<&'static str, std::env::VarError> {
    static ENV: std::sync::OnceLock<Vec<(std::ffi::OsString, std::ffi::OsString)>> = std::sync::OnceLock::new();
    let env = ENV.get_or_init(|| std::env::vars_os().collect());
    match env.iter().find(|(k, _)| k.as_os_str() == std::ffi::OsStr::new(name)) {
        Some((_, v)) => v.to_str().ok_or_else(|| std::env::VarError::NotUnicode(v.clone())),
        None => Err(std::env::VarError::NotPresent),
    }
}
