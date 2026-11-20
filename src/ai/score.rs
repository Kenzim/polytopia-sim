//! AI scoring, reproducing the game's AI.
//!
//! Closed numeric rules are implemented here. The five command builders score the
//! offers their opaque enumerators would produce. Weights are the game's own
//! constants. See `docs/specs/ai-score.md`.

use crate::WorldCoordinates;

/// Opinion love limit.
pub const LOVE_LIMIT: f32 = 2.0;
/// Opinion hate limit, the mirror of [`LOVE_LIMIT`].
pub const HATE_LIMIT: f32 = -2.0;
/// Opinion longsight.
pub const LONGSIGHT: i32 = 4;

/// Mission type.
#[repr(i32)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MissionType {
    None = 0,
    Capture = 1,
}

/// An AI mission. `tile` is the mission target. The game stores a tile
/// reference; coordinate identity is the stand-in.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Mission {
    pub kind: MissionType,
    pub tile: WorldCoordinates,
    pub score: f32,
}

/// A unit target. The game also holds the unit itself; this crate has no unit
/// object, so only the score field is represented.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct UnitTarget {
    pub score: f32,
}

/// A scored command. The game holds the command itself. Command construction is
/// not ported, so a scored command here is the numeric score only.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ScoredCommand {
    pub score: f32,
}

/// Unit stats. Abilities keep insertion order from [`get_desired_unit_stats`].
#[derive(Debug, Clone, PartialEq)]
pub struct UnitStats {
    pub defence: f32,
    pub attack: f32,
    pub movement: f32,
    pub range: f32,
    pub abilities: Vec<(i32, f32)>,
}

/// Per-player map data. Lists stand in for the game's unit and tile lists.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlayerMapData {
    pub units: Vec<u32>,
    pub empire_tiles: Vec<WorldCoordinates>,
    pub city_tiles: Vec<WorldCoordinates>,
}

/// Command type ids the builders emit.
pub mod command_type {
    pub const BUILD: u16 = 1;
    pub const TRAIN: u16 = 5;
    pub const RESEARCH: u16 = 8;
    pub const PEACE_TREATY: u16 = 27;
    pub const BREAK_PEACE: u16 = 29;
    pub const ESTABLISH_EMBASSY: u16 = 30;
}

/// Improvement type id of a road.
pub const ROAD_IMPROVEMENT: i32 = 3;
/// The embassy cost is the flat cost at `version <= 113`.
pub const EMBASSY_COST_VERSION: i32 = 0x71;
/// High aggression threshold. Embassy scoring returns 0 above this.
pub const HIGH_AGGRESSION: i32 = 5000;
/// Integer dividend (140) in the upgrade population term.
const UPGRADE_POP_DIVIDEND: u32 = 0x8c;

/// A command the tile builders append, plus the score the final command pick reads.
#[derive(Debug, Clone, PartialEq)]
pub struct IssuedCommand {
    pub command: TileCommand,
    pub score: f32,
    pub command_type: u16,
}

/// Command objects constructed by the five builders.
#[derive(Debug, Clone, PartialEq)]
pub enum TileCommand {
    Train { player: u8, unit_type: i32, at: WorldCoordinates },
    Research { player: u8, tech: i32 },
    Build { player: u8, improvement: i32, at: WorldCoordinates },
    BreakPeace { player: u8, opponent: u8, at: WorldCoordinates },
    PeaceTreaty { player: u8, opponent: u8 },
    EstablishEmbassy { player: u8, opponent: u8 },
}

/// Unit ability ids read by the scorers.
pub mod ability {
    pub const DASH: i32 = 1;
    pub const ESCAPE: i32 = 2;
    pub const PERSIST: i32 = 7;
    pub const CONVERT: i32 = 8;
    pub const SPLASH: i32 = 14;
    pub const FREEZE: i32 = 18;
    pub const FREEZE_AREA: i32 = 19;
    pub const SKATE: i32 = 21;
    pub const FORTIFY: i32 = 22;
    pub const CREEP: i32 = 23;
    pub const POISON: i32 = 26;
    pub const EAT: i32 = 27;
    pub const INFILTRATE: i32 = 35;
    pub const STOMP: i32 = 40;
    pub const TENTACLES: i32 = 44;
    pub const SWARM: i32 = 53;
    pub const DOUBLE_ATTACK: i32 = 54;
    pub const AUTO_FREEZE: i32 = 20;
    pub const AMPHIBIOUS: i32 = 47;
}

/// City reward ids the city reward choice tests for.
#[repr(i32)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CityReward {
    None = 0,
    CityWall = 1,
    Park = 2,
    Workshop = 3,
    Explorer = 4,
    BorderGrowth = 5,
    SuperUnit = 6,
    Resources = 7,
    PopulationGrowth = 8,
    Rebellion = 11,
    Poison = 13,
    TutorialExplorer = 100,
}

/// Normalised military stat fields, read after the normalisation pass.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct NormalizedMilitary {
    pub defence: f32,
    pub attack: f32,
    pub movement: f32,
    pub range: f32,
}

/// One tile in the radius 3 area around a tile (diagonals allowed, centre included).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FreezeScanTile {
    pub explored: bool,
    pub climate: i32,
    pub owner: u8,
    pub is_water: bool,
    /// Friendly unit on this tile has `FreezeArea`.
    pub friendly_freeze_area: bool,
    pub is_ice: bool,
}

/// Inputs for [`get_desired_unit_stats`].
#[derive(Debug, Clone, PartialEq)]
pub struct DesiredUnitInput {
    pub normalized: NormalizedMilitary,
    pub enemy_population: i32,
    pub infiltrate_units: i32,
    pub player_climate: i32,
    /// `None` when the tile is null or the tribe lacks unit ability Freeze.
    /// `Some` is the area scan, which may be empty.
    pub freeze_scan: Option<Vec<FreezeScanTile>>,
}

/// One enemy that [`get_city_threat_level`] might count.
///
/// The game builds the candidate list from the radius 3 area around the city
/// (diagonals allowed, centre excluded).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CityThreatUnit {
    pub explored: bool,
    pub same_owner: bool,
    pub chebyshev: i32,
    pub movement: i32,
    pub has_dash: bool,
    pub health: u16,
    pub max_health: i32,
    pub attack: i32,
}

/// Attacker's current tile, or the null tile that makes a killing spores bonus throw.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AttackerTile {
    Missing,
    Present { is_city: bool, is_water: bool, owned_by_attacker: bool },
}

/// Inputs for [`rate_battle`]. Battle damage comes from the combat calculation,
/// which this module does not recompute.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RateBattleInput {
    pub attacker_has_infiltrate: bool,
    pub defender_is_opponent_city: bool,
    pub defender_present: bool,
    pub attack_damage: i32,
    pub retaliation_damage: i32,
    pub attacker_full_cost: i32,
    pub defender_full_cost: i32,
    pub attacker_cost: i32,
    pub attacker_max_health: i32,
    pub defender_health: i32,
    pub defender_defence: i32,
    pub attacker_data_has_poison: bool,
    pub attacker_data_has_eat: bool,
    pub attacker_data_has_persist: bool,
    pub attacker_state_has_poison: bool,
    pub attacker_state_has_freeze: bool,
    pub attacker_state_has_convert: bool,
    pub defender_data_has_poison: bool,
    pub attacker_poisoned: bool,
    pub defender_poisoned: bool,
    pub defender_has_follower: bool,
    pub spores_visible: bool,
    pub attacker_tile: AttackerTile,
}

impl RateBattleInput {
    /// Stand-in for a [`PositionNeighborUnit::battle`] that [`rate_position`] never reads (own or at-peace units).
    pub const UNUSED: Self = Self {
        attacker_has_infiltrate: false,
        defender_is_opponent_city: false,
        defender_present: false,
        attack_damage: 0,
        retaliation_damage: 0,
        attacker_full_cost: 0,
        defender_full_cost: 0,
        attacker_cost: 0,
        attacker_max_health: 0,
        defender_health: 0,
        defender_defence: 0,
        attacker_data_has_poison: false,
        attacker_data_has_eat: false,
        attacker_data_has_persist: false,
        attacker_state_has_poison: false,
        attacker_state_has_freeze: false,
        attacker_state_has_convert: false,
        defender_data_has_poison: false,
        attacker_poisoned: false,
        defender_poisoned: false,
        defender_has_follower: false,
        spores_visible: false,
        attacker_tile: AttackerTile::Missing,
    };
}

/// Inputs for [`get_position_score_for_improvement`].
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ImprovementPositionInput {
    pub is_city: bool,
    pub owned_by_player: bool,
    pub hate_towards_owner: i32,
    pub can_capture: bool,
    pub can_ally_capture: bool,
    pub threat: f32,
    pub unguarded_cities: i32,
    pub can_city_support_unit: bool,
    pub currency: i32,
    pub defence: i32,
    pub defence_bonus: i32,
    pub health: u16,
    pub max_health: i32,
    pub unit_count: i32,
    pub city_count: i32,
    pub improvement_present: bool,
    pub improvement_has_poison: bool,
    pub player_has_decompose: bool,
    pub unit_present: bool,
    pub unit_poisoned: bool,
}

/// One tile in the radius 1 area around a tile (diagonals allowed, centre excluded).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PositionNeighbor {
    pub explored: bool,
    /// `None` when the tile is explored and has no unit.
    pub unit: Option<PositionNeighborUnit>,
}

/// Unit standing on a [`PositionNeighbor`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PositionNeighborUnit {
    pub same_owner: bool,
    pub at_peace: bool,
    pub detecting_hidden: bool,
    pub has_freeze_area: bool,
    pub tile_freezable: bool,
    pub has_convert: bool,
    /// Used only when this unit is hostile (the engine fills in [`RateBattleInput::UNUSED`]
    /// otherwise). The neighbor is the attacker and the rated tile is the defender.
    pub battle: RateBattleInput,
}

/// Inputs for [`rate_position`].
///
/// Area membership, peace, abilities, and battle inputs are supplied by the
/// caller. `get_best_attack_for_position` and `rate_battle` run inside.
#[derive(Debug, Clone, PartialEq)]
pub struct RatePositionInput {
    pub tile_owner: u8,
    pub player_id: u8,
    pub at_peace_with_tile_owner: bool,
    pub opinion_of_tile_owner: f32,
    pub can_attack: bool,
    /// Attack options passed to `get_best_attack_for_position` when `can_attack`.
    pub attacks: Vec<(f32, i32)>,
    pub has_escape: bool,
    pub has_auto_freeze: bool,
    pub has_freeze_area: bool,
    pub has_dash: bool,
    pub has_invisible: bool,
    pub has_skate: bool,
    pub has_amphibious: bool,
    pub neighbors: Vec<PositionNeighbor>,
    /// Return of `get_position_score_for_missions`.
    pub mission_score: f32,
    /// The player's aggression toward the tile owner. Ignored unless the tile is an
    /// enemy the player is not at peace with.
    pub hate: i32,
    /// `None` when the tile has no improvement. `Some` is
    /// `get_position_score_for_improvement`.
    pub improvement_score: Option<f32>,
    /// `None` when the tile has no tentacle or the tentacle has no owner.
    /// `Some` is the attack damage of that owner's unit against ours.
    pub tentacle_attack_damage: Option<i32>,
    pub unit_health: u16,
    pub terrain_is_ice: bool,
    pub tile_is_land: bool,
    pub tile_is_flooded: bool,
    /// Low 32 bits of the seeded hash of `(current_command * 1000, x, y)`.
    pub position_hash: u32,
    /// Whether the player is on auto-play. False forces stupidity to 0.
    pub auto_play: bool,
    /// The player's handicap.
    pub handicap: i32,
}

/// Ruling city of the tile being scored, when it is not the empty coordinates (`-1, -1`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RulingCityScore {
    /// City improvement exists and its type is `City` (`1`).
    pub is_city: bool,
    pub xp: i16,
    pub level: u16,
    pub is_connected: bool,
    /// A radius-1 diagonal neighbor, center excluded, has an improvement whose
    /// data marks it as a route opener.
    pub neighbor_route_opener: bool,
    pub same_as_start_tile: bool,
}

/// Inputs for [`get_improvement_score`].
#[derive(Debug, Clone, PartialEq)]
pub struct ImprovementScoreInput {
    pub can_build: bool,
    pub reward_population: i32,
    pub growth_population: i32,
    pub has_patina: bool,
    pub max_level: i32,
    pub reward_currency: i32,
    pub work: i32,
    pub cost: i32,
    /// `None` when the improvement creates no unit. `Some` is `get_build_unit_score`
    /// of that unit with `get_desired_unit_stats` for this tile.
    pub created_unit_score: Option<f32>,
    /// `None` when the adjacency improvement list is missing or empty.
    pub adjacency_bonus: Option<i32>,
    /// That list contains the Polaris climate improvement (id `36`).
    pub adjacency_contains_polaris_climate: bool,
    pub frozen_tile_count: i32,
    pub ruling_city: Option<RulingCityScore>,
    pub is_route_opener: bool,
    pub player_cities: i32,
    /// The rules game mode is Perfection (`1`).
    pub rules_are_perfection: bool,
    pub reward_score: i32,
    /// The improvement's ability score.
    pub ability_score: f32,
    /// Improvement type id. Road is `3`.
    pub improvement_idx: i32,
    /// `None` when the tile has no resource. `Some` is whether the
    /// improvement requires that resource.
    pub resource_required: Option<bool>,
}

/// `System.Math.Max(float, float)` for finite values and NaN.
fn fmax(a: f32, b: f32) -> f32 {
    if b < a || a.is_nan() {
        a
    } else {
        b
    }
}

/// `System.Math.Min(float, float)`.
fn fmin(a: f32, b: f32) -> f32 {
    if a < b || a.is_nan() {
        a
    } else {
        b
    }
}

/// `System.Math.Max(int, int)`: strict `>`.
fn imax(a: i32, b: i32) -> i32 {
    if a > b {
        a
    } else {
        b
    }
}

fn i32_to_f32(n: i32) -> f32 {
    n as f32
}

/// A float-to-int cast as the retail game does it (the processor's truncating conversion): truncate toward zero, but NaN and anything
/// outside the `i32` range give `i32::MIN`. Rust's `as i32` saturates instead (`+inf` to `i32::MAX`, NaN to 0).
pub fn cvtt_i32(f: f32) -> i32 {
    if f.is_nan() || !(-2_147_483_648.0..2_147_483_648.0).contains(&f) {
        i32::MIN
    } else {
        f as i32
    }
}

/// The price factor.
///
/// Used by the train, research, and improvement command scorers.
/// `1 / max(0.1 * price * price / max(currency, 1), 1)`.
pub fn price_factor(price: i32, currency: i32) -> f32 {
    let currency = fmax(i32_to_f32(currency), 1.0);
    let price = i32_to_f32(price);
    let scaled = (price / currency) * 0.1 * price;
    1.0 / fmax(scaled, 1.0)
}

/// Rates a battle.
///
/// A killing spores bonus on a null attacker tile throws in the game.
pub fn rate_battle(input: &RateBattleInput) -> f32 {
    if input.attacker_has_infiltrate && input.defender_is_opponent_city {
        return 1000.0;
    }
    if !input.defender_present {
        return 0.0;
    }

    let dealt = i32_to_f32(input.attack_damage.wrapping_mul(input.defender_full_cost));
    let mut score = if input.retaliation_damage > 0 {
        let taken = i32_to_f32(input.retaliation_damage.wrapping_mul(input.attacker_full_cost));
        dealt - taken
    } else {
        dealt
    };
    score *= 0.05;

    let kills = input.attack_damage >= input.defender_health;
    if !kills {
        if input.defender_data_has_poison && !input.attacker_poisoned {
            score += -5.0;
        }
        if input.attacker_state_has_convert {
            return i32_to_f32(input.attacker_cost) * 50.0;
        }
    } else {
        score += 10.0;
        if input.attacker_data_has_eat {
            score += 5.0;
        }
        if input.attacker_data_has_poison && !input.defender_poisoned {
            score += -5.0;
        }
        if input.attacker_data_has_persist {
            score += 10.0;
        }
    }

    if input.attacker_max_health <= input.retaliation_damage {
        score += -2.0;
    }
    if !kills {
        if let AttackerTile::Present { is_city: true, .. } = input.attacker_tile {
            score += -2.0;
        }
    }
    if !input.attacker_state_has_convert && input.defender_has_follower {
        score += -20.0;
    }
    if input.attacker_data_has_poison && !input.defender_poisoned {
        score += i32_to_f32(input.defender_defence);
    }
    if input.defender_poisoned && input.spores_visible {
        score += 2.0;
        if kills {
            match input.attacker_tile {
                AttackerTile::Missing => {
                    panic!("RateBattle spores kill reads a null attacker tile");
                }
                AttackerTile::Present { is_water, owned_by_attacker, .. } => {
                    if owned_by_attacker || is_water {
                        score += 5.0;
                    }
                }
            }
        }
    }
    if input.attacker_state_has_poison || input.attacker_state_has_freeze {
        return score;
    }
    if input.attack_damage <= 0 {
        score += -10.0;
    }
    score
}

/// The threat level of a city.
///
/// Non-cities score 0. A unit counts when it is explored, foreign, and
/// `movement + dash >= chebyshev`. Contribution is
/// `attack * (health / max_health) * 0.01`.
pub fn get_city_threat_level(is_city: bool, units: &[CityThreatUnit]) -> f32 {
    if !is_city {
        return 0.0;
    }
    let mut score = 0.0f32;
    for unit in units {
        if !unit.explored || unit.same_owner {
            continue;
        }
        let reach = unit.movement.wrapping_add(i32::from(unit.has_dash));
        if reach < unit.chebyshev {
            continue;
        }
        let health = f32::from(unit.health);
        let max_health = i32_to_f32(unit.max_health);
        let attack = i32_to_f32(unit.attack);
        let portion = health / max_health;
        score += (attack * portion) * 0.01;
    }
    score
}

/// The best attack for a position, the reduction half.
///
/// Each pair is `(battle rating, aggression)` for an attack option whose tile
/// has a unit. The game enumerates the attack options at the coordinates;
/// that enumeration is not in this module. The kept value is the max of
/// `rate * aggression * 0.001`, starting from 0, and a value must be strictly
/// greater to replace the current best.
pub fn get_best_attack_for_position(attacks: &[(f32, i32)]) -> f32 {
    let mut best = 0.0f32;
    for &(rate, aggression) in attacks {
        let weighted = (rate * i32_to_f32(aggression)) * 0.001;
        if weighted > best {
            best = weighted;
        }
    }
    best
}

/// The mission score of a position.
///
/// `manhattan` is the Manhattan distance. Capture missions are
/// skipped when `can_capture` is false. Same-tile uses coordinate equality in
/// place of the game's tile-reference compare.
/// `score / max(1, manhattan)`, plus another `score` when the unit is on that tile.
pub fn get_position_score_for_missions(here: WorldCoordinates, missions: &[(Mission, i32, bool)]) -> f32 {
    position_score_for_missions(here, missions.iter().copied())
}

/// [`get_position_score_for_missions`] over missions as they are produced: (mission, Manhattan distance, can capture).
pub fn position_score_for_missions(
    here: WorldCoordinates,
    missions: impl Iterator<Item = (Mission, i32, bool)>,
) -> f32 {
    let mut total = 0.0f32;
    for (mission, manhattan, can_capture) in missions {
        if mission.kind == MissionType::Capture && !can_capture {
            continue;
        }
        let distance = i32_to_f32(imax(1, manhattan));
        let mut add = mission.score / distance;
        if mission.tile == here {
            add += mission.score;
        }
        total += add;
    }
    total
}

/// The improvement score of a position.
///
/// A zero `defence_bonus` or `max_health` on the own-city threat path faults in
/// the game (integer division). The threatened unguarded city count and
/// whether a city can support a unit are inputs, not reimplemented.
#[allow(clippy::neg_cmp_op_on_partial_ord)] // `!(a > b)` is deliberately true for NaN
pub fn get_position_score_for_improvement(input: &ImprovementPositionInput) -> f32 {
    let mut score = if !input.is_city {
        0.0
    } else if input.owned_by_player {
        if !(input.threat > 0.0) {
            -10.0
        } else {
            let need = input.unguarded_cities.wrapping_mul(2).wrapping_add(2);
            let supported = input.can_city_support_unit && need < input.currency;
            if input.defence_bonus == 0 || input.max_health == 0 {
                panic!("GetPositionScoreForImprovement divides by zero");
            }
            let defence_term = input.defence.wrapping_div(input.defence_bonus).wrapping_add(-20);
            let health_quot = (u32::from(input.health) / (input.max_health as u32)) as i32;
            let mixed = (i32_to_f32(health_quot) + -5.0) + i32_to_f32(defence_term);
            let mut own = mixed * 0.2;
            if !supported {
                own += 300.0;
            } else {
                if need == 0 {
                    panic!("GetPositionScoreForImprovement divides by zero");
                }
                let ratio = fmin(i32_to_f32(input.unit_count) / i32_to_f32(input.city_count), 5.0);
                let currency_term = i32_to_f32(input.currency.wrapping_div(need)) * -10.0;
                own += currency_term + ratio;
            }
            own
        }
    } else if input.can_capture || !input.can_ally_capture {
        i32_to_f32(input.hate_towards_owner) * 0.1
    } else {
        -50.0
    };

    if input.improvement_present && input.improvement_has_poison && !input.player_has_decompose {
        if !input.unit_present {
            panic!("GetPositionScoreForImprovement poison penalty reads a null unit");
        }
        score += -20.0;
        if !input.unit_poisoned {
            score += -50.0;
        }
        if input.health < 21 {
            score += -100.0;
        }
    }
    score
}

/// The score for building a unit.
///
/// `position_score` is [`rate_position`]. `None` is the null-tile path, which
/// skips the `* 0.5` term. `unit_need` is the player's AI unit need.
pub fn get_build_unit_score(
    unit_defence: i32,
    unit_attack: i32,
    unit_movement: i32,
    unit_range: i32,
    desired: &UnitStats,
    unit_abilities: &[i32],
    position_score: Option<f32>,
    unit_need: f32,
) -> f32 {
    let defence = (i32_to_f32(unit_defence) * 0.1) * desired.defence;
    let attack = (i32_to_f32(unit_attack) * 0.1) * desired.attack;
    let movement = i32_to_f32(unit_movement) * desired.movement;
    let range = i32_to_f32(unit_range) * desired.range;
    let mut score = (attack + defence) + movement;
    score += range;
    for &(ability, weight) in &desired.abilities {
        if unit_abilities.contains(&ability) {
            score += weight;
        }
    }
    if let Some(position) = position_score {
        score += position * 0.5;
    }
    score + unit_need * 20.0
}

/// The unit stats the AI wants.
///
/// Mission distances are computed in the game and discarded. Stats are
/// `normalized * 20`. FreezeArea and Skate are inserted only when `freeze_scan`
/// is `Some`.
pub fn get_desired_unit_stats(input: &DesiredUnitInput) -> UnitStats {
    let defence = input.normalized.defence * 20.0;
    let attack = input.normalized.attack * 20.0;
    let movement = input.normalized.movement * 20.0;
    let range = input.normalized.range * 20.0;
    let mut abilities = Vec::new();
    if let Some(tiles) = &input.freeze_scan {
        let (freeze_area, ice) = freeze_scan_counters(tiles, input.player_climate);
        abilities.push((ability::FREEZE_AREA, i32_to_f32(freeze_area)));
        abilities.push((ability::SKATE, i32_to_f32(ice.wrapping_mul(5))));
    }
    abilities.push((ability::DASH, attack * 0.5));
    abilities.push((ability::FORTIFY, defence * 0.5));
    abilities.push((ability::ESCAPE, 0.5 * movement));
    abilities.push((ability::TENTACLES, defence));
    abilities.push((ability::CREEP, movement));
    abilities.push((ability::SWARM, movement + movement));
    abilities.push((ability::POISON, attack));
    abilities.push((ability::STOMP, attack));
    abilities.push((ability::DOUBLE_ATTACK, attack));
    abilities.push((ability::SPLASH, range));
    let infiltrate = (i32_to_f32(input.enemy_population) * 0.2) / (i32_to_f32(input.infiltrate_units) + 1.0);
    abilities.push((ability::INFILTRATE, infiltrate));
    UnitStats { defence, attack, movement, range, abilities }
}

fn freeze_scan_counters(tiles: &[FreezeScanTile], player_climate: i32) -> (i32, i32) {
    let mut counter = 0i32;
    let mut ice = 0i32;
    for tile in tiles {
        if !tile.explored {
            continue;
        }
        let familiar = tile.climate == player_climate || tile.owner != 0;
        if tile.is_water || !familiar {
            counter = counter.wrapping_add(2);
        }
        if tile.friendly_freeze_area {
            counter = counter.wrapping_sub(80);
        }
        if tile.is_ice {
            ice = ice.wrapping_add(1);
        }
    }
    (counter, ice)
}

/// Break-peace score inside [`add_possible_diplomacy_commands`].
///
/// Considered only when opinion is strictly negative. Score is
/// `opinion * -500 - units_in_borders * 200`, and it is kept only when positive.
#[allow(clippy::neg_cmp_op_on_partial_ord)] // `!(a < b)` is deliberately true for NaN
pub fn break_peace_score(opinion: f32, units_in_borders: i32) -> Option<f32> {
    if !(opinion < 0.0) {
        return None;
    }
    let penalty = i32_to_f32(units_in_borders.wrapping_mul(200));
    let score = opinion * -500.0 - penalty;
    if score > 0.0 {
        Some(score)
    } else {
        None
    }
}

/// Peace-command score when listing diplomacy commands.
///
/// `range_sample` is the seeded hash range draw `0..1` over `(x, 0, current_command)`. The
/// command scores `1000` when `range_sample * opinion` is above the love threshold (2).
pub fn peace_command_score(range_sample: f32, opinion: f32) -> Option<f32> {
    let product = range_sample * opinion;
    if product > LOVE_LIMIT {
        Some(1000.0)
    } else {
        None
    }
}

/// Chooses a city reward, given the hash int.
///
/// The hash is `XXHash`'s virtual call with `(tile.x, tile.y, current_command)`.
/// Border growth wins if it is present, otherwise a super unit, otherwise
/// `rewards[sign_bit]` where the index is 0 for a non-negative hash and 1 for a
/// negative hash. An out-of-range index throws in the game.
pub fn choose_city_reward(rewards: &[CityReward], hash: i32) -> CityReward {
    if rewards.contains(&CityReward::BorderGrowth) {
        return CityReward::BorderGrowth;
    }
    if rewards.contains(&CityReward::SuperUnit) {
        return CityReward::SuperUnit;
    }
    reward_by_sign(rewards, hash)
}

/// Chooses an infiltration reward.
///
/// Same sign-bit index as [`choose_city_reward`], with no border or super-unit
/// preference. The hash inputs are the same `(tile.x, tile.y, current_command)` call.
pub fn choose_infiltration_reward(rewards: &[CityReward], hash: i32) -> CityReward {
    reward_by_sign(rewards, hash)
}

fn reward_by_sign(rewards: &[CityReward], hash: i32) -> CityReward {
    let index = ((hash as u32) >> 31) as usize;
    rewards.get(index).copied().unwrap_or_else(|| panic!("city reward index {index} is outside {}", rewards.len()))
}

/// The opinion of an opponent, the lookup half.
///
/// The game refreshes the opinions first. That refresh is not ported. A missing
/// opponent is 0. The stored value is the opinion's total.
pub fn get_opinion(opinions: &[(u8, f32)], opponent: u8) -> f32 {
    opinions.iter().find(|(id, _)| *id == opponent).map(|(_, total)| *total).unwrap_or(0.0)
}

/// Rates a position.
///
/// Peace with a non-zero owner and a negative opinion returns `-1000` and skips
/// the rest. Otherwise the score starts at `0`, adds the best attack when
/// `can_attack` is set, then walks the radius-1 area. A hostile neighbor
/// subtracts `rate_battle * scale` into an accumulator (`scale` is `0.7`, the
/// escape constant just under 0.1 (`0x3dccccc8`), or `1`). After the walk, an invisible unit
/// subtracts `enemies * 3`; otherwise a positive enemy count adds
/// `accumulator / count`. Missions, the improvement score, tentacles, skate,
/// and amphibious follow. A negative aggression while at war scales the
/// running total by `1 - (|hate| / 1000)` with the integer quotient by 1000.
/// Stupidity is `0` when auto-play is false, otherwise `max(0, 2 - handicap)`.
/// The hash term is `(uint_hash * 2^-32 - 0.5) * stupidity * 50`.
pub fn rate_position(input: &RatePositionInput) -> f32 {
    if input.tile_owner != 0 && input.at_peace_with_tile_owner && input.opinion_of_tile_owner < 0.0 {
        return -1000.0;
    }

    let mut score = 0.0f32;
    if input.can_attack {
        score += get_best_attack_for_position(&input.attacks);
    }
    let scale = if input.can_attack {
        if input.has_escape {
            f32::from_bits(0x3dccccc8)
        } else {
            0.7
        }
    } else {
        1.0
    };
    let scans_freeze = input.has_auto_freeze || input.has_freeze_area;

    let mut threat = 0.0f32;
    let mut enemies = 0i32;
    for neighbor in &input.neighbors {
        if !neighbor.explored {
            score += 2.0;
            continue;
        }
        let Some(unit) = neighbor.unit else {
            continue;
        };
        if unit.same_owner || unit.at_peace {
            if unit.detecting_hidden {
                score += 20.0;
            }
            continue;
        }
        enemies = enemies.wrapping_add(1);
        threat -= rate_battle(&unit.battle) * scale;
        if unit.has_freeze_area && !input.has_dash && !input.has_invisible {
            score += -30.0;
        }
        if scans_freeze && unit.tile_freezable {
            score += 5.0;
        }
        if unit.has_convert {
            score += -20.0;
            if input.can_attack {
                score += 10.0;
            }
        }
    }
    if input.has_invisible {
        score -= i32_to_f32(enemies.wrapping_mul(3));
    } else if enemies > 0 {
        score += threat / i32_to_f32(enemies);
    }

    score += input.mission_score;
    if let Some(improvement) = input.improvement_score {
        score += improvement;
    }
    if let Some(damage) = input.tentacle_attack_damage {
        score -= i32_to_f32(damage.wrapping_mul(2));
        if damage > i32::from(input.unit_health) {
            score += -100.0;
        }
    }
    if input.has_skate && !input.terrain_is_ice {
        score += -20.0;
    }
    if input.has_amphibious && input.tile_is_land && !input.tile_is_flooded {
        score += -10.0;
    }

    let at_war = input.tile_owner != 0 && input.tile_owner != input.player_id && !input.at_peace_with_tile_owner;
    if at_war && input.hate < 0 {
        let magnitude = input.hate.wrapping_neg() as u32 as u64;
        let quotient = magnitude.wrapping_mul(0x10624dd3) >> 38;
        let factor = i32_to_f32((quotient as i32).wrapping_neg());
        score += factor * score;
    }

    let sample = (input.position_hash as f32) * f32::from_bits(0x2f800000) + -0.5;
    let stupidity = if input.auto_play { i32_to_f32(imax(0, 2i32.wrapping_sub(input.handicap))) } else { 0.0 };
    score + (stupidity * sample) * 50.0
}

/// The score of an improvement.
///
/// An improvement that cannot be built scores `0`. Otherwise the base is reward population, or
/// `max_level * growth_population + reward_population` when the improvement has
/// Patina (`5`). That base is multiplied by `25`. Reward currency is multiplied
/// by `2` and work by `20`. Those three floats are added as
/// `work + (currency + population)`. A created unit adds `get_build_unit_score`.
/// A non-empty adjacency list adds `growth_population * bonus * 20` and
/// `work * bonus * 20`, and the Polaris climate (`36`) on that list adds
/// `frozen_tile_count / 20` to the bonus first. A real ruling city whose
/// improvement is a city adds `200 / max(cost, 1)` when `base + xp > level`.
/// A route opener with no neighboring route opener adds `30` when the city is
/// disconnected and `80` when the city is the start tile and `cities >= 2`.
/// Perfection adds `reward_score / 10`. The ability score is always
/// added. Unless `idx` is Road (`3`), a visible resource the improvement does
/// not require multiplies the total by `0.5`.
pub fn get_improvement_score(input: &ImprovementScoreInput) -> f32 {
    if !input.can_build {
        return 0.0;
    }

    let base = if input.has_patina {
        input.max_level.wrapping_mul(input.growth_population).wrapping_add(input.reward_population)
    } else {
        input.reward_population
    };
    let population = i32_to_f32(base.wrapping_mul(25));
    let currency = i32_to_f32(input.reward_currency.wrapping_mul(2));
    let work = i32_to_f32(input.work.wrapping_mul(20));
    let mut score = work + (currency + population);
    if let Some(unit_score) = input.created_unit_score {
        score += unit_score;
    }
    if let Some(bonus) = input.adjacency_bonus {
        let bonus = if input.adjacency_contains_polaris_climate {
            bonus.wrapping_add(input.frozen_tile_count / 20)
        } else {
            bonus
        };
        let scaled = bonus.wrapping_mul(20);
        score += i32_to_f32(input.growth_population.wrapping_mul(scaled));
        score += i32_to_f32(input.work.wrapping_mul(scaled));
    }
    if let Some(city) = input.ruling_city {
        if city.is_city {
            let mixed = base.wrapping_add(i32::from(city.xp));
            if mixed > i32::from(city.level) {
                score += i32_to_f32(200 / imax(input.cost, 1));
            }
        }
        if input.is_route_opener && !city.neighbor_route_opener {
            if !city.is_connected {
                score += 30.0;
            }
            if city.same_as_start_tile && input.player_cities >= 2 {
                score += 80.0;
            }
        }
    }
    if input.rules_are_perfection {
        score += i32_to_f32(input.reward_score) / 10.0;
    }
    score += input.ability_score;
    if input.improvement_idx != 3 && input.resource_required == Some(false) {
        score *= 0.5;
    }
    score
}

/// One trainable unit or upgrade inside [`add_possible_train_unit_commands`].
///
/// The trainable units and the unlocked upgrades for a unit
/// produce these. `valid` is the command's own validity check.
#[derive(Debug, Clone, PartialEq)]
pub struct TrainOffer {
    pub player: u8,
    pub unit_type: i32,
    pub at: WorldCoordinates,
    pub cost: i32,
    pub defence: i32,
    pub attack: i32,
    pub movement: i32,
    pub range: i32,
    pub abilities: Vec<i32>,
    pub valid: bool,
    /// `None` when the unit build score is given a null tile.
    pub position_score: Option<f32>,
}

/// One explored city the train builder scores.
///
/// `explored` is whether the city is explored. `threat` is [`get_city_threat_level`].
/// `desired` is [`get_desired_unit_stats`] before the
/// threat adjustment.
#[derive(Debug, Clone, PartialEq)]
pub struct TrainCity {
    pub explored: bool,
    pub threat: f32,
    pub desired: UnitStats,
    pub trains: Vec<TrainOffer>,
    pub upgrades: Vec<TrainOffer>,
}

/// Improvement term inside one tech, from the `improvement_unlocks` walk.
///
/// `score_sum` adds the improvement score for scores that are not
/// zero. `limited` is the Limited ability (ability 2).
/// `limited_scale` is the count of the player's limited improvements.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ResearchImprovement {
    pub score_sum: f32,
    pub limited: bool,
    pub limited_scale: f32,
}

/// One unlockable tech. `price` is
/// the tech price. `unit_build_scores` are [`get_build_unit_score`]
/// for each `unit_unlocks` entry. `upgradeable_cities` counts cities where
/// the tech can upgrade the city.
#[derive(Debug, Clone, PartialEq)]
pub struct ResearchOffer {
    pub player: u8,
    pub tech: i32,
    pub price: i32,
    pub cities: i32,
    pub unit_build_scores: Vec<f32>,
    pub improvements: Vec<ResearchImprovement>,
    pub ability_unlocks: i32,
    pub upgradeable_cities: i32,
    /// The tech need count for this tech; the scorer adds three per need.
    pub need: i32,
}

/// One buildable improvement. `base_score` is the improvement score.
///
/// The population bonus runs only when growth population is positive, or reward population is positive
/// and `base_score` is above zero, the ruling city is in the tracked list, and
/// the population needed to upgrade the city is positive. Pass `upgrade_need`
/// as 0 when any of those gates fails.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ImprovementOffer {
    pub player: u8,
    pub improvement: i32,
    pub at: WorldCoordinates,
    pub base_score: f32,
    pub cost: i32,
    pub reward_population: i32,
    pub growth_population: i32,
    pub city_tracked: bool,
    pub upgrade_need: i32,
}

/// A tile on a city-to-city path.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RoadStep {
    pub at: WorldCoordinates,
    pub has_road: bool,
}

/// One city pair. `city_potential` is the city potential.
/// Tiles that already have a road are removed before the divisor.
#[derive(Debug, Clone, PartialEq)]
pub struct RoadPlan {
    pub player: u8,
    pub city_potential: i32,
    pub steps: Vec<RoadStep>,
}

/// One diplomacy command offered for an opponent.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum DiplomacyOffer {
    BreakPeace {
        player: u8,
        opponent: u8,
        at: WorldCoordinates,
        /// The player's opinion of the opponent.
        opinion: f32,
        /// The player's units within the opponent's borders.
        units_in_borders: i32,
    },
    Peace {
        player: u8,
        opponent: u8,
        /// The seeded hash range draw `0..1` over `(field, 0, current_command)`.
        range_sample: f32,
        opinion: f32,
    },
    Embassy {
        player: u8,
        opponent: u8,
        score: f32,
    },
}

/// Inputs for [`get_embassy_score`].
///
/// `aggression` is the player's aggression. `at_peace` and `at_war` are whether
/// the players are at peace or at war.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct EmbassyScoreInput {
    pub version: i32,
    pub currency: i32,
    pub embassy_income: i32,
    pub embassy_cost_base: i32,
    /// Players whose relation with this id has
    /// `embassy_level > 0`.
    pub embassy_count: i32,
    pub aggression: i32,
    pub at_peace: bool,
    pub at_war: bool,
}

/// The embassy cost for a player.
pub fn embassy_cost_for_player(version: i32, base_cost: i32, embassy_count: i32) -> i32 {
    if version <= EMBASSY_COST_VERSION {
        base_cost
    } else {
        embassy_count.wrapping_mul(2).wrapping_add(4)
    }
}

/// The embassy score.
///
/// The game progress is not read here. Above [`HIGH_AGGRESSION`], or when
/// `at_war` is true, the score is 0. Otherwise it is
/// `peace_term + aggression * -0.001 + price_factor(cost) * embassy_income`,
/// where peace adds `embassy_income * 5`.
pub fn get_embassy_score(input: &EmbassyScoreInput) -> f32 {
    if input.aggression > HIGH_AGGRESSION || input.at_war {
        return 0.0;
    }
    let cost = embassy_cost_for_player(input.version, input.embassy_cost_base, input.embassy_count);
    let priced = price_factor(cost, input.currency) * i32_to_f32(input.embassy_income);
    let peace = if input.at_peace { i32_to_f32(input.embassy_income.wrapping_mul(5)) } else { 0.0 };
    peace + i32_to_f32(input.aggression) * -0.001 + priced
}

/// Scores the possible train unit commands.
///
/// An explored city whose threat is `> 0` adds `threat * 20` to desired defence
/// and uses addend `100`. Otherwise the addend is `0`. A valid offer is kept
/// when `(unit build score + addend) * price_factor > 0`. Upgrades use the
/// same adjusted stats and addend.
pub fn add_possible_train_unit_commands(currency: i32, unit_need: f32, cities: &[TrainCity]) -> Vec<IssuedCommand> {
    let mut out = Vec::new();
    for city in cities {
        if !city.explored {
            continue;
        }
        let mut desired = city.desired.clone();
        let addend = if city.threat > 0.0 {
            desired.defence += city.threat * 20.0;
            100.0
        } else {
            0.0
        };
        score_trains(&mut out, currency, unit_need, &desired, addend, &city.trains);
        score_trains(&mut out, currency, unit_need, &desired, addend, &city.upgrades);
    }
    out
}

fn score_trains(
    out: &mut Vec<IssuedCommand>,
    currency: i32,
    unit_need: f32,
    desired: &UnitStats,
    addend: f32,
    offers: &[TrainOffer],
) {
    for offer in offers {
        if !offer.valid {
            continue;
        }
        let built = get_build_unit_score(
            offer.defence,
            offer.attack,
            offer.movement,
            offer.range,
            desired,
            &offer.abilities,
            offer.position_score,
            unit_need,
        );
        let score = (built + addend) * price_factor(offer.cost, currency);
        if crate::env_var!("TRAIN_DEBUG").is_ok() {
            eprintln!("TRAIN at {:?} unit {} cost {} currency {} built {} addend {} pf {} need {} pos {:?} desired {:?} score {}", offer.at, offer.unit_type, offer.cost, currency, built, addend, price_factor(offer.cost, currency), unit_need, offer.position_score, desired, score);
        }
        if score > 0.0 {
            out.push(IssuedCommand {
                command: TileCommand::Train { player: offer.player, unit_type: offer.unit_type, at: offer.at },
                score,
                command_type: command_type::TRAIN,
            });
        }
    }
}

/// Scores the possible research commands.
///
/// The total is unit scores times `cities`, then each improvement sum divided
/// by 5 (or multiplied by `limited_scale`), then `ability_unlocks * 5`, then
/// `20` per upgradeable city. That total is multiplied by [`price_factor`].
/// The command is appended even when the score is not positive.
/// The tech needs check is the caller's offer list.
pub fn add_possible_research_commands(currency: i32, techs: &[ResearchOffer]) -> Vec<IssuedCommand> {
    let mut out = Vec::new();
    for tech in techs {
        let mut unit_accum = 0.0f32;
        let cities = i32_to_f32(tech.cities);
        for score in &tech.unit_build_scores {
            unit_accum += score * cities;
        }
        let mut total = i32_to_f32(tech.need.wrapping_mul(3)) + unit_accum;
        for improvement in &tech.improvements {
            let sum = if improvement.limited {
                improvement.score_sum * improvement.limited_scale
            } else {
                improvement.score_sum
            };
            total += sum / 5.0;
        }
        total += i32_to_f32(tech.ability_unlocks.wrapping_mul(5));
        let mut city = 0i32;
        while city < tech.upgradeable_cities {
            total += 20.0;
            city += 1;
        }
        let score = total * price_factor(tech.price, currency);
        if crate::env_var!("RES_DEBUG").is_ok() {
            eprintln!(
                "  RESTOTAL tech {} total {} pf {} score {}",
                tech.tech,
                total,
                price_factor(tech.price, currency),
                score
            );
        }
        out.push(IssuedCommand {
            command: TileCommand::Research { player: tech.player, tech: tech.tech },
            score,
            command_type: command_type::RESEARCH,
        });
    }
    out
}

/// Scores the possible improvement commands.
///
/// When the population gate passes (reward population with a base score above zero, or any growth
/// population), add `(140 / upgrade_need) * (population / cost)`
/// using unsigned division. Multiply by [`price_factor`] of the improvement cost.
/// Keep the command only when that score is `> 1`.
pub fn add_possible_improvement_commands(currency: i32, offers: &[ImprovementOffer]) -> Vec<IssuedCommand> {
    let mut out = Vec::new();
    for offer in offers {
        let mut score = offer.base_score;
        let population = offer.reward_population.wrapping_add(offer.growth_population);
        // Reward population counts only for an offer that scores above zero; growth population always counts.
        if ((offer.base_score > 0.0 && offer.reward_population > 0) || offer.growth_population > 0)
            && offer.city_tracked
            && offer.upgrade_need > 0
        {
            let quot = UPGRADE_POP_DIVIDEND.wrapping_div(offer.upgrade_need as u32);
            score += i32_to_f32(quot as i32) * (i32_to_f32(population) / i32_to_f32(offer.cost));
        }
        let priced = score * price_factor(offer.cost, currency);
        if priced > 1.0 {
            out.push(IssuedCommand {
                command: TileCommand::Build { player: offer.player, improvement: offer.improvement, at: offer.at },
                score: priced,
                command_type: command_type::BUILD,
            });
        }
    }
    out
}

/// Scores the possible road building commands.
///
/// Road tiles are removed from the path first. The score is
/// `city_potential / remaining_count` when that count is positive. Each
/// remaining tile becomes a Road build (improvement type `3`).
pub fn add_possible_road_building_commands(plans: &[RoadPlan]) -> Vec<IssuedCommand> {
    let mut out = Vec::new();
    for plan in plans {
        let open: Vec<WorldCoordinates> = plan.steps.iter().filter(|step| !step.has_road).map(|step| step.at).collect();
        if open.is_empty() {
            continue;
        }
        let score = i32_to_f32(plan.city_potential) / i32_to_f32(open.len() as i32);
        for at in open {
            out.push(IssuedCommand {
                command: TileCommand::Build { player: plan.player, improvement: ROAD_IMPROVEMENT, at },
                score,
                command_type: command_type::BUILD,
            });
        }
    }
    out
}

/// Scores the possible diplomacy commands.
///
/// Break-peace uses [`break_peace_score`]. Peace uses [`peace_command_score`].
/// Embassy keeps [`get_embassy_score`] when it is `> 0`. Other command types
/// are skipped. The game computes the game progress here, but its
/// return value is not stored before this loop.
pub fn add_possible_diplomacy_commands(offers: &[DiplomacyOffer]) -> Vec<IssuedCommand> {
    let mut out = Vec::new();
    for offer in offers {
        match *offer {
            DiplomacyOffer::BreakPeace { player, opponent, at, opinion, units_in_borders } => {
                if let Some(score) = break_peace_score(opinion, units_in_borders) {
                    out.push(IssuedCommand {
                        command: TileCommand::BreakPeace { player, opponent, at },
                        score,
                        command_type: command_type::BREAK_PEACE,
                    });
                }
            }
            DiplomacyOffer::Peace { player, opponent, range_sample, opinion } => {
                if let Some(score) = peace_command_score(range_sample, opinion) {
                    out.push(IssuedCommand {
                        command: TileCommand::PeaceTreaty { player, opponent },
                        score,
                        command_type: command_type::PEACE_TREATY,
                    });
                }
            }
            DiplomacyOffer::Embassy { player, opponent, score } => {
                if score > 0.0 {
                    out.push(IssuedCommand {
                        command: TileCommand::EstablishEmbassy { player, opponent },
                        score,
                        command_type: command_type::ESTABLISH_EMBASSY,
                    });
                }
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn blank_battle() -> RateBattleInput {
        RateBattleInput {
            attacker_has_infiltrate: false,
            defender_is_opponent_city: false,
            defender_present: true,
            attack_damage: 0,
            retaliation_damage: 0,
            attacker_full_cost: 0,
            defender_full_cost: 0,
            attacker_cost: 0,
            attacker_max_health: 10,
            defender_health: 10,
            defender_defence: 0,
            attacker_data_has_poison: false,
            attacker_data_has_eat: false,
            attacker_data_has_persist: false,
            attacker_state_has_poison: false,
            attacker_state_has_freeze: false,
            attacker_state_has_convert: false,
            defender_data_has_poison: false,
            attacker_poisoned: false,
            defender_poisoned: false,
            defender_has_follower: false,
            spores_visible: false,
            attacker_tile: AttackerTile::Present { is_city: false, is_water: false, owned_by_attacker: true },
        }
    }

    #[test]
    fn price_factor_matches_currency_curve() {
        assert_eq!(price_factor(5, 10), 1.0);
        assert_eq!(price_factor(20, 5), 0.125);
        assert_eq!(price_factor(0, 0), 1.0);
        assert_eq!(price_factor(-4, 3), 1.0);
    }

    #[test]
    fn rate_battle_covers_the_closed_branches() {
        assert_eq!(
            rate_battle(&RateBattleInput {
                attacker_has_infiltrate: true,
                defender_is_opponent_city: true,
                ..blank_battle()
            }),
            1000.0
        );
        assert_eq!(rate_battle(&RateBattleInput { defender_present: false, ..blank_battle() }), 0.0);

        let mut trade = blank_battle();
        trade.attack_damage = 10;
        trade.defender_full_cost = 3;
        trade.defender_health = 15;
        assert_eq!(rate_battle(&trade), 1.5);

        trade.retaliation_damage = 4;
        trade.attacker_full_cost = 5;
        assert_eq!(rate_battle(&trade), 0.5);

        trade.retaliation_damage = 0;
        trade.defender_health = 10;
        assert_eq!(rate_battle(&trade), 11.5);

        trade.attack_damage = 3;
        trade.defender_health = 10;
        trade.attacker_state_has_convert = true;
        trade.attacker_cost = 8;
        assert_eq!(rate_battle(&trade), 400.0);

        let mut zero = blank_battle();
        zero.attack_damage = 0;
        assert_eq!(rate_battle(&zero), -10.0);
        zero.attacker_state_has_freeze = true;
        assert_eq!(rate_battle(&zero), 0.0);
    }

    #[test]
    fn city_threat_filters_reach_and_scales_attack() {
        assert_eq!(get_city_threat_level(false, &[]), 0.0);
        let units = [
            CityThreatUnit {
                explored: true,
                same_owner: false,
                chebyshev: 3,
                movement: 2,
                has_dash: true,
                health: 10,
                max_health: 20,
                attack: 4,
            },
            CityThreatUnit {
                explored: true,
                same_owner: false,
                chebyshev: 5,
                movement: 2,
                has_dash: false,
                health: 10,
                max_health: 10,
                attack: 9,
            },
            CityThreatUnit {
                explored: false,
                same_owner: false,
                chebyshev: 1,
                movement: 2,
                has_dash: false,
                health: 10,
                max_health: 10,
                attack: 9,
            },
            CityThreatUnit {
                explored: true,
                same_owner: true,
                chebyshev: 1,
                movement: 2,
                has_dash: false,
                health: 10,
                max_health: 10,
                attack: 9,
            },
        ];
        let expected = (4.0f32 * (10.0 / 20.0)) * 0.01;
        assert_eq!(get_city_threat_level(true, &units), expected);
    }

    #[test]
    fn best_attack_keeps_the_strict_positive_max() {
        let best = get_best_attack_for_position(&[(10.0, 100), (-5.0, 100), (30.0, 100), (20.0, 50)]);
        assert_eq!(best, (30.0f32 * 100.0) * 0.001);
        assert_eq!(get_best_attack_for_position(&[]), 0.0);
        assert_eq!(get_best_attack_for_position(&[(-8.0, 50)]), 0.0);
    }

    #[test]
    fn mission_scores_divide_by_distance_and_double_on_the_tile() {
        let here = WorldCoordinates { x: 2, y: 2 };
        let far = Mission { kind: MissionType::Capture, tile: WorldCoordinates { x: 0, y: 0 }, score: 9.0 };
        let skipped = Mission { kind: MissionType::Capture, tile: WorldCoordinates { x: 1, y: 1 }, score: 100.0 };
        let here_mission = Mission { kind: MissionType::None, tile: here, score: 4.0 };
        let total =
            get_position_score_for_missions(here, &[(far, 3, true), (skipped, 1, false), (here_mission, 0, false)]);
        assert_eq!(total, 3.0 + 8.0);
    }

    #[test]
    fn improvement_position_covers_own_enemy_and_poison() {
        let mut own = ImprovementPositionInput {
            is_city: true,
            owned_by_player: true,
            hate_towards_owner: 0,
            can_capture: false,
            can_ally_capture: false,
            threat: 1.0,
            unguarded_cities: 1,
            can_city_support_unit: false,
            currency: 0,
            defence: 4,
            defence_bonus: 2,
            health: 10,
            max_health: 10,
            unit_count: 4,
            city_count: 2,
            improvement_present: false,
            improvement_has_poison: false,
            player_has_decompose: false,
            unit_present: true,
            unit_poisoned: false,
        };
        let base = ((10.0f32 / 10.0) + -5.0) + ((4 / 2 - 20) as f32);
        assert_eq!(get_position_score_for_improvement(&own), base * 0.2 + 300.0);

        own.can_city_support_unit = true;
        own.currency = 10;
        own.unit_count = 4;
        own.city_count = 2;
        let supported = base * 0.2 + ((10 / 4) as f32 * -10.0 + 4.0 / 2.0);
        assert_eq!(get_position_score_for_improvement(&own), supported);
        own.can_city_support_unit = false;
        own.threat = 0.0;
        assert_eq!(get_position_score_for_improvement(&own), -10.0);

        own.owned_by_player = false;
        own.hate_towards_owner = 30;
        own.can_capture = true;
        assert_eq!(get_position_score_for_improvement(&own), 3.0);

        own.can_capture = false;
        own.can_ally_capture = true;
        assert_eq!(get_position_score_for_improvement(&own), -50.0);

        own.is_city = false;
        own.improvement_present = true;
        own.improvement_has_poison = true;
        own.health = 10;
        own.unit_poisoned = false;
        assert_eq!(get_position_score_for_improvement(&own), -20.0 - 50.0 - 100.0);
    }

    #[test]
    fn build_score_dots_stats_then_adds_position_and_need() {
        let desired =
            UnitStats { defence: 10.0, attack: 5.0, movement: 2.0, range: 1.0, abilities: vec![(ability::DASH, 2.5)] };
        let score = get_build_unit_score(2, 4, 1, 2, &desired, &[ability::DASH], Some(4.0), 0.5);
        let defence = (2.0f32 * 0.1) * 10.0;
        let attack = (4.0f32 * 0.1) * 5.0;
        let stats = (attack + defence) + 1.0 * 2.0 + 2.0 * 1.0;
        assert_eq!(score, stats + 2.5 + 4.0 * 0.5 + 0.5 * 20.0);
        assert_eq!(get_build_unit_score(2, 4, 1, 2, &desired, &[], None, 0.0), stats);
    }

    #[test]
    fn desired_stats_scale_normalized_military_and_freeze_scan() {
        let input = DesiredUnitInput {
            normalized: NormalizedMilitary { defence: 0.5, attack: 0.25, movement: 0.1, range: 0.05 },
            enemy_population: 10,
            infiltrate_units: 1,
            player_climate: 3,
            freeze_scan: Some(vec![
                FreezeScanTile {
                    explored: true,
                    climate: 9,
                    owner: 0,
                    is_water: false,
                    friendly_freeze_area: true,
                    is_ice: false,
                },
                FreezeScanTile {
                    explored: true,
                    climate: 3,
                    owner: 1,
                    is_water: false,
                    friendly_freeze_area: false,
                    is_ice: true,
                },
                FreezeScanTile {
                    explored: false,
                    climate: 0,
                    owner: 0,
                    is_water: true,
                    friendly_freeze_area: false,
                    is_ice: true,
                },
            ]),
        };
        let stats = get_desired_unit_stats(&input);
        assert_eq!(stats.defence, 10.0);
        assert_eq!(stats.attack, 5.0);
        assert_eq!(stats.movement, 2.0);
        assert_eq!(stats.range, 1.0);
        assert_eq!(stats.abilities[0], (ability::FREEZE_AREA, (2 - 80) as f32));
        assert_eq!(stats.abilities[1], (ability::SKATE, 5.0));
        assert_eq!(stats.abilities[2], (ability::DASH, 2.5));
        assert_eq!(*stats.abilities.last().unwrap(), (ability::INFILTRATE, 1.0));
        assert_eq!(stats.abilities.len(), 13);
    }

    #[test]
    fn city_and_infiltration_rewards_follow_the_hash_sign() {
        let plain = [CityReward::Park, CityReward::Workshop];
        assert_eq!(choose_city_reward(&plain, 1), CityReward::Park);
        assert_eq!(choose_city_reward(&plain, -1), CityReward::Workshop);
        assert_eq!(choose_city_reward(&[CityReward::Park, CityReward::BorderGrowth], -5), CityReward::BorderGrowth);
        assert_eq!(choose_city_reward(&[CityReward::Park, CityReward::SuperUnit], 4), CityReward::SuperUnit);
        assert_eq!(choose_infiltration_reward(&plain, 0), CityReward::Park);
        assert_eq!(choose_infiltration_reward(&plain, i32::MIN), CityReward::Workshop);
    }

    #[test]
    fn opinion_lookup_returns_total_or_zero() {
        let opinions = [(1u8, 2.5f32), (4, -0.5)];
        assert_eq!(get_opinion(&opinions, 1), 2.5);
        assert_eq!(get_opinion(&opinions, 9), 0.0);
    }

    #[test]
    fn diplomacy_score_rules_use_recovered_thresholds() {
        assert_eq!(break_peace_score(-1.0, 1), Some(300.0));
        assert_eq!(break_peace_score(0.0, 0), None);
        assert_eq!(break_peace_score(-0.1, 50), None);
        assert_eq!(peace_command_score(1.0, 3.0), Some(1000.0));
        assert_eq!(peace_command_score(1.0, 2.0), None);
        assert_eq!(LOVE_LIMIT, 2.0);
        assert_eq!(HATE_LIMIT, -2.0);
        assert_eq!(LONGSIGHT, 4);
    }

    fn blank_position() -> RatePositionInput {
        RatePositionInput {
            tile_owner: 0,
            player_id: 1,
            at_peace_with_tile_owner: false,
            opinion_of_tile_owner: 0.0,
            can_attack: false,
            attacks: Vec::new(),
            has_escape: false,
            has_auto_freeze: false,
            has_freeze_area: false,
            has_dash: false,
            has_invisible: false,
            has_skate: false,
            has_amphibious: false,
            neighbors: Vec::new(),
            mission_score: 0.0,
            hate: 0,
            improvement_score: None,
            tentacle_attack_damage: None,
            unit_health: 10,
            terrain_is_ice: false,
            tile_is_land: false,
            tile_is_flooded: false,
            position_hash: 0,
            auto_play: false,
            handicap: 0,
        }
    }

    fn hostile(battle: RateBattleInput) -> PositionNeighbor {
        PositionNeighbor {
            explored: true,
            unit: Some(PositionNeighborUnit {
                same_owner: false,
                at_peace: false,
                detecting_hidden: false,
                has_freeze_area: false,
                tile_freezable: false,
                has_convert: false,
                battle,
            }),
        }
    }

    #[test]
    fn rate_position_applies_peace_neighbors_and_stupidity() {
        let mut early = blank_position();
        early.tile_owner = 2;
        early.at_peace_with_tile_owner = true;
        early.opinion_of_tile_owner = -0.1;
        early.mission_score = 40.0;
        assert_eq!(rate_position(&early), -1000.0);

        early.tile_owner = 0;
        assert_eq!(rate_position(&early), 40.0);

        let mut attack = blank_position();
        attack.can_attack = true;
        attack.attacks = vec![(30.0, 100)];
        attack.mission_score = 4.0;
        let infiltrate =
            RateBattleInput { attacker_has_infiltrate: true, defender_is_opponent_city: true, ..blank_battle() };
        attack.neighbors = vec![hostile(infiltrate)];
        let scaled = 1000.0 * 0.7;
        assert_eq!(rate_position(&attack), 3.0 + 4.0 - scaled);

        attack.has_escape = true;
        let escape = 1000.0 * f32::from_bits(0x3dccccc8);
        assert_ne!(escape, 1000.0 * 0.1);
        assert_eq!(rate_position(&attack), 3.0 + 4.0 - escape);

        let mut quiet = blank_position();
        quiet.neighbors = vec![
            PositionNeighbor { explored: false, unit: None },
            PositionNeighbor {
                explored: true,
                unit: Some(PositionNeighborUnit {
                    same_owner: true,
                    at_peace: false,
                    detecting_hidden: true,
                    has_freeze_area: false,
                    tile_freezable: false,
                    has_convert: false,
                    battle: blank_battle(),
                }),
            },
        ];
        assert_eq!(rate_position(&quiet), 22.0);

        let mut frozen = blank_position();
        frozen.can_attack = true;
        frozen.has_freeze_area = true;
        let mut foe = hostile(blank_battle());
        if let Some(unit) = foe.unit.as_mut() {
            unit.has_freeze_area = true;
            unit.tile_freezable = true;
            unit.has_convert = true;
        }
        frozen.neighbors = vec![foe];
        let battle = rate_battle(&blank_battle());
        assert_eq!(rate_position(&frozen), -battle * 0.7 - 30.0 + 5.0 - 10.0);

        let mut hidden = blank_position();
        hidden.has_invisible = true;
        hidden.neighbors = vec![hostile(blank_battle()), hostile(blank_battle())];
        assert_eq!(rate_position(&hidden), -6.0);

        let mut terrain = blank_position();
        terrain.has_skate = true;
        terrain.has_amphibious = true;
        terrain.tile_is_land = true;
        terrain.tentacle_attack_damage = Some(15);
        terrain.unit_health = 10;
        terrain.improvement_score = Some(4.0);
        assert_eq!(rate_position(&terrain), 4.0 - 30.0 - 100.0 - 20.0 - 10.0);

        let mut war = blank_position();
        war.tile_owner = 4;
        war.player_id = 1;
        war.hate = -2500;
        war.mission_score = 8.0;
        assert_eq!(rate_position(&war), 8.0 * (1.0 - 2.0));

        war.at_peace_with_tile_owner = true;
        war.opinion_of_tile_owner = 0.0;
        assert_eq!(rate_position(&war), 8.0);

        let mut dull = blank_position();
        dull.auto_play = true;
        dull.handicap = 0;
        assert_eq!(rate_position(&dull), -50.0);
        dull.position_hash = 0x8000_0000;
        assert_eq!(rate_position(&dull), 0.0);
    }

    fn blank_improvement() -> ImprovementScoreInput {
        ImprovementScoreInput {
            can_build: true,
            reward_population: 0,
            growth_population: 0,
            has_patina: false,
            max_level: 0,
            reward_currency: 0,
            work: 0,
            cost: 1,
            created_unit_score: None,
            adjacency_bonus: None,
            adjacency_contains_polaris_climate: false,
            frozen_tile_count: 0,
            ruling_city: None,
            is_route_opener: false,
            player_cities: 0,
            rules_are_perfection: false,
            reward_score: 0,
            ability_score: 0.0,
            improvement_idx: 5,
            resource_required: None,
        }
    }

    #[test]
    fn improvement_score_stacks_population_city_and_resource_terms() {
        let mut blocked = blank_improvement();
        blocked.can_build = false;
        blocked.reward_population = 9;
        assert_eq!(get_improvement_score(&blocked), 0.0);

        let mut base = blank_improvement();
        base.reward_population = 2;
        base.reward_currency = 3;
        base.work = 1;
        assert_eq!(get_improvement_score(&base), 76.0);

        base.has_patina = true;
        base.max_level = 3;
        base.growth_population = 4;
        assert_eq!(get_improvement_score(&base), 20.0 + (6.0 + 350.0));

        let mut adjacent = blank_improvement();
        adjacent.work = 1;
        adjacent.growth_population = 4;
        adjacent.adjacency_bonus = Some(2);
        assert_eq!(get_improvement_score(&adjacent), 220.0);

        adjacent.adjacency_bonus = Some(0);
        adjacent.adjacency_contains_polaris_climate = true;
        adjacent.frozen_tile_count = 25;
        assert_eq!(get_improvement_score(&adjacent), 20.0 + 80.0 + 20.0);

        let mut city = blank_improvement();
        city.cost = 4;
        city.ruling_city = Some(RulingCityScore {
            is_city: true,
            xp: 3,
            level: 2,
            is_connected: true,
            neighbor_route_opener: false,
            same_as_start_tile: false,
        });
        assert_eq!(get_improvement_score(&city), 50.0);

        city.is_route_opener = true;
        city.player_cities = 2;
        if let Some(ruling) = city.ruling_city.as_mut() {
            ruling.is_connected = false;
            ruling.same_as_start_tile = true;
        }
        assert_eq!(get_improvement_score(&city), 50.0 + 30.0 + 80.0);

        if let Some(ruling) = city.ruling_city.as_mut() {
            ruling.neighbor_route_opener = true;
        }
        assert_eq!(get_improvement_score(&city), 50.0);

        let mut mode = blank_improvement();
        mode.reward_population = 2;
        mode.reward_currency = 3;
        mode.work = 1;
        mode.rules_are_perfection = true;
        mode.reward_score = 25;
        mode.ability_score = 1.0;
        mode.resource_required = Some(false);
        assert_eq!(get_improvement_score(&mode), (76.0 + 2.5 + 1.0) * 0.5);

        mode.improvement_idx = 3;
        assert_eq!(get_improvement_score(&mode), 76.0 + 2.5 + 1.0);
    }

    fn zero_stats() -> UnitStats {
        UnitStats { defence: 1.0, attack: 0.0, movement: 0.0, range: 0.0, abilities: Vec::new() }
    }

    #[test]
    fn train_commands_add_threat_defence_and_keep_positive_prices() {
        let offer = TrainOffer {
            player: 1,
            unit_type: 2,
            at: WorldCoordinates { x: 0, y: 0 },
            cost: 2,
            defence: 10,
            attack: 0,
            movement: 0,
            range: 0,
            abilities: Vec::new(),
            valid: true,
            position_score: None,
        };
        let city =
            TrainCity { explored: true, threat: 2.0, desired: zero_stats(), trains: vec![offer], upgrades: Vec::new() };
        let scored = add_possible_train_unit_commands(5, 0.0, &[city]);
        assert_eq!(scored.len(), 1);
        assert_eq!(scored[0].score, 141.0);
        assert_eq!(scored[0].command_type, command_type::TRAIN);

        let quiet = TrainCity {
            explored: true,
            threat: 0.0,
            desired: zero_stats(),
            trains: vec![TrainOffer { defence: 0, valid: true, ..scored_offer() }],
            upgrades: Vec::new(),
        };
        assert!(add_possible_train_unit_commands(5, 0.0, &[quiet]).is_empty());
    }

    fn scored_offer() -> TrainOffer {
        TrainOffer {
            player: 1,
            unit_type: 2,
            at: WorldCoordinates { x: 0, y: 0 },
            cost: 2,
            defence: 0,
            attack: 0,
            movement: 0,
            range: 0,
            abilities: Vec::new(),
            valid: true,
            position_score: None,
        }
    }

    #[test]
    fn research_price_accumulates_units_improvements_and_city_upgrades() {
        let tech = ResearchOffer {
            player: 1,
            tech: 4,
            price: 0,
            cities: 2,
            unit_build_scores: vec![3.0],
            improvements: vec![ResearchImprovement { score_sum: 10.0, limited: false, limited_scale: 0.0 }],
            ability_unlocks: 0,
            upgradeable_cities: 1,
            need: 0,
        };
        let scored = add_possible_research_commands(0, &[tech]);
        assert_eq!(scored.len(), 1);
        assert_eq!(scored[0].score, 28.0);
        assert_eq!(scored[0].command_type, command_type::RESEARCH);
    }

    #[test]
    fn improvement_commands_require_a_priced_score_above_one() {
        let kept = ImprovementOffer {
            player: 1,
            improvement: 4,
            at: WorldCoordinates { x: 1, y: 1 },
            base_score: 2.0,
            cost: 1,
            reward_population: 0,
            growth_population: 0,
            city_tracked: false,
            upgrade_need: 0,
        };
        let scored = add_possible_improvement_commands(1, &[kept]);
        assert_eq!(scored.len(), 1);
        assert_eq!(scored[0].score, 2.0);

        let tied = ImprovementOffer { base_score: 1.0, ..kept };
        assert!(add_possible_improvement_commands(1, &[tied]).is_empty());
    }

    #[test]
    fn reward_population_counts_only_for_a_positive_base_score_but_growth_population_always_does() {
        let offer = ImprovementOffer {
            player: 1,
            improvement: 8,
            at: WorldCoordinates { x: 1, y: 1 },
            base_score: 3.0,
            cost: 1,
            reward_population: 1,
            growth_population: 0,
            city_tracked: true,
            upgrade_need: 4,
        };
        // 140 / 4 = 35 for each population per star of cost.
        assert_eq!(add_possible_improvement_commands(1, &[offer])[0].score, 38.0);
        for base_score in [0.0, -2.0] {
            let low = ImprovementOffer { base_score, ..offer };
            assert!(add_possible_improvement_commands(1, &[low]).is_empty(), "base score {base_score}");
        }
        let growth = ImprovementOffer { base_score: -2.0, reward_population: 0, growth_population: 1, ..offer };
        assert_eq!(add_possible_improvement_commands(1, &[growth])[0].score, 33.0);
    }

    #[test]
    fn road_score_divides_city_potential_by_roadless_tiles() {
        let plan = RoadPlan {
            player: 1,
            city_potential: 9,
            steps: vec![
                RoadStep { at: WorldCoordinates { x: 0, y: 0 }, has_road: true },
                RoadStep { at: WorldCoordinates { x: 1, y: 0 }, has_road: false },
            ],
        };
        let scored = add_possible_road_building_commands(&[plan]);
        assert_eq!(scored.len(), 1);
        assert_eq!(scored[0].score, 9.0);
        assert_eq!(
            scored[0].command,
            TileCommand::Build { player: 1, improvement: ROAD_IMPROVEMENT, at: WorldCoordinates { x: 1, y: 0 } }
        );
    }

    #[test]
    fn diplomacy_keeps_break_peace_peace_and_positive_embassy() {
        let offers = [
            DiplomacyOffer::BreakPeace {
                player: 1,
                opponent: 2,
                at: WorldCoordinates { x: 0, y: 1 },
                opinion: -1.0,
                units_in_borders: 0,
            },
            DiplomacyOffer::Peace { player: 1, opponent: 2, range_sample: 1.0, opinion: 3.0 },
            DiplomacyOffer::Embassy { player: 1, opponent: 2, score: 0.0 },
            DiplomacyOffer::Embassy { player: 1, opponent: 3, score: 0.5 },
        ];
        let scored = add_possible_diplomacy_commands(&offers);
        assert_eq!(scored.len(), 3);
        assert_eq!(scored[0].score, 500.0);
        assert_eq!(scored[1].score, 1000.0);
        assert_eq!(scored[2].score, 0.5);
    }

    #[test]
    fn embassy_score_uses_income_peace_and_aggression() {
        let base = EmbassyScoreInput {
            version: 123,
            currency: 10,
            embassy_income: 4,
            embassy_cost_base: 8,
            embassy_count: 1,
            aggression: 100,
            at_peace: true,
            at_war: false,
        };
        assert_eq!(embassy_cost_for_player(123, 8, 1), 6);
        assert_eq!(embassy_cost_for_player(113, 8, 1), 8);
        assert_eq!(get_embassy_score(&base), 23.9);
        assert_eq!(get_embassy_score(&EmbassyScoreInput { aggression: 5001, ..base }), 0.0);
        assert_eq!(get_embassy_score(&EmbassyScoreInput { at_war: true, ..base }), 0.0);
    }
}
