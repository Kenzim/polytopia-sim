//! Commands that change the board, plus the actions they run.
//!
//! UI, animation, networking, and serialization are not applied. A callee that
//! stays opaque is an argument; it is named on the function and described in
//! `docs/specs/actions.md`.

use crate::rules::actions::perform_attack;
use crate::rules::combat::{get_battle_results, BattleQuery, CombatUnit};
use crate::rules::economy::{self, learn_tech, remove_score};
use crate::rules::path::{self, is_land};
use crate::{
    Aggression, CityReward, DiplomacyLink, DiplomacyRelation, DiplomacyRelationState, GameState, GameStatus,
    ImprovementRuntime, PlayerRuntime, UnitState, WorldCoordinates,
};

/// Break-peace action: `version <= 79` is the war arm.
pub const VERSION_BREAK_PEACE: i32 = 0x4F;
/// Boost action: `version <= 114` adds Boosted, later Swift.
pub const VERSION_BOOST: i32 = 0x72;
/// Break-ice command.
pub const VERSION_BREAK_ICE_REVEAL: i32 = 0x65;
/// Break-ice action.
pub const VERSION_BREAK_ICE_V98: i32 = 0x62;
pub const VERSION_BREAK_ICE_V105: i32 = 0x69;
/// Poison-unit action.
pub const VERSION_POISON: i32 = 0x65;
/// Heal action.
pub const VERSION_HEAL_V81: i32 = 0x51;
pub const VERSION_HEAL_V101: i32 = 0x66;
/// Disband-unit action.
pub const VERSION_DISBAND_V59: i32 = 0x3B;
pub const VERSION_DISBAND_V105: i32 = 0x69;
/// Disembark action.
pub const VERSION_DISEMBARK_V97: i32 = 0x62;
pub const VERSION_DISEMBARK_V108: i32 = 0x6C;
/// Establish-embassy command and meet action.
pub const VERSION_EMBASSY_V105: i32 = 0x69;
pub const VERSION_MEET: i32 = 0x70;
/// Explode-unit action.
pub const VERSION_EXPLODE_V82: i32 = 0x52;
pub const VERSION_EXPLODE_V94: i32 = 0x5E;
pub const VERSION_EXPLODE_V120: i32 = 0x78;
/// Resign command.
pub const VERSION_RESIGN_V91: i32 = 0x5B;
pub const VERSION_RESIGN_V92: i32 = 0x5C;
/// Start-match action.
pub const VERSION_START_MATCH: i32 = 0x63;
/// Upgrade action.
pub const VERSION_UPGRADE_V97: i32 = 0x62;
pub const VERSION_UPGRADE_V108: i32 = 0x6C;
pub const VERSION_UPGRADE_V115: i32 = 0x72;
/// The Cute skin. Poison's default arm adds Charmed for it.
const SKIN_CUTE: i32 = 0x13;
/// The Swamp skin. Flooding a tile also adds Swamped.
const SKIN_SWAMP: i32 = 0x11;
/// Tile effect types.
const EFFECT_FLOODED: i32 = 1;
const EFFECT_SWAMPED: i32 = 2;
const EFFECT_ALGAE: i32 = 4;
const EFFECT_FOAM: i32 = 5;
/// The Water and Ocean terrain types.
const TERRAIN_WATER: i32 = 1;
const TERRAIN_OCEAN: i32 = 2;
const TERRAIN_ICE: i32 = 6;
/// Aggression written by the meet action (-1000).
const MEET_AGGRESSION: i32 = -1000;
/// Game settings kind that pushes an increase-currency action of amount 6.
const START_MATCH_STARS_KIND: i32 = 7;
const START_MATCH_STARS: i32 = 6;

/// The boost command, then the boost action.
///
/// The command sets moved and attacked on the unit at `coordinates`.
/// `boost_targets` is the unit's boost options, including that tile when
/// the option list contains it. `version <= 114` sets Boosted. Later versions
/// set Swift.
pub fn boost(state: &mut GameState, coordinates: WorldCoordinates, boost_targets: &[WorldCoordinates]) -> bool {
    let _ = spend_unit_at(state, coordinates);
    let swift = state.version > VERSION_BOOST;
    let mut applied = false;
    for target in boost_targets {
        if let Some(unit) = unit_at_mut(state, *target) {
            if swift {
                unit.swift = true;
            } else {
                unit.boosted = true;
            }
            applied = true;
        }
    }
    applied || unit_at(state, coordinates).is_some()
}

/// The swarm command spends the unit and runs the same boost
/// action the boost command queues.
pub fn swarm(state: &mut GameState, coordinates: WorldCoordinates, boost_targets: &[WorldCoordinates]) -> bool {
    boost(state, coordinates, boost_targets)
}

/// The stay command sets moved and attacked.
pub fn stay(state: &mut GameState, coordinates: WorldCoordinates) -> bool {
    spend_unit_at(state, coordinates)
}

/// The hide command and action.
/// Attacked is set, then the Invisible effect is added.
pub fn hide(state: &mut GameState, coordinates: WorldCoordinates) -> bool {
    let Some(unit) = unit_at_mut(state, coordinates) else {
        return false;
    };
    unit.attacked = true;
    unit.invisible = true;
    true
}

/// The recover command runs the recover action and the heal action.
///
/// `heal_amount` is the computed heal amount when the action's fixed
/// amount is 0. `max_health` is the unit's maximum health.
pub fn recover(state: &mut GameState, coordinates: WorldCoordinates, heal_amount: u16, max_health: u16) -> bool {
    if !spend_unit_at(state, coordinates) {
        return false;
    }
    heal_unit(state, coordinates, heal_amount, max_health)
}

/// The heal action.
///
/// `<= 81` is V81: a poisoned unit is cured and not healed, and
/// a heal is not clamped. `<= 102` is V101: same cure, then
/// `min(health + amount, max_health)`. Default also clears
/// Charmed on the unit and its passenger.
pub fn heal_unit(state: &mut GameState, coordinates: WorldCoordinates, heal_amount: u16, max_health: u16) -> bool {
    let version = state.version;
    let Some(index) = unit_index_at(state, coordinates) else {
        return false;
    };
    if state.units[index].poisoned {
        state.units[index].poisoned = false;
        if version > VERSION_HEAL_V101 {
            state.units[index].charmed = false;
            if let Some(passenger) = state.units[index].passenger.as_mut() {
                passenger.poisoned = false;
                passenger.charmed = false;
            }
        }
        return true;
    }
    let updated = state.units[index].health.wrapping_add(heal_amount);
    state.units[index].health = if version <= VERSION_HEAL_V81 || updated < max_health { updated } else { max_health };
    true
}

/// The poison-unit action.
///
/// `<= 101` only sets Poisoned on the unit at `target`. Default also sets
/// Charmed when the acting player's skin is Cute (`0x13`) and poisons a passenger.
pub fn poison_unit(state: &mut GameState, player_id: u8, target: WorldCoordinates) -> bool {
    let version = state.version;
    let skin = player_skin(state, player_id);
    let Some(index) = unit_index_at(state, target) else {
        return false;
    };
    state.units[index].poisoned = true;
    if version <= VERSION_POISON {
        return true;
    }
    if skin == SKIN_CUTE {
        state.units[index].charmed = true;
    }
    if let Some(passenger) = state.units[index].passenger.as_mut() {
        passenger.poisoned = true;
    }
    true
}

/// The promote action.
///
/// The versioned and default bodies match. Promotion increments on the passenger when one is
/// present, otherwise on the unit. Health becomes `max_health`
/// on the unit that occupies the tile.
pub fn promote(state: &mut GameState, coordinates: WorldCoordinates, max_health: u16) -> bool {
    let Some(index) = unit_index_at(state, coordinates) else {
        return false;
    };
    if state.units[index].passenger.is_some() {
        let passenger = state.units[index].passenger.as_mut().expect("passenger");
        passenger.promotion = passenger.promotion.wrapping_add(1);
    } else {
        state.units[index].promotion = state.units[index].promotion.wrapping_add(1);
    }
    state.units[index].health = max_health;
    true
}

/// The break-ice command.
///
/// Both arms spend the unit and run the break-ice area action at radius 1.
/// `should_break` is whether the area has breakable ice. `version > 101` also
/// queues a reveal action when the unit is invisible; that popup
/// is not drawn. `revealed` reports the queue.
///
/// Break-ice action: `<= 98` converts without a second check,
/// `<= 105` converts when `should_break` is true, and Default also requires
/// `blocked_by_invisible` to be false (an invisible unit blocks the action).
/// The tile becomes Ocean, or Water when an orthogonal neighbour is land that is
/// not ice. The ice bank level update and shore line generation are not applied
/// here; the engine's own break-ice command does both.
pub fn break_ice<F>(
    state: &mut GameState,
    coordinates: WorldCoordinates,
    mut should_break: F,
    blocked_by_invisible: bool,
) -> bool
where
    F: FnMut(&GameState, WorldCoordinates) -> bool,
{
    let version = state.version;
    let invisible = unit_at(state, coordinates).is_some_and(|unit| unit.invisible);
    let _ = spend_unit_at(state, coordinates);
    let mut tiles = vec![coordinates];
    tiles.extend(path::area(coordinates, 1));
    let mut broke = false;
    for tile in tiles {
        let selected = should_break(state, tile);
        #[allow(clippy::if_same_then_else)] // one arm per game-version window
        let convert = if version <= VERSION_BREAK_ICE_V98 {
            selected
        } else if version <= VERSION_BREAK_ICE_V105 {
            selected
        } else {
            selected && !blocked_by_invisible
        };
        if convert {
            convert_broken_ice(state, tile);
            broke = true;
        }
    }
    broke || (version > VERSION_BREAK_ICE_REVEAL && invisible)
}

fn convert_broken_ice(state: &mut GameState, coordinates: WorldCoordinates) {
    let shallow = orthogonal(coordinates).into_iter().any(|neighbor| {
        tile_mut(state, neighbor).is_some_and(|tile| is_land(tile.terrain) && tile.terrain != TERRAIN_ICE)
    });
    if let Some(tile) = tile_mut(state, coordinates) {
        tile.terrain = if shallow { TERRAIN_WATER } else { TERRAIN_OCEAN };
    }
}

/// The break-peace command and action.
///
/// `version <= 79`: the actor's relation becomes War and
/// the attack-based relation update records the turn; the
/// opponent's relation state becomes Neutral. Later versions set both
/// relations to CoolDown and store `current_turn` in `last_peace_broken_turn`.
/// Every unit of `player_id` that is not standing in `opponent_id`'s territory
/// is spent. Each unit inside that territory queues a disband-unit action with
/// `get_paid = version <= 110`; their coordinates are returned, in unit
/// order, for the caller to disband.
pub fn break_peace(state: &mut GameState, player_id: u8, opponent_id: u8) -> Vec<WorldCoordinates> {
    let version = state.version;
    let turn = state.current_turn as i32;
    if version <= VERSION_BREAK_PEACE {
        relation_mut(state, player_id, opponent_id).state = DiplomacyRelationState::War;
        note_attack(state, player_id, opponent_id);
        relation_mut(state, opponent_id, player_id).state = DiplomacyRelationState::Neutral;
    } else {
        let actor = relation_mut(state, player_id, opponent_id);
        actor.state = DiplomacyRelationState::CoolDown;
        actor.last_peace_broken_turn = turn;
        let other = relation_mut(state, opponent_id, player_id);
        other.last_peace_broken_turn = turn;
        other.state = DiplomacyRelationState::CoolDown;
    }
    let coords: Vec<WorldCoordinates> =
        state.units.iter().filter(|unit| unit.owner == player_id).map(|unit| unit.coordinates).collect();
    let mut disband = Vec::new();
    for coordinates in coords {
        let inside = tile_index(state, coordinates).is_some_and(|i| state.map.tiles[i].owner == opponent_id);
        if inside {
            disband.push(coordinates);
        } else {
            let _ = spend_unit_at(state, coordinates);
        }
    }
    disband
}

/// The destroy-embassy command and action.
///
/// Takes the relation of `opponent` toward `player` and sets its embassy level to 0.
pub fn destroy_embassy(state: &mut GameState, player_id: u8, opponent_id: u8) -> bool {
    relation_mut(state, opponent_id, player_id).embassy_level = 0;
    true
}

/// The establish-embassy command.
///
/// `embassy_cost` is the embassy cost for the player. Currency becomes
/// `currency - cost`; a negative result
/// leaves the board unchanged. The opponent's relation to the actor stores
/// embassy level 1 and `embassy_build_turn = current_turn`.
///
/// This is the establish-embassy action part. For `version > 105`
/// the default body then queues a meet action (opponent, player):
/// the opponent meets the builder. That meet is left to the caller.
pub fn establish_embassy(state: &mut GameState, player_id: u8, opponent_id: u8, embassy_cost: i32) -> bool {
    if !add_currency(state, player_id, embassy_cost.wrapping_neg()) {
        return false;
    }
    let turn = state.current_turn as i32;
    let relation = relation_mut(state, opponent_id, player_id);
    relation.embassy_level = 1;
    relation.embassy_build_turn = turn;
    true
}

/// The meet action for `version <= 112`: `MEET_AGGRESSION`
/// (`-1000`) on both players unless `game_type == 2`.
pub fn meet_aggression(state: &mut GameState, player_id: u8, opponent_id: u8, game_type: i32) {
    if state.version <= VERSION_MEET && game_type != 2 {
        add_aggression(state, player_id, opponent_id, MEET_AGGRESSION);
        add_aggression(state, opponent_id, player_id, MEET_AGGRESSION);
    }
}

/// The peace-treaty command only queues
/// a message with message type 1. No diplomacy field is written.
pub fn peace_treaty(state: &GameState, opponent_id: u8) -> bool {
    state.players.iter().any(|player| player.id == opponent_id)
}

/// The peace-request response action.
///
/// Accepted requests set both relations to Peace. A refusal leaves them.
/// The route update is not applied.
pub fn peace_response(state: &mut GameState, player_id: u8, opponent_id: u8, accepted: bool) -> bool {
    if !accepted {
        return false;
    }
    relation_mut(state, player_id, opponent_id).state = DiplomacyRelationState::Peace;
    relation_mut(state, opponent_id, player_id).state = DiplomacyRelationState::Peace;
    true
}

/// The decompose command and action add the decomposing effect to the improvement.
pub fn decompose(state: &mut GameState, coordinates: WorldCoordinates) -> bool {
    let Some(tile) = tile_index(state, coordinates) else {
        return false;
    };
    let Some(runtime) = improvement_mut(state, tile) else {
        return false;
    };
    runtime.decomposing = true;
    true
}

/// The destroy command and destroy-improvement action clear the tile improvement.
/// Score removal inside that body is unit-score-style data and is passed
/// as `score_delta` into [`remove_score`].
pub fn destroy(state: &mut GameState, player_id: u8, coordinates: WorldCoordinates, score_delta: i32) -> bool {
    let Some(index) = tile_index(state, coordinates) else {
        return false;
    };
    state.map.tiles[index].improvement = 0;
    state.improvements.retain(|runtime| runtime.tile != index as i32);
    if let Some(player) = state.players.iter_mut().find(|player| player.id == player_id) {
        player.score = remove_score(player.score, score_delta);
    }
    true
}

/// The disband command.
///
/// `full_cost` is the unit's full cost. When it is at least 2 the refund
/// is that value divided by 2 toward zero, added once per star by
/// the increase-currency action (amount 1). `version > 105` skips the
/// refund when `get_paid` is false. A unit whose owner is not `player_id` is
/// removed without a refund. `unit_score` is the unit's score.
/// A leader's `follower` link is cleared.
pub fn disband(
    state: &mut GameState,
    player_id: u8,
    coordinates: WorldCoordinates,
    full_cost: i32,
    unit_score: i32,
    get_paid: bool,
) -> bool {
    let Some(index) = unit_index_at(state, coordinates) else {
        return false;
    };
    let owner = state.units[index].owner;
    let leader = state.units[index].leader;
    if leader != 0 {
        if let Some(leader_unit) = state.units.iter_mut().find(|unit| unit.id == leader) {
            leader_unit.follower = 0;
        }
    }
    let pay = owner == player_id && (state.version <= VERSION_DISBAND_V105 || get_paid);
    if pay && full_cost >= 2 {
        let half = full_cost.wrapping_add(full_cost >> 31) >> 1;
        if !add_currency(state, player_id, half) {
            return false;
        }
    }
    if owner == player_id {
        if let Some(player) = state.players.iter_mut().find(|player| player.id == player_id) {
            player.score = remove_score(player.score, unit_score);
        }
    }
    let index = unit_index_at(state, coordinates).expect("unit still present");
    state.units.remove(index);
    let _ = VERSION_DISBAND_V59;
    true
}

/// The disembark action.
///
/// `<= 97`, `<= 108`, and Default all copy the carrier's health onto the
/// passenger and put the passenger onto the tile. Direction and flipped
/// are not fields on [`UnitState`].
pub fn disembark(state: &mut GameState, coordinates: WorldCoordinates) -> bool {
    let _ = state.version;
    let Some(index) = unit_index_at(state, coordinates) else {
        return false;
    };
    let carrier = state.units.remove(index);
    let Some(mut passenger) = carrier.passenger else {
        state.units.insert(index, carrier);
        return false;
    };
    passenger.health = carrier.health;
    passenger.moved = true;
    passenger.attacked = true;
    passenger.coordinates = coordinates;
    state.units.insert(index, *passenger);
    true
}

/// One neighbor damaged by the explode-unit action.
pub struct ExplodeTarget {
    pub coordinates: WorldCoordinates,
    pub defender: CombatUnit,
    pub peaceful: bool,
}

/// The explode command and explode-unit action.
///
/// `<= 82`, `<= 94`, `<= 120`, and Default all work out the battle results
/// ([`crate::rules::combat::get_battle_results`]). When `attack_damage` is below the defender's health they
/// construct a poison-unit action and then an attack action.
/// A leader's follower id is cleared. Peace ([`crate::rules::combat::has_active_peace_treaty`])
/// skips that neighbor.
pub fn explode(
    state: &mut GameState,
    player_id: u8,
    origin: WorldCoordinates,
    attacker: &CombatUnit,
    targets: &[ExplodeTarget],
    query: &BattleQuery,
    attacker_abilities: &[i32],
) -> bool {
    let _ = (state.version, VERSION_EXPLODE_V82, VERSION_EXPLODE_V94, VERSION_EXPLODE_V120);
    if let Some(index) = unit_index_at(state, origin) {
        let leader = state.units[index].leader;
        if leader != 0 {
            if let Some(leader_unit) = state.units.iter_mut().find(|unit| unit.id == leader) {
                leader_unit.follower = 0;
            }
        }
    }
    for target in targets {
        if target.peaceful {
            continue;
        }
        let results = get_battle_results(attacker, &target.defender, query);
        let health = unit_at(state, target.coordinates).map(|unit| i32::from(unit.health));
        if health.is_some_and(|health| results.attack_damage < health) {
            let _ = poison_unit(state, player_id, target.coordinates);
        }
        let _ = perform_attack(
            state,
            player_id,
            origin,
            target.coordinates,
            results.attack_damage,
            attacker_abilities,
            false,
        );
    }
    true
}

/// The flood command spends the unit and floods the tile.
///
/// Effects are not stored on [`Tile`](crate::Tile). The returned ids are what adding
/// the effects would append: Flooded, plus Swamped when the player's skin is Swamp (`0x11`).
pub fn flood(state: &mut GameState, player_id: u8, coordinates: WorldCoordinates) -> Vec<i32> {
    let _ = spend_unit_at(state, coordinates);
    if tile_index(state, coordinates).is_none() {
        return Vec::new();
    }
    let mut effects = vec![EFFECT_FLOODED];
    if player_skin(state, player_id) == SKIN_SWAMP {
        effects.push(EFFECT_SWAMPED);
    }
    effects
}

/// The clear-tile-effect action removes an effect.
/// Algae (`4`) also removes Foam (`5`). Effects are not stored on [`Tile`](crate::Tile);
/// the returned ids are the ones removed. The city connection update is not applied.
pub fn clear_tile_effect(effect: i32) -> Vec<i32> {
    let mut removed = vec![effect];
    if effect == EFFECT_ALGAE {
        removed.push(EFFECT_FOAM);
    }
    removed
}

/// The freeze-area command spends the unit and queues
/// a freeze-area action with radius 1 that also freezes units.
///
/// `freezable` is whether the tile can be frozen. A freezable tile's terrain
/// becomes Ice. Units in the Chebyshev radius are frozen.
pub fn freeze_area<F>(state: &mut GameState, coordinates: WorldCoordinates, mut freezable: F) -> bool
where
    F: FnMut(&GameState, WorldCoordinates) -> bool,
{
    let _ = spend_unit_at(state, coordinates);
    let mut tiles = vec![coordinates];
    tiles.extend(path::area(coordinates, 1));
    for tile in &tiles {
        if freezable(state, *tile) {
            if let Some(tile_mut) = tile_at_mut(state, *tile) {
                tile_mut.terrain = TERRAIN_ICE;
            }
        }
        if let Some(unit) = unit_at_mut(state, *tile) {
            unit.frozen = true;
        }
    }
    true
}

/// The city-reward command pops the pending trigger and runs
/// the city-reward action, which adds the reward to the city.
///
/// `production`, `population`, `currency`, and `score` are the deltas of
/// the reward production modifier and the reward constants the
/// action applies. [`train_on_occupied`] runs when
/// `trained` is set.
pub fn city_reward(
    state: &mut GameState,
    player_id: u8,
    coordinates: WorldCoordinates,
    reward: CityReward,
    production: i32,
    population: i16,
    currency: i32,
    score: i32,
    trained: Option<TrainSpawn>,
) -> bool {
    let Some(index) = tile_index(state, coordinates) else {
        return false;
    };
    let Some(runtime) = improvement_mut(state, index) else {
        return false;
    };
    runtime.rewards.push(reward);
    if production > 0 {
        runtime.production = runtime.production.wrapping_add(production as u16);
    }
    if population != 0 {
        runtime.population = runtime.population.wrapping_add(population);
    }
    let _ = add_currency(state, player_id, currency);
    if let Some(player) = state.players.iter_mut().find(|player| player.id == player_id) {
        player.score = economy::add_score(player.score, score);
    }
    if let Some(spawn) = trained {
        let _ = train_on_occupied(state, player_id, coordinates, spawn);
    }
    true
}

/// Stats for a unit [`train_on_occupied`] places.
/// `push_to` is the tile the default push chose. `None`
/// means the push failed and an occupied tile does not spawn.
#[derive(Clone, Copy, Debug)]
pub struct TrainSpawn {
    pub type_idx: i32,
    pub health: u16,
    pub push_to: Option<WorldCoordinates>,
}

/// Train a unit onto an occupied space.
pub fn train_on_occupied(
    state: &mut GameState,
    player_id: u8,
    coordinates: WorldCoordinates,
    spawn: TrainSpawn,
) -> bool {
    if unit_at(state, coordinates).is_some() {
        let Some(destination) = spawn.push_to else {
            return false;
        };
        if let Some(unit) = unit_at_mut(state, coordinates) {
            unit.coordinates = destination;
            unit.moved = true;
            unit.attacked = true;
        }
    }
    state.current_unit_id = state.current_unit_id.wrapping_add(1);
    let id = state.current_unit_id;
    let created = state.current_turn as u16;
    state.units.push(UnitState {
        id,
        owner: player_id,
        type_idx: spawn.type_idx,
        coordinates,
        home: coordinates,
        health: spawn.health,
        created_turn: created,
        ..UnitState::default()
    });
    true
}

/// The examine-ruins command.
///
/// `version <= 119` uses the V119 ruins reward; later
/// versions use the default ruins reward. The reward id is the
/// caller's result from that function. The ruin flag is cleared.
/// `reward_currency` is added. A unit reward uses [`train_on_occupied`].
pub fn examine_ruins(
    state: &mut GameState,
    player_id: u8,
    coordinates: WorldCoordinates,
    reward_currency: i32,
    trained: Option<TrainSpawn>,
) -> bool {
    let _ = state.version;
    let Some(index) = tile_index(state, coordinates) else {
        return false;
    };
    state.map.tiles[index].has_ruin = false;
    let _ = add_currency(state, player_id, reward_currency);
    if let Some(spawn) = trained {
        let _ = train_on_occupied(state, player_id, coordinates, spawn);
    }
    true
}

/// Which ruins reward arm the examine-ruins command selected.
pub fn ruins_reward_is_v119(version: i32) -> bool {
    version <= 0x77
}

/// The end-match command stores the Ended state (`4`).
pub fn end_match(state: &mut GameState) {
    state.current_state = GameStatus::Ended;
}

/// The game-over action stores `FinalTurn` (`3`) after the
/// log. The log is not written.
pub fn game_over(state: &mut GameState) {
    state.current_state = GameStatus::FinalTurn;
}

/// The resign command.
///
/// `<= 91`, `== 92`, and Default all set `auto_play` and
/// `resigned_at_command` from the command index minus one. `game_type` 1 or 4
/// sets handicap to 1.
pub fn resign(state: &mut GameState, player_id: u8, game_type: i32) -> bool {
    let _ = (VERSION_RESIGN_V91, VERSION_RESIGN_V92, state.version);
    let command = i32::from(state.current_command).wrapping_sub(1);
    let Some(player) = state.players.iter_mut().find(|player| player.id == player_id) else {
        return false;
    };
    player.auto_play = true;
    if game_type == 1 || game_type == 4 {
        player.handicap = 1;
    }
    runtime_mut(state, player_id).resigned_at_command = command;
    true
}

/// The start-match command and action.
///
/// Both arms set the current state to Started (`2`) and learn each
/// `(player, tech, data_cost)` through [`learn_tech`] at price 0.
/// `settings_kind == 7` adds 6 currency per player (increase-currency action).
/// Exactly version 99 also reaches the task check for lighthouses; that task
/// write is reported and not applied. `version > 99` looks up the first discovered
/// lighthouse tile, passed as `lighthouse`.
pub fn start_match(
    state: &mut GameState,
    settings_kind: i32,
    techs: &[(u8, i32, i32)],
    lighthouse: Option<WorldCoordinates>,
) -> bool {
    for (player_id, tech, data_cost) in techs {
        let Some(player) = state.players.iter().find(|player| player.id == *player_id).copied() else {
            continue;
        };
        let unlocked =
            state.tech.iter().find(|row| row.player == *player_id).map(|row| row.tech.clone()).unwrap_or_default();
        let effect = learn_tech(&player, &unlocked, *tech, 0, *data_cost, &[], &[]);
        if effect.applied {
            if let Some(row) = state.tech.iter_mut().find(|row| row.player == *player_id) {
                row.tech = effect.unlocked;
            } else {
                state.tech.push(crate::OwnedTech { player: *player_id, tech: effect.unlocked });
            }
            if let Some(player) = state.players.iter_mut().find(|player| player.id == *player_id) {
                player.currency = effect.currency;
                player.score = effect.score;
            }
        }
    }
    if settings_kind == START_MATCH_STARS_KIND {
        let ids: Vec<u8> = state.players.iter().map(|player| player.id).collect();
        for id in ids {
            let _ = add_currency(state, id, START_MATCH_STARS);
        }
    }
    state.current_state = GameStatus::Started;
    let _ = lighthouse;
    state.version == VERSION_START_MATCH
}

/// The upgrade command and action.
///
/// `<= 98`, `<= 108`, `<= 114`, and Default subtract `cost` from the
/// currency and replace the unit's type. Health, home, passenger, and
/// the spent flags are copied from the unit that was there. A negative
/// currency result leaves the board unchanged.
pub fn upgrade(state: &mut GameState, player_id: u8, coordinates: WorldCoordinates, type_idx: i32, cost: i32) -> bool {
    let _ = (VERSION_UPGRADE_V97, VERSION_UPGRADE_V108, VERSION_UPGRADE_V115, state.version);
    if !add_currency(state, player_id, cost.wrapping_neg()) {
        return false;
    }
    let Some(unit) = unit_at_mut(state, coordinates) else {
        return false;
    };
    unit.type_idx = type_idx;
    true
}

/// End-turn is already in `actions`. This records the diplomacy check
/// only as the version gate it uses: at `version >= 82` a missing sender is
/// dropped. Messages are not stored on [`GameState`], so the body does not
/// write a row. `sender_alive` is [`crate::rules::diplomacy::is_alive`].
pub fn check_diplomacy_keeps_message(version: i32, sender_alive: bool) -> bool {
    version < 0x52 || sender_alive
}

fn spend_unit_at(state: &mut GameState, coordinates: WorldCoordinates) -> bool {
    let Some(unit) = unit_at_mut(state, coordinates) else {
        return false;
    };
    unit.moved = true;
    unit.attacked = true;
    true
}

fn add_currency(state: &mut GameState, player_id: u8, amount: i32) -> bool {
    let Some(player) = state.players.iter_mut().find(|player| player.id == player_id) else {
        return false;
    };
    let next = player.currency.wrapping_add(amount);
    if next < 0 {
        return false;
    }
    player.currency = next;
    true
}

fn player_skin(state: &GameState, player_id: u8) -> i32 {
    state.player_runtime.iter().find(|runtime| runtime.player == player_id).map(|runtime| runtime.skin).unwrap_or(0)
}

fn tile_index(state: &GameState, coordinates: WorldCoordinates) -> Option<usize> {
    state.map.tiles.iter().position(|tile| tile.coordinates == coordinates)
}

fn tile_mut(state: &mut GameState, coordinates: WorldCoordinates) -> Option<&mut crate::Tile> {
    state.map.tiles.iter_mut().find(|tile| tile.coordinates == coordinates)
}

fn tile_at_mut(state: &mut GameState, coordinates: WorldCoordinates) -> Option<&mut crate::Tile> {
    tile_mut(state, coordinates)
}

fn improvement_mut(state: &mut GameState, tile: usize) -> Option<&mut ImprovementRuntime> {
    let tile = tile as i32;
    state.improvements.iter_mut().find(|runtime| runtime.tile == tile)
}

fn unit_index_at(state: &GameState, coordinates: WorldCoordinates) -> Option<usize> {
    state.units.iter().position(|unit| unit.coordinates == coordinates)
}

fn unit_at(state: &GameState, coordinates: WorldCoordinates) -> Option<&UnitState> {
    state.units.iter().find(|unit| unit.coordinates == coordinates)
}

fn unit_at_mut(state: &mut GameState, coordinates: WorldCoordinates) -> Option<&mut UnitState> {
    state.units.iter_mut().find(|unit| unit.coordinates == coordinates)
}

fn orthogonal(center: WorldCoordinates) -> [WorldCoordinates; 4] {
    [
        WorldCoordinates { x: center.x.wrapping_sub(1), y: center.y },
        WorldCoordinates { x: center.x.wrapping_add(1), y: center.y },
        WorldCoordinates { x: center.x, y: center.y.wrapping_sub(1) },
        WorldCoordinates { x: center.x, y: center.y.wrapping_add(1) },
    ]
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

fn add_aggression(state: &mut GameState, player: u8, other: u8, delta: i32) {
    let runtime = runtime_mut(state, player);
    if let Some(entry) = runtime.aggressions.iter_mut().find(|entry| entry.other == other) {
        entry.count = entry.count.wrapping_add(delta);
    } else {
        runtime.aggressions.push(Aggression { other, count: delta });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rules::combat::{BattleQuery, CombatUnit};
    use crate::{Map, PlayerState};

    fn player(id: u8, currency: i32) -> PlayerState {
        PlayerState {
            id,
            handicap: 1,
            currency,
            score: 0,
            tribe: 1,
            auto_play: false,
            start_tile: WorldCoordinates { x: 0, y: 0 },
        }
    }

    fn board(version: i32) -> GameState {
        let mut state = GameState::new(version, 1);
        state.map = Map::new(3, 3);
        state.players.push(player(1, 10));
        state.players.push(player(2, 4));
        state.current_turn = 3;
        state.current_command = 8;
        state
    }

    fn unit_at(_state: &GameState, x: i32, y: i32) -> UnitState {
        UnitState { id: 1, owner: 1, coordinates: WorldCoordinates { x, y }, health: 10, ..UnitState::default() }
    }

    #[test]
    fn boost_uses_boosted_then_swift() {
        let mut state = board(VERSION_BOOST);
        state.units.push(unit_at(&state, 1, 1));
        assert!(boost(&mut state, WorldCoordinates { x: 1, y: 1 }, &[WorldCoordinates { x: 1, y: 1 }],));
        assert!(state.units[0].moved && state.units[0].attacked);
        assert!(state.units[0].boosted);
        assert!(!state.units[0].swift);

        state.version = VERSION_BOOST + 1;
        state.units[0].boosted = false;
        assert!(boost(&mut state, WorldCoordinates { x: 1, y: 1 }, &[WorldCoordinates { x: 1, y: 1 }],));
        assert!(state.units[0].swift);
        assert!(!state.units[0].boosted);
    }

    #[test]
    fn break_peace_war_then_cooldown() {
        let mut state = board(VERSION_BREAK_PEACE);
        state.units.push(unit_at(&state, 0, 0));
        assert!(break_peace(&mut state, 1, 2).is_empty());
        let actor = relation_between(&state, 1, 2);
        let other = relation_between(&state, 2, 1);
        assert_eq!(actor.state, DiplomacyRelationState::War);
        assert_eq!(other.state, DiplomacyRelationState::Neutral);
        assert_eq!(actor.last_attack_turn, 3);
        assert!(state.units[0].moved);

        let mut state = board(VERSION_BREAK_PEACE + 1);
        break_peace(&mut state, 1, 2);
        assert_eq!(relation_between(&state, 1, 2).state, DiplomacyRelationState::CoolDown);
        assert_eq!(relation_between(&state, 2, 1).state, DiplomacyRelationState::CoolDown);
        assert_eq!(relation_between(&state, 1, 2).last_peace_broken_turn, 3);
    }

    #[test]
    fn break_ice_converts_ocean_and_water_on_both_arms() {
        let mut state = board(VERSION_BREAK_ICE_V98);
        state.map.tiles[4].terrain = TERRAIN_ICE;
        state.map.tiles[5].terrain = 3;
        state.map.tiles[3].terrain = TERRAIN_ICE;
        state.units.push(unit_at(&state, 1, 1));
        assert!(break_ice(
            &mut state,
            WorldCoordinates { x: 1, y: 1 },
            |_, coords| coords == (WorldCoordinates { x: 1, y: 1 }),
            false,
        ));
        assert_eq!(state.map.tiles[4].terrain, TERRAIN_WATER);
        assert_eq!(state.map.tiles[5].terrain, 3);
        assert_eq!(state.map.tiles[3].terrain, TERRAIN_ICE);

        let mut state = board(VERSION_BREAK_ICE_V105 + 1);
        state.map.tiles[4].terrain = TERRAIN_ICE;
        state.units.push(UnitState { invisible: true, ..unit_at(&state, 1, 1) });
        let _ = break_ice(&mut state, WorldCoordinates { x: 1, y: 1 }, |_, _| true, true);
        assert_eq!(state.map.tiles[4].terrain, TERRAIN_ICE);
        assert!(break_ice(&mut state, WorldCoordinates { x: 1, y: 1 }, |_, _| true, false,));
        assert_eq!(state.map.tiles[4].terrain, TERRAIN_OCEAN);
    }

    #[test]
    fn heal_cure_and_clamp_follow_the_version_arms() {
        let mut state = board(VERSION_HEAL_V81);
        state.units.push(UnitState { poisoned: true, health: 4, ..unit_at(&state, 0, 0) });
        assert!(heal_unit(&mut state, WorldCoordinates { x: 0, y: 0 }, 5, 10));
        assert!(!state.units[0].poisoned);
        assert_eq!(state.units[0].health, 4);

        state.units[0].health = 8;
        assert!(heal_unit(&mut state, WorldCoordinates { x: 0, y: 0 }, 5, 10));
        assert_eq!(state.units[0].health, 13);

        state.version = VERSION_HEAL_V101;
        state.units[0].health = 8;
        assert!(heal_unit(&mut state, WorldCoordinates { x: 0, y: 0 }, 5, 10));
        assert_eq!(state.units[0].health, 10);

        state.version = VERSION_HEAL_V101 + 1;
        state.units[0].poisoned = true;
        state.units[0].charmed = true;
        state.units[0].passenger = Some(Box::new(UnitState { poisoned: true, charmed: true, ..UnitState::default() }));
        assert!(heal_unit(&mut state, WorldCoordinates { x: 0, y: 0 }, 5, 10));
        assert!(!state.units[0].charmed);
        assert!(!state.units[0].passenger.as_ref().unwrap().charmed);
    }

    #[test]
    fn poison_charms_cute_only_after_101() {
        let mut state = board(VERSION_POISON);
        state.player_runtime.push(crate::PlayerRuntime {
            player: 1,
            skin: SKIN_CUTE,
            ..crate::PlayerRuntime::default()
        });
        state.units.push(UnitState { passenger: Some(Box::new(UnitState::default())), ..unit_at(&state, 0, 0) });
        state.units[0].owner = 2;
        assert!(poison_unit(&mut state, 1, WorldCoordinates { x: 0, y: 0 }));
        assert!(state.units[0].poisoned);
        assert!(!state.units[0].charmed);

        state.version = VERSION_POISON + 1;
        state.units[0].poisoned = false;
        assert!(poison_unit(&mut state, 1, WorldCoordinates { x: 0, y: 0 }));
        assert!(state.units[0].charmed);
        assert!(state.units[0].passenger.as_ref().unwrap().poisoned);
    }

    #[test]
    fn embassy_sets_level_without_meeting_and_meet_aggression_stops_after_112() {
        let mut state = board(VERSION_EMBASSY_V105 + 1);
        assert!(establish_embassy(&mut state, 1, 2, 3));
        assert_eq!(state.players[0].currency, 7);
        assert_eq!(relation_between(&state, 2, 1).embassy_level, 1);
        assert_eq!(relation_between(&state, 2, 1).embassy_build_turn, 3);
        assert!(state.diplomacy.iter().find(|row| row.player == 1).is_none_or(|row| row.relations.is_empty()));
        assert!(!establish_embassy(&mut state, 1, 2, 30));

        let mut state = board(VERSION_MEET);
        meet_aggression(&mut state, 1, 2, 0);
        assert_eq!(
            state.player_runtime.iter().find(|runtime| runtime.player == 1).unwrap().aggressions[0].count,
            MEET_AGGRESSION
        );

        let mut state = board(VERSION_MEET + 1);
        meet_aggression(&mut state, 1, 2, 0);
        assert!(state
            .player_runtime
            .iter()
            .find(|runtime| runtime.player == 1)
            .is_none_or(|runtime| runtime.aggressions.is_empty()));
    }

    #[test]
    fn disband_refunds_half_and_respects_get_paid_after_105() {
        let mut state = board(VERSION_DISBAND_V105);
        state.units.push(unit_at(&state, 0, 0));
        assert!(disband(&mut state, 1, WorldCoordinates { x: 0, y: 0 }, 5, 0, false));
        assert!(state.units.is_empty());
        assert_eq!(state.players[0].currency, 12);

        let mut state = board(VERSION_DISBAND_V105 + 1);
        state.units.push(unit_at(&state, 0, 0));
        assert!(disband(&mut state, 1, WorldCoordinates { x: 0, y: 0 }, 5, 0, false));
        assert_eq!(state.players[0].currency, 10);
        state.units.push(unit_at(&state, 1, 0));
        assert!(disband(&mut state, 1, WorldCoordinates { x: 1, y: 0 }, 5, 0, true));
        assert_eq!(state.players[0].currency, 12);
    }

    #[test]
    fn disembark_replaces_carrier_on_every_arm() {
        for version in [VERSION_DISEMBARK_V97, VERSION_DISEMBARK_V108, VERSION_DISEMBARK_V108 + 1] {
            let mut state = board(version);
            state.units.push(UnitState {
                health: 7,
                passenger: Some(Box::new(UnitState { id: 2, owner: 1, health: 3, ..UnitState::default() })),
                ..unit_at(&state, 1, 0)
            });
            assert!(disembark(&mut state, WorldCoordinates { x: 1, y: 0 }));
            assert_eq!(state.units.len(), 1);
            assert_eq!(state.units[0].id, 2);
            assert_eq!(state.units[0].health, 7);
            assert!(state.units[0].moved);
        }
    }

    #[test]
    fn flood_adds_swamped_only_for_the_swamp_skin() {
        let mut state = board(123);
        assert_eq!(flood(&mut state, 1, WorldCoordinates { x: 0, y: 0 }), vec![EFFECT_FLOODED]);
        state.player_runtime.push(crate::PlayerRuntime {
            player: 1,
            skin: SKIN_SWAMP,
            ..crate::PlayerRuntime::default()
        });
        assert_eq!(flood(&mut state, 1, WorldCoordinates { x: 0, y: 0 }), vec![EFFECT_FLOODED, EFFECT_SWAMPED]);
        assert_eq!(clear_tile_effect(EFFECT_ALGAE), vec![EFFECT_ALGAE, EFFECT_FOAM]);
        assert_eq!(clear_tile_effect(EFFECT_FLOODED), vec![EFFECT_FLOODED]);
    }

    #[test]
    fn start_and_end_match_set_status_on_both_sides_of_99() {
        let mut state = board(VERSION_START_MATCH - 1);
        assert!(!start_match(&mut state, START_MATCH_STARS_KIND, &[(1, 4, 0)], None));
        assert_eq!(state.current_state, GameStatus::Started);
        assert_eq!(state.players[0].currency, 16);
        assert!(state.tech.iter().any(|row| row.tech.contains(&4)));

        let mut state = board(VERSION_START_MATCH);
        assert!(start_match(&mut state, 0, &[], None));
        assert_eq!(state.current_state, GameStatus::Started);

        let mut state = board(VERSION_START_MATCH + 1);
        assert!(!start_match(&mut state, 0, &[], None));
        assert_eq!(state.current_state, GameStatus::Started);
        end_match(&mut state);
        assert_eq!(state.current_state, GameStatus::Ended);
    }

    #[test]
    fn resign_sets_autoplay_on_v91_and_later() {
        for version in [VERSION_RESIGN_V91, VERSION_RESIGN_V92, VERSION_RESIGN_V92 + 1] {
            let mut state = board(version);
            assert!(resign(&mut state, 1, 1));
            assert!(state.players[0].auto_play);
            assert_eq!(state.players[0].handicap, 1);
            assert_eq!(state.player_runtime.iter().find(|runtime| runtime.player == 1).unwrap().resigned_at_command, 7);
        }
    }

    #[test]
    fn peace_response_sets_peace_only_when_accepted() {
        let mut state = board(123);
        assert!(!peace_response(&mut state, 1, 2, false));
        assert!(peace_response(&mut state, 1, 2, true));
        assert_eq!(relation_between(&state, 1, 2).state, DiplomacyRelationState::Peace);
        assert_eq!(relation_between(&state, 2, 1).state, DiplomacyRelationState::Peace);
    }

    #[test]
    fn explode_poisons_when_damage_is_below_health() {
        let mut state = board(VERSION_EXPLODE_V82);
        state.units.push(UnitState {
            id: 3,
            owner: 2,
            health: 10,
            coordinates: WorldCoordinates { x: 1, y: 0 },
            ..UnitState::default()
        });
        let attacker = CombatUnit {
            health: 10,
            max_health: 10,
            attack: 1,
            defence: 1,
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
        let query = BattleQuery {
            game_version: VERSION_EXPLODE_V82,
            account_for_peace: false,
            has_active_peace_treaty: false,
            simple_data: true,
            attacker_in_attack_options: true,
            can_path_onto_defender: false,
        };
        assert!(explode(
            &mut state,
            1,
            WorldCoordinates { x: 0, y: 0 },
            &attacker,
            &[ExplodeTarget { coordinates: WorldCoordinates { x: 1, y: 0 }, defender: attacker, peaceful: false }],
            &query,
            &[],
        ));
        let defender = state.units.iter().find(|unit| unit.id == 3).unwrap();
        assert!(defender.poisoned || defender.health < 10);

        let mut later = board(VERSION_EXPLODE_V120 + 1);
        later.units.push(UnitState {
            id: 3,
            owner: 2,
            health: 10,
            coordinates: WorldCoordinates { x: 1, y: 0 },
            ..UnitState::default()
        });
        let mut query = query;
        query.game_version = VERSION_EXPLODE_V120 + 1;
        later.version = VERSION_EXPLODE_V120 + 1;
        assert!(explode(
            &mut later,
            1,
            WorldCoordinates { x: 0, y: 0 },
            &attacker,
            &[ExplodeTarget { coordinates: WorldCoordinates { x: 1, y: 0 }, defender: attacker, peaceful: true }],
            &query,
            &[],
        ));
        assert_eq!(later.units[0].health, 10);
        assert!(!later.units[0].poisoned);
    }

    #[test]
    fn upgrade_spends_on_both_sides_of_108() {
        for version in [VERSION_UPGRADE_V97, VERSION_UPGRADE_V108 + 1] {
            let mut state = board(version);
            state.units.push(unit_at(&state, 0, 0));
            assert!(upgrade(&mut state, 1, WorldCoordinates { x: 0, y: 0 }, 9, 4));
            assert_eq!(state.units[0].type_idx, 9);
            assert_eq!(state.players[0].currency, 6);
            assert!(!upgrade(&mut state, 1, WorldCoordinates { x: 0, y: 0 }, 3, 100));
            assert_eq!(state.units[0].type_idx, 9);
        }
    }

    fn relation_between(state: &GameState, player: u8, other: u8) -> DiplomacyRelation {
        state
            .diplomacy
            .iter()
            .find(|row| row.player == player)
            .unwrap()
            .relations
            .iter()
            .find(|link| link.other == other)
            .unwrap()
            .relation
    }
}
