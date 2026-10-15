//! The random walk of a unit with nothing to do (movement 10): the choice of a destination by
//! class, the searches and the fallbacks, with the draws in the original's order.
//!
//! Kept as the original has it: with no suitable tile on the ring, the corner of the ring is the
//! destination (engine issue 304). Not here yet: the water and tank modes of an animal and its
//! amphibious neighbours, the exhibits a staff member is assigned to, the escape of an animal.

use super::{Kind, Unit, movement};
use crate::config::Config;
use crate::exhibit::{EXTERIOR, GROUNDS};
use crate::path::{self, Dest, Finder, MAX, Mover};
use crate::sim::Rng;
use crate::timing::ini_whole;
use crate::world::World;

#[cfg(test)]
mod tests;

/// The `[AI]` settings of the ring of a stroll (12).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Ring {
    base: (i32, i32),
    random: (i32, i32),
}

impl Default for Ring {
    fn default() -> Ring {
        Ring {
            base: (1, 1),
            random: (5, 5),
        }
    }
}

impl Ring {
    /// `BaseX`, `BaseY` (below 0 gives 0), `RandomX` and `RandomY` (below 1 gives 1).
    pub fn from_ini(ini: &Config) -> Ring {
        let d = Ring::default();
        // No map is larger than the greatest value.
        let key = |k: &str, fallback: i32, least: i32| {
            ini_whole(ini, "AI", k).map_or(fallback, |v| v.clamp(least, 1 << 20))
        };
        Ring {
            base: (key("BaseX", d.base.0, 0), key("BaseY", d.base.1, 0)),
            random: (key("RandomX", d.random.0, 1), key("RandomY", d.random.1, 1)),
        }
    }
}

/// 10.2: the unit at position `at` of the entity list looks for a place to walk to and a path
/// to it. True when a search succeeded; the unit then holds the path and its destination, and a
/// move call starts the walk ([`movement::start`]). A unit that has a path does nothing.
pub fn random_walk(unit: &mut Unit, world: &mut World, rng: &mut Rng, at: usize) -> bool {
    if !unit.path.is_empty() {
        return false;
    }
    let Some(c) = world.zoo.entities.get(at).and_then(|e| e.common.as_ref()) else {
        return false;
    };
    let (x, y, z) = (c.x, c.y, c.z);
    let Some(here) = world.tile_index(x.div_euclid(64), y.div_euclid(64)) else {
        return false;
    };
    // The finder leaves the table for the walk, as the unit does for its turn.
    let mut finder = std::mem::take(&mut world.units.finder);
    let mover = movement::mover(unit, world, at, here);
    let mut walk = Walk {
        unit,
        world,
        finder: &mut finder,
        rng,
        mover,
        here,
        level: z.div_euclid(16),
    };
    let ok = walk.run();
    world.units.finder = finder;
    ok
}

struct Walk<'a> {
    unit: &'a mut Unit,
    world: &'a mut World,
    finder: &'a mut Finder,
    rng: &'a mut Rng,
    mover: Mover,
    here: usize,
    level: i32,
}

impl Walk<'_> {
    fn run(&mut self) -> bool {
        // 1 and 2: the pick of the class, unless it is where the unit stands.
        let pick = self.pick();
        let far = |t: &usize| *t != self.here || self.base(*t) / 16 != self.level;
        if let Some(to) = pick.filter(far)
            && self.search(to, true)
        {
            return true;
        }
        // 3: the ring again, with the lenient tile test and without the path tile test.
        let (tx, ty) = self.xy(self.here);
        let (rx, ry) = self.ring();
        let kept: Vec<usize> = self
            .border((tx, ty), (rx, ry))
            .into_iter()
            .filter(|&t| self.fits(t, true))
            .collect();
        if !kept.is_empty() {
            let to = kept[self.rng.pick(kept.len() as u32) as usize];
            if self.search(to, false) {
                return true;
            }
        }
        // 4: a straight neighbour from a random one on, without wrapping round.
        let start = self.rng.pick(4) as usize;
        for i in start..4 {
            let Some(n) = self.world.neighbour(self.here, i * 2) else {
                continue;
            };
            let cost = path::step(self.world, &self.mover, self.here, n, i * 2, None);
            if self.fits(n, true) && cost < MAX && self.search(n, false) {
                return true;
            }
        }
        // 5: from here to here, which succeeds with the path of the tile.
        self.search(self.here, false)
    }

    fn xy(&self, t: usize) -> (i32, i32) {
        let width = self.world.zoo.width as usize;
        ((t % width) as i32, (t / width) as i32)
    }

    /// The base height of a tile, where a destination in it is.
    fn base(&self, t: usize) -> i32 {
        16 * self.world.zoo.tiles[t].elevation
    }

    /// A search to tile `to`; a successful one gives the unit its path and destination.
    fn search(&mut self, to: usize, partial: bool) -> bool {
        let reply = self.finder.request(self.world, &self.mover, to, partial);
        if let Some(path) = reply.path {
            self.unit.path = path;
        }
        self.unit.destination = match reply.dest {
            Dest::Keep => self.unit.destination,
            Dest::Clear => None,
            Dest::Set(t) => {
                let (x, y) = self.xy(t);
                Some((64 * x + 32, 64 * y + 32, self.base(t)))
            }
        };
        reply.ok
    }

    /// 10.3: the destination by class.
    fn pick(&mut self) -> Option<usize> {
        let region = self.world.exhibits().region(self.here);
        let exhibit = region.filter(|&r| r != GROUNDS && r != EXTERIOR);
        match (self.unit.kind, exhibit) {
            (Kind::Animal, None) => None,
            (Kind::Animal, Some(id)) => {
                let dry: Vec<usize> = (self.world.tiles_of(id).into_iter())
                    .filter(|&t| !self.world.is_water(t) && self.world.tank_at(t).is_none())
                    .collect();
                (!dry.is_empty()).then(|| dry[self.rng.pick(dry.len() as u32) as usize])
            }
            (_, Some(id)) => self.gate_neighbour(id),
            _ => self.stroll(),
        }
    }

    /// The tile outside the gate of exhibit `id`, or one of the tiles around it that the unit
    /// may use; none for an exhibit without a gate.
    fn gate_neighbour(&mut self, id: u32) -> Option<usize> {
        let (tile, side) = self.world.gate_slot(id)?;
        let across = self.world.neighbour(tile, side)?;
        let gate = if self.world.exhibits().region(tile) == Some(id) {
            across
        } else {
            tile
        };
        let (gx, gy) = self.xy(gate);
        let region = self.world.exhibits().region(gate);
        let mut around = Vec::new();
        for ox in -1..=1 {
            for oy in -1..=1 {
                let t = self.world.tile_index(gx + ox, gy + oy);
                if let Some(t) = t.filter(|_| ox != 0 || oy != 0) {
                    let cost = path::terrain(self.world, &self.mover, t, None);
                    if self.world.exhibits().region(t) == region && cost < MAX {
                        around.push(t);
                    }
                }
            }
        }
        Some(match around.is_empty() {
            true => gate,
            false => around[self.rng.pick(around.len() as u32) as usize],
        })
    }

    /// The `rx` and `ry` of the ring: two draws.
    fn ring(&mut self) -> (i32, i32) {
        let ring = self.world.units.ring.clone();
        let rx = self.rng.pick(ring.random.0 as u32) as i32 + 1 + ring.base.0;
        let ry = self.rng.pick(ring.random.1 as u32) as i32 + 1 + ring.base.1;
        (rx, ry)
    }

    /// The tiles on the border of the rectangle round `(tx, ty)`, on the map, outer loop over x.
    fn border(&self, (tx, ty): (i32, i32), (rx, ry): (i32, i32)) -> Vec<usize> {
        let (width, height) = (self.world.zoo.width as i32, self.world.zoo.height as i32);
        let mut tiles = Vec::new();
        for ox in (-rx).max(-tx)..=rx.min(width - 1 - tx) {
            for oy in (-ry).max(-ty)..=ry.min(height - 1 - ty) {
                if ox.abs() == rx || oy.abs() == ry {
                    tiles.extend(self.world.tile_index(tx + ox, ty + oy));
                }
            }
        }
        tiles
    }

    /// A stroll of a guest, keeper, worker or guide on the grounds: a path tile on the ring.
    fn stroll(&mut self) -> Option<usize> {
        let (tx, ty) = self.xy(self.here);
        let (rx, ry) = self.ring();
        let candidates: Vec<usize> = (self.border((tx, ty), (rx, ry)).into_iter())
            .filter(|&t| self.world.path(t).is_some() && self.fits(t, false))
            .collect();
        if candidates.is_empty() {
            // The last tile looked at, the corner of the rectangle, whatever it is (issue 304).
            return self.world.tile_index(tx + rx, ty + ry);
        }
        Some(candidates[self.rng.pick(candidates.len() as u32) as usize])
    }

    /// Whether the unit's tile test accepts tile `t` (10.3), in its strict or its lenient form.
    fn fits(&self, t: usize, lenient: bool) -> bool {
        let w = &*self.world;
        let m = &self.mover;
        let (region, here) = (w.exhibits().region(t), w.exhibits().region(self.here));
        let off_path = lenient && w.path(self.here).is_none();
        match self.unit.kind {
            Kind::Guest => {
                let grounds = |r: Option<u32>| r == Some(GROUNDS) || r == Some(EXTERIOR);
                let tank = here.is_some_and(|r| w.tank(r).is_some());
                w.in_zoo(t)
                    && self.room(t)
                    && (w.path(t).is_some() || off_path)
                    && region.is_some()
                    && here.is_some()
                    && !tank
                    && (!grounds(here) || grounds(region))
            }
            Kind::Keeper | Kind::Worker => {
                w.in_zoo(t)
                    && (!w.is_water(t) || m.swims)
                    && (w.tank_at(t).is_none() || m.underwater)
            }
            Kind::Guide => w.in_zoo(t) && (w.path(t).is_some() || off_path) && self.room(t),
            Kind::Animal => {
                region == here
                    && w.in_zoo(t)
                    && (w.path(t).is_none() || lenient)
                    && !w.is_water(t)
                    && w.tank_at(t).is_none()
                    && self.room(t)
                    && (0..4).all(|q| w.quadrant(t, q).is_none())
            }
        }
    }

    /// Whether tile `t` has room for a unit: one of its four unit slots is empty or holds an
    /// entity the unit can walk through.
    fn room(&self, t: usize) -> bool {
        let units: Vec<usize> = self.world.units_on(t).collect();
        units.len() < 4
            || units
                .iter()
                .any(|&e| self.world.shape(e).is_some_and(|s| s.walkable.0))
    }
}
