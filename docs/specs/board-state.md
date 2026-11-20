# Board state

One set of structs in `src/types.rs` holds a match for every supported version, 51 through 123. `GameState.version` is the game's version `int`. Rules read or ignore fields from that int. There is no second struct per version.

The tables below list each Rust field with a note where the game's data needs one. They say nothing about memory layout in this crate. Nothing here is serialized.

Existing `Tile`, `Map`, `PlayerState`, `MapGeneratorSettings`, `WorldCoordinates`, `MapSize`, `MapPreset`, and `BotDifficulty` fields are unchanged. `Tile.improvement` stays the kind id. Runtime stats, tech, diplomacy, fog, and tasks live on `GameState`.

## GameState

| Rust | Note |
| --- | --- |
| `version: i32` | The game version |
| `seed: i32` | Match seed |
| `map: Map` | The map data |
| `players: Vec<PlayerState>` | Player list |
| `current_turn: u32` | |
| `current_player_index: u8` | |
| `current_unit_id: u32` | Last unit id handed out |
| `current_state: GameStatus` | A byte in the game |
| `current_command: u16` | |

`GameStatus` is `Unknown = 0`, `Lobby = 1`, `Started = 2`, `FinalTurn = 3`, `Ended = 4`.

`current_player_id()` is the current player: `players[current_player_index].id`. A missing index returns `None`. The game throws.

`GameState::default` is the zeroed object: version 0, seed 0, a 0×0 map, empty lists, `Unknown`, command 0. The game's constructor then draws the seed and the village-name seed from a random source in `0..0x7FFFFFFF`. Default does not draw those. `GameState::new(version, seed)` sets the two ints and leaves the rest defaulted.

Also on `GameState`, not part of the game's own state object: `player_runtime`, `units`, `improvements`, `tech`, `diplomacy`, `fog`, `tasks`. The game stores those on the player or the tile. They are here so `PlayerState` and `Tile` stay fixed.

Skipped from the game's match state:

| Field | Why |
| --- | --- |
| Village-name seed | Only feeds village-name strings. The constructor randomizes it. |
| Match settings | Lobby and match setup: name, timers, skins, spectators, tribes, game rules. Map size and preset already have `Map` / `MapGeneratorSettings`. |
| Command log | Not this slice. The constructor allocates an empty list. |
| Action log | Same. |
| Pending command triggers | Same. |
| Route-update flag | Route dirty flag. |
| Random hash and village-name hash | xxHash objects built from the seed; they are not plain ints. |
| Mocked rules data | Test hook. Rules data stays in `data.rs`. |

## UnitState

Board units are `GameState.units`. A passenger is `UnitState.passenger`, nested on the carrier, not a second `units` entry. `leader` and `follower` are ids. A unit has a leader when `leader != 0`.

| Rust | Note |
| --- | --- |
| `id: u32` | |
| `leader: u32` | |
| `follower: u32` | |
| `owner: u8` | |
| `birth_climate: i32` | The tribe the unit was born under |
| `type_idx: i32` | The unit type id |
| `previous_turn_end` | Coordinates at the end of the previous turn |
| `coordinates` | |
| `home` | |
| `passenger` | Nested unit carried by this one |
| `health: u16` | |
| `promotion: u16` | Promotion level |
| `xp: u16` | |
| `moved: bool` | |
| `attacked: bool` | |
| `created_turn: u16` | |
| effect bools | The unit's list of effects, one bool per effect |
| `temporary_explorer: bool` | |

Unit effect bools, in enum order: `frozen`, `poisoned`, `boosted`, `invisible`, `bubble`, `petrified`, `swift`, `double_ready`, `charmed`. Combat reads the first four (Boosted adds 50 attack, Frozen blocks retaliation, Invisible skips the pacifist reset, Poisoned changes defence bonus). The other five are the rest of that enum, not extra effects.

The constructor sets `health` to 10 and allocates an empty effects list. `Default` matches that (`UNIT_CTOR_HEALTH`). Creating a unit through the game replaces health from the unit data; this slice does not.

Skipped:

| Field | Why |
| --- | --- |
| Birth-climate skin type | Visual. |
| Direction | Sprite facing. The constructor writes south (7). |
| Flipped | Sprite. |
| Unit data reference | Static data. Callers look up `type_idx`. |

## Improvement runtime

`ImprovementRuntime` is keyed by `tile` (`Tile.index`). The game's improvement object has no tile index; it hangs off the tile. Kind stays on `Tile.improvement`.

| Rust | Note |
| --- | --- |
| `owner: u8` | Obsolete; the tile owner wins |
| `founder: u8` | |
| `level: u16` | |
| `founded: u16` | Turn founded |
| `xp: i16` | |
| `population: i16` | |
| `production: u16` | |
| `base_score: u16` | |
| `border_size: u16` | |
| `upgrade: u16` | |
| `connected_to_capital_of_player: u8` | |
| `rewards` | List of city rewards |
| `decomposing`, `robbed` | The improvement's effect list, one bool per effect |

`CityReward` values match the game, including the gaps (`Rebellion = 11`, `Poison = 13`, `TutorialExplorer = 100`). The improvement effects are only `decomposing = 0` and `robbed = 1`. The constructor allocates two empty lists and leaves the numbers zero. `Default` is that zero state.

Skipped: the type (it is `Tile.improvement`) and the name (a display string).

## Tech, tasks, diplomacy, fog

`OwnedTech` is the player's available tech (a list of tech types): `player` plus `tech: Vec<i32>` of those idxs. Not a field on the Rust `PlayerState`.

`PlayerTasks` is the player's task list. `PlayerTask.kind` is the task type: `None` 0, `Pacifist` 1, `Genius` 2, `Network` 3, `Wealth` 4, `Killer` 5, `Metropolis` 6, `Explorer` 7, `ExploreLighthouses` 8, `Convert` 9. `is_started` and `is_completed` are the task's started and completed flags. The one extra count a task type carries is `converts` (Convert), `explored_tiles` (Explorer), `kills` (Killer), or `turns` (Pacifist). The other three stay 0. Class targets, not instance fields: Convert 3, Pacifist 5, Killer 10, Metropolis 5, Network 5, Wealth 100.

`PlayerDiplomacy` is the player's relations (a map from player id to relation). `DiplomacyLink.other` is the map key. A relation:

| Rust | Note |
| --- | --- |
| `state` | `Neutral` 0, `Peace` 1, `War` 2, `CoolDown` 3 |
| `last_attack_turn` | |
| `embassy_level` | |
| `last_peace_broken_turn` | |
| `first_meet` | |
| `embassy_build_turn` | |
| `previous_attack_turn` | |

The constructor writes `last_attack_turn` and the other four turn fields to `DIPLOMACY_DEFAULT_TURN` (-100). `state` and `embassy_level` stay 0. `Default` matches that. Embassy income reads `Peace` and `embassy_level`. `has_war_with` reads `last_attack_turn`, not `state == War`.

`TileExplorers` is the tile's explorer list: the ids of the players who have explored that tile index. No `Tile` field changes. An empty `explorers` vec is the empty list map generation writes on lighthouse corners when `version >= 104`. A missing entry is the same for the explored check.

## Player fields not on `PlayerState`

Rust `PlayerState` keeps `id`, `handicap`, `currency`, `score`, `tribe`, `auto_play`, `start_tile`. The rest of the game's player data that is still match data is `PlayerRuntime`, keyed by `player`.

| Rust | Note |
| --- | --- |
| `tribe_mix` | |
| `climate` | |
| `has_chosen_tribe` | |
| `skin` | Skin type |
| `resigned_turn` | |
| `resigned_at_command` | Command index |
| `wiped_at_command` | Command index |
| `aggressions` | Map from player id to aggression |
| `known_players` | |
| `built_unique_improvements` | |
| `end_score` | |
| `cities` | |
| `kills` | |
| `casualties` | |
| `wipe_outs` | |
| `killer_id` | |
| `killed_turn` | |
| `block_train_units` | |

The constructor sets `handicap = 1` (already on `PlayerState`, and that struct has no `Default`), and `resigned_turn` / both command indexes to -1. `PlayerRuntime::default` uses those -1s. `skin` is the skin type int map generation copies onto tiles, not a sprite.

Skipped from the game's player data:

| Field | Why |
| --- | --- |
| User name | Profile string. |
| Account id | Network GUID. |
| Available tech | Stored as `OwnedTech`. |
| Tasks | Stored as `PlayerTasks`. |
| Relations | Stored as `PlayerDiplomacy`. |
| Diplomacy messages | Message type and sender only. Pending inbox, separate from the relations. |
| Colour | Palette. The constructor writes -1. |
| AI state | The AI module owns that. |
| Unlocked-tech cache | Cache of tech data. |
| Capital-count cache | Cache. |
| Opinions | A separate manager, not a plain field. |

## Tile fields left on `Tile` or not stored

Already on `Tile`: coordinates, terrain, climate, resource kind, improvement kind, owner, capital flag, city flag, ruin flag. Not stored here: skin, effects (Flooded through Foam), altitude, capital-of, shorelines, ruling-city coordinates, the road / route / had-route flags, continent, upgrade tech, last population check, available population. Roads and tile effects are real rules data; this slice only lifts `explorers` off the tile. The tile's unit is in `GameState.units`, not a `Tile` field.
