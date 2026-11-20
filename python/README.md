# polytopia-sim for Python

Python bindings for the `polytopia-sim` rules engine: set up games by name, list and play moves as text, let the
built-in AI play, copy games for search, and read and write positions as JSON.

```sh
pip install maturin
maturin develop --release        # from this folder, inside a virtual environment
```

```python
import polytopia_sim as ps

game = ps.Game([("imperius", True), ("bardur", False)], seed=7, size="tiny", mode="might")
print(game.legal_moves())        # ['Move { unit: 1, to: WorldCoordinates { x: 4, y: 5 } }', ..., 'EndTurn']
game.play("EndTurn")
game.step_ai()                   # the built-in AI plays one command for the player to move
trial = game.copy()              # an independent copy
trial.play_out()                 # the AI plays every seat to the end
print(trial.scores(), trial.winners())

text = game.to_position()        # JSON text; ps.Game.from_position(text) reads it back
report = ps.replay("game.state") # replay a save from the game and compare the end with the file
```

`Game(players, seed=1, size="normal", map="continents", mode="domination", turn_limit=0, score_limit=0,
difficulty="normal", version=None, map_attempts=0)`: `players` is a list of `(tribe, human)` pairs in turn order.
`map_attempts=1` sets games up about twenty times faster but gives a different map from the one the game would make.

## engine_fast: batches with polybots-engine's interface

`polytopia_sim.engine_fast` is a drop-in replacement for polybots-engine's `engine_fast` package on the CPU: the same
`Batch(games, size=11, workers=1, backend="cpu", outputs=(), rules=None)` class with `reset`, `moves`, `step`,
`views`, `status`, `winner`, `reason`, `previews`, `defence`, `income` and `max_health`, the same array shapes and
dtypes, and the same constants (`MOVE_FIELDS`, `TILE_PLANES`, `KINDS`, `STATUS`, `OUTPUTS`, `SKIP`).

```python
from polytopia_sim import engine_fast   # instead of: import engine_fast
```

Boards may be 8 or 11 to 30 tiles a side. `rules=` is ignored (the engine has its own rules data) and there is no GPU
backend. See [docs/usage.md](../docs/usage.md#batches-for-training) for an example and for how its games differ from
polybots-engine's.

Run the tests with `pip install pytest && pytest tests`.

Licensed under MIT OR Apache-2.0, like the engine (see `LICENSE-MIT` and `LICENSE-APACHE` in the repository root).
