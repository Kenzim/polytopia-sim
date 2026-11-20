# The Aquarion tribe

What the engine does for Aquarion: its tech tree and units, flooded land, the Bubble, the jelly's tentacles, the crab's
autoflood, the aquapult's drench, the Lost City, and the places where a unit's move or kill behaves differently from the
other tribes. Each rule below is pinned by a test in `tests/aquarion_*.rs`, except the map weights and the unit and tech
tables in "Start", "Tech tree" and "Units", which come from the rules data, and the lines under "Not covered".

The rules were checked against the retail game and against 28 recorded human games with an Aquarion seat (versions 121 to 123): all replay without a refused command and end with the
server's scores. The only differences left in a field-by-field comparison with the server's final state are in the unit record's marker of the position at the end of the
previous turn, which every tribe shows. Eight of the games are in the regression set (`aq_*`, in the `re` repository).

## Start

Aquarion starts with a mermaid warrior, two fish as starting resources and the techs Basic and Aquabase. Its tribe ability
`flooded` is read by the rules named below. In map generation forests are half as common as usual, aquacrop one and a half
times as common and crop half as common (see [map generation](map-generation.md)). Aquabase gives the `pontoon` player
ability (see [abilities](abilities.md)) and a movement cost of 2 on water.

## Tech tree

| Aquarion tech | Replaces | Unlocks |
|---|---|---|
| Spearing | Chivalry | the tridention and the `destroy` ability |
| Aquafishing | Fishing | the Fishing improvement; leads to Sailing and Aquaculture |
| Aquaculture | Ramming | the atoll and the aquafarm; leads to Aquatism |
| Waterways | Roads | the canal and the `bubble` player ability; leads to Trade |
| Marine Life | Sailing | the shark and a movement cost of 2 on ocean; leads to Navigation |

Burn Forest is replaced by Fertilize, which turns wetland into a crop field.

## Units

Aquarion's land units are mermaid versions of the usual ones. All of them, and the amphibian, the crab and the siren,
are Amphibious (they move on land and on water). The naval units are replaced by water-only ones:

| Aquarion unit | Replaces | Cost | Notable abilities |
|---|---|---|---|
| Mermaid warrior, archer, swordsman, defender | warrior, archer, swordsman, defender | 2, 3, 5, 3 | amphibious, swim; dash and fortify where the original has them |
| Mermaid dagger | dagger | 2 | surprise, independent, static |
| Mermaid cloak | cloak | 8 | hide, creep, infiltrate, scout, static, stiff |
| Amphibian | rider | 3 | dash, escape |
| Crab | giant | 10 | autoflood, static, escape |
| Siren | mindbender | 5 | heal, convert, stiff |
| Aquapult | catapult | 8 | drench, stiff; water only |
| Shark | rammer ship | 8 | dash, surprise; water only |
| Jelly | bomber ship | 8 | tentacles, static, stiff; water only |

The shark, the jelly and the aquapult move on water, ocean and wetland, not on dry land. Stats are in
[reference/units.md](reference/units.md).

## Flooded land

A tile floods only if it is floodable: land that is not a mountain and is not flooded yet. Water, ocean, mountains and
ice never flood, and a building or a city on the tile does not matter. A flooded tile counts as wetland, so a water unit can
use it, and a city on flooded land trains the shark and the jelly with no water beside it.

- **Train.** A unit with a movement terrain list can be trained when a tile next to the city matches the list; a wetland
  tile (flooded or algae) matches when the list holds wetland.
- **Crab.** A unit with `autoflood` floods the tile it moves onto, if floodable. It does so on a move, on the step into a
  tile it has just cleared and on being pushed aside.
- **Canal.** The canal (Waterways) floods the tile it is built on and cannot be built on a flooded tile.
- **Capture.** Taking a city floods its tile when the new owner has `flooded` (whoever the city belonged to) and drains it
  for any other tribe.
- **Clearing.** A unit can clear the flood from a tile it stands on. From version 116 the command is not offered to
  a `flooded` tribe's units.

## Bubble

A player with the `bubble` ability (Waterways) whose Amphibious unit ends a move on a flooded tile gives the unit the
Bubble: one extra movement point for as long as it has it. A unit that ends a move on a land tile that is not wetland
loses it again. A player without the ability and a unit that is not Amphibious never get it. A tile that floods under a
unit gives the same check.

## Tentacles

A unit with Tentacles puts the Tentacle effect on the eight tiles around it, unless it is frozen. The tiles are
recomputed after a move, a train, a disband, a freeze, a kill and the end of a turn that concerns such a unit.

- A unit that ends a move on a Tentacle tile takes a hit from the strongest unit with Tentacles next to it that belongs to
  another player that is not at peace with it. The hit is the full battle damage of that unit against it.
- The same happens for a jelly that moves next to an enemy: it hits the neighbours of its new tile with full damage.
- A jelly that has just been trained (version 105 and later) hits the enemies next to its city and then recomputes the
  tentacle tiles.
- Retail fixes the damage of each hit when the move queues it. A jelly that steps into the ring of another jelly is hit
  for the damage of the other jelly's health before the mover's own hit. A transport ship that lands its passenger on a
  Tentacle tile has the hit worked out against the ship and applied to the passenger.

## Drench

A unit with Drench (the aquapult) can target any floodable tile in range, with a unit on it or not. With no unit there the
attack turns the unit to the tile, floods it and spends the unit. A unit on the tile is attacked as usual and the tile stays
dry.

## Lost City

Above version 119, a ruin on an unowned deep-ocean tile that is not on the map's edge and has no city or ruin within two
tiles gives a
`flooded` tribe's examiner a Lost City, before the usual reward is drawn. The four tiles beside the ruin (not the
diagonals) turn to shallow water (ice stays), the ruin's tile becomes a flooded field with a new city that the player captures, and resources are
drawn for the tiles around it with the tribe's weights (every free tile next to it, a third of the others within two). The
examiner moves into the city as a capturer does: its home becomes the new city, so it no longer counts against the city
that trained it.

## Moves that run the whole move

- **The step into a killed unit's tile** is a full move: an amphibian that lands on flooded ground gets the Bubble, a crab
  floods the tile, an amphibian that lands on dry ground drops its Bubble, and a jelly's tentacles hit the new
  neighbours.
- **A unit pushed aside by a train** (for example the unit that stood on the tile a train needs) takes the rest of a
  move: poison, the Bubble step and the loss of the Bubble on dry land, the tentacle hit and the tentacle tiles, and the
  autoflood of a crab.

## Promotion and experience

- A unit can be promoted once its experience reaches 3 (2 for a tribe with Discipline) and it has not been promoted. The promotion limit in
  the unit data is never read, so the shark, the jelly and others that list 0 promote like any other.
- From version 105 a unit with Static (among the Aquarion units the jelly, the crab, the mermaid dagger and the mermaid
  cloak) gets no experience for a kill. Hidden and carrying units never do. Before version 105 a static unit still gets it.

## Not covered

- The aquapult's Drench rests on the retail attack code alone: no recorded human game has an aquapult.
- A pushed unit with Stomp does not damage its neighbours. Running the usual move rules for it made recorded games differ.
  (A pushed centipede head does drag its followers; see [combat](combat.md).)
- The tests build their positions by hand. The recorded games (the `aq_*` regression games) show that real play replays,
  but they do not isolate each rule.
- The unit and improvement tables (the atoll, the aquafarm, the water temple, the tridention) are read from the rules data
  like any tribe's; they are not pinned by Aquarion tests.
