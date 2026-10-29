//! The water of a tank over time and the tank filters (tank staff 7): the purity setter every
//! change of the purity goes through, the timed decay of the purity, and the filter that cleans
//! the water, charges its upkeep, wears out and asks the maintenance workers for a service.
//!
//! A tank records the filter that serves it: the first filter added to the map that faces it
//! while no live filter serves it, then each filter that updates while it serves none. A filter
//! whose own tile has become a tank tile (flooded) or whose tank is served by another live filter
//! facing it (doubled) is removed at its update, as the bulldozer removes it.

use super::{Shape, TankKind, World, piece_event};
use crate::build::number;
use crate::entity::{Catalogue, EntityType};
use crate::money::BUILDING_UPKEEP;
use crate::place::bulldozer::{refund, type_of};
use crate::render::order::Kind;
use crate::timing::file_time;
use crate::zoo::{Common, Entity, Event, FILTER_CHECK_MS, Filter};

/// The events a worn filter posts: decayed (a task) and broken (an urgent one).
const DECAYED: u16 = 10035;
const BROKEN: u16 = 10036;
/// The message of a tank whose water is too murky to see through (tank staff 7.4).
const TOO_MURKY: i32 = 10549;
/// The purity at and below which guests cannot see the animals (tank staff 7.4).
const EXTREME: u32 = 1;

/// The numbers of a filter type, with the program's value for a missing key (tank staff 7.1).
struct Keys {
    start: i64,
    decayed: i64,
    decay_ms: i64,
    delay_ms: i64,
    upkeep: f32,
    clean: i64,
    decayed_clean: i64,
}

impl Keys {
    fn of(ty: &EntityType) -> Keys {
        let key = |name: &str, fallback: i64| number(ty, name).map_or(fallback, |v| v as i64);
        Keys {
            start: key("cStartingHealth", 10),
            decayed: key("cDecayedHealth", 5),
            decay_ms: key("cDecayTime", 200).saturating_mul(1000),
            delay_ms: key("cFilterDelay", 200).saturating_mul(1000),
            upkeep: number(ty, "cFilterUpkeep").unwrap_or(350.0),
            clean: key("cFilterCleanAmount", 10),
            decayed_clean: key("cFilterDecayedCleanAmount", 5),
        }
    }
}

/// A time kept in the record: whole milliseconds in 32 bits.
fn ms(v: i64) -> i32 {
    v.clamp(i64::from(i32::MIN), i64::from(i32::MAX)) as i32
}

/// What a filter of type `ty` placed during play at game time `now` (ms) starts with (tank staff
/// 7.2): the starting health, its decay timer full and a filter timer of 0, so that it filters (and
/// charges) at its first update, no upkeep paid, today as its build and service dates and a count
/// of 1 month.
pub fn placed(ty: &EntityType, now: i64) -> Filter {
    let keys = Keys::of(ty);
    let today = file_time(now);
    Filter {
        health: keys.start.max(0) as u32,
        timers: Some([ms(keys.decay_ms), 0]),
        built: today,
        serviced: today,
        months: 1,
        check: FILTER_CHECK_MS,
        ..Filter::default()
    }
}

/// The months from file time `from` to game time `now` (ms): `(d + 29) / 30` for `d` the whole days
/// between them (information panels 18.3, 18.6; the whole division cuts toward zero, so a date in
/// the future gives 0 up to 29 days ahead and a negative count beyond).
pub fn months_since(from: u64, now: i64) -> i32 {
    let ticks = i128::from(file_time(now)) - i128::from(from);
    let days = (ticks / 864_000_000_000).clamp(i128::from(i32::MIN), i128::from(i32::MAX));
    ((days as i64 + 29) / 30) as i32
}

/// How worn a filter is, by its health (tank staff 7.5; information panels 18.3).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Wear {
    /// Health 0.
    Broken,
    /// Health 1 to `cDecayedHealth`.
    Decayed,
    Healthy,
}

/// The wear of a filter of type `ty` with `health`.
pub fn wear(ty: &EntityType, health: u32) -> Wear {
    match i64::from(health) {
        0 => Wear::Broken,
        h if h <= Keys::of(ty).decayed => Wear::Decayed,
        _ => Wear::Healthy,
    }
}

/// The sell price of a filter of type `ty` with `health` as a part of its purchase cost (tank
/// staff 7.2): 0.8, halved once decayed, 0 once broken.
pub fn sell_rate(ty: &EntityType, health: u32) -> f32 {
    match wear(ty, health) {
        Wear::Broken => 0.0,
        Wear::Decayed => 0.4,
        Wear::Healthy => 0.8,
    }
}

impl World {
    /// Sets the purity of tank `id` to `purity` cut to 0 and `initialWaterPurity` (tank staff 7.4):
    /// the water too murky to see through is told once. The tank is not marked for a new rating
    /// (habitat rating 4).
    pub fn set_purity(&mut self, id: u32, purity: i64) {
        let Some(old) = self.tank(id).map(|t| t.purity) else {
            return;
        };
        let new = purity.clamp(0, i64::from(self.tank_config.purity)) as u32;
        if new == old {
            return;
        }
        self.edit_tank(id, |t, _| t.purity = new);
        if let Some(x) = self.exhibits.get(id)
            && old > EXTREME
            && new <= EXTREME
        {
            let names = vec![x.name.clone()];
            self.units.post_named(TOO_MURKY, 0, names);
        }
    }

    /// The timed decay of the water (exhibits and money 3.10): while a tank holds water and has
    /// purity left, its purity timer runs; at `waterPurityDecayTime` the purity drops by one.
    pub fn decay_water(&mut self, step: u32) {
        let limit = self.tank_config.decay;
        for id in self.tank_ids() {
            let Some(tank) = self.tank(id).filter(|t| t.level > 0 && t.purity > 0) else {
                continue;
            };
            let Some(x) = self.exhibits.get_mut(id) else {
                continue;
            };
            x.purity_ms = x.purity_ms.saturating_add(step);
            if x.purity_ms >= limit {
                x.purity_ms = 0;
                self.set_purity(id, i64::from(tank.purity) - 1);
            }
        }
    }

    /// The tank the filter at `at` faces: the tank of the neighbour of its tile in its facing
    /// direction (tank staff 7.2).
    pub fn filter_tank(&self, at: usize) -> Option<u32> {
        let c = self.zoo.entities.get(at)?.common.as_ref()?;
        let tile = self.tile_index(c.x.div_euclid(64), c.y.div_euclid(64))?;
        self.tank_at(self.neighbour(tile, c.rotation as usize)?)
    }

    /// The filter that serves tank `tank`: the one it recorded, while it is still on the map
    /// (tank staff 7.2).
    fn serving(&self, tank: u32) -> Option<usize> {
        let id = self.exhibits.get(tank)?.filter?;
        let at = self.find(id)?;
        let c = self.zoo.entities[at].common.as_ref()?;
        c.filter.is_some().then_some(at)
    }

    /// A filter was added to the map at position `at`: the tank it faces records it, unless a
    /// live filter serves that tank already (tank staff 7.2).
    pub(super) fn filter_added(&mut self, at: usize) {
        let c = self.zoo.entities[at].common.as_ref();
        let Some(id) = c.filter(|c| c.filter.is_some()).map(|c| c.id) else {
            return;
        };
        let Some(tank) = self.filter_tank(at) else {
            return;
        };
        if self.serving(tank).is_none()
            && let Some(x) = self.exhibits.get_mut(tank)
        {
            x.filter = Some(id);
        }
    }

    /// The facing a tank filter placed on tile `tile` takes (placing 5.1): towards a side
    /// neighbour (directions 0, 2, 4, 6) that is a tile of a tank no live filter serves, with no
    /// piece on the side of `tile` towards it; `facing` when it is one of them, else the last.
    /// None on a tank tile or with no such neighbour.
    pub fn filter_facing(&self, tile: usize, facing: u32) -> Option<u32> {
        if self.tank_at(tile).is_some() {
            return None;
        }
        let open = |d: &u32| {
            let tank = self
                .neighbour(tile, *d as usize)
                .and_then(|n| self.tank_at(n));
            tank.is_some_and(|t| self.serving(t).is_none())
                && self.edge(tile, *d as usize).is_none()
        };
        let sides: Vec<u32> = [0, 2, 4, 6].into_iter().filter(open).collect();
        match sides.contains(&facing) {
            true => Some(facing),
            false => sides.last().copied(),
        }
    }

    /// The tile a filter at `at` stands on, for the worker's task.
    fn filter_tile(&self, at: usize) -> u32 {
        let c = self.zoo.entities[at].common.as_ref();
        let tile = c.and_then(|c| self.tile_index(c.x.div_euclid(64), c.y.div_euclid(64)));
        tile.unwrap_or(0) as u32
    }

    /// Puts a new filter of type `ty` on tile `tile` facing `rotation`, as the player places one
    /// (tank staff 7.2): full health, and a filter timer of 0 so that it filters at its first
    /// update. Whether it was added.
    pub fn add_filter(&mut self, ty: &EntityType, tile: usize, rotation: u32) -> bool {
        let w = self.zoo.width as usize;
        let name = crate::place::unit::named(self, ty);
        let common = Common {
            x: (tile % w) as i32 * 64 + 32,
            y: (tile / w) as i32 * 64 + 32,
            z: self.zoo.tiles[tile].elevation * 16,
            rotation: rotation % 8,
            id: 0,
            name,
            colors: Vec::new(),
            fence: None,
            filter: Some(placed(ty, self.units.now)),
            ..Default::default()
        };
        let text = |bytes: &[u8]| bytes.iter().map(|&b| char::from(b)).collect::<String>();
        let entity = Entity {
            class: ty.class.into(),
            type_name: text(&ty.type_name),
            subtype: text(&ty.subtype),
            body_size: None,
            common: Some(common),
        };
        let shape = Shape {
            kind: Kind::Fixed,
            footprint: (ty.footprint[0], ty.footprint[1]),
            fence: false,
            wall: false,
            gate: false,
            gated: false,
            tank: TankKind::NONE,
            walkable: (false, false),
        };
        self.add(entity, Some(shape)).is_some()
    }

    /// The events a zoo just loaded posts for its filters (tank staff 7.5): a broken one posts
    /// 10036 again and a decayed one 10035.
    pub fn loaded_filter_events(&self, cat: &Catalogue) -> Vec<Event> {
        let mut events = Vec::new();
        for (at, e) in self.zoo.entities.iter().enumerate() {
            let (Some(c), Some(ty)) = (e.common.as_ref(), type_of(cat, e)) else {
                continue;
            };
            let Some(f) = c.filter else { continue };
            let kind = match i64::from(f.health) {
                0 => BROKEN,
                h if h <= Keys::of(ty).decayed => DECAYED,
                _ => continue,
            };
            events.push(piece_event(c.id, kind, self.filter_tile(at)));
        }
        events
    }

    /// One step of `step` milliseconds of the filter at `at`, run at its place in the entity loop
    /// (tank staff 7.3): a flooded or doubled filter is to be removed, and does nothing more;
    /// otherwise its tank records it, then the cycle that charges the upkeep and cleans the water
    /// runs, then the wear. A `world filter` line tells each cycle, each loss of health and a
    /// removal. The events the wear posts are returned, and the id of a filter to remove.
    pub fn filter_step(
        &mut self,
        cat: &Catalogue,
        at: usize,
        step: u32,
    ) -> (Vec<Event>, Option<u32>) {
        let Some((id, mut f, (x, y))) = (self.zoo.entities.get(at))
            .and_then(|e| e.common.as_ref())
            .and_then(|c| Some((c.id, c.filter?, (c.x, c.y))))
        else {
            return (Vec::new(), None);
        };
        let own = self.tile_index(x.div_euclid(64), y.div_euclid(64));
        let faced = self.filter_tank(at);
        let flooded = own.and_then(|t| self.tank_at(t)).is_some();
        let other = faced.and_then(|t| self.serving(t)).filter(|&o| o != at);
        let doubled = other.is_some_and(|o| self.filter_tank(o) == faced);
        if flooded || doubled {
            let word = if flooded { "flooded" } else { "doubled" };
            let purity = faced.and_then(|t| self.tank(t)).map_or(0, |t| t.purity);
            (self.units).world_note(format!("filter {id} {} {purity} {word}", f.health));
            return (Vec::new(), Some(id));
        }
        if let Some(x) = faced.and_then(|t| self.exhibits.get_mut(t)) {
            x.filter = Some(id);
        }
        let ty = type_of(cat, &self.zoo.entities[at]);
        let keys = ty.map(Keys::of);
        let step = i64::from(step);
        let fresh = |k: &Option<Keys>| {
            k.as_ref()
                .map_or([0; 2], |k| [ms(k.decay_ms), ms(k.delay_ms)])
        };
        let [mut decay, mut timer] = f.timers.unwrap_or_else(|| fresh(&keys));
        let tank = faced.filter(|&t| self.tank(t).is_some_and(|k| k.level > 0));
        // The cycle: only a tank holding water is cleaned.
        if let Some(tank) = tank {
            timer = ms(i64::from(timer) - step);
            if timer < 1
                && let Some(k) = keys.as_ref().filter(|_| f.health > 0)
            {
                let amount = match i64::from(f.health) > k.decayed {
                    true => k.clean,
                    false => k.decayed_clean,
                };
                self.units.spent.push((BUILDING_UPKEEP, k.upkeep));
                // Whole money, in the figure of the current period and the one since built.
                let paid = k.upkeep as u32;
                f.upkeep[1] = f.upkeep[1].saturating_add(paid);
                f.upkeep[2] = f.upkeep[2].saturating_add(paid);
                let purity = self.tank(tank).map_or(0, |t| i64::from(t.purity));
                self.set_purity(tank, purity + amount);
                let now = self.tank(tank).map_or(0, |t| t.purity);
                self.units
                    .world_note(format!("filter {id} {} {now}", f.health));
                timer = ms(k.delay_ms);
            }
        }
        // The wear, on or off.
        let mut events = Vec::new();
        decay = ms(i64::from(decay) - step);
        if decay < 1
            && let Some(k) = keys.as_ref().filter(|_| f.health > 0)
        {
            let was = i64::from(f.health);
            f.health -= 1;
            let tile = self.filter_tile(at);
            if was > k.decayed && f.health > 0 && i64::from(f.health) <= k.decayed {
                events.push(piece_event(id, DECAYED, tile));
            }
            if f.health == 0 {
                events.push(piece_event(id, BROKEN, tile));
            }
            decay = ms(k.decay_ms);
            let purity = tank.and_then(|t| self.tank(t)).map_or(0, |t| t.purity);
            self.units
                .world_note(format!("filter {id} {} {purity} worn", f.health));
        }
        f.timers = Some([decay, timer]);
        // The periods of the upkeep, counted from the build date (tank staff 7.3 point 6).
        f.check = ms(i64::from(f.check) - step);
        if f.check < 1 {
            f.check = FILTER_CHECK_MS;
            let months = months_since(f.built, self.units.now);
            if months != 0 && months != f.months {
                f.months = months;
                f.upkeep[0] = f.upkeep[1];
                f.upkeep[1] = 0;
            }
        }
        if let Some(c) = self.zoo.entities[at].common.as_mut() {
            c.filter = Some(f);
        }
        (events, None)
    }

    /// Removes the flooded or doubled filter with this id (tank staff 7.3): nothing is refunded,
    /// but the undo action records its sell price of the moment, which an undo takes back
    /// (placing 8, kind 2). Whether there was such a filter.
    pub fn remove_filter(&mut self, cat: &Catalogue, id: u32) -> bool {
        let price = (self.find(id)).map(|at| refund(cat, &self.zoo.entities[at]));
        let gone = self.remove(id);
        if let Some(price) = price.filter(|_| gone) {
            self.priced(price);
        }
        gone
    }

    /// A start with `reset` or `resetdate` resets every filter (guest buildings 4.1): the upkeep
    /// figures go to 0, the count to 1 and the build and service dates to game time `now` (ms).
    pub fn reset_filters(&mut self, now: i64) {
        let today = file_time(now);
        for f in self
            .zoo
            .entities
            .iter_mut()
            .filter_map(|e| e.common.as_mut()?.filter.as_mut())
        {
            (f.upkeep, f.months, f.built, f.serviced) = ([0; 3], 1, today, today);
        }
    }

    /// The filter with this id is serviced (tank staff 7.6): its health is the starting health.
    /// Whether there is such a filter.
    pub fn service_filter(&mut self, cat: &Catalogue, id: u32) -> bool {
        let Some(at) = self.find(id) else {
            return false;
        };
        let start = type_of(cat, &self.zoo.entities[at]).map_or(10, |ty| Keys::of(ty).start);
        let Some(f) = (self.zoo.entities[at].common.as_mut()).and_then(|c| c.filter.as_mut())
        else {
            return false;
        };
        f.health = start.max(0) as u32;
        f.serviced = file_time(self.units.now);
        let tank = self.filter_tank(at);
        let purity = tank.and_then(|t| self.tank(t)).map_or(0, |t| t.purity);
        self.units
            .world_note(format!("filter {id} {start} {purity} serviced"));
        true
    }
}
