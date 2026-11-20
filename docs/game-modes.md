# Game modes, game end and scoring

When a game ends, who wins, and how a player's score is added up. The rules are in `src/engine/turn.rs` (`check_game_over`, `finish_game`, `pick_winner`, `compute_score`), `src/engine/apply.rs` (`check_eliminations`) and `GameSettings` in `src/types.rs`.

## Settings

`GameSettings` holds what the end checks read. `GameSettings::set_game_mode(mode)` sets the mode; `Engine::new_game` calls it with `GameOptions.mode`, then copies `turn_limit` and `score_limit` from the options.

| Field | Meaning |
| --- | --- |
| `base_game_mode`, `rules_game_mode` | 1 Perfection, 2 Domination, 3 Glory, 4 Might, 5 Custom (7 is the tutorial mode, which only map generation reads) |
| `turn_limit` | the game ends at this turn; 0 for none |
| `score_limit` | the game ends when any player reaches this score; 0 for none |
| `win_by_capital` | a player holding every capital wins at once (Might) |
| `win_by_extermination` | the game ends when one player is left (set by the Might preset) |
| `player_death_condition` | 0: a player is dead without cities; 1: dead without cities or without units |
| `game_type` | the lobby kind (1 multiplayer and 4 competitive end on the last survivor) |

The mode number by itself does little. Only Might changes any rule on its own account (it sets `win_by_capital` and `win_by_extermination` and clears both limits). Perfection, Domination and Glory differ in the limits the caller sets and in how the AI ranks players (see [AI](ai.md)): a Perfection game is normally played to a turn limit, a Glory game to a score limit. Might has its own page: [Might mode](might-mode.md). The AI treats each mode's goal when it judges opponents (`win_hate` in `src/engine/opinion.rs`).

## When a game ends

`Engine::check_game_over` runs from `check_eliminations`, which `Engine::apply` calls after every command, ending a turn included. When any of 1 to 3 below holds, it queues retail's game-over action, which stores `GameStatus::FinalTurn` and nothing else: the game does not end there. The player to move keeps acting (retail accepted nine more commands after the capture of the last capital, a city reward after another, and an end turn that started no new turn and paid no income). The only way out is the end-match command, which `Command::EndTurn` becomes in that state and which calls `finish_game` (status `Ended`, winners picked). The built-in AI sends it at once. Replays of three retail games pin this (`tests/game_end.rs`).

1. **Extermination.** One living player is left and more than one player started, and at least one of: the extermination rule is on (Domination and Might presets), no living player is a human, the game type is multiplayer or competitive, or the settings carry no rules at all (no turn limit, no score limit, no capital rule). A Perfection game with a limit plays on to its turn limit after the last opponent has gone.
2. **Capitals** (Might). The capital rule is on, no turn limit is set, and one living player holds every capital. See [Might mode](might-mode.md).
3. **Score limit.** Any player's score reaches `score_limit`.
4. **Turn limit.** When a player's end of turn starts a new turn and the new turn number is `turn_limit` or more, the game enters `FinalTurn` as well, but the next player's turn starts first (stars, new turn number) and that player may act in it.

## Who wins

`finish_game` refreshes every score and `pick_winner` picks one winner into `Engine::winners`:

- With the capital rule on and a player holding every capital, that player.
- Otherwise the best living player in retail's ranking (`players_sorted_by_rank`). The key depends on the rules mode: in Domination (mode 2) cities held first (the city counter on the player's record), then score; in Might (mode 4) capitals held first, then score; in every other mode (Perfection, Glory, Custom) score alone. Retail sorts the players with that comparer, then puts the living ones first and takes the head of the list, so a player level on both keys is placed by the .NET list sort, which `players_sorted_by_rank` reproduces.
- Players who are not alive never win.

`Engine::game_winners` returns the same answer for a finished position that was loaded from JSON. `Engine::reward_for` turns the state into a number for tree search: 1 for a winner, 0 for a finished game the player did not win, and half the share of the leader's score while it is running.

The ranking only decides the winner when a game ends with several players alive, which in practice means games with no human player. Retail's winner check takes the head of the ranked list from game version 70 on. Version 69 and below use an older routine (the current player, or the highest score at a turn limit) that the engine does not copy: it applies the ranking above to every version, so a Domination game of version 51 to 69 that ends with several players alive may differ.

## Elimination

`Engine::check_eliminations` runs after every command. A player who is no longer alive (`rules::diplomacy::is_alive`: no city, or under the unit death condition no city or no unit; Nature is always alive):

- loses all units, each paying back half its full cost when that is at least 2, with the unit scores removed;
- loses every embassy they hold and every embassy others hold with them (version 60 and later);
- is skipped in the turn order from then on and cannot win.

The game-end check follows. Capturing a player's last city is the usual way: see [capture and conversion](capture.md). There is no resign command in the engine (see [diplomacy](diplomacy.md#resigning)).

## Scoring

A player's score is recomputed after every command (`Engine::refresh_scores`) from the board, so it never drifts. `Engine::compute_score` adds:

| Part | Points |
| --- | --- |
| Territory | 20 for each tile the player owns (`TILE_VALUE`) |
| Exploration | 5 for each tile the player has seen (`EXPLORE_VALUE`) |
| Units | 5 times the full star cost of each unit the player has, boat and passenger together (`unit_score`; only the unit's own cost up to version 100) |
| Technology | 100 times each tech's cost (`tech_score_points`) |
| Cities | for each city, 50 times (level + 1) (`CITY_LEVEL_SCORE`), plus 5 per xp (`CITY_XP_SCORE`); from version 115 a negative xp costs 100 per point instead, and the city's score never goes below 0; plus the city's base score (Park rewards, +250) |
| Buildings | the base score of each improvement the player owns (temples and monuments set it from their rewards), plus, for patina buildings, `level` times the growth reward's score |

Two details matter when comparing with a recorded game. First, a point waiting behind an unanswered city reward is held back: the level-up score is only counted once the reward is chosen (`refresh_scores` subtracts the pending `Deferred::Score` entries). Second, `Engine::score_parts` splits a score into these parts, and `tests/score_parts.rs` checks that they add up to `Engine::score` at every command of the regression replays (skipped when the regress set is absent). What was checked against retail games is listed in [ENGINE.md](../ENGINE.md).

`rules::economy::number_of_stars` and `star_limit_table` (the one to three star rating of a finished game) exist, but the rating limits are not known, so the engine never uses them.
