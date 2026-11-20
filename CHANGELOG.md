# Changelog

All notable changes are listed here. The project follows [Semantic Versioning](https://semver.org/): while the
version is 0.x, a minor release (0.2, 0.3, ...) may change the public surface and a patch release (0.1.1) does not.
The public surface is the `polytopia_sim::game` module, the methods it documents on `Engine`, the command line, the
position format and the Python package; the rest of the crate may change in any release. A rule fix that makes the
engine agree with the game is a patch change even when it changes results.

## [0.1.0] - unreleased

First public release, under MIT OR Apache-2.0.

- Rules engine for The Battle of Polytopia: every tribe (the 12 normal tribes, Cymanti, Polaris, Elyrion and
  Aquarion); game versions 51 to 124, with recorded games replayed exactly for versions 121 to 123.
- The game's map generator, including its search for a fair map, and the game's built-in AI.
- Saves from the game (versions 121 to 124) and the engine's own JSON position format.
- `polytopia_sim::game`: games set up by name, commands as text and JSON, opening game files, replaying saves.
- The `polytopia-sim` command-line tool: `new`, `moves`, `apply`, `play`, `replay`, `record`, `check`, `versions`,
  `bench`.
- Recorded games: a JSON form of a save (positions and commands only) that replays exactly like the save.
- Python bindings (`python/`, built with maturin).
- Speed: about 1.4 to 1.6 times polybots-engine's decisions per second on random play, both engines built the same
  way, and a faster copy of the game state ([docs/speed.md](docs/speed.md), with the harness in
  `tools/bench_polybots/`). The work behind it changed no results: the `digest` example gives the same hash before
  and after.
- Copying a game: `GameState::clone` copies the unit, improvement and fog lists as blocks and clones only the items
  that own memory, and the engine's small ordered maps are `FlatMap` (a sorted vector, `polytopia_sim::flat_map`)
  instead of `BTreeMap`. `AiState::blocked_tiles` has that type now.
- `Engine::is_legal` checks one command without building the whole legal list, and `Engine::query_scope` shares the
  lookups that many questions about one unchanged position have in common. The built-in AI chooses a move in about
  a quarter less time.
- More rules found by replaying recorded games, each with a test under `tests/`: an area is explored nearest tile
  first and the rest waits behind a pending city reward; a Build is accepted while a city reward is pending; the
  Wealth and Network tasks are checked when the game checks them; a resignation ends a two-player Multiplayer or
  Competitive game; sanctuaries and other improvements that take their level from their surroundings change level
  at once; a bridge's record keeps its orientation; Aquarion's bubble, tentacle, flood and embark rules; Cymanti's
  poison, algae and egg rules; infiltration rewards.
- `TileExplorers::explorers` (the players who have explored a tile) is an `Explorers` value rather than a
  `Vec<u8>`: up to 14 ids are kept in place, so copying a game allocates nothing per tile. It dereferences to `[u8]`
  and has `push`, `clear`, `From<Vec<u8>>` and `FromIterator<u8>`; code that needs a `Vec<u8>` converts with
  `Vec::from` or `.to_vec()`.
- `Engine::end_match` and `Engine::resign` for the game's end-match and resign records, which are not `Command`s;
  `game::replay_save` applies them.
- Usage guide ([docs/usage.md](docs/usage.md)) for the Rust crate, the command line and the Python package.
- Units with `independent` (a Polytaur, a dagger) no longer take the home city they capture, and the daggers an
  infiltration places have no home, as in the game: neither uses up a city's unit slots. Every command of the
  recorded games is now legal, including an Elyrion player's Warrior that a Polytaur's captured village used to block.
- New games: Luxidoor's capital starts at level 3, and the water round a Polaris capital starts as ice, as in the
  game. Saves and positions are unaffected (they carry their own state).
- `polytopia_sim.engine_fast`: a batch interface with the same names, array layouts and behaviour as polybots-engine's
  `engine_fast` package (CPU backend), so programs written for it run on this engine by changing the import. It
  plays boards of 8 and 11 to 30 tiles a side. polybots-engine's own Python tests pass against it.
- `Engine::try_new_game_with_seeds` (a new game on a map of any side, with the map seeds drawn from a callback),
  `map::generate::generate_side_with_seed_source` and `max_players_on_side`.
