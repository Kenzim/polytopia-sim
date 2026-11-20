//! Lookup tables derived once from [`GameLogicData`] so the hot paths work on integers.

use crate::data::{GameLogicData, ImprovementData, UnitData};
use std::collections::BTreeMap;

pub mod unit_ability {
    pub const DASH: i32 = 1;
    pub const ESCAPE: i32 = 2;
    pub const SCOUT: i32 = 3;
    pub const SNEAK: i32 = 4;
    pub const HIDE: i32 = 5;
    pub const PERSIST: i32 = 7;
    pub const CONVERT: i32 = 8;
    pub const HEAL: i32 = 9;
    pub const SWIM: i32 = 10;
    pub const CARRY: i32 = 11;
    pub const GROW: i32 = 12;
    pub const FLY: i32 = 13;
    pub const SPLASH: i32 = 14;
    pub const NAVIGATE: i32 = 16;
    pub const CRUSH: i32 = 17;
    pub const FREEZE: i32 = 18;
    pub const FREEZE_AREA: i32 = 19;
    pub const AUTO_FREEZE: i32 = 20;
    pub const SKATE: i32 = 21;
    pub const FORTIFY: i32 = 22;
    pub const CREEP: i32 = 23;
    pub const BOOST: i32 = 24;
    pub const INDEPENDENT: i32 = 25;
    pub const POISON: i32 = 26;
    pub const EAT: i32 = 27;
    pub const UNIQUE: i32 = 29;
    pub const EXPLODE: i32 = 30;
    pub const SURPRISE: i32 = 31;
    pub const INFILTRATE: i32 = 35;
    pub const DETECT: i32 = 36;
    pub const STIFF: i32 = 38;
    pub const PROTECT: i32 = 39;
    pub const STOMP: i32 = 40;
    pub const AUTO_HEAL: i32 = 41;
    pub const STATIC: i32 = 42;
    pub const TENTACLES: i32 = 44;
    pub const AUTO_FLOOD: i32 = 45;
    pub const DRENCH: i32 = 46;
    pub const AMPHIBIOUS: i32 = 47;
    pub const WATER: i32 = 48;
    pub const LAND: i32 = 49;
    pub const ALGAE: i32 = 51;
    pub const CONSUMED: i32 = 52;
    pub const SWARM: i32 = 53;
    pub const DOUBLE_ATTACK: i32 = 54;
}

pub mod improvement_ability {
    pub const UNIQUE: i32 = 1;
    pub const LIMITED: i32 = 2;
    pub const EXPAND: i32 = 3;
    pub const CONSUMED: i32 = 4;
    pub const PATINA: i32 = 5;
    pub const HEAL: i32 = 7;
    pub const ATTRACT: i32 = 10;
    pub const BRIDGE: i32 = 13;
    pub const POISON: i32 = 14;
    pub const NETWORK: i32 = 15;
    pub const EMBARK: i32 = 16;
    pub const FREELANCE: i32 = 17;
    pub const MANUAL: i32 = 18;
    pub const SLOW: i32 = 19;
    pub const PICKY: i32 = 21;
    pub const DOUBLED: i32 = 22;
    pub const FLOOD: i32 = 23;
    pub const DISCRETE: i32 = 24;
    pub const FILL: i32 = 25;
    pub const SIMPLE: i32 = 27;
}

pub mod player_ability {
    pub const DISBAND: i32 = 0;
    pub const DESTROY: i32 = 1;
    pub const LITERACY: i32 = 2;
    pub const GLIDE: i32 = 3;
    pub const DECOMPOSE: i32 = 5;
    pub const PEACE_TREATY: i32 = 6;
    pub const EMBASSY: i32 = 7;
    pub const CAPITAL_VISION: i32 = 8;
    pub const DIVIDEND: i32 = 9;
    pub const DISCIPLINE: i32 = 10;
    pub const PONTOON: i32 = 11;
    pub const TREEHOUSE: i32 = 12;
    pub const BUBBLE: i32 = 13;
    pub const MICROBES: i32 = 14;
}

pub mod terrain {
    pub const WATER: i32 = 1;
    pub const OCEAN: i32 = 2;
    pub const FIELD: i32 = 3;
    pub const MOUNTAIN: i32 = 4;
    pub const FOREST: i32 = 5;
    pub const ICE: i32 = 6;
    pub const WETLAND: i32 = 7;
    pub const MANGROVE: i32 = 8;
}

pub mod improvement {
    pub const CITY: i32 = 1;
    pub const RUIN: i32 = 2;
    pub const ROAD: i32 = 3;
    pub const PORT: i32 = 8;
    pub const POLARIS_CLIMATE: i32 = 36;
    pub const BRIDGE: i32 = 48;
    pub const ALGAE_SPAWN: i32 = 55;
}

pub fn improvement_ability_id(name: &str) -> Option<i32> {
    Some(match name {
        "unique" => 1,
        "limited" => 2,
        "expand" => 3,
        "consumed" => 4,
        "patina" => 5,
        "heal" => 7,
        "attract" => 10,
        "bridge" => 13,
        "poison" => 14,
        "network" => 15,
        "embark" => 16,
        "freelance" => 17,
        "manual" => 18,
        "slow" => 19,
        "picky" => 21,
        "doubled" => 22,
        "flood" => 23,
        "discrete" => 24,
        "fill" => 25,
        "simple" => 27,
        _ => return None,
    })
}

pub fn player_ability_id(name: &str) -> Option<i32> {
    Some(match name {
        "disband" => 0,
        "destroy" => 1,
        "literacy" => 2,
        "glide" => 3,
        "decompose" => 5,
        "peacetreaty" => 6,
        "embassy" => 7,
        "capitalvision" => 8,
        "dividend" => 9,
        "discipline" => 10,
        "pontoon" => 11,
        "treehouse" => 12,
        "bubble" => 13,
        "microbes" => 14,
        _ => return None,
    })
}

#[derive(Debug, Clone)]
pub struct UnitInfo {
    pub data: UnitData,
    pub abilities: Vec<i32>,
    /// Terrain ids from `movement_terrain`, when the unit lists any.
    pub terrain: Vec<i32>,
    /// `abilities` as a set (see [`AbilityMask`]).
    pub ability_mask: AbilityMask,
    /// Idx of every unit whose `upgrades_from` names this one, in data order (see `Engine::unlocked_upgrades_for`).
    pub upgrades: Vec<i32>,
}

#[derive(Debug, Clone)]
pub struct ImprovementInfo {
    pub data: ImprovementData,
    pub abilities: Vec<i32>,
    /// Required terrain ids (any one).
    pub terrain_req: Vec<i32>,
    /// Required resource ids (any one).
    pub resource_req: Vec<i32>,
    pub population: i32,
    pub currency: i32,
    pub score: i32,
    /// `abilities` as a set (see [`AbilityMask`]).
    pub ability_mask: AbilityMask,
    /// `data.creates` with names resolved: (resource idx, effect bit or 0, no resource named).
    pub creates_ids: Vec<(Option<i32>, u16, bool)>,
    /// `data.terrain_requirements` with names resolved, idx 0 and unknown names as `None`: (resource, terrain).
    pub terrain_req_ids: Vec<(Option<i32>, Option<i32>)>,
    /// `data.adjacency_requirements` with names resolved (absent or unknown names as `None`): (improvement, terrain,
    /// resource).
    pub adjacency_req_ids: Vec<(Option<i32>, Option<i32>, Option<i32>)>,
    /// `data.routes` through `terrain_idx`, in data order (unknown names dropped).
    pub route_terrain: Vec<i32>,
}

/// An ability id list as a bit set for ids 0..64 (all known unit and improvement abilities); a list with any other
/// id keeps answering from the list.
#[derive(Debug, Clone, Copy, Default)]
pub struct AbilityMask {
    bits: u64,
    exact: bool,
}

impl AbilityMask {
    pub fn new(list: &[i32]) -> Self {
        let exact = list.iter().all(|&a| (0..64).contains(&a));
        let bits = list.iter().filter(|&&a| (0..64).contains(&a)).fold(0u64, |m, &a| m | 1 << a);
        Self { bits, exact }
    }

    /// `list.contains(&ability)` for the list this mask was built from.
    #[inline]
    pub fn contains(&self, list: &[i32], ability: i32) -> bool {
        if self.exact {
            (0..64).contains(&ability) && self.bits & (1 << ability) != 0
        } else {
            list.contains(&ability)
        }
    }

    /// The bits when every ability of the list is in `0..64`.
    #[inline]
    pub fn exact_bits(&self) -> Option<u64> {
        self.exact.then_some(self.bits)
    }
}

impl UnitInfo {
    #[inline]
    pub fn has(&self, ability: i32) -> bool {
        self.ability_mask.contains(&self.abilities, ability)
    }
}

impl ImprovementInfo {
    #[inline]
    pub fn has(&self, ability: i32) -> bool {
        self.ability_mask.contains(&self.abilities, ability)
    }
}

/// Per-tech lists resolved to ids, independent of the tribe.
#[derive(Debug, Clone, Default)]
pub struct TechInfo {
    /// `ability_unlocks` through [`player_ability_id`].
    pub abilities: Vec<i32>,
    pub ability_mask: AbilityMask,
    /// `movement_unlocks` keys through `terrain_idx`, in key order.
    pub movement: Vec<i32>,
    /// `defence_bonus_unlocks` value for the terrain named by each terrain idx (see `Tables::terrain_name`); the
    /// last slot is for a terrain idx without a name (looked up as `""`).
    pub defence: Vec<Option<i32>>,
}

/// What a tribe's overrides make of each tech, derived from static data only.
#[derive(Debug, Clone, Default)]
pub struct TribeTech {
    /// `Engine::tech_nodes(tribe)`.
    pub nodes: Vec<crate::rules::economy::TechNode>,
    /// Per tech idx: `unit_unlocks` through `Engine::tribe_unit` (unresolved names dropped).
    pub units: Vec<Vec<i32>>,
    /// Per tech idx: `improvement_unlocks` through `Engine::tribe_improvement` (unresolved names dropped).
    pub improvements: Vec<Vec<i32>>,
    /// Per tech idx: `improvement_unlocks` through the tribe's overrides when the tribe exists, else the bare name.
    pub improvements_raw: Vec<Vec<i32>>,
    /// Every ability of a unit in `units` (what the tribe-has-unit-ability check scans for).
    pub unit_abilities: Vec<i32>,
    /// Per task kind (`kind as usize`): the task's `improvement_unlocks` through `Engine::tribe_improvement`
    /// (unresolved names dropped; empty for a kind without task data).
    pub task_improvements: Vec<Vec<i32>>,
}

impl TribeTech {
    pub fn units(&self, tech: i32) -> &[i32] {
        usize::try_from(tech).ok().and_then(|i| self.units.get(i)).map_or(&[], Vec::as_slice)
    }

    pub fn improvements(&self, tech: i32) -> &[i32] {
        usize::try_from(tech).ok().and_then(|i| self.improvements.get(i)).map_or(&[], Vec::as_slice)
    }

    pub fn improvements_raw(&self, tech: i32) -> &[i32] {
        usize::try_from(tech).ok().and_then(|i| self.improvements_raw.get(i)).map_or(&[], Vec::as_slice)
    }

    pub fn task_improvements(&self, kind: crate::TaskKind) -> &[i32] {
        self.task_improvements.get(kind as usize).map_or(&[], Vec::as_slice)
    }
}

/// Immutable tables shared by every clone of an engine.
#[derive(Debug, Clone)]
pub struct Tables {
    pub logic: GameLogicData,
    pub units: Vec<Option<UnitInfo>>,
    pub improvements: Vec<Option<ImprovementInfo>>,
    pub terrain_idx: BTreeMap<String, i32>,
    pub resource_idx: BTreeMap<String, i32>,
    pub unit_idx: BTreeMap<String, i32>,
    pub improvement_idx: BTreeMap<String, i32>,
    pub tech_idx: BTreeMap<String, i32>,
    pub tech_name: Vec<Option<String>>,
    pub tribe_name: Vec<Option<String>>,
    /// `tech(idx)` and `tribe(idx)` resolved once.
    tech_data: Vec<Option<crate::data::TechData>>,
    tribe_data: Vec<Option<crate::data::TribeData>>,
    /// Per tech idx (`None` where `tech(idx)` is).
    tech_info: Vec<Option<TechInfo>>,
    /// Per tribe idx; `tribe_tech_missing` serves every idx without tribe data beyond the table.
    tribe_tech: Vec<TribeTech>,
    tribe_tech_missing: TribeTech,
    /// The name of the first terrain (in key order) with each idx.
    terrain_names: Vec<Option<String>>,
}

impl Tables {
    pub fn new(logic: GameLogicData) -> Self {
        let terrain_idx: BTreeMap<String, i32> = logic.terrains.iter().map(|(k, v)| (k.clone(), v.idx)).collect();
        let resource_idx: BTreeMap<String, i32> = logic.resources.iter().map(|(k, v)| (k.clone(), v.idx)).collect();
        let unit_idx: BTreeMap<String, i32> = logic.units.iter().map(|(k, v)| (k.clone(), v.idx)).collect();
        let improvement_idx: BTreeMap<String, i32> =
            logic.improvements.iter().map(|(k, v)| (k.clone(), v.idx)).collect();
        let tech_idx: BTreeMap<String, i32> = logic.tech.iter().map(|(k, v)| (k.clone(), v.idx)).collect();
        let mut units: Vec<Option<UnitInfo>> = Vec::new();
        for unit in logic.units.values() {
            let slot = unit.idx as usize;
            if units.len() <= slot {
                units.resize(slot + 1, None);
            }
            let abilities: Vec<i32> =
                unit.unit_abilities.iter().filter_map(|n| crate::ai::decide::unit_ability_id(n)).collect();
            let terrain = unit.movement_terrain.iter().filter_map(|n| terrain_idx.get(n).copied()).collect();
            let ability_mask = AbilityMask::new(&abilities);
            let upgrades = logic
                .units
                .values()
                .filter(|t| t.upgrades_from.as_deref() == Some(unit.id.as_str()))
                .filter_map(|t| unit_idx.get(t.id.as_str()).copied())
                .collect();
            units[slot] = Some(UnitInfo { data: unit.clone(), abilities, terrain, ability_mask, upgrades });
        }
        let mut improvements: Vec<Option<ImprovementInfo>> = Vec::new();
        for imp in logic.improvements.values() {
            let slot = imp.idx as usize;
            if improvements.len() <= slot {
                improvements.resize(slot + 1, None);
            }
            let abilities: Vec<i32> =
                imp.improvement_abilities.iter().filter_map(|n| improvement_ability_id(n)).collect();
            let mut terrain_req = Vec::new();
            let mut resource_req = Vec::new();
            for r in &imp.terrain_requirements {
                if let Some(t) = &r.terrain {
                    if let Some(i) = terrain_idx.get(t) {
                        terrain_req.push(*i);
                    }
                }
                if let Some(t) = &r.resource {
                    if let Some(i) = resource_idx.get(t) {
                        resource_req.push(*i);
                    }
                }
            }
            let population = imp.rewards.iter().map(|r| r.population_or_default()).sum();
            let currency = imp.rewards.iter().map(|r| r.currency_or_default()).sum();
            let score = imp.rewards.iter().map(|r| r.score_or_default()).sum();
            let creates_ids = imp
                .creates
                .iter()
                .map(|c| {
                    let res = c.resource.as_ref().and_then(|n| resource_idx.get(n).copied());
                    let effect = match c.effect.as_deref() {
                        Some("algae") => crate::engine::Engine::EFFECT_ALGAE,
                        Some("flooded") => crate::engine::Engine::EFFECT_FLOODED,
                        _ => 0,
                    };
                    (res, effect, c.resource.is_none())
                })
                .collect();
            let terrain_req_ids = imp
                .terrain_requirements
                .iter()
                .map(|r| {
                    let res = r.resource.as_ref().and_then(|n| resource_idx.get(n).copied()).filter(|&x| x != 0);
                    let ter = r.terrain.as_ref().and_then(|n| terrain_idx.get(n).copied()).filter(|&x| x != 0);
                    (res, ter)
                })
                .collect();
            let adjacency_req_ids = imp
                .adjacency_requirements
                .iter()
                .map(|r| {
                    (
                        r.improvement.as_ref().and_then(|n| improvement_idx.get(n).copied()),
                        r.terrain.as_ref().and_then(|n| terrain_idx.get(n).copied()),
                        r.resource.as_ref().and_then(|n| resource_idx.get(n).copied()),
                    )
                })
                .collect();
            let ability_mask = AbilityMask::new(&abilities);
            let route_terrain = imp.routes.iter().filter_map(|n| terrain_idx.get(n).copied()).collect();
            improvements[slot] = Some(ImprovementInfo {
                data: imp.clone(),
                abilities,
                terrain_req,
                resource_req,
                population,
                currency,
                score,
                ability_mask,
                creates_ids,
                terrain_req_ids,
                adjacency_req_ids,
                route_terrain,
            });
        }
        let mut tech_name = Vec::new();
        for (name, t) in &logic.tech {
            let slot = t.idx as usize;
            if tech_name.len() <= slot {
                tech_name.resize(slot + 1, None);
            }
            tech_name[slot] = Some(name.clone());
        }
        let mut tribe_name = Vec::new();
        for (name, t) in &logic.tribes {
            let slot = t.idx as usize;
            if tribe_name.len() <= slot {
                tribe_name.resize(slot + 1, None);
            }
            tribe_name[slot] = Some(name.clone());
        }
        let tech_data = tech_name.iter().map(|n| n.as_ref().and_then(|n| logic.tech.get(n)).cloned()).collect();
        let tribe_data = tribe_name.iter().map(|n| n.as_ref().and_then(|n| logic.tribes.get(n)).cloned()).collect();
        let mut terrain_names: Vec<Option<String>> = Vec::new();
        for (name, t) in &logic.terrains {
            let Ok(slot) = usize::try_from(t.idx) else { continue };
            if terrain_names.len() <= slot {
                terrain_names.resize(slot + 1, None);
            }
            if terrain_names[slot].is_none() {
                terrain_names[slot] = Some(name.clone());
            }
        }
        let mut t = Self {
            logic,
            units,
            improvements,
            terrain_idx,
            resource_idx,
            unit_idx,
            improvement_idx,
            tech_idx,
            tech_name,
            tribe_name,
            tech_data,
            tribe_data,
            tech_info: Vec::new(),
            tribe_tech: Vec::new(),
            tribe_tech_missing: TribeTech::default(),
            terrain_names,
        };
        t.tech_info = (0..t.tech_data.len()).map(|i| t.build_tech_info(i as i32)).collect();
        t.tribe_tech = (0..t.tribe_data.len()).map(|i| t.build_tribe_tech(Some(i as i32))).collect();
        t.tribe_tech_missing = t.build_tribe_tech(None);
        t
    }

    fn build_tech_info(&self, tech: i32) -> Option<TechInfo> {
        let td = self.tech(tech)?;
        let mut defence: Vec<Option<i32>> = self
            .terrain_names
            .iter()
            .map(|n| td.defence_bonus_unlocks.get(n.as_deref().unwrap_or("")).copied())
            .collect();
        defence.push(td.defence_bonus_unlocks.get("").copied());
        let abilities: Vec<i32> = td.ability_unlocks.iter().filter_map(|n| player_ability_id(n)).collect();
        Some(TechInfo {
            ability_mask: AbilityMask::new(&abilities),
            abilities,
            movement: td.movement_unlocks.keys().filter_map(|n| self.terrain_idx.get(n).copied()).collect(),
            defence,
        })
    }

    /// `tribe` `None` builds the lists for an idx without tribe data.
    fn build_tribe_tech(&self, tribe: Option<i32>) -> TribeTech {
        let tribe_data = tribe.and_then(|i| self.tribe(i));
        let lists = |names: fn(&crate::data::TechData) -> &Vec<String>,
                     f: &dyn Fn(&str) -> Option<i32>|
         -> Vec<Vec<i32>> {
            (0..self.tech_data.len())
                .map(|i| {
                    self.tech(i as i32).map(|td| names(td).iter().filter_map(|n| f(n)).collect()).unwrap_or_default()
                })
                .collect()
        };
        let unit = |name: &str| -> Option<i32> {
            let tribe = tribe_data?;
            let n = tribe.unit_overrides.get(name).map(String::as_str).unwrap_or(name);
            self.unit_idx.get(n).copied()
        };
        let improvement = |name: &str| -> Option<i32> {
            let tribe = tribe_data?;
            let n = tribe.improvement_overrides.get(name).map(String::as_str).unwrap_or(name);
            self.improvement_idx.get(n).copied()
        };
        let improvement_raw = |name: &str| -> Option<i32> {
            let n = tribe_data.and_then(|t| t.improvement_overrides.get(name)).map(String::as_str).unwrap_or(name);
            self.improvement_idx.get(n).copied()
        };
        let units = lists(|t| &t.unit_unlocks, &unit);
        let mut unit_abilities: Vec<i32> = Vec::new();
        for &idx in units.iter().flatten() {
            for &a in self.unit(idx).map_or(&[][..], |i| i.abilities.as_slice()) {
                if !unit_abilities.contains(&a) {
                    unit_abilities.push(a);
                }
            }
        }
        TribeTech {
            nodes: self.build_tech_nodes(tribe_data),
            units,
            improvements: lists(|t| &t.improvement_unlocks, &improvement),
            improvements_raw: lists(|t| &t.improvement_unlocks, &improvement_raw),
            unit_abilities,
            task_improvements: super::tasks::TASK_KINDS
                .iter()
                .map(|&k| {
                    self.logic.task(super::tasks::kind_name(k)).map_or_else(Vec::new, |d| {
                        d.improvement_unlocks.iter().filter_map(|n| improvement(n)).collect()
                    })
                })
                .collect(),
        }
    }

    fn build_tech_nodes(&self, tribe_data: Option<&crate::data::TribeData>) -> Vec<crate::rules::economy::TechNode> {
        let overrides = tribe_data.map(|t| &t.tech_overrides);
        let map = |name: &str| -> Option<i32> {
            let n = overrides.and_then(|o| o.get(name)).map(String::as_str).unwrap_or(name);
            self.tech_idx.get(n).copied()
        };
        let mut nodes: Vec<crate::rules::economy::TechNode> = Vec::new();
        // Walk techs in idx order, applying overrides to the child names.
        let mut by_idx: Vec<(&String, &crate::data::TechData)> = self.logic.tech.iter().collect();
        by_idx.sort_by_key(|(_, t)| t.idx);
        // The retail tribe tech list is the tree in breadth-first order from `basic` (replay-verified).
        let mut bfs: Vec<i32> = Vec::new();
        if let Some(root) = by_idx.iter().find(|(n, _)| n.as_str() == "basic") {
            bfs.push(root.1.idx);
            let mut head = 0;
            while head < bfs.len() {
                let cur = bfs[head];
                head += 1;
                if let Some((_, td)) = by_idx.iter().find(|(_, t)| t.idx == cur) {
                    for n in &td.tech_unlocks {
                        if let Some(c) = map(n) {
                            if !bfs.contains(&c) {
                                bfs.push(c);
                            }
                        }
                    }
                }
            }
        }
        let mut ordered: Vec<(&String, &crate::data::TechData)> = Vec::new();
        for idx in &bfs {
            if let Some(e) = by_idx.iter().find(|(_, t)| t.idx == *idx) {
                ordered.push(*e);
            }
        }
        // Techs the tree never reaches (other tribes' replacements) are not part of this tribe's list.
        if bfs.is_empty() {
            ordered = by_idx.clone();
        }
        let by_idx = ordered;
        for (name, td) in by_idx {
            // A tech replaced by an override is not in this tribe's tree.
            if let Some(o) = overrides {
                if o.contains_key(name.as_str()) {
                    continue;
                }
            }
            nodes.push(crate::rules::economy::TechNode {
                id: td.idx,
                tech_ids: td.tech_unlocks.iter().filter_map(|n| map(n)).collect(),
                improvement_ids: td
                    .improvement_unlocks
                    .iter()
                    .filter_map(|n| {
                        let n = tribe_data
                            .and_then(|t| t.improvement_overrides.get(n.as_str()))
                            .map(String::as_str)
                            .unwrap_or(n.as_str());
                        self.improvement_idx.get(n).copied()
                    })
                    .collect(),
                unit_ids: td
                    .unit_unlocks
                    .iter()
                    .filter_map(|n| {
                        let n = tribe_data
                            .and_then(|t| t.unit_overrides.get(n.as_str()))
                            .map(String::as_str)
                            .unwrap_or(n.as_str());
                        self.unit_idx.get(n).copied()
                    })
                    .collect(),
                ability_ids: td.ability_unlocks.iter().filter_map(|n| player_ability_id(n)).collect(),
                task_ids: td.task_unlocks.iter().filter_map(|n| self.logic.task(n).map(|t| t.idx)).collect(),
                movement_terrains: td
                    .movement_unlocks
                    .keys()
                    .filter_map(|n| self.terrain_idx.get(n).copied())
                    .collect(),
            });
        }
        nodes
    }

    pub fn unit(&self, idx: i32) -> Option<&UnitInfo> {
        self.units.get(usize::try_from(idx).ok()?)?.as_ref()
    }

    pub fn improvement(&self, idx: i32) -> Option<&ImprovementInfo> {
        self.improvements.get(usize::try_from(idx).ok()?)?.as_ref()
    }

    pub fn tech(&self, idx: i32) -> Option<&crate::data::TechData> {
        self.tech_data.get(usize::try_from(idx).ok()?)?.as_ref()
    }

    pub fn tribe(&self, idx: i32) -> Option<&crate::data::TribeData> {
        self.tribe_data.get(usize::try_from(idx).ok()?)?.as_ref()
    }

    pub fn tech_info(&self, idx: i32) -> Option<&TechInfo> {
        self.tech_info.get(usize::try_from(idx).ok()?)?.as_ref()
    }

    /// The tech lists of tribe idx `tribe` (what `Engine::tech_nodes`, `tribe_unit` and `tribe_improvement` give).
    pub fn tribe_tech(&self, tribe: i32) -> &TribeTech {
        usize::try_from(tribe).ok().and_then(|i| self.tribe_tech.get(i)).unwrap_or(&self.tribe_tech_missing)
    }

    /// Defence bonus of a terrain over `techs` in order: the first tech whose
    /// `defence_bonus_unlocks` names the terrain (the first terrain key with that idx, else `""`), else the default.
    pub fn unlocked_defence_bonus(&self, techs: &[i32], terrain: i32) -> i32 {
        let named = usize::try_from(terrain).ok().filter(|&i| self.terrain_names.get(i).is_some_and(Option::is_some));
        for &tech in techs {
            let Some(info) = self.tech_info(tech) else { continue };
            let slot = named.unwrap_or(info.defence.len() - 1);
            if let Some(v) = info.defence[slot] {
                return v;
            }
        }
        crate::rules::combat::DEFAULT_UNLOCKED_DEFENCE_BONUS
    }

    pub fn has_unit_ability(&self, type_idx: i32, ability: i32) -> bool {
        self.unit(type_idx).is_some_and(|u| u.has(ability))
    }

    pub fn has_improvement_ability(&self, improvement: i32, ability: i32) -> bool {
        self.improvement(improvement).is_some_and(|i| i.has(ability))
    }
}
