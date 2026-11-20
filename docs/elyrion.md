# The Elyrion tribe

What the engine does for Elyrion: its tech tree, Enchant Animal, the Sanctuary, the dragons and the way a dragon's growth
can hold up the end of a turn. Each rule below is pinned by a test in `tests/elyrion_tribe.rs`, except the map weights and `rainbowvision` in "Start"
and the lines under "Not covered". The numbers come from the rules data, not from code.

The rules were checked against the retail game and against the 75 recorded human games with an Elyrion seat
(versions 122 and 123): all replay to the saved state except one, whose first difference is a Cymanti centipede segment
(not an Elyrion rule). Versions 121 to 124 take the same code path for everything on this page.

## Start

Elyrion starts with a warrior, 5 stars, two game resources near the capital and the techs Basic and Forest Magic. Its
terrain and resource weights make mountains rarer and crops more common (see [map generation](map-generation.md)). Its tribe ability
`rainbowvision` is not read by any rule in the game's logic, so the engine has nothing for it.

## Tech tree

Forest Magic replaces Hunting and Water Magic replaces Whaling:

| Elyrion tech | Replaces | Follows | Unlocks |
|---|---|---|---|
| Forest Magic | Hunting | Basic | Enchant Animal; Archery and Forestry follow it |
| Water Magic | Whaling | no tech | Aquatism, and the Enchant Whale improvement named below |

- No tech leads to Water Magic, so an Elyrion player can never research it. Its data names an Enchant Whale
  improvement, but the rules data has no such improvement, so even a position that holds the tech unlocks no
  improvement. Aquatism is still reachable without it.
- Forestry unlocks the Sanctuary and the Lumber Hut. The Sanctuary takes the place of Clear Forest, and Elyrion cannot
  burn a forest, so neither Clear Forest nor Burn Forest is on offer.
- Tech prices are the usual ones: `cost + 4` stars for one city and `cost` more for each further city.

## Units

- A city trains the warrior and, with Archery, the archer. Nothing else: the polytaur, the dragon egg, the baby dragon
  and the fire dragon are hidden, and the giant is replaced by the dragon egg.
- A super-unit city reward gives an Elyrion player a dragon egg where other tribes get a giant.
- Polytaur: 15 health, attack 3, defence 1, move 1, with dash (it can move and then attack in the same turn) and
  independent (see Enchant Animal).
- Dragon egg: 10 health, no attack and move 1. It is stiff: it never hits back when it is attacked.
- Baby dragon: 15 health, attack 3, move 2; it flies (water is no obstacle), dashes and escapes.
- Fire dragon: 20 health, attack 4, move 3, range 2; it flies, dashes, and splashes the enemies beside its target. A
  ranged attacker does not step onto the tile it kills on.

### Growing up

A dragon egg and a baby dragon grow when their age (the current turn minus the turn they were created) reaches 3, so an
egg is a fire dragon six turns after it appeared.

- The growth happens at the end of the owner's turn, once for each unit that is ready.
- It is an upgrade of cost 0 that takes a unit id, as a bought upgrade does. The unit keeps its own id, and the id counter
  moves on by one. Any other unit created afterwards has an id one higher for each growth that took place, so a
  recorded game only replays if the engine takes the ids too.
- The new unit keeps the damage the old one had taken, and explores around itself: a fire dragon sees further than a
  baby, which can uncover a map corner.
- A unit that is not old enough is left as it is and takes no id.

### When a growth holds the turn

Uncovering a map corner adds population, and that can raise a city to a new level and so raise a city reward. A reward
raised by the turn's own end belongs to the player whose turn is ending, and the game does not hand the turn on until it
is answered. The engine does the same:

- `current_player()` stays with the player who ended the turn. `reward_pending()` is true and `legal_commands()` offers
  only the `CityReward` choices; answering one passes the turn.
- The turn is held only when the end of the turn raised the reward. A reward that was already open before the end is
  left as it was.
- The hold is saved in a position (`handover_held`), so a copy made while the turn is held passes it the same way.
- The retail game runs the effects of the turn's end as a stack of actions, and its action loop runs the top action
  only while the current player has no pending command trigger. The action that passes the turn lies beneath the
  growth, so a reward the growth raises blocks it. In retail the end-of-turn effects beneath the growth that raised the
  reward also wait for the answer; the engine runs them all first and holds only the pass. The two differ only if
  the answer changes what those effects see, and no recorded game shows that.
- A caller that treats a change of `current_player()` after `EndTurn` as the sign that the turn has passed sees one more
  step in this case. The client, the AI and the referee read `current_player()` after every command instead of
  assuming who is next, so none of them needed a change.

## Buildings

### Enchant Animal

- Built on a tile that holds game, needs Forest Magic and costs 3 stars.
- The game is used up and no building is left on the tile. A polytaur stands there instead.
- The polytaur is made by a train action, so it takes a unit id, and it is independent: it has no home city, and
  capturing a city does not give it one.
- A unit of the builder's own that stands on the tile is pushed aside to make room.

### Sanctuary

- Needs Forestry, costs 5 stars, and is built on a field, mountain or forest tile that has a forest beside it. Only one
  Sanctuary stands under a city.
- It has the Attract ability. At the start of its owner's turn, when its age (turns since it was built) is a multiple
  of its growth rate (2), it fills one forest with game. The target is the first tile of the radius-1 area around it,
  diagonals included and its own tile left out, that the same player owns and that is a forest with no improvement and
  no resource. Only one tile is filled each time, and the area is searched in the same order every time.
- The game it makes is what Enchant Animal builds on, so a Sanctuary feeds a supply of polytaurs.

## Poison

A unit that moves onto the tile of an enemy it has just defeated takes that step through the same check as any other
move, so a polytaur that kills a unit standing on a poisonous improvement (Fungi or Clathrus) is poisoned by the step.

## Not covered

- Autoheal (ability 41) and autoflood (ability 45) are defined by the engine, but no Elyrion unit has either; autoflood
  belongs to the Aquarion crab.
- The tests build their positions by hand. The recorded games (the `ely_*` regression games) show that real play
  replays, but they do not isolate each rule.
