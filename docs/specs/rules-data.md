# Rules data

The game keeps its rules (units, tribes, techs, improvements, terrain, resources, tasks, skins, diplomacy) in one JSON document per rules version. The game's current rules version is **28** and the game version is 123. Rules versions 9 through 28 are supported here (game versions 51 and up). Versions 1 to 8 belong to game versions this engine refuses and are not available.

There are two formats in this crate:

- **The engine's own files** in `data/rules/`: `v28.json` is the full rules, and `v27.json` down to `v9.json` are small change files (a JSON merge patch against the next newer version). Key names are snake_case. The format is described in the module docs of `src/data/rules.rs`. These files are compiled into the crate and are what `load_embedded_game_logic` (version 28) and `load_game_logic_data(logic_data_version)` (9..=28) read. `load_game_logic_data_from_dir` reads the same files from a directory.
- **The game's data file**, which this document describes. `load_game_logic_json` parses one document in that layout, with a small handwritten JSON reader (no serde). `src/data/convert.rs` converts between the two layouts and checks that the round trip is exact for every version. The camelCase key names below are the game's.

No unit rows are guessed. A record the file lacks is absent from the loaded rules.

## The game's data file

The game ships one such document for each rules version 1 to 28. The text is read by a lenient JSON reader (Newtonsoft's): it accepts one trailing comma in an object or array, comments and single-quoted strings. A second comma in a row is refused. Plain strict parsers (Python's `json.loads`, for one) reject even the single trailing comma, and versions 10, 11, 14, 15 and 18 each contain one. The handwritten reader here allows one trailing comma in objects and arrays, so all of 9..=28 parse. None of the documents uses comments or single quotes.

Byte lengths of the original documents (UTF-8), for checking a copy: 9 is 39518, 10 is 40803, 11 is 40860, 12 is 41112, 13 is 41128, 14 is 41292, 15 is 41386, 16 is 41467, 17 is 44455, 18 is 43585, 19 is 41758, 20 is 47572, 21 is 47559, 22 is 47690, 23 is 48187, 24 is 51419, 25 is 51395, 26 is 51396, 27 is 51488, 28 is 51501. The built-in rules do not store the original bytes any more; `data::tests` pins a fingerprint per version instead.

## Layout of the game's file

Root keys, and the section each becomes in the engine's own files:

| Game key | Engine key |
| --- | --- |
| `unit_data` | `units` |
| `tribe_data` | `tribes` |
| `tech_data` | `techs` |
| `improvement_data` | `improvements` |
| `terrain_data` | `terrains` |
| `resource_data` | `resources` |
| `task_data` | `tasks` |
| `skin_data` | `skins` |
| `diplomacy_data` | `diplomacy` |

Dictionary entries are objects keyed by a lowercase id (`warrior`, `imperius`). `idx` is the game's numeric id for the entry when it has one (`warrior` is 2, `imperius` is 7, `xinxi` is 13). Skin ids on tribes are stored verbatim (`Ninja`, `Scholar`), not lowercased. Override maps (`unit_overrides`, `tech_overrides`, `improvement_overrides`) hold string ids in the file; the game turns them into objects after parsing.

`tech_data.rituals` lists `"cost": 2` twice. Both values are 2. The parser keeps that value and returns an error if a duplicate key ever carries a different value.

### Counts in v28

59 units, 18 tribes, 49 techs, 49 improvements, 9 terrains, 10 resources, 9 tasks, 3 skins, one diplomacy object.

### Units

Fields the game reads: `idx`, `hidden`, `cost`, `health`, `defence`, `movement`, `weapon`, `range`, `attack`, `unit_abilities`, `movement_terrain`, `upgrades_from`, `promotes_to`, `growth_rate`.

Every unit in v28 has `cost`, `health`, `defence`, `movement`, `weapon`, `range`, `attack`, `idx` and `promotion_limit`. `promotion_limit` is not a stored field in the game (there it is computed), so the game's own reader would ignore it. It is kept here because it is in the file. `hidden` defaults to false when omitted. `growth_rate` stays `None` when omitted (`growth_rate_or_default` gives the game's default of 0). `promotes_to` does not occur. `upgrades_from` is set for the dragon, bug and naval upgrade lines.

`weapon` is the game's weapon number (1 is the club, on the warrior). `weapon_name` maps that number to its name; it is not a stat table.

### Tribes

Fields the game reads: `idx`, `color`, `language`, `starting_stars`, `bonus`, `starting_unit`, `category`, `price_tier`, `skins`, `terrain_modifier`, `resource_modifier`, `starting_tech`, `starting_resource`, `tribe_abilities`, `tech_overrides`, `unit_overrides`, `improvement_overrides`.

`style` and `climate` are on every tribe in the file but the game does not read them. `starting_stars` is present on only some tribes (Xin-xi is 7). Modifiers are numbers; integers such as `"fruit": 2` are kept as floats.

### Techs

Fields: `idx`, `cost`, `tech_unlocks`, `improvement_unlocks`, `unit_unlocks`, `ability_unlocks`, `task_unlocks`, `movement_unlocks`, `defence_bonus_unlocks`. All of those appear. Omitted lists are empty.

### Improvements

Fields: `idx`, `hidden`, `cost`, `work`, `border_size`, `max_level`, `improvement_abilities`, `creates`, `rewards`, `terrain_requirements`, `adjacency_requirements`, `adjacency_improvements`, `routes`, `range`, `growth_rate`, `growth_rewards`.

`cost` is omitted for ruins, monuments, forest clearing and several other rows. That is `None`, not a made-up price; `cost_or_default` gives the game's default of 0. `should_suggest_unlock` appears in the file only (the game does not read it). Nested rewards, growth rewards and requirement objects use `Option`, so a missing `currency` is not turned into a real 0 unless the caller asks for the default. `creates.effect` is the string `algae` (the game's tile effect number 4). Adjacency objects allow the keys `resource` and `not_improvement`; v28 does not use them.

### Terrains, resources, tasks, skins, diplomacy

Terrains are only `idx`. Resources add `resource_terrain_requirements`. Tasks add `improvement_unlocks`. Skins have `language` and sometimes `color` (magma and cute omit colour). Diplomacy has cost 5, income 2, growth time 3, max level 3, upgrade cost 20 and dividend income 2. The growth time key is spelt `embasy_growth_time` in the game's file, and the engine keeps that spelling (`embasy_growth_time`). `embassy_tech_price_multiplier` is 0.5 and is not a field the game reads.

## Stats that were not invented

The game's unit type list includes `Ship`, `Battleship`, `Boat` and `Navalon`. None of those keys are in v28, and `load_embedded_game_logic` does not create them. Weapon names that no unit uses (including 15, `Laser`) exist only in the name table behind `weapon_name`.

Version 9 omits `diplomacy_data` and `skin_data`. Versions 10 to 19 omit `skin_data`. A missing `skin_data` object is an empty map. A missing `diplomacy_data` object is `None`, not a diplomacy row of zeros. Every other root section is present in 9..=28, and record fields that v28 requires on a present row are present. No extra keys showed up, so the v28 structs did not grow new stat fields.
