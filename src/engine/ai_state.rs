//! Per-player AI scratch state (the AI state, military statistics, missions and unit targets).

use super::*;
use std::collections::BTreeMap;

/// Military statistics: summed unit strength, plus the normalised copy `update_normal` builds.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct MilitaryStats {
    pub defense: f32,
    pub attack: f32,
    pub movement: f32,
    pub range: f32,
    pub normalized: Option<[f32; 4]>,
}

impl MilitaryStats {
    /// Total: `((D + A) + M) + R`.
    pub fn total(&self) -> f32 {
        self.defense + self.attack + self.movement + self.range
    }

    /// Max: the largest of the four, floored at 0.
    pub fn max(&self) -> f32 {
        let m = self.defense.max(0.0);
        let m = self.attack.max(m);
        let m = self.movement.max(m);
        self.range.max(m)
    }

    /// Weighted total: weights 9, 10, 7, 7.
    pub fn weighted_total(&self) -> f32 {
        let a = self.attack * 10.0;
        let d = self.defense * 9.0;
        let m = self.movement * 7.0;
        let r = self.range * 7.0;
        ((a + d) + m) + r
    }

    /// Normalise: clamp at 0, then divide by the largest component.
    pub fn update_normal(&mut self) {
        let v = [self.defense.max(0.0), self.attack.max(0.0), self.movement.max(0.0), self.range.max(0.0)];
        let mut n = v;
        let mut m = v[0].max(0.0);
        m = v[1].max(m);
        m = v[2].max(m);
        m = v[3].max(m);
        if m > 0.0 {
            for x in &mut n {
                *x /= m;
            }
        }
        self.normalized = Some(n);
    }

    /// The normalised copy, zeros before `update_normal` has run.
    pub fn get_normal(&self) -> [f32; 4] {
        self.normalized.unwrap_or([0.0; 4])
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Mission {
    pub kind: i32,
    pub tile: Wc,
    pub score: f32,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct UnitTarget {
    pub unit: u32,
    pub score: f32,
}

/// The AI's memory for one player, with its summary of the map.
#[derive(Debug, Clone, Default)]
pub struct AiState {
    pub missions_for_turn: u32,
    pub missions: Vec<Mission>,
    pub hit_list: Vec<UnitTarget>,
    /// `blocked_tiles`: per-coordinate position score added by this player's own units.
    pub blocked_tiles: BTreeMap<(i32, i32), f32>,
    /// Debug: every analysis-phase position rating (unit, x, y, value).
    pub analysis_rates: Vec<(u32, i32, i32, f32)>,
    pub player_military_stats: Vec<(u8, MilitaryStats)>,
    pub enemy_cities: i32,
    pub enemy_population: i32,
    pub military_needs: MilitaryStats,
    pub rough_terrain: f32,
    pub units: Vec<u32>,
    pub empire_tiles: Vec<Wc>,
    pub city_tiles: Vec<Wc>,
    pub frozen_tile_count: i32,
    pub unit_need: f32,
    pub explored_tiles: f32,
    pub upgradeable_cities: Vec<Wc>,
}

impl AiState {
    /// Reset the scratch state.
    pub fn clear(&mut self) {
        self.missions.clear();
        self.hit_list.clear();
        self.upgradeable_cities.clear();
        self.blocked_tiles.clear();
        self.analysis_rates.clear();
        self.player_military_stats.clear();
        self.military_needs = MilitaryStats::default();
        self.enemy_cities = 0;
        self.enemy_population = 0;
        self.rough_terrain = 0.0;
        self.frozen_tile_count = 0;
        self.unit_need = 0.0;
        self.explored_tiles = 0.0;
    }
}

/// A .NET `List.Sort` with a descending-by-score comparison.
pub fn sort_desc_by_score<T, F: Fn(&T) -> f32>(items: &mut [T], score: F) {
    crate::dotnet_sort::sort_by(items, |a, b| {
        let (x, y) = (score(a), score(b));
        // `float.CompareTo(float)`: NaN sorts below everything else.
        match (y.is_nan(), x.is_nan()) {
            (true, true) => std::cmp::Ordering::Equal,
            (true, false) => std::cmp::Ordering::Less,
            (false, true) => std::cmp::Ordering::Greater,
            _ => y.partial_cmp(&x).unwrap(),
        }
    });
}

/// A .NET `List.Sort` with an arbitrary comparison, using the .NET introsort.
pub fn sort_with<T, F: FnMut(&T, &T) -> std::cmp::Ordering>(items: &mut [T], f: F) {
    crate::dotnet_sort::sort_by(items, f);
}
