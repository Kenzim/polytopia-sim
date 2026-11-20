//! AI decisions and difficulty.
//!
//! Formulas reproduce the game's AI. See `docs/specs/ai-decide.md`. Unit search calls [`crate::rules::path`] and
//! the scorers in [`super::score`].

use super::score::{
    self, DiplomacyOffer, ImprovementOffer, IssuedCommand, ResearchOffer, RoadPlan, TileCommand, TrainCity,
};
use crate::data::{load_game_logic, GameLogicData};
use crate::rules::economy::tile_is_frozen;
use crate::rules::path::{
    attack_options, get_move_options, movement_points, require_port_for_unit, tile_being_captured, PathSettings,
};
use crate::{BotDifficulty, GameState, PlayerState, UnitState, WorldCoordinates};

/// Late-game progress limit. Not read by the functions ported here.
pub const LATE_GAME_PROGRESS_LIMIT: f32 = 0.8;
/// Break-peace limit. Not read by the functions ported here.
pub const BREAK_PEACE_LIMIT: f32 = 0.0;
/// High aggression threshold. Not read by the functions ported here.
pub const HIGH_AGGRESSION: i32 = 5000;

/// The AI's defaults at start-up: log off, AI on.
pub const AI_ENABLE_LOG_DEFAULT: bool = false;
pub const AI_IS_ENABLED_DEFAULT: bool = true;

/// The final-turn game state. The end-of-turn choice compares the current state with this.
pub const FINAL_TURN_STATE: i32 = 3;
/// Player id of Nature. Situation analysis returns early for it, after clearing.
pub const NATURE_PLAYER_ID: u8 = 255;

/// Surprise ability type. First unit pass includes range > 1 or this.
pub const SURPRISE_ABILITY: i32 = 31;
/// The command pick feeds the seeded unit hash `y = 101 + index`.
pub const PICK_NOISE_BASE: i32 = 101;
/// Situation analysis starts mission noise at this `y`, then continues into the hit list.
pub const MISSION_NOISE_BASE: i32 = 501;

/// Ruin and unlocked manual-improvement missions store this score (30.0).
pub const FIXED_MISSION_SCORE: f32 = 30.0;

/// Versions greater than this use the harmonic bonus.
pub const DIFFICULTY_BONUS_VERSION_SPLIT: i32 = 103;

const HANDICAP_FROM_DIFFICULTY: [i32; 5] = [0, 1, 2, 4, -1];
const DIFFICULTY_FROM_HANDICAP: [i32; 6] = [4, 0, 1, 2, 1, 3];
const DIFFICULTY_BONUS: [f32; 5] = [0.0, 1.0, 2.0, 4.0, -1.0];

/// The unit hash multiplies the zero-extended hash by this (2^-32).
const HASH_UNIT_SCALE: f32 = f32::from_bits(0x2f80_0000);
const STUPID_HASH_BIAS: f32 = -0.5;
const STUPID_HASH_SCALE: f32 = 50.0;
const BONUS_DIVISOR: f32 = 5.0;
const HARMONIC_NUMERATOR: f32 = 0.2;
const OLD_LOG_DIVISOR: f64 = 11.4f32 as f64;
const OLD_BONUS_SCALE: f64 = 100.0;
const OLD_BONUS_DIVISOR: f32 = 100.0;

/// What the end-of-turn choice builds.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EndCommandKind {
    /// An end-turn command when the state is not the final turn.
    EndTurn,
    /// An end-match command when the state is the final turn.
    EndMatch,
}

/// Mirror of [`score::ScoredCommand`]. The command payload is owned by the caller.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ScoredCommand<C> {
    pub command: C,
    pub score: f32,
}

/// Handicap for a difficulty.
///
/// Table for 0..=4 is `[0, 1, 2, 4, -1]`. Any other `i32`, including negatives,
/// returns 0 because the bounds check is unsigned.
pub fn handicap_from_difficulty(difficulty: BotDifficulty) -> i32 {
    handicap_from_difficulty_value(difficulty as i32)
}

pub fn handicap_from_difficulty_value(difficulty: i32) -> i32 {
    if (difficulty as u32) > 4 {
        0
    } else {
        HANDICAP_FROM_DIFFICULTY[difficulty as usize]
    }
}

/// Difficulty for a handicap.
///
/// Index is `handicap + 1` (wrapping). Index 0..=5 maps to Frozen, Easy,
/// Normal, Hard, Normal, Crazy. Any other index returns Normal.
pub fn difficulty_from_handicap(handicap: i32) -> BotDifficulty {
    match difficulty_from_handicap_value(handicap) {
        0 => BotDifficulty::Easy,
        2 => BotDifficulty::Hard,
        3 => BotDifficulty::Crazy,
        4 => BotDifficulty::Frozen,
        _ => BotDifficulty::Normal,
    }
}

pub fn difficulty_from_handicap_value(handicap: i32) -> i32 {
    let index = handicap.wrapping_add(1);
    if (index as u32) > 5 {
        1
    } else {
        DIFFICULTY_FROM_HANDICAP[index as usize]
    }
}

/// The score sheet's difficulty bonus multiplier.
pub fn difficulty_bonus_multiplier(difficulty: BotDifficulty, opponent_count: i32, version: i32) -> f32 {
    difficulty_bonus_multiplier_value(difficulty as i32, opponent_count, version)
}

pub fn difficulty_bonus_multiplier_value(difficulty: i32, opponent_count: i32, version: i32) -> f32 {
    let bonus = difficulty_bonus(difficulty);
    if version > DIFFICULTY_BONUS_VERSION_SPLIT {
        new_difficulty_bonus(bonus, opponent_count)
    } else {
        old_difficulty_bonus(bonus, opponent_count)
    }
}

fn difficulty_bonus(difficulty: i32) -> f32 {
    if (difficulty as u32) > 4 {
        0.0
    } else {
        DIFFICULTY_BONUS[difficulty as usize]
    }
}

fn new_difficulty_bonus(bonus: f32, opponent_count: i32) -> f32 {
    let base = bonus / BONUS_DIVISOR;
    if opponent_count < 2 {
        return base;
    }
    let mut sum = f64::from(base);
    let mut i = 1;
    while i < opponent_count {
        sum += f64::from(HARMONIC_NUMERATOR / (i as f32));
        i += 1;
    }
    sum as f32
}

fn old_difficulty_bonus(bonus: f32, opponent_count: i32) -> f32 {
    let n = opponent_count.max(1);
    let x = (f64::from(n).ln() / OLD_LOG_DIVISOR + f64::from(bonus / BONUS_DIVISOR)) * OLD_BONUS_SCALE;
    let rounded = x.round_ties_even() as f32;
    rounded / OLD_BONUS_DIVISOR
}

/// A player's stupidity: 0 unless auto-play, else `max(0, 2 - handicap)`.
pub fn stupidity(auto_play: bool, handicap: i32) -> i32 {
    if !auto_play {
        0
    } else {
        2i32.wrapping_sub(handicap).max(0)
    }
}

pub fn player_stupidity(player: &PlayerState) -> i32 {
    stupidity(player.auto_play, player.handicap)
}

/// The unit value of a three-word hash, after the hash has returned `hash`.
///
/// The hash itself is the seeded xxHash32 (see [`crate::rng`]). This is only the scale step:
/// convert the unsigned hash to `f32` and multiply by 2^-32.
pub fn hash_to_unit_interval(hash: u32) -> f32 {
    (hash as f32) * HASH_UNIT_SCALE
}

/// The AI's stupid factor, once the seeded hash has produced `unit_hash`.
///
/// `unit_hash` is the unit value of `(current_command * 1000, rng_counter1, rng_counter2)`.
/// Result is `stupidity * (unit_hash - 0.5) * 50`, with the bias added as `-0.5`.
pub fn stupid_factor(unit_hash: f32, stupidity: i32) -> f32 {
    let biased = unit_hash + STUPID_HASH_BIAS;
    let scaled = (stupidity as f32) * biased;
    scaled * STUPID_HASH_SCALE
}

/// Words the stupid factor hashes.
pub fn stupid_factor_hash_args(current_command: u16, rng_counter1: i32, rng_counter2: i32) -> (i32, i32, i32) {
    (i32::from(current_command).wrapping_mul(1000), rng_counter1, rng_counter2)
}

/// `y` coordinate used when the command pick perturbs command `index`.
pub fn pick_noise_y(index: i32) -> i32 {
    index.wrapping_add(PICK_NOISE_BASE)
}

/// `y` coordinate for mission `index`. Hit-list noise continues after the last mission.
pub fn mission_noise_y(index: i32) -> i32 {
    index.wrapping_add(MISSION_NOISE_BASE)
}

pub fn apply_score_noise(score: f32, unit_hash: f32, stupidity: i32) -> f32 {
    score + stupid_factor(unit_hash, stupidity)
}

/// The end-of-turn choice: the final turn builds an end-match command, every other state an end-turn command.
pub fn end_command(current_state: i32) -> EndCommandKind {
    if current_state == FINAL_TURN_STATE {
        EndCommandKind::EndMatch
    } else {
        EndCommandKind::EndTurn
    }
}

/// Early exit of the AI's move search.
///
/// `None` means the search continues (map data, two unit passes, then tile commands).
/// `Some` means the AI ends its turn (or the match) and does not search.
/// With the default `is_enabled = true`, only a final turn or
/// handicap `-1` (Frozen) takes this exit.
pub fn get_move_immediate_end(ai_enabled: bool, current_state: i32, handicap: i32) -> Option<EndCommandKind> {
    let frozen_handicap = handicap_from_difficulty(BotDifficulty::Frozen);
    if !ai_enabled || current_state == FINAL_TURN_STATE || handicap == frozen_handicap {
        Some(end_command(current_state))
    } else {
        None
    }
}

/// First unit pass of the move search: range above 1, or range at most 1 and the Surprise ability.
pub fn unit_in_priority_pass(range: i32, has_surprise: bool) -> bool {
    range > 1 || has_surprise
}

/// The tile command type filter applies only when `specific_command != 0`.
pub fn command_passes_specific_filter(command_type: u16, specific_command: u16) -> bool {
    specific_command == 0 || command_type == specific_command
}

/// `unit_need` as written by the tile command search.
///
/// `max(0, (20*cities + 25 - explored_tiles) / 10) + hit_list + missions - units`,
/// divided by 3, plus 1. Negative results are doubled (`x < 0 ? x+x : x`).
pub fn unit_need(cities: i32, explored_tiles: f32, hit_list_count: i32, mission_count: i32, unit_count: i32) -> f32 {
    let base = cities.wrapping_mul(20).wrapping_add(25);
    let mut pressure = (base as f32 - explored_tiles) / 10.0;
    if pressure < 0.0 {
        pressure = 0.0;
    }
    let x = (pressure + hit_list_count as f32 + mission_count as f32 - unit_count as f32) / 3.0 + 1.0;
    if x < 0.0 {
        x + x
    } else {
        x
    }
}

/// Explore-mission score from situation analysis. `None` when fewer than 2 unexplored neighbors.
pub fn explore_mission_score(unexplored_neighbors: i32) -> Option<f32> {
    if unexplored_neighbors < 2 {
        None
    } else {
        Some(unexplored_neighbors.wrapping_mul(5) as f32)
    }
}

/// How many sorted explore missions are copied onto the player. `max` is not used:
/// `3 * city_count` when that is >= 2, otherwise 1.
pub fn explore_missions_kept(city_count: i32) -> i32 {
    let scaled = city_count.wrapping_mul(3);
    if scaled >= 2 {
        scaled
    } else {
        1
    }
}

/// Explore area radius: 2 on mountain (`terrain == 4`), otherwise 1. Diagonal on, center excluded.
pub fn explore_area_radius(terrain: i32) -> i32 {
    if terrain == 4 {
        2
    } else {
        1
    }
}

/// Rough tiles divided by explored tiles, both converted to `f32` first.
pub fn rough_terrain_ratio(rough_tiles: i32, explored_tiles: i32) -> f32 {
    (rough_tiles as f32) / (explored_tiles as f32)
}

/// Unit ability ids. Names are the JSON strings.
pub fn unit_ability_id(name: &str) -> Option<i32> {
    Some(match name {
        "dash" => 1,
        "escape" => 2,
        "scout" => 3,
        "sneak" => 4,
        "hide" => 5,
        "persist" => 7,
        "convert" => 8,
        "heal" => 9,
        "swim" => 10,
        "carry" => 11,
        "grow" => 12,
        "fly" => 13,
        "splash" => 14,
        "navigate" => 16,
        "crush" => 17,
        "freeze" => 18,
        "freezearea" => 19,
        "autofreeze" => 20,
        "skate" => 21,
        "fortify" => 22,
        "creep" => 23,
        "boost" => 24,
        "independent" => 25,
        "poison" => 26,
        "eat" => 27,
        "unique" => 29,
        "explode" => 30,
        "surprise" => 31,
        "infiltrate" => 35,
        "detect" => 36,
        "stiff" => 38,
        "protect" => 39,
        "stomp" => 40,
        "autoheal" => 41,
        "static" => 42,
        "tentacles" => 44,
        "autoflood" => 45,
        "drench" => 46,
        "amphibious" => 47,
        "water" => 48,
        "land" => 49,
        "algae" => 51,
        "consumed" => 52,
        "swarm" => 53,
        "doubleattack" => 54,
        _ => return None,
    })
}

/// A unit's range, looked up from the unit data.
pub fn get_range(logic: &GameLogicData, type_idx: i32) -> Option<i32> {
    logic.unit_by_idx(type_idx).map(|unit| unit.range)
}

/// Whether an ability list contains `ability`.
///
/// A null ability list returns false.
pub fn has_ability(abilities: &[i32], ability: i32) -> bool {
    abilities.contains(&ability)
}

/// Whether a unit can attack.
///
/// Attacked or frozen (effect 0) is false. A unit that
/// has moved can still attack when it has Dash.
pub fn can_attack(unit: &UnitState, abilities: &[i32]) -> bool {
    if unit.attacked || unit.frozen {
        return false;
    }
    if unit.moved && !has_ability(abilities, score::ability::DASH) {
        return false;
    }
    true
}

/// Whether a unit can move. Moved or frozen is false.
pub fn can_move(unit: &UnitState) -> bool {
    !unit.moved && !unit.frozen
}

/// Whether a unit can perform any ability.
///
/// In retail order: a unit without a leader that is hurt (below `max_health`, which is the passenger's for a
/// carrier without Protect) or poisoned and can move and attack; then `later`, the disjunction of HealOthers with
/// heal options, promotion, FreezeArea, BreakIce, exploding and an unlocked Disband on a fresh unit; then
/// Grow (`12`) forces false; last `can_upgrade`, whether the player can afford any unlocked upgrade.
pub fn can_perform_any_ability(
    unit: &UnitState,
    max_health: i32,
    abilities: &[i32],
    later: bool,
    can_upgrade: bool,
) -> bool {
    let hurt = max_health > i32::from(unit.health) || unit.poisoned;
    if unit.leader == 0 && hurt && can_move(unit) && can_attack(unit, abilities) {
        return true;
    }
    if later {
        return true;
    }
    if has_ability(abilities, 12) {
        return false;
    }
    can_upgrade
}

/// Whether a unit can perform any action.
///
/// An ability that can be performed returns true. A unit that has not moved
/// returns true after that. A moved unit without Dash must be promotable
/// or the function returns false. Otherwise it returns whether
/// the attack options are non-empty.
pub fn can_perform_any_action(
    unit: &UnitState,
    max_health: i32,
    abilities: &[i32],
    later_ability: bool,
    can_upgrade: bool,
    can_be_promoted: bool,
    attack_option_count: i32,
) -> bool {
    if can_perform_any_ability(unit, max_health, abilities, later_ability, can_upgrade) {
        return true;
    }
    if unit.moved {
        let dash = !unit.attacked && has_ability(abilities, score::ability::DASH);
        if !dash && !can_be_promoted {
            return false;
        }
    }
    if !unit.moved {
        return true;
    }
    attack_option_count != 0
}

/// Facts the attack option step reads after rating the battle.
///
/// `battle_sum` is the summed battle rating. `adjust_for_position`
/// is the low bit tested after the battle results. `position_score`
/// is the position rating with `can_attack` false. `aggression` is
/// the player's aggression. `infiltrate_city` is the Infiltrate ability
/// together with an opponent city.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct AttackFacts {
    pub battle_sum: f32,
    pub adjust_for_position: bool,
    pub position_score: f32,
    pub aggression: i32,
    pub infiltrate_city: bool,
}

/// Score kept for an attack option when it is strictly positive.
#[allow(clippy::neg_cmp_op_on_partial_ord)] // `!(a > b)` is deliberately true for NaN
pub fn attack_option_score(current: f32, facts: &AttackFacts) -> Option<f32> {
    let mut score = facts.battle_sum;
    if !facts.infiltrate_city {
        if facts.adjust_for_position {
            score += (facts.position_score - current) * 0.3;
        }
        if !(score > 0.0) {
            return None;
        }
    }
    score += score * (facts.aggression as f32) * 0.001;
    if score > 0.0 {
        Some(score)
    } else {
        None
    }
}

/// What the move search returns. Tile variants are built by [`super::score`].
#[derive(Debug, Clone, PartialEq)]
pub enum AiCommand {
    EndTurn { player: u8 },
    EndMatch { player: u8 },
    Stay { player: u8, at: WorldCoordinates },
    Move { player: u8, unit: u32, from: WorldCoordinates, to: WorldCoordinates },
    Attack { player: u8, unit: u32, from: WorldCoordinates, to: WorldCoordinates },
    Tile(TileCommand),
}

impl AiCommand {
    pub fn command_type(&self) -> u16 {
        match self {
            AiCommand::EndTurn { .. } => 15,
            AiCommand::EndMatch { .. } => 22,
            AiCommand::Stay { .. } => 21,
            AiCommand::Move { .. } => 6,
            AiCommand::Attack { .. } => 2,
            AiCommand::Tile(command) => match command {
                TileCommand::Train { .. } => score::command_type::TRAIN,
                TileCommand::Research { .. } => score::command_type::RESEARCH,
                TileCommand::Build { .. } => score::command_type::BUILD,
                TileCommand::BreakPeace { .. } => score::command_type::BREAK_PEACE,
                TileCommand::PeaceTreaty { .. } => score::command_type::PEACE_TREATY,
                TileCommand::EstablishEmbassy { .. } => score::command_type::ESTABLISH_EMBASSY,
            },
        }
    }

    fn player(&self) -> u8 {
        match self {
            AiCommand::EndTurn { player }
            | AiCommand::EndMatch { player }
            | AiCommand::Stay { player, .. }
            | AiCommand::Move { player, .. }
            | AiCommand::Attack { player, .. } => *player,
            AiCommand::Tile(command) => match command {
                TileCommand::Train { player, .. }
                | TileCommand::Research { player, .. }
                | TileCommand::Build { player, .. }
                | TileCommand::BreakPeace { player, .. }
                | TileCommand::PeaceTreaty { player, .. }
                | TileCommand::EstablishEmbassy { player, .. } => *player,
            },
        }
    }
}

/// Offers passed into the tile command search.
///
/// The tech needs check is already reflected by `research`.
#[derive(Debug, Clone, PartialEq)]
pub struct TileCommandInputs {
    pub explored_tiles: f32,
    pub hit_list_count: i32,
    pub mission_count: i32,
    pub cities: i32,
    pub trains: Vec<TrainCity>,
    pub research: Vec<ResearchOffer>,
    pub improvements: Vec<ImprovementOffer>,
    pub diplomacy: Vec<DiplomacyOffer>,
    pub roads: Vec<RoadPlan>,
}

impl Default for TileCommandInputs {
    fn default() -> Self {
        Self {
            explored_tiles: 0.0,
            hit_list_count: 0,
            mission_count: 0,
            cities: 0,
            trains: Vec::new(),
            research: Vec::new(),
            improvements: Vec::new(),
            diplomacy: Vec::new(),
            roads: Vec::new(),
        }
    }
}

/// The number of frozen tiles a player owns.
///
/// Counts owned tiles for which [`tile_is_frozen`] is true.
pub fn frozen_tiles_owned(state: &GameState, player: u8) -> i32 {
    let mut count = 0i32;
    for tile in &state.map.tiles {
        if tile.owner == player && tile_is_frozen(tile.terrain, tile.climate) {
            count = count.wrapping_add(1);
        }
    }
    count
}

/// The basic command validation.
///
/// True when `command_player` is the current player and
/// no pending command trigger exists. This board has no
/// pending-trigger list, so a matching current player is enough.
pub fn passes_basic_validation(state: &GameState, command_player: u8) -> bool {
    state.players.get(usize::from(state.current_player_index)).is_some_and(|player| player.id == command_player)
}

fn end_to_command(kind: EndCommandKind, player: u8) -> AiCommand {
    match kind {
        EndCommandKind::EndTurn => AiCommand::EndTurn { player },
        EndCommandKind::EndMatch => AiCommand::EndMatch { player },
    }
}

fn ability_ids(names: &[String]) -> Vec<i32> {
    names.iter().filter_map(|name| unit_ability_id(name)).collect()
}

fn terrain_ids(logic: &GameLogicData, names: &[String]) -> Vec<i32> {
    names.iter().filter_map(|name| logic.terrain(name).map(|terrain| terrain.idx)).collect()
}

/// The best move for one unit.
///
/// `rate_position` is the position rating. `attack_facts` supplies the
/// opaque battle inputs. `movement_cost` is the movement cost.
/// `unit_hash` is the seeded unit hash for the pick noise.
pub fn get_best_unit_move<R, F, C, H>(
    state: &GameState,
    logic: &GameLogicData,
    player: &PlayerState,
    unit: &UnitState,
    mut rate_position: R,
    mut attack_facts: F,
    mut movement_cost: C,
    mut unit_hash: H,
) -> Option<AiCommand>
where
    R: FnMut(&UnitState, WorldCoordinates, bool) -> f32,
    F: FnMut(&UnitState, WorldCoordinates) -> AttackFacts,
    C: FnMut(WorldCoordinates, WorldCoordinates) -> i32,
    H: FnMut(i32, i32, i32) -> f32,
{
    let data = logic.unit_by_idx(unit.type_idx)?;
    let abilities = ability_ids(&data.unit_abilities);
    let terrain = terrain_ids(logic, &data.movement_terrain);
    let current = rate_position(unit, unit.coordinates, false);
    let mut commands = Vec::new();
    if can_attack(unit, &abilities) {
        let options = attack_options(
            state,
            unit,
            data.range,
            false,
            data.attack,
            &abilities,
            false,
            |i| tile_being_captured(state, i),
            |_| false,
        );
        for target in options {
            let facts = attack_facts(unit, target);
            if let Some(score) = attack_option_score(current, &facts) {
                commands.push(ScoredCommand {
                    command: AiCommand::Attack { player: player.id, unit: unit.id, from: unit.coordinates, to: target },
                    score,
                });
            }
        }
    }
    if can_move(unit) {
        commands.push(ScoredCommand {
            command: AiCommand::Stay { player: player.id, at: unit.coordinates },
            score: current * 0.8,
        });
        let settings = PathSettings {
            version: state.version,
            player_id: player.id,
            allowed_terrain: &terrain,
            abilities: &abilities,
            mover: Some(unit),
            require_port: require_port_for_unit(has_ability(&abilities, 10), &abilities),
            allow_occupied: false,
            allow_enemy: true,
            allow_allies: false,
            can_player_embark: false,
        };
        let movement = movement_points(state.version, data.movement, unit);
        let destinations = get_move_options(
            state,
            unit.coordinates,
            movement,
            unit,
            &settings,
            None,
            |_| false,
            |_, _| false,
            |_| 8,
            &mut movement_cost,
        );
        let attack_ready = can_attack(unit, &abilities);
        for destination in destinations {
            let mut score = rate_position(unit, destination, attack_ready);
            if has_ability(&abilities, 11) {
                let origin_water = state
                    .map
                    .tiles
                    .iter()
                    .find(|tile| tile.coordinates == unit.coordinates)
                    .is_some_and(|tile| crate::rules::path::is_water(tile.terrain));
                let dest_water = state
                    .map
                    .tiles
                    .iter()
                    .find(|tile| tile.coordinates == destination)
                    .is_some_and(|tile| crate::rules::path::is_water(tile.terrain));
                if origin_water && !dest_water {
                    score += (data.cost as f32) * -10.0;
                }
            }
            score += 5.0;
            commands.push(ScoredCommand {
                command: AiCommand::Move { player: player.id, unit: unit.id, from: unit.coordinates, to: destination },
                score,
            });
        }
    }
    pick_best(state, player, &mut commands, &mut unit_hash)
}

/// Picks the best possible command.
///
/// Adds the stupid factor at `y = 101 + index`, sorts descending, and returns index
/// 0 when [`passes_basic_validation`] accepts it. It does not try the next entry.
pub fn pick_best<H>(
    state: &GameState,
    player: &PlayerState,
    commands: &mut [ScoredCommand<AiCommand>],
    mut unit_hash: H,
) -> Option<AiCommand>
where
    H: FnMut(i32, i32, i32) -> f32,
{
    if commands.is_empty() {
        return None;
    }
    let stupidity = player_stupidity(player);
    for (index, command) in commands.iter_mut().enumerate() {
        let hash = unit_hash(i32::from(state.current_command).wrapping_mul(1000), pick_noise_y(index as i32), 0);
        command.score = apply_score_noise(command.score, hash, stupidity);
    }
    commands.sort_by(|left, right| right.score.total_cmp(&left.score));
    let best = &commands[0];
    if passes_basic_validation(state, best.command.player()) {
        Some(best.command.clone())
    } else {
        None
    }
}

/// The tile command search.
///
/// Writes `unit_need` and the frozen-tile count, then runs the five builders.
/// `specific_command != 0` keeps only that command type. The pick is [`pick_best`].
pub fn get_tile_commands<H>(
    state: &GameState,
    player: &PlayerState,
    specific_command: u16,
    inputs: &TileCommandInputs,
    unit_hash: H,
) -> (Option<AiCommand>, f32, i32)
where
    H: FnMut(i32, i32, i32) -> f32,
{
    let units = state.units.iter().filter(|unit| unit.owner == player.id).count() as i32;
    let need = unit_need(inputs.cities, inputs.explored_tiles, inputs.hit_list_count, inputs.mission_count, units);
    let frozen = frozen_tiles_owned(state, player.id);
    let mut issued = Vec::new();
    issued.extend(score::add_possible_research_commands(player.currency, &inputs.research));
    issued.extend(score::add_possible_train_unit_commands(player.currency, need, &inputs.trains));
    issued.extend(score::add_possible_improvement_commands(player.currency, &inputs.improvements));
    issued.extend(score::add_possible_diplomacy_commands(&inputs.diplomacy));
    issued.extend(score::add_possible_road_building_commands(&inputs.roads));
    if specific_command != 0 {
        issued.retain(|command| command.command_type == specific_command);
    }
    let mut scored: Vec<ScoredCommand<AiCommand>> = issued
        .into_iter()
        .map(|command: IssuedCommand| ScoredCommand { score: command.score, command: AiCommand::Tile(command.command) })
        .collect();
    let chosen = pick_best(state, player, &mut scored, unit_hash);
    (chosen, need, frozen)
}

/// The AI's move search, after the Frozen and final-turn exits.
///
/// The first pass keeps units with range `> 1` or Surprise. The second pass
/// retries every unit, including those the first pass already tried. The first
/// non-null [`get_best_unit_move`] is returned. Otherwise [`get_tile_commands`],
/// otherwise [`end_command`].
pub fn get_move<R, F, C, H>(
    state: &GameState,
    player_index: usize,
    specific_command: u16,
    ai_enabled: bool,
    mut rate_position: R,
    mut attack_facts: F,
    mut movement_cost: C,
    mut unit_hash: H,
    tiles: &TileCommandInputs,
) -> AiCommand
where
    R: FnMut(&UnitState, WorldCoordinates, bool) -> f32,
    F: FnMut(&UnitState, WorldCoordinates) -> AttackFacts,
    C: FnMut(WorldCoordinates, WorldCoordinates) -> i32,
    H: FnMut(i32, i32, i32) -> f32,
{
    let Some(player) = state.players.get(player_index) else {
        return AiCommand::EndTurn { player: 0 };
    };
    let status = state.current_state as i32;
    if let Some(kind) = get_move_immediate_end(ai_enabled, status, player.handicap) {
        return end_to_command(kind, player.id);
    }
    if let Ok(logic) = load_game_logic(state.version) {
        let owned: Vec<usize> = state
            .units
            .iter()
            .enumerate()
            .filter(|(_, unit)| unit.owner == player.id)
            .map(|(index, _)| index)
            .collect();
        if let Some(command) = unit_pass(
            state,
            &logic,
            player,
            &owned,
            true,
            &mut rate_position,
            &mut attack_facts,
            &mut movement_cost,
            &mut unit_hash,
        ) {
            return command;
        }
        if let Some(command) = unit_pass(
            state,
            &logic,
            player,
            &owned,
            false,
            &mut rate_position,
            &mut attack_facts,
            &mut movement_cost,
            &mut unit_hash,
        ) {
            return command;
        }
    }
    if let Some(command) = get_tile_commands(state, player, specific_command, tiles, unit_hash).0 {
        return command;
    }
    end_to_command(end_command(status), player.id)
}

fn unit_pass<R, F, C, H>(
    state: &GameState,
    logic: &GameLogicData,
    player: &PlayerState,
    owned: &[usize],
    priority_only: bool,
    rate_position: &mut R,
    attack_facts: &mut F,
    movement_cost: &mut C,
    unit_hash: &mut H,
) -> Option<AiCommand>
where
    R: FnMut(&UnitState, WorldCoordinates, bool) -> f32,
    F: FnMut(&UnitState, WorldCoordinates) -> AttackFacts,
    C: FnMut(WorldCoordinates, WorldCoordinates) -> i32,
    H: FnMut(i32, i32, i32) -> f32,
{
    for index in owned {
        let unit = &state.units[*index];
        let Some(data) = logic.unit_by_idx(unit.type_idx) else {
            continue;
        };
        let abilities = ability_ids(&data.unit_abilities);
        let range = data.range;
        let surprise = has_ability(&abilities, SURPRISE_ABILITY);
        if priority_only && !unit_in_priority_pass(range, surprise) {
            continue;
        }
        let options = if can_attack(unit, &abilities) {
            attack_options(
                state,
                unit,
                range,
                false,
                data.attack,
                &abilities,
                false,
                |i| tile_being_captured(state, i),
                |_| false,
            )
            .len() as i32
        } else {
            0
        };
        let max_health = i32::from(unit.promotion).wrapping_mul(50).wrapping_add(data.health);
        if !can_perform_any_action(unit, max_health, &abilities, false, false, false, options) {
            continue;
        }
        if let Some(command) = get_best_unit_move(
            state,
            logic,
            player,
            unit,
            &mut *rate_position,
            &mut *attack_facts,
            &mut *movement_cost,
            &mut *unit_hash,
        ) {
            return Some(command);
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::score;
    use super::*;
    use crate::{BotDifficulty, GameStatus, PlayerState, UnitState};

    #[test]
    fn handicap_table_matches_jump_table() {
        assert_eq!(handicap_from_difficulty(BotDifficulty::Easy), 0);
        assert_eq!(handicap_from_difficulty(BotDifficulty::Normal), 1);
        assert_eq!(handicap_from_difficulty(BotDifficulty::Hard), 2);
        assert_eq!(handicap_from_difficulty(BotDifficulty::Crazy), 4);
        assert_eq!(handicap_from_difficulty(BotDifficulty::Frozen), -1);
        assert_eq!(handicap_from_difficulty_value(5), 0);
        assert_eq!(handicap_from_difficulty_value(-1), 0);
    }

    #[test]
    fn difficulty_from_handicap_table() {
        assert_eq!(difficulty_from_handicap(-1), BotDifficulty::Frozen);
        assert_eq!(difficulty_from_handicap(0), BotDifficulty::Easy);
        assert_eq!(difficulty_from_handicap(1), BotDifficulty::Normal);
        assert_eq!(difficulty_from_handicap(2), BotDifficulty::Hard);
        assert_eq!(difficulty_from_handicap(3), BotDifficulty::Normal);
        assert_eq!(difficulty_from_handicap(4), BotDifficulty::Crazy);
        assert_eq!(difficulty_from_handicap(5), BotDifficulty::Normal);
        assert_eq!(difficulty_from_handicap(-2), BotDifficulty::Normal);
        assert_eq!(difficulty_from_handicap(i32::MAX), BotDifficulty::Normal);
    }

    #[test]
    fn difficulty_round_trip_except_unproduced_handicap_three() {
        for difficulty in [
            BotDifficulty::Easy,
            BotDifficulty::Normal,
            BotDifficulty::Hard,
            BotDifficulty::Crazy,
            BotDifficulty::Frozen,
        ] {
            let handicap = handicap_from_difficulty(difficulty);
            assert_eq!(difficulty_from_handicap(handicap), difficulty);
        }
    }

    #[test]
    fn bonus_multiplier_bits() {
        let samples = [
            (0, 1, 104, 0x0000_0000),
            (1, 1, 104, 0x3e4c_cccd),
            (2, 1, 104, 0x3ecc_cccd),
            (3, 1, 104, 0x3f4c_cccd),
            (4, 1, 104, 0xbe4c_cccd),
            (0, 2, 104, 0x3e4c_cccd),
            (3, 2, 104, 0x3f80_0000),
            (4, 2, 104, 0x0000_0000),
            (4, 3, 104, 0x3dcc_cccd),
            (2, 4, 104, 0x3f44_4444),
            (1, 3, 103, 0x3e99_999a),
            (4, 3, 103, 0xbdcc_cccd),
            (0, 8, 103, 0x3e38_51ec),
            (5, 3, 104, 0x3e99_999a),
            (-1, 2, 104, 0x3e4c_cccd),
            (3, 8, 104, 0x3fa8_c6f3),
            (4, 0, 90, 0xbe4c_cccd),
            (1, 0, 103, 0x3e4c_cccd),
            (2, 4, 103, 0x3f05_1eb8),
            (3, 2, 103, 0x3f5c_28f6),
        ];
        for (difficulty, opponents, version, bits) in samples {
            assert_eq!(
                difficulty_bonus_multiplier_value(difficulty, opponents, version).to_bits(),
                bits,
                "difficulty {difficulty} opponents {opponents} version {version}"
            );
        }
        assert_eq!(difficulty_bonus_multiplier(BotDifficulty::Crazy, 1, 104).to_bits(), 0x3f4c_cccd);
    }

    #[test]
    fn stupidity_clamps_autoplay_only() {
        assert_eq!(stupidity(false, -1), 0);
        assert_eq!(stupidity(false, 0), 0);
        assert_eq!(stupidity(true, -1), 3);
        assert_eq!(stupidity(true, 0), 2);
        assert_eq!(stupidity(true, 1), 1);
        assert_eq!(stupidity(true, 2), 0);
        assert_eq!(stupidity(true, 3), 0);
        assert_eq!(stupidity(true, 4), 0);
    }

    #[test]
    fn stupid_factor_and_hash_scale() {
        assert_eq!(stupid_factor(0.0, 2).to_bits(), 0xc248_0000);
        assert_eq!(stupid_factor(1.0, 3).to_bits(), 0x4296_0000);
        assert_eq!(stupid_factor(0.25, 1).to_bits(), 0xc148_0000);
        assert_eq!(stupid_factor(0.5, 4).to_bits(), 0);
        // +0 * negative bias is -0, matching the game's float multiply.
        assert_eq!(stupid_factor(0.25, 0).to_bits(), 0x8000_0000);
        assert_eq!(stupid_factor(1.0, 0).to_bits(), 0);
        assert_eq!(hash_to_unit_interval(0).to_bits(), 0);
        assert_eq!(hash_to_unit_interval(1).to_bits(), 0x2f80_0000);
        assert_eq!(stupid_factor_hash_args(7, 101, 0), (7000, 101, 0));
        assert_eq!(pick_noise_y(0), 101);
        assert_eq!(mission_noise_y(0), 501);
        assert_eq!(apply_score_noise(10.0, 0.5, 2).to_bits(), 10.0f32.to_bits());
    }

    #[test]
    #[allow(clippy::assertions_on_constants)] // pins the default flags
    fn frozen_ends_the_turn_and_other_handicaps_do_not() {
        assert_eq!(
            get_move_immediate_end(true, 2, handicap_from_difficulty(BotDifficulty::Frozen)),
            Some(EndCommandKind::EndTurn)
        );
        assert_eq!(get_move_immediate_end(true, FINAL_TURN_STATE, 0), Some(EndCommandKind::EndMatch));
        assert_eq!(get_move_immediate_end(true, FINAL_TURN_STATE, -1), Some(EndCommandKind::EndMatch));
        for handicap in [0, 1, 2, 4] {
            assert_eq!(get_move_immediate_end(true, 2, handicap), None);
        }
        assert_eq!(get_move_immediate_end(false, 2, 0), Some(EndCommandKind::EndTurn));
        assert_eq!(end_command(4), EndCommandKind::EndTurn);
        assert!(AI_IS_ENABLED_DEFAULT);
        assert!(!AI_ENABLE_LOG_DEFAULT);
    }

    #[test]
    fn unit_need_explore_and_priority_pass() {
        assert_eq!(unit_need(1, 0.0, 0, 0, 0).to_bits(), 0x4020_0000);
        assert_eq!(unit_need(0, 100.0, 0, 0, 10).to_bits(), 0xc095_5555);
        assert_eq!(unit_need(2, 5.0, 1, 2, 3).to_bits(), 0x4040_0000);
        assert_eq!(explore_mission_score(1), None);
        assert_eq!(explore_mission_score(2), Some(10.0));
        assert_eq!(explore_mission_score(3), Some(15.0));
        assert_eq!(explore_missions_kept(0), 1);
        assert_eq!(explore_missions_kept(1), 3);
        assert_eq!(explore_missions_kept(2), 6);
        assert_eq!(explore_area_radius(4), 2);
        assert_eq!(explore_area_radius(5), 1);
        assert_eq!(rough_terrain_ratio(1, 4).to_bits(), 0.25f32.to_bits());
        assert!(rough_terrain_ratio(0, 0).is_nan());
        assert!(unit_in_priority_pass(2, false));
        assert!(!unit_in_priority_pass(1, false));
        assert!(unit_in_priority_pass(1, true));
        assert!(command_passes_specific_filter(5, 0));
        assert!(command_passes_specific_filter(5, 5));
        assert!(!command_passes_specific_filter(6, 5));
        assert_eq!(FIXED_MISSION_SCORE.to_bits(), 0x41f0_0000);
    }

    fn zero_hash(_x: i32, _y: i32, _z: i32) -> f32 {
        0.0
    }

    fn flat_cost(_from: WorldCoordinates, _to: WorldCoordinates) -> i32 {
        1
    }

    fn no_attack(_unit: &UnitState, _target: WorldCoordinates) -> AttackFacts {
        AttackFacts {
            battle_sum: 0.0,
            adjust_for_position: false,
            position_score: 0.0,
            aggression: 0,
            infiltrate_city: false,
        }
    }

    fn tiny_board(handicap: i32) -> GameState {
        let mut state = GameState::new(123, 1);
        state.current_state = GameStatus::Started;
        state.current_player_index = 0;
        state.map = crate::Map::new(3, 3);
        for tile in &mut state.map.tiles {
            tile.terrain = 3;
            tile.owner = 1;
            state.fog.push(crate::TileExplorers { tile: tile.index, explorers: vec![1].into() });
        }
        let city = state.map.tiles.iter().position(|tile| tile.coordinates.x == 1 && tile.coordinates.y == 1).unwrap();
        state.map.tiles[city].improvement = 1;
        state.map.tiles[city].is_city = true;
        state.players.push(PlayerState {
            id: 1,
            handicap,
            currency: 5,
            score: 0,
            tribe: 1,
            auto_play: false,
            start_tile: WorldCoordinates { x: 1, y: 1 },
        });
        state.units.push(UnitState {
            id: 7,
            owner: 1,
            type_idx: 2,
            coordinates: WorldCoordinates { x: 1, y: 0 },
            health: 100,
            ..UnitState::default()
        });
        state
    }

    #[test]
    fn range_ability_and_action_gates() {
        let logic = crate::data::load_game_logic(123).expect("logic");
        assert_eq!(get_range(&logic, 2), Some(1));
        let dash = [score::ability::DASH, score::ability::FORTIFY];
        assert!(!has_ability(&dash, SURPRISE_ABILITY));
        assert!(has_ability(&dash, score::ability::DASH));
        let fresh = UnitState { health: 100, ..UnitState::default() };
        assert!(can_perform_any_action(&fresh, 100, &dash, false, false, false, 0));
        let spent = UnitState { health: 100, moved: true, attacked: true, ..UnitState::default() };
        assert!(!can_perform_any_action(&spent, 100, &dash, false, false, false, 0));
    }

    #[test]
    fn attack_option_scales_battle_and_drops_non_positive() {
        let facts = AttackFacts {
            battle_sum: 10.0,
            adjust_for_position: true,
            position_score: 20.0,
            aggression: 0,
            infiltrate_city: false,
        };
        assert_eq!(attack_option_score(0.0, &facts), Some(16.0));
        let dead = AttackFacts { battle_sum: -1.0, adjust_for_position: false, ..facts };
        assert_eq!(attack_option_score(0.0, &dead), None);
    }

    #[test]
    fn warrior_on_a_tiny_map_returns_a_unit_command() {
        let state = tiny_board(1);
        let command = get_move(
            &state,
            0,
            0,
            true,
            |_unit, _at, _can_attack| 0.0,
            no_attack,
            flat_cost,
            zero_hash,
            &TileCommandInputs::default(),
        );
        assert!(matches!(command, AiCommand::Move { player: 1, unit: 7, to: WorldCoordinates { x: 1, y: 1 }, .. }));
    }

    #[test]
    fn frozen_handicap_ends_the_turn_without_searching() {
        let state = tiny_board(-1);
        let command = get_move(
            &state,
            0,
            0,
            true,
            |_unit, _at, _can_attack| panic!("rate position"),
            |_unit, _at| panic!("attack facts"),
            |_from, _to| panic!("movement cost"),
            |_x, _y, _z| panic!("hash"),
            &TileCommandInputs::default(),
        );
        assert_eq!(command, AiCommand::EndTurn { player: 1 });
    }
}
