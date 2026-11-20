# The AI

The engine includes the game's computer player. It is deterministic: the same position and command counter give the same move, because its noise comes from a hash of the game seed and the command counter, not from a random generator that changes between runs. Per-area detail is in [specs/ai-decide.md](specs/ai-decide.md) (difficulty, noise, move search order) and [specs/ai-score.md](specs/ai-score.md) (scoring weights). This page ties them to the engine code.

## Where the code is

| Part | Code |
| --- | --- |
| Pure decision rules and weights (no board access) | `src/ai/decide.rs`, `src/ai/score.rs` |
| Situation analysis, missions, hit list | `src/engine/ai.rs` |
| Move driver, unit moves, train and upgrade scoring | `src/engine/ai_move.rs` |
| Position and battle rating, city threat | `src/engine/ai_rate.rs` |
| Research needs and research scoring | `src/engine/ai_research.rs` |
| Improvement and road scoring | `src/engine/ai_improve.rs` |
| Peace, embassy and break-peace offers | `src/engine/ai_diplomacy.rs` |
| Opinion and aggression | `src/engine/opinion.rs` |
| Per-player scratch state | `src/engine/ai_state.rs` |

The two modules in `src/ai/` take every board fact as an argument. The files in `src/engine/` read the board and call them.

## Asking for a move

- `Engine::ai_get_move` returns the command the AI would play next for the current player.
- `Engine::step_ai` and `Engine::step_ai_cmd` play it. They end the turn instead when a player has issued more than 400 commands in one turn, or when the chosen command is not in `legal_commands`.
- `Engine::ai_candidates` lists every command the AI would score, best first, without the noise (used by planners).
- `examples/play_ai.rs` and `examples/play_game.rs` run whole games this way; see [ENGINE.md](../ENGINE.md).

## How a move is chosen

`ai_get_move` goes through these steps in order and returns the first command it finds.

1. **Pending trigger.** A waiting peace request is answered with `should_accept_peace`. A waiting city reward is chosen with `choose_city_reward` from a hash of the city and the command counter.
2. **Early exits.** Nature, a finished game, the final turn, and a Frozen bot (handicap -1) end the turn.
3. **Situation analysis** (`analyze_situation`, run again before every command). It clears the player's AI state, then walks every explored tile and builds:
   - exploration missions: a reachable tile with at least 2 unexplored tiles around it (radius 2 on a mountain) scores 5 per unexplored tile; only the best `3 * cities` (at least 1) are kept;
   - capture missions for cities: unowned and unoccupied 50, own city with an enemy standing in it 200, an enemy city `aggression * 0.06` (or `* 0.005` when another unit stands in it); none against a player it is at peace with;
   - ruin and resource missions at 30 each (a resource counts once per improvement that the player has unlocked and can build on it);
   - the hit list: every visible foreign unit scored by its location (inside an own city 100, a neutral city 20, another city 15, a ruin 20), plus a fifth of the best battle rating against an own unit, plus 30 or 10 for each own or neutral city it could move to;
   - the military statistics of every player, the player's military needs (`adjust_military_needs`), the share of mountain and forest among explored tiles, and a position rating of the tiles around each own unit (`blocked_tiles`).

   Finally noise is added to every mission and then every hit-list entry, and both lists are sorted best first.
4. **Unit pass.** Each own unit, in tile order, is first checked for something to do: attack options, healing others, promotion, exploding, disbanding with the right tech, or an affordable upgrade. The first pass takes units with range above 1 or the Surprise ability, the second pass takes all units. The first unit with a worthwhile move wins (`best_unit_move`): staying scores the current position rating times 0.8, a move scores the rating of the destination plus 5, an attack scores the battle rating plus any hit-list score of the target, adjusted for aggression and for the position it would end on. Unit commands also include healing, promotion and similar actions (`add_unit_action_options`). Position rating is `rate_position_unit`: threats, friends, terrain, nearby cities and missions around the tile. Battle rating is `rate_battle`, built on [the combat rules](combat.md).
5. **Tile commands** (`tile_command_candidates`), scored together and the best taken:
   - research (`research_candidates`): the value of what the tech unlocks (units, buildings, abilities, city upgrades) times a price factor;
   - training in each own city and upgrading units standing on a tile, scored against the stats the player wants (`desired_stats`), with a bonus for defence where the city is threatened;
   - building improvements (`improvement_commands`) and roads (`road_commands`);
   - diplomacy (`diplomacy_ai_commands`): peace, embassy and breaking peace, see [diplomacy](diplomacy.md).
6. **End the turn** if nothing was found.

Within each step the candidates get noise added, are sorted best first, and only the head of the list is looked at: if the best candidate is a research or build the player cannot afford, that step finds nothing (the AI does not fall back to the second best).

The "unit need" (`decide::unit_need`) is higher while the player has explored little for its number of cities (below `20 * cities + 25` tiles), and with more hit-list entries and missions; each unit the player already has lowers it. A bot with few units therefore trains more.

## Difficulty and noise

Difficulty is stored as the player's `handicap` (Easy 0, Normal 1, Hard 2, Crazy 4, Frozen -1). In the engine it matters in two ways:

- Frozen never moves: it ends its turn at once.
- Stupidity (`Engine::stupidity`) is `max(0, 2 - handicap)` for an auto-play player. The noise added to mission, hit-list and command scores is `stupidity * (hash - 0.5) * 50`, so Easy is noisy, Normal less so, Hard and Crazy not at all.

Hard and Crazy therefore choose identically. The game's difficulty score bonus (`difficulty_bonus_multiplier` in `src/ai/decide.rs`) is ported and tested but the engine's move search does not use it. Details are in [specs/ai-decide.md](specs/ai-decide.md).

## Opinion

Opinion and aggression decide who the AI attacks, who it accepts peace from and who it offers embassies to. They are explained in [diplomacy](diplomacy.md#opinion-and-the-ais-choices). The goal of the game mode feeds into opinion too: the AI dislikes whoever is winning under the current mode ([game modes](game-modes.md)).

## What the engine does not do

- The `specific_command` filter of the generic search in `src/ai/decide.rs` is not used by the engine.
- Tentacle owners are not modelled in the position rating.
- Unit moves can still differ from a recorded retail game where two candidates score almost the same. [ENGINE.md](../ENGINE.md) lists how closely recorded games are reproduced.

## Related

- [AI scoring weights](specs/ai-score.md)
- [AI decisions and difficulty](specs/ai-decide.md)
- [Seed search from a starting view](map-generation.md#seed-search)
