//! Rules database for logic-data versions 9 through 28.
//!
//! The rules are the engine's own files in `data/rules/` (see [`rules`]), compiled
//! in. `load_embedded_game_logic` is version 28. `load_game_logic_data` selects
//! 9..=28. `load_game_logic` maps a game version onto that data. The same loader
//! reads a directory of such files at run time (`load_game_logic_data_from_dir`).
//! `load_game_logic_json` reads one document in the game's own JSON layout.
//! [`convert`] holds the conversion between the two.
//! Units a version does not list are absent.

pub mod convert;
pub mod rules;

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::sync::OnceLock;

/// The newest rules version, which is also the built-in default.
pub const GAME_LOGIC_DATA_VERSION: i32 = crate::version::GAME_LOGIC_DATA_VERSION;

/// Failure while reading rules JSON. Carries no replacement stats.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DataError {
    pub message: String,
}

impl DataError {
    fn msg(message: impl Into<String>) -> Self {
        Self { message: message.into() }
    }
}

impl fmt::Display for DataError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for DataError {}

/// Parsed rules. Map keys are the JSON object names (`warrior`, `imperius`).
#[derive(Debug, Clone, PartialEq)]
pub struct GameLogicData {
    pub units: BTreeMap<String, UnitData>,
    pub tribes: BTreeMap<String, TribeData>,
    pub terrains: BTreeMap<String, TerrainData>,
    pub resources: BTreeMap<String, ResourceData>,
    pub tasks: BTreeMap<String, TaskData>,
    pub improvements: BTreeMap<String, ImprovementData>,
    pub tech: BTreeMap<String, TechData>,
    pub skins: BTreeMap<String, SkinData>,
    /// `None` when the file omits `diplomacy_data` (version 9). Version 28 is `Some`.
    pub diplomacy: Option<DiplomacyData>,
}

/// One unit type.
///
/// `promotes_to` is never set in the rules files. Absent `growth_rate` stays
/// `None` (the game treats it as 0).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnitData {
    pub id: String,
    pub idx: i32,
    pub hidden: bool,
    pub cost: i32,
    pub health: i32,
    pub defence: i32,
    pub movement: i32,
    /// Weapon type discriminant. See [`weapon_name`].
    pub weapon: i32,
    pub range: i32,
    pub attack: i32,
    pub unit_abilities: Vec<String>,
    pub movement_terrain: Vec<String>,
    pub upgrades_from: Option<String>,
    pub promotes_to: Option<String>,
    pub growth_rate: Option<i32>,
    pub promotion_limit: i32,
}

impl UnitData {
    pub fn growth_rate_or_default(&self) -> i32 {
        self.growth_rate.unwrap_or(0)
    }
}

/// One tribe.
///
/// Override maps are string ids, as the rules files store them.
#[derive(Debug, Clone, PartialEq)]
pub struct TribeData {
    pub id: String,
    pub idx: i32,
    pub color: i32,
    pub language: String,
    pub starting_stars: Option<i32>,
    pub bonus: i32,
    pub starting_unit: String,
    pub category: i32,
    pub price_tier: i32,
    pub skins: Vec<String>,
    pub terrain_modifier: BTreeMap<String, f64>,
    pub resource_modifier: BTreeMap<String, f64>,
    pub starting_tech: Vec<String>,
    pub starting_resource: Vec<String>,
    pub tribe_abilities: Vec<String>,
    pub tech_overrides: BTreeMap<String, String>,
    pub unit_overrides: BTreeMap<String, String>,
    pub improvement_overrides: BTreeMap<String, String>,
    pub style: i32,
    pub climate: i32,
}

/// One technology.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TechData {
    pub id: String,
    pub idx: i32,
    pub cost: i32,
    pub tech_unlocks: Vec<String>,
    pub improvement_unlocks: Vec<String>,
    pub unit_unlocks: Vec<String>,
    pub ability_unlocks: Vec<String>,
    pub task_unlocks: Vec<String>,
    pub movement_unlocks: BTreeMap<String, i32>,
    pub defence_bonus_unlocks: BTreeMap<String, i32>,
}

/// One improvement. Omitted `cost` is `None`, not a guessed 0.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImprovementData {
    pub id: String,
    pub idx: i32,
    pub hidden: bool,
    pub cost: Option<i32>,
    pub work: Option<i32>,
    pub border_size: Option<i32>,
    pub max_level: Option<i32>,
    pub improvement_abilities: Vec<String>,
    pub creates: Vec<Creates>,
    pub rewards: Vec<Rewards>,
    pub terrain_requirements: Vec<TerrainRequirement>,
    pub adjacency_requirements: Vec<AdjacencyRequirement>,
    pub adjacency_improvements: Vec<AdjacencyImprovement>,
    pub routes: Vec<String>,
    pub range: Option<i32>,
    pub growth_rate: Option<i32>,
    pub growth_rewards: Vec<GrowthRewards>,
    pub should_suggest_unlock: Option<bool>,
}

impl ImprovementData {
    pub fn cost_or_default(&self) -> i32 {
        self.cost.unwrap_or(0)
    }
}

/// One terrain type. Only `idx` is stored.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TerrainData {
    pub id: String,
    pub idx: i32,
}

/// One resource type.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResourceData {
    pub id: String,
    pub idx: i32,
    pub resource_terrain_requirements: Vec<String>,
}

/// One task.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TaskData {
    pub id: String,
    pub idx: i32,
    pub improvement_unlocks: Vec<String>,
}

/// One skin. `color` is omitted for some skins.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SkinData {
    pub id: String,
    pub color: Option<i32>,
    pub language: String,
}

/// Embassy and diplomacy numbers. The growth time keeps the game's spelling,
/// `embasy_growth_time`.
#[derive(Debug, Clone, PartialEq)]
pub struct DiplomacyData {
    pub embassy_cost: i32,
    pub embassy_income: i32,
    pub embasy_growth_time: i32,
    pub embassy_max_level: i32,
    pub embassy_upgrade_cost: i32,
    pub dividend_income: i32,
    pub embassy_tech_price_multiplier: f64,
}

/// Rewards of an improvement. Absent ints stay `None`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Rewards {
    pub population: Option<i32>,
    pub currency: Option<i32>,
    pub score: Option<i32>,
}

impl Rewards {
    pub fn population_or_default(&self) -> i32 {
        self.population.unwrap_or(0)
    }
    pub fn currency_or_default(&self) -> i32 {
        self.currency.unwrap_or(0)
    }
    pub fn score_or_default(&self) -> i32 {
        self.score.unwrap_or(0)
    }
}

/// Rewards for an improvement levelling up.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GrowthRewards {
    pub score: Option<i32>,
    pub population: Option<i32>,
}

/// Terrain or resource an improvement needs under it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TerrainRequirement {
    pub terrain: Option<String>,
    pub resource: Option<String>,
}

/// What an improvement needs on neighbouring tiles.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AdjacencyRequirement {
    pub terrain: Option<String>,
    pub improvement: Option<String>,
    pub resource: Option<String>,
    pub not_improvement: Option<String>,
}

/// Improvements that count towards a neighbouring resource.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AdjacencyImprovement {
    pub improvement: Option<String>,
    pub resources: Option<String>,
}

/// What an improvement creates. `effect` is a name such as `algae`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Creates {
    pub terrain: Option<String>,
    pub resource: Option<String>,
    pub unit: Option<String>,
    pub effect: Option<String>,
}

/// Name of a `UnitData::weapon` value. This does not invent a unit.
pub fn weapon_name(weapon: i32) -> Option<&'static str> {
    const NAMES: [&str; 18] = [
        "None", "Club", "Sword", "Arrow", "Magic", "Gun", "Rock", "Claw", "FireBlow", "Trident", "IceArrow", "Poison",
        "Sting", "Dagger", "Burn", "Laser", "Pierce", "Water",
    ];
    usize::try_from(weapon).ok().and_then(|i| NAMES.get(i).copied())
}

impl GameLogicData {
    pub fn unit(&self, id: &str) -> Option<&UnitData> {
        self.units.get(id)
    }
    pub fn unit_by_idx(&self, idx: i32) -> Option<&UnitData> {
        self.units.values().find(|u| u.idx == idx)
    }
    pub fn tribe(&self, id: &str) -> Option<&TribeData> {
        self.tribes.get(id)
    }
    pub fn tribe_by_idx(&self, idx: i32) -> Option<&TribeData> {
        self.tribes.values().find(|t| t.idx == idx)
    }
    pub fn tech(&self, id: &str) -> Option<&TechData> {
        self.tech.get(id)
    }
    pub fn improvement(&self, id: &str) -> Option<&ImprovementData> {
        self.improvements.get(id)
    }
    pub fn terrain(&self, id: &str) -> Option<&TerrainData> {
        self.terrains.get(id)
    }
    pub fn resource(&self, id: &str) -> Option<&ResourceData> {
        self.resources.get(id)
    }
    pub fn task(&self, id: &str) -> Option<&TaskData> {
        self.tasks.get(id)
    }
    pub fn skin(&self, id: &str) -> Option<&SkinData> {
        self.skins.get(id)
    }
}

/// Load the built-in version-28 rules.
pub fn load_embedded_game_logic() -> Result<GameLogicData, DataError> {
    load_game_logic_data(GAME_LOGIC_DATA_VERSION)
}

/// [`load_embedded_game_logic`] without the copy: the cached data, or `None` when it does not load.
pub fn embedded_game_logic_ref() -> Option<&'static GameLogicData> {
    static DATA: OnceLock<Option<GameLogicData>> = OnceLock::new();
    DATA.get_or_init(|| load_embedded_game_logic().ok()).as_ref()
}

/// Load built-in `GameLogicData` for a logic-data version in 9..=28.
///
/// The argument is the rules-file version, not the game version.
pub fn load_game_logic_data(logic_data_version: i32) -> Result<GameLogicData, DataError> {
    static CACHE: [OnceLock<Result<GameLogicData, DataError>>; 20] = [const { OnceLock::new() }; 20];
    let slot = usize::try_from(logic_data_version - rules::OLDEST_VERSION).ok().and_then(|i| CACHE.get(i));
    match slot {
        Some(cell) => cell.get_or_init(|| rules::load(rules::Source::Embedded, logic_data_version)).clone(),
        None => rules::load(rules::Source::Embedded, logic_data_version),
    }
}

/// Load the built-in rules for a game version.
///
/// `game_version` is mapped with [`crate::logic_data_version`]. A logic-data
/// version outside 9..=28 is the [`DataError`] from [`load_game_logic_data`].
pub fn load_game_logic(game_version: i32) -> Result<GameLogicData, DataError> {
    load_game_logic_data(crate::logic_data_version(game_version))
}

/// Like [`load_game_logic_data`], reading the rules files (`v9.json` ..) from a directory.
pub fn load_game_logic_data_from_dir(
    dir: &std::path::Path,
    logic_data_version: i32,
) -> Result<GameLogicData, DataError> {
    rules::load(rules::Source::Dir(dir), logic_data_version)
}

/// Like [`load_game_logic`], reading the rules files from a directory.
pub fn load_game_logic_from_dir(dir: &std::path::Path, game_version: i32) -> Result<GameLogicData, DataError> {
    load_game_logic_data_from_dir(dir, crate::logic_data_version(game_version))
}

/// Parse one `GameLogicData` document in the game's own JSON layout. Missing records are not invented.
pub fn load_game_logic_json(json: &str) -> Result<GameLogicData, DataError> {
    let value = json_parse::parse(json)?;
    let root = json_parse::as_object(&value, "root")?;
    allow(
        root,
        &[
            "tribeData",
            "techData",
            "unitData",
            "improvementData",
            "terrainData",
            "resourceData",
            "taskData",
            "skinData",
            "diplomacyData",
        ],
        "root",
    )?;
    for key in ["tribeData", "techData", "unitData", "improvementData", "terrainData", "resourceData", "taskData"] {
        if !root.iter().any(|(k, _)| k == key) {
            return Err(DataError::msg(format!("missing {key}")));
        }
    }

    let units = parse_indexed(root, "unitData", parse_unit)?;
    let tribes = parse_indexed(root, "tribeData", parse_tribe)?;
    let tech = parse_indexed(root, "techData", parse_tech)?;
    let improvements = parse_indexed(root, "improvementData", parse_improvement)?;
    let terrains = parse_indexed(root, "terrainData", parse_terrain)?;
    let resources = parse_indexed(root, "resourceData", parse_resource)?;
    let tasks = parse_indexed(root, "taskData", parse_task)?;
    let skins = if root.iter().any(|(k, _)| k == "skinData") {
        parse_map(root, "skinData", parse_skin)?
    } else {
        BTreeMap::new()
    };
    let diplomacy = match root.iter().find(|(k, _)| k == "diplomacyData") {
        Some((_, value)) => Some(parse_diplomacy(value)?),
        None => None,
    };

    Ok(GameLogicData { units, tribes, terrains, resources, tasks, improvements, tech, skins, diplomacy })
}

fn parse_unit(id: &str, fields: &[(String, json_parse::Value)]) -> Result<UnitData, DataError> {
    let ctx = format!("unitData.{id}");
    allow(
        fields,
        &[
            "idx",
            "hidden",
            "cost",
            "health",
            "defence",
            "movement",
            "weapon",
            "range",
            "attack",
            "unitAbilities",
            "movementTerrain",
            "upgradesFrom",
            "promotesTo",
            "growthRate",
            "promotionLimit",
        ],
        &ctx,
    )?;
    Ok(UnitData {
        id: id.to_string(),
        idx: req_i32(fields, "idx", &ctx)?,
        hidden: bool_or(fields, "hidden", false, &ctx)?,
        cost: req_i32(fields, "cost", &ctx)?,
        health: req_i32(fields, "health", &ctx)?,
        defence: req_i32(fields, "defence", &ctx)?,
        movement: req_i32(fields, "movement", &ctx)?,
        weapon: req_i32(fields, "weapon", &ctx)?,
        range: req_i32(fields, "range", &ctx)?,
        attack: req_i32(fields, "attack", &ctx)?,
        unit_abilities: opt_strings(fields, "unitAbilities", &ctx)?,
        movement_terrain: opt_strings(fields, "movementTerrain", &ctx)?,
        upgrades_from: opt_string(fields, "upgradesFrom", &ctx)?,
        promotes_to: opt_string(fields, "promotesTo", &ctx)?,
        growth_rate: opt_i32(fields, "growthRate", &ctx)?,
        promotion_limit: req_i32(fields, "promotionLimit", &ctx)?,
    })
}

fn parse_tribe(id: &str, fields: &[(String, json_parse::Value)]) -> Result<TribeData, DataError> {
    let ctx = format!("tribeData.{id}");
    allow(
        fields,
        &[
            "idx",
            "color",
            "language",
            "startingStars",
            "bonus",
            "startingUnit",
            "category",
            "priceTier",
            "skins",
            "terrainModifier",
            "resourceModifier",
            "startingTech",
            "startingResource",
            "tribeAbilities",
            "techOverrides",
            "unitOverrides",
            "improvementOverrides",
            "style",
            "climate",
        ],
        &ctx,
    )?;
    Ok(TribeData {
        id: id.to_string(),
        idx: req_i32(fields, "idx", &ctx)?,
        color: req_i32(fields, "color", &ctx)?,
        language: req_string(fields, "language", &ctx)?,
        starting_stars: opt_i32(fields, "startingStars", &ctx)?,
        bonus: req_i32(fields, "bonus", &ctx)?,
        starting_unit: req_string(fields, "startingUnit", &ctx)?,
        category: req_i32(fields, "category", &ctx)?,
        price_tier: req_i32(fields, "priceTier", &ctx)?,
        skins: opt_strings(fields, "skins", &ctx)?,
        terrain_modifier: opt_f64_map(fields, "terrainModifier", &ctx)?,
        resource_modifier: opt_f64_map(fields, "resourceModifier", &ctx)?,
        starting_tech: opt_strings(fields, "startingTech", &ctx)?,
        starting_resource: opt_strings(fields, "startingResource", &ctx)?,
        tribe_abilities: opt_strings(fields, "tribeAbilities", &ctx)?,
        tech_overrides: opt_string_map(fields, "techOverrides", &ctx)?,
        unit_overrides: opt_string_map(fields, "unitOverrides", &ctx)?,
        improvement_overrides: opt_string_map(fields, "improvementOverrides", &ctx)?,
        style: req_i32(fields, "style", &ctx)?,
        climate: req_i32(fields, "climate", &ctx)?,
    })
}

fn parse_tech(id: &str, fields: &[(String, json_parse::Value)]) -> Result<TechData, DataError> {
    let ctx = format!("techData.{id}");
    allow(
        fields,
        &[
            "idx",
            "cost",
            "techUnlocks",
            "improvementUnlocks",
            "unitUnlocks",
            "abilityUnlocks",
            "taskUnlocks",
            "movementUnlocks",
            "defenceBonusUnlocks",
        ],
        &ctx,
    )?;
    Ok(TechData {
        id: id.to_string(),
        idx: req_i32(fields, "idx", &ctx)?,
        cost: req_i32(fields, "cost", &ctx)?,
        tech_unlocks: opt_strings(fields, "techUnlocks", &ctx)?,
        improvement_unlocks: opt_strings(fields, "improvementUnlocks", &ctx)?,
        unit_unlocks: opt_strings(fields, "unitUnlocks", &ctx)?,
        ability_unlocks: opt_strings(fields, "abilityUnlocks", &ctx)?,
        task_unlocks: opt_strings(fields, "taskUnlocks", &ctx)?,
        movement_unlocks: opt_i32_map(fields, "movementUnlocks", &ctx)?,
        defence_bonus_unlocks: opt_i32_map(fields, "defenceBonusUnlocks", &ctx)?,
    })
}

fn parse_improvement(id: &str, fields: &[(String, json_parse::Value)]) -> Result<ImprovementData, DataError> {
    let ctx = format!("improvementData.{id}");
    allow(
        fields,
        &[
            "idx",
            "hidden",
            "cost",
            "work",
            "borderSize",
            "maxLevel",
            "improvementAbilities",
            "creates",
            "rewards",
            "terrainRequirements",
            "adjacencyRequirements",
            "adjacencyImprovements",
            "routes",
            "range",
            "growthRate",
            "growthRewards",
            "shouldSuggestUnlock",
        ],
        &ctx,
    )?;
    Ok(ImprovementData {
        id: id.to_string(),
        idx: req_i32(fields, "idx", &ctx)?,
        hidden: bool_or(fields, "hidden", false, &ctx)?,
        cost: opt_i32(fields, "cost", &ctx)?,
        work: opt_i32(fields, "work", &ctx)?,
        border_size: opt_i32(fields, "borderSize", &ctx)?,
        max_level: opt_i32(fields, "maxLevel", &ctx)?,
        improvement_abilities: opt_strings(fields, "improvementAbilities", &ctx)?,
        creates: opt_objects(fields, "creates", &ctx, parse_creates)?,
        rewards: opt_objects(fields, "rewards", &ctx, parse_rewards)?,
        terrain_requirements: opt_objects(fields, "terrainRequirements", &ctx, parse_terrain_req)?,
        adjacency_requirements: opt_objects(fields, "adjacencyRequirements", &ctx, parse_adjacency_req)?,
        adjacency_improvements: opt_objects(fields, "adjacencyImprovements", &ctx, parse_adjacency_imp)?,
        routes: opt_strings(fields, "routes", &ctx)?,
        range: opt_i32(fields, "range", &ctx)?,
        growth_rate: opt_i32(fields, "growthRate", &ctx)?,
        growth_rewards: opt_objects(fields, "growthRewards", &ctx, parse_growth_rewards)?,
        should_suggest_unlock: opt_bool(fields, "shouldSuggestUnlock", &ctx)?,
    })
}

fn parse_terrain(id: &str, fields: &[(String, json_parse::Value)]) -> Result<TerrainData, DataError> {
    let ctx = format!("terrainData.{id}");
    allow(fields, &["idx"], &ctx)?;
    Ok(TerrainData { id: id.to_string(), idx: req_i32(fields, "idx", &ctx)? })
}

fn parse_resource(id: &str, fields: &[(String, json_parse::Value)]) -> Result<ResourceData, DataError> {
    let ctx = format!("resourceData.{id}");
    allow(fields, &["idx", "resourceTerrainRequirements"], &ctx)?;
    Ok(ResourceData {
        id: id.to_string(),
        idx: req_i32(fields, "idx", &ctx)?,
        resource_terrain_requirements: opt_strings(fields, "resourceTerrainRequirements", &ctx)?,
    })
}

fn parse_task(id: &str, fields: &[(String, json_parse::Value)]) -> Result<TaskData, DataError> {
    let ctx = format!("taskData.{id}");
    allow(fields, &["idx", "improvementUnlocks"], &ctx)?;
    Ok(TaskData {
        id: id.to_string(),
        idx: req_i32(fields, "idx", &ctx)?,
        improvement_unlocks: opt_strings(fields, "improvementUnlocks", &ctx)?,
    })
}

fn parse_skin(id: &str, fields: &[(String, json_parse::Value)]) -> Result<SkinData, DataError> {
    let ctx = format!("skinData.{id}");
    allow(fields, &["color", "language"], &ctx)?;
    Ok(SkinData {
        id: id.to_string(),
        color: opt_i32(fields, "color", &ctx)?,
        language: req_string(fields, "language", &ctx)?,
    })
}

fn parse_diplomacy(value: &json_parse::Value) -> Result<DiplomacyData, DataError> {
    let fields = json_parse::as_object(value, "diplomacyData")?;
    allow(
        fields,
        &[
            "embassyCost",
            "embassyIncome",
            "embasyGrowthTime",
            "embassyMaxLevel",
            "embassyUpgradeCost",
            "dividendIncome",
            "embassyTechPriceMultiplier",
        ],
        "diplomacyData",
    )?;
    Ok(DiplomacyData {
        embassy_cost: req_i32(fields, "embassyCost", "diplomacyData")?,
        embassy_income: req_i32(fields, "embassyIncome", "diplomacyData")?,
        embasy_growth_time: req_i32(fields, "embasyGrowthTime", "diplomacyData")?,
        embassy_max_level: req_i32(fields, "embassyMaxLevel", "diplomacyData")?,
        embassy_upgrade_cost: req_i32(fields, "embassyUpgradeCost", "diplomacyData")?,
        dividend_income: req_i32(fields, "dividendIncome", "diplomacyData")?,
        embassy_tech_price_multiplier: req_f64(fields, "embassyTechPriceMultiplier", "diplomacyData")?,
    })
}

fn parse_rewards(fields: &[(String, json_parse::Value)], ctx: &str) -> Result<Rewards, DataError> {
    allow(fields, &["population", "currency", "score"], ctx)?;
    Ok(Rewards {
        population: opt_i32(fields, "population", ctx)?,
        currency: opt_i32(fields, "currency", ctx)?,
        score: opt_i32(fields, "score", ctx)?,
    })
}

fn parse_growth_rewards(fields: &[(String, json_parse::Value)], ctx: &str) -> Result<GrowthRewards, DataError> {
    allow(fields, &["score", "population"], ctx)?;
    Ok(GrowthRewards { score: opt_i32(fields, "score", ctx)?, population: opt_i32(fields, "population", ctx)? })
}

fn parse_terrain_req(fields: &[(String, json_parse::Value)], ctx: &str) -> Result<TerrainRequirement, DataError> {
    allow(fields, &["terrain", "resource"], ctx)?;
    Ok(TerrainRequirement {
        terrain: opt_string(fields, "terrain", ctx)?,
        resource: opt_string(fields, "resource", ctx)?,
    })
}

fn parse_adjacency_req(fields: &[(String, json_parse::Value)], ctx: &str) -> Result<AdjacencyRequirement, DataError> {
    allow(fields, &["terrain", "improvement", "resource", "notImprovement"], ctx)?;
    Ok(AdjacencyRequirement {
        terrain: opt_string(fields, "terrain", ctx)?,
        improvement: opt_string(fields, "improvement", ctx)?,
        resource: opt_string(fields, "resource", ctx)?,
        not_improvement: opt_string(fields, "notImprovement", ctx)?,
    })
}

fn parse_adjacency_imp(fields: &[(String, json_parse::Value)], ctx: &str) -> Result<AdjacencyImprovement, DataError> {
    allow(fields, &["improvement", "resources"], ctx)?;
    Ok(AdjacencyImprovement {
        improvement: opt_string(fields, "improvement", ctx)?,
        resources: opt_string(fields, "resources", ctx)?,
    })
}

fn parse_creates(fields: &[(String, json_parse::Value)], ctx: &str) -> Result<Creates, DataError> {
    allow(fields, &["terrain", "resource", "unit", "effect"], ctx)?;
    Ok(Creates {
        terrain: opt_string(fields, "terrain", ctx)?,
        resource: opt_string(fields, "resource", ctx)?,
        unit: opt_string(fields, "unit", ctx)?,
        effect: opt_string(fields, "effect", ctx)?,
    })
}

fn parse_indexed<T, F>(
    root: &[(String, json_parse::Value)],
    key: &str,
    parse_one: F,
) -> Result<BTreeMap<String, T>, DataError>
where
    T: Idx,
    F: Fn(&str, &[(String, json_parse::Value)]) -> Result<T, DataError>,
{
    let map = parse_map(root, key, parse_one)?;
    let mut seen = BTreeSet::new();
    for (id, item) in &map {
        if !seen.insert(item.idx()) {
            return Err(DataError::msg(format!("duplicate idx {} in {key} ({id})", item.idx())));
        }
    }
    Ok(map)
}

fn parse_map<T, F>(
    root: &[(String, json_parse::Value)],
    key: &str,
    parse_one: F,
) -> Result<BTreeMap<String, T>, DataError>
where
    F: Fn(&str, &[(String, json_parse::Value)]) -> Result<T, DataError>,
{
    let obj = json_parse::as_object(need(root, key, "root")?, key)?;
    let mut map = BTreeMap::new();
    for (id, value) in obj {
        let ctx = format!("{key}.{id}");
        let fields = json_parse::as_object(value, &ctx)?;
        let parsed = parse_one(id, fields)?;
        map.insert(id.clone(), parsed);
    }
    Ok(map)
}

trait Idx {
    fn idx(&self) -> i32;
}

impl Idx for UnitData {
    fn idx(&self) -> i32 {
        self.idx
    }
}
impl Idx for TribeData {
    fn idx(&self) -> i32 {
        self.idx
    }
}
impl Idx for TechData {
    fn idx(&self) -> i32 {
        self.idx
    }
}
impl Idx for ImprovementData {
    fn idx(&self) -> i32 {
        self.idx
    }
}
impl Idx for TerrainData {
    fn idx(&self) -> i32 {
        self.idx
    }
}
impl Idx for ResourceData {
    fn idx(&self) -> i32 {
        self.idx
    }
}
impl Idx for TaskData {
    fn idx(&self) -> i32 {
        self.idx
    }
}

fn allow(fields: &[(String, json_parse::Value)], keys: &[&str], ctx: &str) -> Result<(), DataError> {
    for (key, _) in fields {
        if !keys.iter().any(|allowed| allowed == key) {
            return Err(DataError::msg(format!("unknown field {key} in {ctx}")));
        }
    }
    Ok(())
}

fn need<'a>(
    fields: &'a [(String, json_parse::Value)],
    key: &str,
    ctx: &str,
) -> Result<&'a json_parse::Value, DataError> {
    fields
        .iter()
        .find(|(k, _)| k == key)
        .map(|(_, v)| v)
        .ok_or_else(|| DataError::msg(format!("missing {key} in {ctx}")))
}

fn req_i32(fields: &[(String, json_parse::Value)], key: &str, ctx: &str) -> Result<i32, DataError> {
    json_parse::as_i32(need(fields, key, ctx)?, &format!("{ctx}.{key}"))
}

fn opt_i32(fields: &[(String, json_parse::Value)], key: &str, ctx: &str) -> Result<Option<i32>, DataError> {
    match fields.iter().find(|(k, _)| k == key) {
        None => Ok(None),
        Some((_, value)) => json_parse::as_i32(value, &format!("{ctx}.{key}")).map(Some),
    }
}

fn req_f64(fields: &[(String, json_parse::Value)], key: &str, ctx: &str) -> Result<f64, DataError> {
    json_parse::as_f64(need(fields, key, ctx)?, &format!("{ctx}.{key}"))
}

fn req_string(fields: &[(String, json_parse::Value)], key: &str, ctx: &str) -> Result<String, DataError> {
    json_parse::as_string(need(fields, key, ctx)?, &format!("{ctx}.{key}"))
}

fn opt_string(fields: &[(String, json_parse::Value)], key: &str, ctx: &str) -> Result<Option<String>, DataError> {
    match fields.iter().find(|(k, _)| k == key) {
        None => Ok(None),
        Some((_, json_parse::Value::Null)) => Ok(None),
        Some((_, value)) => json_parse::as_string(value, &format!("{ctx}.{key}")).map(Some),
    }
}

fn bool_or(fields: &[(String, json_parse::Value)], key: &str, default: bool, ctx: &str) -> Result<bool, DataError> {
    match fields.iter().find(|(k, _)| k == key) {
        None => Ok(default),
        Some((_, json_parse::Value::Bool(v))) => Ok(*v),
        Some(_) => Err(DataError::msg(format!("{ctx}.{key} is not a bool"))),
    }
}

fn opt_bool(fields: &[(String, json_parse::Value)], key: &str, ctx: &str) -> Result<Option<bool>, DataError> {
    match fields.iter().find(|(k, _)| k == key) {
        None => Ok(None),
        Some((_, json_parse::Value::Bool(v))) => Ok(Some(*v)),
        Some(_) => Err(DataError::msg(format!("{ctx}.{key} is not a bool"))),
    }
}

fn opt_strings(fields: &[(String, json_parse::Value)], key: &str, ctx: &str) -> Result<Vec<String>, DataError> {
    match fields.iter().find(|(k, _)| k == key) {
        None => Ok(Vec::new()),
        Some((_, value)) => json_parse::as_string_array(value, &format!("{ctx}.{key}")),
    }
}

fn opt_string_map(
    fields: &[(String, json_parse::Value)],
    key: &str,
    ctx: &str,
) -> Result<BTreeMap<String, String>, DataError> {
    match fields.iter().find(|(k, _)| k == key) {
        None => Ok(BTreeMap::new()),
        Some((_, value)) => {
            let obj = json_parse::as_object(value, &format!("{ctx}.{key}"))?;
            let mut map = BTreeMap::new();
            for (id, item) in obj {
                map.insert(id.clone(), json_parse::as_string(item, &format!("{ctx}.{key}.{id}"))?);
            }
            Ok(map)
        }
    }
}

fn opt_i32_map(
    fields: &[(String, json_parse::Value)],
    key: &str,
    ctx: &str,
) -> Result<BTreeMap<String, i32>, DataError> {
    match fields.iter().find(|(k, _)| k == key) {
        None => Ok(BTreeMap::new()),
        Some((_, value)) => {
            let obj = json_parse::as_object(value, &format!("{ctx}.{key}"))?;
            let mut map = BTreeMap::new();
            for (id, item) in obj {
                map.insert(id.clone(), json_parse::as_i32(item, &format!("{ctx}.{key}.{id}"))?);
            }
            Ok(map)
        }
    }
}

fn opt_f64_map(
    fields: &[(String, json_parse::Value)],
    key: &str,
    ctx: &str,
) -> Result<BTreeMap<String, f64>, DataError> {
    match fields.iter().find(|(k, _)| k == key) {
        None => Ok(BTreeMap::new()),
        Some((_, value)) => {
            let obj = json_parse::as_object(value, &format!("{ctx}.{key}"))?;
            let mut map = BTreeMap::new();
            for (id, item) in obj {
                map.insert(id.clone(), json_parse::as_f64(item, &format!("{ctx}.{key}.{id}"))?);
            }
            Ok(map)
        }
    }
}

fn opt_objects<T, F>(
    fields: &[(String, json_parse::Value)],
    key: &str,
    ctx: &str,
    parse_one: F,
) -> Result<Vec<T>, DataError>
where
    F: Fn(&[(String, json_parse::Value)], &str) -> Result<T, DataError>,
{
    match fields.iter().find(|(k, _)| k == key) {
        None => Ok(Vec::new()),
        Some((_, value)) => {
            let items = json_parse::as_array(value, &format!("{ctx}.{key}"))?;
            let mut out = Vec::with_capacity(items.len());
            for (i, item) in items.iter().enumerate() {
                let item_ctx = format!("{ctx}.{key}[{i}]");
                let obj = json_parse::as_object(item, &item_ctx)?;
                out.push(parse_one(obj, &item_ctx)?);
            }
            Ok(out)
        }
    }
}

mod json_parse {
    use super::DataError;

    #[derive(Debug, Clone, PartialEq)]
    pub(super) enum Value {
        Null,
        Bool(bool),
        Int(i64),
        Float(f64),
        Str(String),
        Arr(Vec<Value>),
        Obj(Vec<(String, Value)>),
    }

    pub(super) fn parse(src: &str) -> Result<Value, DataError> {
        let mut parser = Parser { src: src.as_bytes(), i: 0 };
        let value = parser.parse_value()?;
        parser.skip_ws();
        if parser.i != parser.src.len() {
            return Err(parser.err("trailing data after JSON value"));
        }
        Ok(value)
    }

    pub(super) fn as_object<'a>(value: &'a Value, ctx: &str) -> Result<&'a [(String, Value)], DataError> {
        match value {
            Value::Obj(items) => Ok(items),
            _ => Err(DataError::msg(format!("{ctx} is not an object"))),
        }
    }

    pub(super) fn as_array<'a>(value: &'a Value, ctx: &str) -> Result<&'a [Value], DataError> {
        match value {
            Value::Arr(items) => Ok(items),
            _ => Err(DataError::msg(format!("{ctx} is not an array"))),
        }
    }

    pub(super) fn as_i32(value: &Value, ctx: &str) -> Result<i32, DataError> {
        match value {
            Value::Int(n) => i32::try_from(*n).map_err(|_| DataError::msg(format!("{ctx} does not fit i32"))),
            Value::Float(n) if n.fract() == 0.0 && *n >= i32::MIN as f64 && *n <= i32::MAX as f64 => Ok(*n as i32),
            _ => Err(DataError::msg(format!("{ctx} is not an integer"))),
        }
    }

    pub(super) fn as_f64(value: &Value, ctx: &str) -> Result<f64, DataError> {
        match value {
            Value::Float(n) => Ok(*n),
            Value::Int(n) => Ok(*n as f64),
            _ => Err(DataError::msg(format!("{ctx} is not a number"))),
        }
    }

    pub(super) fn as_string(value: &Value, ctx: &str) -> Result<String, DataError> {
        match value {
            Value::Str(s) => Ok(s.clone()),
            _ => Err(DataError::msg(format!("{ctx} is not a string"))),
        }
    }

    pub(super) fn as_string_array(value: &Value, ctx: &str) -> Result<Vec<String>, DataError> {
        let items = as_array(value, ctx)?;
        let mut out = Vec::with_capacity(items.len());
        for (i, item) in items.iter().enumerate() {
            out.push(as_string(item, &format!("{ctx}[{i}]"))?);
        }
        Ok(out)
    }

    struct Parser<'a> {
        src: &'a [u8],
        i: usize,
    }

    impl<'a> Parser<'a> {
        fn err(&self, message: impl Into<String>) -> DataError {
            DataError::msg(format!("{} at byte {}", message.into(), self.i))
        }

        fn peek(&self) -> Option<u8> {
            self.src.get(self.i).copied()
        }

        fn bump(&mut self) -> Result<u8, DataError> {
            let b = self.peek().ok_or_else(|| self.err("unexpected end"))?;
            self.i += 1;
            Ok(b)
        }

        fn skip_ws(&mut self) {
            while matches!(self.peek(), Some(b' ' | b'\n' | b'\r' | b'\t')) {
                self.i += 1;
            }
        }

        fn parse_value(&mut self) -> Result<Value, DataError> {
            self.skip_ws();
            match self.peek() {
                Some(b'{') => self.parse_object().map(Value::Obj),
                Some(b'[') => self.parse_array().map(Value::Arr),
                Some(b'"') => self.parse_string().map(Value::Str),
                Some(b't' | b'f' | b'n') => self.parse_literal(),
                Some(b'-' | b'0'..=b'9') => self.parse_number(),
                Some(b) => Err(self.err(format!("unexpected byte {b}"))),
                None => Err(self.err("unexpected end")),
            }
        }

        fn parse_object(&mut self) -> Result<Vec<(String, Value)>, DataError> {
            self.bump()?;
            let mut items = Vec::new();
            self.skip_ws();
            if self.peek() == Some(b'}') {
                self.i += 1;
                return Ok(items);
            }
            loop {
                self.skip_ws();
                let key = self.parse_string()?;
                self.skip_ws();
                if self.bump()? != b':' {
                    return Err(self.err("expected ':'"));
                }
                let value = self.parse_value()?;
                if let Some(pos) = items.iter().position(|(existing, _)| existing == &key) {
                    if items[pos].1 != value {
                        return Err(self.err(format!("duplicate key {key} with different values")));
                    }
                    items[pos].1 = value;
                } else {
                    items.push((key, value));
                }
                self.skip_ws();
                match self.bump()? {
                    b'}' => return Ok(items),
                    b',' => {
                        // The game's reader accepts a single trailing comma.
                        self.skip_ws();
                        if self.peek() == Some(b'}') {
                            self.i += 1;
                            return Ok(items);
                        }
                        continue;
                    }
                    _ => return Err(self.err("expected ',' or '}'")),
                }
            }
        }

        fn parse_array(&mut self) -> Result<Vec<Value>, DataError> {
            self.bump()?;
            let mut items = Vec::new();
            self.skip_ws();
            if self.peek() == Some(b']') {
                self.i += 1;
                return Ok(items);
            }
            loop {
                items.push(self.parse_value()?);
                self.skip_ws();
                match self.bump()? {
                    b']' => return Ok(items),
                    b',' => {
                        // Same for arrays.
                        self.skip_ws();
                        if self.peek() == Some(b']') {
                            self.i += 1;
                            return Ok(items);
                        }
                        continue;
                    }
                    _ => return Err(self.err("expected ',' or ']'")),
                }
            }
        }

        fn parse_literal(&mut self) -> Result<Value, DataError> {
            if self.consume(b"true") {
                return Ok(Value::Bool(true));
            }
            if self.consume(b"false") {
                return Ok(Value::Bool(false));
            }
            if self.consume(b"null") {
                return Ok(Value::Null);
            }
            Err(self.err("invalid literal"))
        }

        fn consume(&mut self, lit: &[u8]) -> bool {
            if self.src[self.i..].starts_with(lit) {
                let after = self.i + lit.len();
                if self.src.get(after).is_some_and(|b| b.is_ascii_alphanumeric()) {
                    return false;
                }
                self.i = after;
                true
            } else {
                false
            }
        }

        fn parse_number(&mut self) -> Result<Value, DataError> {
            let start = self.i;
            if self.peek() == Some(b'-') {
                self.i += 1;
            }
            let int_start = self.i;
            let digits = self.take_digits();
            if digits == 0 {
                return Err(self.err("invalid number"));
            }
            if digits > 1 && self.src[int_start] == b'0' {
                return Err(self.err("invalid number"));
            }
            let mut float = false;
            if self.peek() == Some(b'.') {
                float = true;
                self.i += 1;
                if self.take_digits() == 0 {
                    return Err(self.err("invalid number"));
                }
            }
            if matches!(self.peek(), Some(b'e' | b'E')) {
                float = true;
                self.i += 1;
                if matches!(self.peek(), Some(b'+' | b'-')) {
                    self.i += 1;
                }
                if self.take_digits() == 0 {
                    return Err(self.err("invalid number"));
                }
            }
            let token = std::str::from_utf8(&self.src[start..self.i]).map_err(|_| self.err("invalid number"))?;
            if float {
                let n = token.parse::<f64>().map_err(|_| self.err("invalid number"))?;
                Ok(Value::Float(n))
            } else {
                let n = token.parse::<i64>().map_err(|_| self.err("invalid number"))?;
                Ok(Value::Int(n))
            }
        }

        fn take_digits(&mut self) -> usize {
            let start = self.i;
            while matches!(self.peek(), Some(b'0'..=b'9')) {
                self.i += 1;
            }
            self.i - start
        }

        fn parse_string(&mut self) -> Result<String, DataError> {
            if self.bump()? != b'"' {
                return Err(self.err("expected string"));
            }
            let mut out = Vec::new();
            loop {
                let b = self.bump()?;
                match b {
                    b'"' => break,
                    b'\\' => {
                        let esc = self.bump()?;
                        match esc {
                            b'"' | b'\\' | b'/' => out.push(esc),
                            b'b' => out.push(0x08),
                            b'f' => out.push(0x0c),
                            b'n' => out.push(b'\n'),
                            b'r' => out.push(b'\r'),
                            b't' => out.push(b'\t'),
                            b'u' => self.push_unicode(&mut out)?,
                            _ => return Err(self.err("invalid string escape")),
                        }
                    }
                    0x00..=0x1f => return Err(self.err("unescaped control in string")),
                    byte => out.push(byte),
                }
            }
            String::from_utf8(out).map_err(|_| self.err("string is not utf-8"))
        }

        fn push_unicode(&mut self, out: &mut Vec<u8>) -> Result<(), DataError> {
            let unit = self.read_hex4()?;
            let cp = if (0xD800..=0xDBFF).contains(&unit) {
                if self.bump()? != b'\\' || self.bump()? != b'u' {
                    return Err(self.err("lonely unicode surrogate"));
                }
                let low = self.read_hex4()?;
                if !(0xDC00..=0xDFFF).contains(&low) {
                    return Err(self.err("lonely unicode surrogate"));
                }
                0x10000 + (((unit as u32 - 0xD800) << 10) | (low as u32 - 0xDC00))
            } else if (0xDC00..=0xDFFF).contains(&unit) {
                return Err(self.err("lonely unicode surrogate"));
            } else {
                unit as u32
            };
            let ch = char::from_u32(cp).ok_or_else(|| self.err("invalid unicode scalar"))?;
            let mut buf = [0u8; 4];
            out.extend_from_slice(ch.encode_utf8(&mut buf).as_bytes());
            Ok(())
        }

        fn read_hex4(&mut self) -> Result<u16, DataError> {
            let mut n = 0u16;
            for _ in 0..4 {
                let h = self.bump()?;
                let dig = match h {
                    b'0'..=b'9' => h - b'0',
                    b'a'..=b'f' => h - b'a' + 10,
                    b'A'..=b'F' => h - b'A' + 10,
                    _ => return Err(self.err("invalid hex escape")),
                };
                n = (n << 4) | u16::from(dig);
            }
            Ok(n)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn load() -> GameLogicData {
        load_embedded_game_logic().expect("embedded GameLogicData28")
    }

    #[test]
    fn embedded_rules_are_the_engines_own_files() {
        assert_eq!(GAME_LOGIC_DATA_VERSION, 28);
        assert_eq!(rules::BASE_VERSION, 28);
        assert_eq!(rules::EMBEDDED_FILES.len(), 20);
        for (i, (version, text)) in rules::EMBEDDED_FILES.iter().enumerate() {
            assert_eq!(*version, 9 + i as i32);
            assert!(text.starts_with("{\n  \"format\": \"polytopia-rules-1\",\n"), "v{version}");
            assert_eq!(text.contains("\"from\""), *version != 28, "v{version}");
        }
        assert!(rules::EMBEDDED_FILES.iter().all(|(_, text)| !text.contains("tribeData")));
    }

    #[test]
    fn loads_version_28_records_present_in_the_extract() {
        let data = load();
        assert_eq!(data.units.len(), 59);
        assert_eq!(data.tribes.len(), 18);
        assert_eq!(data.tech.len(), 49);
        assert_eq!(data.improvements.len(), 49);
        assert_eq!(data.terrains.len(), 9);
        assert_eq!(data.resources.len(), 10);
        assert_eq!(data.tasks.len(), 9);
        assert_eq!(data.skins.len(), 3);
    }

    #[test]
    fn warrior_stats_match_the_extract() {
        let data = load();
        let warrior = data.unit("warrior").expect("warrior");
        assert_eq!(warrior.idx, 2);
        assert!(!warrior.hidden);
        assert_eq!(warrior.cost, 2);
        assert_eq!(warrior.health, 100);
        assert_eq!(warrior.defence, 20);
        assert_eq!(warrior.movement, 1);
        assert_eq!(warrior.range, 1);
        assert_eq!(warrior.attack, 20);
        assert_eq!(warrior.weapon, 1);
        assert_eq!(weapon_name(warrior.weapon), Some("Club"));
        assert_eq!(warrior.unit_abilities, ["dash", "fortify", "land"]);
        assert!(warrior.movement_terrain.is_empty());
        assert_eq!(warrior.upgrades_from, None);
        assert_eq!(warrior.promotes_to, None);
        assert_eq!(warrior.growth_rate, None);
        assert_eq!(warrior.promotion_limit, 3);
        assert_eq!(data.unit_by_idx(2).map(|u| u.id.as_str()), Some("warrior"));
    }

    #[test]
    fn rider_and_knight_stats_match_the_extract() {
        let data = load();
        let rider = data.unit("rider").unwrap();
        assert_eq!((rider.cost, rider.attack, rider.defence, rider.movement, rider.idx), (3, 20, 10, 2, 3));
        let knight = data.unit("knight").unwrap();
        assert_eq!((knight.cost, knight.attack, knight.defence, knight.movement, knight.health), (8, 35, 10, 3, 100));
    }

    #[test]
    fn refuses_unit_ids_missing_from_the_extract() {
        let data = load();
        for id in ["ship", "battleship", "boat", "navalon"] {
            assert!(data.unit(id).is_none(), "{id} is not in GameLogicData28");
        }
    }

    #[test]
    fn tribe_tech_improvement_and_diplomacy_match_the_extract() {
        let data = load();
        let xinxi = data.tribe("xinxi").unwrap();
        assert_eq!(xinxi.idx, 13);
        assert_eq!(xinxi.starting_stars, Some(7));
        assert_eq!(xinxi.starting_unit, "warrior");
        assert_eq!(xinxi.starting_tech, ["basic", "climbing"]);
        assert_eq!(xinxi.skins, ["Ninja"]);
        assert_eq!(xinxi.terrain_modifier["mountain"], 1.5);
        assert_eq!(xinxi.resource_modifier["metal"], 1.5);
        assert_eq!(data.tribe_by_idx(7).unwrap().id, "imperius");

        let elyrion = data.tribe("elyrion").unwrap();
        assert_eq!(elyrion.tech_overrides["hunting"], "forestmagic");
        assert_eq!(elyrion.unit_overrides["giant"], "dragonegg");

        let rituals = data.tech("rituals").unwrap();
        assert_eq!(rituals.cost, 2);
        assert_eq!(rituals.idx, 49);

        let basic = data.tech("basic").unwrap();
        assert_eq!(basic.cost, 0);
        assert_eq!(basic.idx, 0);
        assert_eq!(basic.movement_unlocks["forest"], 2);
        assert!(basic.unit_unlocks.iter().any(|id| id == "warrior"));

        let farm = data.improvement("farm").unwrap();
        assert_eq!(farm.cost, Some(5));
        assert_eq!(farm.idx, 5);
        assert_eq!(farm.rewards.len(), 1);
        assert_eq!(farm.rewards[0].population, Some(2));
        assert_eq!(farm.rewards[0].currency, None);
        assert_eq!(farm.terrain_requirements[0].resource.as_deref(), Some("crop"));
        assert_eq!(data.improvement("clearforest").unwrap().cost, None);

        assert_eq!(data.terrain("water").unwrap().idx, 1);
        assert_eq!(data.resource("crop").unwrap().resource_terrain_requirements, ["field"]);
        let diplomacy = data.diplomacy.as_ref().expect("v28 diplomacy");
        assert_eq!(diplomacy.embassy_cost, 5);
        assert_eq!(diplomacy.embassy_income, 2);
        assert_eq!(diplomacy.embasy_growth_time, 3);
        assert_eq!(diplomacy.embassy_max_level, 3);
        assert_eq!(diplomacy.embassy_upgrade_cost, 20);
        assert_eq!(diplomacy.dividend_income, 2);
        assert_eq!(diplomacy.embassy_tech_price_multiplier, 0.5);
    }

    #[test]
    fn keeps_utf8_text_from_the_extract() {
        let data = load();
        assert!(data.tribe("yadakk").unwrap().language.contains('ü'));
        assert!(data.skin("magma").unwrap().language.contains('\u{25ca}'));
        assert_eq!(data.skin("swamp").unwrap().color, Some(6786096));
        assert_eq!(data.skin("magma").unwrap().color, None);
    }

    #[test]
    fn bad_json_returns_an_error() {
        assert!(load_game_logic_json("").is_err());
        assert!(load_game_logic_json("{").is_err());
        assert!(load_game_logic_json("[]").is_err());
        // A second comma is not a trailing comma and is refused.
        assert!(load_game_logic_json("{\"a\":1,,}").is_err());
        assert!(load_game_logic_json("[1,,]").is_err());
    }

    /// Fingerprint of the rules of each version, taken from the game's own files when
    /// the rules files were first written. A change to any rule changes one of these.
    const FINGERPRINTS: [(i32, u64); 20] = [
        (9, 0x57e8ce40b3c5d331),
        (10, 0xdf33cc5a1a1c08a6),
        (11, 0x031653bcc52143c2),
        (12, 0x3c0825f85686f765),
        (13, 0xe6ec42335ecdaf19),
        (14, 0x733c5f879bb56d67),
        (15, 0x883bbccd544f75fc),
        (16, 0x391d6d0dbb53ccb7),
        (17, 0xe01ecc98d09845f9),
        (18, 0xd8b1004db9751d4f),
        (19, 0xe58c9d40937e44ef),
        (20, 0x14de3df234db3a6c),
        (21, 0xbd9f4c22a7eb52f3),
        (22, 0x8d99c0eb80042cdb),
        (23, 0x72b67fff7a8e62d4),
        (24, 0x83546f77bfa8d50c),
        (25, 0xbc3c6443aa1e88b8),
        (26, 0xe0ccfbff602e22ee),
        (27, 0xa28e4bfe16ec4c2b),
        (28, 0xfa4128429236bea7),
    ];

    #[test]
    fn each_logic_data_version_loads_and_matches_its_fingerprint() {
        for (version, fingerprint) in FINGERPRINTS {
            let data = load_game_logic_data(version).unwrap_or_else(|err| panic!("rules v{version}: {err}"));
            assert!(!data.units.is_empty());
            assert_eq!(
                rules::fingerprint(&data),
                fingerprint,
                "rules v{version} changed (fingerprint {:016x})",
                rules::fingerprint(&data)
            );
        }
        let embedded = load_embedded_game_logic().expect("v28");
        let selected = load_game_logic_data(28).expect("v28");
        assert_eq!(selected, embedded);
        let warrior = selected.unit("warrior").expect("warrior");
        assert_eq!(warrior.cost, 2);
        assert_eq!(warrior.health, 100);
        assert_eq!(warrior.attack, 20);
        assert!(load_game_logic_data(8).is_err());
        assert!(load_game_logic_data(29).is_err());
        assert!(load_game_logic_data(-1).is_err());
        assert!(load_game_logic_data(i32::MIN).is_err());
    }

    #[test]
    fn rules_files_are_in_canonical_form() {
        // The files are what the converter writes, so a hand edit that is not
        // re-rendered (`convert_gamedata` or `rules::render_*`) shows up here.
        let mut newer: Option<(i32, GameLogicData)> = None;
        for (version, text) in rules::EMBEDDED_FILES.iter().rev() {
            let data = load_game_logic_data(*version).expect("rules");
            let want = match &newer {
                None => rules::render_base(&data, *version),
                Some((newer_version, newer_data)) => rules::render_change(&data, *version, newer_data, *newer_version),
            };
            assert_eq!(&want, text, "rules v{version} is not in canonical form");
            newer = Some((*version, data));
        }
    }

    #[test]
    fn a_directory_of_rules_files_loads_like_the_built_in_ones() {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("data/rules");
        for version in 9..=28 {
            assert_eq!(
                load_game_logic_data_from_dir(&dir, version).expect("dir"),
                load_game_logic_data(version).expect("built in"),
                "v{version}"
            );
        }
        assert_eq!(load_game_logic_from_dir(&dir, 123).expect("game 123"), load_game_logic(123).expect("game 123"));
        assert!(load_game_logic_data_from_dir(&dir, 8).is_err());
        assert!(load_game_logic_data_from_dir(&dir.join("missing"), 28).is_err());
    }

    fn mini(version: i32, body: &str) -> String {
        format!("{{\"format\": \"polytopia-rules-1\", \"version\": {version}, {body}}}")
    }

    fn mini_base() -> String {
        mini(
            28,
            r#""units": {"a": {"idx": 1, "cost": 2, "health": 3, "defence": 4, "movement": 5, "weapon": 6, "range": 7, "attack": 8, "promotion_limit": 0}},
            "tribes": {}, "techs": {}, "improvements": {}, "terrains": {}, "resources": {}, "tasks": {}"#,
        )
    }

    #[test]
    fn change_files_merge_into_the_version_they_name() {
        let files = vec![
            (28, mini_base()),
            (
                27,
                mini(
                    27,
                    r#""from": 28, "changes": {"units": {"a": {"cost": 9, "unit_abilities": ["x"]}, "b": {"idx": 2, "cost": 1, "health": 1, "defence": 1, "movement": 1, "weapon": 0, "range": 1, "attack": 1, "promotion_limit": 0}}, "skins": {"s": {"language": "l"}}}"#,
                ),
            ),
            (
                26,
                mini(
                    26,
                    r#""from": 27, "changes": {"units": {"a": null, "b": {"unit_abilities": null}}, "skins": null}"#,
                ),
            ),
        ];
        let at = |v| rules::load(rules::Source::Files(&files), v).expect("load");
        assert_eq!(at(28).units["a"].cost, 2);
        let v27 = at(27);
        assert_eq!(v27.units["a"].cost, 9);
        assert_eq!(v27.units["a"].health, 3);
        assert_eq!(v27.units["a"].unit_abilities, ["x"]);
        assert_eq!(v27.units["b"].idx, 2);
        assert_eq!(v27.skins["s"].language, "l");
        let v26 = at(26);
        assert!(!v26.units.contains_key("a"));
        assert!(v26.skins.is_empty());
        assert!(v26.units["b"].unit_abilities.is_empty());
        assert!(v26.diplomacy.is_none());
    }

    #[test]
    fn bad_rules_files_are_refused() {
        let load = |files: Vec<(i32, String)>, v| rules::load(rules::Source::Files(&files), v);
        assert!(load(vec![(28, mini_base())], 28).is_ok());
        // The version inside must be the file's version.
        assert!(load(vec![(27, mini_base())], 27).is_err());
        // Unknown section, unknown field, unknown format.
        assert!(load(vec![(28, mini_base().replace("\"tasks\": {}", "\"tasks\": {}, \"x\": {}"))], 28).is_err());
        assert!(load(vec![(28, mini_base().replace("\"cost\"", "\"price\""))], 28).is_err());
        assert!(load(vec![(28, mini_base().replace("rules-1", "rules-2"))], 28).is_err());
        // A section the loader needs.
        assert!(load(vec![(28, mini_base().replace("\"tasks\": {}", "\"skins\": {}"))], 28).is_err());
        // Duplicate idx.
        let twin = mini_base().replace(
            "\"a\": {",
            "\"z\": {\"idx\": 1, \"cost\": 2, \"health\": 3, \"defence\": 4, \"movement\": 5, \"weapon\": 6, \"range\": 7, \"attack\": 8, \"promotion_limit\": 0}, \"a\": {",
        );
        assert!(load(vec![(28, twin)], 28).is_err());
        // A change file whose base is missing, that links to itself, or that links in a loop.
        let change = |v, from| mini(v, &format!("\"from\": {from}, \"changes\": {{}}"));
        assert!(load(vec![(27, change(27, 28))], 27).is_err());
        assert!(load(vec![(27, change(27, 27))], 27).is_err());
        assert!(load(vec![(26, change(26, 27)), (27, change(27, 26))], 26).is_err());
        // A change file may not carry sections of its own.
        let loose = mini(27, r#""from": 28, "units": {}"#);
        assert!(load(vec![(28, mini_base()), (27, loose)], 27).is_err());
    }

    #[test]
    fn load_game_logic_uses_the_version_ladder() {
        assert_eq!(load_game_logic(123).expect("game 123"), load_embedded_game_logic().expect("embedded"));
        assert_eq!(load_game_logic(51).expect("game 51"), load_game_logic_data(9).expect("data 9"));
        assert_eq!(load_game_logic(80).expect("game 80"), load_game_logic_data(11).expect("data 11"));
    }

    #[test]
    fn older_files_omit_sections_without_inventing_stats() {
        let v9 = load_game_logic_data(9).expect("v9");
        assert!(v9.diplomacy.is_none());
        assert!(v9.skins.is_empty());
        let v10 = load_game_logic_data(10).expect("v10");
        assert!(v10.skins.is_empty());
        assert_eq!(v10.diplomacy.as_ref().unwrap().dividend_income, 2);
        let v19 = load_game_logic_data(19).expect("v19");
        assert!(v19.skins.is_empty());
        assert!(v19.diplomacy.is_some());
        let v20 = load_game_logic_data(20).expect("v20");
        assert_eq!(v20.skins.len(), 1);
    }
}
