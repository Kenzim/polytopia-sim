# Combat, retaliation, healing and promotion

An overview of fighting. The damage formula and its gates are in [specs/combat.md](specs/combat.md); this page says how the engine uses them. Unit stats are in [reference/units.md](reference/units.md), special abilities in [abilities](abilities.md), and who can be attacked is also shaped by [diplomacy](diplomacy.md).

## Who can be attacked

`Engine::attack_targets` lists the targets of a unit: enemy units within the unit's range. Invisible units and units of a player the attacker is at peace with are skipped. The scan is `rules::path::attack_options` ([specs/path.md](specs/path.md)). A unit that has moved cannot attack unless it has `dash`; a unit that has attacked cannot attack again unless it has `persist` and has just made a kill. Units that attack with 0 attack are still allowed to target when they have `convert` or `infiltrate`.

## Damage

`get_battle_results` ([specs/combat.md](specs/combat.md)) turns the two units into an attack damage and a retaliation damage, both in tenths of health:

- A unit's force is its current health times its attack (or defence) divided by its maximum health, so a wounded unit hits and holds worse.
- Attack damage is the attacker's attack times its share of the total force, rounded to a multiple of 10. Retaliation uses the defender's defence in the same way.
- Defence is multiplied by a bonus. It is 1.5 on a tile with a defence tech for that terrain (for example Forest with Archery, Mountain with Climbing; see [reference/techs.md](reference/techs.md)). A `fortify` unit in its own city gets 1.5, or 4.0 if the city has the City Wall reward. A poisoned unit's bonus is halved (and fixed at 0.7 up to version 114).
- Maximum health is the unit's health plus 50 per promotion level, or the passenger's when a boat carries one and has no `protect`.
- A boosted unit adds 0.5 to its attack. `tentacles` attack with their defence value.

## What happens in an attack

`Engine::do_attack` (`src/engine/apply.rs`) runs these effects in this order:

1. The damage is dealt to the target (`perform_attack`): health drops, never below zero; a surviving target is poisoned if the attacker has `poison`, even when the damage rounds to 0; boost, swift and bubble on the victim are cleared. A kill gives the attacker 1 XP unless it is a boat or a hidden unit. The kill count of the player goes up for the owner of the unit on the origin tile, or for the acting player when the attack starts and ends on one tile (an explosion hits each victim like that) or has no origin tile; a kill of a Nature unit counts for nobody, and the victim's owner always gets the casualty.
2. An attacker with `freeze` (the Ice Archer) freezes a surviving target and stops here, with no splash, step or retaliation (see [freezing](freezing.md)). Otherwise splash: a ranged attacker with `splash` damages (half damage) or, with `poison`, poisons every unit next to the target that is not its own or a peace partner's. The victims are hit in reverse neighbour order, so the attacker, which turns to each victim it hits, ends facing the first neighbour that held one.
3. If the target was killed and the attacker has range 1, it steps onto the target's tile (it may embark or disembark there, and `stomp` and `tentacles` strike its new neighbours). A centipede that steps in drags its tail: a segment that is no longer next to the head's new tile moves onto the tile the head left, and the next segment is tested against that tile in turn, down the chain; a segment that is still next to the head stays put. A head pushed aside by a unit that appears on its tile (a train, a city reward) drags its tail the same way; the follower then stands on the tile the new unit takes, so the new unit pushes it aside in turn, by the direction to its leader. Otherwise, if the target survived and may hit back, the retaliation is dealt to the attacker (only when it is above 0, so a retaliation that rounds to nothing neither damages nor poisons). If that kills the attacker, the defender gets 1 XP.
4. `eat` units that killed grow a segment. The segment has no home city (a unit made on a tile that is not a city has none), so it does not count against a city's unit limit. `convert` units turn a surviving target to their side (see [capture and conversion](capture.md)); converting a centipede segment converts every follower and leader in its chain to the same side.
5. The attacker's flags are set: it has attacked and, unless it has `escape`, moved. `persist` and `doubleattack` can give a further attack. Units with `consumed` die after attacking.

Every attack on another player's unit or city calls `note_attack`: it records the attack turn (two players who attacked each other this turn or the last are at war, see `has_war_with`) and destroys the embassies between the two players ([diplomacy](diplomacy.md)).

## Retaliation

A defender hits back when all of these hold (`can_defender_retaliate`):

- the attacker is within the defender's own attack range (range 1 means adjacent);
- the defender survived;
- the defender does not have `stiff` and is not frozen;
- the attacker does not have `convert`, `surprise` or (above version 119) `freeze`.

Catapults and other `stiff` units never hit back; units with 0 attack cannot retaliate in versions up to 119.

## Healing

Healing happens at the end of a player's turn (`Engine::end_of_turn_heals`), not at the start:

- **Resting**: a damaged or poisoned unit that did not move or attack during the turn heals. The amount is 20, or 40 when it stands on a tile its owner owns or on a peace partner's tile (`heal_amount`). It never exceeds missing health. A poisoned unit is cured instead of healed. Frozen units, and units with a leader, do not rest-heal.
- **`Recover`** is the same heal as a command: the unit uses its turn to recover straight away.
- **Heal others** (`heal` ability, Mind Bender and Siren): the unit heals each damaged adjacent friendly unit by 40, curing poison, and uses its turn.
- **Mycelium** (a Cymanti improvement with the `heal` ability): units on and next to it heal 40 at the end of the owner's turn.

Healing does not depend on whether the unit is in a city. `fortify` units only gain defence there.

## Poison

A poisoned unit's defence bonus is halved from version 115 (fixed at 0.7 up to version 114) and, from version 115, it loses a movement point. Poison is removed by resting or by a healer. Units with `poison` poison what they hit; improvements with the `poison` ability (Fungi, Clathrus) poison any unit that steps onto them, unless the unit flies or its tribe has `poisonresist` (Cymanti). See [abilities](abilities.md).

## Promotion

A unit gains 1 XP for each kill. When its XP reaches its promotion limit (the unit's `promotion_limit` in [reference/units.md](reference/units.md)) and it has not been promoted, it may be promoted with the `Promote` command (`can_be_promoted`). Promotion raises maximum health by 50 and heals the unit fully. `static` and hidden units cannot be promoted.
