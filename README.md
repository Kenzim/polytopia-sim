# polytopia-sim

A rules engine for The Battle of Polytopia, written in Rust: the game state, every command and its legality, turn
flow and scoring, the map generator, the game's built-in AI, readers for the game's save files, and JSON positions.
Given the same seed and the same commands, a game plays out the same way every time.

It is meant for bots, analysis tools, map tools and AI research. It is checked against the real game: 44 recorded
games replay command for command and end in the same state as the server's, and ten generated maps match the game's
tile for tile (see [How it is checked](#how-it-is-checked)). It is fast: about 80,000 decisions per second on one
core (random play on an 11 x 11 map), 1.5 to 1.6 times polybots-engine (see [Speed](#speed)).

This is an unofficial fan project. It is not affiliated with, endorsed by or connected to Midjiwan AB, the makers of
The Battle of Polytopia. See [Licence and disclaimer](#licence-and-disclaimer).

## Quick start

[docs/usage.md](docs/usage.md) has a short guide to all three ways in, with more examples.

### Rust

```toml
[dependencies]
polytopia-sim = { git = "https://git.stackken.com/kenzim/polytopia-sim" }
```

```rust
use polytopia_sim::game::{GameMode, NewGame};
use polytopia_sim::MapSize;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut game = NewGame::new().seed(7).size(MapSize::Tiny).mode(GameMode::Might).human("imperius").ai("bardur").build()?;
    for cmd in game.legal_commands() {
        println!("{cmd}");                     // e.g. Move { unit: 1, to: WorldCoordinates { x: 4, y: 5 } }
    }
    let cmd = game.parse_command("EndTurn")?;
    game.apply(cmd)?;                           // refuses an illegal command
    let mut ai_commands = 0;
    game.step_ai(&mut ai_commands);             // the built-in AI plays one command for the player to move
    let copy = game.clone();                    // copies are cheap (a few microseconds), for search
    let json = copy.to_position();              // polytopia_sim::Engine::from_position reads it back
    println!("{} bytes of position", json.to_string().len());
    Ok(())
}
```

`polytopia_sim::game` is the stable surface: setting games up by name, command text and JSON, opening game files,
scores and winners, and replaying saves. The rest of the crate is public too, for tools that need the rules in detail,
but may change between minor releases. `cargo doc --open` has the details.

Examples (`cargo run --release --example <name>`): `play_game` (the AI plays every seat), `play_random` (random
legal play), `greedy_search` (a one-command lookahead player against the built-in AI), `replay` and `position`
(the tools behind the command line's `replay` and `check`, with more switches), `digest` (a hash of everything
observable over a fixed set of games, to show that a change to the engine changes no results).

### Command line

```sh
cargo install --path .                                  # installs polytopia-sim
polytopia-sim new --size tiny --human imperius --ai bardur -o game.json
polytopia-sim moves game.json                           # legal commands and the AI's choice
polytopia-sim apply game.json "EndTurn" ai-turn -o next.json
polytopia-sim play --size small --turn-limit 30         # the AI plays every seat
polytopia-sim replay my_game.state                      # replay a save from the game and compare
polytopia-sim record my_game.state -o my_game.json      # the same game without names or account ids
polytopia-sim versions                                  # supported game versions
polytopia-sim bench --size tiny --turn-limit 30         # random playouts per second
```

`polytopia-sim --help` lists every option. Game files are positions (`.json`), saves from the game (`.state`), or
two server records given as `initial.bin+current.bin`.

### Python

```sh
cd python && pip install maturin && maturin develop --release
```

```python
import polytopia_sim as ps

game = ps.Game([("imperius", True), ("bardur", False)], size="tiny", mode="might")
game.play(game.legal_moves()[0])
trial = game.copy()
trial.play_out()
print(trial.scores(), trial.winners())
```

See [python/README.md](python/README.md).

`polytopia_sim.engine_fast` is a drop-in replacement for polybots-engine's `engine_fast` package (its CPU backend):
the same `Batch` class, move rows, observation planes and constants, so a trainer written for it switches engines by
changing its import:

```python
from polytopia_sim import engine_fast   # instead of: import engine_fast
```

The two engines play the same interface but not always the same game; see [docs/usage.md](docs/usage.md#batches-for-training).

## What it covers

- Every tribe: the 12 normal tribes, Cymanti, Polaris (freezing included, see [docs/freezing.md](docs/freezing.md)),
  Elyrion ([docs/elyrion.md](docs/elyrion.md)) and Aquarion ([docs/aquarion.md](docs/aquarion.md)).
- Units, buildings, technologies, city rewards, tasks, diplomacy and the game modes (Perfection, Domination, Glory,
  Might), with fog of war per player.
- The game's map generator for every map size and type, including its search for a fair map.
- The game's built-in AI.
- Saves from the game (versions 121 to 124) and the engine's own JSON position format
  ([ENGINE.md](ENGINE.md#position-files)).
- Game versions 51 to 124, with the rules data of each (below). New games start at version 108 or later.

The mechanics are written up in [docs/](docs/README.md), with reference tables generated from the rules data.

## How it is checked

- **Recorded games.** 44 games recorded from the game (versions 121 to 123; every tribe) replay command by command.
  Every recorded command is legal on the engine, and at the end every player's stars, score, cities, units and techs
  match the server's. The same games compare every move of the game's AI with the engine's AI; they agree on all but
  14 of about 7,300 moves, all by the one Elyrion AI player.
- **Maps.** Ten maps captured from the game are rebuilt tile for tile from their seeds (`tests/map_capture.rs`).
- **Rules.** About 420 tests, many of them pinning a rule found in a recorded game.

The recorded games are not in this repository because they hold other players' accounts; the tests that read them
skip without them (see [Build and test](#build-and-test)).

## Speed

Compared with [polybots-engine](https://github.com/polybots-dev/polybots-engine), another Polytopia engine built for
training throughput, on the same random 1v1 games (no resigning, 1,000 decisions per game) on AMD EPYC 7443
processors, both engines built with Rust 1.92. Decisions per second:

| | 11 x 11, 1 thread | 11 x 11, 48 threads | 16 x 16, 1 thread | 16 x 16, 48 threads |
| --- | ---: | ---: | ---: | ---: |
| polybots-engine, release build | 52,121 | 1,997,371 | 37,561 | 1,442,228 |
| polytopia-sim, release build | 80,269 | 3,020,033 | 60,284 | 2,302,303 |
| polybots-engine, tuned build | 66,314 | 2,628,139 | 46,863 | 1,747,684 |
| polytopia-sim, tuned build | 103,980 | 3,828,428 | 74,999 | 2,829,444 |

That is 1.5 to 1.6 times as many decisions per second, like for like. The tuned builds add full link-time
optimisation, mimalloc and profile-guided optimisation, the same for both. polybots-engine still copies a game state
about twice as fast. [docs/speed.md](docs/speed.md) has the method, per-operation
timings, the caveats and a script to reproduce it.

For your own machine, `polytopia-sim bench --size tiny --turn-limit 30` measures random playouts. For the most speed in
your own program, build with the `search` profile (`cargo build --profile search`: full link-time optimisation) and the
`mimalloc` feature.

## Supported game versions

A game version is **green** when regression replays or captures of that version pass, **amber** when it loads and
runs with its rules data but has no replay evidence, and **red** when the engine refuses it. The lights are
`support_level` in `src/version.rs` (`GAME_VERSION`, `MIN_SUPPORTED_GAME_VERSION`, `MIN_TESTED_GAME_VERSION`,
`MAX_TESTED_GAME_VERSION`).

| Game versions | Light | Tests behind it |
| --- | --- | --- |
| below 51 | red | `version::tests::support_level_boundaries`, `version::tests::rejected_versions_say_why` |
| `51..=120` | amber | `versions::every_supported_version_starts_and_plays_a_few_turns` (every version starts a game and plays four turns); `version::tests::every_supported_game_version_has_loadable_rules`; the rule-split unit tests listed below |
| `121` | green | regress set: 5 games (the engine AI against the retail AI on a weekly challenge, a weekly challenge save, and 3 Aquarion games); `score_parts::parts_sum_on_regression_replays` |
| `122` | green | regress set: 9 games (4 Cymanti, 2 Elyrion, 2 Aquarion, 1 Polaris); `score_parts::parts_sum_on_regression_replays` |
| `123` | green | regress set: 30 games (normal tribes, Cymanti, 5 Polaris, 4 Elyrion and 3 Aquarion games, including saves taken mid-game); `score_parts::parts_sum_on_regression_replays`; the ten map captures `map_capture::capture_a_*` to `capture_j_*` (map generation matches the retail game tile for tile) |
| `124` | amber | retail 2.18 (the current version). It loads, starts and plays (`versions::every_supported_version_starts_and_plays_a_few_turns`) and its one rule change is tested (`explode_order`), but no game played on a 2.18 client has been replayed yet, so it stays amber until one passes the regress set |
| above 124 | red | `version::tests::support_level_boundaries`, `version::tests::rejected_versions_say_why` |

How to read it:
- The regress set is 44 games recorded from the retail game, read from `POLYTOPIA_REGRESS`. It is not in git. The
  `replay` example replays a game command by command and, with `ai`, compares every recorded AI move with the engine
  AI's choice; `score_parts` and `versions::regression_replays_cover_exactly_the_tested_versions` read the same files.
  One game includes an Elyrion AI player: every command replays, but the engine's AI chooses differently from the
  game's on 14 of that player's moves (the engine's AI is not tuned to Elyrion yet).
- Amber has no replay of that version. The version-dependent rules are ported with their version gates, and some
  gates have their own unit test: `rules::economy::tests::start_turn_currency_drops_foreign_tiles_at_version_59`,
  `rules::commands::tests::resign_sets_autoplay_on_v91_and_later`, `rules::combat::tests::peace_treaty_adds_cooldown_only_after_version_99`,
  `rules::economy::tests::improvement_level_splits_at_version_103`, `rules::economy::tests::doubling_follows_the_version_split`,
  `rules::path::tests::port_gate_and_leader_follow_the_version_splits`,
  `rules::path::tests::poison_reduces_movement_only_from_version_115`,
  `rules::commands::tests::heal_cure_and_clamp_follow_the_version_arms`,
  `rules::actions::tests::attack_damage_clamps_and_version_clears_swift` and
  `map_seed::generate_version_98_and_123_return_the_requested_size`. Treat amber results as unverified against the
  retail game.
- The retail save loader reads the version 121 to 124 layouts only (124 has the layout of 123), so older games start
  from `Engine::new_game` (new matches are meant to be `108..=124`, `GameVersion::for_new_match`).
- `tests/versions.rs` reads this section and fails when a range or light disagrees with `support_level`, or when the
  rules table below disagrees with `RULES_LADDER`, so edit both together.

### Rules data per game version

Each supported game version reads one rules file from `data/rules/` (`GameVersion::logic_data_version`).

| Game versions | Rules file |
| --- | --- |
| `51..=59` | `v9` |
| `60..=79` | `v10` |
| `80` | `v11` |
| `81..=82` | `v12` |
| `83..=85` | `v13` |
| `86..=94` | `v14` |
| `95` | `v15` |
| `96..=98` | `v16` |
| `99` | `v17` |
| `100..=103` | `v18` |
| `104` | `v19` |
| `105..=106` | `v20` |
| `107..=109` | `v21` |
| `110..=113` | `v22` |
| `114` | `v23` |
| `115` | `v24` |
| `116` | `v25` |
| `117..=118` | `v26` |
| `119` | `v27` |
| `120..=124` | `v28` |

In code (`src/version.rs`): `GameVersion::for_save` and `for_new_match` return a `VersionError` for a version that is
too old or too new instead of panicking; `GameVersion::at_least(NAVAL_1)` and `is_before` compare against the named
milestones; `MILESTONES` lists them in order and `RULES_LADDER` is the table above, extended down to the oldest game.

## Build and test
```sh
cargo build --release
cargo test --release
cargo clippy --release --all-targets
```

The crate needs no other crates (`mimalloc` is an optional feature). Layout: `src/engine/` (state, commands, turns,
the AI driver, saves and positions), `src/rules/` (the rule formulas), `src/map/` (the map generator), `src/ai/` (the
AI's scoring), `src/game.rs` (the stable surface), `src/main.rs` (the command line), `data/rules/` (the rules data per
version, embedded in the build), `python/` (the Python package), `docs/`.

Some tests replay recorded games that are not in git; each reads its folder from an environment variable and skips
with a note when the variable is unset:

| Variable | What the tests read |
| --- | --- |
| `POLYTOPIA_REGRESS` | the regression games behind the green versions |
| `POLYTOPIA_WEEKLY_RAW` | recorded weekly challenge games |
| `POLYTOPIA_HUMAN_RAW` | recorded games between people (Polaris and others) |
| `POLYTOPIA_CLIENT_GAMES` | online games, finished or mid-game |
| `POLYTOPIA_DIVERGENCE` | online game saves where the engine once disagreed with the server |
| `POLYTOPIA_REJECTED` | online game saves where the server refused a command |
| `POLYTOPIA_WEEKLY_SCRATCH` | the retail AI's move lists from a weekly challenge |
| `WEEKLY_STATE` | this week's challenge save (else the game's save folder) |

Cargo's `[env]` table in a local `.cargo/config.toml` is a convenient place to set them.

## Licence and disclaimer

Licensed under either of the [Apache License, Version 2.0](LICENSE-APACHE) or the [MIT licence](LICENSE-MIT), at
your option (SPDX `MIT OR Apache-2.0`). Unless you say otherwise, any contribution you submit for inclusion in this
project, as defined in the Apache-2.0 licence, is licensed the same way, without any extra terms or conditions.

This is an independent, unofficial reimplementation of the rules of The Battle of Polytopia, made by fans. It is not
affiliated with, endorsed by, sponsored by or connected to Midjiwan AB. "The Battle of Polytopia" and "Polytopia" are
trademarks of Midjiwan AB and are used here only to say which game the engine follows. The repository contains no
art, audio or program code from the game; the rules data in `data/rules/` (unit statistics, costs and technology
trees for each game version) is kept in the engine's own format. Use the engine in a way that respects the game's
terms of service; in particular, do not use it to play online games on behalf of people who have not agreed to it.
