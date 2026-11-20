# Freezing

Polaris freezes the board. This page says what the engine models; the code is in `src/engine/freeze.rs`.

## What is covered

- **Autofreeze** (the unit ability of the Mooni and the Gaami): any move except a push freezes the 3x3 square round the tile the unit reaches. That is a move command, and also the step a melee unit takes into the tile of a unit it has just killed (so a Gaami that kills freezes round the tile it steps into). A Mooni cannot kill, so only the Gaami steps.
- The shared **freeze-area** step that the autofreeze uses: which tiles and units can be frozen, and what freezing does to them.
- The **Polaris spread** (tribe ability `spread`): see below.
- The **freeze on attack** (unit ability `freeze`, the Ice Archer): see below.
- The freeze-area and break-ice commands, and the ice bank levels: see below.

The flood that a tribe with the flooded ability adds when it takes a tile is covered in [aquarion.md](aquarion.md).

## What can be frozen

A tile can be frozen by a player when any of these holds:

- it is water (any version above 79), or it is flooded;
- its climate or skin differs from the freezing player's (above version 111; up to it, only an unowned or own tile of another climate);
- a unit stands on it that the player may freeze.

A unit may be frozen when it is not the player's own, not a unit of a player at peace with them (or in the cool-down after peace, above version 99), and not frozen already. An invisible unit counts only for its owner and for players at peace with it. All of this is decided for the whole square before anything freezes, so the order of the tiles does not matter.

## What freezing does

- A frozen **tile**: water turns to Ice, a flood is lifted, and the tile takes the freezing player's climate and skin. A player's climate is the climate of their tribe (16 for Polaris).
- A frozen **unit**: it counts as attacked by the freezer (so relations record the attack), it is marked moved and attacked, and the freezer's Pacifist task starts again. With the Magma skin it is petrified as well.

A frozen unit can do nothing: `legal_commands` offers no move or attack for it. It thaws when its owner ends a turn, so it stays frozen through the owner's next turn.

## The Polaris spread

The Polaris climate spreads with the cities and is taken back when the tiles change hands.

- **Capturing a city** (a village or a city): a player with the spread ability freezes every tile the captured city rules, as a freeze of each tile in turn (water turns to Ice, the rest takes the player's climate and skin, no units are frozen). A player without it takes the Polaris climate back from those tiles.
- **Border growth**: when any city takes the border-growth reward, the tiles it now rules are treated as after a capture (a Polaris player freezes them, any other player takes a spreading climate back from them). Then, when a Polaris city takes it, the new border is frozen as a freeze-area action round the city with the new border size as radius, for the player's own tiles only and without freezing units.
- **Start of a turn** (from version 112): every tile the player owns whose climate spreads is checked. A player of another tribe takes the tile's climate and skin to their own, but only on land and never on Ice. A Polaris player only brings the skin of its land into line.
- **Founding** is not a separate case: a city only comes from capturing a village, and the first capital is placed with its area by the map setup.

Water and Ice keep the Polaris climate when another tribe owns them, so they stay frozen.

## The freeze on attack

An attacker with the `freeze` ability (the Ice Archer) deals its damage as any attacker does. Then, instead of the splash, the step into the target's tile and the retaliation, it freezes the unit it hit, if that unit survived: the unit counts as attacked by the attacker's player, is marked moved and attacked, and is frozen (petrified as well with the Magma skin). The tile is not frozen. A kill pays the usual XP and freezes nothing. The attacker's own flags are set as after any attack. The defender never retaliates against such an attacker (above version 119; before that, against an arrow weapon). Up to version 81 the freeze does not start the Pacifist task again and does not petrify; the engine does not model that difference.

## The two unit commands

- **Freeze area** (command 17): a unit with the freeze-area ability (the Gaami) that has neither moved nor attacked, with at least one tile beside it that can be frozen. The unit is marked moved and attacked (it is not frozen itself), then the square of radius 1 round it freezes like an autofreeze, units included.
- **Break ice** (command 18): a unit that leads no tail, has not attacked, has not moved (unless it has Dash, above version 98), has no autofreeze, and has breakable ice beside it. The unit is marked moved and attacked and shows itself if it was hidden. Every breakable ice tile in the 3x3 square round it (its own tile included) then breaks: it becomes Ocean, or Water when a land tile that is not ice lies directly above, below, left or right of it, and above version 105 its skin is cleared. Ice with a unit on it is not breakable up to version 98. Above 98 it breaks under a unit that swims; above 105 it also breaks under a unit that cannot swim only when that unit is hidden from the player, in which case the unit is revealed instead of the ice breaking; above 115 a tile with Algae or a bridge, or a flying unit on it, counts as breakable too. Routes and city connections are updated afterwards.

## The ice banks

After a tile freezes, after ice breaks and after a build, every Ice Bank is set to a level of one for every twenty frozen tiles on the map (ice, or land of the Polaris climate), at most the bank's maximum level. A rise goes up one level at a time, with the level-up rewards; a fall sets the level at once.

The shore lines that the game draws round the broken tile are only a picture and are not kept in the state.

## What the AI does with freezing

- **Which tiles count as freezable** to the AI is the same test as for the freeze-area command (`Engine::is_freezable`): water, a flooded tile, a tile of another climate or skin, or a tile holding a unit the player could freeze. A neighbouring enemy that can freeze is worth -30 to a tile and one that stands on a freezable tile adds 5, when the unit being placed has Autofreeze or Freeze-area.
- **Freeze-area command**: scored 50 for every freezable tile in the 3x3 square round the unit, the unit's own tile included.
- **Break-ice command**: scored 50 for every breakable ice tile beside the unit (its own tile not counted). A Polaris player never breaks ice.
- Either command makes the unit count as able to do something, so a unit that cannot move or attack is still looked at.
- An Ice Bank in the rules of game versions 100 to 103 lists the Polaris climate among its adjacency improvements; there the AI adds one level for every twenty frozen tiles the player owns to the bank's adjacency bonus when it scores building one.
