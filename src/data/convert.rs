//! Conversion between the game's own `GameLogicData` JSON and the engine's rules
//! files (see [`super::rules`]), with the round-trip check between the two.
//!
//! This is tooling: `examples/convert_gamedata.rs` and `tests/rules_roundtrip.rs`
//! use it. Nothing here is needed to load rules at run time.
//!
//! The check for each version 9..=28:
//! 1. the game JSON, read by [`load_game_logic_json`], gives the same `GameLogicData`
//!    as the rules files the converter writes for it, and as the built-in files;
//! 2. the converter's files are byte for byte the built-in files;
//! 3. the game JSON written back from the loaded rules ([`game_json_text`]) equals the
//!    original after both are put in one canonical form ([`canonical_game_json`]).

use std::collections::BTreeMap;
use std::fmt::Debug;
use std::path::Path;

use super::json_parse::{self, Value};
use super::rules::{self, section, Rec, Source};
use super::*;

/// File name of a version in the game's own layout.
pub fn game_file_name(version: i32) -> String {
    format!("gamedata-v{version}.json")
}

/// Read the game's data files for every supported version (`gamedata-v9.json` .. `gamedata-v28.json`) from a directory.
pub fn read_game_files(dir: &Path) -> Result<Vec<(i32, String)>, String> {
    (rules::OLDEST_VERSION..=rules::BASE_VERSION)
        .map(|version| {
            let path = dir.join(game_file_name(version));
            std::fs::read_to_string(&path)
                .map(|text| (version, text))
                .map_err(|err| format!("{}: {err}", path.display()))
        })
        .collect()
}

/// The rules files for every version: the full file for the newest and a change
/// file for each older one, as `(version, text)` in descending version order.
pub fn rules_files(game_files: &[(i32, String)]) -> Result<Vec<(i32, String)>, DataError> {
    let mut loaded = BTreeMap::new();
    for version in rules::OLDEST_VERSION..=rules::BASE_VERSION {
        let (_, text) = game_files
            .iter()
            .find(|(v, _)| *v == version)
            .ok_or_else(|| DataError::msg(format!("no game file for version {version}")))?;
        let data =
            load_game_logic_json(text).map_err(|err| DataError::msg(format!("{}: {err}", game_file_name(version))))?;
        loaded.insert(version, data);
    }
    let mut files = vec![(rules::BASE_VERSION, rules::render_base(&loaded[&rules::BASE_VERSION], rules::BASE_VERSION))];
    for version in (rules::OLDEST_VERSION..rules::BASE_VERSION).rev() {
        files.push((version, rules::render_change(&loaded[&version], version, &loaded[&(version + 1)], version + 1)));
    }
    Ok(files)
}

/// The game's own layout for loaded rules, one record per object, for reading.
pub fn game_json_text(data: &GameLogicData) -> String {
    rules::render(&Value::Obj(game_value(data)), 2)
}

/// Any text in the game's layout, reduced to a canonical single line: keys sorted,
/// whole numbers written as integers, and the entries the reader treats as absent
/// (`null`, empty lists and objects, `"hidden": false`) dropped.
pub fn canonical_game_json(text: &str) -> Result<String, DataError> {
    let value = json_parse::parse(text)?;
    Ok(rules::flat(&canonical(&value)))
}

fn canonical(value: &Value) -> Value {
    match value {
        Value::Float(n) if n.fract() == 0.0 && n.abs() < 1e15 => Value::Int(*n as i64),
        Value::Arr(items) => Value::Arr(items.iter().map(canonical).collect()),
        Value::Obj(items) => {
            let mut kept: Vec<(String, Value)> = items
                .iter()
                .map(|(k, v)| (k.clone(), canonical(v)))
                .filter(|(k, v)| match v {
                    Value::Null => false,
                    Value::Arr(a) => !a.is_empty(),
                    Value::Obj(o) => !o.is_empty(),
                    Value::Bool(false) => k != "hidden",
                    _ => true,
                })
                .collect();
            kept.sort_by(|a, b| a.0.cmp(&b.0));
            Value::Obj(kept)
        }
        other => other.clone(),
    }
}

/// Run the checks in the module docs on every version. `Ok` has one report line per
/// version; `Err` names the first thing that differs.
pub fn check_round_trip(game_files: &[(i32, String)]) -> Result<Vec<String>, String> {
    let files = rules_files(game_files).map_err(|err| err.to_string())?;
    let mut report = Vec::new();
    for version in rules::OLDEST_VERSION..=rules::BASE_VERSION {
        let text = &game_files
            .iter()
            .find(|(v, _)| *v == version)
            .ok_or_else(|| format!("no game file for version {version}"))?
            .1;
        let original = load_game_logic_json(text).map_err(|err| format!("v{version}: {err}"))?;
        let converted =
            rules::load(Source::Files(&files), version).map_err(|err| format!("v{version}: converted files: {err}"))?;
        let embedded =
            rules::load(Source::Embedded, version).map_err(|err| format!("v{version}: built-in files: {err}"))?;
        if let Some(what) = first_difference(&original, &converted) {
            return Err(format!("v{version}: converted files differ from the game file: {what}"));
        }
        if let Some(what) = first_difference(&original, &embedded) {
            return Err(format!("v{version}: built-in files differ from the game file: {what}"));
        }
        let converted_text = &files.iter().find(|(v, _)| *v == version).map(|(_, t)| t);
        let built_in_text = rules::EMBEDDED_FILES.iter().find(|(v, _)| *v == version).map(|(_, t)| *t);
        if converted_text.map(String::as_str) != built_in_text {
            return Err(format!("v{version}: built-in file is not the converter's output"));
        }
        let wanted = canonical_game_json(text).map_err(|err| format!("v{version}: {err}"))?;
        let regenerated =
            canonical_game_json(&game_json_text(&embedded)).map_err(|err| format!("v{version}: regenerated: {err}"))?;
        if wanted != regenerated {
            let at = wanted.bytes().zip(regenerated.bytes()).take_while(|(a, b)| a == b).count();
            return Err(format!("v{version}: regenerated game JSON differs at byte {at} of the canonical form"));
        }
        report.push(format!(
            "v{version}: ok  units {} tribes {} techs {} improvements {}  rules file {} bytes  game file {} bytes  fingerprint {:016x}",
            original.units.len(),
            original.tribes.len(),
            original.tech.len(),
            original.improvements.len(),
            built_in_text.map_or(0, str::len),
            text.len(),
            rules::fingerprint(&original),
        ));
    }
    Ok(report)
}

/// The first record (or section) in which two rule sets differ, `None` when equal.
pub fn first_difference(a: &GameLogicData, b: &GameLogicData) -> Option<String> {
    fn maps<T: PartialEq + Debug>(name: &str, a: &BTreeMap<String, T>, b: &BTreeMap<String, T>) -> Option<String> {
        for (id, left) in a {
            match b.get(id) {
                None => return Some(format!("{name}.{id} only in the first")),
                Some(right) if left != right => {
                    return Some(format!("{name}.{id}: {left:?} against {right:?}"));
                }
                Some(_) => {}
            }
        }
        b.keys().find(|id| !a.contains_key(*id)).map(|id| format!("{name}.{id} only in the second"))
    }
    maps("units", &a.units, &b.units)
        .or_else(|| maps("tribes", &a.tribes, &b.tribes))
        .or_else(|| maps("terrains", &a.terrains, &b.terrains))
        .or_else(|| maps("resources", &a.resources, &b.resources))
        .or_else(|| maps("tasks", &a.tasks, &b.tasks))
        .or_else(|| maps("improvements", &a.improvements, &b.improvements))
        .or_else(|| maps("tech", &a.tech, &b.tech))
        .or_else(|| maps("skins", &a.skins, &b.skins))
        .or_else(|| (a.diplomacy != b.diplomacy).then(|| "diplomacy".to_string()))
}

// The game's own layout (camelCase keys, `tribe_data` ... sections).

fn game_value(data: &GameLogicData) -> Vec<(String, Value)> {
    let mut out = vec![
        ("tribeData".to_string(), section(&data.tribes, |t| t.idx, game_tribe)),
        ("techData".to_string(), section(&data.tech, |t| t.idx, game_tech)),
        ("unitData".to_string(), section(&data.units, |u| u.idx, game_unit)),
        ("improvementData".to_string(), section(&data.improvements, |i| i.idx, game_improvement)),
        ("terrainData".to_string(), section(&data.terrains, |t| t.idx, game_terrain)),
        ("resourceData".to_string(), section(&data.resources, |r| r.idx, game_resource)),
        ("taskData".to_string(), section(&data.tasks, |t| t.idx, game_task)),
    ];
    if !data.skins.is_empty() {
        out.push(("skinData".to_string(), section(&data.skins, |_| 0, game_skin)));
    }
    if let Some(d) = &data.diplomacy {
        let mut r = Rec::default();
        r.int("embassyCost", d.embassy_cost);
        r.int("embassyIncome", d.embassy_income);
        r.int("embasyGrowthTime", d.embasy_growth_time);
        r.int("embassyMaxLevel", d.embassy_max_level);
        r.int("embassyUpgradeCost", d.embassy_upgrade_cost);
        r.int("dividendIncome", d.dividend_income);
        r.float("embassyTechPriceMultiplier", d.embassy_tech_price_multiplier);
        out.push(("diplomacyData".to_string(), r.done()));
    }
    out
}

fn game_unit(u: &UnitData) -> Value {
    let mut r = Rec::default();
    r.int("idx", u.idx);
    r.flag("hidden", u.hidden);
    r.int("cost", u.cost);
    r.int("health", u.health);
    r.int("defence", u.defence);
    r.int("movement", u.movement);
    r.int("weapon", u.weapon);
    r.int("range", u.range);
    r.int("attack", u.attack);
    r.texts("unitAbilities", &u.unit_abilities);
    r.texts("movementTerrain", &u.movement_terrain);
    r.opt_text("upgradesFrom", &u.upgrades_from);
    r.opt_text("promotesTo", &u.promotes_to);
    r.opt_int("growthRate", u.growth_rate);
    r.int("promotionLimit", u.promotion_limit);
    r.done()
}

fn game_tribe(t: &TribeData) -> Value {
    let mut r = Rec::default();
    r.int("idx", t.idx);
    r.int("color", t.color);
    r.text("language", &t.language);
    r.opt_int("startingStars", t.starting_stars);
    r.int("bonus", t.bonus);
    r.text("startingUnit", &t.starting_unit);
    r.int("category", t.category);
    r.int("priceTier", t.price_tier);
    r.texts("skins", &t.skins);
    r.float_map("terrainModifier", &t.terrain_modifier);
    r.float_map("resourceModifier", &t.resource_modifier);
    r.texts("startingTech", &t.starting_tech);
    r.texts("startingResource", &t.starting_resource);
    r.texts("tribeAbilities", &t.tribe_abilities);
    r.text_map("techOverrides", &t.tech_overrides);
    r.text_map("unitOverrides", &t.unit_overrides);
    r.text_map("improvementOverrides", &t.improvement_overrides);
    r.int("style", t.style);
    r.int("climate", t.climate);
    r.done()
}

fn game_tech(t: &TechData) -> Value {
    let mut r = Rec::default();
    r.int("idx", t.idx);
    r.int("cost", t.cost);
    r.texts("techUnlocks", &t.tech_unlocks);
    r.texts("improvementUnlocks", &t.improvement_unlocks);
    r.texts("unitUnlocks", &t.unit_unlocks);
    r.texts("abilityUnlocks", &t.ability_unlocks);
    r.texts("taskUnlocks", &t.task_unlocks);
    r.int_map("movementUnlocks", &t.movement_unlocks);
    r.int_map("defenceBonusUnlocks", &t.defence_bonus_unlocks);
    r.done()
}

fn game_improvement(i: &ImprovementData) -> Value {
    let mut r = Rec::default();
    r.int("idx", i.idx);
    r.flag("hidden", i.hidden);
    r.opt_int("cost", i.cost);
    r.opt_int("work", i.work);
    r.opt_int("borderSize", i.border_size);
    r.opt_int("maxLevel", i.max_level);
    r.texts("improvementAbilities", &i.improvement_abilities);
    r.records(
        "creates",
        i.creates
            .iter()
            .map(|c| {
                let mut r = Rec::default();
                r.opt_text("terrain", &c.terrain);
                r.opt_text("resource", &c.resource);
                r.opt_text("unit", &c.unit);
                r.opt_text("effect", &c.effect);
                r.done()
            })
            .collect(),
    );
    r.records(
        "rewards",
        i.rewards
            .iter()
            .map(|w| {
                let mut r = Rec::default();
                r.opt_int("population", w.population);
                r.opt_int("currency", w.currency);
                r.opt_int("score", w.score);
                r.done()
            })
            .collect(),
    );
    r.records(
        "terrainRequirements",
        i.terrain_requirements
            .iter()
            .map(|q| {
                let mut r = Rec::default();
                r.opt_text("terrain", &q.terrain);
                r.opt_text("resource", &q.resource);
                r.done()
            })
            .collect(),
    );
    r.records(
        "adjacencyRequirements",
        i.adjacency_requirements
            .iter()
            .map(|q| {
                let mut r = Rec::default();
                r.opt_text("terrain", &q.terrain);
                r.opt_text("improvement", &q.improvement);
                r.opt_text("resource", &q.resource);
                r.opt_text("notImprovement", &q.not_improvement);
                r.done()
            })
            .collect(),
    );
    r.records(
        "adjacencyImprovements",
        i.adjacency_improvements
            .iter()
            .map(|a| {
                let mut r = Rec::default();
                r.opt_text("improvement", &a.improvement);
                r.opt_text("resources", &a.resources);
                r.done()
            })
            .collect(),
    );
    r.texts("routes", &i.routes);
    r.opt_int("range", i.range);
    r.opt_int("growthRate", i.growth_rate);
    r.records(
        "growthRewards",
        i.growth_rewards
            .iter()
            .map(|g| {
                let mut r = Rec::default();
                r.opt_int("score", g.score);
                r.opt_int("population", g.population);
                r.done()
            })
            .collect(),
    );
    r.opt_flag("shouldSuggestUnlock", i.should_suggest_unlock);
    r.done()
}

fn game_terrain(t: &TerrainData) -> Value {
    let mut r = Rec::default();
    r.int("idx", t.idx);
    r.done()
}

fn game_resource(res: &ResourceData) -> Value {
    let mut r = Rec::default();
    r.int("idx", res.idx);
    r.texts("resourceTerrainRequirements", &res.resource_terrain_requirements);
    r.done()
}

fn game_task(t: &TaskData) -> Value {
    let mut r = Rec::default();
    r.int("idx", t.idx);
    r.texts("improvementUnlocks", &t.improvement_unlocks);
    r.done()
}

fn game_skin(s: &SkinData) -> Value {
    let mut r = Rec::default();
    r.opt_int("color", s.color);
    r.text("language", &s.language);
    r.done()
}
