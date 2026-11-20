# Documentation index

Start here to find how a mechanic of the engine works. There are three kinds of document:

- **Mechanic guides** (this directory): one page per topic, written to be read. They say what the engine does and point to the code.
- **Reference tables** (`reference/`): generated from the rules data, so the numbers cannot drift from the engine. Do not edit them by hand.
- **Specs** (`specs/`): exact behaviour of individual modules, with the version gates and constants. Read them when you need to reproduce or change a rule.

Scope: the 12 normal tribes, Cymanti, Polaris ([freezing.md](freezing.md), [polaris.md](polaris.md)), Elyrion ([elyrion.md](elyrion.md)) and Aquarion ([aquarion.md](aquarion.md)).

To use the engine from Rust, the command line or Python, start with [usage.md](usage.md). How fast it is, measured
against polybots-engine, is in [speed.md](speed.md).

What is ported and how it was checked against recorded games is in [ENGINE.md](../ENGINE.md). Which game versions are supported (green, amber, red) is in the README: [Supported game versions](../README.md#supported-game-versions).

## Mechanics

| Topic | Guide | Specs and reference |
| --- | --- | --- |
| Movement, pathing, vision and fog | [movement.md](movement.md) | [specs/path.md](specs/path.md), [terrain and resources](reference/terrain-and-resources.md) |
| Combat, retaliation, healing, poison, promotion | [combat.md](combat.md) | [specs/combat.md](specs/combat.md), [units](reference/units.md) |
| Unit, improvement, player and tribe abilities (dash, escape, persist, ...) | [abilities.md](abilities.md) | [reference/abilities.md](reference/abilities.md) |
| Stars, income, city growth and population, building, research, roads, tasks | [economy.md](economy.md) | [specs/economy.md](specs/economy.md), [improvements](reference/improvements.md), [techs](reference/techs.md), [tasks](reference/tasks-and-diplomacy.md) |
| Polaris freezing: autofreeze, what can be frozen, frozen units | [freezing.md](freezing.md) | [abilities](reference/abilities.md) |
| Polaris tribe: tech tree, units, buildings, movement on ice | [polaris.md](polaris.md) | [techs](reference/techs.md) |
| Elyrion tribe: tech tree, Enchant Animal, Sanctuary, dragons, the held end of turn | [elyrion.md](elyrion.md) | [techs](reference/techs.md), [units](reference/units.md) |
| Aquarion tribe: flooded land, the Bubble, tentacles, drench, the Lost City, moves that run whole | [aquarion.md](aquarion.md) | [techs](reference/techs.md), [units](reference/units.md) |
| Capture, conversion, infiltration, elimination | [capture.md](capture.md) | [specs/actions.md](specs/actions.md) |
| Diplomacy, peace, embassies, opinion | [diplomacy.md](diplomacy.md) | [tasks and diplomacy tables](reference/tasks-and-diplomacy.md) |
| Game modes, game end, winners and scoring | [game-modes.md](game-modes.md), [might-mode.md](might-mode.md) | [specs/economy.md](specs/economy.md) |
| Map generation and seeds | [map-generation.md](map-generation.md) | [specs/map-shape.md](specs/map-shape.md), [specs/map-terrain.md](specs/map-terrain.md), [specs/map-contents.md](specs/map-contents.md), [specs/rng.md](specs/rng.md) |
| The AI | [ai.md](ai.md) | [specs/ai-decide.md](specs/ai-decide.md), [specs/ai-score.md](specs/ai-score.md) |
| Game versions and rules versions | [versions.md](versions.md) | [specs/version.md](specs/version.md), [specs/rules-data.md](specs/rules-data.md), [change log](reference/changelog.md) |
| Game state and commands | [specs/board-state.md](specs/board-state.md) | [specs/actions.md](specs/actions.md) |

## Reference tables (generated)

[reference/README.md](reference/README.md) lists them: [units](reference/units.md), [improvements](reference/improvements.md), [techs](reference/techs.md), [tribes](reference/tribes.md), [terrain and resources](reference/terrain-and-resources.md), [abilities](reference/abilities.md), [tasks and diplomacy](reference/tasks-and-diplomacy.md) and the per-version [change log](reference/changelog.md).

They are written by `examples/gen_docs.rs` from the built-in rules data. After changing `data/rules/` or the generator, run

```text
cargo run --release --example gen_docs
```

and commit the result. The test `tests/docs_fresh.rs` regenerates the text in memory and fails when a committed file differs, or when `reference/` holds a file the generator no longer produces.

## Spec files

| File | Area |
| --- | --- |
| [specs/actions.md](specs/actions.md) | The action rules (`src/rules/actions.rs`) and what `src/engine` does itself |
| [specs/ai-decide.md](specs/ai-decide.md) | AI difficulty, noise and move search |
| [specs/ai-score.md](specs/ai-score.md) | AI scoring weights |
| [specs/board-state.md](specs/board-state.md) | The game state |
| [specs/combat.md](specs/combat.md) | Damage, retaliation and battle results |
| [specs/economy.md](specs/economy.md) | Income, city, research and score formulas |
| [specs/map-contents.md](specs/map-contents.md) | Cities, ruins, resources, fairness score |
| [specs/map-shape.md](specs/map-shape.md) | Generator entry point, presets, capitals |
| [specs/map-terrain.md](specs/map-terrain.md) | Noise, land, continents, climates |
| [specs/path.md](specs/path.md) | Movement and attack ranges, path search |
| [specs/rng.md](specs/rng.md) | The random number generators and hashes |
| [specs/rules-data.md](specs/rules-data.md) | The rules data files and loader |
| [specs/version.md](specs/version.md) | Version constants, milestones, the rules ladder |
