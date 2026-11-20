//! Game version constants, the supported window and the logic-data ladder.
//!
//! See `docs/specs/version.md`.

use std::fmt;

/// The current game version. Default for a new sim match.
pub const GAME_VERSION: i32 = 124;
/// Oldest version a new match may use. Floor for `for_new_match`.
pub const MIN_NEW_GAME_VERSION: i32 = 108;
/// Oldest version a save may have. Floor for `for_save`.
pub const MIN_SUPPORTED_GAME_VERSION: i32 = 51;
/// Logic-data version for [`GAME_VERSION`].
pub const GAME_LOGIC_DATA_VERSION: i32 = 28;
/// Game version of the weekly challenge.
pub const WEEKLY_CHALLENGE_GAME_VERSION: i32 = 121;
/// Oldest version tournaments accept.
pub const MIN_TOURNAMENT_GAME_VERSION: i32 = 96;

/// Named milestones rules compare against. In release order, `CYMANTI_VERSION` through
/// `ACHIEVEMENTS`.
pub const CYMANTI_VERSION: i32 = 40;
pub const BALANCE_1: i32 = 45;
pub const BALANCE_2: i32 = 50;
pub const DIPLOMACY_VERSION: i32 = 60;
pub const ESPORT_VERSION: i32 = 70;
pub const DIPLOMACY_VERSION_2: i32 = 80;
pub const DIPLOMACY_VERSION_3: i32 = 81;
pub const DIPLOMACY_VERSION_4: i32 = 82;
pub const DIPLOMACY_VERSION_5: i32 = 83;
pub const DIPLOMACY_VERSION_6: i32 = 84;
pub const DIPLOMACY_VERSION_7: i32 = 85;
pub const TRIBE_SKINS_VERSION: i32 = 86;
pub const TRIBE_SKINS_VERSION_2: i32 = 87;
pub const LOBBY_VERSION: i32 = 90;
pub const LOBBY_VERSION_2: i32 = 91;
pub const LOBBY_VERSION_3: i32 = 92;
pub const LOBBY_VERSION_4: i32 = 93;
pub const LOBBY_VERSION_5: i32 = 94;
pub const REFACTOR_VERSION: i32 = 95;
pub const SKIN_PACK_2_VERSION: i32 = 96;
pub const SKIN_PACK_3_VERSION: i32 = 97;
pub const SKIN_PACK_3_VERSION_2: i32 = 98;
pub const NAVAL_1: i32 = 99;
pub const NAVAL_2: i32 = 100;
pub const NAVAL_3: i32 = 101;
pub const NAVAL_4: i32 = 102;
pub const SKIN_PACK_4_VERSION: i32 = 103;
pub const NAVAL_5: i32 = 104;
pub const AQUARION_REWORK: i32 = 105;
pub const SUMMER24_BUG_CLEANUP: i32 = 106;
pub const AQUARION_REWORK_BALANCE: i32 = 107;
pub const AQUARION_REWORK_BALANCE_2: i32 = 108;
pub const SCORE_FIXES: i32 = 109;
pub const SCORE_FIXES_AND_PIRATE: i32 = 110;
pub const BUGFIXES_Q1_2025: i32 = 111;
pub const POLARIS_SKIN: i32 = 112;
pub const REMOVE_AGGRESSION: i32 = 113;
pub const BALANCE_2025: i32 = 114;
pub const CYMANTI_REWORK: i32 = 115;
pub const CYMANTI_REWORK_PATCH: i32 = 116;
pub const CYMANTI_REWORK_PATCH2: i32 = 117;
pub const CYMANTI_REWORK_PATCH3: i32 = 118;
pub const CYMANTI_SKIN: i32 = 119;
pub const BUG_PATCH_Q1_26: i32 = 120;
pub const PLAYER_CLIMATE: i32 = 121;
pub const CHAMPIONSHIP_FIXES: i32 = 122;
pub const AWAY_STARS_CHAMPIONSHIP_FIXES: i32 = 123;
/// The release that added the achievements system. The one rule gated on it: an explode command queues the
/// end-of-command and run-deferred actions like every other command (earlier versions did not).
pub const ACHIEVEMENTS: i32 = 124;

/// Oldest version with regression replays behind it (see [`support_level`]).
pub const MIN_TESTED_GAME_VERSION: i32 = 121;
/// Newest version with regression replays behind it. Versions above it are supported but not yet tested.
pub const MAX_TESTED_GAME_VERSION: i32 = 123;

/// Width of the supported window: versions `MIN_SUPPORTED_GAME_VERSION` plus 0..74.
const SUPPORTED_SPAN: u32 = (GAME_VERSION - MIN_SUPPORTED_GAME_VERSION + 1) as u32;

/// A named release a rule can compare against: the constant's name and its game version.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Milestone {
    pub name: &'static str,
    pub version: i32,
}

macro_rules! milestones {
    ($($name:ident),* $(,)?) => {
        [$(Milestone { name: stringify!($name), version: $name }),*]
    };
}

/// Every named milestone from `CYMANTI_VERSION` to `ACHIEVEMENTS`, oldest first.
/// The versions strictly increase.
pub const MILESTONES: [Milestone; 48] = milestones![
    CYMANTI_VERSION,
    BALANCE_1,
    BALANCE_2,
    DIPLOMACY_VERSION,
    ESPORT_VERSION,
    DIPLOMACY_VERSION_2,
    DIPLOMACY_VERSION_3,
    DIPLOMACY_VERSION_4,
    DIPLOMACY_VERSION_5,
    DIPLOMACY_VERSION_6,
    DIPLOMACY_VERSION_7,
    TRIBE_SKINS_VERSION,
    TRIBE_SKINS_VERSION_2,
    LOBBY_VERSION,
    LOBBY_VERSION_2,
    LOBBY_VERSION_3,
    LOBBY_VERSION_4,
    LOBBY_VERSION_5,
    REFACTOR_VERSION,
    SKIN_PACK_2_VERSION,
    SKIN_PACK_3_VERSION,
    SKIN_PACK_3_VERSION_2,
    NAVAL_1,
    NAVAL_2,
    NAVAL_3,
    NAVAL_4,
    SKIN_PACK_4_VERSION,
    NAVAL_5,
    AQUARION_REWORK,
    SUMMER24_BUG_CLEANUP,
    AQUARION_REWORK_BALANCE,
    AQUARION_REWORK_BALANCE_2,
    SCORE_FIXES,
    SCORE_FIXES_AND_PIRATE,
    BUGFIXES_Q1_2025,
    POLARIS_SKIN,
    REMOVE_AGGRESSION,
    BALANCE_2025,
    CYMANTI_REWORK,
    CYMANTI_REWORK_PATCH,
    CYMANTI_REWORK_PATCH2,
    CYMANTI_REWORK_PATCH3,
    CYMANTI_SKIN,
    BUG_PATCH_Q1_26,
    PLAYER_CLIMATE,
    CHAMPIONSHIP_FIXES,
    AWAY_STARS_CHAMPIONSHIP_FIXES,
    ACHIEVEMENTS,
];

/// One step of the rules-data ladder: game versions from `first_game_version` up to the next step
/// use the rules-data file `rules_version`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RulesStep {
    pub first_game_version: i32,
    pub rules_version: i32,
}

/// Which rules-data version (`data/rules/v<N>.json`) each game version uses. Sorted by
/// `first_game_version`; the first step also covers every older game version, the last every newer
/// one. Rules versions 1..=8 belong to game versions below the supported window and have no file.
pub const RULES_LADDER: [RulesStep; 28] = [
    RulesStep { first_game_version: i32::MIN, rules_version: 1 },
    RulesStep { first_game_version: 9, rules_version: 2 },
    RulesStep { first_game_version: 10, rules_version: 3 },
    RulesStep { first_game_version: 11, rules_version: 4 },
    RulesStep { first_game_version: 14, rules_version: 5 },
    RulesStep { first_game_version: 40, rules_version: 6 },
    RulesStep { first_game_version: 43, rules_version: 7 },
    RulesStep { first_game_version: 45, rules_version: 8 },
    RulesStep { first_game_version: 50, rules_version: 9 },
    RulesStep { first_game_version: 60, rules_version: 10 },
    RulesStep { first_game_version: 80, rules_version: 11 },
    RulesStep { first_game_version: 81, rules_version: 12 },
    RulesStep { first_game_version: 83, rules_version: 13 },
    RulesStep { first_game_version: 86, rules_version: 14 },
    RulesStep { first_game_version: 95, rules_version: 15 },
    RulesStep { first_game_version: 96, rules_version: 16 },
    RulesStep { first_game_version: 99, rules_version: 17 },
    RulesStep { first_game_version: 100, rules_version: 18 },
    RulesStep { first_game_version: 104, rules_version: 19 },
    RulesStep { first_game_version: 105, rules_version: 20 },
    RulesStep { first_game_version: 107, rules_version: 21 },
    RulesStep { first_game_version: 110, rules_version: 22 },
    RulesStep { first_game_version: 114, rules_version: 23 },
    RulesStep { first_game_version: 115, rules_version: 24 },
    RulesStep { first_game_version: 116, rules_version: 25 },
    RulesStep { first_game_version: 117, rules_version: 26 },
    RulesStep { first_game_version: 119, rules_version: 27 },
    RulesStep { first_game_version: 120, rules_version: 28 },
];

/// How far a game version is backed, for the traffic light in the README.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Support {
    /// Outside 51..=124: refused.
    Unsupported,
    /// Loads and runs, rules data present, but no regression replay of that version.
    Supported,
    /// Backed by regression replays (`MIN_TESTED_GAME_VERSION..=MAX_TESTED_GAME_VERSION`).
    Tested,
}

/// The traffic light for any game version: green [`Support::Tested`], amber [`Support::Supported`],
/// red [`Support::Unsupported`].
pub const fn support_level(version: i32) -> Support {
    if !is_supported(version) {
        Support::Unsupported
    } else if version >= MIN_TESTED_GAME_VERSION && version <= MAX_TESTED_GAME_VERSION {
        Support::Tested
    } else {
        Support::Supported
    }
}

/// Game version this client can load. Always in `51..=124`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct GameVersion(i32);

impl GameVersion {
    /// Ok only for 51..=124 (see [`is_supported`]).
    pub const fn for_save(n: i32) -> Result<Self, VersionError> {
        if is_supported(n) {
            Ok(Self(n))
        } else {
            Err(VersionError::Unsupported(n))
        }
    }

    /// New matches: Ok only for [`MIN_NEW_GAME_VERSION`]..=[`GAME_VERSION`] (108..=124).
    pub const fn for_new_match(n: i32) -> Result<Self, VersionError> {
        if n >= MIN_NEW_GAME_VERSION && is_supported(n) {
            Ok(Self(n))
        } else if is_supported(n) {
            Err(VersionError::BeforeNewMatch(n))
        } else {
            Err(VersionError::Unsupported(n))
        }
    }

    /// [`GAME_VERSION`] (124).
    pub const fn current() -> Self {
        Self(GAME_VERSION)
    }

    pub const fn as_i32(self) -> i32 {
        self.0
    }

    /// Logic-data file selected for this supported version.
    pub const fn logic_data_version(self) -> i32 {
        logic_data_version(self.0)
    }

    /// Every supported version, oldest first (`51..=124`).
    pub fn supported() -> impl DoubleEndedIterator<Item = GameVersion> {
        (MIN_SUPPORTED_GAME_VERSION..=GAME_VERSION).map(GameVersion)
    }

    /// Whether this version is `milestone` or later, e.g. `v.at_least(NAVAL_1)`.
    pub const fn at_least(self, milestone: i32) -> bool {
        self.0 >= milestone
    }

    /// Whether this version is older than `milestone`.
    pub const fn is_before(self, milestone: i32) -> bool {
        self.0 < milestone
    }

    /// The newest named milestone this version has reached, or `None` below `CYMANTI_VERSION`.
    pub fn latest_milestone(self) -> Option<Milestone> {
        MILESTONES.iter().rev().find(|m| m.version <= self.0).copied()
    }

    /// The traffic light of this version (always [`Support::Supported`] or [`Support::Tested`]).
    pub const fn support(self) -> Support {
        support_level(self.0)
    }

    /// The built-in rules for this version.
    pub fn load_rules(self) -> Result<crate::data::GameLogicData, crate::data::DataError> {
        crate::data::load_game_logic_data(self.logic_data_version())
    }
}

impl Default for GameVersion {
    fn default() -> Self {
        Self::current()
    }
}

impl From<GameVersion> for i32 {
    fn from(version: GameVersion) -> i32 {
        version.0
    }
}

/// Same as [`GameVersion::for_save`].
impl TryFrom<i32> for GameVersion {
    type Error = VersionError;

    fn try_from(n: i32) -> Result<Self, VersionError> {
        Self::for_save(n)
    }
}

impl fmt::Display for GameVersion {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

/// Why [`GameVersion::for_save`] or [`GameVersion::for_new_match`] rejected `n`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VersionError {
    /// Outside the supported window (51..=124).
    Unsupported(i32),
    /// Inside the save window, below [`MIN_NEW_GAME_VERSION`].
    BeforeNewMatch(i32),
}

impl fmt::Display for VersionError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            VersionError::Unsupported(version) => {
                write!(f, "game version {version} is outside {MIN_SUPPORTED_GAME_VERSION}..={GAME_VERSION}")
            }
            VersionError::BeforeNewMatch(version) => {
                write!(f, "game version {version} is below {MIN_NEW_GAME_VERSION}")
            }
        }
    }
}

impl VersionError {
    /// The version that was rejected.
    pub const fn version(self) -> i32 {
        match self {
            VersionError::Unsupported(version) | VersionError::BeforeNewMatch(version) => version,
        }
    }

    /// The rejected version is older than every supported version.
    pub const fn is_too_old(self) -> bool {
        is_too_old(self.version())
    }

    /// The rejected version is newer than [`GAME_VERSION`].
    pub const fn is_too_new(self) -> bool {
        self.version() > GAME_VERSION
    }
}

impl std::error::Error for VersionError {}

/// Whether the version is inside the supported window: true iff
/// `(version - 51) as u32 < 73`.
pub const fn is_supported(version: i32) -> bool {
    (version.wrapping_sub(MIN_SUPPORTED_GAME_VERSION) as u32) < SUPPORTED_SPAN
}

/// True when `version < 51`. A version above 124 is unsupported and not too old.
pub const fn is_too_old(version: i32) -> bool {
    version < MIN_SUPPORTED_GAME_VERSION
}

/// Logic-data version for a game version.
///
/// Does not reject versions outside 51..=124.
pub const fn logic_data_version(game_version: i32) -> i32 {
    let mut i = RULES_LADDER.len() - 1;
    while RULES_LADDER[i].first_game_version > game_version {
        i -= 1;
    }
    RULES_LADDER[i].rules_version
}

// The tables and the constants must agree; a mismatch is a compile error.
const _: () = {
    assert!(ACHIEVEMENTS == GAME_VERSION);
    assert!(logic_data_version(GAME_VERSION) == GAME_LOGIC_DATA_VERSION);
    assert!(CYMANTI_VERSION < MIN_SUPPORTED_GAME_VERSION);
    assert!(MIN_SUPPORTED_GAME_VERSION < MIN_NEW_GAME_VERSION);
    assert!(MIN_NEW_GAME_VERSION <= WEEKLY_CHALLENGE_GAME_VERSION);
    assert!(WEEKLY_CHALLENGE_GAME_VERSION <= GAME_VERSION);
    assert!(MIN_NEW_GAME_VERSION < MIN_TESTED_GAME_VERSION && MIN_TESTED_GAME_VERSION <= MAX_TESTED_GAME_VERSION);
    assert!(MAX_TESTED_GAME_VERSION <= GAME_VERSION);
    assert!(logic_data_version(MIN_SUPPORTED_GAME_VERSION) >= crate::data::rules::OLDEST_VERSION);
    assert!(RULES_LADDER[0].first_game_version == i32::MIN);
};

#[cfg(test)]
mod tests {
    use super::*;

    /// Inclusive game-version bounds and the logic-data version they share.
    const SUPPORTED_LADDER: &[(i32, i32, i32)] = &[
        (51, 59, 9),
        (60, 79, 10),
        (80, 80, 11),
        (81, 82, 12),
        (83, 85, 13),
        (86, 94, 14),
        (95, 95, 15),
        (96, 98, 16),
        (99, 99, 17),
        (100, 103, 18),
        (104, 104, 19),
        (105, 106, 20),
        (107, 109, 21),
        (110, 113, 22),
        (114, 114, 23),
        (115, 115, 24),
        (116, 116, 25),
        (117, 118, 26),
        (119, 119, 27),
        (120, 124, 28),
    ];

    #[test]
    fn milestone_constants_match_game() {
        assert_eq!(GAME_VERSION, 124);
        assert_eq!(MIN_TESTED_GAME_VERSION, 121);
        assert_eq!(MAX_TESTED_GAME_VERSION, 123);
        assert_eq!(MIN_NEW_GAME_VERSION, 108);
        assert_eq!(MIN_SUPPORTED_GAME_VERSION, 51);
        assert_eq!(GAME_LOGIC_DATA_VERSION, 28);
        assert_eq!(WEEKLY_CHALLENGE_GAME_VERSION, 121);
        assert_eq!(MIN_TOURNAMENT_GAME_VERSION, 96);
        assert_eq!(CYMANTI_VERSION, 40);
        assert_eq!(BALANCE_1, 45);
        assert_eq!(BALANCE_2, 50);
        assert_eq!(DIPLOMACY_VERSION, 60);
        assert_eq!(ESPORT_VERSION, 70);
        assert_eq!(DIPLOMACY_VERSION_2, 80);
        assert_eq!(DIPLOMACY_VERSION_3, 81);
        assert_eq!(DIPLOMACY_VERSION_4, 82);
        assert_eq!(DIPLOMACY_VERSION_5, 83);
        assert_eq!(DIPLOMACY_VERSION_6, 84);
        assert_eq!(DIPLOMACY_VERSION_7, 85);
        assert_eq!(TRIBE_SKINS_VERSION, 86);
        assert_eq!(TRIBE_SKINS_VERSION_2, 87);
        assert_eq!(LOBBY_VERSION, 90);
        assert_eq!(LOBBY_VERSION_2, 91);
        assert_eq!(LOBBY_VERSION_3, 92);
        assert_eq!(LOBBY_VERSION_4, 93);
        assert_eq!(LOBBY_VERSION_5, 94);
        assert_eq!(REFACTOR_VERSION, 95);
        assert_eq!(SKIN_PACK_2_VERSION, 96);
        assert_eq!(SKIN_PACK_3_VERSION, 97);
        assert_eq!(SKIN_PACK_3_VERSION_2, 98);
        assert_eq!(NAVAL_1, 99);
        assert_eq!(NAVAL_2, 100);
        assert_eq!(NAVAL_3, 101);
        assert_eq!(NAVAL_4, 102);
        assert_eq!(SKIN_PACK_4_VERSION, 103);
        assert_eq!(NAVAL_5, 104);
        assert_eq!(AQUARION_REWORK, 105);
        assert_eq!(SUMMER24_BUG_CLEANUP, 106);
        assert_eq!(AQUARION_REWORK_BALANCE, 107);
        assert_eq!(AQUARION_REWORK_BALANCE_2, 108);
        assert_eq!(SCORE_FIXES, 109);
        assert_eq!(SCORE_FIXES_AND_PIRATE, 110);
        assert_eq!(BUGFIXES_Q1_2025, 111);
        assert_eq!(POLARIS_SKIN, 112);
        assert_eq!(REMOVE_AGGRESSION, 113);
        assert_eq!(BALANCE_2025, 114);
        assert_eq!(CYMANTI_REWORK, 115);
        assert_eq!(CYMANTI_REWORK_PATCH, 116);
        assert_eq!(CYMANTI_REWORK_PATCH2, 117);
        assert_eq!(CYMANTI_REWORK_PATCH3, 118);
        assert_eq!(CYMANTI_SKIN, 119);
        assert_eq!(BUG_PATCH_Q1_26, 120);
        assert_eq!(PLAYER_CLIMATE, 121);
        assert_eq!(CHAMPIONSHIP_FIXES, 122);
        assert_eq!(AWAY_STARS_CHAMPIONSHIP_FIXES, 123);
        assert_eq!(ACHIEVEMENTS, 124);
        assert_eq!(ACHIEVEMENTS, GAME_VERSION);
    }

    #[test]
    fn logic_data_version_for_every_supported_game_version() {
        let mut covered = [false; 125];
        for &(start, end, data) in SUPPORTED_LADDER {
            for version in start..=end {
                assert!(!covered[version as usize], "game version {version} listed twice");
                covered[version as usize] = true;
                assert_eq!(logic_data_version(version), data, "game version {version}");
            }
        }
        for version in MIN_SUPPORTED_GAME_VERSION..=GAME_VERSION {
            assert!(covered[version as usize], "game version {version} missing");
        }
    }

    #[test]
    fn logic_data_version_below_support_and_above_current() {
        let spots = [
            (i32::MIN, 1),
            (0, 1),
            (8, 1),
            (9, 2),
            (10, 3),
            (11, 4),
            (13, 4),
            (14, 5),
            (39, 5),
            (40, 6),
            (42, 6),
            (43, 7),
            (44, 7),
            (45, 8),
            (49, 8),
            (50, 9),
            (123, 28),
            (124, 28),
            (125, 28),
            (i32::MAX, 28),
        ];
        for (version, data) in spots {
            assert_eq!(logic_data_version(version), data, "game version {version}");
        }
    }

    #[test]
    fn save_and_new_match_windows() {
        assert!(GameVersion::for_save(50).is_err());
        assert!(GameVersion::for_save(125).is_err());
        assert_eq!(GameVersion::for_save(51).unwrap().as_i32(), 51);
        assert_eq!(GameVersion::for_save(123).unwrap().as_i32(), 123);
        assert_eq!(GameVersion::for_save(124).unwrap().as_i32(), 124);

        assert_eq!(GameVersion::for_new_match(107), Err(VersionError::BeforeNewMatch(107)));
        assert_eq!(GameVersion::for_new_match(108).unwrap().as_i32(), 108);
        assert_eq!(GameVersion::for_new_match(123).unwrap().as_i32(), 123);
        assert_eq!(GameVersion::for_new_match(124).unwrap().as_i32(), 124);
        assert_eq!(GameVersion::for_new_match(125), Err(VersionError::Unsupported(125)));
        assert_eq!(GameVersion::for_new_match(50), Err(VersionError::Unsupported(50)));

        assert_eq!(GameVersion::current().as_i32(), GAME_VERSION);
        assert_eq!(GameVersion::default().as_i32(), GAME_VERSION);
        assert_eq!(GameVersion::current().logic_data_version(), GAME_LOGIC_DATA_VERSION);
        assert_eq!(i32::from(GameVersion::current()), GAME_VERSION);
    }

    #[test]
    fn support_predicates_match_the_tiny_functions() {
        assert!(is_too_old(50));
        assert!(is_too_old(i32::MIN));
        assert!(!is_too_old(51));
        assert!(!is_too_old(123));
        assert!(!is_too_old(124));
        assert!(!is_too_old(125));
        assert!(!is_too_old(i32::MAX));

        assert!(!is_supported(50));
        assert!(!is_supported(i32::MIN));
        assert!(is_supported(51));
        assert!(is_supported(123));
        assert!(is_supported(124));
        assert!(!is_supported(125));
        assert!(!is_supported(i32::MAX));

        for version in MIN_SUPPORTED_GAME_VERSION..=GAME_VERSION {
            assert!(is_supported(version), "{version}");
            assert!(!is_too_old(version), "{version}");
            assert!(GameVersion::for_save(version).is_ok(), "{version}");
        }
    }

    #[test]
    fn milestones_are_in_release_order() {
        assert_eq!(MILESTONES.first().map(|m| (m.name, m.version)), Some(("CYMANTI_VERSION", 40)));
        assert_eq!(MILESTONES.last().map(|m| (m.name, m.version)), Some(("ACHIEVEMENTS", GAME_VERSION)));
        assert_eq!(MILESTONES.len(), 48);
        let second_last = MILESTONES[MILESTONES.len() - 2];
        assert_eq!((second_last.name, second_last.version), ("AWAY_STARS_CHAMPIONSHIP_FIXES", 123));
        for pair in MILESTONES.windows(2) {
            assert!(
                pair[0].version < pair[1].version,
                "{} ({}) must come before {} ({})",
                pair[0].name,
                pair[0].version,
                pair[1].name,
                pair[1].version
            );
        }
        let find = |name: &str| MILESTONES.iter().find(|m| m.name == name).map(|m| m.version);
        assert_eq!(find("NAVAL_1"), Some(NAVAL_1));
        assert_eq!(find("CYMANTI_REWORK"), Some(CYMANTI_REWORK));
        assert_eq!(find("LOBBY_VERSION_5"), Some(94));
        assert_eq!(find("NO_SUCH_MILESTONE"), None);
    }

    #[test]
    fn window_constants_sit_on_milestones_or_inside_the_window() {
        let is_milestone = |v: i32| MILESTONES.iter().any(|m| m.version == v);
        for v in [MIN_NEW_GAME_VERSION, WEEKLY_CHALLENGE_GAME_VERSION, MIN_TOURNAMENT_GAME_VERSION, GAME_VERSION] {
            assert!(is_milestone(v), "{v} is not a named release");
        }
        assert_eq!(SUPPORTED_SPAN, 74);
    }

    #[test]
    fn rules_ladder_is_sorted_and_steps_one_rules_version_at_a_time() {
        for (i, step) in RULES_LADDER.iter().enumerate() {
            assert_eq!(step.rules_version, i as i32 + 1, "step {i}");
        }
        for pair in RULES_LADDER.windows(2) {
            assert!(pair[0].first_game_version < pair[1].first_game_version, "{pair:?}");
        }
        assert_eq!(RULES_LADDER.last().unwrap().rules_version, GAME_LOGIC_DATA_VERSION);
    }

    #[test]
    fn rules_ladder_boundaries() {
        for pair in RULES_LADDER.windows(2) {
            let first = pair[1].first_game_version;
            assert_eq!(logic_data_version(first), pair[1].rules_version, "game version {first}");
            assert_eq!(logic_data_version(first - 1), pair[0].rules_version, "game version {}", first - 1);
        }
    }

    #[test]
    fn every_supported_game_version_has_loadable_rules() {
        let mut used = std::collections::BTreeSet::new();
        for version in GameVersion::supported() {
            let rules = version.logic_data_version();
            assert!(
                crate::data::rules::EMBEDDED_FILES.iter().any(|(v, _)| *v == rules),
                "game version {version}: no rules file v{rules}"
            );
            let data = version.load_rules().unwrap_or_else(|err| panic!("game version {version}: {err}"));
            assert!(!data.units.is_empty(), "game version {version}");
            used.insert(rules);
        }
        // Every rules file is reachable from a supported game version.
        let files: std::collections::BTreeSet<i32> =
            crate::data::rules::EMBEDDED_FILES.iter().map(|(v, _)| *v).collect();
        assert_eq!(used, files);
    }

    #[test]
    fn supported_lists_the_whole_window() {
        let all: Vec<i32> = GameVersion::supported().map(i32::from).collect();
        assert_eq!(all.len(), 74);
        assert_eq!(all.first(), Some(&MIN_SUPPORTED_GAME_VERSION));
        assert_eq!(all.last(), Some(&GAME_VERSION));
        assert_eq!(GameVersion::supported().next_back(), Some(GameVersion::current()));
    }

    #[test]
    fn milestone_queries() {
        let v = GameVersion::for_save(NAVAL_1).unwrap();
        assert!(v.at_least(NAVAL_1));
        assert!(!v.at_least(NAVAL_2));
        assert!(v.is_before(NAVAL_2));
        assert!(!v.is_before(NAVAL_1));
        assert_eq!(v.latest_milestone().map(|m| m.name), Some("NAVAL_1"));

        let between = GameVersion::for_save(88).unwrap();
        assert_eq!(between.latest_milestone().map(|m| m.name), Some("TRIBE_SKINS_VERSION_2"));
        assert_eq!(GameVersion::current().latest_milestone().map(|m| m.version), Some(GAME_VERSION));
        for version in GameVersion::supported() {
            assert!(version.latest_milestone().is_some(), "{version}");
        }
    }

    #[test]
    fn support_level_boundaries() {
        assert_eq!(support_level(i32::MIN), Support::Unsupported);
        assert_eq!(support_level(50), Support::Unsupported);
        assert_eq!(support_level(51), Support::Supported);
        assert_eq!(support_level(120), Support::Supported);
        assert_eq!(support_level(121), Support::Tested);
        assert_eq!(support_level(123), Support::Tested);
        // 124 loads and runs but no retail game of that version has been replayed yet.
        assert_eq!(support_level(124), Support::Supported);
        assert_eq!(support_level(125), Support::Unsupported);
        assert_eq!(support_level(i32::MAX), Support::Unsupported);
        assert_eq!(GameVersion::current().support(), Support::Supported);
        assert_eq!(GameVersion::for_save(MAX_TESTED_GAME_VERSION).unwrap().support(), Support::Tested);
        assert_eq!(GameVersion::for_save(51).unwrap().support(), Support::Supported);
        for version in -300..400 {
            assert_eq!(support_level(version) != Support::Unsupported, is_supported(version), "{version}");
        }
    }

    #[test]
    fn rejected_versions_say_why() {
        let old = GameVersion::try_from(50).unwrap_err();
        assert_eq!(old, VersionError::Unsupported(50));
        assert!(old.is_too_old() && !old.is_too_new());
        assert_eq!(old.version(), 50);
        assert!(old.to_string().contains("51..=124"));

        let new = GameVersion::try_from(125).unwrap_err();
        assert!(new.is_too_new() && !new.is_too_old());

        let early = GameVersion::for_new_match(100).unwrap_err();
        assert_eq!(early, VersionError::BeforeNewMatch(100));
        assert!(!early.is_too_old() && !early.is_too_new());

        assert_eq!(GameVersion::try_from(124), Ok(GameVersion::current()));
        assert_eq!(GameVersion::current().to_string(), "124");
        for n in [i32::MIN, -1, 0, 8, 50, 125, 1000, i32::MAX] {
            assert!(GameVersion::try_from(n).is_err(), "{n}");
        }
    }
}
