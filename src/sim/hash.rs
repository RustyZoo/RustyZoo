//! The state hash of a run (the `sim` command): FNV-1a over the bytes of fixed-width fields,
//! little-endian, so that every system gets the same number. Whole numbers are written at the
//! width of the field, decimal values by the bits of their `f32`, a string or a list after its
//! length as a `u32`. The tables are walked in list order, never through a hash map.

use super::{Seconds, Sim};
use crate::exhibit::{Exhibit, rating};
use crate::money::Money;
use crate::world::World;
use crate::zoo::{Common, Entity, Event, Zoo};

const OFFSET: u64 = 0xcbf2_9ce4_8422_2325;
const PRIME: u64 = 0x0000_0100_0000_01b3;

/// An FNV-1a hash being fed.
pub struct Fnv(u64);

impl Default for Fnv {
    fn default() -> Fnv {
        Fnv(OFFSET)
    }
}

impl Fnv {
    pub fn finish(&self) -> u64 {
        self.0
    }

    pub(super) fn put(&mut self, bytes: &[u8]) {
        for &b in bytes {
            self.0 = (self.0 ^ u64::from(b)).wrapping_mul(PRIME);
        }
    }

    fn u8(&mut self, v: u8) {
        self.put(&[v]);
    }

    fn u16(&mut self, v: u16) {
        self.put(&v.to_le_bytes());
    }

    fn u32(&mut self, v: u32) {
        self.put(&v.to_le_bytes());
    }

    fn i32(&mut self, v: i32) {
        self.u32(v as u32);
    }

    fn u64(&mut self, v: u64) {
        self.put(&v.to_le_bytes());
    }

    fn f32(&mut self, v: f32) {
        self.u32(v.to_bits());
    }

    fn len(&mut self, n: usize) {
        self.u32(n as u32);
    }

    fn text(&mut self, s: &str) {
        self.len(s.len());
        self.put(s.as_bytes());
    }

    fn option(&mut self, v: Option<u32>) {
        self.u8(u8::from(v.is_some()));
        self.u32(v.unwrap_or(0));
    }

    fn event(&mut self, e: &Event) {
        for v in [e.sender, e.target, e.data] {
            self.u32(v);
        }
        for v in [e.extra, e.kind, e.delay] {
            self.u16(v);
        }
        self.u8(e.sender_channel);
        self.u8(e.channel);
    }

    fn entity(&mut self, e: &Entity) {
        for s in [&e.class, &e.type_name, &e.subtype] {
            self.text(s);
        }
        self.option(e.body_size);
        self.u8(u8::from(e.common.is_some()));
        if let Some(Common {
            x,
            y,
            z,
            rotation,
            id,
            name,
            colors,
            fence,
            movement_mode,
            animal: _,
        }) = &e.common
        {
            for v in [*x, *y, *z] {
                self.i32(v);
            }
            self.u32(*rotation);
            self.u32(*id);
            self.text(name);
            self.len(colors.len());
            self.put(colors);
            self.u8(u8::from(fence.is_some()));
            if let Some(f) = fence {
                for v in [f.strength, f.life, f.timers[0], f.timers[1]] {
                    self.u32(v);
                }
                self.u8(u8::from(f.portal));
            }
            self.u32(*movement_mode);
        }
    }

    fn zoo(&mut self, zoo: &Zoo) {
        self.len(zoo.tiles.len());
        for t in &zoo.tiles {
            self.i32(t.elevation);
            self.put(&[t.shape, t.terrain, t.edges]);
        }
        self.len(zoo.entities.len());
        for e in &zoo.entities {
            self.entity(e);
        }
        self.u32(zoo.entrance.0);
        self.u32(zoo.entrance.1);
        self.u32(zoo.id_counter);
        self.len(zoo.habitat_tiles.len());
        for &(x, y) in &zoo.habitat_tiles {
            self.i32(x);
            self.i32(y);
        }
        self.len(zoo.habitat_gates.len());
        for &((x, y), side) in &zoo.habitat_gates {
            for v in [x, y, side] {
                self.i32(v);
            }
        }
        self.len(zoo.tanks.len());
        for t in &zoo.tanks {
            for v in [t.tile.0, t.tile.1, t.gate.0, t.gate.1] {
                self.i32(v);
            }
            for v in [t.height, t.level, t.purity] {
                self.u32(v);
            }
            self.u8(u8::from(t.salt));
        }
    }

    fn exhibit(&mut self, x: &Exhibit) {
        self.u32(x.id);
        self.u64(x.first as u64);
        self.u64(x.tiles as u64);
        self.len(x.order.len());
        for &t in &x.order {
            self.u64(t as u64);
        }
        self.text(&x.name);
        self.option(x.gate);
        self.u8(u8::from(x.tank.is_some()));
        if let Some(t) = &x.tank {
            for v in [t.height, t.level, t.purity] {
                self.u32(v);
            }
            self.put(&[u8::from(t.show), u8::from(t.salt), u8::from(t.filled)]);
            self.i32(t.platform.0);
            self.i32(t.platform.1);
        }
        self.rating(&x.rating);
    }

    fn rating(&mut self, s: &rating::State) {
        self.put(&[u8::from(s.dirty), u8::from(s.revise)]);
        self.u32(s.species_ms);
        self.u32(s.rating_ms);
        self.len(s.records.len());
        for r in &s.records {
            self.text(&r.key);
            self.u32(r.animals);
            for v in [
                r.terrain,
                r.objects,
                r.foliage,
                r.rocks,
                r.elevation,
                r.shelters,
                r.toys,
                r.depth,
                r.water,
                r.salinity,
            ] {
                self.f32(v);
            }
            self.u8(u8::from(r.tank));
            self.u64(r.advice);
            self.u64(r.important);
        }
        self.len(s.types.len());
        for t in &s.types {
            self.text(t);
        }
        self.len(s.sounds.len());
        for (name, tile) in &s.sounds {
            self.text(name);
            self.u64(*tile as u64);
        }
        self.i32(s.attractiveness);
        self.option(s.keeper);
        self.u64(s.serviced);
    }

    fn money(&mut self, m: &Money) {
        self.f32(m.cash);
        self.u32(m.month);
        self.u32(m.year);
        for v in m.monthly.iter().flatten().chain(m.yearly.iter().flatten()) {
            self.f32(*v);
        }
        for v in &m.total {
            self.f32(*v);
        }
    }

    fn sim(&mut self, sim: &Sim) {
        let (elapsed, month_changed) = sim.calendar.state();
        self.u64(elapsed);
        self.u8(u8::from(month_changed));
        for v in [
            sim.total,
            sim.ai_rem,
            sim.ai_ticks,
            sim.next_clear,
            sim.world_secs,
            sim.world_rem,
            sim.rng.state(),
            sim.tiles as u32,
        ] {
            self.u32(v);
        }
        match sim.seconds {
            Seconds::Clock => self.u8(0),
            Seconds::Test { seed, n } => {
                self.u8(1);
                self.u32(seed);
                self.u32(n);
            }
        }
        self.len(sim.removals.len());
        for &id in &sim.removals {
            self.u32(id);
        }
        let [current, next] = sim.events.buffers();
        for buffer in [current, next] {
            self.len(buffer.len());
            for e in buffer {
                self.event(e);
            }
        }
    }
}

/// The hash of the whole deterministic state of a game: the tiles, the entities, the units, the
/// exhibits, the money, the calendar, the clocks and the generator.
pub fn state(sim: &Sim, world: &World, money: &Money) -> u64 {
    let mut h = Fnv::default();
    h.zoo(&world.zoo);
    h.u64(world.units.hash());
    h.rating(&world.exhibits().grounds);
    h.len(world.exhibits().list().len());
    for x in world.exhibits().list() {
        h.exhibit(x);
    }
    h.u32(world.exhibits().next);
    h.u32(world.exhibits().counter);
    h.money(money);
    h.sim(sim);
    h.finish()
}
