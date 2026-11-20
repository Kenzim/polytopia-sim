# Pathfinding and attack options

These functions run in `src/rules/path.rs`. They are not a retail-identity claim.

## What runs

| Behaviour | Rust |
| --- | --- |
| Tile accessibility | `is_tile_accessible` |
| Port requirement for the moving unit | `require_port_for_unit` |
| Reachable tiles | `get_move_options` |
| Neighbour lookup | `neighbor_coords` / `area` |
| May the unit stop on the tile | `is_allowed_to_finish` |
| Shortest path on the map | `get_path` |
| Shortest path for a unit in a game state | same search, after the port requirement is set up for the unit |
| Path rebuild | destination-first list inside `get_path` |
| One A* step | inside `get_path` |
| Unit movement points | `movement_points` |
| Attack options, for a unit and from a given position | `attack_options` |
| Water and land tests | `is_water`, `is_land` |
| Direction of a step and its opposite | `direction_of_delta`, `opposite_direction` |
| Embark improvement check | inside the port gate |
| Can the unit be seen | `visible_unit` |
| Manhattan and plain distance | heuristic and A* priority |
| Tile movement cost | `get_movement_cost` |

`get_move_options` and `get_path` dequeue at most 1000 nodes. The enqueue limit is `max_cost * 10` with a wrapping 32-bit multiply. A leader returns an empty move list. Equal endpoints return a one-element path before the finish check. A destination that fails `is_allowed_to_finish` returns `None` (the game returns null). The rebuilt path is destination, then each `came_from` step, ending at the start.

Neighbors are the Chebyshev square, diagonals included, center excluded, outer `x` then inner `y`.

## Version splits tested

| Gate | Test |
| --- | --- |
| Occupied enemy: `<= 82` Invisible effect, `> 82` Hide | `occupied_enemy_uses_invisible_then_hide` |
| Port entry: `<= 98` owner match, `> 98` friendly and the player can embark | `port_gate_and_leader_follow_the_version_splits` |
| Embark improvement: `<= 94` Port idx 8, later Embark ability 16 | same port test (ability callback) |
| Leader blocks the port step at `>= 120` | same port test |
| Poison movement at `>= 115` | `poison_reduces_movement_only_from_version_115` |
| Infiltrate city rejects `robbed` at `> 82` | `attack_scan_skips_peace_and_empty_attack_and_checks_robbed` |

`movement_points` also adds 1 for Boosted and 1 for Swift or Bubble before the poison step. Poison on a positive total uses `max(movement, 2) - 1`, so 1 stays 1.

## Callees left as arguments

These are arguments of `src/rules/path.rs`. `Engine::path_settings` (`src/engine/legal.rs`) fills them in from the engine state, where `Tile.has_road` and `Tile.effects` do exist.

| Game behaviour | Argument |
| --- | --- |
| Can the player embark | `PathSettings.can_player_embark` |
| Wetland test | `is_wetland(tile_index)`. Effects are not on `Tile`. |
| Direction a bridge may not be built | `blocked_bridge(tile_index)` |
| Improvement has an ability | `improvement_ability(kind, ability)` |
| Unit is aquatic | `require_port_for_unit(is_aquatic, abilities)` |
| Tile is being captured | `being_captured(tile_index)`. Engine callers pass `tile_being_captured(state, tile_index)`: a city whose tile has an owner and a unit of another player standing on it. |
| Tile is floodable | `floodable(tile_index)` |

Looking up a relation inserts a default one in the game. A missing row is read as Neutral and is not inserted.

Unit abilities and `UnitData.movement` are caller inputs. `load_game_logic` is the source for those stats; this module does not hard-code unit tables.

## Movement cost

`get_movement_cost` is the tile movement cost. It does not replace the `movement_cost` callback on `get_move_options` or `get_path`.

| Arm | Compare | Costs that differ |
| --- | --- | --- |
| V114 | `version <= 114` | Skate off ice is 30. Skate along ice at `version <= 113` is 5. Swim off water is 30 above 104 and 20 at or below 104. Zone of control includes a unit that has a leader. |
| Default | `version > 114` | Skate off ice is 1000. Skate on ice is 10. Glide on ice is 9. Swim off water is 1000. A leader does not exert zone of control. a unit that will embark (non-zero) is 1000. |

A road link (`has_road_to`) costs 5. A Slow improvement (ability 19), Mountain, or Forest costs 1000. Flooded plus a water-bound unit, or Algae plus a land-bound unit, costs 1000. Otherwise the step is 10. Fly is 10. No unit that follows transport paths and a road, city, bridge, or route is 5.

`will_embark`, `dest_has_embark`, `has_road_to`, `has_glide`, `has_tech_32`, and the road/effect flags stay arguments of this module; the engine supplies them (`Engine::will_embark` and `Engine::has_road_between` in `src/engine/legal.rs`).

Test: `movement_cost_splits_skate_embark_and_leader_zoc`.
