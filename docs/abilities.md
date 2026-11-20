# Abilities

What each ability does in the engine. Who has which ability, with the ids, is in the generated [reference/abilities.md](reference/abilities.md); unit and improvement stats are in [reference/units.md](reference/units.md) and [reference/improvements.md](reference/improvements.md). The pages on [movement](movement.md), [combat](combat.md), [cities and economy](economy.md), [capture and conversion](capture.md) and [diplomacy](diplomacy.md) say how the rules around them work.

Abilities that the data has but the engine does not act on are listed at the end of each table. Elyrion is covered in [elyrion.md](elyrion.md) and Aquarion in [aquarion.md](aquarion.md).

## Unit abilities

### Movement and attack timing

| Ability | Effect |
| --- | --- |
| `dash` | A unit that has moved can still attack in the same turn. |
| `escape` | After attacking (or converting) the unit keeps its move, so it can retreat. |
| `persist` | After a kill the unit can attack again if it has a target left. |
| `doubleattack` | At the end of its owner's turn the unit becomes ready for a free second attack (`double_ready`). |
| `scout` | Sight range 2 instead of 1. |
| `sneak` | Ignores enemy zone of control. |
| `hide` | The unit turns invisible when it moves, which also spends its attack; ignores zone of control and can pass through enemy-occupied tiles. Enemies that are not at peace cannot see or target it. On embarking it becomes the cloak boat. A unit that examines a ruin is revealed first, so its next move hides it again and spends its attack. |
| `fly` | Can enter any terrain at the flat cost, counts as aquatic for embarking, and is immune to poisonous improvements. |
| `swim` | Aquatic unit: stays on water and needs a port to enter it from land. |
| `water` / `land` | A unit with `water` and no `land` is restricted to water, ocean and wetland; a unit with `land` and no `water` cannot enter algae. |
| `amphibious` | Decides which tile-effect clearing commands are offered (algae, flood) and which units can get the Bubble ([aquarion.md](aquarion.md)). |
| `creep` | Every step costs the flat 10 and mountains are blocked; roads give no discount. |
| `skate` | Moves on ice at the flat cost; blocked off ice from version 115. Ending a move off ice sets `attacked`. Polaris unit ability, see [Polaris](polaris.md). |
| `carry` | A boat: carries one passenger and lets it disembark. Earns no kill XP. |
| `stiff` | The unit never retaliates. |
| `static` | The unit cannot be promoted, and from version 105 it gets no experience for a kill. |
| `independent` | The unit keeps its home city: capturing a city does not make that city its home, and a converted unit is not re-homed to the converter's capital. |
| `protect` | A boat with `protect` keeps its own health when carrying; without it the maximum health is the passenger's. |

### Damage and effects on others

| Ability | Effect |
| --- | --- |
| `splash` | A ranged attacker also hits (for half damage) every enemy unit next to the target. With `poison` it poisons them instead. |
| `poison` | A damaged survivor is poisoned. See [combat](combat.md). |
| `surprise` | The defender cannot retaliate. |
| `convert` | A surviving target changes owner; the converter is spent and cannot be retaliated against. See [capture and conversion](capture.md). |
| `infiltrate` | Attacking a foreign city kills the unit and triggers a reward (Cymanti Cloak). See [capture and conversion](capture.md). |
| `heal` | Adds the heal-others command: heals adjacent friendly units by 40 and spends the unit. |
| `fortify` | Extra defence in its own city. |
| `explode` | Adds the `Explode` command: damages and poisons adjacent non-peace units and removes the unit. A boat explodes its passenger instead. From version 124 the command also queues the end-of-command and run-deferred actions, like every other command; before that it queued neither. |
| `stomp` | After moving, after stepping into a killed unit's tile and on embarking, damages the neighbours. |
| `tentacles` | Attack damage uses the unit's defence value; full damage to neighbours when it moves or is trained; puts the Tentacle effect on the tiles around it ([aquarion.md](aquarion.md)). |
| `autoflood` | The tile the unit moves onto floods, if it is floodable. |
| `algae` | A move leaves Algae on every water tile of its path (the Cymanti island). |
| `boost` | Adds the `Boost` command: its neighbours become Swift (Boosted before version 115), which gives one more movement point, without spending the unit. No unit in the current rules has this ability. |
| `swarm` | Adds the `Swarm` command: spends the unit and boosts its neighbours the same way (Shaman). |
| `eat` | After a kill a new segment is added behind the tail (Cymanti Centipede). |
| `grow` | When its age reaches the growth rate, at the end of its owner's turn, the unit turns into its successor through an upgrade of cost 0 that takes a unit id (dragon eggs, baby dragons). See [elyrion.md](elyrion.md). |
| `consumed` | The unit dies after it attacks. |
| `freeze` | An attacker with `freeze` is never retaliated against (above version 119), and freezes the target it hits and leaves alive; see [freezing](freezing.md). |
| `drench` | Allows targeting empty floodable tiles in range to flood them ([aquarion.md](aquarion.md)). |

### Not acted on by the engine

`detect`, `navigate`, `crush`, `unique`. The data carries them but the engine has no gameplay for them. The engine also defines `autoheal` (id 41) without using it.

## Improvement abilities

| Ability | Effect |
| --- | --- |
| `unique` | Cannot be built again once built (monuments, Ice Bank). |
| `limited` | Cannot be built when the same improvement is already within distance 2 under the same city. |
| `consumed` | Used up by building: the improvement is not placed and the tile's resource is cleared (harvests, hunting, fishing). |
| `patina` | Grows with age: its level is age divided by its growth rate, and the level adds to the score (temples, Fungi). |
| `heal` | Heals units on and around it by 40 at the end of its owner's turn (Mycelium). |
| `poison` | A non-flying unit whose tribe lacks `poisonresist` is poisoned when it steps onto it (Fungi, Clathrus). |
| `bridge` | Makes the tile passable; protected from destroy and decompose while a unit stands on it; triggers a route refresh when built. |
| `network` | Gets a route to every city of its owner (Mycelium, Atoll). |
| `embark` | A land unit can embark on a water tile that has it (Port). |
| `freelance` | Can be built on a tile you do not own (roads, bridges, canals, starfishing). |
| `manual` | Needs a visible unit on the tile; building it marks that unit moved and attacked. |
| `slow` | Moving onto it costs the blocked cost unless a road applies. |
| `picky` | Adjacency counts each neighbouring improvement type once. |
| `simple` | Adjacency counts 1 per neighbour instead of its level (Clathrus). |
| `doubled` | Income doubles when a neighbouring Port qualifies. |
| `flood` | Floods the tile it is built on (the canal) and cannot be built on a tile that already has the flooded effect. |
| `discrete` | Can be built on a tile that already has an improvement without replacing it. |
| `fill` | Building it re-runs the route and city-connection passes. |

`attract` (Sanctuary): at the start of its owner's turn, when its age is a multiple of its growth rate, it fills the first owned empty forest of its radius-1 area with game ([elyrion.md](elyrion.md)). Not acted on at run time: `expand`.

## Player abilities (granted by techs)

| Ability | Effect |
| --- | --- |
| `disband` | The Disband command is offered for an unmoved, unattacked unit. |
| `destroy` | The Destroy command is offered for destroyable tiles you own. |
| `literacy` | Tech price multiplied by about two thirds (rounded up). |
| `glide` | Ice costs less to enter (Polarism); see [Polaris](polaris.md). |
| `decompose` | The Decompose command: the improvement is removed at the end of the turn and refunds its cost. |
| `peacetreaty` | Required for peace offers ([diplomacy](diplomacy.md)). |
| `embassy` | Required to build embassies ([diplomacy](diplomacy.md)). |
| `capitalvision` | Meeting a player reveals their capital; researching it reveals the capitals of everyone already met. |
| `pontoon` | Improvements with a field requirement can be built on non-land tiles (except road, forest growth, temple and canal). |
| `treehouse` | A field requirement can be met on a forest tile. |
| `microbes` | Raises the maximum level of Fungi by one. |
| `bubble` | An Amphibious unit that ends a move on a flooded tile gets the Bubble (+1 movement); it loses it on dry land ([aquarion.md](aquarion.md)). |

Not acted on: `dividend` (the data field is never read) and `discipline`.

## Tribe abilities

`poisonresist` (Cymanti): improvements with `poison` do not poison the tribe's units. Polaris has `alienclimate` (map generation, in the climate step: [map generation](map-generation.md)) and `spread` (see [freezing](freezing.md)); the engine does not read its tribe ability `freeze`, the freezing comes from the unit abilities. `flooded` (Aquarion) is covered in [Aquarion](aquarion.md). `rainbowvision` (Elyrion) is read by no rule in the game's logic, so the engine has nothing for it.
