# Actions that change the board

Implementations are in `src/rules/actions.rs`. Damage comes from `crate::rules::combat::get_battle_results`. Star grants for a turn come from `crate::rules::economy::start_turn_currency`. Tech purchase comes from `learn_tech`.

An attack action only forwards the player id, origin, target, and the action's damage field into the shared attack routine (`perform_attack`).

## Actions that run

| Action | Rust |
| --- | --- |
| Attack | `attack`, `perform_attack` |
| Move | `move_unit` |
| Train | `train` |
| Research (learns the tech) | `research` |
| Build | `build` |
| Build road | `build_road` (index only; see below) |
| Capture city | `capture_city` |
| End turn (ends the player's turn) | `end_turn` |
| Start turn | `start_turn` |
| Increase currency | `increase_currency` |
| City level | `city_level_up` |
| Next unit id | `current_unit_id` increment inside `train` |

`perform_attack` subtracts `min(damage, health)` from the defender's `u16` health. Both units lose Boosted. Health 0 removes the unit, adds one kill on the attacker runtime and one casualty on the defender's owner when that owner is not 255, and adds 1 xp when the tiles differ and the attacker data is not hidden and lacks Carry (11).

Setting currency throws on a negative amount and otherwise stores the amount. `increase_currency` replaces currency. `train` stores `currency - cost` through that rule, so a negative result leaves the board unchanged. The next unit id is `current_unit_id += 1`.

A city level-up does `level += 1`, `xp -= new_level`, `upgrade -= 1`, all in 16-bit wrapping, and only when the city should level up (`xp > level`).

Ending a player's turn is done with `new_turn = false` when the action player is current. The index moves forward. Wrapping increments `current_turn`. Robbed is cleared on that player's improvements. `CoolDown` relations of that player become `Neutral`.

A road build writes the tile's road flag. `Tile` has no such field, so `build_road` returns the tile index and does not invent storage.

## Version splits tested

| Action | Compare | Arms | Test |
| --- | --- | --- | --- |
| Attack | `<= 94`, `<= 104`, `<= 118` | four arms: V94, V104, V116 and Default | `attack_damage_clamps_and_version_clears_swift` (104 keeps Swift/Bubble, 105 clears them) |
| Move | `<= 59`, `<= 83`, `<= 98`, `<= 101` | V59, V83 (also 84..=98), V101 and Default | `move_sets_coordinates_and_skips_flags_on_ice_or_push` (Push skips flags only above 101; ice never sets them) |
| Train | `<= 59`, `<= 105`, inside Default `> 115` | V59, V105 and Default | `train_spends_stars_and_embarks_only_between_106_and_115` |
| Build | `<= 105` | V105 and Default | `build_and_level_up_and_capture_splits` (both arms place the kind) |
| Capture city | `<= 59`, `<= 108` | V59, V108 and Default | same test (diplomacy attack record only above 59; 108 and Default share that write) |
| Start turn | `<= 59` | V59 and Default | `end_turn_wraps_and_start_turn_splits_at_59` |

Train's default arm pushes an embark action for a water tile without Swim (10) or Fly (13) when version is 106..=115. The action is not executed. `TrainResult.embarked` is that push.

## Steps left as arguments or not applied

This table describes `src/rules/actions.rs` on its own. `src/engine` does the steps itself: it applies poison, flood, bubble, area damage, kills, embarking, building (rule area, level-up score), embassy removal, the diplomacy check, the Wealth check and the road flag (`Tile.has_road`). It does not call the functions in this module for them. See [documentation index](../README.md).

| Step | What this module does |
| --- | --- |
| Battle results | called; damage is its `attack_damage` |
| Poison unit action | not applied |
| Flood tile action | not applied |
| Bubble action | not applied; Swift/Bubble bools are cleared on V116 and Default |
| Area damage | not applied (Move default, Train default) |
| Kill unit | unit row removed; spores and the rest of the kill are not |
| Train on occupied space | occupied destination does not spawn |
| Embark action | reported, not executed |
| Rule area and the rest of Build | kind, owner, founder, founded only |
| Level-up score | level arithmetic only, no score |
| Remove embassies | not applied |
| Diplomacy check | not applied |
| Is-alive check | `end_turn` callback |
| Aggression normalisation | not applied; known/unknown and enemy-tile deltas from `economy` are |
| Task check / Wealth | not applied |
| Road flag | not stored |

The diplomacy update after an attack is applied for capture above version 59: previous attack turn keeps the old last-attack value, last attack becomes `current_turn`, and the other player's previous attack turn becomes `current_turn`. Destroy-embassy actions from that update are not pushed.

## Commands

`src/rules/commands.rs` runs the commands that change the board, including the action each one queues. UI popups, route rebuilds, and shorelines are not applied.

| Command | Rust |
| --- | --- |
| Boost | `boost` |
| Swarm | `swarm` (same boost write) |
| Stay | `stay` |
| Hide | `hide` |
| Recover | `recover`, `heal_unit` |
| Poison | `poison_unit` |
| Promote | `promote` |
| Break ice | `break_ice` |
| Break peace | `break_peace` |
| Destroy embassy | `destroy_embassy` |
| Establish embassy (includes the meet) | `establish_embassy` |
| Peace treaty | `peace_treaty` (message only) |
| Peace response | `peace_response` |
| Decompose | `decompose` |
| Destroy | `destroy` |
| Disband | `disband` |
| Disembark | `disembark` |
| Explode | `explode` |
| Flood | `flood` |
| Clear tile effect | `clear_tile_effect` |
| Freeze area | `freeze_area` |
| City reward | `city_reward` |
| Train on occupied space | `train_on_occupied` |
| Examine ruins | `examine_ruins` |
| End match | `end_match` |
| Game over | `game_over` |
| Resign | `resign` |
| Start match | `start_match` |
| Upgrade | `upgrade` |

Version compares with a test on each side:

| Compare | Arms | Test |
| --- | --- | --- |
| Boost `<= 114` | Boosted, else Swift | `boost_uses_boosted_then_swift` |
| Break peace `<= 79` | War plus attack record, else CoolDown | `break_peace_war_then_cooldown` |
| Break ice `> 105` | Default skips when an invisible unit blocks the action; the broken tile is Water when land lies beside it, Ocean otherwise | `break_ice_converts_ocean_and_water_on_both_arms` |
| Heal `<= 81`, `<= 102` | uncapped, clamped, Default also clears Charmed | `heal_cure_and_clamp_follow_the_version_arms` |
| Poison `<= 101` | Poisoned only, else Cute (skin 19) also Charms | `poison_charms_cute_only_after_101` |
| Embassy `<= 105`, Meet `<= 112` | no meet; meet plus `-1000` aggression; meet without that aggression | `embassy_meets_only_after_105_and_drops_aggression_after_112` |
| Disband `<= 105` | half of the full cost even when `get_paid` is false; Default obeys `get_paid` | `disband_refunds_half_and_respects_get_paid_after_105` |
| Disembark `<= 97`, `<= 108`, Default | passenger takes the carrier's health | `disembark_replaces_carrier_on_every_arm` |
| Explode `<= 82` and `> 120` | poison when damage is below health; peace skips | `explode_poisons_when_damage_is_below_health` |
| Start match `<= 99` | both set Started; the function result is true only at 99 | `start_and_end_match_set_status_on_both_sides_of_99` |
| Resign `<= 91`, `== 92`, later | auto-play, handicap 1 when game type is 1 or 4 | `resign_sets_autoplay_on_v91_and_later` |
| Upgrade `<= 98` and `> 108` | `currency - cost`, type replaced | `upgrade_spends_on_both_sides_of_108` |
| Flood skin | Swamp (17) also adds Swamped | `flood_adds_swamped_only_for_the_swamp_skin` |

The diplomacy check is not called by these command bodies. `check_diplomacy_keeps_message` is only its `version >= 82` alive-sender gate. Messages are not on `GameState`.

Area damage and rule-area updates are not called by these commands, so they are not inlined here.

## Inputs the caller supplies

| Input | Argument |
| --- | --- |
| Boost options | `boost_targets` |
| Heal amount | `heal_amount` |
| Max health | `max_health` / `promote` |
| Breakable ice | `should_break` |
| Blocked by an invisible unit | `blocked_by_invisible` |
| Embassy cost for the player | `embassy_cost` |
| Meet reward | `meet_reward` |
| Sending a message | not stored |
| Full cost | `full_cost` |
| Unit score | `unit_score` |
| Active peace treaty | `ExplodeTarget.peaceful` |
| Freezable | `freezable` |
| Reward production modifier | city-reward deltas |
| Pushing a unit to a free tile | `TrainSpawn.push_to` |
| Ruins reward (V119 or Default) | reward currency and `ruins_reward_is_v119` |
| First discovered lighthouse tile | `lighthouse` |
| Ice bank level check | not applied |
| Shoreline generation | not applied |
| Route update action | not applied |
| Tile effects | `flood` and `clear_tile_effect` return the ids; `Tile` has no effect list |

The increase-currency action adds its amount to currency and then stores the result with the rule above. Disband, embassy, meet, and the start-match star grant use that add. The older `increase_currency` helper still replaces currency.
