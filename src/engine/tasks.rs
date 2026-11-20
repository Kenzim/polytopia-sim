//! Player tasks (Pacifist, Killer, Metropolis, Network, Wealth, Genius) and the monuments they unlock.
//!
//! The retail tasks are Pacifist (5 quiet turns), Killer (10 kills), Metropolis (a level 5 city),
//! Network (5 connected cities), Wealth (100 stars) and Genius (every tech). A completed task unlocks
//! the monument named in `task_data`. Enabling follows the call sites seen in play: techs (`task_unlocks`),
//! the first city level-up past 1, the first kill and the first city connection.

use super::*;
use crate::{PlayerTask, PlayerTasks, TaskKind};

fn kind_from_name(name: &str) -> Option<TaskKind> {
    Some(match name {
        "pacifist" => TaskKind::Pacifist,
        "genius" => TaskKind::Genius,
        "network" => TaskKind::Network,
        "wealth" => TaskKind::Wealth,
        "killer" => TaskKind::Killer,
        "metropolis" => TaskKind::Metropolis,
        "explorer" => TaskKind::Explorer,
        "explorelighthouses" => TaskKind::ExploreLighthouses,
        "convert" => TaskKind::Convert,
        _ => return None,
    })
}

/// Every task kind, in discriminant order: `TASK_KINDS[kind as usize] == kind`.
pub(super) const TASK_KINDS: [TaskKind; 10] = [
    TaskKind::None,
    TaskKind::Pacifist,
    TaskKind::Genius,
    TaskKind::Network,
    TaskKind::Wealth,
    TaskKind::Killer,
    TaskKind::Metropolis,
    TaskKind::Explorer,
    TaskKind::ExploreLighthouses,
    TaskKind::Convert,
];

pub(super) fn kind_name(kind: TaskKind) -> &'static str {
    match kind {
        TaskKind::Pacifist => "pacifist",
        TaskKind::Genius => "genius",
        TaskKind::Network => "network",
        TaskKind::Wealth => "wealth",
        TaskKind::Killer => "killer",
        TaskKind::Metropolis => "metropolis",
        TaskKind::Explorer => "explorer",
        TaskKind::ExploreLighthouses => "explorelighthouses",
        TaskKind::Convert => "convert",
        TaskKind::None => "none",
    }
}

impl Engine {
    fn task_mut(&mut self, player: u8, kind: TaskKind) -> Option<&mut PlayerTask> {
        self.s.tasks.iter_mut().find(|t| t.player == player)?.tasks.iter_mut().find(|t| t.kind == kind)
    }

    pub fn task(&self, player: u8, kind: TaskKind) -> Option<&PlayerTask> {
        self.s.tasks.iter().find(|t| t.player == player)?.tasks.iter().find(|t| t.kind == kind)
    }

    pub fn enable_task(&mut self, player: u8, kind: TaskKind) {
        if crate::env_var!("TASK_DEBUG").is_ok() {
            eprintln!("ENABLE cmd {} p{} {:?}", self.s.current_command, player, kind);
        }
        if player == NATURE || kind == TaskKind::None {
            return;
        }
        if self.task(player, kind).is_some() {
            return;
        }
        let task = PlayerTask { kind, is_started: true, ..PlayerTask::default() };
        match self.s.tasks.iter_mut().find(|t| t.player == player) {
            Some(row) => row.tasks.push(task),
            None => self.s.tasks.push(PlayerTasks { player, tasks: vec![task] }),
        }
    }

    fn complete_task(&mut self, player: u8, kind: TaskKind) {
        if let Some(t) = self.task_mut(player, kind) {
            t.is_completed = true;
        }
    }

    /// Improvements unlocked by completed tasks (the monuments).
    pub fn task_improvements(&self, player: u8) -> Vec<i32> {
        let mut out = Vec::new();
        let Some(row) = self.s.tasks.iter().find(|t| t.player == player) else { return out };
        let tt = self.t.tribe_tech(self.tribe_of(player));
        for t in row.tasks.iter().filter(|t| t.is_completed) {
            for &idx in tt.task_improvements(t.kind) {
                if !out.contains(&idx) {
                    out.push(idx);
                }
            }
        }
        out
    }

    fn live_task(&self, player: u8, kind: TaskKind) -> bool {
        self.task(player, kind).is_some_and(|t| t.is_started && !t.is_completed)
    }

    /// Techs enable the tasks they list.
    pub fn tasks_after_research(&mut self, player: u8, tech: i32) {
        let names: Vec<String> = self.t.tech(tech).map(|t| t.task_unlocks.clone()).unwrap_or_default();
        if crate::env_var!("TASK_DEBUG").is_ok() {
            eprintln!("RESEARCH cmd {} p{} tech {} names {:?}", self.s.current_command, player, tech, names);
        }
        for n in names {
            if let Some(k) = kind_from_name(&n) {
                self.enable_task(player, k);
            }
        }
        self.check_wealth(player);
        if self.live_task(player, TaskKind::Genius) && self.researchable(player).is_empty() {
            self.complete_task(player, TaskKind::Genius);
        }
    }

    /// Learning the tribe's starting techs when a match begins enables the tasks they list, as retail's start of
    /// match does (a tribe that starts with Philosophy has the Genius task from the first turn).
    pub(super) fn enable_start_tasks(&mut self, player: u8) {
        if player == NATURE {
            return;
        }
        for tech in self.techs(player).to_vec() {
            self.tasks_after_research(player, tech);
        }
    }

    /// Revealing the lighthouse tile (queued by an explore for a map corner): the task check runs before
    /// the task is enabled, then the capital gets one population.
    pub fn on_reveal_lighthouse(&mut self, player: u8) {
        if self.live_task(player, TaskKind::ExploreLighthouses) {
            let (w, h) = (self.width() - 1, self.height() - 1);
            let n = [(0, 0), (w, 0), (0, h), (w, h)]
                .iter()
                .filter(|(x, y)| {
                    self.tile(Wc { x: *x, y: *y }).is_some_and(|t| self.is_explored(player, t.index as usize))
                })
                .count();
            if n == 4 {
                self.complete_task(player, TaskKind::ExploreLighthouses);
            }
        }
        self.enable_task(player, TaskKind::ExploreLighthouses);
        if let Some(cap) = self.capital_of(player) {
            self.add_population(cap, 1);
        }
    }

    /// Revealing the lighthouse tile also queues a level-up on the lighthouse.
    pub fn level_up_lighthouse(&mut self, at: Wc) {
        if let Some(rt) = self.improvement_rt_mut(at) {
            rt.level += 1;
        }
    }

    pub fn tasks_after_level_up(&mut self, player: u8, new_level: u16) {
        if new_level > 1 {
            self.enable_task(player, TaskKind::Metropolis);
        }
        if self.live_task(player, TaskKind::Metropolis) {
            let rich = self.city_tiles(player).iter().any(|c| self.improvement_rt(*c).is_some_and(|r| r.level >= 5));
            if rich {
                self.complete_task(player, TaskKind::Metropolis);
            }
        }
    }

    pub fn tasks_after_kill(&mut self, player: u8) {
        self.enable_task(player, TaskKind::Killer);
        let mut done = false;
        if let Some(t) = self.task_mut(player, TaskKind::Killer) {
            if !t.is_completed {
                t.kills += 1;
                done = t.kills >= 10;
            }
        }
        if done {
            self.complete_task(player, TaskKind::Killer);
        }
    }

    /// A conversion of a unit that is not Nature's (each unit of a converted centipede chain counts): the Convert task,
    /// when the player has it, counts it and is complete at 3, as in the game.
    pub fn tasks_after_convert(&mut self, player: u8) {
        let done = match self.task_mut(player, TaskKind::Convert) {
            Some(t) if t.is_started && !t.is_completed => {
                t.converts += 1;
                t.converts >= 3
            }
            _ => false,
        };
        if done {
            self.complete_task(player, TaskKind::Convert);
        }
    }

    /// Any attack that reaches the Pacifist rule starts the quiet streak again. Only `turns` changes: `converts`
    /// belongs to the Convert task and stays as it is.
    pub fn tasks_after_attack(&mut self, player: u8) {
        if let Some(t) = self.task_mut(player, TaskKind::Pacifist) {
            if !t.is_completed {
                t.turns = 0;
            }
        }
    }

    /// An attack that does not break the streak by the battle rule still does, as retail's attack does.
    pub fn tasks_mark_attacked(&mut self, player: u8) {
        self.tasks_after_attack(player);
    }

    /// End of turn: the Pacifist task adds a turn; completion waits for the task check at the player's next start of
    /// turn.
    pub fn tasks_end_of_turn(&mut self, player: u8) {
        if let Some(t) = self.task_mut(player, TaskKind::Pacifist) {
            if !t.is_completed {
                t.turns += 1;
            }
        }
    }

    /// Start-of-turn check of the Pacifist task: it is complete at `turns >= 5`.
    pub fn tasks_start_of_turn(&mut self, player: u8) {
        let done = self.task(player, TaskKind::Pacifist).is_some_and(|t| !t.is_completed && t.turns >= 5);
        if done {
            self.complete_task(player, TaskKind::Pacifist);
        }
    }

    pub fn tasks_after_connection_change(&mut self, player: u8, newly_connected: bool) {
        if newly_connected {
            self.enable_task(player, TaskKind::Network);
        }
        if self.live_task(player, TaskKind::Network) {
            let start = self.player(player).map(|p| p.start_tile);
            let n = self
                .city_tiles(player)
                .iter()
                .filter(|c| Some(**c) != start)
                .filter(|c| self.improvement_rt(**c).is_some_and(|r| r.connected_to_capital_of_player != 0))
                .count();
            if n >= 5 {
                self.complete_task(player, TaskKind::Network);
            }
        }
    }

    pub fn check_wealth(&mut self, player: u8) {
        if self.live_task(player, TaskKind::Wealth) && self.player(player).is_some_and(|p| p.currency >= 100) {
            self.complete_task(player, TaskKind::Wealth);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn task_kinds_are_in_discriminant_order() {
        for (i, kind) in TASK_KINDS.iter().enumerate() {
            assert_eq!(*kind as usize, i);
        }
    }

    /// The per-tribe monument lists give what resolving the task data's names for the player's tribe gives,
    /// for every tribe idx (and some without tribe data).
    #[test]
    fn task_improvement_tables_match_name_lookup() {
        let mut e = Engine::new_game(&GameOptions::default());
        let id = e.s.players[0].id;
        let mut resolved = 0;
        for tribe in -1..e.t.tribe_name.len() as i32 + 2 {
            e.s.players[0].tribe = tribe;
            for kind in TASK_KINDS {
                let by_name: Vec<i32> =
                    e.t.logic
                        .task(kind_name(kind))
                        .map(|d| d.improvement_unlocks.iter().filter_map(|n| e.tribe_improvement(id, n)).collect())
                        .unwrap_or_default();
                assert_eq!(e.t.tribe_tech(tribe).task_improvements(kind), by_name.as_slice(), "tribe {tribe} {kind:?}");
                resolved += by_name.len();
            }
        }
        assert!(resolved > 0);
    }
}
