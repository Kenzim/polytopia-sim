//! Plain data: the game state, map, tiles, units, players and settings.

pub const MINIMUM_DOMAIN_SIZE: i32 = 3;
pub const DESIRED_DOMAIN_SIZE: i32 = 5;
pub const DEFAULT_CLIMATE: i32 = 7;

/// Map size; the width and height in tiles are given by [`MapSize::width`].
#[repr(i32)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MapSize {
    None = 0,
    Tiny = 1,
    Small = 2,
    Normal = 3,
    Large = 4,
    Huge = 5,
    Massive = 6,
}

impl MapSize {
    /// Side length in tiles (maps are square).
    pub fn width(self) -> u16 {
        match self {
            MapSize::None => 0,
            MapSize::Tiny => 11,
            MapSize::Small => 14,
            MapSize::Normal => 16,
            MapSize::Large => 18,
            MapSize::Huge => 20,
            MapSize::Massive => 30,
        }
    }

    /// The most players (Nature not counted) the map generator can place on this size for every seed: Tiny 9,
    /// Small 16, Normal 16, Large 25, Huge 36, Massive 81. Capitals go one per cell of a `ceil(sqrt(players))` square
    /// grid; with more players some cells are too narrow to hold a capital and generation fails on the seeds that pick
    /// them (see [`crate::map::shape::capital_grid_fits`]).
    pub fn max_players(self) -> usize {
        max_players_on_side(self.width())
    }
}

/// [`MapSize::max_players`] for a map of `side` tiles a side.
pub fn max_players_on_side(side: u16) -> usize {
    let width = i32::from(side);
    let mut cells = 0usize;
    while crate::map::shape::capital_grid_fits(width, ((cells + 1) * (cells + 1)) as i32) {
        cells += 1;
    }
    cells * cells
}

/// The map shape the generator aims for.
#[repr(i32)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MapPreset {
    None = 0,
    Dryland = 1,
    Lakes = 2,
    Continents = 3,
    Archipelago = 4,
    WaterWorld = 5,
    Pangea = 6,
}

/// How strong the built-in AI plays.
#[repr(i32)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum BotDifficulty {
    Easy = 0,
    Normal = 1,
    Hard = 2,
    Crazy = 3,
    Frozen = 4,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MapGeneratorSettings {
    pub wetness: f32,
    pub richness: f32,
    pub smooth_iterations: i32,
    pub pre_terrain_city_density: f32,
    pub post_terrain_city_density: f32,
    pub min_suburb_count: i32,
    pub max_suburb_count: i32,
    pub surrounding_space_value: f32,
    pub shallow_percent_of_water: f32,
    pub equality_iterations: i32,
    pub equality_limit: f32,
    pub map_type: MapPreset,
}

impl Default for MapGeneratorSettings {
    fn default() -> Self {
        Self {
            wetness: 0.45,
            richness: 1.0,
            smooth_iterations: 2,
            pre_terrain_city_density: 0.3,
            post_terrain_city_density: 1.0,
            min_suburb_count: 1,
            max_suburb_count: 2,
            surrounding_space_value: 0.5,
            shallow_percent_of_water: 0.0,
            equality_iterations: 0,
            equality_limit: 0.0,
            map_type: MapPreset::None,
        }
    }
}

/// A tile position on the map: `x` across, `y` down, counted from 0.
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq, Hash)]
pub struct WorldCoordinates {
    pub x: i32,
    pub y: i32,
}

/// One map tile: terrain, resource, improvement, owner and the flags the rules read.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Tile {
    pub index: i32,
    pub coordinates: WorldCoordinates,
    pub terrain: i32,
    pub climate: i32,
    pub resource: i32,
    pub improvement: i32,
    pub owner: u8,
    pub is_capital: bool,
    /// The player this tile was placed as capital for, kept through captures
    /// (`is_capital` is `capital_of != 0`).
    pub capital_of: u8,
    pub is_city: bool,
    pub has_ruin: bool,
    /// Altitude. Clearing the map writes -1; land that generation
    /// has claimed gets 1. Not derivable from `terrain`: a water-scan city is Field with -1.
    pub altitude: i32,
    /// The city whose area holds this tile, or (-1,-1).
    pub ruling_city: WorldCoordinates,
    /// Whether the tile has a road.
    pub has_road: bool,
    /// Whether the tile is on a route.
    pub has_route: bool,
    /// Tile effects as a bit set over the tile effect type (bit = value).
    pub effects: u16,
    /// Skin type. Freezing a tile repaints it, and the freezable check compares it with the player's skin.
    pub skin: i32,
}

impl Default for Tile {
    fn default() -> Self {
        Self {
            index: 0,
            coordinates: WorldCoordinates { x: 0, y: 0 },
            terrain: 0,
            climate: 0,
            resource: 0,
            improvement: 0,
            owner: 0,
            is_capital: false,
            capital_of: 0,
            is_city: false,
            has_ruin: false,
            altitude: -1,
            ruling_city: WorldCoordinates { x: -1, y: -1 },
            has_road: false,
            has_route: false,
            effects: 0,
            skin: 0,
        }
    }
}

/// The board: `width * height` tiles in row order (`index = y * width + x`).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Map {
    pub width: u16,
    pub height: u16,
    pub tiles: Vec<Tile>,
}

impl Map {
    /// An empty map of default tiles.
    pub fn new(width: u16, height: u16) -> Self {
        let mut tiles = Vec::with_capacity(usize::from(width) * usize::from(height));
        for y in 0..height {
            for x in 0..width {
                let x = i32::from(x);
                let y = i32::from(y);
                tiles.push(Tile {
                    index: y * i32::from(width) + x,
                    coordinates: WorldCoordinates { x, y },
                    ..Tile::default()
                });
            }
        }
        Self { width, height, tiles }
    }
}

/// The handicap the game gives every human player. It is added to the income of the player's original capital,
/// so a new human capital makes 2 stars a turn. Every human seat in the retail saves the engine is tested on has it.
pub const HUMAN_HANDICAP: i32 = 1;

/// A player (or Nature, id 255) as the game state stores it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct PlayerState {
    /// Player id, counted from 1 in seat order.
    pub id: u8,
    /// Handicap: [`HUMAN_HANDICAP`] for a human, set from the difficulty for an AI. It is added to the income of
    /// the player's original capital and makes a bot play better.
    pub handicap: i32,
    /// Stars in hand.
    pub currency: i32,
    pub score: u32,
    /// Tribe number.
    pub tribe: i32,
    /// Whether the built-in AI plays this seat.
    pub auto_play: bool,
    /// The starting tile, or (-1, -1) before the map is placed.
    pub start_tile: WorldCoordinates,
}

#[repr(i32)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum DiplomacyRelationState {
    #[default]
    Neutral = 0,
    Peace = 1,
    War = 2,
    CoolDown = 3,
}

pub const DIPLOMACY_DEFAULT_TURN: i32 = -100;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct DiplomacyRelation {
    pub state: DiplomacyRelationState,
    pub last_attack_turn: i32,
    pub embassy_level: i32,
    pub last_peace_broken_turn: i32,
    pub first_meet: i32,
    pub embassy_build_turn: i32,
    pub previous_attack_turn: i32,
}

impl Default for DiplomacyRelation {
    fn default() -> Self {
        Self {
            state: DiplomacyRelationState::Neutral,
            last_attack_turn: DIPLOMACY_DEFAULT_TURN,
            embassy_level: 0,
            last_peace_broken_turn: DIPLOMACY_DEFAULT_TURN,
            first_meet: DIPLOMACY_DEFAULT_TURN,
            embassy_build_turn: DIPLOMACY_DEFAULT_TURN,
            previous_attack_turn: DIPLOMACY_DEFAULT_TURN,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct DiplomacyLink {
    pub other: u8,
    pub relation: DiplomacyRelation,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Default)]
pub struct PlayerDiplomacy {
    pub player: u8,
    pub relations: Vec<DiplomacyLink>,
}

#[repr(i32)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum TaskKind {
    #[default]
    None = 0,
    Pacifist = 1,
    Genius = 2,
    Network = 3,
    Wealth = 4,
    Killer = 5,
    Metropolis = 6,
    Explorer = 7,
    ExploreLighthouses = 8,
    Convert = 9,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct PlayerTask {
    pub kind: TaskKind,
    pub is_started: bool,
    pub is_completed: bool,
    pub converts: i32,
    pub explored_tiles: i32,
    pub kills: i32,
    pub turns: i32,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Default)]
pub struct PlayerTasks {
    pub player: u8,
    pub tasks: Vec<PlayerTask>,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Default)]
pub struct OwnedTech {
    pub player: u8,
    pub tech: Vec<i32>,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Aggression {
    pub other: u8,
    pub count: i32,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct PlayerRuntime {
    pub player: u8,
    pub tribe_mix: i32,
    pub climate: i32,
    pub has_chosen_tribe: bool,
    pub skin: i32,
    pub resigned_turn: i32,
    pub resigned_at_command: i32,
    pub wiped_at_command: i32,
    pub aggressions: Vec<Aggression>,
    pub known_players: Vec<u8>,
    pub built_unique_improvements: Vec<i32>,
    pub end_score: u32,
    pub cities: i32,
    pub kills: u32,
    pub casualties: u32,
    pub wipe_outs: u32,
    pub killer_id: u8,
    pub killed_turn: u32,
    pub block_train_units: bool,
    /// `PlayerState.messages`: (message type, sender).
    pub messages: Vec<(u8, u8)>,
}

impl Default for PlayerRuntime {
    fn default() -> Self {
        Self {
            player: 0,
            tribe_mix: 0,
            climate: 0,
            has_chosen_tribe: false,
            skin: 0,
            resigned_turn: -1,
            resigned_at_command: -1,
            wiped_at_command: -1,
            aggressions: Vec::new(),
            known_players: Vec::new(),
            built_unique_improvements: Vec::new(),
            end_score: 0,
            cities: 0,
            kills: 0,
            casualties: 0,
            wipe_outs: 0,
            killer_id: 0,
            killed_turn: 0,
            block_train_units: false,
            messages: Vec::new(),
        }
    }
}

#[repr(i32)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum CityReward {
    #[default]
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

#[derive(Debug, Clone, PartialEq, Eq, Hash, Default)]
pub struct ImprovementRuntime {
    pub tile: i32,
    pub owner: u8,
    pub founder: u8,
    pub level: u16,
    pub founded: u16,
    pub xp: i16,
    pub population: i16,
    pub production: u16,
    pub base_score: u16,
    pub border_size: u16,
    pub upgrade: u16,
    pub connected_to_capital_of_player: u8,
    pub rewards: Vec<CityReward>,
    pub decomposing: bool,
    pub robbed: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Default)]
pub struct TileExplorers {
    pub tile: i32,
    pub explorers: Explorers,
}

/// The players who have explored one tile, in the order they explored it. Behaves like a `Vec<u8>` (it derefs to
/// `[u8]` and has `push` and `clear`), but keeps up to [`Explorers::INLINE`] ids in place, so cloning a game state
/// allocates nothing per tile; a longer list moves to the heap.
#[derive(Clone)]
pub struct Explorers(ExplorersRepr);

#[derive(Clone)]
enum ExplorersRepr {
    Inline { len: u8, ids: [u8; Explorers::INLINE] },
    Heap(Box<[u8]>),
}

impl Explorers {
    /// The most ids kept without a heap allocation.
    pub const INLINE: usize = 14;

    pub const fn new() -> Self {
        Self(ExplorersRepr::Inline { len: 0, ids: [0; Self::INLINE] })
    }

    pub fn push(&mut self, id: u8) {
        match &mut self.0 {
            ExplorersRepr::Inline { len, ids } if usize::from(*len) < Self::INLINE => {
                ids[usize::from(*len)] = id;
                *len += 1;
            }
            ExplorersRepr::Inline { len, ids } => {
                let mut v = ids[..usize::from(*len)].to_vec();
                v.push(id);
                self.0 = ExplorersRepr::Heap(v.into_boxed_slice());
            }
            ExplorersRepr::Heap(b) => {
                let mut v = std::mem::take(b).into_vec();
                v.push(id);
                *b = v.into_boxed_slice();
            }
        }
    }

    pub fn clear(&mut self) {
        *self = Self::new();
    }

    pub fn as_slice(&self) -> &[u8] {
        match &self.0 {
            ExplorersRepr::Inline { len, ids } => &ids[..usize::from(*len)],
            ExplorersRepr::Heap(v) => v,
        }
    }

    pub fn as_mut_slice(&mut self) -> &mut [u8] {
        match &mut self.0 {
            ExplorersRepr::Inline { len, ids } => &mut ids[..usize::from(*len)],
            ExplorersRepr::Heap(v) => v,
        }
    }
}

impl Default for Explorers {
    fn default() -> Self {
        Self::new()
    }
}

impl std::ops::Deref for Explorers {
    type Target = [u8];

    fn deref(&self) -> &[u8] {
        self.as_slice()
    }
}

impl std::ops::DerefMut for Explorers {
    fn deref_mut(&mut self) -> &mut [u8] {
        self.as_mut_slice()
    }
}

impl std::fmt::Debug for Explorers {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.as_slice().fmt(f)
    }
}

impl PartialEq for Explorers {
    fn eq(&self, other: &Self) -> bool {
        self.as_slice() == other.as_slice()
    }
}

impl Eq for Explorers {}

impl PartialEq<Vec<u8>> for Explorers {
    fn eq(&self, other: &Vec<u8>) -> bool {
        self.as_slice() == other.as_slice()
    }
}

impl PartialEq<[u8]> for Explorers {
    fn eq(&self, other: &[u8]) -> bool {
        self.as_slice() == other
    }
}

impl std::hash::Hash for Explorers {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.as_slice().hash(state);
    }
}

impl FromIterator<u8> for Explorers {
    fn from_iter<I: IntoIterator<Item = u8>>(iter: I) -> Self {
        let mut out = Self::new();
        out.extend(iter);
        out
    }
}

impl Extend<u8> for Explorers {
    fn extend<I: IntoIterator<Item = u8>>(&mut self, iter: I) {
        for id in iter {
            self.push(id);
        }
    }
}

impl From<&[u8]> for Explorers {
    fn from(ids: &[u8]) -> Self {
        if ids.len() <= Self::INLINE {
            let mut inline = [0; Self::INLINE];
            inline[..ids.len()].copy_from_slice(ids);
            Self(ExplorersRepr::Inline { len: ids.len() as u8, ids: inline })
        } else {
            Self(ExplorersRepr::Heap(ids.into()))
        }
    }
}

impl From<Vec<u8>> for Explorers {
    fn from(ids: Vec<u8>) -> Self {
        if ids.len() <= Self::INLINE {
            ids.as_slice().into()
        } else {
            Self(ExplorersRepr::Heap(ids.into_boxed_slice()))
        }
    }
}

impl From<Explorers> for Vec<u8> {
    fn from(ids: Explorers) -> Self {
        match ids.0 {
            ExplorersRepr::Inline { len, ids } => ids[..usize::from(len)].to_vec(),
            ExplorersRepr::Heap(v) => v.into_vec(),
        }
    }
}

impl<'a> IntoIterator for &'a Explorers {
    type Item = &'a u8;
    type IntoIter = std::slice::Iter<'a, u8>;

    fn into_iter(self) -> Self::IntoIter {
        self.as_slice().iter()
    }
}

pub const UNIT_CTOR_HEALTH: u16 = 10;

/// One unit on the board.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct UnitState {
    pub id: u32,
    pub leader: u32,
    pub follower: u32,
    pub owner: u8,
    pub birth_climate: i32,
    pub type_idx: i32,
    pub previous_turn_end: WorldCoordinates,
    pub coordinates: WorldCoordinates,
    pub home: WorldCoordinates,
    pub passenger: Option<Box<UnitState>>,
    pub health: u16,
    pub promotion: u16,
    pub xp: u16,
    pub moved: bool,
    pub attacked: bool,
    pub created_turn: u16,
    pub frozen: bool,
    pub poisoned: bool,
    pub boosted: bool,
    pub invisible: bool,
    pub bubble: bool,
    pub petrified: bool,
    pub swift: bool,
    pub double_ready: bool,
    pub charmed: bool,
    pub temporary_explorer: bool,
    /// `UnitState.direction` (SW 0, W 1, NW 2, N 3, NE 4, E 5, SE 6, S 7).
    pub direction: u8,
}

impl Default for UnitState {
    fn default() -> Self {
        Self {
            id: 0,
            leader: 0,
            follower: 0,
            owner: 0,
            birth_climate: 0,
            type_idx: 0,
            previous_turn_end: WorldCoordinates { x: 0, y: 0 },
            coordinates: WorldCoordinates { x: 0, y: 0 },
            home: WorldCoordinates { x: 0, y: 0 },
            passenger: None,
            health: UNIT_CTOR_HEALTH,
            promotion: 0,
            xp: 0,
            moved: false,
            attacked: false,
            created_turn: 0,
            frozen: false,
            poisoned: false,
            boosted: false,
            invisible: false,
            bubble: false,
            petrified: false,
            swift: false,
            double_ready: false,
            charmed: false,
            temporary_explorer: false,
            direction: 7,
        }
    }
}

/// Where the game is in its life: [`GameStatus::Ended`] once a winner is decided.
#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum GameStatus {
    #[default]
    Unknown = 0,
    Lobby = 1,
    Started = 2,
    FinalTurn = 3,
    Ended = 4,
}

/// Game modes (`GameSettings::base_game_mode` and `rules_game_mode`).
pub const GAME_MODE_PERFECTION: i32 = 1;
pub const GAME_MODE_DOMINATION: i32 = 2;
pub const GAME_MODE_GLORY: i32 = 3;
pub const GAME_MODE_MIGHT: i32 = 4;
pub const GAME_MODE_CUSTOM: i32 = 5;
/// Player death condition: a player is dead without cities (0) or without cities and units (1).
pub const DEATH_CONDITION_CITIES: i32 = 0;
pub const DEATH_CONDITION_UNITS: i32 = 1;

/// The parts of the game settings and game rules the rules read.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct GameSettings {
    /// Base game mode.
    pub base_game_mode: i32,
    /// Game mode of the rules.
    pub rules_game_mode: i32,
    /// Turn limit.
    pub turn_limit: i32,
    /// Score limit.
    pub score_limit: i32,
    /// Win by holding every capital.
    pub win_by_capital: bool,
    /// Win by being the last player left.
    pub win_by_extermination: bool,
    /// Whether special tribes are allowed.
    pub allow_special_tribes: bool,
    /// Whether tech sharing is allowed.
    pub allow_tech_sharing: bool,
    /// Player death condition (`DEATH_CONDITION_*`).
    pub player_death_condition: i32,
    /// Map preset.
    pub map_preset: i32,
    /// Bot difficulty.
    pub difficulty: i32,
    /// Number of opponents.
    pub opponent_count: i32,
    /// Game type.
    pub game_type: i32,
}

impl GameSettings {
    /// Select a game mode: sets both the base mode and the rules mode.
    ///
    /// Might also loads its rule preset: the game ends once one player holds every capital (`win_by_capital`), or
    /// when only one player is left (`win_by_extermination`), and the turn and score limits are cleared. Set a
    /// limit afterwards if one is wanted; while `turn_limit` is above 0 the capital rule is off (a turn-limited game
    /// ends at the limit, as in retail). Any other mode only clears `win_by_capital` and leaves the limits and the
    /// other flags alone, so the settings of an existing mode come out exactly as when the two mode fields were
    /// written directly.
    pub fn set_game_mode(&mut self, mode: i32) {
        self.base_game_mode = mode;
        self.rules_game_mode = mode;
        self.win_by_capital = mode == GAME_MODE_MIGHT;
        if mode == GAME_MODE_MIGHT {
            self.win_by_extermination = true;
            self.turn_limit = 0;
            self.score_limit = 0;
        }
    }
}

impl Default for GameSettings {
    fn default() -> Self {
        Self {
            base_game_mode: GAME_MODE_DOMINATION,
            rules_game_mode: GAME_MODE_DOMINATION,
            turn_limit: 0,
            score_limit: 0,
            win_by_capital: false,
            win_by_extermination: false,
            allow_special_tribes: true,
            allow_tech_sharing: false,
            player_death_condition: DEATH_CONDITION_CITIES,
            map_preset: 0,
            difficulty: 1,
            opponent_count: 0,
            game_type: 0,
        }
    }
}

/// The whole state of a game: map, players, units, research, diplomacy, fog and settings.
/// [`Engine`](crate::Engine) wraps it with the rule tables and the turn flow.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct GameState {
    /// The game version the rules follow.
    pub version: i32,
    /// The seed the map was generated from.
    pub seed: i32,
    pub map: Map,
    /// Every player in seat order, with Nature last.
    pub players: Vec<PlayerState>,
    pub player_runtime: Vec<PlayerRuntime>,
    pub units: Vec<UnitState>,
    pub improvements: Vec<ImprovementRuntime>,
    pub tech: Vec<OwnedTech>,
    pub diplomacy: Vec<PlayerDiplomacy>,
    pub fog: Vec<TileExplorers>,
    pub tasks: Vec<PlayerTasks>,
    pub current_player_index: u8,
    /// The turn number, counted from 0.
    pub current_turn: u32,
    pub current_unit_id: u32,
    pub current_state: GameStatus,
    pub current_command: u16,
    pub settings: GameSettings,
    /// City names by tile index; the rules never read them.
    pub city_names: std::collections::BTreeMap<i32, String>,
    /// Lookup cache for [`GameState::unit_index_at`]; not part of the state.
    pub unit_grid: UnitGrid,
}

/// A tile -> first-unit index over `GameState.units`, filled by [`GameState::unit_grid_begin`] for a stretch in
/// which `units` does not change (legal command generation, an AI move) and dropped by
/// [`GameState::unit_grid_end`]. Outside such a stretch lookups scan `units`. It is not part of the state: it compares
/// equal, hashes to nothing and is not cloned.
#[derive(Default)]
pub struct UnitGrid {
    active: std::cell::Cell<bool>,
    /// Per tile, the first unit's index or [`GRID_NONE`]; sized by the first fill, after which a map of another size
    /// leaves the grid off.
    first: std::cell::OnceCell<Box<[std::cell::Cell<u16>]>>,
}

const GRID_NONE: u16 = u16::MAX;

impl Clone for UnitGrid {
    fn clone(&self) -> Self {
        Self::default()
    }
}

impl PartialEq for UnitGrid {
    fn eq(&self, _: &Self) -> bool {
        true
    }
}

impl Eq for UnitGrid {}

impl std::hash::Hash for UnitGrid {
    fn hash<H: std::hash::Hasher>(&self, _: &mut H) {}
}

impl std::fmt::Debug for UnitGrid {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("UnitGrid")
    }
}

impl Default for GameState {
    fn default() -> Self {
        Self {
            version: 0,
            seed: 0,
            map: Map { width: 0, height: 0, tiles: Vec::new() },
            players: Vec::new(),
            player_runtime: Vec::new(),
            units: Vec::new(),
            improvements: Vec::new(),
            tech: Vec::new(),
            diplomacy: Vec::new(),
            fog: Vec::new(),
            tasks: Vec::new(),
            current_player_index: 0,
            current_turn: 0,
            current_unit_id: 0,
            current_state: GameStatus::Unknown,
            current_command: 0,
            settings: GameSettings::default(),
            city_names: Default::default(),
            unit_grid: UnitGrid::default(),
        }
    }
}

impl GameState {
    pub fn new(version: i32, seed: i32) -> Self {
        Self { version, seed, ..Self::default() }
    }

    pub fn current_player_id(&self) -> Option<u8> {
        self.players.get(usize::from(self.current_player_index)).map(|player| player.id)
    }

    fn grid_index(&self, c: WorldCoordinates) -> Option<usize> {
        let (w, h) = (i32::from(self.map.width), i32::from(self.map.height));
        (c.x >= 0 && c.y >= 0 && c.x < w && c.y < h).then(|| (c.y * w + c.x) as usize)
    }

    /// The index of the first unit in `units` standing on `c`.
    pub fn unit_index_at(&self, c: WorldCoordinates) -> Option<usize> {
        if self.unit_grid.active.get() {
            if let (Some(k), Some(first)) = (self.grid_index(c), self.unit_grid.first.get()) {
                let i = first[k].get();
                let found = (i != GRID_NONE).then_some(usize::from(i));
                #[cfg(feature = "grid_check")]
                assert_eq!(found, self.units.iter().position(|u| u.coordinates == c), "stale unit grid at {c:?}");
                return found;
            }
        }
        self.units.iter().position(|u| u.coordinates == c)
    }

    /// The first unit in `units` standing on `c`.
    pub fn unit_at(&self, c: WorldCoordinates) -> Option<&UnitState> {
        self.unit_index_at(c).map(|i| &self.units[i])
    }

    /// Fill the unit grid; `units` must not change until [`GameState::unit_grid_end`]. Returns false (and does
    /// nothing) when a grid is already active, so nested stretches leave the end to the outermost one.
    pub fn unit_grid_begin(&self) -> bool {
        if self.unit_grid.active.get() || self.units.len() >= usize::from(GRID_NONE) {
            return false;
        }
        let len = usize::from(self.map.width) * usize::from(self.map.height);
        let first = self.unit_grid.first.get_or_init(|| (0..len).map(|_| std::cell::Cell::new(GRID_NONE)).collect());
        if first.len() != len {
            return false;
        }
        for cell in first.iter() {
            cell.set(GRID_NONE);
        }
        for (i, u) in self.units.iter().enumerate() {
            if let Some(k) = self.grid_index(u.coordinates) {
                if first[k].get() == GRID_NONE {
                    first[k].set(i as u16);
                }
            }
        }
        self.unit_grid.active.set(true);
        true
    }

    pub fn unit_grid_end(&self) {
        self.unit_grid.active.set(false);
    }

    /// Panic unless the active grid still matches `units` on every tile.
    #[cfg(feature = "grid_check")]
    pub fn unit_grid_check(&self) {
        for y in 0..i32::from(self.map.height) {
            for x in 0..i32::from(self.map.width) {
                let _ = self.unit_index_at(WorldCoordinates { x, y });
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn explorers_behave_like_a_vec_inline_and_on_the_heap() {
        let mut e = Explorers::new();
        let mut v: Vec<u8> = Vec::new();
        for id in 0..40u8 {
            e.push(id * 3);
            v.push(id * 3);
            assert_eq!(e, v);
            assert_eq!(e.clone(), v);
            assert_eq!(Explorers::from(v.clone()), e);
            assert_eq!(Vec::<u8>::from(e.clone()), v);
            assert_eq!(e.iter().copied().collect::<Explorers>(), e);
        }
        e.clear();
        assert!(e.is_empty());
        e.push(7);
        assert_eq!(e, vec![7]);
        assert!(std::mem::size_of::<Explorers>() <= 24);
    }

    #[test]
    fn stores_version_on_empty_match() {
        let newest = GameState::new(123, 0);
        let oldest = GameState::new(51, 0);
        assert_eq!(newest.version, 123);
        assert_eq!(oldest.version, 51);
        assert!(newest.map.tiles.is_empty());
        assert!(oldest.map.tiles.is_empty());
        assert!(newest.units.is_empty());
        assert!(oldest.units.is_empty());
        assert_eq!(newest.map.width, 0);
        assert_eq!(oldest.map.height, 0);
    }
}
