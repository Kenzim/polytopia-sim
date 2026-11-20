# Capture, conversion and infiltration

Three ways a unit changes who owns something: it walks into a city and captures it, it converts an enemy unit, or it infiltrates a city and robs it. A player who loses their last city is eliminated, see [game modes and winning](game-modes.md#elimination).

## Capturing a city

`Command::Capture { unit, at }` is offered by `Engine::legal_commands` when `Engine::can_capture(unit, at)` holds (`src/engine/legal.rs`). The unit must:

- not belong to Nature, and stand on a city tile (a village counts: it is a city that nobody owns, owner `0`);
- have both its move and its attack still available, and not be a follower in a chain (`leader` is 0);
- be standing on the tile, so a unit first moves onto the city or kills the defender and steps in (see [combat](combat.md));
- be at war with the owner of the city. It cannot capture the city of a player it has peace with. From version 120 it also cannot capture during a cool-down after a broken peace (see [diplomacy](diplomacy.md#war-attacks-and-cool-down)).

Applying the command (`do_capture`, `src/engine/apply.rs`) does this, in order:

1. The capturer is exhausted: it cannot move or attack again this turn. From version 116 its double-move, boost and any followers' flags are reset as well (`make_exhausted`).
2. Units of other players that call this city home lose their home (it becomes unset).
3. The capturer's own home becomes the captured city, unless the capturer has `independent` (a Polytaur, a dagger): it keeps its home, usually none, and so takes none of the city's unit slots.
4. `Engine::capture_city_at` hands the city over (below).
5. Routes are recalculated over the new territory, then city connections are checked for the new and the old owner, so a city is connected in the turn it is captured when a route reaches it.
6. The capturer's city count goes up by one.

### What `capture_city_at` changes

- **Owner.** The city's runtime record changes owner. A village that never had one gets a fresh record: level 1, no population, production 1 and border size 1. The founder of a conquered city stays the founder.
- **Territory.** Every tile the city rules changes owner to the capturer and keeps the city as its ruling city. A village has no area yet, so it claims a radius of 1 and the capturer explores around it. A city that already has an area is explored again for its new owner, without triggering meetings.
- **Buildings.** Improvements on the ruled tiles change owner, and keep their levels and the population already paid into the city.
- **Adjacency buildings.** Buildings that depend on their neighbours (windmills, forges and similar) take the level their same-owner neighbours now give them; a ruling city gains or loses the growth population that level change implies.
- **Units.** Other units are untouched and stay where they stand.
- **Capital.** A capital is the player's start tile while they own it. Losing it makes the capital the first remaining city in map order (`Engine::capital_of`). The capturer's start tile is unchanged, so a captured capital is an ordinary city for them.
- **Diplomacy.** `note_attack` records an attack on the previous owner, which destroys the embassies between the two players (when the old owner is a player and not Nature or the capturer).
- **Score.** Nothing is added directly. Score is the sum of its parts, so the capturer gains the city, its territory and its buildings, and the loser loses them ([game modes and scoring](game-modes.md#scoring)).

A captured city keeps the level and population it had. A city that has an enemy unit standing in it earns nothing (`calculate_work`), so a city is worth no income to its new owner while a foreign unit sits in it.

## Converting a unit

A unit with the `convert` ability (Mind Bender, Shaman, Siren, Ciru) converts instead of killing. In `do_attack`, after the damage has been dealt (see [combat](combat.md)), `convert_unit` runs when the target survived and does not belong to Nature:

- the target changes owner to the converter's owner;
- both are marked as having moved and attacked, so the converted unit cannot act this turn, and the converter is spent. A converter with `escape` keeps its move;
- the target's home becomes the new owner's capital, unless the target has `independent` (then it keeps its old home);
- the new owner explores around the unit.

A Ciru also has `consumed`, so it is removed after the attack. A target killed by the damage is not converted. Score follows ownership, as above.

## Infiltration

A unit with `infiltrate` (Cloak, Cloak boat, Mermaid cloak and the Cymanti Moth) that attacks a foreign city does not fight: `do_infiltrate` (`src/engine/infiltrate.rs`) spends and removes the unit, records the attack on the city's owner, and runs `infiltration_reward` for the city:

- The city is marked robbed. From version 94 a robbed city earns no income, and from version 83 it cannot be infiltrated again, until its owner has finished a turn, when the mark is cleared.
- **Rebellion** (the reward for every infiltrator except the Moth): the infiltrator's owner is paid the city's raw production once the last unit is placed, which is `level + production`, minus 1 unless it is the owner's capital. Rebel units (Daggers, replaced by the tribe's own version such as the Cymanti Bug egg) appear on free tiles that the city rules, best tile first. If the owner can sail, tiles of water also count, and a Dagger placed on one is put on a boat. The number of spawns is the city level, up to 5. Each spawn is an action of its own, and so is the payment: a spawned unit's sight can reveal a map corner, which adds a population to its owner's capital and can level it up. The city reward that raises stops the action stack, so the spawns and the payment still to come wait until it is answered (`Deferred::Infiltrate`).
- **Poison** (the reward for the Moth): no payment, and the spawned units are Bug eggs.
- A probe unit of the infiltrator's type first fights any defender standing in the city, using the normal damage formula. If the defender survives, the city tile itself is not used for a spawn; if it dies, the city tile becomes a candidate.
- A spawned unit has no home city, so it takes none of a city's unit slots.
- A spawn tile's order comes from a score: +2 for the city tile, +1 for a tile with a defence bonus, +0.5 for a resource, -0.1 for water, -0.5 for a poisoning improvement unless the player resists poison, plus a small random value from the game hash.

## Elimination

`Engine::check_eliminations` runs after each command. A player who is no longer alive (`rules::diplomacy::is_alive`: no city, or with the unit death condition no city or no unit) has all their units removed, each paying back half its full cost when that is at least 2, and loses all embassies they hold or others hold toward them. The wipe is recorded once: the wiped player's `wiped_at_command` is the index of the command that did it (the command counter before that command), `killed_turn` the turn and `killer_id` the player who acted; that player's `wipe_outs` goes up by one. The game-end check runs next ([game modes and winning](game-modes.md)).
