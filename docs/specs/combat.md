# Combat

The battle calculation is one formula with two log-and-bail exits; rounding to tens is a separate helper. There is no second battle formula.

## Exported API (`src/rules/combat.rs`)

- `BattleResults`, `floored_half_attack_damage`
- `CombatUnit`, `BattleQuery`
- `get_battle_results`, `can_defender_retaliate`
- `scaled_attack`, `promoted_max_health`
- `unlocked_defence_bonus`, `defence_bonus`, `DefenceBonusInput`
- `has_active_peace_treaty`
- `WEAPON_ICE_ARROW = 10`, `OLD_RETALIATION_VERSION = 119`
- poison gates `114` and `115`; peace cooldown gate `99`

`attack` and `defence` on `CombatUnit` are the unit's effective attack and defence (see the stat helpers below). `defence_bonus` is the [`defence_bonus`] result. `attacker_in_attack_options` and `can_path_onto_defender` stay explicit inputs of this module; `src/engine` computes them (see "How the engine feeds this module").

## Damage formula (ported)

`get_battle_results(game_state, attacker, defender, account_for_peace, simple_data)`.

Early sentinel, returned as attack `0`, retaliation `-1`, both flags clear:

- `account_for_peace` and `has_active_peace_treaty(defender, state, attacker_player)`
- the maximum health of either unit is `<= 0` (the game logs an error)
- `attack_force + defense_force <= 0` (the game logs a warning)

Otherwise, with 32-bit wrapping multiplies for the force product and a 64-bit multiply for the damage product:

```
attack_stat = has Tentacles ? UnitData.defence * 10 : attack(attacker)
attack_force = (attacker.health * attack_stat * 100) / attacker_max_health
defense_force = (defender.health * defence(defender) * 100) / defender_max_health
total = attack_force + defense_force
attack_damage = Round(attack_stat * attack_force * 450 / (total * 1000))
```

`health` is an unsigned 16-bit field. The `/` is a mixed divide: unsigned 32-bit when both operands fit in 32 bits, otherwise signed 64-bit toward zero. `Round` is `round_to_10` (remainder 5 or more rounds away from zero's side of the lower multiple of 10; the rng module owns that function). The quotient is narrowed to 32 bits (sign-extended) before rounding.

Retaliation starts at `-1`. If `can_defender_retaliate`:

```
retaliation = Round(defense_force * defence(defender) * 4500 / (total * 1000 * defenceBonus(defender)))
```

`defence` is already `raw_defence * defence_bonus`, so the extra divide by the bonus cancels one factor and leaves the bonus inside `defense_force` only. A zero bonus is a divide fault in the game.

If `attack_damage >= defender.health`, retaliation is forced to `0` (a killed unit does not hit back).

Flags, skipped entirely when `simple_data` is set:

- `should_move_to_defeated_enemy_tile`: the kill check passed, `UnitData.range == 1`, and the path search (`get_path`) returned a list. The search allows occupied tiles and uses the attacker's `movement_points` as the maximum cost.
- `should_reset_pacifist_task`: `attack_damage > 0` and the defender does not have the Invisible effect.

`floored_half_attack_damage` is `Floor(attack_damage / 2)` with the half taken toward zero, then `floor_to_10`.

## Retaliation gates

`can_defender_retaliate` returns false when any of these hold:

- defender has Stiff (unit ability 38)
- attacker has Convert (8) or Surprise (31)
- defender has the Frozen effect
- game version above 119 and the attacker has Freeze (18)
- version `<= 119` and the defender's attack is 0
- version `<= 119` and `UnitData.weapon == IceArrow` (10)

It then takes the defender's `attack_options` (at its unit range, ignoring the diplomacy relation) and returns whether that list contains the attacker's coordinates. The list is empty, before any tile scan, when:

- the defender has a leader
- `UnitData.attack * 10 == 0` and the defender has neither Convert nor Infiltrate (35)

The scan does not include hidden tiles: a tile the defender's owner has never explored is skipped, so a defender does not retaliate against an attacker standing on such a tile. The tile scan is not part of this module. The caller passes its result as `attacker_in_attack_options`.

## Stat helpers that feed the formula

| Stat | Value |
| --- | --- |
| Attack, from the unit type | `attack * 10` |
| Attack, from a unit in play | `attack * 10`, or `attack * 10 + 50` when Boosted |
| Defence | `UnitData.defence * defence bonus`, or `0` if the unit data lookup fails |
| Maximum health | `promotion_level * 50 + unit_data.health` |

Tentacles reads the unit type's `defence` and multiplies by 10. It does not use the boosted attack value.

The defence bonus is `defence_bonus`. The poison shift is a logical right shift, which matches an arithmetic shift on these non-negative bonuses.

- version `<= 114` and Poisoned: return `7` immediately. This arm does not shift.
- otherwise start at `10`. A missing tile stays there.
- owner non-zero, the player lookup succeeded, and `unlocked_defence_bonus >= 2`: set `15` (`5 * (unlocked >= 2) + 10`)
- improvement present, the improvement data lookup succeeded, the improvement is a City (`has_improvement`), `tile.owner == unit.owner`, and Fortify (22): `40` if the city has the City Wall reward, else `15`
- version `>= 115` and Poisoned: shift the bonus right by 1

`unlocked_defence_bonus` walks the player's unlocked techs in order. Empty `defence_bonus_unlocks` maps are skipped. The first map that contains the terrain returns that int; otherwise the result is `1`. The parsed field is `TechData.defence_bonus_unlocks` (`BTreeMap<String, i32>`). Keys are the JSON terrain ids (`forest`, `mountain`, `water`, `ocean`). The unit bonus does not copy that int: `15` in v28 only passes the `>= 2` test and becomes bonus `15`. A stored `1` leaves the bonus at `10`.

`has_active_peace_treaty`: the same owner returns true. Otherwise `version <= 99` is `has_peace_with` and `version > 99` also accepts `has_cool_down_with`. Both return false when the unit owner is `0`. Otherwise they compare the relation state to Peace (`1`) or CoolDown (`3`). `get_relation` throws if the entry is missing. Relations are not on `PlayerState`, so those two compares are bool arguments. `get_battle_results` consumes the resulting bool.

The maximum health also redirects to the passenger when one is present and the unit lacks Protect (39). `promoted_max_health` does not do that; `Engine::max_health` in `src/engine/mod.rs` does, so the redirect is applied before `CombatUnit` is built.

## Executing a stored attack

The attack action hands straight to `perform_attack` with origin, target and the damage already stored on the action. It does not recompute `get_battle_results`. `Engine::perform_attack` (`src/engine/apply.rs`) clears Boosted on the attacker, and Boosted, Swift and Bubble on the victim. It turns both units to face each other, subtracts the damage (never below zero health) and poisons a surviving victim when the attacker has Poison and the damage was above 0. A kill counts for the attacker's kills and the victim's casualties and kills the unit.

## How the engine feeds this module

Nothing here is blocked any more. `src/engine` supplies every input that this module takes as an argument:

| Input | Supplied by |
| --- | --- |
| `attacker_in_attack_options` | `Engine::battle_query_base` (`src/engine/apply.rs`): the defender's attack options from `rules::path::attack_options` (leader cleared, diplomacy ignored) contain the attacker's tile |
| `can_path_onto_defender` | `Engine::can_path_onto` (`src/engine/legal.rs`): `rules::path::get_path` onto the defender's tile with occupied tiles allowed and the attacker's movement as the cost limit. `Engine::battle_damage_query` skips it for callers that read only damage |
| `has_active_peace_treaty` | the defender owns the attacker, or `Engine::at_peace` (`rules::diplomacy::has_peace_with`) |
| passenger redirect | `Engine::max_health` |
| `defence_bonus` | `Engine::defence_bonus`, from tile, owner techs and the City Wall reward |

One difference from the standalone helper: the engine's peace test uses `has_peace_with` only. The cool-down state is not added in `battle_query_base`, although `has_active_peace_treaty` in this module would add it for versions above 99. Capture is the place where the engine refuses on cool-down (`Engine::can_capture`, version 120 and later).

Area attacks are in `Engine::area_damage` (Stomp and Tentacles, run after the unit moves or steps onto a tile) and in `Engine::do_attack` (Splash on a ranged unit). See [abilities](../abilities.md).
