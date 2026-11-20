# Game versions and rules versions

Two numbers are in play, and they are not the same:

- The **game version** is the number saved in a game (51 to 124 here; 124 is the current one, `GAME_VERSION`). Many rules switch on it: a fix lands at version N and applies to games of version N and later. These gates are written into the rules code and are mentioned in the mechanic documents where they matter (for example "from version 99" or "above 119").
- The **rules version** is the version of the data (units, techs, improvements, tribes) in `data/rules/`. Rules versions 9 to 28 exist. Version 28 is stored in full and every older one is a change file against the next newer one (see [rules data](specs/rules-data.md)).

`logic_data_version(game_version)` maps one to the other through `RULES_LADDER`: for example every game version from 120 up uses rules version 28, and 119 uses 27. Game versions below 51 are refused, and so are versions whose rules data would need rules versions 1 to 8, which have no file. The whole table is in [specs/version.md](specs/version.md).

## What the engine accepts

The README has the traffic light for each version, green, amber or red, with the tests behind each: [Supported game versions](../README.md#supported-game-versions). In code it is `support_level(version)`, returning `Support::Tested` (green, 121 to 123), `Support::Supported` (amber, 51 to 120 and 124) or `Support::Unsupported` (red).

New matches are created at 108 or later (`GameVersion::for_new_match`); older versions can only be loaded from a save and played on.

## Finding what changed

- Rules data between versions: the generated [change log](reference/changelog.md) lists, for each of rules versions 9 to 27, what differs from the next newer version.
- Rules code between versions: search the docs for "version" and the spec for the area. The named milestones (`MILESTONES` in `src/version.rs`) give the version numbers their names, for example `NAVAL_1` (99) or `CYMANTI_REWORK` (115); they are listed in [specs/version.md](specs/version.md).
- Version 124 (retail 2.18, `ACHIEVEMENTS`) has one rule change: an explode command now queues the end-of-command and run-deferred actions like every other command. Earlier versions skip both. It is amber until a game played on a 2.18 client has been replayed.
