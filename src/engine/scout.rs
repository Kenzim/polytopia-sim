//! The scout move (the Explorer reward): the next-target choice with the fog-distance map.
//!
//! Every step builds an 11x11 window around the scout, marks unexplored (fog) cells as sources of a breadth-first
//! distance map (step cost 100, cheaper next to fog), penalises tiles the scout has already visited and walks to the
//! neighbour with the smallest distance (ties broken with the game hash and the action's seed).
//! Ported from the retail routines that build the lookup grid, the fog distance map, the neighbour queue update,
//! the fog-tile score, the backtracking penalty and the next-target choice.

use super::*;
use crate::hash::HashFunction;

const RADIUS: i32 = 5;
const WIDTH: i32 = 2 * RADIUS + 1;

const F_EXISTS: u32 = 0x01;
const F_PASS: u32 = 0x02;
const F_FOG: u32 = 0x08;
const F_BRIDGE: u32 = 0x10;
const F_IMPR47: u32 = 0x20;
const F_MOUNTAIN: u32 = 0x40;
const F_CLOSED: u32 = 0x80;

/// Longest scout path: the start tile and the most moves a scout makes.
pub(super) const SCOUT_PATH_MAX: usize = 16;

/// A scout move chain still to walk: the moves left (the step in progress included), the seed of the next step and
/// the tiles visited so far.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ScoutRun {
    pub player: u8,
    pub moves: u8,
    pub seed: u32,
    /// Number of tiles of `path` in use.
    pub len: u8,
    /// Where in the explore order of the last tile of `path` the walk stopped, 0 when that step is complete.
    pub area: u8,
    pub path: [Wc; SCOUT_PATH_MAX],
}

impl Engine {
    /// Number of moves a scout makes (depends on the version).
    pub fn scout_moves(&self) -> i32 {
        if let Ok(v) = crate::env_var!("SCOUT_MOVES") {
            return v.parse().unwrap_or(12);
        }
        if self.s.version < 0x78 {
            15
        } else {
            12
        }
    }

    /// The lookup grid around the scout.
    fn scout_lookup(&self, player: u8, center: Wc) -> Vec<u32> {
        let n = (WIDTH * WIDTH) as usize;
        let mut lookup = vec![0u32; n];
        let allowed = self.unlocked_movement_terrain(player);
        let (ox, oy) = (center.x - RADIUS, center.y - RADIUS);
        for (i, slot) in lookup.iter_mut().enumerate() {
            let c = Wc { x: ox + (i as i32 % WIDTH), y: oy + (i as i32 / WIDTH) };
            let Some(tile) = self.tile(c) else { continue };
            let explored = self.is_explored(player, tile.index as usize);
            let city = tile.improvement == improvement::CITY;
            let mut bridge = false;
            let mut is47 = false;
            if tile.improvement != 0 {
                bridge = self.t.has_improvement_ability(tile.improvement, improvement_ability::BRIDGE);
                is47 = tile.improvement == 47;
            }
            let mut pass = city;
            if !bridge && !city {
                pass = allowed.contains(&tile.terrain);
            }
            let mut f = F_EXISTS;
            if bridge {
                f |= F_BRIDGE;
            }
            if !explored {
                f |= F_FOG;
            }
            if pass {
                f |= F_PASS | 0x04;
            }
            if is47 {
                f |= F_IMPR47;
            }
            if tile.terrain == terrain::MOUNTAIN && explored {
                f |= F_MOUNTAIN;
            }
            *slot = f;
        }
        lookup
    }

    /// Fog score of a cell: the unexplored cells around it within `radius`.
    fn fog_score(lookup: &[u32], idx: i32, radius: i32) -> i32 {
        let width = WIDTH;
        let (x, y) = (idx % width, idx / width);
        let x0 = (x - radius).max(0);
        let x1 = (x + radius).min(width - 1);
        let y0 = (y - radius).max(0);
        let mut y1 = y + radius;
        if y1 * width >= lookup.len() as i32 {
            y1 = lookup.len() as i32 / width - 1;
        }
        let mut count = 0;
        let mut fog47 = false;
        if !(y0 > y1 || x0 > x1) {
            for yy in y0..=y1 {
                for xx in x0..=x1 {
                    let v = lookup[(yy * width + xx) as usize];
                    count += ((v >> 3) & 1) as i32;
                    if (!v & (F_FOG | F_IMPR47)) == 0 {
                        fog47 = true;
                    }
                }
            }
        }
        let count = count.min(4);
        if radius == 2 && count == 0 && !fog47 {
            return 9999;
        }
        let base = -10 * count;
        if fog47 {
            base + 40
        } else {
            base + 73
        }
    }

    /// The fog distance map over the lookup grid.
    fn scout_fog_distances(lookup: &mut [u32]) -> Vec<i32> {
        let n = lookup.len();
        let w = WIDTH;
        let mut dist: Vec<i32> = lookup.iter().map(|f| if f & F_PASS == 0 { 9999 } else { 1000 }).collect();
        let mut queue: Vec<i32> = Vec::new();
        for i in 0..n {
            if lookup[i] & F_FOG != 0 {
                dist[i] = 0;
                queue.push(i as i32);
            }
        }
        // Explored mountains see two tiles: they count as sources scored by the fog around them.
        for i in 0..n {
            if lookup[i] & (F_MOUNTAIN | F_FOG | F_PASS) != (F_MOUNTAIN | F_PASS) {
                continue;
            }
            let score = Self::fog_score(lookup, i as i32, 2);
            if score == 9999 {
                continue;
            }
            dist[i] = score + 100;
            queue.push(i as i32);
            lookup[i] |= F_CLOSED;
        }
        let mut q = 0usize;
        while q < queue.len() && q != 0x1f2 {
            let cur = queue[q];
            q += 1;
            let x = cur % w;
            let right = if x < w - 1 { 1 } else { 9999 };
            let left = if x <= 0 { -9999 } else { -1 };
            let new_cost = dist[cur as usize] + 100;
            let down = cur + w;
            let up = cur - w;
            for nb in [down, up, cur + right, cur + left, down + left, down + right, up + left, up + right] {
                Self::scout_try_update(&mut dist, lookup, &mut queue, nb, new_cost);
            }
        }
        dist
    }

    /// Update a neighbour's distance and enqueue it.
    fn scout_try_update(dist: &mut [i32], lookup: &[u32], queue: &mut Vec<i32>, nb: i32, mut cost: i32) {
        if nb < 0 || nb as usize >= dist.len() {
            return;
        }
        let i = nb as usize;
        if dist[i] <= cost {
            return;
        }
        let f = lookup[i];
        if f & F_CLOSED != 0 {
            return;
        }
        // A bridge tile defers to the path finder; everything else needs the existence and passable bits.
        let allowed = f & F_BRIDGE != 0 || (f & 3) == 3;
        if !allowed {
            return;
        }
        if cost == 100 && f & F_FOG == 0 {
            cost = Self::fog_score(lookup, nb, 1) + 100;
        }
        dist[i] = cost;
        queue.push(nb);
    }

    /// Penalise tiles the scout has already visited.
    fn scout_backtrack(dist: &mut [i32], center: Wc, path: &[Wc]) {
        let (ox, oy) = (center.x - RADIUS, center.y - RADIUS);
        let mut penalty = 1;
        let mut k = 0;
        for (i, p) in path.iter().enumerate() {
            let _ = i;
            if p.x >= ox && p.x < ox + WIDTH && p.y >= oy && p.y < oy + WIDTH {
                let idx = ((p.y - oy) * WIDTH + (p.x - ox)) as usize;
                if idx < dist.len() {
                    dist[idx] += penalty;
                }
            }
            if path.len() < 9 {
                penalty += 1;
            } else {
                k += 1;
                penalty += (!k) & 1;
            }
        }
    }

    /// The tile the scout walks to next.
    pub fn scout_next_target(&self, player: u8, from: Wc, path: &[Wc], seed: u32) -> Option<Wc> {
        let mut lookup = self.scout_lookup(player, from);
        let mut dist = Self::scout_fog_distances(&mut lookup);
        Self::scout_backtrack(&mut dist, from, path);
        const OFFSETS: [(i32, i32); 8] = [(-1, -1), (-1, 0), (-1, 1), (0, -1), (0, 1), (1, -1), (1, 0), (1, 1)];
        let mut best = i32::MAX;
        let mut ties: Vec<usize> = Vec::new();
        for (k, (dx, dy)) in OFFSETS.iter().enumerate() {
            let idx = (60 + dx + dy * WIDTH) as usize;
            let d = dist[idx];
            if d < best {
                best = d;
                ties.clear();
                ties.push(k);
            } else if d == best {
                ties.push(k);
            }
        }
        if best > 0x270e || ties.is_empty() {
            return None;
        }
        let pick =
            if ties.len() == 1 { 0 } else { self.random_hash().range1(0, ties.len() as i32, seed as i32)? as usize };
        let (dx, dy) = OFFSETS[ties[pick]];
        let c = Wc { x: from.x + dx, y: from.y + dy };
        self.tile(c)?;
        Some(c)
    }

    /// Run a whole scout move chain from `from`: up to `scout_moves()` steps that reveal the map. A step that
    /// raises a city reward (the lighthouse corner's population change can level the capital up) leaves the rest of
    /// the chain on the action stack until the reward is answered.
    pub fn run_scout(&mut self, player: u8, from: Wc, seed: u32) {
        let mut run =
            ScoutRun { player, moves: self.scout_moves() as u8, seed, len: 1, area: 0, path: [from; SCOUT_PATH_MAX] };
        self.continue_scout(&mut run);
    }

    /// Walk the remaining steps of `run`. A city reward raised by a step's explore actions holds what is left of
    /// the step and the chain until the reward is answered.
    pub(super) fn continue_scout(&mut self, run: &mut ScoutRun) {
        while run.moves >= 1 {
            let mut start = run.area as usize;
            if start == 0 {
                if self.reward_pending() {
                    self.hold(Deferred::Scout(*run));
                    return;
                }
                let at = run.path[run.len as usize - 1];
                let Some(target) = self.scout_next_target(run.player, at, &run.path[..run.len as usize], run.seed)
                else {
                    return;
                };
                if run.len as usize == SCOUT_PATH_MAX {
                    return;
                }
                run.path[run.len as usize] = target;
                run.len += 1;
                if self.dbg("SCOUT_DEBUG") || crate::env_var!("SCOUT_DEBUG").is_ok() {
                    eprintln!(
                        "SCOUT p{} step from ({},{}) -> ({},{}) moves_left {} seed {}",
                        run.player, at.x, at.y, target.x, target.y, run.moves, run.seed
                    );
                }
                if run.moves >= 2 {
                    run.seed = self.random_hash().hash_i32s(&[run.seed as i32]);
                }
                start = 0;
            }
            let at = run.path[run.len as usize - 1];
            if let Some(rest) = self.explore_area(run.player, at, 1, start, true) {
                run.area = rest as u8;
                self.hold(Deferred::Scout(*run));
                return;
            }
            run.area = 0;
            run.moves -= 1;
        }
    }
}
