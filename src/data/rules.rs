//! The engine's own rules data format.
//!
//! The rules for logic-data versions 9 through 28 live in `data/rules/` as one
//! full file for the newest version (`v28.json`) and one change file per older
//! version (`v27.json` .. `v9.json`). A change file names the version it is
//! applied to (`"from"`) and lists only what differs, as a JSON merge patch
//! (RFC 7386): objects merge, `null` deletes a key, anything else replaces.
//! Loading version N follows the `from` links up to the full file, then
//! applies the change files back down to N.
//!
//! ```text
//! {
//!   "format": "polytopia-rules-1",
//!   "version": 28,
//!   "units": {
//!     "warrior": {"idx": 2, "cost": 2, "health": 100, ..., "unit_abilities": ["dash", "fortify", "land"]},
//!   },
//!   "tribes": { ... }, "techs": { ... }, "improvements": { ... },
//!   "terrains": { ... }, "resources": { ... }, "tasks": { ... },
//!   "skins": { ... }, "diplomacy": { ... }
//! }
//! ```
//!
//! Key names are the snake_case field names of the structs in [`super`].
//! Fields that hold their default (false, empty list or map, `None`) are
//! omitted. A change file looks like this:
//!
//! ```text
//! {
//!   "format": "polytopia-rules-1",
//!   "version": 27,
//!   "from": 28,
//!   "changes": {
//!     "units": {
//!       "warrior": {"cost": 3},
//!       "ship": null
//!     }
//!   }
//! }
//! ```
//!
//! The same loader serves the built-in files ([`Source::Embedded`]) and a
//! directory of files with the same names ([`Source::Dir`]).

use std::borrow::Cow;
use std::collections::BTreeMap;
use std::path::Path;

use super::json_parse::{self, Value};
use super::*;

/// Value of the `format` key in every rules file.
pub const FORMAT: &str = "polytopia-rules-1";
/// Version stored as a full file. Every other version is a change file.
pub const BASE_VERSION: i32 = GAME_LOGIC_DATA_VERSION;
/// Oldest logic-data version the engine keeps.
pub const OLDEST_VERSION: i32 = 9;

const SECTIONS: [&str; 9] =
    ["units", "tribes", "techs", "improvements", "terrains", "resources", "tasks", "skins", "diplomacy"];
const REQUIRED_SECTIONS: [&str; 7] = ["units", "tribes", "techs", "improvements", "terrains", "resources", "tasks"];
const MAX_CHAIN: usize = 64;
const LINE_WIDTH: usize = 300;

macro_rules! embedded_files {
    ($($version:literal),*) => {
        [$(($version, include_str!(concat!("../../data/rules/v", $version, ".json")))),*]
    };
}

/// The built-in rules files, `(logic-data version, file text)`.
pub static EMBEDDED_FILES: [(i32, &str); 20] =
    embedded_files!(9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20, 21, 22, 23, 24, 25, 26, 27, 28);

/// Where [`load`] reads the rules files from.
#[derive(Debug, Clone, Copy)]
pub enum Source<'a> {
    /// The files compiled into the engine.
    Embedded,
    /// A directory holding `v9.json` .. `v28.json` (or any set of versions that links up).
    Dir(&'a Path),
    /// Files held in memory, `(version, text)`.
    Files(&'a [(i32, String)]),
}

/// File name of a version inside a rules directory.
pub fn file_name(version: i32) -> String {
    format!("v{version}.json")
}

/// Load the rules of one logic-data version from `source`.
pub fn load(source: Source<'_>, version: i32) -> Result<GameLogicData, DataError> {
    let sections = resolve(source, version, 0)?;
    decode(&sections)
}

fn file_text<'a>(source: Source<'a>, version: i32) -> Result<Cow<'a, str>, DataError> {
    let missing =
        || DataError::msg(format!("logic data version {version} is not in {OLDEST_VERSION}..={BASE_VERSION}"));
    match source {
        Source::Embedded => {
            EMBEDDED_FILES.iter().find(|(v, _)| *v == version).map(|(_, text)| Cow::Borrowed(*text)).ok_or_else(missing)
        }
        Source::Files(files) => {
            files.iter().find(|(v, _)| *v == version).map(|(_, text)| Cow::Borrowed(text.as_str())).ok_or_else(missing)
        }
        Source::Dir(dir) => {
            let path = dir.join(file_name(version));
            std::fs::read_to_string(&path)
                .map(Cow::Owned)
                .map_err(|err| DataError::msg(format!("{}: {err}", path.display())))
        }
    }
}

/// The section map (`units`, `tribes`, ...) of a version, change files applied.
fn resolve(source: Source<'_>, version: i32, depth: usize) -> Result<Vec<(String, Value)>, DataError> {
    let ctx = format!("rules v{version}");
    if depth > MAX_CHAIN {
        return Err(DataError::msg(format!("{ctx}: change files link in a loop")));
    }
    let text = file_text(source, version)?;
    let value = json_parse::parse(&text).map_err(|err| DataError::msg(format!("{ctx}: {err}")))?;
    let root = json_parse::as_object(&value, &ctx)?;
    let format = json_parse::as_string(need(root, "format", &ctx)?, &format!("{ctx}.format"))?;
    if format != FORMAT {
        return Err(DataError::msg(format!("{ctx}: unknown format {format}")));
    }
    let declared = req_i32(root, "version", &ctx)?;
    if declared != version {
        return Err(DataError::msg(format!("{ctx}: file says version {declared}")));
    }
    if !root.iter().any(|(k, _)| k == "from") {
        let mut allowed = vec!["format", "version"];
        allowed.extend(SECTIONS);
        allow(root, &allowed, &ctx)?;
        return Ok(root.iter().filter(|(k, _)| k != "format" && k != "version").cloned().collect());
    }
    allow(root, &["format", "version", "from", "changes"], &ctx)?;
    let from = req_i32(root, "from", &ctx)?;
    if from == version {
        return Err(DataError::msg(format!("{ctx}: changes are applied to itself")));
    }
    let mut merged = Value::Obj(resolve(source, from, depth + 1)?);
    merge_patch(&mut merged, need(root, "changes", &ctx)?);
    match merged {
        Value::Obj(sections) => Ok(sections),
        _ => Err(DataError::msg(format!("{ctx}: changes must be an object"))),
    }
}

/// RFC 7386 merge patch: objects merge, `null` deletes, anything else replaces.
fn merge_patch(target: &mut Value, patch: &Value) {
    let Value::Obj(patch_items) = patch else {
        *target = patch.clone();
        return;
    };
    if !matches!(target, Value::Obj(_)) {
        *target = Value::Obj(Vec::new());
    }
    let Value::Obj(items) = target else {
        return;
    };
    for (key, change) in patch_items {
        let pos = items.iter().position(|(k, _)| k == key);
        if matches!(change, Value::Null) {
            if let Some(pos) = pos {
                items.remove(pos);
            }
            continue;
        }
        match pos {
            Some(pos) => merge_patch(&mut items[pos].1, change),
            None => {
                let mut fresh = Value::Null;
                merge_patch(&mut fresh, change);
                items.push((key.clone(), fresh));
            }
        }
    }
}

/// The merge patch that turns `old` into `new`, or `None` when they are equal.
fn diff(old: &Value, new: &Value) -> Option<Value> {
    if old == new {
        return None;
    }
    let (Value::Obj(old_items), Value::Obj(new_items)) = (old, new) else {
        return Some(new.clone());
    };
    let mut out = Vec::new();
    for (key, new_value) in new_items {
        match old_items.iter().find(|(k, _)| k == key) {
            None => out.push((key.clone(), new_value.clone())),
            Some((_, old_value)) => {
                if let Some(change) = diff(old_value, new_value) {
                    out.push((key.clone(), change));
                }
            }
        }
    }
    for (key, _) in old_items {
        if !new_items.iter().any(|(k, _)| k == key) {
            out.push((key.clone(), Value::Null));
        }
    }
    Some(Value::Obj(out))
}

// Rendering.

/// The text of the full file for `version`.
pub fn render_base(data: &GameLogicData, version: i32) -> String {
    let mut root = vec![
        ("format".to_string(), Value::Str(FORMAT.to_string())),
        ("version".to_string(), Value::Int(i64::from(version))),
    ];
    root.extend(encode(data));
    render(&Value::Obj(root), 2)
}

/// The text of the change file that turns `newer` (at `newer_version`) into `older`.
pub fn render_change(older: &GameLogicData, older_version: i32, newer: &GameLogicData, newer_version: i32) -> String {
    let changes = diff(&Value::Obj(encode(newer)), &Value::Obj(encode(older))).unwrap_or(Value::Obj(Vec::new()));
    let root = vec![
        ("format".to_string(), Value::Str(FORMAT.to_string())),
        ("version".to_string(), Value::Int(i64::from(older_version))),
        ("from".to_string(), Value::Int(i64::from(newer_version))),
        ("changes".to_string(), changes),
    ];
    render(&Value::Obj(root), 3)
}

/// Text of a value: objects shallower than `expand` get a line per entry, deeper
/// ones stay on one line while they fit in [`LINE_WIDTH`].
pub(super) fn render(value: &Value, expand: usize) -> String {
    let mut out = String::new();
    write_value(&mut out, value, 0, 0, expand);
    out.push('\n');
    out
}

fn write_value(out: &mut String, value: &Value, indent: usize, depth: usize, expand: usize) {
    let (open, close, count) = match value {
        Value::Obj(items) => ('{', '}', items.len()),
        Value::Arr(items) => ('[', ']', items.len()),
        _ => {
            out.push_str(&flat(value));
            return;
        }
    };
    if count == 0 {
        out.push(open);
        out.push(close);
        return;
    }
    let one_line = flat(value);
    let column = out.rsplit('\n').next().map_or(0, |line| line.chars().count());
    if depth >= expand && column + one_line.chars().count() < LINE_WIDTH {
        out.push_str(&one_line);
        return;
    }
    out.push(open);
    out.push('\n');
    let pad = |out: &mut String, level: usize| out.extend(std::iter::repeat_n("  ", level));
    match value {
        Value::Obj(items) => {
            for (i, (key, item)) in items.iter().enumerate() {
                pad(out, indent + 1);
                out.push_str(&json_string(key));
                out.push_str(": ");
                write_value(out, item, indent + 1, depth + 1, expand);
                out.push_str(if i + 1 < count { ",\n" } else { "\n" });
            }
        }
        Value::Arr(items) => {
            for (i, item) in items.iter().enumerate() {
                pad(out, indent + 1);
                write_value(out, item, indent + 1, depth + 1, expand);
                out.push_str(if i + 1 < count { ",\n" } else { "\n" });
            }
        }
        _ => {}
    }
    pad(out, indent);
    out.push(close);
}

pub(super) fn flat(value: &Value) -> String {
    match value {
        Value::Null => "null".to_string(),
        Value::Bool(b) => b.to_string(),
        Value::Int(n) => n.to_string(),
        Value::Float(n) => format!("{n:?}"),
        Value::Str(s) => json_string(s),
        Value::Arr(items) => {
            let parts: Vec<String> = items.iter().map(flat).collect();
            format!("[{}]", parts.join(", "))
        }
        Value::Obj(items) => {
            let parts: Vec<String> = items.iter().map(|(k, v)| format!("{}: {}", json_string(k), flat(v))).collect();
            format!("{{{}}}", parts.join(", "))
        }
    }
}

pub(super) fn json_string(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('"');
    for ch in s.chars() {
        match ch {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

/// FNV-1a over the full-file text of the rules (written without a version), a short
/// stand-in for "these are exactly the same rules".
pub fn fingerprint(data: &GameLogicData) -> u64 {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in render_base(data, 0).bytes() {
        hash ^= u64::from(byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    hash
}

// Encoding: `GameLogicData` to the section map.

/// Builds a record object, leaving out fields that hold their default.
#[derive(Default)]
pub(super) struct Rec(Vec<(String, Value)>);

impl Rec {
    pub(super) fn int(&mut self, key: &str, value: i32) {
        self.0.push((key.to_string(), Value::Int(i64::from(value))));
    }
    pub(super) fn text(&mut self, key: &str, value: &str) {
        self.0.push((key.to_string(), Value::Str(value.to_string())));
    }
    pub(super) fn float(&mut self, key: &str, value: f64) {
        self.0.push((key.to_string(), Value::Float(value)));
    }
    pub(super) fn opt_int(&mut self, key: &str, value: Option<i32>) {
        if let Some(value) = value {
            self.int(key, value);
        }
    }
    pub(super) fn opt_text(&mut self, key: &str, value: &Option<String>) {
        if let Some(value) = value {
            self.text(key, value);
        }
    }
    pub(super) fn opt_flag(&mut self, key: &str, value: Option<bool>) {
        if let Some(value) = value {
            self.0.push((key.to_string(), Value::Bool(value)));
        }
    }
    pub(super) fn flag(&mut self, key: &str, value: bool) {
        if value {
            self.0.push((key.to_string(), Value::Bool(true)));
        }
    }
    pub(super) fn texts(&mut self, key: &str, values: &[String]) {
        if !values.is_empty() {
            let items = values.iter().map(|v| Value::Str(v.clone())).collect();
            self.0.push((key.to_string(), Value::Arr(items)));
        }
    }
    pub(super) fn text_map(&mut self, key: &str, values: &BTreeMap<String, String>) {
        if !values.is_empty() {
            let items = values.iter().map(|(k, v)| (k.clone(), Value::Str(v.clone()))).collect();
            self.0.push((key.to_string(), Value::Obj(items)));
        }
    }
    pub(super) fn int_map(&mut self, key: &str, values: &BTreeMap<String, i32>) {
        if !values.is_empty() {
            let items = values.iter().map(|(k, v)| (k.clone(), Value::Int(i64::from(*v)))).collect();
            self.0.push((key.to_string(), Value::Obj(items)));
        }
    }
    pub(super) fn float_map(&mut self, key: &str, values: &BTreeMap<String, f64>) {
        if !values.is_empty() {
            let items = values.iter().map(|(k, v)| (k.clone(), Value::Float(*v))).collect();
            self.0.push((key.to_string(), Value::Obj(items)));
        }
    }
    pub(super) fn records(&mut self, key: &str, values: Vec<Value>) {
        if !values.is_empty() {
            self.0.push((key.to_string(), Value::Arr(values)));
        }
    }
    pub(super) fn done(self) -> Value {
        Value::Obj(self.0)
    }
}

/// A section: records ordered by `idx` (then id), or by id when there is no `idx`.
pub(super) fn section<T>(items: &BTreeMap<String, T>, idx: impl Fn(&T) -> i32, record: impl Fn(&T) -> Value) -> Value {
    let mut ordered: Vec<(&String, &T)> = items.iter().collect();
    ordered.sort_by_key(|(id, item)| (idx(item), (*id).clone()));
    Value::Obj(ordered.into_iter().map(|(id, item)| (id.clone(), record(item))).collect())
}

fn encode(data: &GameLogicData) -> Vec<(String, Value)> {
    let mut out = vec![
        ("units".to_string(), section(&data.units, |u| u.idx, encode_unit)),
        ("tribes".to_string(), section(&data.tribes, |t| t.idx, encode_tribe)),
        ("techs".to_string(), section(&data.tech, |t| t.idx, encode_tech)),
        ("improvements".to_string(), section(&data.improvements, |i| i.idx, encode_improvement)),
        ("terrains".to_string(), section(&data.terrains, |t| t.idx, encode_terrain)),
        ("resources".to_string(), section(&data.resources, |r| r.idx, encode_resource)),
        ("tasks".to_string(), section(&data.tasks, |t| t.idx, encode_task)),
    ];
    if !data.skins.is_empty() {
        out.push(("skins".to_string(), section(&data.skins, |_| 0, encode_skin)));
    }
    if let Some(diplomacy) = &data.diplomacy {
        out.push(("diplomacy".to_string(), encode_diplomacy(diplomacy)));
    }
    out
}

fn encode_unit(u: &UnitData) -> Value {
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
    r.texts("unit_abilities", &u.unit_abilities);
    r.texts("movement_terrain", &u.movement_terrain);
    r.opt_text("upgrades_from", &u.upgrades_from);
    r.opt_text("promotes_to", &u.promotes_to);
    r.opt_int("growth_rate", u.growth_rate);
    r.int("promotion_limit", u.promotion_limit);
    r.done()
}

fn encode_tribe(t: &TribeData) -> Value {
    let mut r = Rec::default();
    r.int("idx", t.idx);
    r.int("color", t.color);
    r.text("language", &t.language);
    r.opt_int("starting_stars", t.starting_stars);
    r.int("bonus", t.bonus);
    r.text("starting_unit", &t.starting_unit);
    r.int("category", t.category);
    r.int("price_tier", t.price_tier);
    r.texts("skins", &t.skins);
    r.float_map("terrain_modifier", &t.terrain_modifier);
    r.float_map("resource_modifier", &t.resource_modifier);
    r.texts("starting_tech", &t.starting_tech);
    r.texts("starting_resource", &t.starting_resource);
    r.texts("tribe_abilities", &t.tribe_abilities);
    r.text_map("tech_overrides", &t.tech_overrides);
    r.text_map("unit_overrides", &t.unit_overrides);
    r.text_map("improvement_overrides", &t.improvement_overrides);
    r.int("style", t.style);
    r.int("climate", t.climate);
    r.done()
}

fn encode_tech(t: &TechData) -> Value {
    let mut r = Rec::default();
    r.int("idx", t.idx);
    r.int("cost", t.cost);
    r.texts("tech_unlocks", &t.tech_unlocks);
    r.texts("improvement_unlocks", &t.improvement_unlocks);
    r.texts("unit_unlocks", &t.unit_unlocks);
    r.texts("ability_unlocks", &t.ability_unlocks);
    r.texts("task_unlocks", &t.task_unlocks);
    r.int_map("movement_unlocks", &t.movement_unlocks);
    r.int_map("defence_bonus_unlocks", &t.defence_bonus_unlocks);
    r.done()
}

fn encode_improvement(i: &ImprovementData) -> Value {
    let mut r = Rec::default();
    r.int("idx", i.idx);
    r.flag("hidden", i.hidden);
    r.opt_int("cost", i.cost);
    r.opt_int("work", i.work);
    r.opt_int("border_size", i.border_size);
    r.opt_int("max_level", i.max_level);
    r.texts("improvement_abilities", &i.improvement_abilities);
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
        "terrain_requirements",
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
        "adjacency_requirements",
        i.adjacency_requirements
            .iter()
            .map(|q| {
                let mut r = Rec::default();
                r.opt_text("terrain", &q.terrain);
                r.opt_text("improvement", &q.improvement);
                r.opt_text("resource", &q.resource);
                r.opt_text("not_improvement", &q.not_improvement);
                r.done()
            })
            .collect(),
    );
    r.records(
        "adjacency_improvements",
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
    r.opt_int("growth_rate", i.growth_rate);
    r.records(
        "growth_rewards",
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
    r.opt_flag("should_suggest_unlock", i.should_suggest_unlock);
    r.done()
}

fn encode_terrain(t: &TerrainData) -> Value {
    let mut r = Rec::default();
    r.int("idx", t.idx);
    r.done()
}

fn encode_resource(res: &ResourceData) -> Value {
    let mut r = Rec::default();
    r.int("idx", res.idx);
    r.texts("resource_terrain_requirements", &res.resource_terrain_requirements);
    r.done()
}

fn encode_task(t: &TaskData) -> Value {
    let mut r = Rec::default();
    r.int("idx", t.idx);
    r.texts("improvement_unlocks", &t.improvement_unlocks);
    r.done()
}

fn encode_skin(s: &SkinData) -> Value {
    let mut r = Rec::default();
    r.opt_int("color", s.color);
    r.text("language", &s.language);
    r.done()
}

fn encode_diplomacy(d: &DiplomacyData) -> Value {
    let mut r = Rec::default();
    r.int("embassy_cost", d.embassy_cost);
    r.int("embassy_income", d.embassy_income);
    r.int("embassy_growth_time", d.embasy_growth_time);
    r.int("embassy_max_level", d.embassy_max_level);
    r.int("embassy_upgrade_cost", d.embassy_upgrade_cost);
    r.int("dividend_income", d.dividend_income);
    r.float("embassy_tech_price_multiplier", d.embassy_tech_price_multiplier);
    r.done()
}

// Decoding: the section map to `GameLogicData`.

type Fields = [(String, Value)];

fn decode(root: &Fields) -> Result<GameLogicData, DataError> {
    allow(root, &SECTIONS, "rules")?;
    for key in REQUIRED_SECTIONS {
        if !root.iter().any(|(k, _)| k == key) {
            return Err(DataError::msg(format!("missing {key}")));
        }
    }
    let skins =
        if root.iter().any(|(k, _)| k == "skins") { parse_map(root, "skins", decode_skin)? } else { BTreeMap::new() };
    let diplomacy = match root.iter().find(|(k, _)| k == "diplomacy") {
        Some((_, value)) => Some(decode_diplomacy(value)?),
        None => None,
    };
    Ok(GameLogicData {
        units: parse_indexed(root, "units", decode_unit)?,
        tribes: parse_indexed(root, "tribes", decode_tribe)?,
        terrains: parse_indexed(root, "terrains", decode_terrain)?,
        resources: parse_indexed(root, "resources", decode_resource)?,
        tasks: parse_indexed(root, "tasks", decode_task)?,
        improvements: parse_indexed(root, "improvements", decode_improvement)?,
        tech: parse_indexed(root, "techs", decode_tech)?,
        skins,
        diplomacy,
    })
}

fn decode_unit(id: &str, f: &Fields) -> Result<UnitData, DataError> {
    let ctx = format!("units.{id}");
    allow(
        f,
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
            "unit_abilities",
            "movement_terrain",
            "upgrades_from",
            "promotes_to",
            "growth_rate",
            "promotion_limit",
        ],
        &ctx,
    )?;
    Ok(UnitData {
        id: id.to_string(),
        idx: req_i32(f, "idx", &ctx)?,
        hidden: bool_or(f, "hidden", false, &ctx)?,
        cost: req_i32(f, "cost", &ctx)?,
        health: req_i32(f, "health", &ctx)?,
        defence: req_i32(f, "defence", &ctx)?,
        movement: req_i32(f, "movement", &ctx)?,
        weapon: req_i32(f, "weapon", &ctx)?,
        range: req_i32(f, "range", &ctx)?,
        attack: req_i32(f, "attack", &ctx)?,
        unit_abilities: opt_strings(f, "unit_abilities", &ctx)?,
        movement_terrain: opt_strings(f, "movement_terrain", &ctx)?,
        upgrades_from: opt_string(f, "upgrades_from", &ctx)?,
        promotes_to: opt_string(f, "promotes_to", &ctx)?,
        growth_rate: opt_i32(f, "growth_rate", &ctx)?,
        promotion_limit: req_i32(f, "promotion_limit", &ctx)?,
    })
}

fn decode_tribe(id: &str, f: &Fields) -> Result<TribeData, DataError> {
    let ctx = format!("tribes.{id}");
    allow(
        f,
        &[
            "idx",
            "color",
            "language",
            "starting_stars",
            "bonus",
            "starting_unit",
            "category",
            "price_tier",
            "skins",
            "terrain_modifier",
            "resource_modifier",
            "starting_tech",
            "starting_resource",
            "tribe_abilities",
            "tech_overrides",
            "unit_overrides",
            "improvement_overrides",
            "style",
            "climate",
        ],
        &ctx,
    )?;
    Ok(TribeData {
        id: id.to_string(),
        idx: req_i32(f, "idx", &ctx)?,
        color: req_i32(f, "color", &ctx)?,
        language: req_string(f, "language", &ctx)?,
        starting_stars: opt_i32(f, "starting_stars", &ctx)?,
        bonus: req_i32(f, "bonus", &ctx)?,
        starting_unit: req_string(f, "starting_unit", &ctx)?,
        category: req_i32(f, "category", &ctx)?,
        price_tier: req_i32(f, "price_tier", &ctx)?,
        skins: opt_strings(f, "skins", &ctx)?,
        terrain_modifier: opt_f64_map(f, "terrain_modifier", &ctx)?,
        resource_modifier: opt_f64_map(f, "resource_modifier", &ctx)?,
        starting_tech: opt_strings(f, "starting_tech", &ctx)?,
        starting_resource: opt_strings(f, "starting_resource", &ctx)?,
        tribe_abilities: opt_strings(f, "tribe_abilities", &ctx)?,
        tech_overrides: opt_string_map(f, "tech_overrides", &ctx)?,
        unit_overrides: opt_string_map(f, "unit_overrides", &ctx)?,
        improvement_overrides: opt_string_map(f, "improvement_overrides", &ctx)?,
        style: req_i32(f, "style", &ctx)?,
        climate: req_i32(f, "climate", &ctx)?,
    })
}

fn decode_tech(id: &str, f: &Fields) -> Result<TechData, DataError> {
    let ctx = format!("techs.{id}");
    allow(
        f,
        &[
            "idx",
            "cost",
            "tech_unlocks",
            "improvement_unlocks",
            "unit_unlocks",
            "ability_unlocks",
            "task_unlocks",
            "movement_unlocks",
            "defence_bonus_unlocks",
        ],
        &ctx,
    )?;
    Ok(TechData {
        id: id.to_string(),
        idx: req_i32(f, "idx", &ctx)?,
        cost: req_i32(f, "cost", &ctx)?,
        tech_unlocks: opt_strings(f, "tech_unlocks", &ctx)?,
        improvement_unlocks: opt_strings(f, "improvement_unlocks", &ctx)?,
        unit_unlocks: opt_strings(f, "unit_unlocks", &ctx)?,
        ability_unlocks: opt_strings(f, "ability_unlocks", &ctx)?,
        task_unlocks: opt_strings(f, "task_unlocks", &ctx)?,
        movement_unlocks: opt_i32_map(f, "movement_unlocks", &ctx)?,
        defence_bonus_unlocks: opt_i32_map(f, "defence_bonus_unlocks", &ctx)?,
    })
}

fn decode_improvement(id: &str, f: &Fields) -> Result<ImprovementData, DataError> {
    let ctx = format!("improvements.{id}");
    allow(
        f,
        &[
            "idx",
            "hidden",
            "cost",
            "work",
            "border_size",
            "max_level",
            "improvement_abilities",
            "creates",
            "rewards",
            "terrain_requirements",
            "adjacency_requirements",
            "adjacency_improvements",
            "routes",
            "range",
            "growth_rate",
            "growth_rewards",
            "should_suggest_unlock",
        ],
        &ctx,
    )?;
    Ok(ImprovementData {
        id: id.to_string(),
        idx: req_i32(f, "idx", &ctx)?,
        hidden: bool_or(f, "hidden", false, &ctx)?,
        cost: opt_i32(f, "cost", &ctx)?,
        work: opt_i32(f, "work", &ctx)?,
        border_size: opt_i32(f, "border_size", &ctx)?,
        max_level: opt_i32(f, "max_level", &ctx)?,
        improvement_abilities: opt_strings(f, "improvement_abilities", &ctx)?,
        creates: opt_objects(f, "creates", &ctx, |o, c| {
            allow(o, &["terrain", "resource", "unit", "effect"], c)?;
            Ok(Creates {
                terrain: opt_string(o, "terrain", c)?,
                resource: opt_string(o, "resource", c)?,
                unit: opt_string(o, "unit", c)?,
                effect: opt_string(o, "effect", c)?,
            })
        })?,
        rewards: opt_objects(f, "rewards", &ctx, |o, c| {
            allow(o, &["population", "currency", "score"], c)?;
            Ok(Rewards {
                population: opt_i32(o, "population", c)?,
                currency: opt_i32(o, "currency", c)?,
                score: opt_i32(o, "score", c)?,
            })
        })?,
        terrain_requirements: opt_objects(f, "terrain_requirements", &ctx, |o, c| {
            allow(o, &["terrain", "resource"], c)?;
            Ok(TerrainRequirement { terrain: opt_string(o, "terrain", c)?, resource: opt_string(o, "resource", c)? })
        })?,
        adjacency_requirements: opt_objects(f, "adjacency_requirements", &ctx, |o, c| {
            allow(o, &["terrain", "improvement", "resource", "not_improvement"], c)?;
            Ok(AdjacencyRequirement {
                terrain: opt_string(o, "terrain", c)?,
                improvement: opt_string(o, "improvement", c)?,
                resource: opt_string(o, "resource", c)?,
                not_improvement: opt_string(o, "not_improvement", c)?,
            })
        })?,
        adjacency_improvements: opt_objects(f, "adjacency_improvements", &ctx, |o, c| {
            allow(o, &["improvement", "resources"], c)?;
            Ok(AdjacencyImprovement {
                improvement: opt_string(o, "improvement", c)?,
                resources: opt_string(o, "resources", c)?,
            })
        })?,
        routes: opt_strings(f, "routes", &ctx)?,
        range: opt_i32(f, "range", &ctx)?,
        growth_rate: opt_i32(f, "growth_rate", &ctx)?,
        growth_rewards: opt_objects(f, "growth_rewards", &ctx, |o, c| {
            allow(o, &["score", "population"], c)?;
            Ok(GrowthRewards { score: opt_i32(o, "score", c)?, population: opt_i32(o, "population", c)? })
        })?,
        should_suggest_unlock: opt_bool(f, "should_suggest_unlock", &ctx)?,
    })
}

fn decode_terrain(id: &str, f: &Fields) -> Result<TerrainData, DataError> {
    let ctx = format!("terrains.{id}");
    allow(f, &["idx"], &ctx)?;
    Ok(TerrainData { id: id.to_string(), idx: req_i32(f, "idx", &ctx)? })
}

fn decode_resource(id: &str, f: &Fields) -> Result<ResourceData, DataError> {
    let ctx = format!("resources.{id}");
    allow(f, &["idx", "resource_terrain_requirements"], &ctx)?;
    Ok(ResourceData {
        id: id.to_string(),
        idx: req_i32(f, "idx", &ctx)?,
        resource_terrain_requirements: opt_strings(f, "resource_terrain_requirements", &ctx)?,
    })
}

fn decode_task(id: &str, f: &Fields) -> Result<TaskData, DataError> {
    let ctx = format!("tasks.{id}");
    allow(f, &["idx", "improvement_unlocks"], &ctx)?;
    Ok(TaskData {
        id: id.to_string(),
        idx: req_i32(f, "idx", &ctx)?,
        improvement_unlocks: opt_strings(f, "improvement_unlocks", &ctx)?,
    })
}

fn decode_skin(id: &str, f: &Fields) -> Result<SkinData, DataError> {
    let ctx = format!("skins.{id}");
    allow(f, &["color", "language"], &ctx)?;
    Ok(SkinData { id: id.to_string(), color: opt_i32(f, "color", &ctx)?, language: req_string(f, "language", &ctx)? })
}

fn decode_diplomacy(value: &Value) -> Result<DiplomacyData, DataError> {
    let f = json_parse::as_object(value, "diplomacy")?;
    allow(
        f,
        &[
            "embassy_cost",
            "embassy_income",
            "embassy_growth_time",
            "embassy_max_level",
            "embassy_upgrade_cost",
            "dividend_income",
            "embassy_tech_price_multiplier",
        ],
        "diplomacy",
    )?;
    Ok(DiplomacyData {
        embassy_cost: req_i32(f, "embassy_cost", "diplomacy")?,
        embassy_income: req_i32(f, "embassy_income", "diplomacy")?,
        embasy_growth_time: req_i32(f, "embassy_growth_time", "diplomacy")?,
        embassy_max_level: req_i32(f, "embassy_max_level", "diplomacy")?,
        embassy_upgrade_cost: req_i32(f, "embassy_upgrade_cost", "diplomacy")?,
        dividend_income: req_i32(f, "dividend_income", "diplomacy")?,
        embassy_tech_price_multiplier: req_f64(f, "embassy_tech_price_multiplier", "diplomacy")?,
    })
}
