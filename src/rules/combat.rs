//! Combat resolution: battle results, retaliation and the defence bonus.
//!
//! Rounding goes through [`crate::rng::round_to_10`] and [`crate::rng::floor_to_10`].

use crate::rng::{floor_to_10, round_to_10};

/// The Ice Arrow weapon. On game version `<= 119` this weapon cannot retaliate.
pub const WEAPON_ICE_ARROW: i32 = 10;

/// Game version threshold in [`can_defender_retaliate`].
///
/// `version <= 119` rejects a defender with attack 0 and Ice Arrow.
/// `version > 119` rejects the attacker's Freeze ability instead.
pub const OLD_RETALIATION_VERSION: i32 = 119;

/// [`defence_bonus`] returns 7 immediately when `version <= 114` and Poisoned.
pub const OLD_POISON_DEFENCE_VERSION: i32 = 114;
/// `version >= 115` and Poisoned shifts the bonus right by 1.
pub const POISON_SHIFT_DEFENCE_VERSION: i32 = 115;
/// Flat return of the `version <= 114` poison arm.
pub const POISON_DEFENCE_BONUS: i32 = 7;
/// Bonus before tech, city, and the poison shift.
pub const BASE_DEFENCE_BONUS: i32 = 10;
/// Bonus when [`unlocked_defence_bonus`] is `>= 2`, and when Fortify has no City Wall.
pub const TECH_DEFENCE_BONUS: i32 = 15;
/// Own fortified city whose improvement has the City Wall reward.
pub const CITY_WALL_DEFENCE_BONUS: i32 = 40;
/// [`unlocked_defence_bonus`] default when no unlocked tech names the terrain.
pub const DEFAULT_UNLOCKED_DEFENCE_BONUS: i32 = 1;
/// [`defence_bonus`] treats an unlocked value at or above this as the tech bonus.
pub const DEFENCE_BONUS_UNLOCK_THRESHOLD: i32 = 2;
/// The Fortify unit ability.
pub const FORTIFY_ABILITY: i32 = 22;
/// The City Wall city reward.
pub const CITY_WALL_REWARD: i32 = 1;
/// The City improvement type.
pub const CITY_IMPROVEMENT: i32 = 1;

/// [`has_active_peace_treaty`] includes the cool-down state only when `version > 99`.
pub const PEACE_COOLDOWN_VERSION: i32 = 99;
/// Diplomacy relation state Peace.
pub const RELATION_PEACE: i32 = 1;
/// Diplomacy relation state CoolDown.
pub const RELATION_COOLDOWN: i32 = 3;

/// The outcome of one attack.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BattleResults {
    pub attack_damage: i32,
    pub retaliation_damage: i32,
    pub should_move_to_defeated_enemy_tile: bool,
    pub should_reset_pacifist_task: bool,
}

impl BattleResults {
    /// Half of `attack_damage`, toward zero, then floored to a multiple of 10.
    pub fn floored_half_attack_damage(self) -> i32 {
        floor_to_10(self.attack_damage / 2)
    }
}

/// Stats and flags [`get_battle_results`] reads off the two units.
///
/// `attack` is the scaled attack ([`scaled_attack`]), not the raw data
/// field. `defence` is the scaled defence. `defence_bonus` is
/// the [`defence_bonus`] result. Attack and defence stay inputs because the
/// unit-data multiply is applied by the caller.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CombatUnit {
    /// Unit health (a 16-bit field in the game). Values outside `0..=65535` are
    /// truncated to 16 bits.
    pub health: i32,
    /// Maximum health. See [`promoted_max_health`] for the
    /// arithmetic; the passenger redirect is not applied here.
    pub max_health: i32,
    /// Scaled attack. Ignored for the damage stat when `has_tentacles`.
    pub attack: i32,
    /// Scaled defence (`raw_defence * defence_bonus` when unit data exists).
    pub defence: i32,
    /// [`defence_bonus`] result. Retaliation divides by this.
    pub defence_bonus: i32,
    /// Raw attack from the unit data. Zero means the unit has no attack options
    /// unless the defender has Convert or Infiltrate.
    pub raw_attack: i32,
    /// Raw defence from the unit data. Tentacles substitutes `raw_defence * 10`.
    pub raw_defence: i32,
    /// Range from the unit data. Only range 1 may step onto a killed enemy.
    pub range: i32,
    /// Weapon type from the unit data.
    pub weapon: i32,
    pub has_stiff: bool,
    pub has_convert: bool,
    pub has_surprise: bool,
    pub has_freeze: bool,
    pub has_tentacles: bool,
    pub has_leader: bool,
    pub has_infiltrate: bool,
    pub frozen: bool,
    pub invisible: bool,
}

/// Call-site inputs that are not fields of the two units.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BattleQuery {
    pub game_version: i32,
    pub account_for_peace: bool,
    /// [`has_active_peace_treaty`] for the defender against the attacker's player.
    /// Relations are not stored on [`crate::PlayerState`].
    pub has_active_peace_treaty: bool,
    /// "Simple data" mode of [`get_battle_results`]. Skips the move and pacifist flags.
    pub simple_data: bool,
    /// The defender's attack options (ignoring diplomacy, at its range) contain the attacker's tile.
    ///
    /// The position-based attack option scan is not ported. Leader and
    /// zero-attack empty lists are applied inside [`can_defender_retaliate`];
    /// this flag is only the remaining position scan.
    pub attacker_in_attack_options: bool,
    /// The path finder returned a path. The search is not ported. The game
    /// allows occupied tiles for it, with the attacker's movement as the
    /// maximum cost.
    pub can_path_onto_defender: bool,
}

/// The defence bonus unlocked by tech for a terrain.
///
/// Walks unlocked tech in order. An empty `defence_bonus_unlocks` map is skipped.
/// The first map that contains `terrain` supplies the value. If none do, the
/// result is [`DEFAULT_UNLOCKED_DEFENCE_BONUS`].
///
/// `terrain` is the JSON key on [`crate::data::TechData::defence_bonus_unlocks`]
/// (`"forest"`, `"mountain"`, `"water"`, `"ocean"`, …), not a guessed number.
/// Pass the player's unlocked tech in the game's order.
pub fn unlocked_defence_bonus(tech_bonuses: &[std::collections::BTreeMap<String, i32>], terrain: &str) -> i32 {
    for bonuses in tech_bonuses {
        if bonuses.is_empty() {
            continue;
        }
        if let Some(value) = bonuses.get(terrain) {
            return *value;
        }
    }
    DEFAULT_UNLOCKED_DEFENCE_BONUS
}

/// Inputs for [`defence_bonus`]. Relation and path data are not read here.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DefenceBonusInput {
    pub version: i32,
    pub poisoned: bool,
    /// The map has a tile there. A missing tile stays at the base bonus.
    pub tile_present: bool,
    /// Unit owner. Zero skips the tech bonus.
    pub owner: u8,
    /// The owner's player lookup succeeded. A failed lookup skips the tech bonus.
    pub player_resolved: bool,
    /// [`unlocked_defence_bonus`] for this tile's terrain.
    pub unlocked_defence_bonus: i32,
    /// Improvement exists and its data lookup succeeded. Either miss skips Fortify.
    pub improvement_known: bool,
    /// The tile has a City improvement.
    pub is_city: bool,
    /// `tile.owner == unit.owner`.
    pub same_owner: bool,
    /// The unit's abilities contain Fortify (22). A missing list does not.
    pub has_fortify: bool,
    /// The improvement has the City Wall reward.
    pub has_city_wall: bool,
}

/// The defence bonus of a unit.
///
/// `version <= 114` and Poisoned returns 7 and does not shift. Otherwise the
/// bonus starts at 10. A resolved non-zero owner whose unlocked bonus is
/// `>= 2` sets 15. An own city with Fortify sets 40 when the improvement has
/// City Wall, and 15 when it does not. `version >= 115` and Poisoned then
/// logically shifts that bonus right by 1.
pub fn defence_bonus(input: &DefenceBonusInput) -> i32 {
    if input.version <= OLD_POISON_DEFENCE_VERSION && input.poisoned {
        return POISON_DEFENCE_BONUS;
    }
    let mut bonus = BASE_DEFENCE_BONUS;
    if input.tile_present {
        if input.owner != 0 && input.player_resolved {
            let tech = i32::from(input.unlocked_defence_bonus >= DEFENCE_BONUS_UNLOCK_THRESHOLD);
            bonus = tech.wrapping_mul(5).wrapping_add(BASE_DEFENCE_BONUS);
        }
        if input.improvement_known && input.is_city && input.same_owner && input.has_fortify {
            bonus = if input.has_city_wall { CITY_WALL_DEFENCE_BONUS } else { TECH_DEFENCE_BONUS };
        }
    }
    if input.version >= POISON_SHIFT_DEFENCE_VERSION && input.poisoned {
        bonus = ((bonus as u32) >> 1) as i32;
    }
    bonus
}

/// Whether a unit is covered by a peace treaty with another player.
///
/// The same owner is always treated as a treaty. Otherwise this is
/// [`crate::rules::diplomacy::has_peace_with`]. `version > 99` also accepts
/// [`crate::rules::diplomacy::has_cool_down_with`].
///
/// Both return false when `unit_owner == 0` and otherwise compare
/// the relation state (`Peace` is 1, `CoolDown` is 3). The game throws when
/// the relation entry is missing. Relations are not on
/// [`crate::PlayerState`], so `relation_is_peace` and `relation_is_cooldown`
/// are those compares. [`get_battle_results`] still takes the resulting bool.
pub fn has_active_peace_treaty(
    version: i32,
    unit_owner: u8,
    other_player_id: u8,
    relation_is_peace: bool,
    relation_is_cooldown: bool,
) -> bool {
    if other_player_id == unit_owner {
        return true;
    }
    let peace = unit_owner != 0 && relation_is_peace;
    if version > PEACE_COOLDOWN_VERSION {
        peace || (unit_owner != 0 && relation_is_cooldown)
    } else {
        peace
    }
}

/// The scaled attack of a unit.
///
/// Unboosted: the raw attack times 10. Boosted: that value plus 50.
pub fn scaled_attack(raw_attack: i32, boosted: bool) -> i32 {
    if boosted {
        let times5 = raw_attack.wrapping_mul(5);
        times5.wrapping_add(times5).wrapping_add(50)
    } else {
        let doubled = raw_attack.wrapping_add(raw_attack);
        doubled.wrapping_add(doubled.wrapping_mul(4))
    }
}

/// Inner arithmetic of the maximum health: `promotion_level * 50 + unit_data.health`.
///
/// `promotion_level` is a 16-bit field. If the unit has a passenger and
/// lacks Protect (unit ability 39), the game uses the passenger's data
/// and level instead. That redirect is not applied here.
pub fn promoted_max_health(unit_data_health: i32, promotion_level: i32) -> i32 {
    let level = i32::from(promotion_level as u16);
    level.wrapping_mul(50).wrapping_add(unit_data_health)
}

/// The outcome of an attack.
///
/// Attack and retaliation are rounded to a multiple of 10. A result of
/// `attack_damage == 0` and `retaliation_damage == -1` is the early-out used
/// for peace, non-positive max health, and a non-positive force total (the
/// game logs and returns that sentinel).
pub fn get_battle_results(attacker: &CombatUnit, defender: &CombatUnit, query: &BattleQuery) -> BattleResults {
    if query.account_for_peace && query.has_active_peace_treaty {
        return no_battle();
    }
    if attacker.max_health <= 0 || defender.max_health <= 0 {
        return no_battle();
    }

    let attack_stat = damage_attack_stat(attacker);
    let attack_force = scaled_force(attacker.health, attack_stat, attacker.max_health);
    let defense_force = scaled_force(defender.health, defender.defence, defender.max_health);
    let total = attack_force.wrapping_add(defense_force);
    if total <= 0 {
        return no_battle();
    }

    let attack_damage = round_to_10(damage_quotient(
        i64::from(attack_stat).wrapping_mul(attack_force).wrapping_mul(450),
        total.wrapping_mul(1000),
    ));

    let mut retaliation_damage =
        if can_defender_retaliate(attacker, defender, query.game_version, query.attacker_in_attack_options) {
            // `defense_force * defence * 4500 / (total * 1000 * defence_bonus)`.
            // The scaled defence already includes the bonus, and this divides it back out.
            round_to_10(damage_quotient(
                defense_force.wrapping_mul(i64::from(defender.defence)).wrapping_mul(4500),
                total.wrapping_mul(1000).wrapping_mul(i64::from(defender.defence_bonus)),
            ))
        } else {
            -1
        };

    let defender_health = i32::from(defender.health as u16);
    let killed = attack_damage >= defender_health;
    if killed {
        retaliation_damage = 0;
    }

    if query.simple_data {
        return BattleResults {
            attack_damage,
            retaliation_damage,
            should_move_to_defeated_enemy_tile: false,
            should_reset_pacifist_task: false,
        };
    }

    let should_move = killed && attacker.range == 1 && query.can_path_onto_defender;
    let should_reset = attack_damage > 0 && !defender.invisible;
    BattleResults {
        attack_damage,
        retaliation_damage,
        should_move_to_defeated_enemy_tile: should_move,
        should_reset_pacifist_task: should_reset,
    }
}

/// Whether the defender retaliates, including the empty attack-option
/// shortcuts.
///
/// `attacker_in_attack_options` is the position scan only. A defender with a
/// leader, or with raw attack 0 and neither Convert nor Infiltrate, cannot
/// retaliate even when that flag is set.
pub fn can_defender_retaliate(
    attacker: &CombatUnit,
    defender: &CombatUnit,
    game_version: i32,
    attacker_in_attack_options: bool,
) -> bool {
    if defender.has_stiff || attacker.has_convert || attacker.has_surprise || defender.frozen {
        return false;
    }
    if game_version > OLD_RETALIATION_VERSION {
        if attacker.has_freeze {
            return false;
        }
    } else if defender.attack == 0 || defender.weapon == WEAPON_ICE_ARROW {
        return false;
    }
    if defender.has_leader {
        return false;
    }
    if defender.raw_attack == 0 && !defender.has_convert && !defender.has_infiltrate {
        return false;
    }
    attacker_in_attack_options
}

fn no_battle() -> BattleResults {
    BattleResults {
        attack_damage: 0,
        retaliation_damage: -1,
        should_move_to_defeated_enemy_tile: false,
        should_reset_pacifist_task: false,
    }
}

/// Tentacles (unit ability 44) replaces the attack with the raw defence times 10.
fn damage_attack_stat(unit: &CombatUnit) -> i32 {
    if unit.has_tentacles {
        let doubled = unit.raw_defence.wrapping_add(unit.raw_defence);
        doubled.wrapping_add(doubled.wrapping_mul(4))
    } else {
        unit.attack
    }
}

/// `(health * stat * 100) / max_health`, with 32-bit wrapping multiplies.
fn scaled_force(health: i32, stat: i32, max_health: i32) -> i64 {
    let health = i32::from(health as u16);
    let prod = health.wrapping_mul(stat).wrapping_mul(100);
    idiv_combat(i64::from(prod), i64::from(max_health))
}

/// Quotient narrowed to 32 bits before rounding.
fn damage_quotient(numer: i64, denom: i64) -> i32 {
    idiv_combat(numer, denom) as i32
}

/// The game's mixed divide: unsigned 32-bit division when both operands fit
/// in 32 bits, otherwise signed 64-bit division (toward zero).
fn idiv_combat(numer: i64, denom: i64) -> i64 {
    let marker = (numer as u64) | (denom as u64);
    if marker >> 32 == 0 {
        i64::from((numer as u32) / (denom as u32))
    } else {
        numer / denom
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn unit(health: i32, max_health: i32, attack: i32, defence: i32, bonus: i32) -> CombatUnit {
        CombatUnit {
            health,
            max_health,
            attack,
            defence,
            defence_bonus: bonus,
            raw_attack: if attack == 0 { 0 } else { 1 },
            raw_defence: 0,
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
        }
    }

    fn query() -> BattleQuery {
        BattleQuery {
            game_version: 120,
            account_for_peace: false,
            has_active_peace_treaty: false,
            simple_data: false,
            attacker_in_attack_options: true,
            can_path_onto_defender: false,
        }
    }

    #[test]
    fn scaled_attack_and_promoted_health() {
        assert_eq!(scaled_attack(2, false), 20);
        assert_eq!(scaled_attack(2, true), 70);
        assert_eq!(scaled_attack(0, true), 50);
        assert_eq!(promoted_max_health(100, 0), 100);
        assert_eq!(promoted_max_health(100, 1), 150);
        assert_eq!(promoted_max_health(10, 2), 110);
    }

    #[test]
    fn equal_full_health_rounds_the_quotient() {
        // attack stat 200, defence 200, bonus 10, 100/100.
        // forces 20000 + 20000, quotient 45, retaliation quotient 45.
        let attacker = unit(100, 100, 200, 0, 10);
        let defender = unit(100, 100, 200, 200, 10);
        let result = get_battle_results(&attacker, &defender, &query());
        assert_eq!(result.attack_damage, round_to_10(45));
        assert_eq!(result.retaliation_damage, round_to_10(45));
        assert!(!result.should_move_to_defeated_enemy_tile);
        assert!(result.should_reset_pacifist_task);
        assert_eq!(result.floored_half_attack_damage(), floor_to_10(result.attack_damage / 2));
    }

    #[test]
    fn small_equal_stats_quotient_is_four() {
        // Same shape with stat 20: quotient 4. Round, not a second formula, decides it.
        let attacker = unit(100, 100, 20, 0, 10);
        let defender = unit(100, 100, 20, 20, 10);
        let result = get_battle_results(&attacker, &defender, &query());
        assert_eq!(result.attack_damage, round_to_10(4));
        assert_eq!(result.retaliation_damage, round_to_10(4));
        assert!(!result.should_reset_pacifist_task);
    }

    #[test]
    fn half_health_attacker_changes_both_sides() {
        // Attacker 50/100 stat 200, defender full defence 200 bonus 10.
        // attack quotient 30, retaliation quotient 60.
        let attacker = unit(50, 100, 200, 0, 10);
        let defender = unit(100, 100, 200, 200, 10);
        let result = get_battle_results(&attacker, &defender, &query());
        assert_eq!(result.attack_damage, round_to_10(30));
        assert_eq!(result.retaliation_damage, round_to_10(60));
    }

    #[test]
    fn defence_bonus_is_divided_back_out_of_retaliation() {
        // defence 30 is raw 2 * bonus 15. Attack quotient 3, retaliation quotient 5.
        let attacker = unit(100, 100, 20, 0, 10);
        let defender = unit(100, 100, 20, 30, 15);
        let result = get_battle_results(&attacker, &defender, &query());
        assert_eq!(result.attack_damage, round_to_10(3));
        assert_eq!(result.retaliation_damage, round_to_10(5));
    }

    #[test]
    fn kill_zeroes_retaliation_and_may_step_onto_the_tile() {
        // Defender 30/100 defence 20 bonus 10. Attack quotient 87.
        let attacker = unit(100, 100, 200, 0, 10);
        let defender = unit(30, 100, 20, 20, 10);
        let mut q = query();
        q.can_path_onto_defender = true;
        let result = get_battle_results(&attacker, &defender, &q);
        assert_eq!(result.attack_damage, round_to_10(87));
        assert!(result.attack_damage >= defender.health);
        assert_eq!(result.retaliation_damage, 0);
        assert!(result.should_move_to_defeated_enemy_tile);
        assert!(result.should_reset_pacifist_task);

        let mut ranged = attacker;
        ranged.range = 2;
        let ranged_result = get_battle_results(&ranged, &defender, &q);
        assert!(!ranged_result.should_move_to_defeated_enemy_tile);
        assert_eq!(ranged_result.retaliation_damage, 0);

        q.simple_data = true;
        let simple = get_battle_results(&attacker, &defender, &q);
        assert_eq!(simple.attack_damage, round_to_10(87));
        assert_eq!(simple.retaliation_damage, 0);
        assert!(!simple.should_move_to_defeated_enemy_tile);
        assert!(!simple.should_reset_pacifist_task);
    }

    #[test]
    fn surprise_suppresses_retaliation_without_clearing_damage() {
        let attacker = unit(100, 100, 200, 0, 10);
        let mut defender = unit(100, 100, 200, 200, 10);
        let mut surprised = attacker;
        surprised.has_surprise = true;
        let result = get_battle_results(&surprised, &defender, &query());
        assert_eq!(result.attack_damage, round_to_10(45));
        assert_eq!(result.retaliation_damage, -1);
        assert!(result.should_reset_pacifist_task);

        defender.invisible = true;
        let hidden = get_battle_results(&attacker, &defender, &query());
        assert_eq!(hidden.attack_damage, round_to_10(45));
        assert!(!hidden.should_reset_pacifist_task);
    }

    #[test]
    fn retaliation_gates() {
        let attacker = unit(100, 100, 200, 0, 10);
        let defender = unit(100, 100, 200, 200, 10);
        assert!(can_defender_retaliate(&attacker, &defender, 120, true));

        let mut stiff = defender;
        stiff.has_stiff = true;
        assert!(!can_defender_retaliate(&attacker, &stiff, 120, true));

        let mut frozen = defender;
        frozen.frozen = true;
        assert!(!can_defender_retaliate(&attacker, &frozen, 120, true));

        let mut convert = attacker;
        convert.has_convert = true;
        assert!(!can_defender_retaliate(&convert, &defender, 120, true));

        let mut freeze = attacker;
        freeze.has_freeze = true;
        assert!(!can_defender_retaliate(&freeze, &defender, 120, true));
        assert!(can_defender_retaliate(&freeze, &defender, 119, true));

        let mut ice = defender;
        ice.weapon = WEAPON_ICE_ARROW;
        assert!(!can_defender_retaliate(&attacker, &ice, 119, true));
        assert!(can_defender_retaliate(&attacker, &ice, 120, true));

        let mut zero = defender;
        zero.attack = 0;
        zero.raw_attack = 0;
        assert!(!can_defender_retaliate(&attacker, &zero, 119, true));
        assert!(!can_defender_retaliate(&attacker, &zero, 120, true));

        let mut mind = zero;
        mind.has_convert = true;
        assert!(!can_defender_retaliate(&attacker, &mind, 119, true));
        assert!(can_defender_retaliate(&attacker, &mind, 120, true));

        let mut led = defender;
        led.has_leader = true;
        assert!(!can_defender_retaliate(&attacker, &led, 120, true));
        assert!(!can_defender_retaliate(&attacker, &defender, 120, false));
    }

    #[test]
    fn peace_and_non_positive_forces_return_the_sentinel() {
        let attacker = unit(100, 100, 200, 0, 10);
        let defender = unit(100, 100, 200, 200, 10);
        let mut q = query();
        q.account_for_peace = true;
        q.has_active_peace_treaty = false;
        assert_eq!(get_battle_results(&attacker, &defender, &q).attack_damage, round_to_10(45));
        q.has_active_peace_treaty = true;
        assert_eq!(get_battle_results(&attacker, &defender, &q), no_battle());

        let mut broken = attacker;
        broken.max_health = 0;
        assert_eq!(get_battle_results(&broken, &defender, &query()), no_battle());

        let zero_atk = unit(100, 100, 0, 0, 10);
        let zero_def = unit(100, 100, 0, 0, 10);
        assert_eq!(get_battle_results(&zero_atk, &zero_def, &query()), no_battle());
    }

    #[test]
    fn tentacles_use_defence_times_ten() {
        // raw defence 4 -> attack stat 40. Both sides quotient 9.
        let mut attacker = unit(100, 100, 999, 0, 10);
        attacker.has_tentacles = true;
        attacker.raw_defence = 4;
        let defender = unit(100, 100, 40, 40, 10);
        let result = get_battle_results(&attacker, &defender, &query());
        assert_eq!(result.attack_damage, round_to_10(9));
        assert_eq!(result.retaliation_damage, round_to_10(9));
    }

    #[test]
    fn attack_force_multiply_wraps_at_32_bits() {
        // health 100, stat 50_000_000. The 32-bit `health * stat * 100` wraps;
        // the unwrapped product would be 5_000_000_000. Quotient is 22474801.
        let attacker = unit(100, 100, 50_000_000, 0, 10);
        let defender = unit(100, 100, 200, 200, 10);
        let result = get_battle_results(&attacker, &defender, &query());
        assert_eq!(result.attack_damage, round_to_10(22_474_801));
        assert_eq!(result.retaliation_damage, 0);
    }

    #[test]
    fn defence_bonus_uses_the_unlock_table_and_both_poison_arms() {
        let mut forest = std::collections::BTreeMap::new();
        forest.insert("forest".to_string(), 15);
        let mut weak = std::collections::BTreeMap::new();
        weak.insert("forest".to_string(), 1);
        let empty = std::collections::BTreeMap::new();
        assert_eq!(unlocked_defence_bonus(&[empty, forest.clone()], "forest"), 15);
        assert_eq!(unlocked_defence_bonus(&[weak.clone(), forest.clone()], "forest"), 1);
        assert_eq!(unlocked_defence_bonus(&[forest, weak], "mountain"), DEFAULT_UNLOCKED_DEFENCE_BONUS);

        let mut input = DefenceBonusInput {
            version: 115,
            poisoned: false,
            tile_present: true,
            owner: 1,
            player_resolved: true,
            unlocked_defence_bonus: unlocked_defence_bonus(
                &[std::collections::BTreeMap::from([("forest".to_string(), 15)])],
                "forest",
            ),
            improvement_known: false,
            is_city: false,
            same_owner: false,
            has_fortify: false,
            has_city_wall: false,
        };
        assert_eq!(input.unlocked_defence_bonus, 15);
        assert_eq!(defence_bonus(&input), TECH_DEFENCE_BONUS);
        input.unlocked_defence_bonus = 1;
        assert_eq!(defence_bonus(&input), BASE_DEFENCE_BONUS);
        input.unlocked_defence_bonus = 2;
        assert_eq!(defence_bonus(&input), TECH_DEFENCE_BONUS);
        input.owner = 0;
        assert_eq!(defence_bonus(&input), BASE_DEFENCE_BONUS);
        input.owner = 1;
        input.player_resolved = false;
        assert_eq!(defence_bonus(&input), BASE_DEFENCE_BONUS);

        input.player_resolved = true;
        input.unlocked_defence_bonus = 1;
        input.improvement_known = true;
        input.is_city = true;
        input.same_owner = true;
        input.has_fortify = true;
        input.has_city_wall = false;
        assert_eq!(defence_bonus(&input), TECH_DEFENCE_BONUS);
        input.has_city_wall = true;
        assert_eq!(defence_bonus(&input), CITY_WALL_DEFENCE_BONUS);
        assert_eq!(CITY_IMPROVEMENT, 1);
        assert_eq!(FORTIFY_ABILITY, 22);
        assert_eq!(CITY_WALL_REWARD, 1);

        input.version = OLD_POISON_DEFENCE_VERSION;
        input.poisoned = true;
        assert_eq!(defence_bonus(&input), POISON_DEFENCE_BONUS);
        input.version = POISON_SHIFT_DEFENCE_VERSION;
        assert_eq!(defence_bonus(&input), CITY_WALL_DEFENCE_BONUS >> 1);
        input.has_city_wall = false;
        input.has_fortify = false;
        input.unlocked_defence_bonus = 15;
        assert_eq!(defence_bonus(&input), TECH_DEFENCE_BONUS >> 1);
        input.tile_present = false;
        assert_eq!(defence_bonus(&input), BASE_DEFENCE_BONUS >> 1);
        input.version = 114;
        input.poisoned = false;
        input.tile_present = true;
        input.has_city_wall = true;
        input.has_fortify = true;
        assert_eq!(defence_bonus(&input), CITY_WALL_DEFENCE_BONUS);
    }

    #[test]
    fn peace_treaty_adds_cooldown_only_after_version_99() {
        assert!(has_active_peace_treaty(99, 2, 2, false, false));
        assert!(has_active_peace_treaty(100, 2, 2, false, false));
        assert!(has_active_peace_treaty(PEACE_COOLDOWN_VERSION, 2, 1, true, false));
        assert!(has_active_peace_treaty(100, 2, 1, true, false));
        assert!(!has_active_peace_treaty(99, 2, 1, false, true));
        assert!(has_active_peace_treaty(100, 2, 1, false, true));
        assert!(!has_active_peace_treaty(99, 0, 1, true, true));
        assert!(!has_active_peace_treaty(100, 0, 1, true, true));
        assert_eq!(RELATION_PEACE, 1);
        assert_eq!(RELATION_COOLDOWN, 3);
    }

    #[test]
    fn floored_half_uses_toward_zero_then_floor() {
        let positive = BattleResults {
            attack_damage: 50,
            retaliation_damage: 0,
            should_move_to_defeated_enemy_tile: false,
            should_reset_pacifist_task: false,
        };
        assert_eq!(positive.floored_half_attack_damage(), floor_to_10(25));
        let negative = BattleResults { attack_damage: -15, ..positive };
        assert_eq!(negative.floored_half_attack_damage(), floor_to_10(-7));
    }
}
