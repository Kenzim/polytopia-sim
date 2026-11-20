# How the engine is built

Everything below is in `src/engine/` unless it says otherwise. The mechanics are documented in
[docs/README.md](docs/README.md); which game versions are supported is in the README.

| Piece | Files | What it does |
| --- | --- | --- |
| state, commands, legality, execution | `mod.rs legal.rs apply.rs city.rs turn.rs vision.rs` | the game state, `legal_commands`, `apply` and `apply_unchecked`, turn flow, scoring, fog |
| routes and city connections | `routes.rs` | `update_routes`, `find_connected_cities`, `update_city_connections`; run on build and capture |
| retail AI (`ai_get_move`) | `ai.rs ai_rate.rs ai_move.rs ai_improve.rs ai_research.rs ai_state.rs opinion.rs` | situation analysis, missions, hit list, opinions, `rate_position`/`rate_battle`, unit moves, research (with `check_for_tech_needs`), train, build, upgrades, roads, diplomacy offers (see [docs/ai.md](docs/ai.md)); issues the Polaris freeze-area and break-ice commands |
| retail save loader and replay | `load.rs replay.rs` | saves of game versions 121 to 124, the recorded command stack, `Engine::start_match`, `command_from_raw` |
| position files | `position.rs` (`../json.rs`) | the engine's own JSON game format: `Engine::to_position` / `Engine::from_position`; every save round-trips identically, and so does every turn start of the recorded games (state and AI move) |
| map generator | `../map/` | the game's map generator, including its search for a fair map; matches ten captures of the retail game tile for tile |

## Examples

`cargo run --release --example <name> -- args`:

* `replay <save> [ai]`: replay every recorded command of a save on the engine, print the first commands that are not
  legal and the differences in the final state; with `ai`, also how often `ai_get_move` chooses the recorded AI
  command. A save is `<init>+<cur>` for the two files the game keeps. Debug switches are environment variables:
  `LIMIT=n`, `DUMP_AT=n`, `AI_SHOW=n`, `AI_DEBUG=1`, `CITY_DIFF=1`, `TRACK_UNIT=id` and others listed in the file.
* `position from-save <save> [out.json]`: the save's current position as a position file. `check <save>...`: save,
  position and engine agree, with the same legal commands. `replay-check <save>`: the same at every turn start of the
  recorded game, including the AI's move. `moves <position.json> [--json]`: legal commands and the AI's move for the
  player to move. `meta [version]`: unit, tech, tribe and improvement tables.
* `play_game [seed] [turns]`, `play_ai [seed] [turns]`: the built-in AI plays every seat.
* `play_random [seed]`: random legal play, a smoke test.
* `export_map`, `make_view`, `seed_search`: map JSON export and the map-seed search from a 5x5 starting view
  (`src/search.rs`).
* `convert_gamedata`: convert between the game's own rules files (from your own install) and `data/rules/`.
* `gen_docs`: regenerate `docs/reference/` from the rules data.

## Position files

JSON, `{"format": "polytopia-position", "version": 1, "game", "players", "tiles", "improvements", "units", "triggers", "peace_requests", "deferred"}`. `to_position` writes every field; `from_position` fills in whatever is left out, so a hand-made file only needs the map:

* `game`: `turn` (1), `current_player` (first player), `width`/`height` (else `size` or the largest coordinate + 1), `game_version` (123), `seed`, `command`, `unit_id`, `status`, `settings` (`base_game_mode`, `turn_limit`, `difficulty`, ...).
* `players[]`: `id`, `tribe`, `human` (false = AI), `stars` (5), `tech` (tech idx list; missing = the tribe's starting techs), `known` (missing = everyone), `diplomacy` (missing = neutral with everyone), `score` (missing = worked out), plus the save's counters (kills, tasks, messages, ...).
* `tiles[]`: `x`, `y`, `terrain`, `climate`, `resource`, `improvement` (or `city: true`), `owner`, `capital`, `road`, `explored` (player ids; missing on every tile = everyone has explored everything), `altitude`, `ruling` (missing = the nearest own city whose border reaches it), `route` (missing on every tile = recomputed), `effects`.
* `improvements[]`: the runtime record of a city or building at `x, y`: `level`, `xp`, `population`, `rewards` (CityReward codes), `border_size`, `production`, `base_score` (the last three follow from the rewards when missing), `name`, ... A city or building without one gets the defaults of a new one.
* `units[]`: `type`, `owner`, `x`, `y`, `health` (tenths; missing = full), `promotion`, `xp`, `moved`, `attacked`, `flags` (frozen, poisoned, boosted, invisible, bubble, petrified, swift, double_ready, charmed), `passenger` (a unit), `id` (missing = next free), `home` (missing = the ruling city), `previous`, `created`, `direction`, `birth_climate`, `leader`, `follower`.
* `triggers[]` and `peace_requests[]`: city rewards and peace requests waiting for an answer.
* `deferred[]`: actions on the action stack, top last (`Engine::deferred`), each with a `kind`: `city_level_up`, `improvement_level_up` and `improvement_level_down` (`x`, `y`); `income` and `score` (`player`, `amount`); `peace_request` (`player`, `from`); `population` (`x`, `y`, `amount`); and the actions held behind a city reward (see [docs/economy.md](docs/economy.md#city-rewards)): `scout` (`player`, `moves`, `seed`, `area`, `path`), `infiltrate` (`player`, `x`, `y`, `land`, `water`, `next`, `pay`, `tiles`) and `train` (`player`, `type`, `x`, `y`, `home_x`, `home_y`, `veteran`). Missing = empty.

Not stored: the AI's per-player memory (`ai_states`), opinions and the population cache; they are rebuilt, which matches retail at the start of a turn (`replay-check`).

## How it is checked against the game

* **Recorded games.** 44 games recorded from the retail game (game versions 121 to 123; normal tribes, Cymanti,
  Polaris, Elyrion and Aquarion) replay command by command. Every recorded command is legal on the engine, apart
  from the server's own end-of-game bookkeeping commands, which carry no move. At the end, every player's stars,
  score, cities, units and techs match the server's, and so does every surviving unit's home city. The replay tests in
  `tests/` pin individual rules found this way, each with the game it came from.
* **The AI.** The same games compare every recorded move of the retail AI with the engine AI's choice (`replay <save> ai`).
  They agree on every move except 14 by the one Elyrion AI player, whose own abilities (Enchant Animal) the engine AI
  does not yet value as the game's does.
* **Maps.** Ten maps captured from the retail game, with the seeds of every attempt the generator made, are rebuilt
  tile for tile (`tests/map_capture.rs`).
* **Saves and positions.** Every save round-trips through the position format, and so does every turn start of the
  recorded games.

## Known gaps

* The engine's AI is not tuned to Elyrion: it does not value Enchant Animal as the game's AI does.
* Game versions other than 121 to 123 have no recorded games behind them (amber in the README's table).
