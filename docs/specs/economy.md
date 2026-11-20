# Economy, tech, tribes, score

Rules JSON is not loaded here. Improvement work, tech cost, embassy income per level, and the tribe tech graph are arguments.

No economy function branches on the tribe. There is no ported tribe star bonus. Tribe only selects the tech list and `tech_overrides` inside `unlockable_tech` / `required_tech`; callers pass that graph in.

## Income

The income calculation sums the work of every tile that has an improvement.

Own tile (`tile.owner == player`): `calculate_work(tile, state, level)`.

Another player's capital (`owner != 0` and `capital_of == owner`) with no opponent unit pays `embassy_income(owner, queried_player, state)`. An opponent unit is one whose owner is not `255` and not the capital owner. Every other tile pays 0.

### Non-city work

Used when the improvement's data `idx != 1`, or the tile owner is 0.

- `work > 0`: `max(work * level, 0)` with a wrapping 32-bit multiply, then if doubled, shift left 1.
- `work <= 0`: leave `work` unchanged, then the same shift.

`income_is_doubled`: the improvement has ability `Doubled` (22), and a neighbor is a `Port` (8). Version `<= 101` ignores the port's owner. Version `> 101` requires the port owner to equal the building owner.

### City work

`idx == 1` and `owner != 0`.

- A unit whose owner differs from the tile owner yields 0.
- Version `>= 94` and effect `robbed` (1) yields 0.
- Otherwise `production + level - 1`.
- If the tile coordinates equal the player's start tile: add `handicap`, then add `embassy_income(owner, each_player)` for every player.
- If `xp < 0` (signed 16-bit): add that `xp`. Positive `xp` adds nothing.
- Then `max(result, 0)`. Cities are not doubled.

`production` is an unsigned 16-bit field of the improvement state, zero-extended. `level` is its unsigned 16-bit stored level when the income calculation calls in. `xp` is a signed 16-bit field.

### Embassy

`embassy_income`:

- `embassy_level <= 0` or `has_war_with(other, owner)` yields 0.
- Multiplier is 2 when `other.id != 0` and that relation's `State` is Peace (1); otherwise 1.
- Result is `DiplomacyData.embassyIncome * multiplier * embassy_level` (wrapping 32-bit).

`has_war_with` is an attack-recency check, not `State == War`. Peace returns false immediately. `current_turn - last_attack_turn <= 0` counts as war. A difference of exactly 1 counts only when that attacker's `index_of_player` is `>=` the current player index. Both directions are checked.

### Rewards and cost

A reward row holds `population`, `currency` and `score`.

- `currency_reward` and `population_reward` sum those fields, negatives included. Null or empty is 0.
- `score_reward` sums `max(score, 0)` per row. `score_reward_raw` does not clamp.
- `improvement_currency_cost` is `ImprovementData.cost`. A failed lookup returns 0.

### Start of turn

The start-of-turn action: version `<= 59` runs the V59 arm. Newer games run the default arm. `start_turn_currency` is the currency difference.

Both arms, for the acting player's own tile: skip when the current turn is 0, the improvement is missing, or its data lookup failed. Otherwise the level is updated (see below), then if three-argument `calculate_work` with that level is `> 0`, queue an increase-currency action for that amount, delay 10.

`version > 59` also pays other tiles. Four-argument `calculate_work` uses the stored level, or 0 when there is no improvement. A positive result queues the same currency action, including on turn 0. `version <= 59` skips every tile whose owner is not the actor, so there is no embassy grant.

The increase-currency action does `currency += amount` and checks the Wealth task (4).

The level update returns `calculate_improvement_level` and queues level-up or level-down actions for the difference. The level formula is [`calculate_improvement_level`].

Also queued, not added into the currency list:

- `should_level_up`: signed `xp >` zero-extended `level`. When the tile owner is the current player this queues a city level-up action.
- Attract (ability 10): `improvement_age` is `current_turn - founded`. If `age % growth_rate == 0` (`growth_rate` is an improvement data field; zero faults), a forest neighbor with the same owner, no improvement, and no resource queues a create-resource action for Game (1), reason Attract (1).
- Version `>= 112` and own tile, default arm only: restoring the climate. The engine does not do this (`VERSION_RESTORE_CLIMATE` is defined and unused).
- Both arms check the Pacifist (1) and Explorer (7) tasks, then the tribe connection. The default arm also checks diplomacy. `Engine::start_turn` runs its own versions of these checks (see "How the engine runs a turn start").

The V59 arm also adjusts aggression directly, not through currency:

- A known other player: aggression changes by `-100` when the actor's score is unsigned-greater, otherwise `+100`.
- An unknown other player: `handicap * 100 - 200` (wrapping).
- An enemy unit on the actor's tile: `+500`. If that tile has a City (`has_improvement`), the non-aggression pact is also removed and aggression gets `+2000`.

`calculate_income_for` uses the stored level. Turn income for own tiles uses the level after `calculate_improvement_level`.

### Improvement level

`calculate_improvement_level`. `version <= 103` uses the older formula. `None` means the game faults.

No improvement returns 1 on both arms.

`version <= 103`:

- Lighthouse (47): explorer count `+ 1`, or 1 if the explorer list is null. Returns before data, adjacency, and max level.
- City (1): the stored `ushort` level. Same early return.
- Missing improvement data: throw (`None`).
- Ice Bank (34): `ice_bank_level`, then return.
- `max_level == 0`: stored level. A negative max level does not take this branch.
- Patina (5): `min(age / growth_rate, max_level)` with signed integer division. Zero `growth_rate` faults. Microbes is not added.
- Otherwise, if `adjacency_improvements` is non-empty, return the adjacency bonus. Else the stored level.

`version > 103`:

- Lighthouse base is `(explorer_count or 0) + 1`. Every other type starts at the stored level.
- Missing improvement data returns 1. It does not throw.
- Patina replaces the base with `age / growth_rate` before adjacency. Zero `growth_rate` faults even when adjacency would have replaced the quotient.
- A non-empty adjacency list then replaces the level with the adjacency bonus.
- Ice Bank then replaces it with `ice_bank_level`.
- The cap (`max_improvement_level`) is `max_level`, plus 1 when `idx == Fungi` (37) and the owner has Microbes (14).
- `max_level <= 0` returns the level with no further clamp. Otherwise `min(level, cap)`.

`ice_bank_level` is `floor((float)frozen_tiles / 20)`, then the smaller of that and the Ice Bank `max_level`, truncated toward zero. `count_frozen_tiles` sums `tile_is_frozen`: terrain Ice, or Polaris climate (16) on any terrain other than Water and Ocean.

The adjacency bonus is still an argument. It scans the area of radius 1, branches on Picky (21) and Simple (27), and scores same-owner neighbors through a helper. Pass `Some(bonus)` only when `adjacency_improvements` is non-null and its count is `> 0`. Do not invent the neighbor score.

## Score

The score sheet defaults are:

| Field | Value |
| --- | --- |
| `TILE_VALUE` | 20 |
| `CITY_LEVEL_SCORE` | 50 |
| `CITY_STARVATION_PENALTY` | 100 |
| `CITY_XP_SCORE` | 5 |
| `EXPLORE_VALUE` | 5 |

`rating_limits` and `score_limits` are each a `uint[3]`. Their element values are not known, so they are arguments. `number_of_stars` returns the first index whose limit is strictly greater than `score` (signed compare). If none, it returns the array length. Perfection (mode 1) uses `score_limits`; every other mode uses `rating_limits`.

`tech_score` of a tech is `(float)(cost * 100)`. The state overload looks up the tech and returns 0 when missing.

`unit_score`: version `> 100` uses `full_unit_cost` (unit cost plus passenger cost) times 5. Older versions use the unit's own `UnitData.cost` times 5.

`add_score`: 32-bit `score += delta`. If the score limit is `> 0`, the new score is at least that limit (unsigned), and state is `Started` (2), it pushes an end-game action. `remove_score` subtracts and does not check the limit.

## Tech

`learn_tech`: if the id is already in the owned-tech list, return. Otherwise append it, `currency -= price_paid` (the argument, not the tech's cost), and add `trunc(tech_score)` to score. The use-actions flag only defers that same score addition. Each `task_unlocks` entry enables that task. Ability 8 (capital vision) reveals all capitals. Then the Genius task (2) is checked.

`tech_price`: cost 0 returns 0. Otherwise `ceil((cities - 1) * cost + cost + 4)` with a wrapping 32-bit multiply before the float conversion. Literacy (ability 2) multiplies by the `f32` `0x3F2AAA3B` (about 0.6667) before rounding up.

`required_tech` walks the tribe tech list and returns the first tech whose list contains the target:

| Overload | Field |
| --- | --- |
| tech | `tech_unlocks` (`idx`) |
| improvement | `improvement_unlocks` |
| unit | `unit_unlocks` |
| ability | `ability_unlocks` (enum value) |
| task | `task_unlocks` |
| terrain | `movement_unlocks` key |

`unlockable_tech`: version `> 113` uses the newer rule. Tribe `None` (0) yields nothing. For each tribe tech, already overridden: an owned tech adds each unowned direct child; an unowned tech is added when any direct child is owned. Duplicates are dropped.

Version `<= 113` uses the older rule. Tribe `0` logs and throws. A tribe id missing from the rules data returns nothing. Otherwise it walks the owned-tech list only. Each owned tech, after the tribe override is applied, contributes `tech_unlocks` children that are not owned. The tribe's `tech_overrides` replaces a child whose idx is a key before that owned check. It does not add an unowned parent because a child is owned. `has_tech` is membership in the owned-tech list. The result list drops a repeated tech id; the game's list lookup is by reference, and one tech record exists per id.

## Ported

Income work, doubling, embassy stars, war-for-embassy, reward sums, currency cost, score add/remove and the limit check, star index walk, tech and unit score scales, learn-tech spend and score, tech price, required-tech lookup, unlockable tech for both sides of version `113`, level-up predicate, improvement age, attract remainder test, start-turn currency for both sides of version `59`, improvement level for both sides of version `103`, ice-bank level, and the improvement level cap.

## How the engine runs a turn start

The functions above are the standalone, version-gated rules. `Engine::start_turn` (`src/engine/turn.rs`) does the same job on a `GameState` with its own code and does not call `calculate_improvement_level`, `start_turn_currency` or the V59 arm. What it does:

- Nature only advances the command counter and ends its turn.
- For every other player, on every turn including turn 0: `check_tribe_connections` (`src/engine/vision.rs`), `tasks_start_of_turn` (`src/engine/tasks.rs`, completes Pacifist after 5 quiet turns) and `check_diplomacy` (receives messages, meets unknown senders, ends elapsed cool-downs, queues one `Deferred::PeaceRequest` per peace request).
- Turn 0 stops there: no level-ups, no income.
- Otherwise it walks every tile once. A tile owned by another player pays the embassy income of that player's capital (`foreign_capital_work`). An own city queues `Deferred::CityLevelUp` when `should_level_up` holds. An own non-city improvement compares `Engine::improvement_level` with the stored level and queues one `ImprovementLevelUp` or `ImprovementLevelDown` per step. Each own improvement queues its `tile_work` as `Deferred::Income`.
- `run_deferred` then runs the stack last-in first-out. Level-ups and downs change the stored level (a level-up also adds the growth-reward population to the ruling city). `Engine::add_currency` adds income and checks the Wealth task.
- Scores are refreshed at the end.

Differences from the standalone rules:

| Topic | Standalone rules | Engine |
| --- | --- | --- |
| Improvement level | `calculate_improvement_level` with Lighthouse, Ice Bank and `version <= 103` branches | `Engine::improvement_level` (`src/engine/city.rs`): stored level, Patina `age / growth_rate` (a zero rate counts as 1, no fault), the adjacency bonus, then the cap from `improvement_max_level` (Fungi gets +1 with Microbes). Missing data gives 1. No Lighthouse, Ice Bank or old-version branch |
| Adjacency bonus | argument | `Engine::adjacency_bonus` (`src/engine/city.rs`): counts same-owner neighbours; Picky counts each type once, Simple counts 1 per neighbour, otherwise each neighbour adds its stored level (at least 1). Resources count only on tiles without an improvement |
| Attract | queues a create-resource action | `Engine::start_turn` (`attract_target` in `src/engine/turn.rs`) creates the game; see [elyrion.md](../elyrion.md) |
| Restore climate (`>= 112`) | described above | not done |
| V59 arm and its aggression changes | `start_turn_currency` | not done. `Engine::start_turn` has no branch on the game version for it |
| Explorer task (7) check | at turn start | not checked; the task kind exists |
| Increase-currency action | `increase_currency` in `src/rules/actions.rs` | `Engine::add_currency` adds the amount and checks Wealth |

Tasks (`src/engine/tasks.rs`) are enabled and checked as follows: a tech's task unlocks enable tasks when researched; Killer completes on 10 kills; Metropolis on a level 5 city; Network on 5 connected non-capital cities; ExploreLighthouses when all four map corners are explored; Wealth when currency reaches 100 (`check_wealth`, from `add_currency`); Genius when nothing is left to research; Pacifist after 5 turns without fighting. Capital vision (tech ability 8) is applied in `Engine::do_research` (`src/engine/apply.rs`) and on a later meeting in `Engine::meet` (`src/engine/vision.rs`): the start tile of every player already met is explored.

## Still open

- `rating_limits` / `score_limits` element values. `number_of_stars` and `star_limit_table` exist in `src/rules/economy.rs`, but no limit values are defined and nothing in `src/engine` calls them.
- Restoring the climate at turn start, the Explorer task check and the V59 arm, as listed in the table above.
- Any tribe numeric bonus. `tribe.bonus` is read only by map generation (`src/map/terrain.rs`, `src/map/contents.rs`) and by new-game setup, which starts a tribe with the city level bonus at a level 3 capital (`src/engine/setup.rs`).

Crossed off since this file was written: the adjacency bonus, enabling and checking tasks (except Explorer), the diplomacy and tribe-connection checks, capital reveal, and running the queued increase-currency, city level-up and improvement level actions.
