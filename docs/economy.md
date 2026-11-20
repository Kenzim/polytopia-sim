# Economy: stars, cities, population and growth

How a player earns and spends stars, how cities grow, and what a building does for a city. The data (costs, work, rewards, who unlocks what) is generated into [reference/improvements.md](reference/improvements.md), [reference/techs.md](reference/techs.md) and [reference/units.md](reference/units.md). The standalone, version-gated formulas are specified in [specs/economy.md](specs/economy.md); this page describes what `Engine` (`src/engine/`) actually does with them.

## Stars

Stars are `PlayerState.currency`. A game starts each player with the tribe's `starting_stars` (5 when the tribe has none, see [reference/tribes.md](reference/tribes.md)). Stars come from:

- **Income** at the start of the player's turn (below).
- **Build rewards**: an improvement with a `currency` reward pays it when built (clearing a forest pays 1, starfishing 8).
- **City rewards**: the Resources reward pays 5.
- **Decomposing** an improvement at the end of the turn refunds its cost; **disbanding** a unit returns half its full cost (see [abilities](abilities.md)).
- **Embassies** (see [diplomacy](diplomacy.md#embassies)).

`Engine::add_currency` is the only place stars change. It also checks the Wealth task (100 stars).

Stars are spent on:

- **Training** a unit in a city (`Command::Train`): the unit's cost. `legal_commands` offers a unit only when the city tile is empty, the stars cover it, the player has unlocked it, and `can_city_support_unit` holds: the number of units whose home is that city must not exceed the city's level. A water unit also needs a neighbouring tile of its terrain. A trained unit has used its move and attack for the turn.
- **Building** (`Command::Build`): the improvement's cost. See [Building](#building).
- **Researching** (`Command::Research`): the tech price.
- **Upgrading** a unit: the cost of the new unit type.
- **Embassies**: 4 stars plus 2 for each embassy already held.

## Income

`Engine::start_turn` (`src/engine/turn.rs`) pays income at the start of each player's turn, except turn 0 (no income and no level-ups on the first turn). It walks every tile once:

- A tile of the player with an improvement pays `Engine::tile_work`. Level-based buildings pay `work * level`, doubled when the building has the `doubled` ability and a Port is adjacent (any owner up to version 101, the same owner after). A city pays `production + level - 1`; the capital also adds the player's handicap and the embassy income, and a negative city `xp` (starvation) takes that much off. Every human has handicap 1 (`HUMAN_HANDICAP`), so a new human capital pays 2 stars; a bot's handicap comes from its difficulty (Easy 0, Normal 1, Hard 2, Crazy 4). A city whose tile holds an enemy unit pays nothing, and neither does a city that was robbed this turn (from version 94).
- A tile of another player pays only when it is that player's capital: the embassy income towards the walker, unless an enemy unit stands in the city.

`Engine::income(player)` gives the sum for the current board. The walk first queues the improvement level-ups and city level-ups, then the income, and `run_deferred` runs the queue last-in first-out. Incomes are computed from the levels as they were before any queued level-up ran. Turn-start details and the places where the engine differs from the standalone rules are in [specs/economy.md](specs/economy.md#how-the-engine-runs-a-turn-start).

## Cities, population and levels

A city has a **level**, a **population** count and **xp** (the engine's `ImprovementRuntime`). A new city is level 1 with `production` 1.

- **Population** is added to the city that rules the tile (`Engine::add_population`). Each point adds 1 xp.
- A city levels up when `xp > level` (`should_level_up`): a level 1 city needs 2 population, a level 2 city 3, and so on. Levelling subtracts the new level from `xp`, so any surplus carries over. The check runs whenever population is added and again at the owner's turn start. Levelling up scores `50 - level * 5` (the `CITY_LEVEL_SCORE` and `CITY_XP_SCORE` constants in `src/rules/economy.rs`) and, from level 2, enables the Metropolis task.
- **Levelling up asks for a reward** (from level 2). The player answers with `Command::CityReward`, and nothing else can be done until it is answered. See [City rewards](#city-rewards).
- **Negative xp** is starvation: it lowers the city's income by that amount and, from version 115, costs 100 score per point (`improvement_score`). Population is taken away when a building that gave it is destroyed, decomposes or loses a level, and when a connection to the capital is lost (see [Roads and connections](#roads-and-connections)).
- **Borders**: a city claims the unowned tiles within its `border_size` (1 at founding) in `rule_area`. A tile belongs to the ruling city that claimed it, and buildings on it report their population to that city.

### City rewards

`Engine::reward_options(level)` lists the choices, in this order:

| City level reached | Choices |
| --- | --- |
| 2 | Workshop (+1 `production`, so +1 star a turn), Explorer (reveals tiles as a scout would) |
| 3 | City wall (defence bonus, see [combat](combat.md)), Resources (+5 stars) |
| 4 | Population growth (+3 population), Border growth (border +1, claims the new ring) |
| 5 and above | Park (+250 score, and +1 `production` from version 99), Super unit (a Giant in the city) |

A Giant trained on an occupied city tile first pushes the occupant aside. The push explores from the occupant's new tile, and a city reward that raises (a corner reveal levelling the capital up) stops the action stack, so the Giant appears only once that reward is answered (`Deferred::Train`), after the unit the answering reward itself trains.

A ring that border growth (or a village's first border) claims takes its neutral improvements along: a ruin, a village or a corner lighthouse on a claimed tile has its record's owner set to the tile's owner, as retail records them.

**A reward pauses the rest of its command.** A command runs as a queue of actions, and the queue stops at the first city level-up until the reward is answered. Population changes that come later in the same command are held as `Deferred::Population` (`Engine::hold`, fired from `add_population` while `Engine::reward_pending`), and run after the matching `CityReward`, in the order they were raised; one of them may level up another city and raise a second reward, which is answered in turn. The usual case is a port that connects a city to the capital for the first time: the port's own population goes first (a building's own population runs before its route and connection refresh), then the capital gains 1 and levels up, and the connected city's +1 waits for the capital's reward. With a single level-up nothing is held. `legal_commands` offers only the rewards while a reward is pending, so the AI and clients need no extra handling; the held changes are in `Engine::deferred` (and in position files as `"kind": "population"`).

The same hold applies to the longer actions a command can start, each kept in `Engine::deferred` with what it still has to do:

- `Deferred::Scout` (`ScoutRun`): the Explorer's walk, 12 steps (15 before game version 120) that each reveal the tiles around it. A step that reveals a map corner can level the capital up; the walk stops there, partway through that step's reveal if need be, and carries on from the same place once the reward is answered.
- `Deferred::Infiltrate` (`InfiltrateRun`): the rebel units still to place and the stars still to pay after an infiltration, when a placed unit's sight raised a reward (see [capture](capture.md#infiltration)).
- `Deferred::Train`: a Giant from the Super unit reward whose push of the occupant raised a reward (above).

Position files store them with the kinds `scout`, `infiltrate` and `train` (see [ENGINE.md](../ENGINE.md#position-files)).

### Building levels

An improvement with `max_level` above 0 has a level that changes while it is in play. Level changes are queued at turn start, one step at a time, and each level-up pays the improvement's `growth_rewards` population to its ruling city.

- **Adjacency buildings** (windmill, sawmill, forge, market, clathrus and the like) take their level from the matching neighbours: `Engine::adjacency_bonus` counts same-owner neighbours (each type once for `picky`, one per neighbour for `simple`, otherwise each neighbour's level, at least 1), capped at the maximum level. Building a neighbour can raise the level at once (`grow_neighbors`), and destroying one lowers it (`check_surrounding_area`).
- **Patina buildings** (temples, fungi) gain a level every `growth_rate` turns of age, up to the maximum.
- Temples, monuments and level-based buildings also add score; see [game modes and scoring](game-modes.md#scoring).

`Engine::improvement_level` and the standalone `calculate_improvement_level` are described in [specs/economy.md](specs/economy.md#improvement-level), including the cases the engine does not model (Lighthouse and Ice Bank levels).

## Building

`Engine::can_build` decides whether a `Command::Build` is offered. The improvement must be unlocked by one of the player's techs (or a completed task) and not hidden, and the stars must cover its cost. Then:

- The tile must be the player's own. An improvement with `freelance` (roads, bridges, starfishing) may also go on an unowned tile, and on another player's tile only if it is `manual` as well.
- A tile may hold one improvement, except for `discrete` ones (algae, canal, algae spawn) and roads. A road cannot go on a tile that already has a road, a city or a bridge.
- `limited` buildings (windmill, sawmill, forge, market and similar) may not repeat within 2 tiles of the same city; `unique` buildings (monuments, ice bank) only once per player.
- The terrain and resource requirements of the improvement must be met (field for a windmill, forest for a lumber hut, a crop resource for a farm, and so on), with the Wetland, Mangrove, Pontoon and Treehouse substitutions the engine applies in `meets_requirement`.
- A required neighbour (an improvement, terrain or resource next to the tile) must exist.
- A `manual` improvement needs a friendly or peaceful unit on the tile.
- A bridge needs a water tile with a tile that is not water (land, or any other non-water terrain) on both sides of it, left and right or above and below. Another bridge does not count: it sits on water, so a bridge cannot be extended sideways from a bridge. Retail checks only the terrain of the two tiles, not who owns them (from version 104; older versions have a further condition on the anchors that the engine does not model).

Building then pays the cost, places the improvement (a `consumed` one such as hunting leaves no building and uses up the resource), applies what it `creates` (terrain, resource, unit or effect), pays its `currency` reward and adds its `population` to the ruling city. Roads, bridges, route-opening buildings and anything that reshapes the tile refresh routes and city connections afterwards.

**Destroy** removes an improvement at once and takes back the population it gave. **Decompose** marks it and removes it at the end of the owner's turn for a refund of its cost. Both need a tech that grants the matching player ability (see [abilities](abilities.md)).

## Research

`Engine::researchable` lists the techs the player can learn (the children of owned techs, with the tribe's overrides; see [reference/techs.md](reference/techs.md)). The price is `Engine::tech_price`:

```
price = ceil((cities - 1) * cost + cost + 4)        (times 2/3 first, with Literacy)
```

where `cost` is the tech's data cost (shown in the reference) and `cities` is the number of cities the player owns. Learning a tech:

- adds it to the player's list and counts its score (`cost * 100`);
- enables the tasks it lists (`tasks_after_research`);
- for a tech with the capital-vision ability, reveals the capitals of all players already met (also when the tech is a ruins free tech: `grant_tech` and a research share `reveal_capitals_for_tech`);
- makes the units, improvements, terrain access and player abilities it unlocks available (`unlocked_units`, `unlocked_improvements`).

## Roads and connections

A city connected to the capital grows both. `Engine::update_routes` (`src/engine/routes.rs`) marks tiles as part of a route: every pair of the player's route openers (cities, ports, bridges and the other improvements with routes) within each other's range gets a path over the terrain they route across, and each `network` improvement is also linked to the player's cities. Roads and bridges make a tile cheaper for the path search. A route does not cross land owned by a player the owner is not at peace with.

`Engine::update_city_connections` then walks outwards from the capital over route tiles (`find_connected_cities`). Each city that becomes connected gives the capital and itself 1 population; each city that loses its connection takes that population off again. Retail queues one connection per city, in city order, on a stack that runs last in, first out, so with several changes at once the last city's runs first (this decides which level-up raises its reward first). The Network task (5 connected cities) is checked here. Routes are recomputed after every command that flagged them (a road, a route-opening building, an exploration that reveals terrain, a bridge or a peace treaty). Results are memoised per set of route inputs, which does not change them.

## Tasks and monuments

A task is an objective that a tech enables. Completing one lets the player build a monument (3 population, 400 score, once per player). `src/engine/tasks.rs` enables and checks them:

| Task | Completes when |
| --- | --- |
| Pacifist | 5 turns pass without the player attacking (checked at the next turn start) |
| Genius | no tech is left to research |
| Network | 5 non-capital cities are connected to the capital |
| Wealth | the player holds 100 stars |
| Killer | the player has made 10 kills |
| Metropolis | a city reaches level 5 |
| Convert | 3 units that are not Nature's have been converted |
| Explore lighthouses | all four map corners are explored (revealing the lighthouse also gives the capital 1 population; a corner revealed by capturing a city counts too) |

The Explorer task exists in the data and is enabled by techs, but the engine never completes it. Convert completes after 3 conversions of units that are not Nature's (each unit of a converted centipede chain counts). Which task unlocks which monument is in [reference/tasks-and-diplomacy.md](reference/tasks-and-diplomacy.md).
