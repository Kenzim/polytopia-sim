//! Freezing: the freeze-area action, what it may freeze, and the autofreeze of a unit that moves.
//!
//! A freeze-area action looks at every tile of a square round a centre. A tile that can be frozen turns to
//! Ice when it is water, and takes the freezing player's climate and skin. When the action freezes units, a
//! unit on the tile that can be frozen is frozen too: it counts as attacked and moved, so it can do nothing
//! until its owner has ended a turn. All checks are made before anything freezes, so the order of the tiles
//! does not change what is frozen.
//!
//! The two Polaris unit commands live here too: a Gaami's freeze-area command and the break-ice command of any
//! unit beside breakable ice. Every freeze and every break of ice checks the ice bank levels.
//!
//! See `docs/freezing.md`.

use super::*;

/// Skin id that petrifies a frozen unit as well.
const SKIN_MAGMA: i32 = 18;
/// Skin id of the swamp tiles, which a drain turns back.
const SKIN_SWAMP: i32 = 17;
/// Versions above this use the active-peace rule for units.
const VERSION_PEACE_UNITS: i32 = 99;
/// Versions above this treat water as always freezable.
const VERSION_WATER_ALWAYS: i32 = 79;
/// Versions above this freeze a tile of another climate or skin.
const VERSION_CLIMATE_FREEZES: i32 = 111;
/// Versions above this leave a flooded tile's flood in place.
const VERSION_FLOOD_STAYS: i32 = 119;
/// From this version a player's own tiles are checked for a climate to restore at the start of the turn, and a
/// tribe without the spread ability keeps the Polaris climate on ice and water.
const VERSION_RESTORE_CLIMATE: i32 = 112;
/// Versions above this let a unit that dashed (moved but not attacked) break ice, and make a broken tile clear its skin.
const VERSION_BREAK_ICE_DASH: i32 = 98;
/// Versions above this skip an ice tile with a unit on it that cannot swim.
const VERSION_BREAK_ICE_SWIM: i32 = 105;
/// Versions above this treat a unit hidden from the player as no obstacle when listing breakable ice.
const VERSION_BREAK_ICE_HIDDEN: i32 = 115;

impl Engine {
    /// The climate and skin a player's freezing paints on a tile.
    pub(crate) fn freezing_climate(&self, player: u8) -> (i32, i32) {
        self.s.player_runtime.iter().find(|r| r.player == player).map(|r| (r.climate, r.skin)).unwrap_or((0, 0))
    }

    /// Whether `unit` can be frozen by `player`: not the player's own unit, not a unit of a player at peace or in
    /// the cool-down after peace, and not one that is frozen already.
    pub(crate) fn unit_freezable(&self, unit: &UnitState, player: u8) -> bool {
        let friendly = if self.s.version > VERSION_PEACE_UNITS {
            unit.owner == player
                || self.at_peace(player, unit.owner)
                || crate::rules::diplomacy::has_cool_down_with(&self.s, player, unit.owner)
        } else {
            unit.owner == player || self.at_peace(player, unit.owner)
        };
        !friendly && !unit.frozen
    }

    /// Whether the tile at `at` can be frozen by `player`. With `only_owned` the tile must belong to the player.
    ///
    /// Water always can. So can a flooded tile, a tile of another climate or skin than the player's, and a tile
    /// whose unit (as the player sees it) can be frozen. Up to version 111 a tile of another climate counts only
    /// when it is unowned or the player's own.
    pub(crate) fn tile_freezable(&self, at: Wc, player: u8, only_owned: bool) -> bool {
        let Some(t) = self.tile(at) else { return false };
        if only_owned && t.owner != player {
            return false;
        }
        let version = self.s.version;
        let seen = self.unit_at(at).filter(|u| !u.invisible || u.owner == player || self.at_peace(player, u.owner));
        let is_water = t.terrain == terrain::WATER || t.terrain == terrain::OCEAN;
        if version <= VERSION_WATER_ALWAYS && is_water {
            return self.unit_at(at).is_none();
        }
        if version > VERSION_WATER_ALWAYS && (is_water || t.effects & (1 << Self::EFFECT_FLOODED) != 0) {
            return true;
        }
        let (climate, skin) = self.freezing_climate(player);
        if version > VERSION_CLIMATE_FREEZES {
            if t.climate != climate || t.skin != skin {
                return true;
            }
        } else if (t.owner == 0 || t.owner == player) && t.climate != climate {
            return true;
        }
        seen.is_some_and(|u| self.unit_freezable(u, player))
    }

    /// The freeze-area action: freeze the tiles and, with `freeze_units`, the units of the square of the given
    /// radius round `center`, for `player`. Returns how many tiles were freezable.
    pub(crate) fn freeze_area(
        &mut self,
        player: u8,
        center: Wc,
        radius: i32,
        freeze_units: bool,
        only_owned: bool,
    ) -> i32 {
        let mut tiles = self.area(center, radius);
        tiles.push(center);
        tiles.sort_by_key(|c| (c.y, c.x));
        // Everything is decided before the first tile freezes.
        let plan: Vec<(Wc, bool, bool)> = tiles
            .iter()
            .map(|&c| {
                let tile = self.tile_freezable(c, player, only_owned);
                let unit = freeze_units && self.unit_at(c).is_some_and(|u| self.unit_freezable(u, player));
                (c, tile, unit)
            })
            .collect();
        let mut frozen_tiles = 0;
        for (c, tile, unit) in plan {
            if unit {
                self.freeze_unit(player, c);
            }
            if tile {
                frozen_tiles += 1;
                self.freeze_tile(player, c);
            }
        }
        // The freeze-area action queues the route update and the player's city-connection update before its tile and
        // unit actions, so they run once those are done, whether or not anything froze.
        self.update_routes();
        self.update_city_connections(player);
        frozen_tiles
    }

    /// Flood a tile for `player` : it gets the Flooded effect, and the Swamped effect as well
    /// when the player's skin is the swamp skin.
    pub(crate) fn flood_tile(&mut self, player: u8, at: Wc) {
        let (_, skin) = self.freezing_climate(player);
        if let Some(t) = self.tile_mut(at) {
            t.effects |= 1 << Self::EFFECT_FLOODED;
            if skin == SKIN_SWAMP {
                t.effects |= 1 << Self::EFFECT_SWAMPED;
            }
        }
    }

    /// Drain a tile: it loses the Algae and Flooded effects, and the Swamped effect when the
    /// tile has the swamp skin.
    pub(crate) fn drain_tile(&mut self, at: Wc) {
        if let Some(t) = self.tile_mut(at) {
            t.effects &= !((1 << Self::EFFECT_ALGAE) | (1 << Self::EFFECT_FLOODED));
            if t.skin == SKIN_SWAMP {
                t.effects &= !(1 << Self::EFFECT_SWAMPED);
            }
        }
    }

    /// Freeze one tile: water becomes Ice, a flood is lifted, and the tile takes the player's climate and skin.
    pub(crate) fn freeze_tile(&mut self, player: u8, at: Wc) {
        if !self.tile_freezable(at, player, false) {
            return;
        }
        let (climate, skin) = self.freezing_climate(player);
        let version = self.s.version;
        let Some(t) = self.tile_mut(at) else { return };
        if t.terrain == terrain::WATER || t.terrain == terrain::OCEAN {
            t.terrain = terrain::ICE;
        }
        let flooded = t.effects & (1 << Self::EFFECT_FLOODED) != 0;
        if version > VERSION_FLOOD_STAYS {
            if flooded {
                t.effects &= !(1 << Self::EFFECT_SWAMPED);
            }
        } else if flooded {
            t.effects &= !((1 << Self::EFFECT_ALGAE) | (1 << Self::EFFECT_FLOODED));
            if t.skin == SKIN_SWAMP {
                t.effects &= !(1 << Self::EFFECT_SWAMPED);
            }
        }
        t.climate = climate;
        t.skin = skin;
        self.check_ice_bank_levels();
    }

    /// Freeze the unit on `at`, as an attack of `player` on its owner would: the relation notes the attack, the
    /// unit counts as moved and attacked, and the Pacifist task of the freezer starts again.
    pub(crate) fn freeze_unit(&mut self, player: u8, at: Wc) {
        let Some(i) = self.unit_index_at(at) else { return };
        let owner = self.s.units[i].owner;
        self.note_attack(player, owner);
        let (_, skin) = self.freezing_climate(player);
        let u = &mut self.s.units[i];
        u.moved = true;
        u.attacked = true;
        u.frozen = true;
        if skin == SKIN_MAGMA {
            u.petrified = true;
        }
        if self.has_ability(&self.s.units[i].clone(), unit_ability::TENTACLES) {
            self.update_tentacles();
        }
        self.tasks_mark_attacked(player);
    }

    /// Whether `climate` is the climate of a tribe with the spread ability (Polaris).
    pub(crate) fn climate_spreads(&self, climate: i32) -> bool {
        self.t.tribe(climate).is_some_and(|t| t.tribe_abilities.iter().any(|a| a == "spread"))
    }

    /// A climate that spreads is taken back when the tile is in the hands of a player of another tribe: the tile
    /// takes the owner's climate and skin. A player whose own climate spreads only has the skin brought into line.
    /// Water stays as it is. Without the spread ability, ice stays as well (from version 112).
    pub(crate) fn restore_climate(&mut self, player: u8, at: Wc) {
        let Some(t) = self.tile(at).copied() else { return };
        if !self.climate_spreads(t.climate) {
            return;
        }
        let (climate, skin) = self.freezing_climate(player);
        if self.climate_spreads(climate) {
            if !crate::rules::path::is_land(t.terrain) || t.skin == skin {
                return;
            }
        } else if self.s.version >= VERSION_RESTORE_CLIMATE
            && (!crate::rules::path::is_land(t.terrain) || t.terrain == terrain::ICE)
        {
            return;
        }
        if let Some(t) = self.tile_mut(at) {
            t.climate = climate;
            t.skin = skin;
        }
    }

    /// What a city capture does to the climate of the city's tiles. A player with the spread ability freezes every
    /// tile the city rules (water turns to Ice, the rest takes the player's climate and skin); any other player
    /// takes a spreading climate back.
    pub(crate) fn capture_climate(&mut self, player: u8, city: Wc) {
        let spreads = self.has_tribe_ability(player, "spread");
        for c in self.city_area(city) {
            if spreads {
                self.freeze_tile(player, c);
            } else {
                self.restore_climate(player, c);
            }
        }
    }

    /// The start of a player's turn: each tile the player owns gets its spreading climate restored.
    pub(crate) fn restore_climate_at_turn_start(&mut self, player: u8) {
        if self.s.version < VERSION_RESTORE_CLIMATE {
            return;
        }
        let owned: Vec<Wc> = self.s.map.tiles.iter().filter(|t| t.owner == player).map(|t| t.coordinates).collect();
        for c in owned {
            self.restore_climate(player, c);
        }
    }

    /// Set every ice bank's level to the number of frozen tiles on the map divided by 20 (at most the bank's maximum
    /// level), one level at a time, as retail does after a tile freezes, after ice breaks and after a build.
    pub(crate) fn check_ice_bank_levels(&mut self) {
        let banks: Vec<Wc> = self
            .s
            .map
            .tiles
            .iter()
            .filter(|t| t.improvement == crate::rules::economy::ICE_BANK_IMPROVEMENT)
            .map(|t| t.coordinates)
            .collect();
        if banks.is_empty() {
            return;
        }
        let Some(max) =
            self.t.improvement(crate::rules::economy::ICE_BANK_IMPROVEMENT).map(|i| i.data.max_level.unwrap_or(0))
        else {
            return;
        };
        let frozen =
            self.s.map.tiles.iter().filter(|t| crate::rules::economy::tile_is_frozen(t.terrain, t.climate)).count()
                as i32;
        let target = crate::rules::economy::ice_bank_level(frozen, max).max(0);
        for c in banks {
            let Some(level) = self.improvement_rt(c).map(|r| i32::from(r.level)) else { continue };
            for _ in level..target {
                self.improvement_level_up(c);
            }
            if level > target {
                if let Some(rt) = self.improvement_rt_mut(c) {
                    rt.level = target as u16;
                }
            }
        }
    }

    /// Whether the unit on `at`, as `viewer` sees it, hides from the viewer: it is invisible and neither the viewer's
    /// own nor at peace with the viewer.
    fn hidden_from(&self, u: &UnitState, viewer: u8) -> bool {
        u.invisible && u.owner != viewer && !self.at_peace(viewer, u.owner)
    }

    /// Whether `viewer` may break the ice on `at`. Only ice breaks. Up to version 98 a unit on it blocks; up to 105 a
    /// unit that swims does not; up to 115 a unit hidden from the viewer does not either; after that so does a unit
    /// that flies, and a tile with Algae or a bridge always breaks.
    pub(crate) fn has_breakable_ice(&self, at: Wc, viewer: u8) -> bool {
        let Some(t) = self.tile(at) else { return false };
        if t.terrain != terrain::ICE {
            return false;
        }
        let version = self.s.version;
        let Some(u) = self.unit_at(at) else { return true };
        if version <= VERSION_BREAK_ICE_DASH {
            return false;
        }
        if self.has_ability(u, unit_ability::SWIM) {
            return true;
        }
        if version <= VERSION_BREAK_ICE_SWIM {
            return false;
        }
        if version > VERSION_BREAK_ICE_HIDDEN
            && (t.effects & (1 << Self::EFFECT_ALGAE) != 0
                || t.improvement == improvement::BRIDGE
                || self.has_ability(u, unit_ability::FLY))
        {
            return true;
        }
        self.hidden_from(u, viewer)
    }

    /// Whether the unit can freeze the area round it: it has the freeze-area ability, has neither moved nor attacked,
    /// and some tile next to it can be frozen.
    pub fn can_freeze_area(&self, unit: &UnitState) -> bool {
        !unit.moved
            && !unit.attacked
            && self.has_ability(unit, unit_ability::FREEZE_AREA)
            && self.area_iter(unit.coordinates, 1).any(|c| self.tile_freezable(c, unit.owner, false))
    }

    /// Whether the unit can break ice: it leads no tail, has not attacked, and has not moved unless it has Dash; a
    /// unit with Auto-freeze cannot (before version 99 a unit with Freeze-area cannot); and breakable ice lies next to it.
    pub fn can_break_ice(&self, unit: &UnitState) -> bool {
        if unit.leader != 0 || unit.attacked {
            return false;
        }
        if self.s.version > VERSION_BREAK_ICE_DASH {
            if (unit.moved && !self.has_ability(unit, unit_ability::DASH))
                || self.has_ability(unit, unit_ability::AUTO_FREEZE)
            {
                return false;
            }
        } else if unit.moved || self.has_ability(unit, unit_ability::FREEZE_AREA) {
            return false;
        }
        self.area_iter(unit.coordinates, 1).any(|c| self.has_breakable_ice(c, unit.owner))
    }

    /// The freeze-area command: the unit is spent, then the area round it freezes, units included.
    pub(crate) fn do_freeze_area(&mut self, unit_id: u32) {
        let Some(i) = self.unit_index(unit_id) else { return };
        let (owner, at) = (self.s.units[i].owner, self.s.units[i].coordinates);
        self.s.units[i].moved = true;
        self.s.units[i].attacked = true;
        self.freeze_area(owner, at, 1, true, false);
    }

    /// The break-ice command: the unit is spent (and shows itself), then the ice in the square round it breaks.
    /// A broken tile becomes Ocean, or Water when land that is not ice lies beside it, and loses its skin. A tile
    /// holding a unit hidden from the player is not broken: the unit is revealed instead. Routes and city
    /// connections are updated afterwards.
    pub(crate) fn do_break_ice(&mut self, unit_id: u32) {
        let Some(i) = self.unit_index(unit_id) else { return };
        let (owner, at) = (self.s.units[i].owner, self.s.units[i].coordinates);
        self.s.units[i].moved = true;
        self.s.units[i].attacked = true;
        self.s.units[i].invisible = false;
        let mut tiles = self.area(at, 1);
        tiles.push(at);
        tiles.sort_by_key(|c| (c.y, c.x));
        let plan: Vec<Wc> = tiles.into_iter().filter(|&c| self.has_breakable_ice(c, owner)).collect();
        for c in plan {
            self.break_ice_tile(owner, c);
        }
        self.update_routes();
        self.update_city_connections(owner);
    }

    /// Break the ice on one tile for `player`, if it still can be.
    fn break_ice_tile(&mut self, player: u8, at: Wc) {
        let version = self.s.version;
        if version > VERSION_BREAK_ICE_SWIM {
            if let Some(i) = self.unit_index_at(at) {
                if self.hidden_from(&self.s.units[i], player) {
                    self.s.units[i].invisible = false;
                    return;
                }
            }
        }
        if self.has_breakable_ice(at, player) {
            let shallow = [(0, -1), (1, 0), (0, 1), (-1, 0)]
                .into_iter()
                .map(|(dx, dy)| Wc { x: at.x + dx, y: at.y + dy })
                .any(|n| {
                    self.tile(n).is_some_and(|t| crate::rules::path::is_land(t.terrain) && t.terrain != terrain::ICE)
                });
            if let Some(t) = self.tile_mut(at) {
                t.terrain = if shallow { terrain::WATER } else { terrain::OCEAN };
                if version > VERSION_BREAK_ICE_SWIM {
                    t.skin = 0;
                }
            }
        }
        self.check_ice_bank_levels();
    }
}
