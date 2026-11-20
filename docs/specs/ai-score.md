# AI scoring

These are the retail AI scoring weights. Floats are the exact `f32` constants the game uses. No weight in `src/ai/score.rs` was guessed.

`get_opinion` in `src/ai/score.rs` returns the opinion total or `0` when the opponent key is missing; the opinion update that precedes it in the game is done by the engine (`Engine::opinion` and `update_opinions` in `src/engine/opinion.rs`, recomputed once per command). The AI's static limits are:

| Static | Value |
| --- | --- |
| Love limit | `2` |
| Hate limit | `-2` |
| Long-sight | `4` |

Peace-offer scoring reads the love limit. The hate limit and long-sight are not read by these functions.

## Ported

| Behaviour | Rust |
| --- | --- |
| Price factor (used by the command scorers) | `price_factor` |
| Embassy score | `get_embassy_score` |
| Train-unit command builder | `add_possible_train_unit_commands` |
| Research command builder | `add_possible_research_commands` |
| Improvement command builder | `add_possible_improvement_commands` |
| Road-building command builder | `add_possible_road_building_commands` |
| Diplomacy command builder | `add_possible_diplomacy_commands` |
| Battle rating | `rate_battle` |
| City threat level | `get_city_threat_level` |
| Best attack for a position (the max reduction) | `get_best_attack_for_position` |
| Position score for missions | `get_position_score_for_missions` |
| Position score for an improvement | `get_position_score_for_improvement` |
| Build-unit score | `get_build_unit_score` |
| Desired unit stats | `get_desired_unit_stats` |
| City reward choice | `choose_city_reward` |
| Infiltration reward choice | `choose_infiltration_reward` |
| Opinion lookup | `get_opinion` |
| Break-peace score inside diplomacy | `break_peace_score` |
| Peace-offer score inside diplomacy | `peace_command_score` |
| Position rating | `rate_position` |
| Improvement score | `get_improvement_score` |
| Stupidity (tail of the position rating) | inlined in `rate_position` |

The battle result is not recomputed. `rate_battle` takes its `attack_damage` and `retaliation_damage`. Attack-option enumeration is not in this module: `get_best_attack_for_position` reduces the `(rate, aggression)` pairs that the caller built. `Engine::rate_position_unit` (`src/engine/ai_rate.rs`) builds them from `attack_targets_at`, which wraps `rules::path::attack_options`. `get_build_unit_score` takes that position result as `position_score`.

## Command builders

The builders are `add_possible_train_unit_commands`, `add_possible_research_commands`, `add_possible_improvement_commands`, `add_possible_road_building_commands`, `add_possible_diplomacy_commands`, and the embassy score `get_embassy_score`.

Own-city threat `> 0` adds `threat * 20` to desired defence and sets the train addend to `100` (otherwise `0`). A valid train is kept when `(get_build_unit_score + addend) * price_factor > 0`. Research sums `get_build_unit_score * cities`, adds each improvement sum `/ 5` (or `* limited_scale` when the improvement has the Limited ability), adds `ability_unlocks * 5`, adds `20` per upgradeable city, then multiplies by `price_factor`. The research command is appended even when that score is not positive. Improvement adds `(140 / upgrade_need) * (population / cost)` when the population gate passes (the city is tracked, `upgrade_need > 0`, and either the growth population is positive or the reward population is positive and the improvement's own score is above zero), multiplies by `price_factor`, and keeps the command only when the result is `> 1`. Road score is the city potential divided by the number of route tiles without a road, after tiles that already have a road are removed. Diplomacy dispatches break-peace, peace, and embassy. Embassy is `0` when aggression is `> 5000` or `has_war_with` is true; otherwise `embassy_income * 5` when at peace, plus `aggression * -0.001`, plus `price_factor(embassy_cost) * embassy_income`. `embassy_cost_for_player` uses the data cost at `version <= 113` and `count * 2 + 4` after that. The game-progress value is computed by diplomacy and its return value is not stored before the command loop.

These enumerators are arguments of the functions in `src/ai/score.rs`. `src/engine` supplies each of them when it runs the AI:

| Argument | Supplied by |
| --- | --- |
| trainable units of a city | `train_commands_pub` (`src/engine/ai_move.rs`) |
| unlocked upgrades for a unit | `unlocked_upgrades_for` (`src/engine/apply.rs`) |
| explored-tile set | `is_explored` and `is_explored_at` |
| command validity | `legal_commands`, `ai_head_valid` |
| unlockable techs and tech price | `researchable` and `tech_price` (`src/engine/ai_research.rs`) |
| whether a tech can upgrade a city, the tech-needs check | `can_tech_upgrade_city` and `check_for_tech_needs` (`src/engine/ai_research.rs`) |
| buildable improvements | `build_commands_with` (`src/engine/ai_improve.rs`) |
| population needed to upgrade a city | `can_city_be_upgraded` (`src/engine/ai_rate.rs`) |
| city potential, path search | `city_potential` (`src/engine/ai_improve.rs`), `rules::path::get_path` |
| diplomacy commands against an opponent | `diplomacy_commands_for` (`src/engine/ai_diplomacy.rs`) |
| opinion, aggression | `Engine::opinion` and the aggression function in `src/engine/opinion.rs` |
| units within opponent borders, hash-based range draw | inline in `src/engine/ai_diplomacy.rs` |
| peace and war tests | `Engine::at_peace`, `rules::diplomacy` |
| embassy count | `rules::diplomacy::get_embassy_count`, used in `src/engine/ai_diplomacy.rs` |

The engine does not call `add_possible_diplomacy_commands` or `add_possible_road_building_commands`. It scores diplomacy and road building with its own code in `ai_diplomacy.rs` and `ai_improve.rs`. Only the generic search in `src/ai/decide.rs` calls the builders in this module. One visible difference: the engine's embassy cost is the fixed value 5 at `version <= 113`, where `embassy_cost_for_player` here uses the data cost.

## Weights

### `price_factor`

`max(currency as f32, 1)`, then `max((price / currency) * 0.1 * price, 1)`, then `1 / that`.

### `rate_battle`

| Weight | When |
| --- | --- |
| `1000` | Attacker has Infiltrate (`35`) and the tile holds an opponent city. |
| `0.05` | Multiplies `attack_damage * defender_full_cost`, minus `retaliation_damage * attacker_full_cost` when retaliation `> 0`. Products are 32-bit wraps, then converted to `f32`. |
| `-5` | Attack does not kill, defender data has Poison (`26`), attacker is not poisoned. |
| `50` | Attack does not kill and attacker state has Convert (`8`): return `unit_data.cost * 50` and stop. |
| `10` | Attack kills (`attack_damage >= defender.health`). |
| `5` | Kill and attacker data has Eat (`27`). |
| `-5` | Kill, attacker data has Poison, defender is not poisoned. |
| `10` | Kill and attacker data has Persist (`7`). |
| `-2` | `unit_data.health <= retaliation_damage`. |
| `-2` | Attack does not kill and the attacker's own tile is a city. |
| `-20` | Attacker state lacks Convert and the defender has a follower. |
| `defender defence` | Attacker data has Poison and the defender is not poisoned. Added as a float, not a constant. |
| `2` | Defender is poisoned and resource Spores (`7`) is visible. |
| `5` | That spores case, the hit kills, and the attacker tile is owned by the attacker or is water. A null tile throws. |
| `-10` | Attacker state has neither Poison nor Freeze (`18`), and `attack_damage <= 0`. |

No defender unit returns `0`.

### `get_city_threat_level`

`0.01`. Per explored foreign unit in the radius-3 diagonal area, center excluded, when `movement + (1 if Dash else 0) >= chebyshev`:

`attack * (health / max_health) * 0.01`.

Not a city returns `0`.

### `get_best_attack_for_position`

`0.001`. `rate_battle * aggression * 0.001`. Best starts at `0` and updates only when the new value is strictly greater. Tiles with no unit are skipped.

### `get_position_score_for_missions`

No non-zero constant. `mission.score / max(1, manhattan)`. On the mission tile (reference equality in the game) add `mission.score` again. Capture (`MissionType = 1`) is skipped when `can_capture` is false. The mission walk in `get_desired_unit_stats` computes the Chebyshev distance and ignores the result.

### `get_position_score_for_improvement`

| Weight | When |
| --- | --- |
| `-10` | Own city and threat `<= 0`. |
| `-20` | Integer `defence / defence_bonus - 20` on an own city with threat `> 0`. |
| `-5` | Added to the integer `health / max_health` on that same path, then the sum is multiplied by `0.2`. |
| `0.2` | Scales that own-city defence/health mix. |
| `300` | Own threatened city that cannot support the unit, or `unguarded * 2 + 2 >= currency`. |
| `5` | Cap on `unit_count / city_count` when the city can support the unit and `need < currency`. |
| `-10` | Times integer `currency / need` on that supported path. `need = unguarded_cities * 2 + 2`. |
| `0.1` | Enemy city the unit can capture, or that an ally cannot capture: `hate * 0.1`. |
| `-50` | Enemy city the unit cannot capture but an ally can. |
| `-20` | Improvement has Poison ability (`14`) and the player lacks Decompose (`5`). |
| `-50` | That penalty, and the unit is not poisoned. |
| `-100` | That penalty, and `unit.health < 21`. |

The area for the freeze desire scan is radius `3`, diagonal, center included. Threat's area is radius `3`, diagonal, center excluded.

### `get_build_unit_score`

Defence and attack are each multiplied by `0.1` before the desired-stat product. Movement and range are not. Sum order is `(attack_term + defence_term) + movement_term + range_term`. Each matching desired ability weight is added. `rate_position * 0.5` when the tile is non-null. Then `ai_state.unit_need * 20`.

### `get_desired_unit_stats`

Normalized defence, attack, movement, and range are each multiplied by `20`.

| Ability | Weight |
| --- | --- |
| Dash `1` | `attack * 0.5` |
| Fortify `22` | `defence * 0.5` |
| Escape `2` | `0.5 * movement` |
| Tentacles `44` | `defence` |
| Creep `23` | `movement` |
| Swarm `53` | `movement * 2` |
| Poison `26` | `attack` |
| Stomp `40` | `attack` |
| Double attack `54` | `attack` |
| Splash `14` | `range` |
| Infiltrate `35` | `(enemy_population * 0.2) / (infiltrate_unit_count + 1)` |
| Freeze area `19` | Scan counter, only if the tribe has Freeze and the tile is non-null. |
| Skate `21` | `ice_tiles * 5` under the same condition. |

Scan counter, explored tiles only: `+2` when the tile is water, or unowned with a climate other than the player's; `-80` when a friendly unit has Freeze area; ice terrain (`6`) increments the skate count. Unexplored tiles are ignored.

### Rewards

`choose_city_reward` returns BorderGrowth (`5`) if that reward is in the array, else SuperUnit (`6`) if present, else `rewards[hash >> 31]` (logical shift: index `0` or `1`). `choose_infiltration_reward` uses only the sign-bit index. The hash int comes from the game's seeded random hash (`random_hash`) with `(tile.x, tile.y, current_command)`. The hash-based range draw itself is not reimplemented here.

### Diplomacy scores that are closed

| Weight | When |
| --- | --- |
| `-500` | Opinion is `< 0`. Score is `opinion * -500 - units_inside_borders * 200`. Kept when `> 0`. `200` is an integer multiply, not a float constant. |
| `2` (love limit) | Peace offer: `range(0, 1, field, 0, current_command) * opinion > 2` (the hash-based range). |
| `1000` | Peace offer score when that compare passes. |

### `rate_position`

Peace with a non-zero tile owner and `get_opinion < 0` returns `-1000`. Otherwise:

| Weight | When |
| --- | --- |
| best attack | Added only when `can_attack` is true. |
| `0.7` | `can_attack` and the unit lacks Escape (`2`). Multiplies each hostile `rate_battle` before it is averaged. |
| `0x3dccccc8` | `can_attack` and the unit has Escape. This is the `f32` just under 0.1, not canonical `0.1` (`0x3dcccccd`). |
| `1` | `can_attack` is false. Same battle scale. |
| `20` | Explored friendly or peaceful neighbor whose unit detects hidden units. |
| `2` | Neighbor was not explored by the player. |
| `-30` | Hostile has Freeze area (`19`), and our unit lacks Dash (`1`) and Invisible (`3`). |
| `5` | Our unit has Auto-freeze (`20`) or Freeze area, and the hostile tile is freezable (`is_freezable`). |
| `-20` | Hostile has Convert (`8`) and `can_attack` is false. |
| `10` | That Convert penalty becomes `-10` when `can_attack` is true (`-20 + 10`). |
| `* 3` | After the walk, Invisible subtracts `hostile_count * 3` instead of the average. |
| average | Otherwise, if `hostile_count > 0`, add `sum(-(battle * scale)) / count`. |
| mission score | `get_position_score_for_missions`, added next. |
| improvement score | Added when the tile's improvement is non-null. |
| `attack_damage * 2` | Tile effect Tentacle (`3`) and the tentacle's owner is non-null. Subtracted. |
| `-100` | That attack damage is `>` our unit's health. |
| `-20` | Skate (`21`) and terrain is not Ice (`6`). |
| `-10` | Amphibious (`47`), tile `is_land`, and the tile lacks Flooded (`1`). |
| `/ 1000` | At war (`owner != 0`, not us, no peace) and aggression `< 0`. Quotient is `abs / 1000` (integer), then `score += score * -quotient`. |
| `-0.5`, `50` | `(hash_uint * 2^-32 - 0.5) * stupidity * 50`. `2^-32` is the `f32` `0x2f800000`. |

`stupidity`: auto-play off returns `0`, else `max(0, 2 - handicap)`.

The area is radius `1`, diagonal, center excluded. Same-owner neighbors skip the peace check and are friendly.

These callees are inputs of `rate_position`, not reimplemented in this module (`Engine::rate_position_unit` in `src/engine/ai_rate.rs` supplies them):

- `has_peace_with`
- the player's opinion of another player (`get_opinion`)
- the area around a tile
- the explored-tile test
- the unit on a tile
- whether a unit detects hidden units
- whether a unit has an ability (`has_ability`), and whether a unit or a tile has an effect (`has_effect`)
- whether a tile is freezable (`is_freezable`)
- aggression
- the tentacle owner of a tile
- the battle result (`get_battle_results`)
- whether a tile is land (`is_land`)
- the three-word seeded hash behind the `value` draw

The attack-options scan is not in this module; the engine's scan is `attack_targets_at` in `src/engine/ai_rate.rs`. The engine does not model every input: tentacle owners are not modelled. `is_freezable` is `Engine::tile_freezable` for the player counting tiles of any owner (see [freezing](../freezing.md)). The hash arguments are `(current_command * 1000, tile.x, tile.y)`.

### `get_improvement_score`

`can_build` false returns `0`. Patina (`5`) sets the population base to `max_level * growth_population + reward_population`; otherwise the base is reward population. Integer products wrap in 32 bits, then converted to `f32`.

| Weight | When |
| --- | --- |
| `* 25` | Population base. |
| `* 2` | `currency_reward` of the reward list. |
| `* 20` | `ImprovementData.work`. Sum order is `work + (currency + population)`. |
| created-unit score | The created unit's data is non-null: add `get_build_unit_score` after `get_desired_unit_stats`. |
| `* 20` | Adjacency list non-empty: `growth_population * bonus * 20` and `work * bonus * 20`. |
| `/ 20` | That list contains the Polaris climate (`36`): `bonus += frozen_tile_count / 20` toward zero. |
| `200 / max(cost, 1)` | Ruling city is not the empty coordinates (`-1, -1`), its improvement type is City (`1`), and `base + xp > level`. |
| `30` | Improvement is a route opener (`is_route_opener`), no neighboring route opener, city tile is not connected. |
| `80` | Same route path, city coordinates equal the player's start tile, and `player.cities >= 2`. |
| `/ 10` | The rules game mode is Perfection (`1`): add the rewards' score / 10. |
| ability score | `improvement_ability_score`, always added after the mode term. |
| `* 0.5` | `idx != Road` (`3`) and the tile's resource is non-null and not required by the improvement. |

A neighboring route opener suppresses both `30` and `80`. The neighbor scan is the area around the ruling city, radius `1`, diagonal, center excluded.

These callees stay inputs:

- whether the improvement can be built (`can_build`)
- the population of a reward list and of the growth rewards
- whether the improvement has an ability (`has_ability`)
- the improvement level cap (`max_improvement_level`)
- the currency of a reward list (`currency_reward`)
- the data of the unit an improvement creates
- the adjacency bonus
- list membership tests
- whether an improvement is a route opener (`is_route_opener`)
- the area around a tile
- the improvement data lookup
- whether the city is connected
- the score of a reward list (`score_reward`)
- the improvement ability score (`improvement_ability_score`)
- the tile's resource, and whether the improvement requires it

### Train / research / improvement command pool

Train keeps a command when the priced score is `> 0`. Threat `> 0` adds `threat * 20` to desired defence and uses addend `100` (otherwise `0`). Improvement keeps a command when the priced score is `> 1`, and may add `(140 / upgrade_need) * (population / cost)` first. Research always appends. `1` and `0.1` are the inlined price factor. Research also uses float `5` and `20` while accumulating upgrade need. Road's divisor is the roadless path length, not a pool constant.
