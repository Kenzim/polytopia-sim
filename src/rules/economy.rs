//! Economy, tech, tribes, and score.
//!
//! Numeric effects below follow the game's economy code. Inputs that live
//! in the game logic data (improvement work, tech cost, embassy income per level,
//! tribe tech graph) are arguments. No tribe star bonus is applied here:
//! none of the economy functions branch on the tribe.

use std::collections::BTreeMap;

use crate::PlayerState;

/// Score sheet constants.
pub const TILE_VALUE: i32 = 20;
pub const CITY_LEVEL_SCORE: i32 = 50;
pub const CITY_STARVATION_PENALTY: i32 = 100;
pub const CITY_XP_SCORE: i32 = 5;
pub const EXPLORE_VALUE: i32 = 5;

/// The City improvement type. [`calculate_work`] takes the city path when `idx == 1`.
pub const CITY_IMPROVEMENT_INDEX: i32 = 1;
/// The Port improvement type, the neighbor that doubles `Doubled` buildings.
pub const PORT_IMPROVEMENT: i32 = 8;
/// The Doubled improvement ability.
pub const DOUBLED_ABILITY: i32 = 22;
/// The Attract improvement ability.
pub const ATTRACT_ABILITY: i32 = 10;
/// The robbed improvement effect.
pub const ROBBED_EFFECT: i32 = 1;
/// The Forest terrain type.
pub const FOREST_TERRAIN: i32 = 5;
/// The Game resource type, spawned by Attract.
pub const GAME_RESOURCE: i32 = 1;
/// The Literacy player ability.
pub const LITERACY_ABILITY: i32 = 2;
/// The capital vision player ability.
pub const CAPITAL_VISION_ABILITY: i32 = 8;
/// The Peace diplomacy relation state.
pub const PEACE_STATE: i32 = 1;
/// The Started game state.
pub const GAME_STATE_STARTED: u8 = 2;
/// The Perfection game mode.
pub const PERFECTION_MODE: i32 = 1;
/// The Genius task type, checked at the end of [`learn_tech`].
pub const GENIUS_TASK: i32 = 2;
/// The Wealth task type, checked by the increase-currency action.
pub const WEALTH_TASK: i32 = 4;

/// Version gates taken from the comparisons in the game's code.
pub const VERSION_ROBBED_BLOCKS_CITY: i32 = 0x5E;
pub const VERSION_UNIT_SCORE_FULL_COST: i32 = 0x64;
pub const VERSION_PORT_OWNER_MATTERS: i32 = 0x65;
pub const VERSION_START_TURN_DEFAULT: i32 = 0x3B;
pub const VERSION_IMPROVEMENT_LEVEL: i32 = 0x67;
pub const VERSION_RESTORE_CLIMATE: i32 = 0x70;
pub const VERSION_UNLOCKABLE_DEFAULT: i32 = 0x71;

/// The lighthouse improvement type.
pub const LIGHTHOUSE_IMPROVEMENT: i32 = 47;
/// The ice bank improvement type.
pub const ICE_BANK_IMPROVEMENT: i32 = 34;
/// The Fungi improvement type. [`max_improvement_level`] adds one when this idx has Microbes.
pub const FUNGI_IMPROVEMENT: i32 = 37;
/// The Patina improvement ability.
pub const PATINA_ABILITY: i32 = 5;
/// The Microbes player ability.
pub const MICROBES_ABILITY: i32 = 14;
/// The Ice terrain type.
pub const ICE_TERRAIN: i32 = 6;
/// The Polaris climate, which [`tile_is_frozen`] treats as frozen off water.
pub const POLARIS_CLIMATE: i32 = 16;
/// f32 divisor in [`ice_bank_level`] (bits `0x41A00000`).
pub const ICE_BANK_TILES_PER_LEVEL: f32 = 20.0;
/// V59 start-turn aggression when the acting player's score is strictly above the other's.
pub const V59_AHEAD_AGGRESSION: i32 = -100;
/// V59 start-turn aggression when the acting player's score is not above the other's.
pub const V59_BEHIND_AGGRESSION: i32 = 100;
/// Subtracted after `handicap * 100` for a player the actor does not know.
pub const V59_UNKNOWN_AGGRESSION_BIAS: i32 = -200;
/// Aggression change for an enemy unit standing on the actor's tile.
pub const V59_ENEMY_UNIT_AGGRESSION: i32 = 500;
/// Second aggression change when that tile is a city.
pub const V59_ENEMY_CITY_AGGRESSION: i32 = 2000;

/// The f32 `+ 4` added inside [`tech_price`].
pub const TECH_PRICE_BIAS: f32 = 4.0;
/// Literacy multiplier, exact bits `0x3F2AAA3B`.
pub const LITERACY_PRICE_FACTOR: f32 = f32::from_bits(0x3F2A_AA3B);
/// Delay passed to the increase-currency action from the default start-turn body.
pub const TURN_CURRENCY_DELAY: i32 = 10;

/// One rewards row: population, currency and score.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Reward {
    pub population: i32,
    pub currency: i32,
    pub score: i32,
}

/// Sum of every `currency`, negatives included.
pub fn currency_reward(rewards: &[Reward]) -> i32 {
    rewards.iter().map(|reward| reward.currency).sum()
}

/// Sum of every `population`.
pub fn population_reward(rewards: &[Reward]) -> i32 {
    rewards.iter().map(|reward| reward.population).sum()
}

/// Each score is clamped to `>= 0`, then summed.
pub fn score_reward(rewards: &[Reward]) -> i32 {
    rewards.iter().map(|reward| if reward.score > 0 { reward.score } else { 0 }).sum()
}

/// Raw sum, unlike [`score_reward`].
pub fn score_reward_raw(rewards: &[Reward]) -> i32 {
    rewards.iter().map(|reward| reward.score).sum()
}

/// The currency cost of an improvement is its `cost` field.
/// A failed data lookup returns 0; pass `0` for that case.
pub fn improvement_currency_cost(cost: i32) -> i32 {
    cost
}

/// Whether the improvement is doubled.
///
/// Version `<= 101` doubles when any neighbor is a port.
/// Version `> 101` also requires that port's owner to equal the building's owner.
/// The building must have the Doubled improvement ability (22).
pub fn income_is_doubled(
    version: i32,
    has_doubled_ability: bool,
    neighbor_is_port: bool,
    neighbor_same_owner: bool,
) -> bool {
    if !has_doubled_ability || !neighbor_is_port {
        return false;
    }
    version <= VERSION_PORT_OWNER_MATTERS || neighbor_same_owner
}

/// Non-city branch of [`calculate_work`].
///
/// `work > 0` becomes `max(work * level, 0)` with a wrapping 32-bit multiply.
/// `work <= 0` is left unchanged (no multiply, no clamp). A doubled building
/// then shifts the result left by 1.
pub fn non_city_work(work: i32, level: i32, doubled: bool) -> i32 {
    let mut result = work;
    if work > 0 {
        let product = work.wrapping_mul(level);
        result = if product > 0 { product } else { 0 };
    }
    if doubled {
        result = result.wrapping_shl(1);
    }
    result
}

/// City branch of [`calculate_work`].
///
/// `production + level - 1`, plus `handicap` and the pre-summed embassy income
/// when the tile is the player's start tile, plus `xp` only when `xp < 0`,
/// then `max(result, 0)`. Cities are not doubled.
pub fn city_work(production: i32, level: i32, xp: i16, is_start_tile: bool, handicap: i32, embassy_income: i32) -> i32 {
    let mut income = production.wrapping_add(level).wrapping_sub(1);
    if is_start_tile {
        income = income.wrapping_add(handicap).wrapping_add(embassy_income);
    }
    if xp < 0 {
        income = income.wrapping_add(i32::from(xp));
    }
    if income > 0 {
        income
    } else {
        0
    }
}

/// The work (income) of a tile at an improvement level.
///
/// An unowned city (`owner == 0`) uses the non-city formula. A city with a unit
/// whose owner differs from the tile owner yields 0. From version 94, the
/// `robbed` effect also yields 0.
pub fn calculate_work(input: &WorkInput) -> i32 {
    let city = input.is_city && input.owner != 0;
    if !city {
        return non_city_work(input.work, input.level, input.doubled);
    }
    if input.occupied_by_other_unit {
        return 0;
    }
    if input.version >= VERSION_ROBBED_BLOCKS_CITY && input.robbed {
        return 0;
    }
    city_work(input.production, input.level, input.xp, input.is_start_tile, input.handicap, input.embassy_income)
}

/// Arguments for [`calculate_work`]. Fields the shared [`crate::Tile`] does not carry.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct WorkInput {
    pub is_city: bool,
    pub work: i32,
    pub level: i32,
    pub doubled: bool,
    pub production: i32,
    pub xp: i16,
    pub owner: u8,
    pub occupied_by_other_unit: bool,
    pub version: i32,
    pub robbed: bool,
    pub is_start_tile: bool,
    pub handicap: i32,
    pub embassy_income: i32,
}

impl Default for WorkInput {
    fn default() -> Self {
        Self {
            is_city: false,
            work: 0,
            level: 1,
            doubled: false,
            production: 0,
            xp: 0,
            owner: 1,
            occupied_by_other_unit: false,
            version: VERSION_UNLOCKABLE_DEFAULT + 1,
            robbed: false,
            is_start_tile: false,
            handicap: 0,
            embassy_income: 0,
        }
    }
}

/// Whether the tile has an opponent unit: a unit exists, is not Nature (`255`),
/// and is not owned by `player_id`.
pub fn has_opponent_unit(unit_owner: Option<u8>, player_id: u8) -> bool {
    match unit_owner {
        Some(owner) => owner != 0xFF && owner != player_id,
        None => false,
    }
}

/// One tile passed to [`calculate_income_for`]. Only tiles with an improvement are summed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct IncomeTile {
    pub has_improvement: bool,
    pub owner: u8,
    /// The tile's `capital_of` player. Embassy income is read from another player's capital.
    pub capital_of: u8,
    pub unit_owner: Option<u8>,
    pub work: WorkInput,
    /// Embassy stars this tile pays the queried player when it is someone else's capital.
    pub embassy_for_queried_player: i32,
}

/// The four-argument work calculation, the per-tile step of [`calculate_income_for`].
///
/// Own tiles use [`calculate_work`]. Another player's capital with no opponent unit
/// pays [`IncomeTile::embassy_for_queried_player`]. Every other tile pays 0.
pub fn work_for_player(tile: &IncomeTile, player_id: u8) -> i32 {
    if !tile.has_improvement {
        return 0;
    }
    if tile.owner == player_id {
        let mut work = tile.work;
        work.owner = tile.owner;
        return calculate_work(&work);
    }
    if tile.owner != 0 && tile.capital_of == tile.owner {
        if has_opponent_unit(tile.unit_owner, tile.owner) {
            return 0;
        }
        return tile.embassy_for_queried_player;
    }
    0
}

/// The income of a player over a set of tiles.
///
/// Sums [`work_for_player`] over every tile. The game skips tiles with a null
/// improvement before the call; [`work_for_player`] returns 0 for those.
pub fn calculate_income_for(tiles: &[IncomeTile], player_id: u8) -> i32 {
    tiles.iter().map(|tile| work_for_player(tile, player_id)).sum()
}

/// Income from an embassy.
///
/// `embassy_level <= 0` or `at_war` yields 0. Peace with a non-zero opponent id
/// multiplies by 2. `income_per_level` is the embassy income per level from the diplomacy data.
pub fn embassy_income(
    other_player_id: u8,
    relation_state: i32,
    embassy_level: i32,
    at_war: bool,
    income_per_level: i32,
) -> i32 {
    if embassy_level <= 0 || at_war {
        return 0;
    }
    let multiplier = if other_player_id != 0 && relation_state == PEACE_STATE { 2 } else { 1 };
    income_per_level.wrapping_mul(multiplier).wrapping_mul(embassy_level)
}

/// [`crate::rules::diplomacy::has_war_with`], as called from embassy income.
///
/// Peace on the first player's relation returns false before the attack-turn
/// checks. An attack on the current turn (`turns_since <= 0`) counts. An attack
/// exactly one turn ago counts only when that attacker's player index is still
/// at or after `current_player_index`.
pub fn has_war_with(attack: &RecentAttack) -> bool {
    if attack.opponent_id != 0 && attack.relation_state == PEACE_STATE {
        return false;
    }
    if attack.turns_since_self_attacked <= 0 {
        return true;
    }
    if attack.turns_since_self_attacked == 1 && attack.self_index >= attack.current_player_index {
        return true;
    }
    if attack.turns_since_opponent_attacked <= 0 {
        return true;
    }
    if attack.turns_since_opponent_attacked != 1 {
        return false;
    }
    attack.opponent_index >= attack.current_player_index
}

/// Turn deltas for [`has_war_with`]. `turns_since_*` is the current turn minus the last attack turn.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RecentAttack {
    pub opponent_id: u8,
    pub relation_state: i32,
    pub turns_since_self_attacked: i32,
    pub self_index: i32,
    pub turns_since_opponent_attacked: i32,
    pub opponent_index: i32,
    pub current_player_index: i32,
}

/// Improvement age: `current_turn - founded`.
pub fn improvement_age(current_turn: u32, founded: u16) -> i32 {
    (current_turn as i32).wrapping_sub(i32::from(founded))
}

/// Whether the improvement levels up: signed `xp` is greater than zero-extended `level`.
pub fn should_level_up(xp: i16, level: u16) -> bool {
    i32::from(xp) > i32::from(level)
}

/// Attract growth test in the default start-turn body.
///
/// The game does a signed integer division by `growth_rate`. A zero rate is a
/// divide-by-zero fault, so this returns `None` for that input. Otherwise it is
/// `age % growth_rate == 0`.
pub fn attract_growth_due(age: i32, growth_rate: i32) -> Option<bool> {
    if growth_rate == 0 {
        return None;
    }
    Some(age % growth_rate == 0)
}

/// Currency queued by the start-turn action when work is positive (increase-currency action).
pub fn turn_currency_grant(work: i32) -> i32 {
    if work > 0 {
        work
    } else {
        0
    }
}

/// Numeric effect of adding score: 32-bit `score += delta`.
///
/// When `score_limit > 0`, the new score is at least that limit, and
/// the game state is `Started`, the game also pushes an end-game action.
/// [`score_reaches_limit`] reports that condition. Analytics for human players
/// are not ported.
pub fn add_score(score: u32, delta: i32) -> u32 {
    score.wrapping_add(delta as u32)
}

/// Numeric effect of removing score: 32-bit `score -= delta`.
pub fn remove_score(score: u32, delta: i32) -> u32 {
    score.wrapping_sub(delta as u32)
}

/// Win check inside [`add_score`]. `score_limit <= 0` never fires.
pub fn score_reaches_limit(score: u32, score_limit: i32, state: u8) -> bool {
    score_limit > 0 && score >= score_limit as u32 && state == GAME_STATE_STARTED
}

/// Apply [`add_score`] to a [`PlayerState`]. Returns whether the limit fired.
pub fn apply_score(player: &mut PlayerState, delta: i32, score_limit: i32, state: u8) -> bool {
    player.score = add_score(player.score, delta);
    score_reaches_limit(player.score, score_limit, state)
}

/// Number of stars for a score.
///
/// Returns the first index whose limit is strictly greater than `score`.
/// If `score` has reached every limit, returns `limits.len()`. Comparison is
/// the game's signed 64-bit compare of a sign-extended score against a
/// sign-extended 32-bit limit.
pub fn number_of_stars(score: i32, limits: &[u32]) -> i32 {
    for (index, limit) in limits.iter().enumerate() {
        let limit = *limit as i32 as i64;
        if (score as i64) < limit {
            return index as i32;
        }
    }
    limits.len() as i32
}

/// Perfection (game mode 1) reads `score_limits`. Every other mode reads `rating_limits`.
pub fn star_limit_table<'a>(game_mode: i32, rating_limits: &'a [u32], score_limits: &'a [u32]) -> &'a [u32] {
    if game_mode == PERFECTION_MODE {
        score_limits
    } else {
        rating_limits
    }
}

/// Tech score: `(float)(cost * 100)`.
pub fn tech_score(cost: i32) -> f32 {
    (cost.wrapping_mul(100)) as f32
}

/// Truncation used when [`learn_tech`] turns that float into a score delta.
pub fn tech_score_points(cost: i32) -> i32 {
    tech_score(cost) as i32
}

/// Unit score.
///
/// Version `> 100` multiplies the full cost (the unit's data cost plus a
/// passenger's data cost) by 5. Older versions multiply the unit's own
/// data cost by 5.
pub fn unit_score(version: i32, own_cost: i32, full_cost: i32) -> i32 {
    let cost = if version > VERSION_UNIT_SCORE_FULL_COST { full_cost } else { own_cost };
    cost.wrapping_mul(5)
}

/// Full unit cost: looked-up cost, plus passenger cost when present.
pub fn full_unit_cost(unit_cost: i32, passenger_cost: Option<i32>) -> i32 {
    unit_cost.wrapping_add(passenger_cost.unwrap_or(0))
}

/// Result of [`learn_tech`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LearnTechEffect {
    pub applied: bool,
    pub currency: i32,
    pub score: u32,
    pub score_delta: i32,
    pub unlocked: Vec<i32>,
    pub enabled_tasks: Vec<i32>,
    pub reveals_capitals: bool,
    pub checks_genius: bool,
}

/// Learn a tech.
///
/// If `tech_id` is already in `unlocked`, nothing changes. Otherwise the id is
/// appended, `price_paid` is subtracted from currency, and score gains
/// `trunc(tech_data_cost * 100)`. The game can defer that score change into an
/// action; the numeric delta is identical.
///
/// `enabled_tasks` are the idx values of the tech's task unlocks. Capital vision
/// (ability 8) sets `reveals_capitals`. A successful learn always checks the
/// Genius task.
pub fn learn_tech(
    player: &PlayerState,
    unlocked: &[i32],
    tech_id: i32,
    price_paid: i32,
    tech_data_cost: i32,
    task_ids: &[i32],
    ability_ids: &[i32],
) -> LearnTechEffect {
    if unlocked.contains(&tech_id) {
        return LearnTechEffect {
            applied: false,
            currency: player.currency,
            score: player.score,
            score_delta: 0,
            unlocked: unlocked.to_vec(),
            enabled_tasks: Vec::new(),
            reveals_capitals: false,
            checks_genius: false,
        };
    }
    let score_delta = tech_score_points(tech_data_cost);
    let mut unlocked = unlocked.to_vec();
    unlocked.push(tech_id);
    LearnTechEffect {
        applied: true,
        currency: player.currency.wrapping_sub(price_paid),
        score: add_score(player.score, score_delta),
        score_delta,
        unlocked,
        enabled_tasks: task_ids.to_vec(),
        reveals_capitals: ability_ids.contains(&CAPITAL_VISION_ABILITY),
        checks_genius: true,
    }
}

/// The price of a tech.
///
/// A zero base cost returns 0. Otherwise
/// `ceil((cities - 1) * cost + cost + 4)`, with the multiply wrapping in 32
/// bits before the float conversion. Literacy multiplies by [`LITERACY_PRICE_FACTOR`]
/// before `ceilf`.
pub fn tech_price(base_cost: i32, city_count: i32, has_literacy: bool) -> i32 {
    if base_cost == 0 {
        return 0;
    }
    let scaled = city_count.wrapping_sub(1).wrapping_mul(base_cost);
    let mut price = (scaled as f32) + (base_cost as f32) + TECH_PRICE_BIAS;
    if has_literacy {
        price *= LITERACY_PRICE_FACTOR;
    }
    price.ceil() as i32
}

/// One tribe tech, already replaced by the tribe's override when one exists.
///
/// List fields match the tech data: `tech_unlocks`, `improvement_unlocks`,
/// `unit_unlocks`, `ability_unlocks`, `task_unlocks` and the keys of
/// `movement_unlocks`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TechNode {
    pub id: i32,
    pub tech_ids: Vec<i32>,
    pub improvement_ids: Vec<i32>,
    pub unit_ids: Vec<i32>,
    pub ability_ids: Vec<i32>,
    pub task_ids: Vec<i32>,
    pub movement_terrains: Vec<i32>,
}

/// Which kind of unlock the required-tech lookup is asked for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RequiredTechKind {
    Tech,
    Improvement,
    Unit,
    Ability,
    Task,
    /// `movement_unlocks` contains the terrain id. Defence-bonus keys are not this overload.
    TerrainMovement,
}

/// First tech in `techs` whose matching unlock list contains `target`.
///
/// This is the shared shape of the required-tech lookups. Ability entries are
/// the enum values themselves. The other object
/// lists compare `idx`. Terrain uses a dictionary key lookup on
/// `movement_unlocks`.
pub fn required_tech(techs: &[TechNode], kind: RequiredTechKind, target: i32) -> Option<i32> {
    techs
        .iter()
        .find(|tech| match kind {
            RequiredTechKind::Tech => tech.tech_ids.contains(&target),
            RequiredTechKind::Improvement => tech.improvement_ids.contains(&target),
            RequiredTechKind::Unit => tech.unit_ids.contains(&target),
            RequiredTechKind::Ability => tech.ability_ids.contains(&target),
            RequiredTechKind::Task => tech.task_ids.contains(&target),
            RequiredTechKind::TerrainMovement => tech.movement_terrains.contains(&target),
        })
        .map(|tech| tech.id)
}

/// The unlockable tech for `version > 113`.
///
/// `tribe == 0` yields nothing. Otherwise, walking the tribe tech list:
/// an owned tech contributes each direct `tech_unlocks` child that is not owned;
/// an unowned tech is itself contributed when any direct child is owned.
/// Duplicates are dropped. Tribe overrides must already be applied to `techs`.
///
/// Version `<= 113` is [`unlockable_tech_v113`]. [`unlockable_tech_for_version`]
/// selects the arm.
pub fn unlockable_tech(tribe: i32, techs: &[TechNode], unlocked: &[i32]) -> Vec<i32> {
    if tribe == 0 {
        return Vec::new();
    }
    let mut owned = IdSet::default();
    owned.extend(unlocked);
    let mut added = IdSet::default();
    let mut result = Vec::new();
    for tech in techs {
        if owned.contains(tech.id) {
            for &child in &tech.tech_ids {
                if !owned.contains(child) && added.insert(child) {
                    result.push(child);
                }
            }
        } else if tech.tech_ids.iter().any(|&child| owned.contains(child)) && added.insert(tech.id) {
            result.push(tech.id);
        }
    }
    result
}

/// A set of non-negative ids as a bit set; ids outside `0..256` fall back to a list.
#[derive(Default)]
pub(crate) struct IdSet {
    bits: [u64; 4],
    other: Vec<i32>,
}

impl IdSet {
    fn slot(id: i32) -> Option<(usize, u64)> {
        let i = usize::try_from(id).ok().filter(|&i| i < 256)?;
        Some((i / 64, 1u64 << (i % 64)))
    }

    pub(crate) fn contains(&self, id: i32) -> bool {
        match Self::slot(id) {
            Some((w, b)) => self.bits[w] & b != 0,
            None => self.other.contains(&id),
        }
    }

    /// Adds `id`; whether it was new.
    pub(crate) fn insert(&mut self, id: i32) -> bool {
        match Self::slot(id) {
            Some((w, b)) => {
                let new = self.bits[w] & b == 0;
                self.bits[w] |= b;
                new
            }
            None if self.other.contains(&id) => false,
            None => {
                self.other.push(id);
                true
            }
        }
    }

    pub(crate) fn extend(&mut self, ids: &[i32]) {
        for &id in ids {
            self.insert(id);
        }
    }
}

/// Why [`unlockable_tech_v113`] did not return a list.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UnlockableFault {
    /// `player.tribe == 0`. The game logs and throws.
    TribeNone,
    /// Tribe id was not in the game logic data. The game returns a null list.
    TribeMissing,
}

/// The unlockable tech for `version <= 113`.
///
/// Walks `unlocked` (the player's owned techs) in order. Each owned tech, already
/// replaced by its tribe override, contributes `tech_unlocks` children
/// that are not owned. `child_overrides` is the tribe's tech overrides: a child
/// idx present there is replaced before the owned check. Duplicates are dropped
/// by id. The game's list lookup is reference equality; one tech
/// object exists per id, so id equality matches.
///
/// This does not add an unowned parent because a child is owned. That rule is
/// only in the `version > 113` arm.
pub fn unlockable_tech_v113(
    tribe: i32,
    tribe_known: bool,
    techs: &[TechNode],
    unlocked: &[i32],
    child_overrides: &BTreeMap<i32, i32>,
) -> Result<Vec<i32>, UnlockableFault> {
    if tribe == 0 {
        return Err(UnlockableFault::TribeNone);
    }
    if !tribe_known {
        return Err(UnlockableFault::TribeMissing);
    }
    let mut result = Vec::new();
    for owned in unlocked {
        let Some(tech) = techs.iter().find(|tech| tech.id == *owned) else {
            continue;
        };
        for child in &tech.tech_ids {
            let child = child_overrides.get(child).copied().unwrap_or(*child);
            if unlocked.contains(&child) || result.contains(&child) {
                continue;
            }
            result.push(child);
        }
    }
    Ok(result)
}

/// The unlockable tech by version. `version <= 113` uses [`unlockable_tech_v113`].
/// Newer versions use [`unlockable_tech`] and ignore `tribe_known` and `child_overrides`
/// because those overrides are already applied to `techs`.
pub fn unlockable_tech_for_version(
    version: i32,
    tribe: i32,
    tribe_known: bool,
    techs: &[TechNode],
    unlocked: &[i32],
    child_overrides: &BTreeMap<i32, i32>,
) -> Result<Vec<i32>, UnlockableFault> {
    if version <= VERSION_UNLOCKABLE_DEFAULT {
        unlockable_tech_v113(tribe, tribe_known, techs, unlocked, child_overrides)
    } else {
        Ok(unlockable_tech(tribe, techs, unlocked))
    }
}

/// One map tile passed to [`start_turn_currency`].
///
/// `own_work` is the three-argument work calculation after the improvement level update.
/// `foreign_work` is the four-argument work calculation using the
/// stored level, or 0 when the tile has no improvement.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct StartTurnTile {
    pub owner: u8,
    pub has_improvement: bool,
    /// The data lookup succeeded for that improvement. Own-tile currency requires it.
    pub improvement_known: bool,
    pub own_work: i32,
    pub foreign_work: i32,
}

/// Currency amounts queued by the start-turn action.
///
/// `version <= 59` is the V59 body: only the acting player's own
/// tiles, and only when `turn != 0`, the tile has an improvement, and its data
/// resolved. Each positive [`StartTurnTile::own_work`] is one
/// increase-currency action with delay [`TURN_CURRENCY_DELAY`].
///
/// `version > 59` is the default body. Own tiles use the same
/// gate. Other tiles add [`StartTurnTile::foreign_work`] when it is positive,
/// including on turn 0, with or without an improvement.
///
/// Also queued, not summed here: a create-resource action (Game, reason Attract)
/// when Attract's `age % growth_rate == 0` finds a same-owner forest neighbor
/// with no improvement and no resource; a city level-up action when
/// [`should_level_up`] and the tile owner is the current player.
/// The V59 body also changes aggression (see [`v59_known_player_aggression`])
/// and does not run the diplomacy check. Both arms check the tasks
/// Pacifist (1) and Explorer (7), then the tribe connection.
pub fn start_turn_currency(version: i32, turn: u32, player_id: u8, tiles: &[StartTurnTile]) -> Vec<i32> {
    let mut grants = Vec::new();
    for tile in tiles {
        let amount = if tile.owner == player_id {
            if turn == 0 || !tile.has_improvement || !tile.improvement_known {
                continue;
            }
            tile.own_work
        } else if version > VERSION_START_TURN_DEFAULT {
            tile.foreign_work
        } else {
            continue;
        };
        if amount > 0 {
            grants.push(amount);
        }
    }
    grants
}

/// V59 start-turn aggression toward a known player.
///
/// Unsigned compare of the two `score` fields: strictly greater yields -100,
/// otherwise 100.
pub fn v59_known_player_aggression(self_score: u32, other_score: u32) -> i32 {
    if self_score > other_score {
        V59_AHEAD_AGGRESSION
    } else {
        V59_BEHIND_AGGRESSION
    }
}

/// V59 start-turn aggression toward a player the actor does not know.
///
/// `handicap * 100 - 200`, with a wrapping 32-bit multiply and add.
pub fn v59_unknown_player_aggression(handicap: i32) -> i32 {
    handicap.wrapping_mul(100).wrapping_add(V59_UNKNOWN_AGGRESSION_BIAS)
}

/// Whether a tile is frozen.
///
/// Terrain Ice (6) is frozen. Otherwise Polaris climate (16) is frozen when
/// the terrain is neither Water (1) nor Ocean (2). The second test is the
/// unsigned compare `(terrain - 3) < 0xFFFFFFFE`.
pub fn tile_is_frozen(terrain: i32, climate: i32) -> bool {
    if terrain == ICE_TERRAIN {
        return true;
    }
    climate == POLARIS_CLIMATE && (terrain.wrapping_sub(3) as u32) < 0xFFFF_FFFE
}

/// How many tiles [`tile_is_frozen`] accepts.
///
/// A null tile throws in the game. The count is a wrapping 32-bit sum of 0/1.
pub fn count_frozen_tiles(tiles: &[(i32, i32)]) -> i32 {
    let mut count = 0i32;
    for (terrain, climate) in tiles {
        if tile_is_frozen(*terrain, *climate) {
            count = count.wrapping_add(1);
        }
    }
    count
}

/// The Ice Bank level.
///
/// `floor(frozen_tile_count as f32 / 20)` then the minimum with `max_level` in
/// double, truncated toward zero. `frozen_tile_count` is [`count_frozen_tiles`].
/// `max_level` is the Ice Bank's `max_level`.
pub fn ice_bank_level(frozen_tile_count: i32, max_level: i32) -> i32 {
    let steps = ((frozen_tile_count as f32) / ICE_BANK_TILES_PER_LEVEL).floor() as f64;
    let cap = max_level as f64;
    // `Math.Min`: a NaN first operand stays NaN; otherwise the smaller value.
    let chosen = if steps.is_nan() || steps < cap { steps } else { cap };
    chosen as i32
}

/// The maximum level of an improvement.
///
/// The data `max_level`, plus 1 when `idx` is Fungi (37) and the owner has
/// Microbes (14). `has_microbes` is that bool.
pub fn max_improvement_level(idx: i32, max_level: i32, has_microbes: bool) -> i32 {
    if idx == FUNGI_IMPROVEMENT && has_microbes {
        max_level.wrapping_add(1)
    } else {
        max_level
    }
}

/// Inputs for [`calculate_improvement_level`].
///
/// `adjacency_bonus` is `Some` only when `adjacency_improvements` is non-null
/// and its count is `> 0`. The value is the adjacency bonus at the tile.
/// That walk (area of radius 1, Picky 21, Simple 27, same-owner neighbors) is
/// not reimplemented here.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ImprovementLevelInput {
    pub version: i32,
    pub has_improvement: bool,
    pub improvement_type: i32,
    /// The improvement data `idx`. Fungi's microbes bump reads this, not the type field.
    pub idx: i32,
    pub stored_level: u16,
    /// `None` when the tile's explorers list is missing. Otherwise the list count.
    pub explorer_count: Option<i32>,
    pub data_found: bool,
    pub has_patina: bool,
    pub age: i32,
    pub growth_rate: i32,
    pub max_level: i32,
    pub adjacency_bonus: Option<i32>,
    pub frozen_tile_count: i32,
    pub has_microbes: bool,
}

/// The level of an improvement.
///
/// `version <= 103` uses the older body (`calculate_improvement_level_v104`).
/// `None` is a runtime fault in the game: missing improvement data on that old arm, or a
/// zero `growth_rate` on a Patina divide. The new arm returns 1 when data is
/// missing instead of throwing.
pub fn calculate_improvement_level(input: &ImprovementLevelInput) -> Option<i32> {
    if input.version <= VERSION_IMPROVEMENT_LEVEL {
        calculate_improvement_level_v104(input)
    } else {
        calculate_improvement_level_current(input)
    }
}

fn calculate_improvement_level_v104(input: &ImprovementLevelInput) -> Option<i32> {
    if !input.has_improvement {
        return Some(1);
    }
    if input.improvement_type == LIGHTHOUSE_IMPROVEMENT {
        return Some(match input.explorer_count {
            None => 1,
            Some(count) => count.wrapping_add(1),
        });
    }
    if input.improvement_type == CITY_IMPROVEMENT_INDEX {
        return Some(i32::from(input.stored_level));
    }
    if !input.data_found {
        return None;
    }
    if input.improvement_type == ICE_BANK_IMPROVEMENT {
        return Some(ice_bank_level(input.frozen_tile_count, input.max_level));
    }
    if input.max_level == 0 {
        return Some(i32::from(input.stored_level));
    }
    if input.has_patina {
        let quot = idiv_growth(input.age, input.growth_rate)?;
        return Some(min_i32(quot, input.max_level));
    }
    if let Some(bonus) = input.adjacency_bonus {
        return Some(bonus);
    }
    Some(i32::from(input.stored_level))
}

fn calculate_improvement_level_current(input: &ImprovementLevelInput) -> Option<i32> {
    if !input.has_improvement {
        return Some(1);
    }
    let mut level = if input.improvement_type == LIGHTHOUSE_IMPROVEMENT {
        input.explorer_count.unwrap_or(0).wrapping_add(1)
    } else {
        i32::from(input.stored_level)
    };
    if !input.data_found {
        return Some(1);
    }
    if input.has_patina {
        level = idiv_growth(input.age, input.growth_rate)?;
    }
    if let Some(bonus) = input.adjacency_bonus {
        level = bonus;
    }
    if input.improvement_type == ICE_BANK_IMPROVEMENT {
        level = ice_bank_level(input.frozen_tile_count, input.max_level);
    }
    if input.max_level <= 0 {
        return Some(level);
    }
    let limit = max_improvement_level(input.idx, input.max_level, input.has_microbes);
    Some(min_i32(level, limit))
}

/// Signed integer division. A zero divisor is the game's fault, reported as `None`.
fn idiv_growth(age: i32, growth_rate: i32) -> Option<i32> {
    if growth_rate == 0 {
        None
    } else {
        Some(age / growth_rate)
    }
}

/// Integer minimum: signed `<`, equal keeps the second.
fn min_i32(left: i32, right: i32) -> i32 {
    if left < right {
        left
    } else {
        right
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn player() -> PlayerState {
        PlayerState {
            id: 1,
            handicap: 3,
            currency: 10,
            score: 40,
            tribe: 7,
            auto_play: false,
            start_tile: crate::WorldCoordinates { x: 1, y: 2 },
        }
    }

    #[test]
    fn reward_sums_match_the_list_helpers() {
        let rewards = [
            Reward { population: 2, currency: 1, score: 5 },
            Reward { population: -1, currency: -4, score: -3 },
            Reward { population: 0, currency: 0, score: 0 },
        ];
        assert_eq!(currency_reward(&rewards), -3);
        assert_eq!(population_reward(&rewards), 1);
        assert_eq!(score_reward(&rewards), 5);
        assert_eq!(score_reward_raw(&rewards), 2);
        assert_eq!(currency_reward(&[]), 0);
        assert_eq!(improvement_currency_cost(5), 5);
    }

    #[test]
    fn non_city_work_multiplies_clamps_and_shifts() {
        assert_eq!(non_city_work(2, 3, false), 6);
        assert_eq!(non_city_work(2, 3, true), 12);
        assert_eq!(non_city_work(2, 0, false), 0);
        assert_eq!(non_city_work(2, -1, true), 0);
        assert_eq!(non_city_work(-3, 4, false), -3);
        assert_eq!(non_city_work(-3, 4, true), -6);
        assert_eq!(non_city_work(0, 4, true), 0);
    }

    #[test]
    fn doubling_follows_the_version_split() {
        assert!(!income_is_doubled(120, false, true, true));
        assert!(income_is_doubled(101, true, true, false));
        assert!(!income_is_doubled(102, true, true, false));
        assert!(income_is_doubled(102, true, true, true));
    }

    #[test]
    fn city_work_adds_handicap_on_the_start_tile_and_only_negative_xp() {
        assert_eq!(city_work(4, 3, 9, false, 5, 8), 6);
        assert_eq!(city_work(4, 3, -2, false, 5, 8), 4);
        assert_eq!(city_work(4, 3, -2, true, 5, 8), 17);
        assert_eq!(city_work(0, 1, -5, false, 0, 0), 0);
    }

    #[test]
    fn city_gates_zero_captured_and_robbed_cities() {
        let mut input = WorkInput { is_city: true, production: 2, level: 2, owner: 0, work: 3, ..WorkInput::default() };
        assert_eq!(calculate_work(&input), 6);
        input.owner = 1;
        input.occupied_by_other_unit = true;
        assert_eq!(calculate_work(&input), 0);
        input.occupied_by_other_unit = false;
        input.robbed = true;
        input.version = 94;
        assert_eq!(calculate_work(&input), 0);
        input.version = 93;
        assert_eq!(calculate_work(&input), 3);
    }

    #[test]
    fn income_for_sums_own_work_and_foreign_capital_embassies() {
        let own = IncomeTile {
            has_improvement: true,
            owner: 1,
            capital_of: 1,
            unit_owner: None,
            work: WorkInput { is_city: false, work: 2, level: 2, owner: 1, ..WorkInput::default() },
            embassy_for_queried_player: 99,
        };
        let foreign_capital = IncomeTile {
            has_improvement: true,
            owner: 2,
            capital_of: 2,
            unit_owner: Some(2),
            work: WorkInput::default(),
            embassy_for_queried_player: 4,
        };
        let blocked = IncomeTile { unit_owner: Some(1), embassy_for_queried_player: 4, ..foreign_capital };
        let empty = IncomeTile { has_improvement: false, ..own };
        assert_eq!(calculate_income_for(&[own, foreign_capital, blocked, empty], 1), 8);
        assert!(has_opponent_unit(Some(1), 2));
        assert!(!has_opponent_unit(Some(0xFF), 2));
        assert!(!has_opponent_unit(None, 2));
    }

    #[test]
    fn embassy_income_doubles_on_peace_and_dies_at_war() {
        assert_eq!(embassy_income(2, PEACE_STATE, 3, false, 2), 12);
        assert_eq!(embassy_income(2, 0, 3, false, 2), 6);
        assert_eq!(embassy_income(0, PEACE_STATE, 3, false, 2), 6);
        assert_eq!(embassy_income(2, PEACE_STATE, 0, false, 2), 0);
        assert_eq!(embassy_income(2, PEACE_STATE, 3, true, 2), 0);
    }

    #[test]
    fn recent_attacks_count_as_war_until_the_turn_order_passes() {
        let base = RecentAttack {
            opponent_id: 2,
            relation_state: 0,
            turns_since_self_attacked: 5,
            self_index: 0,
            turns_since_opponent_attacked: 5,
            opponent_index: 1,
            current_player_index: 1,
        };
        assert!(!has_war_with(&base));
        assert!(!has_war_with(&RecentAttack { relation_state: PEACE_STATE, turns_since_self_attacked: 0, ..base }));
        assert!(has_war_with(&RecentAttack { turns_since_self_attacked: 0, ..base }));
        assert!(has_war_with(&RecentAttack { turns_since_self_attacked: 1, self_index: 1, ..base }));
        assert!(!has_war_with(&RecentAttack { turns_since_self_attacked: 1, self_index: 0, ..base }));
    }

    #[test]
    fn score_wraps_and_stars_walk_the_supplied_limits() {
        assert_eq!(add_score(1, -2), u32::MAX);
        assert_eq!(remove_score(1, 2), u32::MAX);
        assert!(score_reaches_limit(10, 10, GAME_STATE_STARTED));
        assert!(!score_reaches_limit(9, 10, GAME_STATE_STARTED));
        assert!(!score_reaches_limit(10, 0, GAME_STATE_STARTED));
        assert!(!score_reaches_limit(10, 10, 1));
        let rating = [10, 20, 30];
        let score = [100, 200];
        assert_eq!(number_of_stars(0, &rating), 0);
        assert_eq!(number_of_stars(10, &rating), 1);
        assert_eq!(number_of_stars(30, &rating), 3);
        assert_eq!(star_limit_table(PERFECTION_MODE, &rating, &score), &score[..]);
        assert_eq!(star_limit_table(2, &rating, &score), &rating[..]);
        assert_eq!(TILE_VALUE, 20);
        assert_eq!(CITY_LEVEL_SCORE, 50);
        assert_eq!(CITY_STARVATION_PENALTY, 100);
        assert_eq!(CITY_XP_SCORE, 5);
        assert_eq!(EXPLORE_VALUE, 5);
    }

    #[test]
    fn unit_and_tech_scores_scale_by_the_proven_factors() {
        assert_eq!(unit_score(100, 3, 9), 15);
        assert_eq!(unit_score(101, 3, 9), 45);
        assert_eq!(full_unit_cost(4, Some(1)), 5);
        assert_eq!(full_unit_cost(4, None), 4);
        assert_eq!(tech_score_points(6), 600);
        assert_eq!(tech_score(6), 600.0);
    }

    #[test]
    fn learn_tech_spends_the_paid_price_and_scores_the_data_cost() {
        let effect = learn_tech(&player(), &[1], 4, 7, 5, &[3], &[CAPITAL_VISION_ABILITY]);
        assert!(effect.applied);
        assert_eq!(effect.currency, 3);
        assert_eq!(effect.score_delta, 500);
        assert_eq!(effect.score, 540);
        assert_eq!(effect.unlocked, vec![1, 4]);
        assert_eq!(effect.enabled_tasks, vec![3]);
        assert!(effect.reveals_capitals);
        assert!(effect.checks_genius);
        let again = learn_tech(&player(), &[4], 4, 7, 5, &[3], &[]);
        assert!(!again.applied);
        assert_eq!(again.currency, 10);
        assert_eq!(again.score, 40);
    }

    #[test]
    fn tech_price_adds_four_and_applies_the_literacy_factor() {
        assert_eq!(tech_price(0, 4, false), 0);
        assert_eq!(tech_price(5, 1, false), 9);
        assert_eq!(tech_price(5, 3, false), 19);
        let literate = tech_price(5, 3, true);
        let raw = ((3 - 1) * 5 + 5) as f32 + 4.0;
        assert_eq!(literate, (raw * LITERACY_PRICE_FACTOR).ceil() as i32);
        assert!(literate < 19);
    }

    #[test]
    fn tech_graph_required_and_unlockable_follow_the_lists() {
        let techs = vec![
            TechNode {
                id: 8,
                tech_ids: vec![6],
                improvement_ids: vec![5],
                unit_ids: vec![],
                ability_ids: vec![LITERACY_ABILITY],
                task_ids: vec![],
                movement_terrains: vec![1],
            },
            TechNode {
                id: 6,
                tech_ids: vec![9],
                improvement_ids: vec![],
                unit_ids: vec![4],
                ability_ids: vec![],
                task_ids: vec![2],
                movement_terrains: vec![],
            },
        ];
        assert_eq!(required_tech(&techs, RequiredTechKind::Tech, 6), Some(8));
        assert_eq!(required_tech(&techs, RequiredTechKind::Improvement, 5), Some(8));
        assert_eq!(required_tech(&techs, RequiredTechKind::Unit, 4), Some(6));
        assert_eq!(required_tech(&techs, RequiredTechKind::Ability, LITERACY_ABILITY), Some(8));
        assert_eq!(required_tech(&techs, RequiredTechKind::Task, 2), Some(6));
        assert_eq!(required_tech(&techs, RequiredTechKind::TerrainMovement, 1), Some(8));
        assert_eq!(required_tech(&techs, RequiredTechKind::Tech, 99), None);
        assert_eq!(unlockable_tech(0, &techs, &[8]), Vec::<i32>::new());
        assert_eq!(unlockable_tech(7, &techs, &[8]), vec![6]);
        assert_eq!(unlockable_tech(7, &techs, &[8, 6]), vec![9]);
        assert_eq!(unlockable_tech(7, &techs, &[6]), vec![8, 9]);
    }

    #[test]
    fn level_up_age_and_turn_grant_are_the_start_turn_predicates() {
        assert!(should_level_up(3, 2));
        assert!(!should_level_up(2, 2));
        assert!(!should_level_up(-1, 0));
        assert_eq!(improvement_age(10, 4), 6);
        assert_eq!(attract_growth_due(8, 4), Some(true));
        assert_eq!(attract_growth_due(7, 4), Some(false));
        assert_eq!(attract_growth_due(8, 0), None);
        assert_eq!(turn_currency_grant(4), 4);
        assert_eq!(turn_currency_grant(0), 0);
        assert_eq!(turn_currency_grant(-2), 0);
    }

    fn level_input() -> ImprovementLevelInput {
        ImprovementLevelInput {
            version: 104,
            has_improvement: true,
            improvement_type: 5,
            idx: 5,
            stored_level: 4,
            explorer_count: None,
            data_found: true,
            has_patina: false,
            age: 0,
            growth_rate: 1,
            max_level: 8,
            adjacency_bonus: None,
            frozen_tile_count: 0,
            has_microbes: false,
        }
    }

    #[test]
    fn start_turn_currency_drops_foreign_tiles_at_version_59() {
        let tiles = [
            StartTurnTile { owner: 1, has_improvement: true, improvement_known: true, own_work: 4, foreign_work: 9 },
            StartTurnTile { owner: 2, has_improvement: false, improvement_known: false, own_work: 0, foreign_work: 3 },
            StartTurnTile { owner: 1, has_improvement: true, improvement_known: true, own_work: 0, foreign_work: 8 },
        ];
        assert_eq!(start_turn_currency(59, 1, 1, &tiles), vec![4]);
        assert_eq!(start_turn_currency(VERSION_START_TURN_DEFAULT, 0, 1, &tiles), Vec::<i32>::new());
        assert_eq!(start_turn_currency(60, 1, 1, &tiles), vec![4, 3]);
        assert_eq!(start_turn_currency(60, 0, 1, &tiles), vec![3]);
        assert_eq!(v59_known_player_aggression(11, 10), V59_AHEAD_AGGRESSION);
        assert_eq!(v59_known_player_aggression(10, 10), V59_BEHIND_AGGRESSION);
        assert_eq!(v59_known_player_aggression(1, 2), 100);
        assert_eq!(v59_unknown_player_aggression(3), 100);
        assert_eq!(v59_unknown_player_aggression(0), V59_UNKNOWN_AGGRESSION_BIAS);
        assert_eq!(V59_ENEMY_UNIT_AGGRESSION, 500);
        assert_eq!(V59_ENEMY_CITY_AGGRESSION, 2000);
    }

    #[test]
    fn unlockable_tech_v113_lists_only_children_of_owned_tech() {
        let techs = [
            TechNode {
                id: 8,
                tech_ids: vec![6],
                improvement_ids: vec![],
                unit_ids: vec![],
                ability_ids: vec![],
                task_ids: vec![],
                movement_terrains: vec![],
            },
            TechNode {
                id: 6,
                tech_ids: vec![9],
                improvement_ids: vec![],
                unit_ids: vec![],
                ability_ids: vec![],
                task_ids: vec![],
                movement_terrains: vec![],
            },
        ];
        let overrides = BTreeMap::new();
        assert_eq!(unlockable_tech_for_version(113, 7, true, &techs, &[6], &overrides), Ok(vec![9]));
        assert_eq!(
            unlockable_tech_for_version(VERSION_UNLOCKABLE_DEFAULT, 7, true, &techs, &[6], &overrides),
            Ok(vec![9])
        );
        assert_eq!(unlockable_tech_for_version(114, 7, true, &techs, &[6], &overrides), Ok(vec![8, 9]));
        assert_eq!(
            unlockable_tech_for_version(113, 0, true, &techs, &[6], &overrides),
            Err(UnlockableFault::TribeNone)
        );
        assert_eq!(unlockable_tech_for_version(114, 0, true, &techs, &[6], &overrides), Ok(Vec::<i32>::new()));
        assert_eq!(
            unlockable_tech_for_version(113, 7, false, &techs, &[6], &overrides),
            Err(UnlockableFault::TribeMissing)
        );
        let mut remap = BTreeMap::new();
        remap.insert(6, 99);
        assert_eq!(unlockable_tech_v113(7, true, &techs, &[8], &remap), Ok(vec![99]));
        assert_eq!(unlockable_tech_v113(7, true, &techs, &[8, 99], &remap), Ok(Vec::<i32>::new()));
    }

    #[test]
    fn improvement_level_splits_at_version_103() {
        assert!(tile_is_frozen(ICE_TERRAIN, 0));
        assert!(tile_is_frozen(3, POLARIS_CLIMATE));
        assert!(!tile_is_frozen(1, POLARIS_CLIMATE));
        assert!(!tile_is_frozen(2, POLARIS_CLIMATE));
        assert!(!tile_is_frozen(5, 4));
        assert_eq!(count_frozen_tiles(&[(6, 0), (1, 16), (5, 16), (3, 0)]), 2);
        assert_eq!(ice_bank_level(19, 8), 0);
        assert_eq!(ice_bank_level(20, 8), 1);
        assert_eq!(ice_bank_level(100, 3), 3);
        assert_eq!(ice_bank_level(100, -1), -1);
        assert_eq!(ICE_BANK_TILES_PER_LEVEL, 20.0);
        assert_eq!(max_improvement_level(FUNGI_IMPROVEMENT, 2, true), 3);
        assert_eq!(max_improvement_level(FUNGI_IMPROVEMENT, 2, false), 2);
        assert_eq!(MICROBES_ABILITY, 14);
        assert_eq!(PATINA_ABILITY, 5);

        let mut input = level_input();
        input.version = 103;
        input.improvement_type = CITY_IMPROVEMENT_INDEX;
        input.stored_level = 9;
        input.max_level = 2;
        assert_eq!(calculate_improvement_level(&input), Some(9));
        input.version = 104;
        assert_eq!(calculate_improvement_level(&input), Some(2));

        input = level_input();
        input.version = VERSION_IMPROVEMENT_LEVEL;
        input.improvement_type = LIGHTHOUSE_IMPROVEMENT;
        input.explorer_count = Some(5);
        input.adjacency_bonus = Some(4);
        input.max_level = 0;
        assert_eq!(calculate_improvement_level(&input), Some(6));
        input.version = 104;
        assert_eq!(calculate_improvement_level(&input), Some(4));

        input = level_input();
        input.version = 103;
        input.data_found = false;
        assert_eq!(calculate_improvement_level(&input), None);
        input.version = 104;
        assert_eq!(calculate_improvement_level(&input), Some(1));

        input = level_input();
        input.version = 103;
        input.has_patina = true;
        input.max_level = -1;
        input.age = 10;
        input.growth_rate = 2;
        assert_eq!(calculate_improvement_level(&input), Some(-1));
        input.version = 104;
        assert_eq!(calculate_improvement_level(&input), Some(5));

        input = level_input();
        input.version = 103;
        input.idx = FUNGI_IMPROVEMENT;
        input.has_patina = true;
        input.has_microbes = true;
        input.max_level = 2;
        input.age = 10;
        input.growth_rate = 3;
        assert_eq!(calculate_improvement_level(&input), Some(2));
        input.version = 104;
        assert_eq!(calculate_improvement_level(&input), Some(3));

        input = level_input();
        input.version = 103;
        input.adjacency_bonus = Some(9);
        input.max_level = 2;
        input.stored_level = 1;
        assert_eq!(calculate_improvement_level(&input), Some(9));
        input.version = 104;
        assert_eq!(calculate_improvement_level(&input), Some(2));

        input = level_input();
        input.improvement_type = ICE_BANK_IMPROVEMENT;
        input.frozen_tile_count = 40;
        input.max_level = 1;
        input.version = 103;
        assert_eq!(calculate_improvement_level(&input), Some(1));
        input.version = 104;
        assert_eq!(calculate_improvement_level(&input), Some(1));

        input = level_input();
        input.version = 104;
        input.has_patina = true;
        input.growth_rate = 0;
        input.adjacency_bonus = Some(3);
        assert_eq!(calculate_improvement_level(&input), None);
        input.version = 103;
        input.max_level = 0;
        assert_eq!(calculate_improvement_level(&input), Some(i32::from(input.stored_level)));
    }
}
