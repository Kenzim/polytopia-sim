# The Polaris tribe

What the engine does for Polaris, apart from freezing (see [freezing.md](freezing.md)). Every line below is pinned by a
test in `tests/polaris_tribe.rs`. The numbers come from the rules data, not from code.

## Tech tree

Polaris starts with Basic and Frostwork. Frostwork replaces Fishing, and four more techs replace tree nodes:

| Polaris tech | Replaces | Follows | Unlocks |
|---|---|---|---|
| Frostwork | Fishing | Basic | Outpost, Mooni |
| Sledding | Aquaculture | Frostwork | Battle Sled |
| Ice Fishing | Sailing | Frostwork | the Fishing improvement |
| Polar Warfare | Navigation | Sledding | Ice Fortress, Starfishing |
| Polarism | Aquatism | Ice Fishing | Ice Temple, the Glide ability |

A tech costs `cost + 4` stars for one city and `cost` more for each further city, as for every tribe.

## Units

- The Mooni comes with Frostwork, the Battle Sled with Sledding, the Ice Fortress with Polar Warfare, the Ice Archer
  (in place of the archer) with Archery. The Gaami (in place of the giant) is hidden: it is never trained, only given by
  the super-unit city reward.
- A unit with Skate (Mooni, Battle Sled, Ice Fortress) steps over ice at the normal cost. A step onto any other terrain
  is possible only as the last step of the move and uses the unit's attack as well as its move.
- Escape: an Ice Fortress or Battle Sled that attacks from ice keeps its move; from other terrain it does not.
- Glide (Polarism) makes a step onto ice cheaper for every unit of the player that does not skate, so a one-move unit
  crosses two ice tiles. It does nothing off the ice.
- A Mooni has no attack. It freezes by moving.

## Buildings

- Outpost (Frostwork): 5 stars, on ice only, adds a population to its city. Outposts route over ice within five tiles of
  each other, and an outpost beside a city joins that city to the capital like a port does.
- Ice Temple (Polarism): 20 stars, on ice only; 100 score and a population on completion, and a level every two turns.
- Ice Bank (replaces the market, comes with Trade): 20 stars, one per player, on field or ice. It earns 2 stars for each
  level, and its level is the number of frozen tiles on the map divided by twenty (see [freezing.md](freezing.md)).
- Fishing (Ice Fishing): 2 stars on a fish tile, adds a population. Starfishing (Polar Warfare): free on a starfish
  tile, needs one of the player's units on the tile and pays 8 stars.

## Not covered

- Versions before 114 price a step on ice differently (a Polarism owner crosses chains of ice at the road cost); no test
  pins that.
