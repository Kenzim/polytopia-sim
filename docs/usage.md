# Using polytopia-sim

There are three ways in: the Rust crate, the `polytopia-sim` command line and the Python package. All three play the
same rules and read the same game files: positions (`.json`, the engine's own format), saves from the game
(`.state`) and recorded games (`.json`, from `polytopia-sim record`).

A game is a sequence of commands. The player to move picks one of the legal commands; `EndTurn` passes the turn on.
The built-in AI can pick for any seat, whether it was set up as human or as AI.

## Rust

```toml
[dependencies]
polytopia-sim = { git = "https://git.stackken.com/kenzim/polytopia-sim" }
```

### Play one seat yourself against the built-in AI

This player looks one command ahead: it plays whichever command raises its score most, and ends its turn when none
does.

```rust
use polytopia_sim::game::{GameMode, NewGame};
use polytopia_sim::{Command, MapSize};

let mut game = NewGame::new()
    .seed(3)
    .size(MapSize::Tiny)
    .mode(GameMode::Might)
    .turn_limit(8)
    .human("imperius")
    .ai("bardur")
    .build()
    .unwrap();
let me = game.current_player();
let mut ai_commands = 0;
while !game.is_over() {
    if game.current_player() != me {
        game.step_ai(&mut ai_commands); // one command by the built-in AI
        continue;
    }
    let legal = game.legal_commands();
    let can_end = legal.contains(&Command::EndTurn); // not while a city reward waits for an answer
    let best = legal
        .iter()
        .filter(|&&c| c != Command::EndTurn)
        .map(|&c| {
            let mut trial = game.clone(); // copies are cheap: a few microseconds
            trial.apply(c).unwrap();
            (trial.score(me), c)
        })
        .max_by_key(|&(score, _)| score);
    match best {
        Some((score, c)) if score > game.score(me) || !can_end => game.apply(c).unwrap(),
        _ => game.apply(Command::EndTurn).unwrap(),
    }
}
println!("scores {:?}, winners {:?}", game.scores(), game.game_winners());
```

`apply` refuses an illegal command with a `Reject`; `apply_unchecked` skips the check when you already know the
command is legal (as search code usually does). Commands print as text (`Move { unit: 3, to: ... }`) and
`game.parse_command(text)` reads them back.

### Save and load games

```rust
use polytopia_sim::game::NewGame;
use polytopia_sim::json::Json;
use polytopia_sim::{Engine, MapSize};

let game = NewGame::new().seed(5).size(MapSize::Tiny).human("oumaji").ai("xin-xi").build().unwrap();
let text = game.to_position().to_string(); // JSON text, ready for a file
let back = Engine::from_position(&Json::parse(&text).unwrap()).unwrap();
assert_eq!(back.current_player(), game.current_player());
assert_eq!(back.legal_commands(), game.legal_commands());
```

`Engine::open_save(path)` opens any game file. `polytopia_sim::game::replay_save(path, check_ai)` replays a save or a
recorded game command by command and reports any command the engine would not allow and any difference from the
final state stored in the file.

The rest of the crate (the rules in detail, the map generator, the AI's scoring) is public too, but only
`polytopia_sim::game` and the methods it documents on `Engine` are kept stable between minor releases. `cargo doc
--open` has the full API.

## Command line

```sh
cargo install --path .     # from a checkout; installs polytopia-sim
```

Start a game, look at the moves, play some, and let the AI finish the turn:

```sh
polytopia-sim new --seed 3 --size tiny --mode might --human imperius --ai bardur -o game.json
polytopia-sim moves game.json                       # the player to move, every legal command and the AI's choice
polytopia-sim apply game.json "Research { tech: 1 }" ai-turn -o game.json
polytopia-sim apply game.json ai-turn ai-turn -o game.json   # the AI plays the next two turns
```

A step for `apply` is a command's text exactly as `moves` prints it, `ai` (one AI command) or `ai-turn` (the AI
plays out the current player's turn). `moves --json` gives the same list as JSON, for other programs.

Other commands:

```sh
polytopia-sim play --size small --ai imperius --ai bardur --turn-limit 30   # the AI plays every seat
polytopia-sim replay my_game.state --ai      # replay a save, compare the end and every AI move
polytopia-sim record my_game.state -o my_game.json   # the same game without player names or account ids
polytopia-sim versions                       # supported game versions and how well each is tested
polytopia-sim bench --size tiny --turn-limit 30   # random playouts per second on this machine
```

`polytopia-sim --help` lists every option.

## Python

```sh
cd python
python3 -m venv .venv && . .venv/bin/activate
pip install maturin && maturin develop --release
```

Choose a move by playing each one out with the built-in AI and keeping the best result:

```python
import polytopia_sim as ps

game = ps.Game([("imperius", True), ("bardur", False)], seed=3, size="tiny", mode="might", turn_limit=8)
me = game.current_player


def final_score(move):
    trial = game.copy()
    trial.play(move)
    trial.play_out()  # the AI plays every seat to the end
    return dict(trial.scores())[me]


best = max(game.legal_moves(), key=final_score)
game.play(best)
print(best, game.to_position()[:60])
```

`ps.Game.open(path)` opens any game file and `ps.replay(path)` replays a save. See
[python/README.md](../python/README.md) for every method.

### Batches for training

`polytopia_sim.engine_fast` steps many games at once from numpy arrays, with the interface of polybots-engine's
`engine_fast` package: the same `Batch` class and methods, move rows (`MOVE_FIELDS`: kind, source, target, option,
opponent, price), observation planes (`TILE_PLANES`), constants (`KINDS`, `STATUS`, `SKIP`) and error messages. Code
written for polybots-engine runs on this engine by changing its import.

```python
import numpy as np
from polytopia_sim import engine_fast

batch = engine_fast.Batch(games=256, size=11, workers=8, outputs=("previews",))
batch.reset(np.arange(256), np.arange(1, 257), [7, 7])   # slots, seeds, the seats' tribe numbers
rng = np.random.default_rng(0)
while (batch.status() == 1).any():
    offsets, rows = batch.moves()                # slot s's rows are rows[offsets[s]:offsets[s + 1]]
    tiles, scalars, players, tech = batch.views()   # what the player to move sees
    counts = np.diff(offsets)
    pick = np.where(batch.status() == 1, rng.integers(0, np.maximum(counts, 1)), engine_fast.SKIP)
    batch.step(pick)                             # one row index per slot
print(batch.winner())
```

What differs from polybots-engine:
- **CPU only.** `backend="gpu"` and the GPU-only methods raise the same errors polybots-engine's CPU backend
  raises.
- **Rules data.** The engine carries the game's rules data, so `rules=` and `ENGINE_FAST_RULES` are accepted and
  ignored.
- **The games themselves.** Seeds give the same maps on both engines (checked on every side from 8 to 30 and every map
  type), except some maps with a Polaris seat on the Continents and Pangea map types, and the first moves match.
  Random games stay identical for about 120 to 170
  decisions, then differ where the engines' rules differ. Where we have checked, this engine is the one that agrees
  with the game: for example polybots-engine credits an embassy's income to the player who built it, but the
  recorded games end with the stars this engine gives (the host earns it). Aquarion games differ from the first
  turn because its flooding is not in this engine yet. Moves on tiles the player has not explored are left out of
  `moves()`, as polybots-engine does (the game itself would accept them).
