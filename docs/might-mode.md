# Might mode

Might is game mode 4 (`GAME_MODE_MIGHT`). The game ends when one player holds every capital.

## The rule

- A capital is the tile a player started on (`PlayerState.start_tile`, the original capital). It is fixed for the
  whole game. Capturing a city moves ownership of the tile, never the capital itself.
- Every player in the game has one, Nature has none. The total is `Engine::capital_total()`.
- A player holds a capital while they own that tile. A player's own capital counts like any other: a player who
  has lost it must take it back before they can win.
- The game is decided the moment a living player holds all of them. The check runs after every command, so a capture
  that takes the last capital puts the game into `FinalTurn` (see [Game modes](game-modes.md#when-a-game-ends)); the
  end-match command (`Command::EndTurn` there) then ends it.
- Capitals of eliminated players count. Their cities are gone, so each is held by whoever captured it.
- The winner is that player (`Engine::winners == [id]` once the end-match command has set status `Ended`).
- A turn limit switches the capital rule off: a game with `turn_limit > 0` ends at the limit, as in retail. The
  Might preset has no turn limit and no score limit. Leave both at 0.
- Might also ends when one player is left alive (the extermination rule, which the preset sets), and in the other
  cases the existing check already covers (no human player left). Then the winner is the living player holding the
  most capitals, ties by score.
- Rankings in Might (`players_sorted_by_rank`, used by the AI) already order by capitals held and then score.

## Settings

`GameSettings::set_game_mode(mode)` sets `base_game_mode` and `rules_game_mode`. For Might it also sets
`win_by_capital` and `win_by_extermination` and clears both limits. For any other mode it only clears
`win_by_capital`, so existing modes are set up exactly as before. The rule itself keys on `win_by_capital`, the
way the retail rules object does, so a retail save that carries the flag plays by it whatever its mode number.

A position JSON written by `to_position` carries the mode and the two flags under `game.settings`
(`base_game_mode`, `rules_game_mode`, `win_by_capital`, `win_by_extermination`). A position with rules mode 4 that
leaves the two flags out gets the Might preset's.

## API

```rust
use polytopia_sim::engine::{Engine, GameOptions, PlayerSetup};
use polytopia_sim::GAME_MODE_MIGHT;

// New generated game
let mut opts = GameOptions::default();
opts.mode = GAME_MODE_MIGHT;          // turn_limit and score_limit stay 0
let mut e = Engine::new_game(&opts);

// Or switch a loaded position (editor map, saved game) to Might
let mut e = Engine::from_position(&json)?;
e.s.settings.set_game_mode(GAME_MODE_MIGHT);

// Reading the state
e.is_over();            // status is Ended (after the end-match command; FinalTurn until then)
e.game_winners();       // Vec<u8>: empty while running; works on a reloaded finished position too
e.winners;              // same, but only filled in the engine that played the game to its end
e.capital_total();      // number of capitals in the game
e.capital_counts();     // [(player id, capitals held)], Nature left out
e.capital_holder();     // Some(id) once a living player holds them all
```

A position file does not carry `winners`, so `game_winners()` works them out again from the board for a game that
is over.
