# Movement, pathing and vision

How a unit gets from one tile to another, what stops it, and what each player can see. The formulas are in `src/rules/path.rs`; the engine wraps them in `src/engine/legal.rs` and `src/engine/apply.rs`. Exact version splits and callee lists are in [specs/path.md](specs/path.md). Unit movement stats are in [reference/units.md](reference/units.md), the terrain a tech opens is in [reference/techs.md](reference/techs.md) and [reference/terrain-and-resources.md](reference/terrain-and-resources.md).

## Reachable tiles and paths

- `Engine::move_destinations` lists the tiles a unit may move to and `Engine::move_path` gives the route. Both call `path::get_move_options` and `path::get_path`. A search looks at no more than 1000 nodes.
- Neighbours are the eight tiles around a tile, diagonals included.
- Movement points are the unit's movement stat times 10, so a normal step (cost 10) uses one point. `movement_points` adds 1 for Boosted and 1 for Swift or Bubble. From version 115 a poisoned unit loses 1 point, but never drops below 1.
- A move is one command: the unit walks the whole path. A unit that has a leader (a follower) gets no move options of its own. A centipede that moves drags its tail the way an attack that steps in does: a segment no longer next to the new tile moves onto the tile the head left (not along the path), and the next segment is tested against the tile that segment left, down the chain; see [combat](combat.md).
- A unit can move once per turn (`moved`), and not while frozen.

## Step costs

`get_movement_cost` gives the cost of entering a tile (full rules in [specs/path.md](specs/path.md)):

| Case | Cost |
| --- | --- |
| Ordinary step | 10 |
| Step along a road, city, bridge or connected route | 5 |
| Mountain or forest (a unit without a road link) | 1000, which means the unit can enter but cannot go further this move |
| Slow improvement (ability 19) | 1000 |
| Flying unit | 10 on any terrain |
| Skating unit | 10 on ice. Off ice it is blocked above version 114 |
| Creeping unit | 10 everywhere except mountains |
| Water-bound unit on a flooded tile, land-bound unit on algae | 1000 |
| Embarking (land to port, or a boat landing) above version 114 | 1000, so embarking ends the move |

Roads cost 5 only when both tiles are linked by a road (`Engine::has_road_between`); cities and bridges count as road. Ice, and skating, creeping and swimming units, get no road discount.

## Which terrain a unit may enter

- Land terrain is open to land units. Mountains, water and deep ocean need the tech that unlocks them (Climbing, Fishing, Sailing in the current rules; see [reference/techs.md](reference/techs.md)). `Engine::unlocked_movement_terrain` and `Engine::allowed_terrain` combine the owner's techs with the unit's own terrain list. A unit with its own list ignores techs. Flying units can enter every terrain.
- A swimming unit stays on water. A water-only unit (ability `water`) stays on water, ocean and wetland; a land-only unit (`land`) cannot enter algae tiles.
- A land unit enters a boat by moving onto a friendly port (or a boat on shore) and becomes the boat's passenger. Only the unit and the tile it steps onto decide this, not the tile it leaves, so a land unit leaving an algae tile (water a land unit may enter) embarks onto a port like one leaving dry land; on the boat's next move the passenger can disembark onto land (a city or dry land; wetland, which is land under the Flooded effect, is no landing). Above version 98 the port must be friendly and the player must be able to embark, which means a technology it knows unlocks, for its tribe, an improvement with the embark ability (a port); movement onto water alone is not enough, so a land unit that reaches an algae port without such a technology stays a land unit. A leader or follower cannot embark at version 120 and later. The boat is a new unit with a new id: it starts with the passenger's health but no promotion and no XP, and its created turn is the embark turn. The passenger keeps its own promotion, XP and created turn. See [abilities](abilities.md) for `carry` and `hide` (the cloak becomes a cloak boat).

## Blocking

- **Zone of control** (`zoc_blocks`): a step next to an enemy unit costs 1000, so the unit stops there. Units with `sneak` or `hide` and invisible units ignore it. Invisible enemy units, your own units and units of a player you are at peace with exert none. Units that have a leader exert none above version 114.
- Tiles holding another unit cannot be entered; an enemy unit is attacked instead. Hidden units can pass through enemy-occupied tiles.
- Peace changes who counts as an enemy: units of a peace partner exert no zone of control and cannot be attacked (see [diplomacy](diplomacy.md)).

## After the move

- The unit's `moved` flag is set. A unit that has moved cannot attack unless it has `dash`. `escape` units can attack and keep their move, `persist` units can attack again after a kill. See [abilities](abilities.md).
- Each step explores tiles around the unit (below). A skating unit that ends its move off ice also gets `attacked` set.
- Area abilities (`stomp`, `tentacles`) fire as the unit moves or steps into a freed tile. See [combat](combat.md).
- A unit with the `algae` ability (the Cymanti island) leaves an Algae trail: every water tile of the move path, the tile it leaves and the tile it reaches included, that has no Algae yet gets the effect, as a free build of the algae spawn improvement for the unit's owner. Land tiles get nothing. Only a move command leaves a trail; a push does not.

## Vision and fog

- Exploration is permanent: once a player has explored a tile it stays explored (`GameState.fog` holds the explorers per tile). There is no hiding of terrain again.
- `Engine::sight_range` is 1, or 2 for a unit with `scout`. `Engine::explore_from` explores a square around the unit; a unit standing on a mountain sees at least 2.
- Tiles are explored after each step of a move, when a unit is trained, converted or lands, when an embassy is built (the opponent's capital area) and when a city grows its border.
- A move whose destination holds a unit, which can only be an invisible one the mover cannot see (the path search allows it), does not move the mover: it stays on its tile with its move unspent, the unit on the destination loses its Invisible effect, the mover turns to face it, and from version 106 the two owners meet if they have not yet. Nothing else of the move happens (no path exploration, no hide). Invisible units (`hide`, the cloak units) cannot be seen or targeted by enemies that are not at peace with the owner. The `detect` ability exists in the data but the engine does not use it to reveal units.
- The map editor's position files can leave `explored` out, which means everyone has explored everything (see [../ENGINE.md](../ENGINE.md)).
