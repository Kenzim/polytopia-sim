//! Diplomacy and player-status queries.
//!
//! Relations live in [`GameState::diplomacy`]; a missing relation reads as the default (the game
//! creates and stores a default one on access, which reads the same).

use crate::map::contents::{IMPROVEMENT_CITY as IMPROVEMENT_CITY_KIND, NATURE_PLAYER_ID};
use crate::{DiplomacyRelation, DiplomacyRelationState, GameState, DEATH_CONDITION_UNITS};

/// The relation `player` holds toward `opponent`.
///
/// For `opponent == 0` the game returns a fresh default that is not stored.
pub fn get_relation(state: &GameState, player: u8, opponent: u8) -> DiplomacyRelation {
    if opponent == 0 {
        return DiplomacyRelation::default();
    }
    state
        .diplomacy
        .iter()
        .find(|row| row.player == player)
        .and_then(|row| row.relations.iter().find(|link| link.other == opponent))
        .map(|link| link.relation)
        .unwrap_or_default()
}

/// Mutable access that inserts the default relation like the game does.
pub fn get_relation_mut(state: &mut GameState, player: u8, opponent: u8) -> &mut DiplomacyRelation {
    let row = match state.diplomacy.iter().position(|row| row.player == player) {
        Some(at) => at,
        None => {
            state.diplomacy.push(crate::PlayerDiplomacy { player, relations: Vec::new() });
            state.diplomacy.len() - 1
        }
    };
    let links = &mut state.diplomacy[row].relations;
    let at = match links.iter().position(|link| link.other == opponent) {
        Some(at) => at,
        None => {
            links.push(crate::DiplomacyLink { other: opponent, relation: DiplomacyRelation::default() });
            links.len() - 1
        }
    };
    &mut links[at].relation
}

/// Whether `player` is at peace with `opponent`: `opponent == 0` is false.
pub fn has_peace_with(state: &GameState, player: u8, opponent: u8) -> bool {
    opponent != 0 && get_relation(state, player, opponent).state == DiplomacyRelationState::Peace
}

/// Whether the relation is in its cool-down.
pub fn has_cool_down_with(state: &GameState, player: u8, opponent: u8) -> bool {
    opponent != 0 && get_relation(state, player, opponent).state == DiplomacyRelationState::CoolDown
}

/// The last attack turn of `player`'s relation toward `opponent`.
pub fn get_last_attack(state: &GameState, player: u8, opponent: u8) -> i32 {
    get_relation(state, player, opponent).last_attack_turn
}

/// Whether `player` has an embassy of `opponent`: `opponent`'s relation toward `player` has an
/// embassy level above 0.
pub fn has_embassy_with(state: &GameState, player: u8, opponent: u8) -> bool {
    get_relation(state, opponent, player).embassy_level > 0
}

/// Position in the player list, or -1.
pub fn index_of_player(state: &GameState, player: u8) -> i32 {
    state.players.iter().position(|candidate| candidate.id == player).map_or(-1, |at| at as i32)
}

/// Whether `player` and `opponent` are at war.
///
/// Peace returns false at once (only checked when `opponent != 0`). Otherwise either side's
/// last attack counts when `turn - last_attack <= 0`, or `== 1` and that player has not yet
/// acted this turn (`index_of_player >= current player index`).
pub fn has_war_with(state: &GameState, player: u8, opponent: u8) -> bool {
    if opponent != 0 && get_relation(state, player, opponent).state == DiplomacyRelationState::Peace {
        return false;
    }
    let turn = state.current_turn as i32;
    let current = i32::from(state.current_player_index);
    let first = turn.wrapping_sub(get_relation(state, player, opponent).last_attack_turn);
    if first <= 0 || (first == 1 && index_of_player(state, player) >= current) {
        return true;
    }
    let second = turn.wrapping_sub(get_relation(state, opponent, player).last_attack_turn);
    if second <= 0 {
        return true;
    }
    second == 1 && index_of_player(state, opponent) >= current
}

/// Number of players holding an embassy level above 0 toward `player`.
pub fn get_embassy_count(state: &GameState, player: u8) -> i32 {
    state.players.iter().filter(|other| get_relation(state, other.id, player).embassy_level > 0).count() as i32
}

/// Number of players (not Nature) whose start tile `player` owns.
pub fn count_capitals(state: &GameState, player: u8) -> i32 {
    let width = i32::from(state.map.width);
    state
        .players
        .iter()
        .filter(|other| other.id != NATURE_PLAYER_ID)
        .filter(|other| {
            let (x, y) = (other.start_tile.x, other.start_tile.y);
            if x < 0 || y < 0 || x >= width || y >= i32::from(state.map.height) {
                return false;
            }
            state.map.tiles[(y * width + x) as usize].owner == player
        })
        .count() as i32
}

/// Whether the player is alive. Nature is always alive. The death condition is
/// the player death condition setting: Cities (0) needs a city, Units (1) needs a city and a unit,
/// any other value is always alive.
pub fn is_alive(state: &GameState, player: u8) -> bool {
    if player == NATURE_PLAYER_ID {
        return true;
    }
    let condition = state.settings.player_death_condition;
    if condition != 0 && condition != DEATH_CONDITION_UNITS {
        return true;
    }
    let has_city = state.map.tiles.iter().any(|tile| tile.owner == player && tile.improvement == IMPROVEMENT_CITY_KIND);
    if condition == 0 || !has_city {
        return has_city;
    }
    // A unit of the player standing on the coordinates of some tile.
    let on_a_tile = |c: crate::WorldCoordinates| match crate::rules::path::coord_index(&state.map, c) {
        Some(i) if state.map.tiles[i].coordinates == c => true,
        _ => state.map.tiles.iter().any(|tile| tile.coordinates == c),
    };
    state.units.iter().any(|unit| unit.owner == player && on_a_tile(unit.coordinates))
}

/// [`is_alive`] for every player id from one pass over the tiles, or `None` under the death condition that also reads
/// units (then ask [`is_alive`] per player).
pub fn alive_by_id(state: &GameState) -> Option<[bool; 256]> {
    let condition = state.settings.player_death_condition;
    if condition != 0 && condition != DEATH_CONDITION_UNITS {
        return Some([true; 256]);
    }
    if condition != 0 {
        return None;
    }
    let mut alive = [false; 256];
    for tile in &state.map.tiles {
        if tile.improvement == IMPROVEMENT_CITY_KIND {
            alive[usize::from(tile.owner)] = true;
        }
    }
    alive[usize::from(NATURE_PLAYER_ID)] = true;
    Some(alive)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{DiplomacyLink, PlayerDiplomacy, PlayerState, WorldCoordinates};

    fn two_players() -> GameState {
        let mut state = GameState::new(123, 1);
        state.map = crate::Map::new(4, 4);
        for id in [1u8, 2, 255] {
            state.players.push(PlayerState {
                id,
                handicap: 0,
                currency: 0,
                score: 0,
                tribe: 1,
                auto_play: true,
                start_tile: WorldCoordinates { x: -1, y: -1 },
            });
        }
        state
    }

    fn set_relation(state: &mut GameState, player: u8, other: u8, relation: DiplomacyRelation) {
        *get_relation_mut(state, player, other) = relation;
    }

    #[test]
    fn missing_relation_reads_as_default_and_opponent_zero_is_never_peace() {
        let state = two_players();
        assert_eq!(get_relation(&state, 1, 2), DiplomacyRelation::default());
        assert!(!has_peace_with(&state, 1, 0));
        assert!(!has_peace_with(&state, 1, 2));
    }

    #[test]
    fn peace_and_cooldown_read_the_relation_state() {
        let mut state = two_players();
        set_relation(
            &mut state,
            1,
            2,
            DiplomacyRelation { state: DiplomacyRelationState::Peace, ..Default::default() },
        );
        set_relation(
            &mut state,
            2,
            1,
            DiplomacyRelation { state: DiplomacyRelationState::CoolDown, ..Default::default() },
        );
        assert!(has_peace_with(&state, 1, 2));
        assert!(!has_peace_with(&state, 2, 1));
        assert!(has_cool_down_with(&state, 2, 1));
    }

    #[test]
    fn war_follows_attack_recency_and_turn_order() {
        let mut state = two_players();
        state.current_turn = 10;
        state.current_player_index = 1;
        // Never attacked: the default last attack turn is -100.
        assert!(!has_war_with(&state, 1, 2));
        // Attacked this turn.
        set_relation(&mut state, 1, 2, DiplomacyRelation { last_attack_turn: 10, ..Default::default() });
        assert!(has_war_with(&state, 1, 2));
        // One turn ago counts only when that player has not yet acted (index >= current).
        set_relation(&mut state, 1, 2, DiplomacyRelation { last_attack_turn: 9, ..Default::default() });
        assert!(!has_war_with(&state, 1, 2)); // player 1 is index 0 < current index 1
        set_relation(&mut state, 2, 1, DiplomacyRelation { last_attack_turn: 9, ..Default::default() });
        assert!(has_war_with(&state, 1, 2)); // player 2 is index 1 >= 1
                                             // Peace overrides.
        set_relation(
            &mut state,
            1,
            2,
            DiplomacyRelation { state: DiplomacyRelationState::Peace, last_attack_turn: 10, ..Default::default() },
        );
        assert!(!has_war_with(&state, 1, 2));
    }

    #[test]
    fn embassy_counts_and_alive_checks() {
        let mut state = two_players();
        set_relation(&mut state, 2, 1, DiplomacyRelation { embassy_level: 2, ..Default::default() });
        assert!(has_embassy_with(&state, 1, 2));
        assert!(!has_embassy_with(&state, 2, 1));
        assert_eq!(get_embassy_count(&state, 1), 1);

        assert!(is_alive(&state, 255));
        assert!(!is_alive(&state, 1));
        state.map.tiles[5].owner = 1;
        state.map.tiles[5].improvement = IMPROVEMENT_CITY_KIND;
        assert!(is_alive(&state, 1));
        state.settings.player_death_condition = DEATH_CONDITION_UNITS;
        assert!(!is_alive(&state, 1));
        state.units.push(crate::UnitState {
            owner: 1,
            coordinates: state.map.tiles[5].coordinates,
            ..Default::default()
        });
        assert!(is_alive(&state, 1));
    }

    #[test]
    fn capitals_are_start_tiles_owned_by_the_player() {
        let mut state = two_players();
        state.players[0].start_tile = WorldCoordinates { x: 1, y: 1 };
        state.players[1].start_tile = WorldCoordinates { x: 2, y: 2 };
        state.map.tiles[5].owner = 1;
        state.map.tiles[10].owner = 1;
        assert_eq!(count_capitals(&state, 1), 2);
        assert_eq!(count_capitals(&state, 2), 0);
        let _ = (PlayerDiplomacy::default(), DiplomacyLink { other: 0, relation: DiplomacyRelation::default() });
    }
}
