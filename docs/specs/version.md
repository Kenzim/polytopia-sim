# Version

The game decides which game versions it can load, and which rules file each one uses, with a small set of version checks and a lookup table. Support and the rules-data map here are copied from that behaviour; this file does not invent ranges.

The traffic light (green, amber, red) for each version is in the [README](../../README.md#supported-game-versions). `tests/versions.rs` keeps that table in step with `support_level`.

## Exported API (`src/version.rs`)

### `GameVersion`

A newtype whose inner value is always 51..=124.

| Item | Behaviour |
| --- | --- |
| `GameVersion::for_save(n)` | Ok only when `is_supported(n)`. |
| `GameVersion::for_new_match(n)` | Ok only for 108..=124. A supported version below 108 is `VersionError::BeforeNewMatch`. Anything else is `VersionError::Unsupported`. |
| `GameVersion::current()` and `Default` | `GAME_VERSION` (124). |
| `GameVersion::supported()` | Every supported version, oldest first (51..=124), as a double-ended iterator. |
| `as_i32`, `From<GameVersion> for i32`, `TryFrom<i32>` | Conversions. `TryFrom<i32>` is `for_save`. `Display` prints the number. |
| `at_least(milestone)` | True when this version is the milestone or later, for example `v.at_least(NAVAL_1)`. |
| `is_before(milestone)` | True when this version is older than the milestone. |
| `latest_milestone()` | The newest named milestone this version has reached, or `None` below `CYMANTI_VERSION` (40). Every supported version is above 40, so it is always `Some` for a `GameVersion`. |
| `support()` | The traffic light of this version, `Supported` or `Tested`. |
| `logic_data_version()` | The rules-data version for this game version (calls `logic_data_version`). |
| `load_rules()` | The built-in rules for this version, loaded through `crate::data::load_game_logic_data`. |

### Free functions

- `is_supported(version)`: the window test below.
- `is_too_old(version)`: true when `version < 51`. Too-new is not too-old.
- `logic_data_version(game_version)`: the full ladder, including versions this client refuses.
- `support_level(version) -> Support`: the traffic light for any integer.

### Errors

`VersionError` has two cases, `Unsupported(n)` and `BeforeNewMatch(n)`. Its methods are `version()`, `is_too_old()` (the rejected number is below 51) and `is_too_new()` (above 124). It implements `Display` and `std::error::Error`, and the message names the window that was missed.

### `Support` and `support_level`

`Support` orders `Unsupported < Supported < Tested`.

| Version | Result | README light |
| --- | --- | --- |
| outside 51..=124 | `Unsupported` | red |
| 51..=120 | `Supported` | amber: loads and runs with its rules data, no regression replay of that version |
| 121..=123 (`MIN_TESTED_GAME_VERSION`..=`MAX_TESTED_GAME_VERSION`) | `Tested` | green: backed by regression replays |
| 124 (`GAME_VERSION`) | `Supported` | amber: loads and runs, its one rule change is unit tested, but no game played on a 2.18 client has been replayed yet |

### Tables

- `MILESTONES: [Milestone; 48]`: every named milestone from `CYMANTI_VERSION` to `ACHIEVEMENTS`, oldest first, with strictly increasing versions. A `Milestone` has `name` (the constant's name as a string) and `version`.
- `RULES_LADDER: [RulesStep; 28]`: `RulesStep { first_game_version, rules_version }`. A game version uses the last step whose `first_game_version` is not above it. The first step starts at `i32::MIN`, so every older game version maps to rules version 1, and the last step covers every newer one. `logic_data_version` is a lookup in this table.

A compile-time check ties the tables and constants together: `ACHIEVEMENTS == GAME_VERSION`, the ladder entry for `GAME_VERSION` is `GAME_LOGIC_DATA_VERSION`, and the window constants stay in order. A mismatch does not compile.

Loading rules is done by `load_game_logic_data` in `src/data.rs` (see `rules-data.md`); `GameVersion::load_rules` is a shortcut to it.

## Constants

Window: `GAME_VERSION = 124`, `MIN_NEW_GAME_VERSION = 108`, `MIN_SUPPORTED_GAME_VERSION = 51`, `MIN_TESTED_GAME_VERSION = 121`, `MAX_TESTED_GAME_VERSION = 123`, `GAME_LOGIC_DATA_VERSION = 28`, `WEEKLY_CHALLENGE_GAME_VERSION = 121`, `MIN_TOURNAMENT_GAME_VERSION = 96`.

Milestones, in order: `CYMANTI_VERSION` 40, `BALANCE_1` 45, `BALANCE_2` 50, `DIPLOMACY_VERSION` 60, `ESPORT_VERSION` 70, `DIPLOMACY_VERSION_2` 80 through `DIPLOMACY_VERSION_7` 85, `TRIBE_SKINS_VERSION` 86, `TRIBE_SKINS_VERSION_2` 87, `LOBBY_VERSION` 90 through `LOBBY_VERSION_5` 94, `REFACTOR_VERSION` 95, `SKIN_PACK_2_VERSION` 96, `SKIN_PACK_3_VERSION` 97, `SKIN_PACK_3_VERSION_2` 98, `NAVAL_1` 99 through `NAVAL_4` 102, `SKIN_PACK_4_VERSION` 103, `NAVAL_5` 104, `AQUARION_REWORK` 105, `SUMMER24_BUG_CLEANUP` 106, `AQUARION_REWORK_BALANCE` 107, `AQUARION_REWORK_BALANCE_2` 108, `SCORE_FIXES` 109, `SCORE_FIXES_AND_PIRATE` 110, `BUGFIXES_Q1_2025` 111, `POLARIS_SKIN` 112, `REMOVE_AGGRESSION` 113, `BALANCE_2025` 114, `CYMANTI_REWORK` 115, `CYMANTI_REWORK_PATCH` 116, `CYMANTI_REWORK_PATCH2` 117, `CYMANTI_REWORK_PATCH3` 118, `CYMANTI_SKIN` 119, `BUG_PATCH_Q1_26` 120, `PLAYER_CLIMATE` 121, `CHAMPIONSHIP_FIXES` 122, `AWAY_STARS_CHAMPIONSHIP_FIXES` 123, `ACHIEVEMENTS` 124.

Not included: the game's default semantic version, default version and avatar data version constants, and its semantic-version getters.

## `is_supported`

A game version is supported when it is in 51..=124, which is `(version - 51) as u32 < 74` with wrapping subtraction, so any negative or too-small input falls outside the window. The upper end is `GAME_VERSION`, so 125 is unsupported.

## `is_too_old`

True when `version < 51` (signed). 125 is unsupported and not too old.

## `logic_data_version`

Maps a game version to the rules-data version. It does not reject versions outside 51..=124.

| Game version | Logic data |
| --- | --- |
| < 9 | 1 |
| 9 | 2 |
| 10 | 3 |
| 11..=13 | 4 |
| 14..=39 | 5 |
| 40..=42 | 6 |
| 43..=44 | 7 |
| 45..=49 | 8 |
| 50..=59 | 9 |
| 60..=79 | 10 |
| 80 | 11 |
| 81..=82 | 12 |
| 83..=85 | 13 |
| 86..=94 | 14 |
| 95 | 15 |
| 96..=98 | 16 |
| 99 | 17 |
| 100..=103 | 18 |
| 104 | 19 |
| 105..=106 | 20 |
| 107..=109 | 21 |
| 110..=113 | 22 |
| 114 | 23 |
| 115 | 24 |
| 116 | 25 |
| 117..=118 | 26 |
| 119 | 27 |
| >= 120 | 28 |

Rules versions 1..=8 belong to game versions this engine refuses and have no file. `>= 120` stays on 28, including versions above 124.

The per-version differences between the rules files are listed in [the generated change log](../reference/changelog.md).
