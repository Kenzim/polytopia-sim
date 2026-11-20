//! Board-changing actions.
//!
//! Each action is ported with every version compare it makes.
//! Bodies that only push UI, animation, or a later action type keep the board
//! write that is already on [`GameState`] and say what is skipped.

use crate::rules::combat::{get_battle_results, BattleQuery, CombatUnit};
use crate::rules::economy::{
    self, learn_tech, start_turn_currency, v59_known_player_aggression, v59_unknown_player_aggression, LearnTechEffect,
    StartTurnTile,
};
use crate::rules::path::{self, attack_options};
use crate::{
    Aggression, DiplomacyLink, DiplomacyRelation, DiplomacyRelationState, GameState, ImprovementRuntime, OwnedTech,
    PlayerRuntime, PlayerTask, TaskKind, UnitState, WorldCoordinates, UNIT_CTOR_HEALTH,
};

/// The attack body by version: `<= 94` is V94, `<= 104` is V104, `<= 118` is V116.
pub const VERSION_ATTACK_V94: i32 = 0x5E;
pub const VERSION_ATTACK_V104: i32 = 0x68;
pub const VERSION_ATTACK_V116: i32 = 0x76;
/// Move action version gates.
pub const VERSION_MOVE_V59: i32 = 0x3B;
pub const VERSION_MOVE_V83: i32 = 0x53;
pub const VERSION_MOVE_V98: i32 = 0x62;
pub const VERSION_MOVE_V101: i32 = 0x65;
/// Train action version gates, including the water embark check inside Default.
pub const VERSION_TRAIN_V59: i32 = 0x3B;
pub const VERSION_TRAIN_V105: i32 = 0x69;
pub const VERSION_TRAIN_EMBARK: i32 = 0x73;
/// Build action version gate.
pub const VERSION_BUILD_V105: i32 = 0x69;
/// Capture-city action version gates.
pub const VERSION_CAPTURE_V59: i32 = 0x3B;
pub const VERSION_CAPTURE_V108: i32 = 0x6C;
/// Start-turn action version gate.
pub const VERSION_START_V59: i32 = 0x3B;

/// The Push move reason. Default skips the moved/attacked write for it.
pub const MOVE_REASON_PUSH: u8 = 2;
/// The Ice terrain type. A move onto ice does not set moved/attacked.
const TERRAIN_ICE: i32 = 6;
/// Unit ability values the train action's default arm reads.
const ABILITY_SWIM: i32 = 10;
const ABILITY_FLY: i32 = 13;
const ABILITY_CARRY: i32 = 11;

/// Which attack body the version selects.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AttackArm {
    /// Version 94 and older.
    V94,
    /// Versions up to 104.
    V104,
    /// Versions up to 118. Clears Swift and Bubble before the health write.
    V116,
    /// Later versions. Same Swift/Bubble clear, then a Bubble action that is not applied.
    Default,
}

pub fn attack_arm(version: i32) -> AttackArm {
    if version <= VERSION_ATTACK_V94 {
        AttackArm::V94
    } else if version <= VERSION_ATTACK_V104 {
        AttackArm::V104
    } else if version <= VERSION_ATTACK_V116 {
        AttackArm::V116
    } else {
        AttackArm::Default
    }
}

/// The attack action loads damage from the action and goes on to
/// [`perform_attack`]. `damage` here is that field. Call `attack_damage` for
/// the battle results ([`get_battle_results`]).
pub fn perform_attack(
    state: &mut GameState,
    player_id: u8,
    origin: WorldCoordinates,
    target: WorldCoordinates,
    damage: i32,
    attacker_abilities: &[i32],
    attacker_hidden: bool,
) -> bool {
    let arm = attack_arm(state.version);
    let Some(defender_index) = unit_index_at(state, target) else {
        return false;
    };
    let attacker_index = unit_index_at(state, origin);
    let defender_owner = state.units[defender_index].owner;
    let health = i32::from(state.units[defender_index].health);
    let applied = min_i32(damage, health);
    let updated = health.wrapping_sub(applied) as u16;
    state.units[defender_index].health = updated;
    state.units[defender_index].boosted = false;
    if matches!(arm, AttackArm::V116 | AttackArm::Default) {
        state.units[defender_index].swift = false;
        state.units[defender_index].bubble = false;
    }
    if let Some(attacker_index) = attacker_index {
        if attacker_index != defender_index {
            state.units[attacker_index].boosted = false;
        }
    }

    if updated != 0 {
        // Poison, flood, and the Bubble action are later action types. Primary damage is already applied.
        let _ = attacker_abilities;
        return true;
    }

    let killer_id = if origin == target {
        player_id
    } else if let Some(attacker_index) = attacker_index {
        state.units[attacker_index].owner
    } else {
        player_id
    };
    let grant_xp = origin != target
        && attacker_index.is_some()
        && !attacker_hidden
        && !attacker_abilities.contains(&ABILITY_CARRY);
    if grant_xp {
        if let Some(attacker_index) = attacker_index {
            if attacker_index != defender_index {
                let xp = state.units[attacker_index].xp;
                state.units[attacker_index].xp = xp.wrapping_add(1);
            }
        }
    }
    if defender_owner != 255 {
        add_kills(state, killer_id);
        add_casualties(state, defender_owner);
    }
    state.units.remove(defender_index);
    true
}

/// Work out the battle results, then [`perform_attack`] with `attack_damage`.
pub fn attack(
    state: &mut GameState,
    player_id: u8,
    origin: WorldCoordinates,
    target: WorldCoordinates,
    attacker: &CombatUnit,
    defender: &CombatUnit,
    query: &BattleQuery,
    attacker_abilities: &[i32],
    attacker_hidden: bool,
) -> bool {
    let results = get_battle_results(attacker, defender, query);
    perform_attack(state, player_id, origin, target, results.attack_damage, attacker_abilities, attacker_hidden)
}

/// Whether `target` is in [`attack_options`] for `unit`. Convenience for
/// [`BattleQuery::attacker_in_attack_options`].
pub fn target_in_attack_options<C, F>(
    state: &GameState,
    unit: &UnitState,
    target: WorldCoordinates,
    range: i32,
    raw_attack: i32,
    abilities: &[i32],
    being_captured: C,
    floodable: F,
) -> bool
where
    C: Fn(i32) -> bool,
    F: Fn(i32) -> bool,
{
    attack_options(state, unit, range, true, raw_attack, abilities, false, being_captured, floodable).contains(&target)
}

/// The move action.
///
/// The unit's coordinates become the last path entry. Onto non-ice, `moved`
/// and `attacked` are both set, except
/// the Push move reason on the default arm (`version > 101`). Embark, explore,
/// poison, and area damage are not applied.
pub fn move_unit(state: &mut GameState, unit_id: u32, path: &[WorldCoordinates], reason: u8) -> bool {
    let Some(destination) = path.last().copied() else {
        return false;
    };
    let Some(index) = state.units.iter().position(|unit| unit.id == unit_id) else {
        return false;
    };
    let terrain = state.map.tiles.iter().find(|tile| tile.coordinates == destination).map(|tile| tile.terrain);
    state.units[index].coordinates = destination;
    if terrain != Some(TERRAIN_ICE) && move_sets_spent(state.version, reason) {
        state.units[index].moved = true;
        state.units[index].attacked = true;
    }
    true
}

/// V59, the V83 arm (also selected for versions through 98), and V101 all set
/// the spent flags. Default (`version > 101`) does not set them for Push.
fn move_sets_spent(version: i32, reason: u8) -> bool {
    if version <= VERSION_MOVE_V59
        || version <= VERSION_MOVE_V83
        || version <= VERSION_MOVE_V98
        || version <= VERSION_MOVE_V101
    {
        true
    } else {
        reason != MOVE_REASON_PUSH
    }
}

/// The train action.
///
/// Currency becomes `currency - cost`; a negative result is rejected and
/// leaves the board unchanged.
/// The new id comes from the next-unit-id counter: `current_unit_id` increments
/// and that value is the id. Health is the caller's `unit_health` (the value
/// a new unit copies from the unit data).
///
/// On the default arm (`version > 105`), a water tile without Swim or Fly
/// would push an embark action when `version <= 115`. That action
/// is not executed; [`TrainResult::embarked`] reports the push.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TrainResult {
    pub spawned: bool,
    pub unit_id: u32,
    pub embarked: bool,
}

pub fn train(
    state: &mut GameState,
    player_id: u8,
    type_idx: i32,
    coordinates: WorldCoordinates,
    cost: i32,
    unit_health: i32,
    abilities: &[i32],
) -> TrainResult {
    let none = TrainResult { spawned: false, unit_id: 0, embarked: false };
    let Some(player) = state.players.iter_mut().find(|player| player.id == player_id) else {
        return none;
    };
    let next = player.currency.wrapping_sub(cost);
    if next < 0 {
        return none;
    }
    player.currency = next;
    if unit_index_at(state, coordinates).is_some() {
        // Training onto an occupied space is not inlined.
        return none;
    }
    state.current_unit_id = state.current_unit_id.wrapping_add(1);
    let id = state.current_unit_id;
    let health = u16::try_from(unit_health).unwrap_or(UNIT_CTOR_HEALTH);
    state.units.push(UnitState {
        id,
        owner: player_id,
        type_idx,
        coordinates,
        health,
        created_turn: state.current_turn as u16,
        ..UnitState::default()
    });
    let water = state
        .map
        .tiles
        .iter()
        .find(|tile| tile.coordinates == coordinates)
        .is_some_and(|tile| path::is_water(tile.terrain));
    let can_swim = abilities.contains(&ABILITY_SWIM) || abilities.contains(&ABILITY_FLY);
    let embarked = train_pushes_embark(state.version) && water && !can_swim;
    TrainResult { spawned: true, unit_id: id, embarked }
}

/// Default (`version > 105`) pushes an embark action on water through version 115.
fn train_pushes_embark(version: i32) -> bool {
    if version <= VERSION_TRAIN_V59 || version <= VERSION_TRAIN_V105 {
        false
    } else {
        version <= VERSION_TRAIN_EMBARK
    }
}

/// The research action, which learns the tech.
pub fn research(
    state: &mut GameState,
    player_id: u8,
    tech_id: i32,
    price_paid: i32,
    tech_data_cost: i32,
    task_ids: &[i32],
    ability_ids: &[i32],
) -> Option<LearnTechEffect> {
    let player = state.players.iter().find(|player| player.id == player_id)?;
    let unlocked =
        state.tech.iter().find(|row| row.player == player_id).map(|row| row.tech.clone()).unwrap_or_default();
    let effect = learn_tech(player, &unlocked, tech_id, price_paid, tech_data_cost, task_ids, ability_ids);
    if let Some(player) = state.players.iter_mut().find(|player| player.id == player_id) {
        player.currency = effect.currency;
        player.score = effect.score;
    }
    if effect.applied {
        if let Some(row) = state.tech.iter_mut().find(|row| row.player == player_id) {
            row.tech.clone_from(&effect.unlocked);
        } else {
            state.tech.push(OwnedTech { player: player_id, tech: effect.unlocked.clone() });
        }
        if let Some(tasks) = state.tasks.iter_mut().find(|row| row.player == player_id) {
            for id in &effect.enabled_tasks {
                if let Some(kind) = task_kind(*id) {
                    if !tasks.tasks.iter().any(|task| task.kind == kind) {
                        tasks.tasks.push(PlayerTask { kind, is_started: true, ..PlayerTask::default() });
                    }
                }
            }
        }
    }
    Some(effect)
}

/// The build action.
///
/// `version <= 105` selects the V105 body; later versions select
/// the default body. Both place the improvement kind on the tile.
/// City population, borders, and the rule area are not inlined.
/// A negative `currency - cost` is the game's currency-setter throw: the kind is still
/// not written, because the game throws out of the action.
pub fn build(
    state: &mut GameState,
    player_id: u8,
    kind: i32,
    coordinates: WorldCoordinates,
    deduct_cost: bool,
    cost: i32,
) -> bool {
    let _ = state.version <= VERSION_BUILD_V105;
    let Some(tile_index) = state.map.tiles.iter().position(|tile| tile.coordinates == coordinates) else {
        return false;
    };
    if deduct_cost {
        let Some(player) = state.players.iter_mut().find(|player| player.id == player_id) else {
            return false;
        };
        let next = player.currency.wrapping_sub(economy::improvement_currency_cost(cost));
        if next < 0 {
            return false;
        }
        player.currency = next;
    }
    let tile_id = state.map.tiles[tile_index].index;
    state.map.tiles[tile_index].improvement = kind;
    if let Some(runtime) = state.improvements.iter_mut().find(|improvement| improvement.tile == tile_id) {
        runtime.owner = player_id;
        runtime.founder = player_id;
    } else {
        state.improvements.push(ImprovementRuntime {
            tile: tile_id,
            owner: player_id,
            founder: player_id,
            founded: state.current_turn as u16,
            ..ImprovementRuntime::default()
        });
    }
    true
}

/// The build-road action stores `has_road = true` on the tile.
/// [`crate::Tile`] does not carry that flag here, so this reports
/// the tile index that the game would mark and does not invent a field.
pub fn build_road(state: &GameState, coordinates: WorldCoordinates) -> Option<i32> {
    state.map.tiles.iter().find(|tile| tile.coordinates == coordinates).map(|tile| tile.index)
}

/// The capture-city action.
///
/// The tile owner becomes `player_id`. `version <= 59` does not update the
/// diplomacy relations from the attack. Later arms do: the
/// attacker's `previous_attack_turn` keeps the old `last_attack_turn`,
/// `last_attack_turn` becomes `current_turn`, and the other side's
/// `previous_attack_turn` becomes `current_turn`.
/// Embassy removal, score, climate, and flood are not inlined.
pub fn capture_city(state: &mut GameState, player_id: u8, coordinates: WorldCoordinates, old_owner: u8) -> bool {
    let version = state.version;
    let found = state
        .map
        .tiles
        .iter_mut()
        .find(|tile| tile.coordinates == coordinates)
        .map(|tile| {
            tile.owner = player_id;
            true
        })
        .unwrap_or(false);
    if !found {
        return false;
    }
    #[allow(clippy::nonminimal_bool)] // kept as the two version arms of the original rule
    if version > VERSION_CAPTURE_V108 || (version > VERSION_CAPTURE_V59 && version <= VERSION_CAPTURE_V108) {
        note_attack(state, player_id, old_owner);
    }
    true
}

/// The end-turn action.
///
/// When `player_id` is the current player, the end-of-player-turn step is
/// run without starting a new turn. The index advances. A wrap increments
/// `current_turn`. `alive` is [`crate::rules::diplomacy::is_alive`], which
/// walks units and cities and is not inlined.
///
/// Robbed is cleared on the ending player's improvements. A `CoolDown`
/// relation of that player becomes `Neutral`.
pub fn end_turn<F>(state: &mut GameState, player_id: u8, mut alive: F) -> bool
where
    F: FnMut(&crate::PlayerState) -> bool,
{
    let ending_is_current =
        state.players.get(usize::from(state.current_player_index)).is_some_and(|player| player.id == player_id);
    if ending_is_current {
        advance_player(state, &mut alive);
    }
    let owned_tiles: Vec<i32> =
        state.map.tiles.iter().filter(|tile| tile.owner == player_id).map(|tile| tile.index).collect();
    for improvement in &mut state.improvements {
        if owned_tiles.contains(&improvement.tile) {
            improvement.robbed = false;
        }
    }
    if let Some(row) = state.diplomacy.iter_mut().find(|row| row.player == player_id) {
        for link in &mut row.relations {
            if link.relation.state == DiplomacyRelationState::CoolDown {
                link.relation.state = DiplomacyRelationState::Neutral;
            }
        }
    }
    ending_is_current
}

/// The start-turn action.
///
/// Currency grants are [`start_turn_currency`]. Each positive amount is one
/// [`increase_currency`] (the game's currency-setter replacement).
/// `version <= 59` also adds [`v59_known_player_aggression`] or
/// [`v59_unknown_player_aggression`], plus 500 for an enemy unit on the
/// actor's tile and 2000 more when that tile is a city.
/// The diplomacy check is the default arm and is not inlined.
pub fn start_turn(state: &mut GameState, player_id: u8, tiles: &[StartTurnTile]) {
    let grants = start_turn_currency(state.version, state.current_turn, player_id, tiles);
    for amount in grants {
        let _ = increase_currency(state, player_id, amount);
    }
    if state.version <= VERSION_START_V59 {
        apply_v59_aggression(state, player_id);
    }
}

/// The increase-currency action.
///
/// The currency setter throws when `amount < 0` and otherwise stores
/// `max(amount, 0)`. This replaces currency; it does not add. The Wealth
/// task check is not inlined.
pub fn increase_currency(state: &mut GameState, player_id: u8, amount: i32) -> bool {
    if amount < 0 {
        return false;
    }
    let Some(player) = state.players.iter_mut().find(|player| player.id == player_id) else {
        return false;
    };
    player.currency = amount;
    true
}

/// The city level-up action, when the tile is a city and
/// the level-up test holds.
///
/// A level-up increments `level`, subtracts
/// the new level from `xp`, and decrements `upgrade`, all in 16-bit wrapping.
/// The level-up score is not inlined.
pub fn city_level_up(state: &mut GameState, coordinates: WorldCoordinates) -> bool {
    let Some(tile) = state.map.tiles.iter().find(|tile| tile.coordinates == coordinates) else {
        return false;
    };
    if tile.improvement != economy::CITY_IMPROVEMENT_INDEX {
        return false;
    }
    let tile_index = tile.index;
    let Some(runtime) = state.improvements.iter_mut().find(|improvement| improvement.tile == tile_index) else {
        return false;
    };
    if !economy::should_level_up(runtime.xp, runtime.level) {
        return false;
    }
    let level = i32::from(runtime.level).wrapping_add(1) as u16;
    runtime.level = level;
    runtime.xp = runtime.xp.wrapping_sub(level as i16);
    runtime.upgrade = runtime.upgrade.wrapping_sub(1);
    true
}

fn apply_v59_aggression(state: &mut GameState, player_id: u8) {
    let Some(actor) = state.players.iter().find(|player| player.id == player_id).copied() else {
        return;
    };
    let known = state
        .player_runtime
        .iter()
        .find(|runtime| runtime.player == player_id)
        .map(|runtime| runtime.known_players.clone())
        .unwrap_or_default();
    let mut deltas: Vec<(u8, i32)> = Vec::new();
    for other in &state.players {
        if other.id == player_id {
            continue;
        }
        let delta = if known.contains(&other.id) {
            v59_known_player_aggression(actor.score, other.score)
        } else {
            v59_unknown_player_aggression(actor.handicap)
        };
        deltas.push((other.id, delta));
    }
    for unit in &state.units {
        if unit.owner == 0 || unit.owner == player_id {
            continue;
        }
        let Some(tile) = state.map.tiles.iter().find(|tile| tile.coordinates == unit.coordinates) else {
            continue;
        };
        if tile.owner != player_id {
            continue;
        }
        deltas.push((unit.owner, economy::V59_ENEMY_UNIT_AGGRESSION));
        if tile.improvement == economy::CITY_IMPROVEMENT_INDEX {
            deltas.push((unit.owner, economy::V59_ENEMY_CITY_AGGRESSION));
        }
    }
    let runtime = runtime_mut(state, player_id);
    for (other, delta) in deltas {
        if let Some(entry) = runtime.aggressions.iter_mut().find(|entry| entry.other == other) {
            entry.count = entry.count.wrapping_add(delta);
        } else {
            runtime.aggressions.push(Aggression { other, count: delta });
        }
    }
}

fn advance_player<F>(state: &mut GameState, alive: &mut F)
where
    F: FnMut(&crate::PlayerState) -> bool,
{
    let count = state.players.len();
    if count == 0 {
        return;
    }
    let mut index = usize::from(state.current_player_index);
    let mut wrapped = false;
    for _ in 0..count {
        index = index.saturating_add(1);
        if index >= count {
            index = 0;
            wrapped = true;
        }
        if alive(&state.players[index]) {
            state.current_player_index = index as u8;
            if wrapped {
                state.current_turn = state.current_turn.wrapping_add(1);
            }
            return;
        }
    }
}

fn note_attack(state: &mut GameState, player: u8, other: u8) {
    let turn = state.current_turn as i32;
    let old = relation_mut(state, player, other).last_attack_turn;
    let relation = relation_mut(state, player, other);
    relation.previous_attack_turn = old;
    relation.last_attack_turn = turn;
    relation_mut(state, other, player).previous_attack_turn = turn;
}

fn relation_mut(state: &mut GameState, player: u8, other: u8) -> &mut DiplomacyRelation {
    if !state.diplomacy.iter().any(|row| row.player == player) {
        state.diplomacy.push(crate::PlayerDiplomacy { player, relations: Vec::new() });
    }
    let row = state.diplomacy.iter_mut().find(|row| row.player == player).expect("row just inserted");
    if !row.relations.iter().any(|link| link.other == other) {
        row.relations.push(DiplomacyLink { other, relation: DiplomacyRelation::default() });
    }
    &mut row.relations.iter_mut().find(|link| link.other == other).expect("link just inserted").relation
}

fn runtime_mut(state: &mut GameState, player: u8) -> &mut PlayerRuntime {
    if !state.player_runtime.iter().any(|runtime| runtime.player == player) {
        state.player_runtime.push(PlayerRuntime { player, ..Default::default() });
    }
    state.player_runtime.iter_mut().find(|runtime| runtime.player == player).expect("runtime just inserted")
}

fn add_kills(state: &mut GameState, player: u8) {
    let runtime = runtime_mut(state, player);
    runtime.kills = runtime.kills.wrapping_add(1);
}

fn add_casualties(state: &mut GameState, player: u8) {
    let runtime = runtime_mut(state, player);
    runtime.casualties = runtime.casualties.wrapping_add(1);
}

fn unit_index_at(state: &GameState, coordinates: WorldCoordinates) -> Option<usize> {
    state.units.iter().position(|unit| unit.coordinates == coordinates)
}

fn min_i32(left: i32, right: i32) -> i32 {
    if left < right {
        left
    } else {
        right
    }
}

fn task_kind(id: i32) -> Option<TaskKind> {
    Some(match id {
        0 => TaskKind::None,
        1 => TaskKind::Pacifist,
        2 => TaskKind::Genius,
        3 => TaskKind::Network,
        4 => TaskKind::Wealth,
        5 => TaskKind::Killer,
        6 => TaskKind::Metropolis,
        7 => TaskKind::Explorer,
        8 => TaskKind::ExploreLighthouses,
        9 => TaskKind::Convert,
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rules::combat::{BattleQuery, CombatUnit};
    use crate::rules::economy::StartTurnTile;
    use crate::{Map, PlayerState, TileExplorers};

    fn player(id: u8, currency: i32, score: u32) -> PlayerState {
        PlayerState {
            id,
            handicap: 1,
            currency,
            score,
            tribe: 1,
            auto_play: false,
            start_tile: WorldCoordinates { x: 0, y: 0 },
        }
    }

    fn board(version: i32) -> GameState {
        let mut state = GameState::new(version, 1);
        state.map = Map::new(3, 1);
        for tile in &mut state.map.tiles {
            tile.terrain = 3;
            state.fog.push(TileExplorers { tile: tile.index, explorers: vec![1, 2].into() });
        }
        state.players.push(player(1, 10, 40));
        state.players.push(player(2, 0, 10));
        state.current_player_index = 0;
        state.current_turn = 1;
        state
    }

    #[test]
    fn attack_damage_clamps_and_version_clears_swift() {
        let mut state = board(VERSION_ATTACK_V104);
        state.units.push(UnitState {
            id: 1,
            owner: 1,
            coordinates: WorldCoordinates { x: 0, y: 0 },
            health: 10,
            ..UnitState::default()
        });
        state.units.push(UnitState {
            id: 2,
            owner: 2,
            coordinates: WorldCoordinates { x: 1, y: 0 },
            health: 10,
            swift: true,
            bubble: true,
            boosted: true,
            ..UnitState::default()
        });
        assert!(perform_attack(
            &mut state,
            1,
            WorldCoordinates { x: 0, y: 0 },
            WorldCoordinates { x: 1, y: 0 },
            4,
            &[],
            false,
        ));
        let defender = state.units.iter().find(|unit| unit.id == 2).unwrap();
        assert_eq!(defender.health, 6);
        assert!(!defender.boosted);
        assert!(defender.swift);
        assert!(defender.bubble);

        state.version = VERSION_ATTACK_V104 + 1;
        state.units[1].health = 10;
        state.units[1].swift = true;
        state.units[1].bubble = true;
        perform_attack(
            &mut state,
            1,
            WorldCoordinates { x: 0, y: 0 },
            WorldCoordinates { x: 1, y: 0 },
            100,
            &[],
            false,
        );
        assert!(state.units.iter().all(|unit| unit.id != 2));
        assert_eq!(state.player_runtime.iter().find(|runtime| runtime.player == 1).unwrap().kills, 1);
    }

    #[test]
    fn attack_uses_get_battle_results() {
        let mut state = board(123);
        state.units.push(UnitState {
            id: 1,
            owner: 1,
            coordinates: WorldCoordinates { x: 0, y: 0 },
            health: 10,
            ..UnitState::default()
        });
        state.units.push(UnitState {
            id: 2,
            owner: 2,
            coordinates: WorldCoordinates { x: 1, y: 0 },
            health: 10,
            ..UnitState::default()
        });
        state.units[0].health = 100;
        state.units[1].health = 100;
        let query = BattleQuery {
            game_version: 123,
            account_for_peace: false,
            has_active_peace_treaty: false,
            simple_data: true,
            attacker_in_attack_options: true,
            can_path_onto_defender: false,
        };
        let full = |defence: i32| CombatUnit {
            health: 100,
            max_health: 100,
            attack: 200,
            defence,
            defence_bonus: 10,
            raw_attack: 1,
            raw_defence: 1,
            range: 1,
            weapon: 0,
            has_stiff: false,
            has_convert: false,
            has_surprise: false,
            has_freeze: false,
            has_tentacles: false,
            has_leader: false,
            has_infiltrate: false,
            frozen: false,
            invisible: false,
        };
        assert!(attack(
            &mut state,
            1,
            WorldCoordinates { x: 0, y: 0 },
            WorldCoordinates { x: 1, y: 0 },
            &full(0),
            &full(200),
            &query,
            &[],
            false,
        ));
        let defender = state.units.iter().find(|unit| unit.id == 2).unwrap();
        assert!(defender.health < 100);
    }

    #[test]
    fn move_sets_coordinates_and_skips_flags_on_ice_or_push() {
        let mut state = board(VERSION_MOVE_V101 + 1);
        state.units.push(UnitState {
            id: 7,
            owner: 1,
            coordinates: WorldCoordinates { x: 0, y: 0 },
            ..UnitState::default()
        });
        assert!(move_unit(&mut state, 7, &[WorldCoordinates { x: 0, y: 0 }, WorldCoordinates { x: 1, y: 0 },], 0,));
        assert_eq!(state.units[0].coordinates, WorldCoordinates { x: 1, y: 0 });
        assert!(state.units[0].moved && state.units[0].attacked);

        state.map.tiles[2].terrain = TERRAIN_ICE;
        state.units[0].moved = false;
        state.units[0].attacked = false;
        move_unit(&mut state, 7, &[WorldCoordinates { x: 2, y: 0 }], 0);
        assert!(!state.units[0].moved && !state.units[0].attacked);

        state.version = VERSION_MOVE_V101;
        state.map.tiles[0].terrain = 3;
        move_unit(&mut state, 7, &[WorldCoordinates { x: 0, y: 0 }], MOVE_REASON_PUSH);
        assert!(state.units[0].moved);
        state.version = VERSION_MOVE_V101 + 1;
        state.units[0].moved = false;
        state.units[0].attacked = false;
        move_unit(&mut state, 7, &[WorldCoordinates { x: 0, y: 0 }], MOVE_REASON_PUSH);
        assert!(!state.units[0].moved);
    }

    #[test]
    fn train_spends_stars_and_embarks_only_between_106_and_115() {
        let mut state = board(VERSION_TRAIN_EMBARK);
        state.map.tiles[1].terrain = 1;
        let trained = train(&mut state, 1, 2, WorldCoordinates { x: 1, y: 0 }, 3, 10, &[]);
        assert!(trained.spawned);
        assert_eq!(trained.unit_id, 1);
        assert!(trained.embarked);
        assert_eq!(state.players[0].currency, 7);
        assert_eq!(state.units[0].health, 10);

        state.version = VERSION_TRAIN_EMBARK + 1;
        state.map.tiles[0].terrain = 1;
        let later = train(&mut state, 1, 2, WorldCoordinates { x: 0, y: 0 }, 3, 10, &[]);
        assert!(later.spawned && !later.embarked);

        state.version = VERSION_TRAIN_V105;
        state.map.tiles[2].terrain = 1;
        let older = train(&mut state, 1, 2, WorldCoordinates { x: 2, y: 0 }, 3, 10, &[]);
        assert!(older.spawned && !older.embarked);

        let broke = train(&mut state, 1, 2, WorldCoordinates { x: 2, y: 0 }, 100, 10, &[]);
        assert!(!broke.spawned);
        assert_eq!(state.players[0].currency, 1);
    }

    #[test]
    fn research_writes_learn_tech_and_currency_replaces() {
        let mut state = board(123);
        let effect = research(&mut state, 1, 4, 3, 5, &[2], &[8]).unwrap();
        assert!(effect.applied);
        assert_eq!(state.players[0].currency, 7);
        assert_eq!(state.tech[0].tech, vec![4]);
        assert!(!increase_currency(&mut state, 1, -1));
        assert_eq!(state.players[0].currency, 7);
        assert!(increase_currency(&mut state, 1, 2));
        assert_eq!(state.players[0].currency, 2);
    }

    #[test]
    fn build_and_level_up_and_capture_splits() {
        let mut state = board(VERSION_BUILD_V105);
        assert!(build(&mut state, 1, 1, WorldCoordinates { x: 0, y: 0 }, true, 4,));
        assert_eq!(state.map.tiles[0].improvement, 1);
        assert_eq!(state.players[0].currency, 6);
        state.version = VERSION_BUILD_V105 + 1;
        assert!(build(&mut state, 1, 5, WorldCoordinates { x: 1, y: 0 }, false, 0,));
        assert_eq!(state.map.tiles[1].improvement, 5);

        state.improvements[0].xp = 2;
        state.improvements[0].level = 1;
        state.improvements[0].upgrade = 3;
        assert!(city_level_up(&mut state, WorldCoordinates { x: 0, y: 0 }));
        assert_eq!(state.improvements[0].level, 2);
        assert_eq!(state.improvements[0].xp, 0);
        assert_eq!(state.improvements[0].upgrade, 2);
        assert!(!city_level_up(&mut state, WorldCoordinates { x: 0, y: 0 }));

        state.version = VERSION_CAPTURE_V59;
        state.map.tiles[0].owner = 2;
        assert!(capture_city(&mut state, 1, WorldCoordinates { x: 0, y: 0 }, 2,));
        assert!(state.diplomacy.is_empty());
        state.version = VERSION_CAPTURE_V59 + 1;
        state.map.tiles[1].owner = 2;
        capture_city(&mut state, 1, WorldCoordinates { x: 1, y: 0 }, 2);
        let relation = &state.diplomacy[0].relations[0].relation;
        assert_eq!(relation.last_attack_turn, 1);
        assert_eq!(state.map.tiles[1].owner, 1);
        assert_eq!(build_road(&state, WorldCoordinates { x: 1, y: 0 }), Some(1));
    }

    #[test]
    fn end_turn_wraps_and_start_turn_splits_at_59() {
        let mut state = board(VERSION_START_V59);
        state.improvements.push(ImprovementRuntime { tile: 0, robbed: true, ..ImprovementRuntime::default() });
        state.map.tiles[0].owner = 1;
        state.diplomacy.push(crate::PlayerDiplomacy {
            player: 1,
            relations: vec![DiplomacyLink {
                other: 2,
                relation: DiplomacyRelation { state: DiplomacyRelationState::CoolDown, ..DiplomacyRelation::default() },
            }],
        });
        state.player_runtime.push(PlayerRuntime { player: 1, known_players: vec![2], ..PlayerRuntime::default() });
        assert!(end_turn(&mut state, 1, |_| true));
        assert_eq!(state.current_player_index, 1);
        assert_eq!(state.current_turn, 1);
        assert!(!state.improvements[0].robbed);
        assert_eq!(state.diplomacy[0].relations[0].relation.state, DiplomacyRelationState::Neutral);
        end_turn(&mut state, 2, |_| true);
        assert_eq!(state.current_player_index, 0);
        assert_eq!(state.current_turn, 2);

        state.version = VERSION_START_V59;
        state.current_turn = 1;
        let own =
            [StartTurnTile { owner: 1, has_improvement: true, improvement_known: true, own_work: 4, foreign_work: 0 }];
        start_turn(&mut state, 1, &own);
        assert_eq!(state.players[0].currency, 4);
        let aggression = state.player_runtime[0].aggressions.iter().find(|entry| entry.other == 2).unwrap();
        assert_eq!(aggression.count, economy::V59_AHEAD_AGGRESSION);

        state.version = VERSION_START_V59 + 1;
        state.players[0].currency = 9;
        let foreign = [StartTurnTile {
            owner: 2,
            has_improvement: false,
            improvement_known: false,
            own_work: 0,
            foreign_work: 7,
        }];
        let before = state.player_runtime[0].aggressions.len();
        start_turn(&mut state, 1, &foreign);
        assert_eq!(state.players[0].currency, 7);
        assert_eq!(state.player_runtime[0].aggressions.len(), before);
    }
}
