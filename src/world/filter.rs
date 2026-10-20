//! The water of a tank over time and the tank filters (tank staff 7): the purity setter every
//! change of the purity goes through, the timed decay of the purity, and the filter that cleans
//! the water, charges its upkeep, wears out and asks the maintenance workers for a service.
//!
//! The filter is flooded or doubled never: placing keeps a filter out of a tank and one filter
//! to a tank, so the update does not test either (tank staff 7.3 points 1 and 3).

use super::{Shape, TankKind, World, piece_event};
use crate::build::number;
use crate::entity::{Catalogue, EntityType};
use crate::money::BUILDING_UPKEEP;
use crate::place::bulldozer::type_of;
use crate::render::order::Kind;
use crate::zoo::{Common, Entity, Event, Filter};

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

impl World {
    /// Sets the purity of tank `id` to `purity` cut to 0 and `initialWaterPurity` (tank staff 7.4):
    /// the tank is rated again, and the water too murky to see through is told once.
    pub fn set_purity(&mut self, id: u32, purity: i64) {
        let Some(old) = self.tank(id).map(|t| t.purity) else {
            return;
        };
        let new = purity.clamp(0, i64::from(self.tank_config.purity)) as u32;
        if new == old {
            return;
        }
        self.edit_tank(id, |t, _| t.purity = new);
        if let Some(x) = self.exhibits.get_mut(id) {
            x.rating.dirty = true;
            if old > EXTREME && new <= EXTREME {
                let names = vec![x.name.clone()];
                self.units.post_named(TOO_MURKY, 0, names);
            }
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
        let keys = Keys::of(ty);
        let w = self.zoo.width as usize;
        let common = Common {
            x: (tile % w) as i32 * 64 + 32,
            y: (tile / w) as i32 * 64 + 32,
            z: self.zoo.tiles[tile].elevation * 16,
            rotation: rotation % 8,
            id: 0,
            name: String::new(),
            colors: Vec::new(),
            fence: None,
            filter: Some(Filter {
                health: keys.start.max(0) as u32,
                timers: Some([ms(keys.decay_ms), 0]),
            }),
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
    /// (tank staff 7.3 points 4 and 5): the cycle that charges the upkeep and cleans the water,
    /// then the wear. A `world filter` line tells each cycle and each loss of health; the events the
    /// wear posts are returned.
    pub fn filter_step(&mut self, cat: &Catalogue, at: usize, step: u32) -> Vec<Event> {
        let Some((id, mut f)) = (self.zoo.entities.get(at))
            .and_then(|e| e.common.as_ref())
            .and_then(|c| Some((c.id, c.filter?)))
        else {
            return Vec::new();
        };
        let ty = type_of(cat, &self.zoo.entities[at]);
        let keys = ty.map(Keys::of);
        let step = i64::from(step);
        let fresh = |k: &Option<Keys>| {
            k.as_ref()
                .map_or([0; 2], |k| [ms(k.decay_ms), ms(k.delay_ms)])
        };
        let [mut decay, mut timer] = f.timers.unwrap_or_else(|| fresh(&keys));
        let tank = self
            .filter_tank(at)
            .filter(|&t| self.tank(t).is_some_and(|k| k.level > 0));
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
        if let Some(c) = self.zoo.entities[at].common.as_mut() {
            c.filter = Some(f);
        }
        events
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
        let tank = self.filter_tank(at);
        let purity = tank.and_then(|t| self.tank(t)).map_or(0, |t| t.purity);
        self.units
            .world_note(format!("filter {id} {start} {purity} serviced"));
        true
    }
}
