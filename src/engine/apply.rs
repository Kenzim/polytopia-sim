//! Command enumeration, validation and execution.

use super::*;
use crate::hash::HashFunction;
use crate::rules::combat::{get_battle_results, BattleQuery};

/// Up to this version a revealed unit's owner is not introduced to the mover's owner.
const VERSION_REVEAL_MEETS: i32 = 105;
/// The last version whose attack leaves an empty tile alone; a Drench attack floods it after this.
const VERSION_DRENCH_FLOODS: i32 = 104;
/// From this version Eat places the new segment where the path finder lets it stand; the version 119 rule asks for a path
/// from the tail instead.
const VERSION_EAT_ACCESSIBLE: i32 = 119;

impl Engine {
    // ------------------------------------------------------------------
    // Legal command generation
    // ------------------------------------------------------------------

    /// Everything the current player may do right now.
    pub fn legal_commands(&self) -> Vec<Command> {
        let _grid = self.unit_grid_scope();
        let _zoc = self.zoc_scope();
        self.legal_commands_inner()
    }

    /// Whether `cmd` is in [`Engine::legal_commands`], without building the whole list: only the commands of the
    /// unit, city or kind `cmd` names are generated, by the same code.
    pub fn is_legal(&self, cmd: &Command) -> bool {
        let _grid = self.unit_grid_scope();
        let _zoc = self.zoc_scope();
        if self.is_over() {
            return false;
        }
        let me = self.current_player();
        if self.pending_trigger(me).is_some() {
            return self.legal_commands_inner().contains(cmd);
        }
        let mut out = Vec::with_capacity(32);
        match *cmd {
            Command::EndTurn => return true,
            // Every command `unit_commands` adds for a unit carries that unit's id, bar ClearTileEffect.
            Command::Move { unit, .. }
            | Command::Attack { unit, .. }
            | Command::Capture { unit, .. }
            | Command::Promote { unit }
            | Command::Recover { unit }
            | Command::HealOthers { unit }
            | Command::Explode { unit }
            | Command::Disband { unit }
            | Command::Upgrade { unit, .. }
            | Command::ExamineRuins { unit }
            | Command::Swarm { unit }
            | Command::Boost { unit }
            | Command::Disembark { unit }
            | Command::FreezeArea { unit }
            | Command::BreakIce { unit } => {
                for u in self.s.units.iter().filter(|u| u.owner == me && u.id == unit) {
                    self.unit_commands(u, &mut out);
                }
            }
            Command::Train { city, .. } => {
                if self.city_tiles(me).contains(&city) {
                    self.train_commands(me, city, &mut out);
                }
            }
            Command::Build { .. } => self.build_commands(me, &mut out),
            Command::Research { tech } => {
                let currency = self.player(me).map(|p| p.currency).unwrap_or(0);
                return self.researchable(me).contains(&tech) && currency >= self.tech_price(me, tech);
            }
            _ => return self.legal_commands_inner().contains(cmd),
        }
        out.contains(cmd)
    }

    /// Whether the game accepts `cmd` now: every legal command, plus a Destroy or ClearTileEffect that only passes the command's
    /// own check, a Move of a unit that has a leader and a Decompose the improvement's ability list does not offer.
    /// Retail's decompose command only needs the current player and an improvement on the tile (a lighthouse, a city
    /// or one already decomposing included), and its action adds the Decomposing effect only when it is missing.
    pub fn accepts(&self, cmd: &Command) -> bool {
        if self.legal_commands().contains(cmd) {
            return true;
        }
        let me = self.current_player();
        match *cmd {
            Command::Decompose { at } => {
                !self.is_over()
                    && self.pending_trigger(me).is_none()
                    && self.tile(at).is_some_and(|t| t.improvement != 0)
            }
            // The destroy command's own check needs only the current player, no pending trigger and an improvement
            // on the tile; the Destroy ability and the destructable test belong to the improvement's ability list.
            Command::Destroy { at } => {
                !self.is_over()
                    && self.pending_trigger(me).is_none()
                    && self.tile(at).is_some_and(|t| t.improvement != 0)
            }
            // The clear-effect command's own check is the same: no unit and no ability, only the effect on the tile.
            Command::ClearTileEffect { at, effect } => {
                !self.is_over() && self.pending_trigger(me).is_none() && effect < 16 && self.has_effect(at, effect)
            }
            // The move command's own check looks at the unit, its moved and frozen state and a path from its tile to
            // the target, never at its leader, so a centipede segment can be sent on its own although the move
            // options leave it out.
            Command::Move { unit, to } => {
                !self.is_over()
                    && self.pending_trigger(me).is_none()
                    && self.unit(unit).is_some_and(|u| {
                        u.owner == me
                            && u.leader != 0
                            && !u.moved
                            && !u.frozen
                            && self.move_path(u, to).is_some_and(|path| path.len() > 1)
                    })
            }
            // The build command's own check ignores a pending city reward (only the client's action manager waits for
            // it), so a build sent before the reward is answered is accepted.
            Command::Build { .. } => {
                !self.is_over() && self.pending_trigger(me).is_some_and(|t| t.peace_from.is_none()) && {
                    let mut out = Vec::new();
                    self.build_commands(me, &mut out);
                    out.contains(cmd)
                }
            }
            _ => false,
        }
    }

    fn legal_commands_inner(&self) -> Vec<Command> {
        if self.is_over() {
            return Vec::new();
        }
        let me = self.current_player();
        let mut out = Vec::with_capacity(64);
        if let Some(trigger) = self.pending_trigger(me) {
            if let Some(with) = trigger.peace_from {
                out.push(Command::PeaceResponse { with, accept: true });
                out.push(Command::PeaceResponse { with, accept: false });
                return out;
            }
            let level = self.improvement_rt(trigger.city).map(|r| r.level).unwrap_or(0);
            for reward in self.reward_options(level) {
                out.push(Command::CityReward { city: trigger.city, reward });
            }
            return out;
        }
        for unit in self.s.units.iter().filter(|u| u.owner == me) {
            self.unit_commands(unit, &mut out);
        }
        for city in self.city_tiles(me) {
            self.train_commands(me, city, &mut out);
        }
        self.build_commands(me, &mut out);
        // The game's improvement-ability list for a tile: when `can_destroy` holds, Destroy needs player
        // ability Destroy, and Decompose needs ability Decompose and no Decomposing effect on the improvement yet.
        let destroy = self.has_player_ability(me, player_ability::DESTROY);
        let decompose = self.has_player_ability(me, player_ability::DECOMPOSE);
        if destroy || decompose {
            for t in self.s.map.tiles.iter().filter(|t| self.can_destroy(me, t.coordinates)) {
                if destroy {
                    out.push(Command::Destroy { at: t.coordinates });
                }
                if decompose && self.improvement_rt(t.coordinates).is_some_and(|r| !r.decomposing) {
                    out.push(Command::Decompose { at: t.coordinates });
                }
            }
        }
        let currency = self.player(me).map(|p| p.currency).unwrap_or(0);
        for tech in self.researchable(me) {
            if currency >= self.tech_price(me, tech) {
                out.push(Command::Research { tech });
            }
        }
        // Diplomacy (offered by the diplomacy screen rather than a tile): the diplomacy commands for every
        // known player.
        let known: Vec<u8> = self.player_runtime(me).map(|r| r.known_players.clone()).unwrap_or_default();
        for opp in known {
            out.extend(self.diplomacy_commands_for(me, opp).into_iter().map(|(c, _)| c));
        }
        out.push(Command::EndTurn);
        out
    }

    /// Null unless explored by the player, resource present,
    /// no improvement on the tile, and the resource visible to the player.
    pub fn get_resource(&self, at: Wc, player: u8) -> Option<i32> {
        let tile = self.tile(at)?;
        (tile.resource != 0
            && tile.improvement == 0
            && self.is_explored_at(player, at)
            && self.resource_visible_to(tile.resource, player))
        .then_some(tile.resource)
    }

    /// Whether the player can destroy the improvement on the tile.
    pub fn can_destroy(&self, me: u8, at: Wc) -> bool {
        let Some(t) = self.tile(at) else { return false };
        // Destructable: not City, Ruin or Lighthouse.
        if matches!(t.improvement, 0 | 1 | 2 | 47) || (me != 255 && t.owner != me) {
            return false;
        }
        let v = self.s.version;
        let unit = self.unit_at(at);
        if v > 0x53 {
            if let Some(u) = self.visible_unit_for(at, me) {
                let ok = if v > 0x69 {
                    u.owner == me || self.at_peace(me, u.owner)
                } else {
                    unit.is_some_and(|w| w.owner == me)
                };
                if !ok {
                    return false;
                }
            }
        } else if unit.is_some_and(|u| u.owner != me) {
            return false;
        }
        // A bridge with a unit on it stays.
        !(unit.is_some()
            && self.t.improvement(t.improvement).is_some_and(|i| i.abilities.contains(&improvement_ability::BRIDGE)))
    }

    #[doc(hidden)]
    pub fn unit_commands_pub(&self, unit: &UnitState, out: &mut Vec<Command>) {
        self.unit_commands(unit, out)
    }

    /// Unlocked types that upgrade from the unit's type, in data order.
    pub fn unlocked_upgrades_for(&self, me: u8, unit: &UnitState) -> Vec<i32> {
        let Some(info) = self.unit_info(unit) else { return Vec::new() };
        let targets = &info.upgrades;
        if targets.is_empty() {
            return Vec::new();
        }
        let unlocked = self.unlocked_units(me, true);
        // Order of the player's unlocked-unit list.
        let mut out: Vec<i32> = unlocked.iter().copied().filter(|u| targets.contains(u)).collect();
        out.dedup();
        out
    }

    /// The upgrade command's territory rule: from version 99 the tile must be the player's own,
    /// and from 101 a tile of a player at peace with them also passes.
    fn upgrade_territory_ok(&self, unit: &UnitState) -> bool {
        let v = self.s.version;
        if v < 0x63 {
            return true;
        }
        let Some(owner) = self.tile(unit.coordinates).map(|t| t.owner) else { return false };
        owner == unit.owner || (v > 0x64 && crate::rules::diplomacy::has_peace_with(&self.s, unit.owner, owner))
    }

    fn unit_commands(&self, unit: &UnitState, out: &mut Vec<Command>) {
        if self.can_capture(unit, unit.coordinates) {
            out.push(Command::Capture { unit: unit.id, at: unit.coordinates });
        }
        if self.can_attack(unit) {
            for target in self.attack_targets(unit) {
                out.push(Command::Attack { unit: unit.id, target });
            }
        }
        if self.can_move(unit) {
            for to in self.move_destinations(unit) {
                out.push(Command::Move { unit: unit.id, to });
            }
        }
        if self.can_be_promoted(unit) {
            out.push(Command::Promote { unit: unit.id });
        }
        if self.can_freeze_area(unit) {
            out.push(Command::FreezeArea { unit: unit.id });
        }
        if self.can_break_ice(unit) {
            out.push(Command::BreakIce { unit: unit.id });
        }
        // The unit actions list emits one upgrade command per unlocked upgrade, in that list's order;
        // the command filter keeps the ones the player can pay for.
        if unit.leader == 0 && self.upgrade_territory_ok(unit) {
            let me = unit.owner;
            let currency = self.player(me).map(|p| p.currency).unwrap_or(0);
            for ti in self.unlocked_upgrades_for(me, unit) {
                let cost = self.t.unit(ti).map(|i| i.data.cost).unwrap_or(i32::MAX);
                if cost <= currency {
                    out.push(Command::Upgrade { unit: unit.id, to: ti });
                }
            }
        }
        if self.can_recover(unit) {
            out.push(Command::Recover { unit: unit.id });
        }
        if self.can_heal_others(unit) && !self.heal_options(unit.owner, unit.coordinates).is_empty() {
            out.push(Command::HealOthers { unit: unit.id });
        }
        if self.can_examine_ruins(unit) {
            out.push(Command::ExamineRuins { unit: unit.id });
        }
        if self.can_explode(unit) {
            out.push(Command::Explode { unit: unit.id });
        }
        if self.can_attack(unit)
            && self.has_ability(unit, unit_ability::SWARM)
            && !self.boost_options(unit.owner, unit.coordinates).is_empty()
        {
            out.push(Command::Swarm { unit: unit.id });
        }
        // The boost command is valid when the unit has Boost and its boost options are not empty.
        if self.has_ability(unit, unit_ability::BOOST) && !self.boost_options(unit.owner, unit.coordinates).is_empty() {
            out.push(Command::Boost { unit: unit.id });
        }
        // Can disband: not moved, not attacked, the Disband ability, no leader.
        if !unit.moved
            && !unit.attacked
            && unit.leader == 0
            && self.has_player_ability(unit.owner, player_ability::DISBAND)
        {
            out.push(Command::Disband { unit: unit.id });
        }
        if self.can_disembark(unit) {
            out.push(Command::Disembark { unit: unit.id });
        }
        self.clear_effect_commands(unit, out);
    }

    /// The clear-tile-effect commands offered at the end of the unit actions list, for the
    /// unit's own tile. Version 115: with can-attack or can-move, Algae for an Amphibious, Swim or Fly unit and
    /// Flooded for an Amphibious or non-Swim unit. Version 116+: Algae the same; Flooded needs can-attack and
    /// can-move, a tribe without the Flooded ability, and an Amphibious or non-Swim unit.
    fn clear_effect_commands(&self, unit: &UnitState, out: &mut Vec<Command>) {
        let v = self.s.version;
        if v < 0x73 {
            return;
        }
        let at = unit.coordinates;
        let Some(t) = self.tile(at) else { return };
        let has = |e: u16| t.effects & (1 << e) != 0;
        let ab = |a: i32| self.has_ability(unit, a);
        let (attack, mv) = (self.can_attack(unit), self.can_move(unit));
        let algae_ok = (attack || mv)
            && has(Self::EFFECT_ALGAE)
            && (ab(unit_ability::AMPHIBIOUS) || ab(unit_ability::SWIM) || ab(unit_ability::FLY));
        let wader = ab(unit_ability::AMPHIBIOUS) || !ab(unit_ability::SWIM);
        let flood_ok = if v == 0x73 {
            (attack || mv) && has(Self::EFFECT_FLOODED) && wader
        } else {
            attack && mv && has(Self::EFFECT_FLOODED) && !self.has_tribe_ability(unit.owner, "flooded") && wader
        };
        if algae_ok {
            out.push(Command::ClearTileEffect { at, effect: Self::EFFECT_ALGAE });
        }
        if flood_ok {
            out.push(Command::ClearTileEffect { at, effect: Self::EFFECT_FLOODED });
        }
    }

    /// Clear a tile effect: remove the effect (Algae also removes Foam), queue a city-connection update
    /// for the player and a route update, and exhaust the unit on the tile
    /// (the command marks it as triggered by the unit).
    fn do_clear_tile_effect(&mut self, me: u8, at: Wc, effect: u16) {
        if let Some(t) = self.tile_mut(at) {
            t.effects &= !(1 << effect);
            if effect == Self::EFFECT_ALGAE {
                t.effects &= !(1 << Self::EFFECT_FOAM);
            }
        }
        if let Some(i) = self.s.units.iter().position(|u| u.coordinates == at) {
            self.make_exhausted(i);
        }
        self.update_routes();
        self.update_city_connections(me);
    }

    /// Can disembark: a carrier with a passenger on a wetland tile (flooded or algae)
    /// or on an improvement with the Bridge ability. The disembark command's validity check requires nothing else.
    pub fn can_disembark(&self, unit: &UnitState) -> bool {
        if unit.passenger.is_none() {
            return false;
        }
        let Some(t) = self.tile(unit.coordinates) else { return false };
        let wetland = t.effects & ((1 << Self::EFFECT_FLOODED) | (1 << Self::EFFECT_ALGAE)) != 0;
        wetland || (t.improvement != 0 && self.t.has_improvement_ability(t.improvement, improvement_ability::BRIDGE))
    }

    /// Can explode: the Explode ability; after version 114 the unit must not be
    /// frozen and not trained this turn (a tail segment with a leader may), before that it must be able to attack on its owner's turn.
    pub fn can_explode(&self, unit: &UnitState) -> bool {
        if !self.has_ability(unit, unit_ability::EXPLODE) {
            return false;
        }
        if self.s.version > 0x72 {
            !unit.frozen && (u32::from(unit.created_turn) != self.s.current_turn || unit.leader != 0)
        } else {
            self.can_attack(unit) && unit.owner == self.current_player()
        }
    }

    /// Explode action (version > 120). A follower explodes too (queued first, so it
    /// runs last), a leader loses its tail, spores (plus algae on water, see `spawn_spore_resource`) grow on the
    /// tile, and every unit in radius 1 not at peace with the player takes the floored half battle damage and is
    /// poisoned when it survives. The unit leaves the tile at once. Damage by version: up to 82
    /// the full attack damage, 83 to 120 half of it truncated, later
    /// half floored to a multiple of 10. Per victim, the aggression change from an attack does nothing above version 112 and
    /// the diplomacy-relation change is `note_attack`; the score drop comes from `refresh_scores`.
    fn do_explode(&mut self, me: u8, unit_id: u32) {
        let Some(mut unit) = self.unit(unit_id).cloned() else { return };
        let at = unit.coordinates;
        let carrier_id = unit.id;
        if self.has_ability(&unit, unit_ability::CARRY) {
            if let Some(p) = unit.passenger.as_deref() {
                unit = p.clone();
                unit.coordinates = at;
            }
        }
        let follower = if unit.follower != 0 { self.unit(unit.follower).map(|f| f.id) } else { None };
        if unit.leader != 0 {
            if let Some(li) = self.unit_index(unit.leader) {
                self.s.units[li].follower = 0;
            }
        }
        self.spawn_spore_resource(at, Some(unit.owner));
        let a = self.combat_unit(&unit);
        let version = self.s.version;
        let mut hits: Vec<(Wc, i32, bool)> = Vec::new();
        for n in self.neighbors(at) {
            let Some(victim) = self.unit_at(n).cloned() else { continue };
            if victim.id == carrier_id || victim.owner == me || self.at_peace(me, victim.owner) {
                continue;
            }
            let q = self.battle_query(&unit, &victim, false);
            let r = get_battle_results(&a, &self.combat_unit(&victim), &q);
            let dmg = if version <= 0x52 {
                r.attack_damage
            } else if version <= 0x78 {
                r.attack_damage / 2
            } else {
                r.floored_half_attack_damage()
            };
            self.note_attack(me, victim.owner);
            self.tasks_after_attack(me);
            hits.push((n, dmg, dmg < i32::from(victim.health)));
        }
        self.remove_unit(carrier_id);
        // Queued Poison then Attack per tile; last-in first-out runs the tiles in reverse, attack before poison.
        // Retail builds each attack action with the victim's own tile as origin and target, so the
        // victim is the "attacker" of the attack; the `(n, n)` here is deliberate, not a mix-up.
        for &(n, dmg, poison) in hits.iter().rev() {
            self.perform_attack(me, n, n, dmg);
            if poison {
                self.poison_unit_action(me, n);
            }
        }
        if let Some(f) = follower {
            self.do_explode(me, f);
        }
    }

    /// Can heal others: the Heal ability, can-move and can-attack.
    pub fn can_heal_others(&self, unit: &UnitState) -> bool {
        self.has_ability(unit, unit_ability::HEAL) && self.can_move(unit) && self.can_attack(unit)
    }

    /// The neighbours (not the centre) holding an own unit that is
    /// hurt (max health above the carrier's health) or poisoned.
    pub fn heal_options(&self, player: u8, at: Wc) -> Vec<Wc> {
        self.heal_options_with(player, at, false)
    }

    /// `heal_options` with its `include_center` flag: the end-of-turn mycelium pass passes true.
    pub fn heal_options_with(&self, player: u8, at: Wc, include_center: bool) -> Vec<Wc> {
        let mut cells = self.neighbors(at);
        if include_center {
            cells.push(at);
        }
        cells
            .into_iter()
            .filter(|n| {
                self.unit_at(*n)
                    .is_some_and(|u| u.owner == player && (self.max_health(u) > i32::from(u.health) || u.poisoned))
            })
            .collect()
    }

    /// Heal others: the healer is spent, then a heal-others action queues a
    /// heal action with a fixed heal amount of 40 on every heal option.
    fn do_heal_others(&mut self, unit: u32) {
        let Some(i) = self.unit_index(unit) else { return };
        self.s.units[i].moved = true;
        self.s.units[i].attacked = true;
        let (owner, at) = (self.s.units[i].owner, self.s.units[i].coordinates);
        for c in self.heal_options(owner, at) {
            let max = self.unit_at(c).map(|u| self.max_health(u)).unwrap_or(0);
            crate::rules::commands::heal_unit(&mut self.s, c, 40, max.clamp(0, i32::from(u16::MAX)) as u16);
        }
    }

    /// The neighbours (not the centre) holding an own unit that is
    /// not Boosted, not Swift (version > 115) and has no leader.
    pub fn boost_options(&self, player: u8, at: Wc) -> Vec<Wc> {
        let swift_check = self.s.version > 0x73;
        self.area(at, 1)
            .into_iter()
            .filter(|c| *c != at)
            .filter(|c| {
                self.unit_at(*c)
                    .is_some_and(|u| u.owner == player && !u.boosted && !(swift_check && u.swift) && u.leader == 0)
            })
            .collect()
    }

    /// Swarm: the unit is spent, then a boost-others action queues a
    /// boost action per option, which adds Swift (version > 114) or Boosted.
    fn do_swarm(&mut self, me: u8, unit: u32) {
        let Some(i) = self.unit_index(unit) else { return };
        self.s.units[i].moved = true;
        self.s.units[i].attacked = true;
        self.boost_others(me, unit);
    }

    /// Boost others; the boost command queues only this, so the booster is not spent.
    fn boost_others(&mut self, me: u8, unit: u32) {
        let Some(i) = self.unit_index(unit) else { return };
        let at = self.s.units[i].coordinates;
        let swift = self.s.version > 0x72;
        for c in self.boost_options(me, at) {
            if let Some(j) = self.s.units.iter().position(|u| u.coordinates == c) {
                if swift {
                    self.s.units[j].swift = true;
                } else {
                    self.s.units[j].boosted = true;
                }
            }
        }
    }

    pub fn can_examine_ruins(&self, unit: &UnitState) -> bool {
        self.can_move(unit)
            && self.can_attack(unit)
            && self.tile(unit.coordinates).is_some_and(|t| t.improvement == improvement::RUIN || t.has_ruin)
            && if crate::env_var!("OLD_RUINS_STATIC").is_ok() {
                self.unit_info(unit).is_some_and(|i| !i.abilities.contains(&unit_ability::STATIC))
            } else {
                // Can examine ruins: no leader, not Nature; Static units may examine.
                unit.leader == 0 && unit.owner != NATURE
            }
    }

    #[doc(hidden)]
    pub fn train_commands_pub(&self, me: u8, city: Wc, out: &mut Vec<Command>) {
        self.train_commands(me, city, out)
    }

    fn train_commands(&self, me: u8, city: Wc, out: &mut Vec<Command>) {
        if self.unit_at(city).is_some() || !self.can_city_support_unit(city) {
            return;
        }
        let currency = self.player(me).map(|p| p.currency).unwrap_or(0);
        if self.player_runtime(me).is_some_and(|r| r.block_train_units) {
            return;
        }
        for unit_type in self.unlocked_units(me, false) {
            let Some(info) = self.t.unit(unit_type) else { continue };
            if info.data.cost > currency || info.data.cost <= 0 && info.data.hidden {
                continue;
            }
            if !info.terrain.is_empty() {
                // Water-bound units need a tile next to the city that matches their terrain: a tile of a listed terrain,
                // or wetland (flooded or algae) when the list holds wetland.
                let near = self.neighbors(city).iter().any(|n| {
                    self.tile(*n).is_some_and(|t| {
                        info.terrain.contains(&t.terrain)
                            || (info.terrain.contains(&terrain::WETLAND) && self.is_wetland_index(t.index))
                    })
                });
                if !near {
                    continue;
                }
            }
            out.push(Command::Train { city, unit_type });
        }
    }

    pub fn player_runtime(&self, id: u8) -> Option<&crate::PlayerRuntime> {
        self.s.player_runtime.iter().find(|r| r.player == id)
    }

    pub fn build_commands(&self, me: u8, out: &mut Vec<Command>) {
        let currency = self.player(me).map(|p| p.currency).unwrap_or(0);
        self.build_commands_with(me, currency, out);
    }

    /// The buildable-improvements list with unavailable ones included uses `i32::MAX` here: unaffordable builds are listed too.
    pub fn build_commands_with(&self, me: u8, currency: i32, out: &mut Vec<Command>) {
        // The tile-independent rejections of `can_build` once per kind, and its ownership test per tile: a kind
        // without Freelance is only built on the player's own tiles.
        // Also `meets_requirement`, with the player's abilities read once.
        let has_pontoon = self.has_player_ability(me, player_ability::PONTOON);
        let treehouse = self.has_player_ability(me, player_ability::TREEHOUSE);
        let built_unique = self.player_runtime(me).map_or(&[][..], |r| r.built_unique_improvements.as_slice());
        let kinds: Vec<BuildKind> = self
            .unlocked_improvements(me)
            .into_iter()
            .filter_map(|kind| {
                let info = self.t.improvement(kind)?;
                let rejected = info.data.hidden
                    || info.data.cost.unwrap_or(0) > currency
                    || kind == improvement::CITY
                    || (info.has(improvement_ability::UNIQUE) && built_unique.contains(&kind));
                let pontoon = has_pontoon && !Self::NON_PONTOON.contains(&kind);
                (!rejected).then(|| BuildKind::new(kind, info, &info.terrain_req_ids, pontoon))
            })
            .collect();
        if kinds.is_empty() {
            return;
        }
        let any_freelance = kinds.iter().any(|k| k.freelance);
        if kinds.len() <= 64 {
            let sets = KindSets::new(&kinds);
            for tile in self.s.map.tiles.iter() {
                let own = tile.owner == me;
                if !own && !any_freelance {
                    continue;
                }
                let (plain, with_pontoon) = BuildKind::tile_terrains(tile, treehouse);
                let mut candidates = sets.candidates(tile, plain, with_pontoon);
                // The tile-only rejections at the top of `can_build_on`, applied to every candidate at once.
                if !own {
                    candidates &= sets.freelance;
                    if tile.owner != 0 {
                        candidates &= sets.manual;
                    }
                }
                if tile.improvement != 0 {
                    candidates &= sets.discrete | sets.road;
                }
                if tile.has_road || tile.improvement == improvement::CITY || tile.improvement == improvement::BRIDGE {
                    candidates &= !sets.road;
                }
                while candidates != 0 {
                    let k = &kinds[candidates.trailing_zeros() as usize];
                    candidates &= candidates - 1;
                    if k.slow.is_none_or(|_| k.requirement_met(tile, plain, with_pontoon, treehouse))
                        && self.can_build_on(me, tile, k.info, k.kind, false)
                    {
                        out.push(Command::Build { at: tile.coordinates, improvement: k.kind });
                    }
                }
            }
            return;
        }
        for tile in self.s.map.tiles.iter() {
            let own = tile.owner == me;
            if !own && !any_freelance {
                continue;
            }
            let (plain, with_pontoon) = BuildKind::tile_terrains(tile, treehouse);
            for k in &kinds {
                if (own || k.freelance)
                    && k.requirement_met(tile, plain, with_pontoon, treehouse)
                    && self.can_build(me, tile.coordinates, k.kind, currency)
                {
                    out.push(Command::Build { at: tile.coordinates, improvement: k.kind });
                }
            }
        }
    }

    /// Whether the improvement can be built on the tile, plus the affordability check.
    pub fn can_build(&self, me: u8, at: Wc, kind: i32, currency: i32) -> bool {
        let Some(info) = self.t.improvement(kind) else { return false };
        let Some(tile) = self.tile(at) else { return false };
        if info.data.hidden || info.data.cost.unwrap_or(0) > currency {
            return false;
        }
        if kind == improvement::CITY {
            return false;
        }
        self.can_build_on(me, tile, info, kind, true)
    }

    /// The tile-dependent part of [`Engine::can_build`], for a kind that passed its tile-independent checks.
    /// `check_requirement` false skips the terrain and resource requirement, for callers that already know it holds.
    fn can_build_on(&self, me: u8, tile: &Tile, info: &ImprovementInfo, kind: i32, check_requirement: bool) -> bool {
        let at = tile.coordinates;
        let ab = |a: i32| info.has(a);
        if tile.owner != me {
            if !ab(improvement_ability::FREELANCE) {
                return false;
            }
            if tile.owner != 0 && !ab(improvement_ability::MANUAL) {
                return false;
            }
        }
        if kind == improvement::ROAD {
            // Has road: the road flag, or a City or Bridge on the tile.
            if tile.has_road || tile.improvement == improvement::CITY || tile.improvement == improvement::BRIDGE {
                return false;
            }
        } else if tile.improvement != 0 && !ab(improvement_ability::DISCRETE) {
            return false;
        }
        // Every check here is a pure rejection, so the order does not matter: the terrain and resource
        // requirement rejects most tiles, so it comes first.
        if check_requirement && !self.meets_requirement(me, at, kind) {
            return false;
        }
        // A unit on a tile `me` has not explored, or one invisible to `me`, is ignored.
        let manual = ab(improvement_ability::MANUAL);
        let unit_ok = || match self.visible_unit_for(at, me) {
            Some(u) => u.owner == me || self.at_peace(me, u.owner),
            None => !manual,
        };
        // A Manual kind needs a unit on the tile, the cheapest rejection for most tiles.
        if manual && !unit_ok() {
            return false;
        }
        if ab(improvement_ability::LIMITED) {
            // Improvement within city borders: the tiles within distance 2 of the city (centre excluded) ruled by
            // the tile's city that already have this improvement block it (no owner test).
            let city = tile.ruling_city;
            if self
                .area_iter(city, 2)
                .any(|c| c != city && self.tile(c).is_some_and(|t| t.improvement == kind && t.ruling_city == city))
            {
                return false;
            }
        }
        if ab(improvement_ability::UNIQUE)
            && self.player_runtime(me).is_some_and(|r| r.built_unique_improvements.contains(&kind))
        {
            return false;
        }
        // Retail: a `creates` entry that only adds a resource the tile already has, or only an effect the
        // tile already has, blocks the build (a second Algae on the same water).
        for &(res, effect, no_resource) in &info.creates_ids {
            if res.is_some_and(|r| tile.resource == r) && effect == 0 {
                return false;
            }
            if effect != 0 && no_resource && self.has_effect(at, effect) {
                return false;
            }
        }
        if ab(improvement_ability::FLOOD) && self.has_effect(at, Self::EFFECT_FLOODED) {
            return false;
        }
        if kind == 48 {
            // Bridge: a water tile with a non-water tile on two opposite sides (a bridge is no anchor).
            if !path::is_water(tile.terrain) || !self.bridge_possible(at) {
                return false;
            }
        }
        // Adjacency requirement (windmill, forge, sawmill, market, ...).
        if !info.adjacency_req_ids.is_empty() {
            let mut ok = false;
            for &(imp, ter, res) in &info.adjacency_req_ids {
                for n in path::neighbors_iter(at) {
                    let Some(nt) = self.tile(n) else { continue };
                    if imp == Some(nt.improvement) && nt.owner == me {
                        ok = true;
                    }
                    if ter == Some(nt.terrain) {
                        ok = true;
                    }
                    if res == Some(nt.resource) {
                        ok = true;
                    }
                }
            }
            if !ok {
                return false;
            }
        }
        // Every check here is a pure rejection, so the order does not matter: a non-Manual kind scans for the unit last.
        manual || unit_ok()
    }

    /// Retail anchors a bridge on tiles that are not water; a water tile that already carries a bridge is still
    /// water, so a bridge never extends another bridge sideways.
    fn bridge_possible(&self, at: Wc) -> bool {
        let land =
            |dx: i32, dy: i32| self.tile(Wc { x: at.x + dx, y: at.y + dy }).is_some_and(|t| !path::is_water(t.terrain));
        (land(-1, 0) && land(1, 0)) || (land(0, -1) && land(0, 1))
    }

    /// The improvements that are not pontoons (from the rules data): road, growforest, temple, canal.
    const NON_PONTOON: [i32; 4] = [3, 14, 17, 52];

    /// Whether the improvement's requirement is met: every terrain entry is tried with the Wetland, Mangrove,
    /// Pontoon (field on water) and Treehouse (field on forest) substitutions; terrain and resource entries must
    /// each be met when present.
    fn meets_requirement(&self, me: u8, at: Wc, kind: i32) -> bool {
        let (Some(info), Some(tile)) = (self.t.improvement(kind), self.tile(at)) else { return false };
        let pontoon = || !Self::NON_PONTOON.contains(&kind) && self.has_player_ability(me, player_ability::PONTOON);
        let treehouse = || self.has_player_ability(me, player_ability::TREEHOUSE);
        Self::requirement_met(&info.terrain_req_ids, tile, pontoon, treehouse)
    }

    /// The body of [`Engine::meets_requirement`] for one kind's `terrain_req_ids`, with the player's Pontoon (for
    /// this kind) and Treehouse abilities supplied by the caller.
    fn requirement_met(
        req: &[(Option<i32>, Option<i32>)],
        tile: &Tile,
        pontoon: impl Fn() -> bool,
        treehouse: impl Fn() -> bool,
    ) -> bool {
        let ter = tile.terrain;
        let wetland = tile.effects & ((1 << Self::EFFECT_FLOODED) | (1 << Self::EFFECT_ALGAE)) != 0;
        let flooded = tile.effects & (1 << Self::EFFECT_FLOODED) != 0;
        let (mut need_terrain, mut terrain_ok) = (false, false);
        let (mut need_resource, mut resource_ok) = (false, false);
        for &(res, t) in req {
            if let Some(res) = res {
                need_resource = true;
                resource_ok |= tile.resource == res;
            }
            let Some(t) = t else { continue };
            need_terrain = true;
            terrain_ok |= ter == t
                || (wetland && ter != terrain::FOREST && t == terrain::WETLAND)
                || (flooded && ter == terrain::FOREST && t == terrain::MANGROVE)
                || (!path::is_land(ter) && t == terrain::FIELD && pontoon())
                || (ter == terrain::FOREST && t == terrain::FIELD && treehouse());
        }
        (!need_terrain || terrain_ok) && (!need_resource || resource_ok)
    }

    // ------------------------------------------------------------------
    // Execution
    // ------------------------------------------------------------------

    /// Apply a command if it is currently legal.
    pub fn apply(&mut self, cmd: Command) -> Result<(), Reject> {
        if !self.accepts(&cmd) {
            return Err(Reject::NotAllowed("not a legal command"));
        }
        self.apply_unchecked(cmd);
        Ok(())
    }

    /// Apply without checking legality. The caller promises `cmd` came from [`Engine::legal_commands`].
    pub fn apply_unchecked(&mut self, cmd: Command) {
        let me = self.current_player();
        // Before version 124 an explode did not go through the base command execute, so it queued neither the
        // end-of-command action (which marks every opinion for recomputing) nor the run-deferred action.
        let bare_explode = matches!(cmd, Command::Explode { .. }) && self.s.version < crate::ACHIEVEMENTS;
        let before = (self.s.current_command, self.s.current_turn);
        self.s.current_command = self.s.current_command.wrapping_add(1);
        self.hold_at = None;
        match cmd {
            Command::Move { unit, to } => self.do_move(unit, to),
            Command::Attack { unit, target } => self.do_attack(unit, target),
            Command::Capture { unit, at } => self.do_capture(unit, at),
            Command::Train { city, unit_type } => self.do_train(me, city, unit_type),
            Command::Build { at, improvement } => {
                self.do_build(me, at, improvement);
                self.check_ice_bank_levels();
            }
            Command::Research { tech } => self.do_research(me, tech),
            Command::CityReward { city, reward } => {
                self.triggers.retain(|t| !(t.player == me && t.city == city && t.peace_from.is_none()));
                self.apply_city_reward(city, reward);
            }
            Command::Promote { unit } => {
                if let Some(i) = self.unit_index(unit) {
                    // The promotion count goes on the passenger of a carrier; the carrier takes the full health.
                    let u = &mut self.s.units[i];
                    match u.passenger.as_deref_mut() {
                        Some(p) => p.promotion += 1,
                        None => u.promotion += 1,
                    }
                    let max = self.max_health(&self.s.units[i]);
                    self.s.units[i].health = max as u16;
                }
            }
            Command::Recover { unit } => {
                if let Some(i) = self.unit_index(unit) {
                    // Heal action: a poisoned unit is cured instead of healed.
                    let amount = if self.s.units[i].poisoned { 0 } else { self.heal_amount(&self.s.units[i]) };
                    let u = &mut self.s.units[i];
                    u.health = u.health.saturating_add(amount as u16);
                    u.poisoned = false;
                    u.charmed = false;
                    if let Some(p) = u.passenger.as_mut() {
                        p.poisoned = false;
                    }
                    u.moved = true;
                    u.attacked = true;
                }
            }
            Command::HealOthers { unit } => self.do_heal_others(unit),
            Command::ExamineRuins { unit } => self.do_examine_ruins(unit),
            Command::EndTurn => self.end_turn(),
            Command::Stay { unit } => {
                if let Some(i) = self.unit_index(unit) {
                    self.s.units[i].moved = true;
                    self.s.units[i].attacked = true;
                }
            }
            Command::Upgrade { unit, to } => self.do_upgrade(me, unit, to),
            Command::Swarm { unit } => self.do_swarm(me, unit),
            Command::Boost { unit } => self.boost_others(me, unit),
            Command::FreezeArea { unit } => self.do_freeze_area(unit),
            Command::BreakIce { unit } => self.do_break_ice(unit),
            Command::Explode { unit } => self.do_explode(me, unit),
            Command::Destroy { at } => self.do_destroy(me, at),
            Command::Decompose { at } => {
                // Decompose: adds the decomposing effect; the improvement goes at the end of the turn.
                if self.tile(at).is_some_and(|t| t.owner == me && t.improvement != 0) {
                    if let Some(rt) = self.improvement_rt_mut(at) {
                        rt.decomposing = true;
                    }
                }
            }
            // The disband command queues a paid disband action: half the full cost back when it
            // is at least 2, and the unit score removed.
            Command::Disband { unit } => {
                if let Some(u) = self.unit(unit).cloned() {
                    let (cost, score) = (self.full_cost(&u), self.unit_score(&u));
                    let before = self.player(me).map_or(0, |p| p.currency);
                    crate::rules::commands::disband(&mut self.s, me, u.coordinates, cost, score, true);
                    self.check_wealth_after_refund(me, before);
                    if self.has_ability(&u, unit_ability::TENTACLES) {
                        self.update_tentacles();
                    }
                }
            }
            // The disembark command queues the same disembark action a move onto land does.
            Command::Disembark { unit } => self.disembark(unit),
            Command::ClearTileEffect { at, effect } => self.do_clear_tile_effect(me, at, effect),
            // The peace treaty command only sends a message; the answer does the work.
            // The treaty action sends a peace request from the sender to the recipient.
            Command::PeaceTreaty { with } => {
                if crate::env_var!("NO_MESSAGES").is_err() {
                    if let Some(r) = self.s.player_runtime.iter_mut().find(|r| r.player == with) {
                        r.messages.push((1, me));
                    }
                }
            }
            Command::PeaceResponse { with, accept } => {
                // The peace request response command first pops the newest peace request trigger.
                if let Some(i) = self.triggers.iter().rposition(|t| t.player == me && t.peace_from.is_some()) {
                    self.triggers.remove(i);
                }
                // Answering a treaty request makes the two players meet (response action, then meet action).
                if crate::env_var!("NO_PEACE_MEET").is_err() && !self.knows_player(me, with) && me != with {
                    self.meet(me, with);
                }
                if accept && crate::rules::commands::peace_response(&mut self.s, me, with, true) {
                    // The response action queues route and connection updates for both.
                    self.update_routes();
                    self.update_city_connections(me);
                    self.update_city_connections(with);
                }
            }
            Command::BreakPeace { with } => {
                // The break-peace action disbands the breaker's units standing in the opponent's territory.
                let get_paid = self.s.version <= 0x6E;
                for at in crate::rules::commands::break_peace(&mut self.s, me, with) {
                    let Some(u) = self.unit_at(at).cloned() else { continue };
                    let (cost, score) = (self.full_cost(&u), self.unit_score(&u));
                    let before = self.player(me).map_or(0, |p| p.currency);
                    crate::rules::commands::disband(&mut self.s, me, at, cost, score, get_paid);
                    self.check_wealth_after_refund(me, before);
                }
            }
            Command::EstablishEmbassy { with } => {
                // Embassy cost: 4 + 2 per embassy already held (version > 113).
                let cost = crate::rules::diplomacy::get_embassy_count(&self.s, me).wrapping_mul(2).wrapping_add(4);
                let built = crate::rules::commands::establish_embassy(&mut self.s, me, with, cost);
                if built && crate::env_var!("NO_EMBASSY_REVEAL").is_err() {
                    // The establish-embassy action explores the area of radius 1 around the opponent's start tile, centre included:
                    // one tile exploration for each tile the builder has not seen.
                    if let Some(start) = self.player(with).map(|p| p.start_tile) {
                        let mut tiles = self.area(start, 1);
                        tiles.push(start);
                        tiles.sort_by_key(|c| (c.x, c.y));
                        for c in tiles {
                            if let Some(i) = self.tile_index(c) {
                                self.explore_tile(me, i);
                            }
                        }
                    }
                }
                // The establish-embassy command then queues a meet action (opponent, builder);
                // its validity check requires that the player is not yet known.
                if built
                    && self.s.version > crate::rules::commands::VERSION_EMBASSY_V105
                    && !self.knows_player(with, me)
                {
                    self.meet(with, me);
                    let game_type = self.s.settings.game_type;
                    crate::rules::commands::meet_aggression(&mut self.s, with, me, game_type);
                }
            }
        }
        // The command wrapper (version >= 98) queues a run-deferred action for the player under the command's
        // own actions. When an explore or road build flagged the routes, it pushes a city-connection update for the player and
        // then a route update onto the LIFO action stack, so the routes are recomputed first.
        if std::mem::take(&mut self.routes_flagged) && self.s.version >= 0x62 {
            self.update_routes();
            self.update_city_connections(me);
        }
        self.check_eliminations();
        // Answering a trigger lets the rest of the start-of-turn action stack run.
        if !self.deferred.is_empty() && !bare_explode {
            self.run_deferred();
        }
        if self.handover_held && !self.reward_pending() {
            self.handover_held = false;
            self.pass_turn();
        }
        if bare_explode {
            // The opinions stay as they were: they are keyed on the command they were computed for, so carry
            // that key over to this command instead of letting the counter step mark them for recomputing.
            let now = (self.s.current_command, self.s.current_turn);
            for m in self.opinions.borrow_mut().managers.values_mut() {
                if m.clean_at == Some(before) {
                    m.clean_at = Some(now);
                }
            }
        }
        // Retail keeps scores current with increase, decrease and modify score actions as things happen; recomputing them
        // after every command gives the same stored score (checked against live retail games).
        self.refresh_scores();
    }

    /// Heal amount: 20 normally, 40 on own or peaceful ground, capped at missing health.
    pub fn heal_amount(&self, unit: &UnitState) -> i32 {
        let tile_owner = self.tile(unit.coordinates).map(|t| t.owner).unwrap_or(0);
        let mut base = 20;
        if tile_owner == unit.owner || (tile_owner != 0 && self.at_peace(unit.owner, tile_owner)) {
            base = crate::env_var!("HEAL_OWN").ok().and_then(|v| v.parse().ok()).unwrap_or(40);
        }
        base.min(self.max_health(unit) - i32::from(unit.health)).max(0)
    }

    fn do_move(&mut self, unit_id: u32, to: Wc) {
        let Some(i) = self.unit_index(unit_id) else { return };
        let unit = self.s.units[i].clone();
        let Some(path) = self.move_path(&unit, to) else { return };
        // A destination that holds a unit, which can only be one the mover cannot see: the unit does not move, it keeps
        // its move, and the hidden unit is revealed.
        if let Some(j) = self.unit_index_at(to) {
            self.bump_hidden_unit(i, j);
            return;
        }
        let embark = self.will_embark(&unit, to);
        let abilities = self.unit_info(&unit).map(|x| x.abilities.clone()).unwrap_or_default();
        // The move command: a Hide unit without the Invisible effect queues a hide action at the
        // origin, which runs before the move: attacked is set and the unit turns invisible.
        if abilities.contains(&unit_ability::HIDE) && !unit.invisible {
            let u = &mut self.s.units[i];
            u.attacked = true;
            u.invisible = true;
        }
        // Explore along the path.
        let sight = self.sight_range(&unit);
        self.explore_path(unit.owner, sight, &path);
        let skater = abilities.contains(&unit_ability::SKATE);
        let to_ice = self.tile(to).map(|t| t.terrain == terrain::ICE).unwrap_or(false);
        {
            let u = &mut self.s.units[i];
            let last = path.len().checked_sub(2).and_then(|k| path.get(k)).copied().unwrap_or(unit.coordinates);
            u.direction = Self::direction_of(last, to);
            u.coordinates = to;
            u.moved = true;
            if skater && !to_ice {
                u.attacked = true;
            }
        }
        let tentacle_hit = self.tentacle_hit(unit_id, to);
        // The move action: Stomp (40) or Tentacles (44) hit the neighbours at the destination.
        if abilities.contains(&unit_ability::STOMP) || abilities.contains(&44) {
            self.area_damage(unit_id);
        }
        match embark {
            1 => self.embark(unit_id),
            2 => self.disembark(unit_id),
            _ => {}
        }
        if abilities.contains(&unit_ability::ALGAE) {
            self.algae_trail(unit.owner, &path);
        }
        // Stepping into a foreign city counts as an attack on its owner.
        if let Some(t) = self.tile(to) {
            if t.improvement == improvement::CITY && t.owner != 0 && t.owner != unit.owner {
                self.note_attack(unit.owner, t.owner);
            }
        }
        self.check_step_on_poison(&unit, to);
        self.tentacle_hit_then_bubble_step(unit_id, to, tentacle_hit);
        // The autofreeze of a Mooni or Gaami: a move that is not a push freezes the tiles and units round the destination.
        if abilities.contains(&unit_ability::AUTO_FREEZE) {
            self.freeze_area(unit.owner, to, 1, true, false);
        }
        self.drop_bubble_on_dry_land(unit_id, to);
        if abilities.contains(&unit_ability::TENTACLES) {
            self.update_tentacles();
        }
        // The move's last step: a follower that is no longer next to the new tile follows onto the tile the unit left.
        self.followers_follow(unit_id, unit.coordinates, to);
        if abilities.contains(&unit_ability::AUTO_FLOOD) {
            self.flood_tile_action(unit.owner, to);
        }
    }

    /// The game's flood-tile action, queued by a move of a unit with Autoflood onto its destination: it is only valid on a
    /// floodable tile (a field or forest that is not flooded yet). The tile floods for `player`, the routes and the
    /// player's city connections are updated, and a unit standing on the tile gets the bubble step.
    pub(super) fn flood_tile_action(&mut self, player: u8, at: Wc) {
        let Some(index) = self.tile_index(at) else { return };
        if !self.floodable(index as i32) {
            return;
        }
        self.flood_tile(player, at);
        self.update_routes();
        self.update_city_connections(player);
        if let Some(id) = self.unit_at(at).map(|u| u.id) {
            self.check_bubble_step(id, at);
        }
    }

    /// The bubble step of a move (and of a tile flooding under a unit): a player with the Bubble ability (the
    /// Waterways tech) whose Amphibious unit stands on a flooded tile gives the unit the Bubble effect, which adds one
    /// movement point. Retail runs it after the poison step.
    pub(super) fn check_bubble_step(&mut self, unit_id: u32, at: Wc) {
        if self.bubble_step_due(unit_id, at) {
            self.give_bubble(unit_id);
        }
    }

    /// Whether the bubble step queues a Bubble action: retail looks at the unit's Bubble effect when it
    /// queues the action, not when the action runs.
    fn bubble_step_due(&self, unit_id: u32, at: Wc) -> bool {
        let Some(unit) = self.unit(unit_id) else { return false };
        self.has_player_ability(unit.owner, player_ability::BUBBLE)
            && self.has_effect(at, Self::EFFECT_FLOODED)
            && self.has_ability(unit, unit_ability::AMPHIBIOUS)
            && !unit.bubble
    }

    fn give_bubble(&mut self, unit_id: u32) {
        if let Some(i) = self.unit_index(unit_id) {
            self.s.units[i].bubble = true;
        }
    }

    /// The tentacle hit and the bubble step of a move, in the order the action stack runs them: the bubble step is
    /// decided before the hit lands (a unit that already had the bubble queues nothing, so the hit takes it away) and
    /// gives the bubble after it.
    pub(super) fn tentacle_hit_then_bubble_step(&mut self, unit_id: u32, to: Wc, hit: Option<(u8, Wc, i32)>) {
        let due = self.bubble_step_due(unit_id, to);
        self.land_tentacle_hit(to, hit);
        if due {
            self.give_bubble(unit_id);
        }
    }

    /// The tentacle update (version 105 on): every tile loses the Tentacle effect, then every unit with Tentacles (44) that is
    /// not frozen puts it on the eight tiles around it. Retail runs it after a move, a train, a disband, a freeze, a
    /// kill and the end of a turn whenever the unit concerned has Tentacles.
    pub(super) fn update_tentacles(&mut self) {
        if self.s.version < 105 {
            return;
        }
        let bit = 1u16 << Self::EFFECT_TENTACLE;
        for t in &mut self.s.map.tiles {
            t.effects &= !bit;
        }
        let sources: Vec<Wc> = self
            .s
            .units
            .iter()
            .filter(|u| !u.frozen && self.has_ability(u, unit_ability::TENTACLES))
            .map(|u| u.coordinates)
            .collect();
        for at in sources {
            for n in self.area(at, 1) {
                if let Some(t) = self.tile_mut(n) {
                    t.effects |= bit;
                }
            }
        }
    }

    /// The tentacle unit that hits `player`'s unit on `target` (version above 106): among the
    /// eight neighbours, units of another player that is not at peace with `player`, has Tentacles and is not frozen;
    /// the one with the most health wins, the first in area order on a tie.
    fn tentacle_owner(&self, target: Wc, player: u8) -> Option<Wc> {
        if self.s.version <= 106 {
            return None;
        }
        let mut best: Option<(Wc, u16)> = None;
        for n in self.area(target, 1) {
            let Some(u) = self.unit_at(n) else { continue };
            if u.owner == player
                || self.at_peace(u.owner, player)
                || u.frozen
                || !self.has_ability(u, unit_ability::TENTACLES)
            {
                continue;
            }
            if best.is_none_or(|(_, h)| h < u.health) {
                best = Some((n, u.health));
            }
        }
        best.map(|b| b.0)
    }

    /// The tentacle step of a move, first half: a unit that ends on a Tentacle tile takes a hit from the strongest
    /// tentacle unit next to it (the full battle damage of that unit against it). Retail fixes the damage against the
    /// moving unit when the move runs, before a ship lets its passenger off or a land unit boards; returns the hitting
    /// player, the tile it hits from and the damage.
    pub(super) fn tentacle_hit(&self, unit_id: u32, to: Wc) -> Option<(u8, Wc, i32)> {
        if !self.has_effect(to, Self::EFFECT_TENTACLE) {
            return None;
        }
        let unit = self.unit(unit_id)?.clone();
        let from = self.tentacle_owner(to, unit.owner)?;
        let attacker = self.unit_at(from)?.clone();
        let q = self.battle_query(&attacker, &unit, false);
        let r = get_battle_results(&self.combat_unit(&attacker), &self.combat_unit(&unit), &q);
        Some((attacker.owner, from, r.attack_damage))
    }

    /// The second half: the hit lands on whatever stands on the tile when the rest of the move is done.
    pub(super) fn land_tentacle_hit(&mut self, to: Wc, hit: Option<(u8, Wc, i32)>) {
        if let Some((owner, from, damage)) = hit {
            self.perform_attack(owner, from, to, damage);
        }
    }

    /// The last bubble rule of a move: a unit with the Bubble effect that is Amphibious and ends on a land tile that
    /// is not wetland (neither flooded nor algae) loses it.
    fn drop_bubble_on_dry_land(&mut self, unit_id: u32, at: Wc) {
        let Some(i) = self.unit_index(unit_id) else { return };
        let Some(t) = self.tile(at) else { return };
        let dry = crate::rules::path::is_land(t.terrain)
            && t.effects & ((1 << Self::EFFECT_FLOODED) | (1 << Self::EFFECT_ALGAE)) == 0;
        if self.s.units[i].bubble
            && dry
            && self.unit_info(&self.s.units[i]).is_some_and(|x| x.abilities.contains(&unit_ability::AMPHIBIOUS))
        {
            self.s.units[i].bubble = false;
        }
    }

    /// A move that ends on a tile holding another unit (hidden from the mover, so the path allowed it): nothing moves
    /// and the mover is not spent. The reveal action then clears the Invisible effect of the unit on the tile, turns
    /// the mover to face it and, from version 106, introduces the two owners when the mover's owner does not know the
    /// other one yet.
    fn bump_hidden_unit(&mut self, mover: usize, hidden: usize) {
        let (owner, from) = (self.s.units[mover].owner, self.s.units[mover].coordinates);
        let (other, at) = (self.s.units[hidden].owner, self.s.units[hidden].coordinates);
        self.s.units[hidden].invisible = false;
        self.s.units[mover].direction = Self::direction_of(from, at);
        if self.s.version > VERSION_REVEAL_MEETS && !self.knows_player(owner, other) {
            self.meet(owner, other);
        }
    }

    /// The Algae trail of a unit with the Algae ability (the Cymanti island): every water tile of the move path,
    /// the tile it leaves and the tile it reaches included, that has no Algae yet gets the effect, as a free build of
    /// the algae spawn improvement for the unit's owner. Only a move leaves a trail; a push does not.
    fn algae_trail(&mut self, owner: u8, path: &[Wc]) {
        for &c in path {
            let wet = self
                .tile(c)
                .is_some_and(|t| crate::rules::path::is_water(t.terrain) && t.effects & (1 << Self::EFFECT_ALGAE) == 0);
            if wet {
                self.build_improvement(owner, c, improvement::ALGAE_SPAWN, false);
            }
        }
    }

    /// Poison a unit that steps onto a poisonous improvement, called by the move with the destination. An improvement with
    /// the Poison ability poisons a unit whose tribe lacks poison resistance and that cannot Fly. The queued
    /// poison action (acting player = tile owner) runs after the rest of the move, so it hits whatever unit then
    /// stands on the tile (a disembarked unit, or a boat and its passenger). Up to version 82 a 20-damage
    /// attack action, queued after it, runs first.
    pub(super) fn check_step_on_poison(&mut self, unit: &UnitState, to: Wc) {
        let Some(tile) = self.tile(to).copied() else { return };
        let poison = tile.improvement != 0
            && self.t.improvement(tile.improvement).is_some_and(|i| i.abilities.contains(&improvement_ability::POISON));
        if !poison || self.has_tribe_ability(unit.owner, "poisonresist") || self.has_ability(unit, unit_ability::FLY) {
            return;
        }
        if self.s.version <= 0x52 {
            self.perform_attack(unit.owner, to, to, 20);
        }
        self.poison_unit_action(tile.owner, to);
    }

    /// Poison action: up to version 101 only the unit on the target tile gains Poisoned.
    /// After that it also poisons the unit's passenger, and an acting player with the Cute skin
    /// (19) adds Charmed. There is no poison resistance check here.
    pub(super) fn poison_unit_action(&mut self, player: u8, at: Wc) {
        let new = self.s.version > 0x65;
        let charm = new && self.player_runtime(player).is_some_and(|p| p.skin == 19);
        let Some(i) = self.s.units.iter().position(|u| u.coordinates == at) else { return };
        let u = &mut self.s.units[i];
        u.poisoned = true;
        if !new {
            return;
        }
        if charm {
            u.charmed = true;
        }
        if let Some(p) = u.passenger.as_mut() {
            p.poisoned = true;
        }
    }

    /// Upgrade: pay the new unit's cost, train a fresh unit of the new type (new ids burnt
    /// like `embark`) and copy the old unit's state over it: the id, passenger, home, flags and direction survive, xp and
    /// promotion do not. Carriers keep their health; others keep the damage taken. It also
    /// copies the effects list, so a poisoned or boosted unit stays so.
    fn do_upgrade(&mut self, me: u8, unit_id: u32, to: i32) {
        let Some(cost) = self.t.unit(to).map(|u| u.data.cost) else { return };
        self.upgrade_unit(me, unit_id, to, cost);
    }

    /// Upgrade with an explicit cost (a kill passes 0).
    pub(super) fn upgrade_unit(&mut self, me: u8, unit_id: u32, to: i32, cost: i32) {
        let Some(i) = self.unit_index(unit_id) else { return };
        let Some(info) = self.t.unit(to) else { return };
        let (carrier, max_hp) = (info.abilities.contains(&unit_ability::CARRY), info.data.health);
        self.add_currency(me, -cost);
        let old = self.s.units[i].clone();
        let _ = self.next_unit_id();
        if carrier {
            let _ = self.next_unit_id();
        }
        let old_max = self.max_health(&old);
        let climate = self.new_unit_climate(to, old.coordinates);
        let mut u = UnitState {
            id: old.id,
            owner: old.owner,
            birth_climate: climate,
            type_idx: to,
            previous_turn_end: old.coordinates,
            coordinates: old.coordinates,
            home: old.home,
            passenger: old.passenger.clone(),
            created_turn: self.s.current_turn as u16,
            moved: old.moved,
            attacked: old.attacked,
            direction: old.direction,
            follower: old.follower,
            frozen: old.frozen,
            poisoned: old.poisoned,
            boosted: old.boosted,
            invisible: old.invisible,
            bubble: old.bubble,
            petrified: old.petrified,
            swift: old.swift,
            double_ready: old.double_ready,
            charmed: old.charmed,
            ..UnitState::default()
        };
        u.health = if carrier {
            old.health
        } else {
            let lost = (old_max - i32::from(old.health)).min(max_hp - 1).max(0);
            (max_hp - lost) as u16
        };
        self.s.units[i] = u;
        self.explore_unit(unit_id);
    }

    /// Destroy command: the ruling city loses the population
    /// the improvement gave at its level, a required resource comes back, routes and connections refresh.
    pub(super) fn do_destroy(&mut self, me: u8, at: Wc) {
        let Some(tile) = self.tile(at).copied() else { return };
        if tile.improvement == 0 {
            return;
        }
        let Some(info) = self.t.improvement(tile.improvement) else { return };
        let level = self.improvement_rt(at).map(|r| i32::from(r.level)).unwrap_or(0);
        let growth: i32 = info.data.growth_rewards.iter().map(|g| g.population.unwrap_or(0)).sum();
        let pop = info.population + growth * level.max(0);
        let route = !info.data.routes.is_empty() || info.abilities.contains(&improvement_ability::BRIDGE);
        let resource = info.data.terrain_requirements.iter().find_map(|r| r.resource.clone());
        let ruling = tile.ruling_city;
        let idx = self.tile_index(at).map(|x| x as i32).unwrap_or(-1);
        self.s.improvements.retain(|r| r.tile != idx);
        if let Some(t) = self.tile_mut(at) {
            t.improvement = 0;
            // Destroying a route opener or a bridge also clears the tile's road; any other improvement leaves it.
            if route {
                t.has_road = false;
            }
        }
        if let Some(name) = resource {
            if let Some(&rid) = self.t.resource_idx.get(name.as_str()) {
                if let Some(t) = self.tile_mut(at) {
                    t.resource = rid;
                }
            }
        }
        if pop > 0 && ruling.x >= 0 && self.is_city(ruling) {
            for _ in 0..pop {
                self.add_population(ruling, -1);
            }
        }
        if route {
            self.update_routes();
            let ids: Vec<u8> = self.s.players.iter().map(|p| p.id).filter(|i| *i != NATURE).collect();
            for p in ids {
                self.update_city_connections(p);
            }
        }
        // The destroy action ends with the surrounding-area check: a sawmill that lost a lumber hut
        // drops a level at once, not at the next turn start.
        self.check_surrounding_area(at);
        let _ = me;
    }

    /// Surrounding-area check: update the improvement level of every adjacency building within 1
    /// tile (centre included). Each level lost runs the level-down action (version
    /// above 103): level - 1, one population decrease on the ruling city per growth population, then
    /// the surrounding-area check around that building again.
    pub(super) fn check_surrounding_area(&mut self, at: Wc) {
        let mut area = vec![at];
        area.extend(self.neighbors(at));
        for n in area {
            let Some(nt) = self.tile(n).copied() else { continue };
            // Only adjacency buildings take part; testing that first avoids a shared-count update on the tables.
            if self.t.improvement(nt.improvement).is_none_or(|i| i.data.adjacency_improvements.is_empty()) {
                continue;
            }
            let tables = self.t.clone();
            let Some(info) = tables.improvement(nt.improvement) else { continue };
            let Some(old) = self.improvement_rt(n).map(|r| i32::from(r.level)) else { continue };
            let new = self.improvement_level(n);
            for _ in old..new {
                self.improvement_level_up(n);
            }
            for _ in new..old {
                if self.improvement_rt(n).is_none_or(|r| r.level == 0) {
                    break;
                }
                if let Some(rt) = self.improvement_rt_mut(n) {
                    rt.level -= 1;
                }
                let pop: i32 = info.data.growth_rewards.iter().map(|g| g.population.unwrap_or(0)).sum();
                if pop > 0 && nt.ruling_city.x >= 0 && self.is_city(nt.ruling_city) {
                    for _ in 0..pop {
                        self.add_population(nt.ruling_city, -1);
                    }
                }
                self.check_surrounding_area(n);
            }
        }
    }

    /// Area damage: every neighbouring unit not at peace takes half the battle damage
    /// (full damage with Tentacles, 44); the attack counts against its owner.
    pub fn area_damage(&mut self, unit_id: u32) {
        if crate::env_var!("NO_AREA_DAMAGE").is_ok() {
            return;
        }
        let Some(attacker) = self.unit(unit_id).cloned() else { return };
        let full = self.has_ability(&attacker, 44);
        let a = self.combat_unit(&attacker);
        // Retail queues one attack action per neighbour, damage fixed when queued; the action stack then runs them
        // last-in first-out, so the hits land in reverse order (the attacker ends up facing the first neighbour).
        let mut hits = Vec::new();
        for n in self.neighbors(attacker.coordinates) {
            let Some(victim) = self.unit_at(n).cloned() else { continue };
            if victim.owner == attacker.owner || self.at_peace(attacker.owner, victim.owner) {
                continue;
            }
            let q = self.battle_query(&attacker, &victim, false);
            let r = get_battle_results(&a, &self.combat_unit(&victim), &q);
            if r.should_reset_pacifist_task {
                self.tasks_after_attack(attacker.owner);
                self.note_attack(attacker.owner, victim.owner);
            }
            let dmg = if full { r.attack_damage } else { r.floored_half_attack_damage() };
            hits.push((n, dmg));
        }
        for (n, dmg) in hits.into_iter().rev() {
            self.perform_attack(attacker.owner, attacker.coordinates, n, dmg);
        }
    }

    #[doc(hidden)]
    pub fn embark_pub(&mut self, unit_id: u32) {
        self.embark(unit_id)
    }

    #[doc(hidden)]
    pub fn perform_attack_pub(&mut self, origin: Wc, target: Wc, damage: i32) {
        let actor = self.current_player();
        self.perform_attack(actor, origin, target, damage)
    }

    /// Embark: a fresh boat is trained (new unit id, promotion 0, xp 0, created this turn) carrying the old
    /// unit unchanged. The boat starts with the passenger's health, home and flags.
    fn embark(&mut self, unit_id: u32) {
        let Some(i) = self.unit_index(unit_id) else { return };
        let owner = self.s.units[i].owner;
        // Giant -> Juggernaut, Dagger -> Pirate, Hide -> Cloak boat, else transport ship.
        let ty = self.s.units[i].type_idx;
        let hide = self.has_ability(&self.s.units[i].clone(), unit_ability::HIDE);
        let special = if crate::env_var!("OLD_EMBARK_BOAT").is_ok() {
            None
        } else if ty == 12 {
            Some(45)
        } else if ty == 37 {
            Some(40)
        } else if hide {
            Some(39)
        } else {
            None
        };
        let boat = match special {
            Some(b) => b,
            None => match self.tribe_unit(owner, "transportship") {
                Some(b) => b,
                None => return,
            },
        };
        let mut old = self.s.units[i].clone();
        let id = self.next_unit_id();
        // Retail burns a second id per embark (observed: gaps right after every boat id).
        let _ = self.next_unit_id();
        // Both the boat and the passenger record the tile of the boarding as their last turn-end tile, not the tile the
        // passenger started the turn on. A passenger is not visited by the end of turn, so this stays.
        old.previous_turn_end = old.coordinates;
        let mut u = old.clone();
        u.id = id;
        u.type_idx = boat;
        u.passenger = Some(Box::new(old));
        // The boat is a new unit: no xp, no promotion, created this turn. The passenger keeps all three
        // (6bcde: boat 163 is unpromoted, created turn 25, carrying veteran unit 26 from turn 7).
        u.xp = 0;
        u.promotion = 0;
        u.birth_climate = self.new_unit_climate(boat, u.coordinates);
        u.created_turn = self.s.current_turn as u16;
        u.moved = true;
        u.attacked = true;
        self.s.units[i] = u;
        // A Stomp boat (the Juggernaut) hits its neighbours.
        if self.t.unit(boat).is_some_and(|b| b.abilities.contains(&unit_ability::STOMP)) {
            self.area_damage(id);
        }
    }

    fn disembark(&mut self, unit_id: u32) {
        let Some(i) = self.unit_index(unit_id) else { return };
        let boat = self.s.units[i].clone();
        let Some(mut p) = boat.passenger.as_deref().cloned() else { return };
        p.coordinates = boat.coordinates;
        // Disembarking (every arm) copies the carrier's direction onto the passenger, and its health unless the
        // carrier has Protect, so healing done on the boat carries over.
        p.direction = boat.direction;
        p.moved = true;
        p.attacked = true;
        if !self.has_ability(&boat, unit_ability::PROTECT) {
            p.health = boat.health;
        }
        let (id, owner) = (p.id, p.owner);
        let age = self.s.current_turn.wrapping_sub(u32::from(p.created_turn));
        let grows = self.has_ability(&p, unit_ability::GROW);
        self.s.units[i] = p;
        // A unit that grows and lands more than two turns after it was made grows up at once: a free upgrade,
        // which takes a unit id.
        if grows && age > 2 {
            let into = self.unit(id).and_then(|u| self.unlocked_upgrades_for(owner, u).first().copied());
            if let Some(into) = into {
                self.upgrade_unit(owner, id, into, 0);
            }
        }
    }

    /// Diplomacy changes from an attack: records the attack turns, then queues
    /// an embassy destruction from attacker to defender and one from defender to attacker, so the embassies
    /// between the two players are gone.
    pub fn note_attack(&mut self, attacker: u8, defender: u8) {
        if defender == 0 || defender == NATURE || attacker == defender {
            return;
        }
        let turn = self.s.current_turn as i32;
        let me = crate::rules::diplomacy::get_relation_mut(&mut self.s, attacker, defender);
        me.previous_attack_turn = me.last_attack_turn;
        me.last_attack_turn = turn;
        let other = crate::rules::diplomacy::get_relation_mut(&mut self.s, defender, attacker);
        other.previous_attack_turn = turn;
        if crate::env_var!("NO_ATTACK_EMBASSY").is_err() {
            crate::rules::commands::destroy_embassy(&mut self.s, attacker, defender);
            crate::rules::commands::destroy_embassy(&mut self.s, defender, attacker);
        }
    }

    #[doc(hidden)]
    pub fn battle_query_pub(&self, attacker: &UnitState, defender: &UnitState, simple: bool) -> BattleQuery {
        self.battle_query(attacker, defender, simple)
    }

    /// [`Self::battle_query`] for callers that read only the damage fields of `get_battle_results`: the path
    /// search behind `can_path_onto_defender` feeds only `should_move_to_defeated_enemy_tile`, so it is skipped.
    pub(crate) fn battle_damage_query(&self, attacker: &UnitState, defender: &UnitState) -> BattleQuery {
        self.battle_query_base(attacker, defender, false)
    }

    fn battle_query(&self, attacker: &UnitState, defender: &UnitState, simple: bool) -> BattleQuery {
        BattleQuery {
            can_path_onto_defender: crate::env_var!("PATH_ALWAYS").is_ok()
                || self.can_path_onto(attacker, defender.coordinates),
            ..self.battle_query_base(attacker, defender, simple)
        }
    }

    /// Everything but the path search (`can_path_onto_defender` left false).
    fn battle_query_base(&self, attacker: &UnitState, defender: &UnitState, simple: bool) -> BattleQuery {
        let range = self.unit_range(defender);
        // The defender's own scan of the tiles it can hit: it skips tiles its owner has not explored, so a unit
        // attacked from a tile its owner has never seen does not retaliate.
        let in_options = {
            let info = self.unit_info(defender);
            match info {
                Some(info) => {
                    let mut probe = defender.clone();
                    probe.leader = 0;
                    crate::rules::path::attack_options(
                        &self.s,
                        &probe,
                        range,
                        true,
                        info.data.attack,
                        &info.abilities,
                        false,
                        |i| crate::rules::path::tile_being_captured(&self.s, i),
                        |_| false,
                    )
                    .contains(&attacker.coordinates)
                }
                None => false,
            }
        };
        BattleQuery {
            game_version: self.s.version,
            account_for_peace: true,
            has_active_peace_treaty: defender.owner == attacker.owner || self.at_peace(defender.owner, attacker.owner),
            simple_data: simple,
            attacker_in_attack_options: in_options,
            can_path_onto_defender: false,
        }
    }

    /// The attack command. It pushes actions onto the action stack, which runs last-in
    /// first-out, so the effects happen in the reverse of the order the command lists them: the damage
    /// first, then splash, then the step into the tile or the retaliation, then conversion.
    fn do_attack(&mut self, unit_id: u32, target: Wc) {
        let Some(ai) = self.unit_index(unit_id) else { return };
        if crate::env_var!("NO_INFILTRATE").is_err() && self.is_infiltrator(&self.s.units[ai].clone()) {
            self.do_infiltrate(unit_id, target);
            return;
        }
        let Some(di) = self.unit_index_at(target) else {
            self.drench(unit_id, target);
            return;
        };
        let attacker = self.s.units[ai].clone();
        let defender = self.s.units[di].clone();
        let a = self.combat_unit(&attacker);
        let d = self.combat_unit(&defender);
        let q = self.battle_query(&attacker, &defender, false);
        let results = get_battle_results(&a, &d, &q);
        if crate::env_var!("COMBAT_DEBUG").is_ok() {
            eprintln!(
                "COMBAT cmd {} atk {:?} def {:?}\n  a={:?}\n  d={:?}\n  q={:?}\n  r={:?}",
                self.s.current_command, attacker, defender, a, d, q, results
            );
        }
        self.note_attack(attacker.owner, defender.owner);
        // Retail's attack command starts the quiet streak again only when the battle says so (damage to a visible
        // unit); up to version 106 every attack does.
        if self.s.version <= 106 || results.should_reset_pacifist_task {
            self.tasks_after_attack(attacker.owner);
        }
        let abilities = self.unit_info(&attacker).map(|x| x.abilities.clone()).unwrap_or_default();
        let range = self.unit_range(&attacker);
        let damage = results.attack_damage;
        let killed = damage >= i32::from(defender.health);
        if crate::env_var!("XP_STAT").is_ok() {
            eprintln!(
                "ATTACK cmd {} unit {} type {} -> def {} type {} owner {} killed {} dmg {} dhp {} retal {}",
                self.s.current_command,
                attacker.id,
                attacker.type_idx,
                defender.id,
                defender.type_idx,
                defender.owner,
                killed,
                damage,
                defender.health,
                results.retaliation_damage
            );
        }
        let origin = attacker.coordinates;
        self.s.units[ai].direction = Self::direction_of(origin, target);
        // Retail's attack queues the flood for a Drench attacker whether or not a unit stands on the target (the last
        // action queued, so it runs first); the flood only takes a floodable tile.
        if self.s.version > VERSION_DRENCH_FLOODS && abilities.contains(&unit_ability::DRENCH) {
            self.flood_tile_action(attacker.owner, target);
        }

        // Retail builds the splash actions while it builds the attack, from the units as they stand before the damage
        // runs, so a victim that changes type when the attack kills its leader (a centipede segment that becomes the
        // head) still takes the damage worked out for the unit it was.
        let splash_hits: Vec<(Wc, i32)> =
            if abilities.contains(&unit_ability::SPLASH) && range >= 2 && !abilities.contains(&unit_ability::POISON) {
                self.neighbors(target)
                    .into_iter()
                    .rev()
                    .filter_map(|n| {
                        let victim = self.unit_at(n).cloned()?;
                        if victim.owner == attacker.owner || self.at_peace(attacker.owner, victim.owner) {
                            return None;
                        }
                        let vq = self.battle_query(&attacker, &victim, false);
                        Some((n, get_battle_results(&a, &self.combat_unit(&victim), &vq).floored_half_attack_damage()))
                    })
                    .collect()
            } else {
                Vec::new()
            };

        // 1. The attack itself.
        self.perform_attack(attacker.owner, origin, target, damage);
        if killed {
            let killer_xp = !(self.unit_info(&attacker).map(|x| x.data.hidden).unwrap_or(true)
                || abilities.contains(&unit_ability::CARRY)
                || (self.s.version >= 105 && abilities.contains(&unit_ability::STATIC)));
            if let Some(ai) = self.unit_index(unit_id) {
                if killer_xp {
                    self.s.units[ai].xp = self.s.units[ai].xp.saturating_add(1);
                }
            }
        }

        // A unit with the freeze ability (the Ice Archer) freezes the unit it hit, if it survived the damage, and does
        // nothing more: no splash, no step into the tile, no retaliation (the defender cannot hit back at it).
        if abilities.contains(&unit_ability::FREEZE) {
            self.freeze_unit(attacker.owner, target);
            self.after_attack_flags(unit_id, killed);
            return;
        }

        // 2. Splash from ranged attackers hits (or poisons) the units around the target. Victims are hit in reverse
        // neighbour order (the actions run last in, first out), so the attacker, which turns to each victim, ends
        // facing the first neighbour that held one.
        if abilities.contains(&unit_ability::SPLASH) && range >= 2 {
            if abilities.contains(&unit_ability::POISON) {
                for n in self.neighbors(target).into_iter().rev() {
                    let Some(victim) = self.unit_at(n).cloned() else { continue };
                    if victim.owner == attacker.owner || self.at_peace(attacker.owner, victim.owner) {
                        continue;
                    }
                    self.poison_unit_action(attacker.owner, n);
                }
            } else {
                for (n, hit) in splash_hits {
                    self.perform_attack(attacker.owner, origin, n, hit);
                }
            }
        }

        // 3. Step into the defeated unit's tile, or take the retaliation.
        if killed {
            if let Some(ai) = self.unit_index(unit_id) {
                let step_in = results.should_move_to_defeated_enemy_tile
                    && self.unit_info(&attacker).is_some_and(|x| x.data.range == 1);
                if step_in
                    && (abilities.iter().all(|a| *a != unit_ability::STATIC) || self.attacker_may_step(&attacker))
                {
                    let embark = self.will_embark(&self.s.units[ai].clone(), target);
                    self.s.units[ai].coordinates = target;
                    self.explore_unit(unit_id);
                    let tentacle_hit = self.tentacle_hit(unit_id, target);
                    // The step is a move: Stomp / Tentacles hit the neighbours of the new tile.
                    if crate::env_var!("NO_STEP_STOMP").is_err()
                        && (abilities.contains(&unit_ability::STOMP) || abilities.contains(&44))
                    {
                        self.area_damage(unit_id);
                    }
                    self.followers_follow(unit_id, origin, target);
                    // A carrier stepping onto the land it cleared lets its passenger off (move embark status 2);
                    // a land unit stepping onto a port it cleared embarks (status 1).
                    if embark == 2 {
                        self.disembark(unit_id);
                    } else if embark == 1 {
                        // The attack's flags are on the unit before it boards, so the passenger carries them.
                        self.after_attack_flags(unit_id, killed);
                        self.embark(unit_id);
                    }
                    // The step is a move, so a unit with the Algae ability leaves its trail on the tiles it leaves and reaches.
                    if abilities.contains(&unit_ability::ALGAE) {
                        self.algae_trail(attacker.owner, &[origin, target]);
                    }
                    // The step is a move action like any other, so it checks for poison on the tile it steps into. The
                    // check is the mover's, as in a move; the poison then hits whatever stands on the tile, so a
                    // passenger let off (or a boat taken on) carries it although the mover's id is gone.
                    self.check_step_on_poison(&attacker, target);
                    // The step is a move that is not a push, so a Mooni or Gaami freezes round the tile it steps into.
                    if abilities.contains(&unit_ability::AUTO_FREEZE) && self.unit_index(unit_id).is_some() {
                        self.freeze_area(attacker.owner, target, 1, true, false);
                    }
                    // The rest of the move action: the bubble step, a tentacle hit on a Tentacle tile, the bubble lost on dry
                    // land, the tentacles of a unit with Tentacles, and the flood of a unit with Autoflood.
                    self.tentacle_hit_then_bubble_step(unit_id, target, tentacle_hit);
                    self.drop_bubble_on_dry_land(unit_id, target);
                    if abilities.contains(&unit_ability::TENTACLES) {
                        self.update_tentacles();
                    }
                    if abilities.contains(&unit_ability::AUTO_FLOOD) && self.unit_index(unit_id).is_some() {
                        self.flood_tile_action(attacker.owner, target);
                    }
                }
            }
        } else if results.retaliation_damage > 0 && self.unit_index(unit_id).is_some() {
            // The attack command sets the attacker's flags itself, before the queued attack actions run, so a
            // retaliation that kills the attacker passes the spent flags on to a follower that takes its place. The
            // flags of a survivor are set below, after the rest of the attack.
            let before = self.unit(unit_id).map(|u| (u.moved, u.attacked, u.double_ready));
            self.after_attack_flags(unit_id, false);
            self.perform_attack(defender.owner, target, origin, results.retaliation_damage);
            if let (Some((moved, attacked, double_ready)), Some(i)) = (before, self.unit_index(unit_id)) {
                let u = &mut self.s.units[i];
                (u.moved, u.attacked, u.double_ready) = (moved, attacked, double_ready);
            }
            if self.unit_index(unit_id).is_none() {
                // The defender's retaliation killed the attacker: the kill pays xp like an attack.
                if crate::env_var!("NO_RETAL_XP").is_err() {
                    let def_ok = self
                        .unit_info(&defender)
                        .map(|x| {
                            !(x.data.hidden
                                || x.abilities.contains(&unit_ability::CARRY)
                                || (self.s.version >= 105 && x.abilities.contains(&unit_ability::STATIC)))
                        })
                        .unwrap_or(false);
                    if def_ok {
                        if let Some(di) = self.unit_index(defender.id) {
                            self.s.units[di].xp = self.s.units[di].xp.saturating_add(1);
                        }
                    }
                }
                return;
            }
        }

        // 3b. An Eat unit that killed something grows its tail.
        if killed && crate::env_var!("NO_EAT").is_err() && abilities.contains(&unit_ability::EAT) {
            if let Some(ai) = self.unit_index(unit_id) {
                let mut tail = self.s.units[ai].clone();
                while tail.follower != 0 {
                    let Some(next) = self.unit(tail.follower).cloned() else { break };
                    tail = next;
                }
                self.eat(tail.owner, tail.id);
            }
        }

        // 4. Conversion happens after the damage, when the target survived it.
        if abilities.contains(&unit_ability::CONVERT) && !killed && defender.owner != NATURE {
            self.convert_unit(unit_id, defender.id);
        }
        self.after_attack_flags(unit_id, killed);
        // The attack command queues a kill for a Consumed attacker (Ciru) first,
        // so it dies after the rest of the attack.
        if abilities.contains(&unit_ability::CONSUMED) {
            self.kill_unit(unit_id);
        }
    }

    /// An attack by a unit with Drench (version 105 on): an attack on a tile without a unit turns the unit
    /// to the tile and queues a flood-tile action there, which only floods a floodable tile. The attack spends the unit.
    fn drench(&mut self, unit_id: u32, target: Wc) {
        let Some(i) = self.unit_index(unit_id) else { return };
        let unit = self.s.units[i].clone();
        if self.s.version <= VERSION_DRENCH_FLOODS || !self.has_ability(&unit, unit_ability::DRENCH) {
            return;
        }
        self.s.units[i].boosted = false;
        self.s.units[i].direction = Self::direction_of(unit.coordinates, target);
        self.flood_tile_action(unit.owner, target);
        self.after_attack_flags(unit_id, false);
    }

    /// Direction of a step from `from` to `to` (x grows to the upper right, y to the upper left):
    /// the direction with the largest dot product, orthogonal
    /// directions weighted 0x571 and diagonals 0x3d9 (first maximum wins; a non-positive best stays direction 0).
    pub fn direction_from_vector(vx: i32, vy: i32) -> u8 {
        let mut best = 0i32;
        let mut dir = 0u8;
        for d in 0..8u8 {
            let (dx, dy) = Self::direction_delta(d);
            let w = if dx * dx + dy * dy == 1 { 0x571 } else { 0x3d9 };
            let v = (dx * vx + dy * vy) * w;
            if v > best {
                best = v;
                dir = d;
            }
        }
        dir
    }

    /// The direction of the delta when the Chebyshev distance is under
    /// 2 (NONE, 8, for no step), otherwise `direction_from_vector`.
    pub fn direction_of(from: Wc, to: Wc) -> u8 {
        let (dx, dy) = (to.x - from.x, to.y - from.y);
        if dx.abs().max(dy.abs()) < 2 {
            path::direction_of_delta(dx, dy) as u8
        } else {
            Self::direction_from_vector(dx, dy)
        }
    }

    /// Direction deltas, indexed by direction: SW, W, NW, N, NE, E, SE, S.
    pub fn direction_delta(dir: u8) -> (i32, i32) {
        [(-1, -1), (-1, 0), (-1, 1), (0, 1), (1, 1), (1, 0), (1, -1), (0, -1)][usize::from(dir % 8)]
    }

    /// Home of a trained unit: a city tile is its own home. Otherwise, after
    /// version 110 no home (-1, -1); up to 110 the tile's ruling city, or the tile itself when it has none.
    pub(super) fn trained_unit_home(&self, at: Wc) -> Wc {
        let Some(t) = self.tile(at) else { return Wc { x: -1, y: -1 } };
        if t.improvement == improvement::CITY {
            return at;
        }
        if self.s.version > 110 {
            return Wc { x: -1, y: -1 };
        }
        if t.ruling_city.x >= 0 {
            t.ruling_city
        } else {
            at
        }
    }

    /// Train onto an occupied tile: the new unit takes the tile, the occupant steps aside
    /// in the direction it faces (the opposite way for a foreign unit), trying neighbouring directions in turn.
    pub fn train_on_occupied(&mut self, owner: u8, type_idx: i32, at: Wc, home: Wc) -> Option<u32> {
        self.push_occupant(owner, at);
        self.spawn_unit(owner, type_idx, at, home, true)
    }

    /// The push of a train onto an occupied tile: the occupant steps aside or, with no room, is killed.
    pub(super) fn push_occupant(&mut self, owner: u8, at: Wc) {
        if let Some(occ) = self.unit_at(at).cloned() {
            let mut base = if occ.owner != owner { (occ.direction + 4) % 8 } else { occ.direction };
            if let Some(chain) = self.chain_direction(&occ) {
                base = chain;
            }
            let abilities = self.unit_info(&occ).map(|i| i.abilities.clone()).unwrap_or_default();
            let allowed = self.allowed_terrain(&occ);
            // Retail's candidate order and test for a push: each neighbour without a unit is accepted when a
            // path of cost at most 1 to it exists for the occupant. There is no terrain pre-check (a land unit may
            // step onto a bridge over water).
            let settings = self.path_settings(&occ, &allowed, &abilities, false);
            let mut spot = None;
            for k in 1..=8i32 {
                let off = k / (if (k - 1) % 2 == 0 { 2 } else { -2 });
                let dir = ((i32::from(base) + off).rem_euclid(8)) as u8;
                let (dx, dy) = Self::direction_delta(dir);
                let c = Wc { x: at.x + dx, y: at.y + dy };
                if self.tile(c).is_none() || self.unit_at(c).is_some() {
                    continue;
                }
                let found = crate::rules::path::get_path(
                    &self.s,
                    at,
                    c,
                    1,
                    &settings,
                    |i| self.is_wetland_index(i),
                    |kind, ability| self.t.has_improvement_ability(kind, ability),
                    |i| self.blocked_bridge_direction(occ.owner, i),
                    |a, b| self.step_cost(&occ, &abilities, a, b),
                );
                if found.is_some() {
                    spot = Some(c);
                    break;
                }
            }
            match spot {
                Some(spot) => {
                    // The push queues a move (run before the queued train): a land unit
                    // pushed onto a port embarks, a boat pushed ashore disembarks.
                    let embark = self.will_embark(&occ, spot);
                    if let Some(i) = self.unit_index(occ.id) {
                        self.s.units[i].coordinates = spot;
                        self.s.units[i].direction = Self::direction_of(at, spot);
                    }
                    self.explore_unit(occ.id);
                    let tentacle_hit = self.tentacle_hit(occ.id, spot);
                    // The move runs with the reason Push, which skips the Stomp / Tentacles area damage.
                    match embark {
                        1 => self.embark(occ.id),
                        2 => self.disembark(occ.id),
                        _ => {}
                    }
                    self.finish_push_step(&occ, &abilities, spot, tentacle_hit);
                    // The follower's move (queued by the push's move, so run before the train) lands on the tile the
                    // pushed unit left, which the train is about to take: the train then pushes the follower aside too.
                    if occ.follower != 0 && self.unit_index(occ.id).is_some() {
                        self.followers_follow(occ.id, at, spot);
                        self.push_occupant(owner, at);
                    }
                }
                // Train onto an occupied tile: a failed push queues a kill, which runs before
                // the queued train.
                None => self.kill_unit(occ.id),
            }
        }
    }

    /// The facing of a pushed unit that is part of a centipede chain (as retail
    /// turns it): towards its follower, towards its leader, or between the two. It replaces the unit's own facing.
    fn chain_direction(&self, unit: &UnitState) -> Option<u8> {
        let toward = |id: u32| self.unit(id).map_or(0, |u| Self::direction_of(unit.coordinates, u.coordinates));
        match (unit.follower != 0, unit.leader != 0) {
            (true, true) => {
                let (a, b) = (toward(unit.leader), toward(unit.follower));
                let (da, db) = (Self::direction_delta(a), Self::direction_delta(b));
                let sum = (da.0 + db.0, da.1 + db.1);
                Some(if sum == (0, 0) { (a + 2) % 8 } else { Self::direction_from_vector(sum.0, sum.1) })
            }
            (true, false) => Some(toward(unit.follower)),
            (false, true) => Some(toward(unit.leader)),
            (false, false) => None,
        }
    }

    /// The rest of the move action that a push runs (retail queues a move with the reason Push, which differs from a
    /// command's move only in not freezing round the destination, not hitting the neighbours with Stomp or Tentacles
    /// and not using up the unit): poison, the bubble step,
    /// a tentacle hit, the bubble lost on dry land, the tentacles and the flood of the unit.
    fn finish_push_step(&mut self, occ: &UnitState, abilities: &[i32], spot: Wc, tentacle_hit: Option<(u8, Wc, i32)>) {
        if abilities.contains(&unit_ability::ALGAE) {
            self.algae_trail(occ.owner, &[occ.coordinates, spot]);
        }
        self.check_step_on_poison(occ, spot);
        self.tentacle_hit_then_bubble_step(occ.id, spot, tentacle_hit);
        self.drop_bubble_on_dry_land(occ.id, spot);
        if abilities.contains(&unit_ability::TENTACLES) {
            self.update_tentacles();
        }
        if abilities.contains(&unit_ability::AUTO_FLOOD) && self.unit_index(occ.id).is_some() {
            self.flood_tile_action(occ.owner, spot);
        }
    }

    /// Perform an attack: damage the unit on `target`, poison it when the attacker is a poisoner and
    /// the target survives (even when the damage rounds to 0), kill it at zero health. As in every attack variant,
    /// the unit on `origin` turns towards `target` and the one on `target` towards `origin`
    /// (so a retaliating defender faces its attacker); an attack from a unit's own tile (an explosion) leaves the victim
    /// facing no direction, as retail's direction setter does for two equal tiles.
    fn perform_attack(&mut self, actor: u8, origin: Wc, target: Wc, damage: i32) {
        let attacker_id = self.unit_at(origin).map(|u| u.id);
        let Some(di) = self.unit_index_at(target) else { return };
        if let Some(id) = attacker_id {
            if let Some(i) = self.unit_index(id) {
                self.s.units[i].boosted = false;
                self.s.units[i].direction = Self::direction_of(origin, target);
            }
        }
        self.s.units[di].direction = Self::direction_of(target, origin);
        let victim = self.s.units[di].clone();
        {
            let dm = &mut self.s.units[di];
            dm.boosted = false;
            dm.swift = false;
            dm.bubble = false;
            let dealt = damage.max(0).min(i32::from(dm.health)) as u16;
            dm.health -= dealt;
        }
        if self.s.units[di].health > 0 {
            if let Some(id) = attacker_id {
                let poisoner = self.unit(id).filter(|a| self.has_ability(a, unit_ability::POISON)).map(|a| a.owner);
                if let Some(owner) = poisoner {
                    self.poison_unit_action(owner, target);
                }
            }
            return;
        }
        // Killed. The kill goes to the acting player when the attack starts and ends on one tile (an explosion hits each
        // victim like that) or has no origin tile, else to the owner of the unit on the origin tile; a unit of Nature
        // is no kill.
        let killer = if origin == target || self.tile(origin).is_none() {
            Some(actor)
        } else {
            attacker_id.and_then(|id| self.unit(id)).map(|u| u.owner)
        };
        if let Some(owner) = killer.filter(|_| victim.owner != 255) {
            if let Some(rt) = self.s.player_runtime.iter_mut().find(|r| r.player == owner) {
                rt.kills += 1;
            }
            self.tasks_after_kill(owner);
        }
        if let Some(rt) = self.s.player_runtime.iter_mut().find(|r| r.player == victim.owner) {
            rt.casualties += 1;
        }
        self.kill_unit(victim.id);
    }

    /// Followers follow their leader. When a unit that has a follower has moved from `origin` to `dest` (a Move
    /// command, or an attack that steps into the defeated unit's tile), a follower that is no longer next to `dest` moves
    /// onto `origin`, the tile its leader left, and its own follower is then tested against the tile it now stands
    /// on, down the chain. A follower that is still next to `dest` stays where it is. The follower's move explores,
    /// turns it towards its new tile, marks it moved and applies a poisonous improvement. Versions up to 119 also
    /// queue a move of an adjacent follower onto its own tile, which is not ported.
    pub(super) fn followers_follow(&mut self, leader_id: u32, origin: Wc, dest: Wc) {
        let (mut leader_id, mut origin, mut dest) = (leader_id, origin, dest);
        // A chain is at most as long as the number of units; the bound only guards against a corrupt loop.
        for _ in 0..self.s.units.len() {
            let Some(follower) = self.unit(leader_id).map(|l| l.follower).filter(|f| *f != 0) else { return };
            let Some(fi) = self.unit_index(follower) else { return };
            let f = self.s.units[fi].clone();
            if Self::chebyshev(f.coordinates, dest) < 2 {
                return;
            }
            let sight = self.sight_range(&f);
            self.explore_from(f.owner, f.coordinates, sight);
            self.explore_from(f.owner, origin, sight);
            let u = &mut self.s.units[fi];
            u.direction = Self::direction_of(f.coordinates, origin);
            u.coordinates = origin;
            u.moved = true;
            self.check_step_on_poison(&f, origin);
            (leader_id, dest, origin) = (follower, origin, f.coordinates);
        }
    }

    /// Kill a unit. At once: a follower takes over the dead unit's moved/attacked flags, a
    /// leader loses its tail, a poisoned unit grows spores (`spawn_spore_resource`) and the unit leaves the tile. Queued
    /// (run last-in first-out): the spore actions, then an upgrade (owner, centipede, follower tile, cost 0), which
    /// turns the follower into a new head. Score comes from `refresh_scores`. A dead unit with Tentacles takes its tentacle tiles with it.
    pub(super) fn kill_unit(&mut self, unit_id: u32) {
        self.kill_unit_with_spores(unit_id, true);
    }

    /// `kill_unit`, leaving a poisoned unit's spores to the caller: returns the tile they are owed on. The caller
    /// grows them once the actions it queues above them have run.
    pub(super) fn kill_unit_deferring_spores(&mut self, unit_id: u32) -> Option<Wc> {
        self.kill_unit_with_spores(unit_id, false)
    }

    fn kill_unit_with_spores(&mut self, unit_id: u32, spores_now: bool) -> Option<Wc> {
        let victim = self.unit(unit_id).cloned()?;
        let follower =
            if victim.follower != 0 { self.unit_index(victim.follower).map(|j| self.s.units[j].id) } else { None };
        if let Some(f) = follower.and_then(|f| self.unit_index(f)) {
            self.s.units[f].moved = victim.moved;
            self.s.units[f].attacked = victim.attacked;
        }
        if victim.leader != 0 {
            if let Some(li) = self.unit_index(victim.leader) {
                self.s.units[li].follower = 0;
            }
        }
        self.remove_unit(unit_id);
        if self.has_ability(&victim, unit_ability::TENTACLES) {
            self.update_tentacles();
        }
        let owed = victim.poisoned.then_some(victim.coordinates);
        if spores_now {
            if let Some(at) = owed {
                self.spawn_spore_resource(at, Some(victim.owner));
            }
        }
        if let Some(f) = follower {
            self.upgrade_unit(victim.owner, f, 33, 0);
        }
        owed.filter(|_| !spores_now)
    }

    /// Spawn spores on a dead unit's tile (version > 114): a spore resource and, on water,
    /// a free algae build; last-in first-out, so the algae lands first and the spores replace its fruit.
    /// Creating the resource overwrites the resource and runs the surrounding-area check. Version >
    /// 118 acts as the current player; 115 to 118 as the tile owner, else the dead unit's owner. The
    /// pre-115 variant is not ported.
    pub(super) fn spawn_spore_resource(&mut self, at: Wc, unit_owner: Option<u8>) {
        let player = if self.s.version > 0x76 {
            self.current_player()
        } else {
            match self.tile(at).map(|t| t.owner).unwrap_or(0) {
                0 => unit_owner.unwrap_or(0),
                o => o,
            }
        };
        if self.is_water_tile(at) {
            self.build_improvement(player, at, 38, false);
        }
        if let Some(t) = self.tile_mut(at) {
            t.resource = 7;
        }
        self.check_surrounding_area(at);
    }

    /// Convert: the target unit changes to the converter's owner.
    fn convert_unit(&mut self, converter: u32, target: u32) {
        let (Some(ci), Some(ti)) = (self.unit_index(converter), self.unit_index(target)) else { return };
        let new_owner = self.s.units[ci].owner;
        if self.s.units[ti].owner != NATURE {
            self.tasks_after_convert(new_owner);
        }
        let escape = self.has_ability(&self.s.units[ci].clone(), unit_ability::ESCAPE);
        self.s.units[ci].moved = !escape;
        self.s.units[ci].attacked = true;
        let independent = self.has_ability(&self.s.units[ti].clone(), unit_ability::INDEPENDENT);
        let capital = self.capital_of(new_owner);
        let u = &mut self.s.units[ti];
        u.owner = new_owner;
        u.moved = true;
        u.attacked = true;
        if !independent {
            if let Some(cap) = capital {
                u.home = cap;
            }
        }
        // A carried passenger changes hands with its carrier, and takes the new capital as its home unless it is
        // independent (retail sets both on the carried unit as well).
        if let Some(p) = u.passenger.as_deref_mut() {
            p.owner = new_owner;
            if !independent {
                if let Some(cap) = capital {
                    p.home = cap;
                }
            }
        }
        self.explore_unit(target);
        // A converted centipede segment brings the rest of its chain over with it: each follower and leader that
        // belongs to somebody else is converted in turn, with the unit just converted as the converter.
        let (follower, leader) = (self.s.units[ti].follower, self.s.units[ti].leader);
        for next in [follower, leader] {
            if next != 0 && self.unit(next).is_some_and(|n| n.owner != new_owner) {
                self.convert_unit(target, next);
            }
        }
    }

    /// Eat: a new segment appears on a free, accessible tile next to the tail (the tile
    /// opposite its facing first, then the same alternating offsets as a pushed unit). It follows the tail. Like any trained
    /// unit it gets the home of its tile (none unless the tile is a city), not the tail's home, so it does not count
    /// against its owner's city's unit limit. Its facing is the one every new unit gets (towards the map centre), not
    /// the tail's, which the next Eat reads.
    fn eat(&mut self, owner: u8, tail_id: u32) {
        let Some(ti) = self.unit_index(tail_id) else { return };
        let tail = self.s.units[ti].clone();
        let Some(seg) = self.tribe_unit(owner, "segment") else { return };
        let base = (i32::from(tail.direction) + 4) % 8;
        for k in 1..=8i32 {
            let off = k / (if (k - 1) % 2 == 0 { 2 } else { -2 });
            let dir = ((base + off).rem_euclid(8)) as u8;
            let (dx, dy) = Self::direction_delta(dir);
            let c = Wc { x: tail.coordinates.x + dx, y: tail.coordinates.y + dy };
            let Some(t) = self.tile(c) else { continue };
            if self.unit_at(c).is_some() {
                continue;
            }
            let Some(info) = self.t.unit(seg) else { continue };
            if self.s.version > VERSION_EAT_ACCESSIBLE {
                if !self.segment_accessible(owner, seg, tail.coordinates, c) {
                    continue;
                }
            } else {
                let aquatic =
                    info.abilities.contains(&unit_ability::SWIM) || info.abilities.contains(&unit_ability::FLY);
                let water = crate::rules::path::is_water(t.terrain);
                if t.terrain == terrain::MOUNTAIN || (water && !aquatic) {
                    continue;
                }
            }
            let Some(id) = self.spawn_unit(owner, seg, c, Wc { x: -1, y: -1 }, true) else { return };
            if let Some(ni) = self.unit_index(id) {
                self.s.units[ni].leader = tail_id;
            }
            if let Some(ti) = self.unit_index(tail_id) {
                self.s.units[ti].follower = id;
            }
            return;
        }
    }

    /// Whether the new segment of an Eat could stand on `at`, next to the tail on `from`: the path finder's
    /// accessibility test for a unit of the owner's, so a mountain counts once the owner can climb.
    fn segment_accessible(&self, owner: u8, ty: i32, from: Wc, at: Wc) -> bool {
        let u = UnitState {
            id: 0,
            owner,
            type_idx: ty,
            coordinates: from,
            previous_turn_end: from,
            home: from,
            ..UnitState::default()
        };
        let allowed = self.allowed_terrain(&u);
        let abilities = self.unit_info(&u).map(|i| i.abilities.clone()).unwrap_or_default();
        let settings = self.path_settings(&u, &allowed, &abilities, false);
        let (Some(t), Some(o)) = (self.tile(at), self.tile(from)) else { return false };
        crate::rules::path::is_tile_accessible(
            &self.s,
            t,
            o,
            &settings,
            &|i| self.is_wetland_index(i),
            &|k, a| self.t.has_improvement_ability(k, a),
            &|i| self.blocked_bridge_direction(owner, i),
        )
    }

    fn attacker_may_step(&self, attacker: &UnitState) -> bool {
        let _ = attacker;
        true
    }

    /// Flags after an attack: `moved = !Escape`, `attacked = true` unless the unit can act twice; Persist can strike again after a kill.
    fn after_attack_flags(&mut self, unit_id: u32, killed: bool) {
        let Some(i) = self.unit_index(unit_id) else { return };
        let unit = self.s.units[i].clone();
        let abilities = self.unit_info(&unit).map(|x| x.abilities.clone()).unwrap_or_default();
        let escape = abilities.contains(&unit_ability::ESCAPE);
        let off_ice = self.tile(unit.coordinates).map(|t| t.terrain != terrain::ICE).unwrap_or(true);
        let u = &mut self.s.units[i];
        u.moved = !escape;
        if abilities.contains(&unit_ability::SKATE) && escape {
            u.moved = off_ice;
        }
        if u.double_ready {
            u.double_ready = false;
        } else {
            u.attacked = true;
        }
        if killed && abilities.contains(&unit_ability::PERSIST) {
            let again = {
                let probe = self.s.units[i].clone();
                !self.attack_targets_persist(&probe).is_empty()
            };
            if again {
                self.s.units[i].attacked = false;
                self.s.units[i].moved = true;
            }
        }
    }

    fn attack_targets_persist(&self, unit: &UnitState) -> Vec<Wc> {
        let mut probe = unit.clone();
        probe.attacked = false;
        self.attack_targets(&probe)
    }

    pub fn remove_unit(&mut self, id: u32) {
        self.s.units.retain(|u| u.id != id);
    }

    /// Make exhausted (version > 115): moved and attacked, the act-twice and Boosted flags removed,
    /// then every follower is marked moved and attacked. Older versions only get the two flags here
    /// (the pre-116 variant is not ported).
    fn make_exhausted(&mut self, i: usize) {
        let new = self.s.version > 0x73;
        let u = &mut self.s.units[i];
        u.moved = true;
        u.attacked = true;
        if !new {
            return;
        }
        u.double_ready = false;
        u.boosted = false;
        let mut next = u.follower;
        let mut guard = 0;
        while next != 0 && guard < 64 {
            guard += 1;
            let Some(j) = self.unit_index(next) else { break };
            self.s.units[j].moved = true;
            self.s.units[j].attacked = true;
            next = self.s.units[j].follower;
        }
    }

    fn do_capture(&mut self, unit_id: u32, at: Wc) {
        let Some(i) = self.unit_index(unit_id) else { return };
        let owner = self.s.units[i].owner;
        // The capture command exhausts the capturer.
        self.make_exhausted(i);
        // The command queues a reveal of the capturer when it is Invisible (from version 113), so a cloak that
        // takes a city stands visible again.
        if self.s.version > 0x70 {
            self.s.units[i].invisible = false;
        }
        let old_owner = self.tile(at).map(|t| t.owner).unwrap_or(0);
        // City capture: the previous owner's units lose this city as their home; the capturer moves in, unless it
        // is `independent` (a Polytaur, a dagger), which keeps its home and so takes none of the city's unit slots.
        for u in self.s.units.iter_mut() {
            if u.owner != owner && u.home == at {
                u.home = Wc { x: -1, y: -1 };
            }
        }
        if !self.has_ability(&self.s.units[i].clone(), unit_ability::INDEPENDENT) {
            self.s.units[i].home = at;
        }
        if crate::env_var!("CAPTURE_XP").is_ok() {
            let hidden_or_carry = self
                .unit_info(&self.s.units[i].clone())
                .map(|x| x.data.hidden || x.abilities.contains(&unit_ability::CARRY))
                .unwrap_or(true);
            if !hidden_or_carry {
                self.s.units[i].xp = self.s.units[i].xp.saturating_add(1);
            }
        }
        self.capture_city_at(owner, at);
        // The capture command pushes one city-connection update for [capturer, previous owner], then
        // a route update, then the city capture. The action stack pops from the end, so
        // the routes are rebuilt over the new territory before the connections are checked (ef32df78: a mycelium
        // connects a city in the turn it is captured). The update visits the capturer first, but each visit only
        // pushes its connection and population actions, which then run last in, first out: the previous owner's
        // population changes run before the capturer's.
        self.update_routes();
        if old_owner != 0 && old_owner != NATURE && old_owner != owner {
            self.update_city_connections(old_owner);
        }
        self.update_city_connections(owner);
    }

    fn do_train(&mut self, me: u8, city: Wc, unit_type: i32) {
        let Some(info) = self.t.unit(unit_type) else { return };
        let cost = info.data.cost;
        self.add_currency(me, -cost);
        let Some(id) = self.spawn_unit(me, unit_type, city, city, true) else { return };
        // A new unit with Tentacles (44) hits its neighbours before the tentacle tiles are recomputed.
        if self.s.version >= 105 && self.unit(id).is_some_and(|u| self.has_ability(u, unit_ability::TENTACLES)) {
            self.area_damage(id);
            self.update_tentacles();
        }
    }

    fn do_build(&mut self, me: u8, at: Wc, kind: i32) {
        self.build_improvement(me, at, kind, true);
    }

    /// Build an improvement, with a `deduct_cost` flag (spore spawning builds algae for free).
    fn build_improvement(&mut self, me: u8, at: Wc, kind: i32, deduct_cost: bool) {
        let tables = self.t.clone();
        let Some(info) = tables.improvement(kind) else { return };
        if deduct_cost {
            let cost = info.data.cost.unwrap_or(0);
            self.add_currency(me, -cost);
        }
        let ruling = self.tile(at).map(|t| t.ruling_city).unwrap_or(Wc { x: -1, y: -1 });
        let ab = |a: i32| info.abilities.contains(&a);
        let turn = self.s.current_turn as u16;
        let idx = self.tile(at).map(|t| t.index).unwrap_or(0);
        // The record's own owner field is obsolete in retail (the tile's owner is the owner) and the build action never
        // writes it, so a saved record shows the tile's owner: nobody for a bridge built outside every border.
        let record_owner = self.tile(at).map(|t| t.owner).unwrap_or(0);

        if kind == improvement::ROAD {
            if let Some(t) = self.tile_mut(at) {
                t.has_road = true;
            }
            self.routes_flagged = true;
        } else if !ab(improvement_ability::DISCRETE) && !ab(improvement_ability::CONSUMED) {
            self.s.improvements.retain(|r| r.tile != idx);
            let base_score: i32 = info.data.rewards.iter().map(|r| r.score_or_default().max(0)).sum();
            self.s.improvements.push(ImprovementRuntime {
                tile: idx,
                owner: record_owner,
                founder: me,
                level: 0,
                founded: turn,
                xp: 0,
                population: 0,
                production: 1,
                base_score: base_score as u16,
                border_size: info.data.border_size.unwrap_or(0) as u16,
                ..ImprovementRuntime::default()
            });
            if let Some(t) = self.tile_mut(at) {
                t.improvement = kind;
            }
            if ab(improvement_ability::PATINA) {
                let level = self.improvement_level(at);
                if let Some(rt) = self.improvement_rt_mut(at) {
                    rt.level = level.max(0) as u16;
                }
            }
            // A bridge's level field is its orientation: 1 when it runs horizontally (retail stores the
            // orientation there when the bridge is built).
            if kind == improvement::BRIDGE && self.blocked_bridge_direction(me, idx) == 3 {
                if let Some(rt) = self.improvement_rt_mut(at) {
                    rt.level = 1;
                }
            }
        }
        if ab(improvement_ability::UNIQUE) {
            if let Some(rt) = self.s.player_runtime.iter_mut().find(|r| r.player == me) {
                rt.built_unique_improvements.push(kind);
            }
        }
        // A consumed improvement (hunting, fishing, ...) also uses up the resource.
        if ab(improvement_ability::CONSUMED) {
            if let Some(t) = self.tile_mut(at) {
                if t.resource != 0 && kind != 52 {
                    t.resource = 0;
                }
            }
        }
        // `creates`: terrain, resource, unit, effect.
        for c in &info.data.creates {
            if let Some(name) = &c.terrain {
                if let Some(&ter) = self.t.terrain_idx.get(name) {
                    if let Some(t) = self.tile_mut(at) {
                        t.terrain = ter;
                        t.resource = 0;
                    }
                }
            }
            // A resource is created unless the builder can see one on the tile
            // (resource lookup), so a resource the builder cannot see (aquacrop under Algae) is replaced.
            if let Some(name) = &c.resource {
                if let Some(&res) = self.t.resource_idx.get(name) {
                    if self.get_resource(at, me).is_none() {
                        if let Some(t) = self.tile_mut(at) {
                            t.resource = res;
                        }
                    }
                }
            }
            // The build queues a train, which pushes an occupant aside
            // (see `train_on_occupied`): Enchant Animal under the builder's own unit.
            if let Some(name) = &c.unit {
                if let Some(&u) = self.t.unit_idx.get(name) {
                    let home = self.trained_unit_home(at);
                    self.train_on_occupied(me, u, at, home);
                }
            }
            if let Some(name) = &c.effect {
                let bit = match name.as_str() {
                    "algae" => Self::EFFECT_ALGAE,
                    "flooded" => Self::EFFECT_FLOODED,
                    _ => 0,
                };
                // Algae built by a player with the Cute skin also adds Foam.
                let foam = bit == Self::EFFECT_ALGAE && self.player_runtime(me).is_some_and(|p| p.skin == 19);
                if bit != 0 {
                    if let Some(t) = self.tile_mut(at) {
                        t.effects |= 1 << bit;
                        if foam {
                            t.effects |= 1 << Self::EFFECT_FOAM;
                        }
                    }
                }
            }
        }
        // An improvement with the Flood ability (the canal) queues a flood-tile action on the built tile.
        if ab(improvement_ability::FLOOD) {
            self.flood_tile_action(me, at);
        }
        // Roads, route openers, bridges, fills and any `creates` entry that
        // changes terrain or adds an effect re-run the route and city-connection passes.
        let reshapes = info.data.creates.iter().any(|c| c.terrain.is_some() || c.effect.is_some());
        if kind == improvement::ROAD {
            self.update_city_connections(me);
        } else if reshapes
            || self.is_route_opener(kind)
            || ab(improvement_ability::BRIDGE)
            || ab(improvement_ability::FILL)
        {
            self.refresh_routes_and_connections(me);
        }
        // The building's own population runs after the route and connection passes: retail adds the command's actions in
        // the order routes, connections, currency, population and runs them first to last. A port that connects a city
        // lets the connection's population level a city up first, and the port's own population waits for that reward.
        if info.population > 0 && self.is_city(ruling) {
            let is_capital = self.player(me).map(|p| p.start_tile == ruling).unwrap_or(false);
            let mode = crate::env_var!("PORT_MODE").ok().unwrap_or("none");
            let extra = if (kind == improvement::PORT || (kind == 21 && mode == "mine"))
                && (mode.is_empty() && is_capital || mode == "all" || (mode == "mine" && is_capital))
            {
                1
            } else {
                0
            };
            self.add_population(ruling, info.population + extra);
        }
        // Rewards.
        let currency: i32 = info.currency;
        if currency > 0 {
            self.add_currency(me, currency);
        }
        // From version 104 the surrounding-area check levels the new building and its neighbours through the action
        // stack. Before, an adjacency building starts at the level its neighbours give and pays each level's growth
        // reward.
        if self.s.version > super::turn::VERSION_LEVEL_UP_V103 {
            self.build_level_ups(at, me);
        } else if !info.data.adjacency_improvements.is_empty() && !ab(improvement_ability::PATINA) {
            let level = self.improvement_level(at);
            if level > 0 {
                if let Some(rt) = self.improvement_rt_mut(at) {
                    rt.level = level as u16;
                }
                for g in &info.data.growth_rewards {
                    let pop = g.population.unwrap_or(0);
                    if pop > 0 && self.is_city(ruling) {
                        self.add_population(ruling, pop * level);
                    }
                }
            }
        }
        // Buildings next to this one may grow (windmill next to a new farm, ...).
        if self.s.version <= super::turn::VERSION_LEVEL_UP_V103 {
            self.grow_neighbors(at, me);
        }
        if ab(improvement_ability::MANUAL) {
            if let Some(u) = self.unit_index_at(at) {
                self.s.units[u].moved = true;
                self.s.units[u].attacked = true;
            }
        }
        let sight: i32 = crate::env_var!("BUILD_SIGHT").ok().and_then(|v| v.parse().ok()).unwrap_or(0);
        if sight > 0 {
            self.explore_from(me, at, sight);
        }
    }

    /// The build's surrounding-area check: the level-ups around the new building (itself included) go on the action
    /// stack below the build's own actions, which have already run, and run from the top, so the last tile in area
    /// order levels up first. When a city reward is pending they wait below the actions the build already held.
    fn build_level_ups(&mut self, at: Wc, me: u8) {
        let mut ups: Vec<Deferred> =
            self.surrounding_level_ups(at).into_iter().map(Deferred::ImprovementLevelUp).collect();
        // A consumed resource (hunting) takes the level of the sanctuaries around it down again.
        ups.extend(self.surrounding_level_downs(at).into_iter().map(Deferred::ImprovementLevelDown));
        if ups.is_empty() {
            return;
        }
        if self.pending_trigger(me).is_some() {
            for d in ups.into_iter().rev() {
                self.hold(d);
            }
            return;
        }
        let base = self.deferred.len();
        self.deferred.extend(ups);
        self.run_deferred_above(base);
    }

    /// Neighbouring improvements whose adjacency level rose get their growth reward.
    fn grow_neighbors(&mut self, at: Wc, me: u8) {
        let new_kind = self.tile(at).map(|t| t.improvement).unwrap_or(0);
        if new_kind == 0 {
            return;
        }
        let Some(new_info) = self.t.improvement(new_kind) else { return };
        let _ = new_info;
        for n in self.neighbors(at) {
            let Some(nt) = self.tile(n) else { continue };
            if nt.owner != me || nt.improvement == 0 {
                continue;
            }
            if self.t.improvement(nt.improvement).is_none_or(|i| i.data.adjacency_improvements.is_empty()) {
                continue;
            }
            let tables = self.t.clone();
            let Some(info) = tables.improvement(nt.improvement) else { continue };
            let matches = info.data.adjacency_improvements.iter().any(|a| {
                a.improvement.as_ref().and_then(|nme| self.t.improvement_idx.get(nme)).copied() == Some(new_kind)
            });
            if !matches {
                continue;
            }
            let ruling = nt.ruling_city;
            let level = self.improvement_level(n);
            let max = self.improvement_max_level(n);
            let before = self.improvement_rt(n).map(|r| i32::from(r.level)).unwrap_or(0);
            if level > before && (max == 0 || before < max) {
                // The build queues these as improvement level-ups below the new building's own population
                // gain; when that gain raised a city reward, the stack pauses and they wait until it is answered.
                if self.pending_trigger(me).is_some_and(|t| t.peace_from.is_none()) {
                    for _ in before..level {
                        self.deferred.push(Deferred::ImprovementLevelUp(n));
                    }
                    continue;
                }
                if let Some(rt) = self.improvement_rt_mut(n) {
                    rt.level = level as u16;
                }
                for g in &info.data.growth_rewards {
                    let pop = g.population.unwrap_or(0);
                    if pop > 0 && self.is_city(ruling) {
                        self.add_population(ruling, pop * (level - before));
                    }
                }
            }
        }
    }

    pub fn grant_tech(&mut self, me: u8, tech: i32) {
        match self.s.tech.iter_mut().find(|r| r.player == me) {
            Some(row) => row.tech.push(tech),
            None => self.s.tech.push(crate::OwnedTech { player: me, tech: vec![tech] }),
        }
        self.tasks_after_research(me, tech);
        self.reveal_capitals_for_tech(me, tech);
    }

    fn do_research(&mut self, me: u8, tech: i32) {
        let price = self.tech_price(me, tech);
        self.add_currency(me, -price);
        match self.s.tech.iter_mut().find(|r| r.player == me) {
            Some(row) => row.tech.push(tech),
            None => self.s.tech.push(crate::OwnedTech { player: me, tech: vec![tech] }),
        }
        self.tasks_after_research(me, tech);
        self.reveal_capitals_for_tech(me, tech);
    }

    /// Learning a tech with the capital-vision ability (Diplomacy) makes the capitals of every player already met
    /// visible. Retail runs this in its tech-learning step, which both a research and a ruins free tech go through.
    fn reveal_capitals_for_tech(&mut self, me: u8, tech: i32) {
        if crate::env_var!("NO_TECH_REVEAL").is_err()
            && self.t.tech(tech).is_some_and(|t| {
                t.ability_unlocks.iter().any(|n| player_ability_id(n) == Some(player_ability::CAPITAL_VISION))
            })
        {
            let known: Vec<u8> = self
                .s
                .player_runtime
                .iter()
                .find(|r| r.player == me)
                .map(|r| r.known_players.clone())
                .unwrap_or_default();
            for o in known {
                if let Some(start) = self.player(o).map(|p| p.start_tile) {
                    if let Some(i) = self.tile_index(start) {
                        self.explore_tile(me, i);
                    }
                }
            }
        }
    }

    /// Ruins reward: shuffle `[1,2,8,6,3,4]` with
    /// `Range(i, n, x, y, i)` and take the first reward the player is eligible for.
    pub fn ruins_reward(&self, me: u8, at: Wc) -> i32 {
        let Some(tile) = self.tile(at) else { return 1 };
        // Above version 119 a ruin on an unowned ocean tile with room around it gives a Flooded tribe a Lost City, before
        // the shuffle (the Lost City is reward 9).
        if self.s.version > 119
            && !path::is_land(tile.terrain)
            && self.has_tribe_ability(me, "flooded")
            && self.valid_lost_city_location(at)
        {
            return 9;
        }
        let mut list = vec![1, 2, 8, 6, 3, 4];
        let n = list.len() as i32;
        let h = self.random_hash();
        for i in 0..(n - 1) {
            let j = h.range(i, n, &[at.x, at.y, i]).unwrap_or(i);
            list.swap(i as usize, j as usize);
        }
        if crate::env_var!("RUINS_DEBUG").is_ok() {
            eprintln!("RUINS-LIST at ({},{}) shuffled {:?}", at.x, at.y, list);
        }
        for item in list {
            match item {
                3 => {
                    let mut nb = self.area(at, 1);
                    nb.push(at);
                    let accessible = nb
                        .iter()
                        .filter(|c| self.tile(**c).is_some_and(|t| self.can_be_accessed_by_player(t, me)))
                        .count();
                    if accessible == 0 {
                        continue;
                    }
                    let mut r2 = self.area(at, 2);
                    r2.push(at);
                    if r2.iter().any(|c| !self.is_explored_at(me, *c)) {
                        return 3;
                    }
                }
                4 => {
                    if !self.researchable(me).is_empty() {
                        return 4;
                    }
                }
                6 => {
                    if path::is_water(tile.terrain) && self.unlocked_movement_terrain(me).contains(&terrain::OCEAN) {
                        return 6;
                    }
                }
                8 => {
                    // Retail asks for another living player, and Nature is always among the players and always
                    // alive, so in practice any land tile qualifies.
                    if !path::is_water(tile.terrain) && self.s.players.iter().any(|p| p.id != me && self.is_alive(p.id))
                    {
                        return 8;
                    }
                }
                other => return other,
            }
        }
        1
    }

    fn do_examine_ruins(&mut self, unit_id: u32) {
        let Some(i) = self.unit_index(unit_id) else { return };
        let (owner, at) = (self.s.units[i].owner, self.s.units[i].coordinates);
        let mut reward = self.ruins_reward(owner, at);
        if let Ok(f) = crate::env_var!("RUIN_FORCE") {
            for kv in f.split(',') {
                if let Some((c, r)) = kv.split_once(':') {
                    if c.parse::<u16>().ok() == Some(self.s.current_command) {
                        reward = r.parse().unwrap_or(reward);
                    }
                }
            }
        }
        if crate::env_var!("RUINS_DEBUG").is_ok() {
            let unexplored: Vec<(i32, i32)> =
                self.area(at, 2).into_iter().filter(|c| !self.is_explored_at(owner, *c)).map(|c| (c.x, c.y)).collect();
            eprintln!(
                "RUINS p{} at ({},{}) cmd {} reward {} next_unit {} techs {} unexplored_r2 {:?}",
                owner,
                at.x,
                at.y,
                self.s.current_command,
                reward,
                self.s.current_unit_id + 1,
                self.techs(owner).len(),
                unexplored
            );
        }
        // The command queues a reveal of the examiner when it is Invisible (it runs before the reward), so a unit
        // that hid before stands visible again, whatever the reward then does with it.
        self.s.units[i].invisible = false;
        // The ruin disappears and the unit is exhausted (`make_exhausted`).
        self.make_exhausted(i);
        if let Some(t) = self.tile_mut(at) {
            t.has_ruin = false;
            if t.improvement == improvement::RUIN {
                t.improvement = 0;
            }
        }
        let idx = self.tile_index(at).map(|x| x as i32).unwrap_or(-1);
        self.s.improvements.retain(|r| r.tile != idx);
        match reward {
            1 => self.add_currency(owner, 10),
            2 => {
                if let Some(cap) = self.capital_of(owner) {
                    self.add_population(cap, 3);
                }
            }
            3 => {
                // The explorer scout (a temporary unit that still takes an id) walks away and reveals the map.
                let _id = self.next_unit_id();
                let seed = self.random_hash().get_hash2(at.x, at.y);
                if crate::env_var!("OLD_SCOUT").is_ok() {
                    self.explorer_walk(owner, at, 15);
                } else {
                    self.run_scout(owner, at, seed);
                }
            }
            4 => {
                let techs = self.researchable(owner);
                if !techs.is_empty() {
                    let k = self.random_hash().range(0, techs.len() as i32, &[at.x, at.y]).unwrap_or(0) as usize;
                    if crate::env_var!("RUINS_DEBUG").is_ok() {
                        eprintln!("FREE TECH {} from {:?}", techs[k], techs);
                    }
                    self.grant_tech(owner, techs[k]);
                }
            }
            6 => {
                // Battleship reward unit: rammer ship from version 0x68 (bomber ship from 0x63), placed by
                // `train_on_occupied` like the Swordsman reward.
                let v = self.s.version;
                let ship = if v >= 0x68 {
                    44
                } else if v >= 0x63 {
                    41
                } else {
                    7
                };
                let name = self.t.unit(ship).map(|u| u.data.id.clone()).unwrap_or_default();
                if let Some(&ty) = self.t.unit_idx.get(self.tribe_unit_name(owner, &name).as_str()) {
                    let ship_id = self.train_on_occupied(owner, ty, at, Wc { x: -1, y: -1 });
                    // The id after the ship's belongs to a veteran Warrior riding in it (6bcde: ship 96, passenger 97,
                    // the ship at the Warrior's maximum health).
                    let warrior = self.t.unit_idx.get(self.tribe_unit_name(owner, "warrior").as_str()).copied();
                    // A ship that cannot carry (the Cymanti boomchi) has no passenger and burns no second id.
                    let carrier = self.t.unit(ty).is_some_and(|u| u.abilities.contains(&unit_ability::CARRY));
                    match (ship_id, warrior) {
                        (Some(ship_id), Some(w)) if crate::env_var!("SHIP_NOBURN").is_err() && carrier => {
                            let pid = self.next_unit_id();
                            let info_health = self.t.unit(w).map(|u| u.data.health as u16).unwrap_or(100);
                            let mut p = UnitState {
                                id: pid,
                                owner,
                                type_idx: w,
                                coordinates: at,
                                previous_turn_end: at,
                                home: Wc { x: -1, y: -1 },
                                health: info_health,
                                promotion: 1,
                                moved: true,
                                attacked: true,
                                created_turn: self.s.current_turn as u16,
                                ..UnitState::default()
                            };
                            if let Some(i) = self.unit_index(ship_id) {
                                p.direction = self.s.units[i].direction;
                                p.birth_climate = self.s.units[i].birth_climate;
                                let max = self.max_health(&p) as u16;
                                self.s.units[i].health = max;
                                self.s.units[i].passenger = Some(Box::new(p));
                            }
                        }
                        (_, _) if crate::env_var!("SHIP_NOBURN").is_err() && carrier => {
                            let _ = self.next_unit_id();
                        }
                        _ => {}
                    }
                }
            }
            9 => self.found_lost_city(owner, at),
            8 => {
                if crate::env_var!("PROMOTE_OLD").is_ok() {
                    self.promote_unit_at(at);
                }
                if let Some(&sw) = self.t.unit_idx.get(self.tribe_unit_name(owner, "swordsman").as_str()) {
                    // Home of a trained unit: a unit trained off a city has no home city. The examiner's push
                    // explores from its new tile; a city reward that raises stops the action stack, so the
                    // swordsman (and its promotion) wait under it.
                    let home = Wc { x: -1, y: -1 };
                    self.push_occupant(owner, at);
                    if self.reward_pending() {
                        self.hold(Deferred::Train { owner, unit_type: sw, at, home, veteran: true });
                    } else {
                        self.spawn_veteran(owner, sw, at, home);
                    }
                }
            }
            _ => {}
        }
    }

    /// Where a Lost City can rise: not on the map edge, an unowned ocean tile, and no city or ruin within two tiles.
    fn valid_lost_city_location(&self, at: Wc) -> bool {
        let Some(t) = self.tile(at) else { return false };
        crate::map::contents::distance_to_edge(&self.s.map, at) > 0
            && t.owner == 0
            && t.terrain == terrain::OCEAN
            && self.area(at, 2).into_iter().all(|c| {
                self.tile(c).is_some_and(|n| {
                    n.improvement != improvement::CITY && n.improvement != improvement::RUIN && !n.has_ruin
                })
            })
    }

    /// The Lost City reward (above version 110): the four tiles next to the ruin (not the diagonals) turn to shallow water (ice stays),
    /// the ruin's tile becomes a flooded field with a new neutral city that the player captures, and resources are
    /// drawn for the free tiles around it (every tile of radius 1, then a third of the rest of radius 2) with the
    /// player's tribe weights. The routes and city connections are then updated like after a capture.
    fn found_lost_city(&mut self, owner: u8, at: Wc) {
        for c in self.area(at, 1).into_iter().filter(|c| c.x == at.x || c.y == at.y) {
            if let Some(t) = self.tile_mut(c) {
                if t.terrain != terrain::ICE {
                    t.terrain = terrain::WATER;
                    t.altitude = -1;
                }
            }
        }
        if let Some(t) = self.tile_mut(at) {
            t.terrain = terrain::FIELD;
            t.improvement = improvement::CITY;
            t.is_city = true;
            t.effects |= 1 << Self::EFFECT_FLOODED;
        }
        self.new_city_runtime(at, 0);
        // The server's record of a lost city keeps `founded` at 0.
        if let Some(rt) = self.improvement_rt_mut(at) {
            rt.founded = 0;
        }
        let tribe = self.player(owner).map(|p| p.tribe).unwrap_or(0);
        if let Some(modifiers) = self.t.logic.tribe_by_idx(tribe).map(crate::map::terrain::tribe_continent_modifiers) {
            let spawns = crate::map::generate::resource_spawns(&self.t.logic);
            let hash = self.random_hash();
            let mut rng = crate::rng::DotNetRandom::new(0);
            let inner = self.area(at, 1);
            let ring: Vec<Wc> = self.area(at, 2).into_iter().filter(|c| !inner.contains(c)).collect();
            for (cells, share) in [(inner, 1.0_f32), (ring, 1.0 / 3.0)] {
                for c in cells {
                    let Some(i) = self.tile_index(c) else { continue };
                    let t = &self.s.map.tiles[i];
                    if hash.value2(c.x, c.y) > share || t.resource != 0 || t.improvement != 0 || t.owner != 0 {
                        continue;
                    }
                    crate::map::contents::add_resource(
                        &mut self.s.map,
                        i,
                        &spawns,
                        Some(&modifiers),
                        &mut rng,
                        Some(&hash),
                    );
                }
            }
        }
        self.capture_city_at(owner, at);
        // The examiner standing on the ruins moves into the city it founded, as a capturer does.
        if let Some(i) = self.unit_index_at(at) {
            if self.s.units[i].owner == owner && !self.has_ability(&self.s.units[i].clone(), unit_ability::INDEPENDENT)
            {
                self.s.units[i].home = at;
            }
        }
        self.update_routes();
        self.update_city_connections(owner);
    }

    /// Spawn the ruins' swordsman: it comes out promoted once, at full health.
    pub(super) fn spawn_veteran(&mut self, owner: u8, type_idx: i32, at: Wc, home: Wc) {
        if let Some(id) = self.spawn_unit(owner, type_idx, at, home, true) {
            if crate::env_var!("PROMOTE_OLD").is_err() {
                if let Some(i) = self.unit_index(id) {
                    self.s.units[i].promotion += 1;
                    let max = self.max_health(&self.s.units[i]);
                    self.s.units[i].health = max as u16;
                }
            }
        }
    }

    pub fn promote_unit_at(&mut self, at: Wc) {
        if let Some(i) = self.unit_index_at(at) {
            self.s.units[i].promotion += 1;
            let max = self.max_health(&self.s.units[i]);
            self.s.units[i].health = max as u16;
        }
    }

    /// Unit data name after the tribe's overrides.
    pub fn tribe_unit_name(&self, player: u8, name: &str) -> String {
        let tribe = self.player(player).map(|p| p.tribe).unwrap_or(0);
        self.t.tribe(tribe).and_then(|t| t.unit_overrides.get(name).cloned()).unwrap_or_else(|| name.to_string())
    }

    /// Players with no cities (or no units under the unit rule) are out.
    pub fn check_eliminations(&mut self) {
        let ids: Vec<u8> = self.s.players.iter().map(|p| p.id).filter(|id| *id != NATURE).collect();
        // Wiping a player changes no tile, so city-based liveness read once stays right through the loop.
        let alive = crate::rules::diplomacy::alive_by_id(&self.s);
        for id in ids {
            if !alive.map_or_else(|| self.is_alive(id), |a| a[usize::from(id)]) {
                // Wiping a player queues a paid disband for each of the player's units: half the full
                // cost back (when it is at least 2) and the unit score removed.
                let doomed: Vec<UnitState> = self.s.units.iter().filter(|u| u.owner == id).cloned().collect();
                for u in &doomed {
                    let cost = self.full_cost(u);
                    if cost >= 2 {
                        self.add_currency(id, cost / 2);
                    }
                    let score = self.unit_score(u);
                    if let Some(p) = self.player_mut(id) {
                        p.score = crate::rules::economy::remove_score(p.score, score);
                    }
                }
                if !doomed.is_empty() {
                    self.s.units.retain(|u| u.owner != id);
                }
                // Wiping a player (version > 59) then queues an embassy destruction (id, p) and
                // one (p, id) for every player: embassies in both directions are gone.
                if self.s.version > 0x3B {
                    for row in &mut self.s.diplomacy {
                        let holder = row.player;
                        for link in &mut row.relations {
                            if holder == id || link.other == id {
                                link.relation.embassy_level = 0;
                            }
                        }
                    }
                }
                // The wipe action records the wiper (the player whose command it is) and the wiped player: the wiper counts
                // one more wipe-out; the wiped player keeps the turn, the wiper's id and the index of the command on the stack.
                let (wiper, turn, command) =
                    (self.current_player(), self.s.current_turn, i32::from(self.s.current_command) - 1);
                let first =
                    self.s.player_runtime.iter().find(|r| r.player == id).is_some_and(|r| r.wiped_at_command < 0);
                if first {
                    if let Some(rt) = self.s.player_runtime.iter_mut().find(|r| r.player == id) {
                        rt.wiped_at_command = command;
                        rt.killed_turn = turn;
                        rt.killer_id = wiper;
                    }
                    if let Some(rt) = self.s.player_runtime.iter_mut().find(|r| r.player == wiper) {
                        rt.wipe_outs += 1;
                    }
                }
            }
        }
        self.check_game_over();
    }
}

use crate::rules::path;

/// One improvement kind in [`Engine::build_commands_with`], with its terrain and resource requirement as bit masks
/// (bit `id` set for each id in `terrain_req_ids`), or the list itself when an id does not fit in a mask.
struct BuildKind<'a> {
    kind: i32,
    info: &'a ImprovementInfo,
    freelance: bool,
    pontoon: bool,
    terrains: u64,
    resources: u64,
    slow: Option<&'a [(Option<i32>, Option<i32>)]>,
}

impl<'a> BuildKind<'a> {
    fn new(kind: i32, info: &'a ImprovementInfo, req: &'a [(Option<i32>, Option<i32>)], pontoon: bool) -> Self {
        let freelance = info.has(improvement_ability::FREELANCE);
        let bit = |id: i32| u32::try_from(id).ok().filter(|&b| b < 64).map(|b| 1u64 << b);
        let (mut terrains, mut resources, mut fits) = (0u64, 0u64, true);
        for &(res, ter) in req {
            if let Some(r) = res {
                match bit(r) {
                    Some(b) => resources |= b,
                    None => fits = false,
                }
            }
            if let Some(t) = ter {
                match bit(t) {
                    Some(b) => terrains |= b,
                    None => fits = false,
                }
            }
        }
        BuildKind { kind, info, freelance, pontoon, terrains, resources, slow: (!fits).then_some(req) }
    }

    /// The terrains a requirement entry may name to accept the tile ([`Engine::requirement_met`]), without and with
    /// the Pontoon substitution.
    fn tile_terrains(tile: &Tile, treehouse: bool) -> (u64, u64) {
        let ter = tile.terrain;
        let bit = |id: i32| u32::try_from(id).ok().filter(|&b| b < 64).map_or(0, |b| 1u64 << b);
        let wetland = tile.effects & ((1 << Engine::EFFECT_FLOODED) | (1 << Engine::EFFECT_ALGAE)) != 0;
        let flooded = tile.effects & (1 << Engine::EFFECT_FLOODED) != 0;
        let mut plain = bit(ter);
        if wetland && ter != terrain::FOREST {
            plain |= bit(terrain::WETLAND);
        }
        if flooded && ter == terrain::FOREST {
            plain |= bit(terrain::MANGROVE);
        }
        if ter == terrain::FOREST && treehouse {
            plain |= bit(terrain::FIELD);
        }
        let with_pontoon = if path::is_land(ter) { plain } else { plain | bit(terrain::FIELD) };
        (plain, with_pontoon)
    }

    fn requirement_met(&self, tile: &Tile, plain: u64, with_pontoon: u64, treehouse: bool) -> bool {
        if let Some(req) = self.slow {
            return Engine::requirement_met(req, tile, || self.pontoon, || treehouse);
        }
        let resource_ok = self.resources == 0
            || u32::try_from(tile.resource).ok().filter(|&b| b < 64).is_some_and(|b| self.resources & (1u64 << b) != 0);
        let tiles = if self.pontoon { with_pontoon } else { plain };
        resource_ok && (self.terrains == 0 || self.terrains & tiles != 0)
    }
}

/// For up to 64 [`BuildKind`]s (bit `i` is `kinds[i]`): which kinds each terrain and resource bit can satisfy, so
/// a tile's candidates are the kinds whose mask requirement it meets. Kinds with a `slow` list are always
/// candidates and are checked in full.
struct KindSets {
    by_terrain: [u64; 64],
    by_resource: [u64; 64],
    no_terrain: u64,
    no_resource: u64,
    pontoon: u64,
    slow: u64,
    freelance: u64,
    /// Kinds with the Manual ability.
    manual: u64,
    /// Kinds with the Discrete ability.
    discrete: u64,
    /// The Road kind.
    road: u64,
}

impl KindSets {
    fn new(kinds: &[BuildKind<'_>]) -> Self {
        let mut s = KindSets {
            by_terrain: [0; 64],
            by_resource: [0; 64],
            no_terrain: 0,
            no_resource: 0,
            pontoon: 0,
            slow: 0,
            freelance: 0,
            manual: 0,
            discrete: 0,
            road: 0,
        };
        for (i, k) in kinds.iter().enumerate() {
            let bit = 1u64 << i;
            if k.freelance {
                s.freelance |= bit;
            }
            if k.info.has(improvement_ability::MANUAL) {
                s.manual |= bit;
            }
            if k.info.has(improvement_ability::DISCRETE) {
                s.discrete |= bit;
            }
            if k.kind == improvement::ROAD {
                s.road |= bit;
            }
            if k.slow.is_some() {
                s.slow |= bit;
                continue;
            }
            if k.pontoon {
                s.pontoon |= bit;
            }
            if k.terrains == 0 {
                s.no_terrain |= bit;
            }
            if k.resources == 0 {
                s.no_resource |= bit;
            }
            let (mut t, mut r) = (k.terrains, k.resources);
            while t != 0 {
                s.by_terrain[t.trailing_zeros() as usize] |= bit;
                t &= t - 1;
            }
            while r != 0 {
                s.by_resource[r.trailing_zeros() as usize] |= bit;
                r &= r - 1;
            }
        }
        s
    }

    /// The kinds whose requirement the tile may meet: every mask kind that [`BuildKind::requirement_met`] accepts,
    /// and every slow kind.
    fn candidates(&self, tile: &Tile, plain: u64, with_pontoon: u64) -> u64 {
        let union = |mut bits: u64| {
            let mut m = 0u64;
            while bits != 0 {
                m |= self.by_terrain[bits.trailing_zeros() as usize];
                bits &= bits - 1;
            }
            m
        };
        let terrain = self.no_terrain | (union(plain) & !self.pontoon) | (union(with_pontoon) & self.pontoon);
        let resource = self.no_resource
            | u32::try_from(tile.resource).ok().filter(|&b| b < 64).map_or(0, |b| self.by_resource[b as usize]);
        (terrain & resource & !self.slow) | self.slow
    }
}
