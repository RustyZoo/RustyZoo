//! The random walk of a unit with nothing to do (movement 10): the choice of a destination by
//! class, the searches and the fallbacks, with the draws in the original's order.
//!
//! Kept as the original has it: with no suitable tile on the ring, the corner of the ring is the
//! destination (engine issue 304). An animal in the tank mode picks a tile of its tank not behind
//! it and a level for it (tank fauna 6), one in the water mode a water tile. An animal whose
//! exhibit's escape table holds its type first tries the escape walk (animals 11.3). Not here yet:
//! the exhibits a worker is assigned to.

use super::{Kind, Part, Unit, movement, swim};
use crate::animal::tank::kind;
use crate::config::Config;
use crate::entity::Catalogue;
use crate::exhibit::{EXTERIOR, GROUNDS};
use crate::guest::view;
use crate::path::{self, Dest, Finder, MAX, Mover};
use crate::place::bulldozer::type_of;
use crate::sim::Rng;
use crate::staff::assign;
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
/// move call starts the walk ([`movement::call`]). A unit that has a path does nothing.
pub fn random_walk(
    unit: &mut Unit,
    world: &mut World,
    cat: &Catalogue,
    rng: &mut Rng,
    at: usize,
) -> bool {
    walking(unit, world, cat, rng, at, |walk| walk.run())
}

/// The walk of the surface goal (tank fauna 10.2): the destination of the unit's terrain mode and a
/// path to it that is not partial, with no fallback.
pub fn surface_walk(
    unit: &mut Unit,
    world: &mut World,
    cat: &Catalogue,
    rng: &mut Rng,
    at: usize,
) -> bool {
    walking(unit, world, cat, rng, at, |walk| {
        let pick = walk.pick();
        pick.is_some_and(|to| !walk.unit.path.is_empty() || walk.search(to, false))
    })
}

/// A walk of the unit at position `at` that `how` makes.
fn walking(
    unit: &mut Unit,
    world: &mut World,
    cat: &Catalogue,
    rng: &mut Rng,
    at: usize,
    how: impl FnOnce(&mut Walk) -> bool,
) -> bool {
    if !unit.path.is_empty() {
        return false;
    }
    let Some(c) = world.zoo.entities.get(at).and_then(|e| e.common.as_ref()) else {
        return false;
    };
    let (x, y, z, facing) = (c.x, c.y, c.z, c.rotation);
    let Some(here) = world.tile_index(x.div_euclid(64), y.div_euclid(64)) else {
        return false;
    };
    // The finder leaves the table for the walk, as the unit does for its turn.
    let mut finder = std::mem::take(&mut world.units.finder);
    let mover = walker(unit, world, at, here, facing);
    let mut walk = Walk {
        unit,
        world,
        finder: &mut finder,
        cat,
        rng,
        mover,
        at,
        here,
        level: z.div_euclid(16),
        facing,
        goal: None,
    };
    let ok = how(&mut walk);
    world.units.finder = finder;
    ok
}

/// The unit as the searches of a random walk see it: with the turn flag of the call, which makes
/// them charge the first step for the turn off the unit's facing (pathfinding 5.1).
fn walker(unit: &Unit, world: &World, at: usize, here: usize, facing: u32) -> Mover {
    Mover {
        turn: Some(facing as usize),
        ..movement::mover(unit, world, at, here)
    }
}

struct Walk<'a> {
    unit: &'a mut Unit,
    world: &'a mut World,
    finder: &'a mut Finder,
    cat: &'a Catalogue,
    rng: &'a mut Rng,
    mover: Mover,
    /// The position of the unit in the entity list.
    at: usize,
    here: usize,
    level: i32,
    facing: u32,
    /// The level of a destination in a tank (tank fauna 6.2).
    goal: Option<i32>,
}

impl Walk<'_> {
    fn run(&mut self) -> bool {
        // 1 and 2: the pick of the class, unless it is where the unit stands; an escape walk
        // leaves its path on the unit.
        let pick = self.pick();
        let far =
            |t: &usize| *t != self.here || self.goal.unwrap_or(self.base(*t) / 16) != self.level;
        if let Some(to) = pick.filter(far)
            && (!self.unit.path.is_empty() || self.search(to, true))
        {
            return true;
        }
        // The level belongs to the picked destination only.
        self.goal = None;
        let found = self.fallbacks();
        if found {
            // The first search of the fallbacks replaces a tile the pick sent the animal to (tank
            // fauna 6.1); the send stands only when none succeeds.
            self.unit.sent = None;
        }
        found
    }

    /// 3 to 5, the fallbacks of the random walk.
    fn fallbacks(&mut self) -> bool {
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
            self.give_level(to);
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
            if self.fits(n, true) && cost < MAX && {
                self.give_level(n);
                self.search(n, false)
            } {
                return true;
            }
        }
        // 5: from here to here, which succeeds with the path of the tile.
        self.goal = None;
        self.search(self.here, false)
    }

    fn xy(&self, t: usize) -> (i32, i32) {
        let width = self.world.zoo.width as usize;
        ((t % width) as i32, (t / width) as i32)
    }

    /// The level a destination in tile `t` gets (tank fauna 6.2): the fallbacks draw it too.
    fn give_level(&mut self, t: usize) {
        self.goal = Some(swim::level_of(
            self.unit, self.world, self.rng, t, self.level,
        ));
    }

    /// The base height of a tile, where a destination in it is.
    fn base(&self, t: usize) -> i32 {
        16 * self.world.zoo.tiles[t].elevation
    }

    /// A search to tile `to`; a successful one gives the unit its path and destination.
    fn search(&mut self, to: usize, partial: bool) -> bool {
        self.mover.levels.1 = self.goal.unwrap_or(self.mover.levels.0);
        let reply = self.finder.request(self.world, &self.mover, to, partial);
        if let Some(path) = reply.path {
            (self.unit.path, self.unit.levels) = (path, reply.levels.clone());
        }
        self.unit.destination = match reply.dest {
            Dest::Keep => self.unit.destination,
            Dest::Clear => None,
            Dest::Set(t) => {
                let (x, y) = self.xy(t);
                let z = reply.levels.last().map_or(self.base(t), |l| 16 * l);
                Some((64 * x + 32, 64 * y + 32, z))
            }
        };
        // The next request of the walk sees the destination this one left.
        self.mover.dest = (self.unit.destination)
            .and_then(|(x, y, _)| self.world.tile_index(x.div_euclid(64), y.div_euclid(64)));
        reply.ok
    }

    /// 10.3: the destination by class.
    fn pick(&mut self) -> Option<usize> {
        let region = self.world.exhibits().region(self.here);
        let exhibit = region.filter(|&r| r != GROUNDS && r != EXTERIOR);
        let mode = match &self.unit.part {
            Part::Animal(a) => a.terrain,
            _ => 0,
        };
        if let Some(b) = exhibit
            .filter(|_| self.unit.kind == Kind::Animal)
            .and_then(|id| self.escape(id))
        {
            return Some(b);
        }
        match (self.unit.kind, exhibit) {
            (Kind::Animal, None) => None,
            (Kind::Animal, Some(id)) if mode == 2 => self.tank_tile(id),
            (Kind::Animal, Some(id)) if mode == 1 => self.water_tile(id),
            (Kind::Animal, Some(id)) => {
                // The exhibit's tiles, then those of its amphibious neighbours: no other test.
                let dry = self.of_kind(id, 0);
                if dry.is_empty() {
                    // A swimmer on water takes the water mode, a diver in a tank the tank mode;
                    // either stands on a tile of its new kind already, so it is not sent.
                    let (m, w, here) = (&self.unit.mover, &*self.world, self.here);
                    if m.swims && w.is_water(here) {
                        swim::set_terrain(self.unit, 1);
                    } else if m.underwater && !self.egg() && w.tank_at(here).is_some() {
                        swim::set_terrain(self.unit, 2);
                    }
                    return None;
                }
                Some(dry[self.rng.pick(dry.len() as u32) as usize])
            }
            (_, Some(id)) => self.gate_neighbour(id),
            (Kind::Keeper, None) => self.keeper_gate().or_else(|| self.stroll()),
            (Kind::Guide, None) => self.guide_gate().or_else(|| self.stroll()),
            (Kind::Worker, None) => self.worker_gate().or_else(|| self.stroll()),
            _ => self.stroll(),
        }
    }

    /// Animals 11.3, the escape attempt of an animal in exhibit `id` whose escape table holds its
    /// type: the old walk is dropped and a path to the table's tile is searched with the escape
    /// flag up. The path is cut after the first step that crosses a fence piece, one tile further
    /// when the animal crosses it by a jump, a climb, a bash or out of a tank and the ordinary
    /// rules allow the step; the walk's last tile `B` is then its destination. With no path of two
    /// tiles nothing changes and the ordinary pick follows. No draw.
    fn escape(&mut self, id: u32) -> Option<usize> {
        let e = &self.world.zoo.entities[self.at];
        let name = format!("{}/{}", e.type_name, e.subtype);
        let Part::Animal(a) = &mut self.unit.part else {
            return None;
        };
        if a.egg || a.in_show {
            return None;
        }
        let table = &self.world.exhibits().get(id)?.rating.escapes;
        let target = table.iter().find(|(n, _)| *n == name)?.1;
        a.walk = None;
        let w = &*self.world;
        let water = w.tank_at(target).and_then(|t| w.tank(t));
        let mut m = Mover {
            test: true,
            ..self.mover.clone()
        };
        m.levels.1 = w.zoo.tiles[target].elevation + water.map_or(0, |t| t.steps().1);
        let reply = self.finder.request(w, &m, target, false);
        let path = reply.path.filter(|p| reply.ok && p.len() >= 2)?;
        let mut levels = reply.levels;
        let width = (w.zoo.width as usize).max(1);
        let xy = |t: usize| [(t % width) as i32, (t / width) as i32];
        let mut walk = crate::zoo::Escape {
            a: [-1; 2],
            b: xy(target),
            ..Default::default()
        };
        let mut kept = vec![path[0]];
        let tank = w.exhibits().get(id).is_some_and(|e| e.tank.is_some());
        for pair in path.windows(2) {
            let (t1, t2) = (pair[0], pair[1]);
            kept.push(t2);
            let Ok(d) = usize::try_from(movement::direction(w, t1, t2)) else {
                continue;
            };
            let pieces = (w.edge(t1, d), w.edge(t2, (d + 4) % 8));
            if pieces == (None, None) {
                continue;
            }
            walk.a = xy(t1);
            walk.kind = crate::animal::escape::kind(w, &m, tank, pieces);
            let mut point = t2;
            if walk.kind != 0
                && let Some(c) = w.neighbour(t2, d)
            {
                let ordinary = Mover {
                    at: Some(t2),
                    test: false,
                    turn: None,
                    ..self.mover.clone()
                };
                let step = self.finder.request(w, &ordinary, c, false);
                if step.ok && step.path.is_some_and(|p| p.len() == 2) {
                    kept.push(c);
                    levels.extend(step.levels.last());
                    point = c;
                }
            }
            walk.across = u32::from(point == t2);
            walk.b = xy(point);
            break;
        }
        levels.truncate(kept.len());
        let b = w.tile_index(walk.b[0], walk.b[1])?;
        let z = levels.last().map_or(self.base(b), |l| 16 * l);
        self.unit.destination = Some((64 * walk.b[0] + 32, 64 * walk.b[1] + 32, z));
        (self.unit.path, self.unit.levels) = (kept, levels);
        let Part::Animal(a) = &mut self.unit.part else {
            return None;
        };
        // An animal that stands on the crossing tile already starts the crossing now.
        if walk.a == xy(self.here) {
            a.force = crate::animal::escape::crossing(walk.kind);
        }
        a.walk = Some(walk);
        Some(b)
    }

    /// The tiles of exhibit `id` and then of its amphibious neighbours that suit the terrain
    /// mode `mode` (tank fauna 5), each in its list order.
    fn of_kind(&self, id: u32, mode: u8) -> Vec<usize> {
        let around = (self.world.exhibits().get(id)).map_or(Vec::new(), |e| e.amphibious.clone());
        (self.world.tiles_of(id).into_iter())
            .chain(around.into_iter().flat_map(|n| self.world.tiles_of(n)))
            .filter(|&t| kind(self.world, t) == mode)
            .collect()
    }

    fn egg(&self) -> bool {
        matches!(&self.unit.part, Part::Animal(a) if a.egg)
    }

    /// 6.1: a choice in the water or the tank mode found no tile: the animal takes the land mode
    /// and, when the mode changes and it stands on water or in a tank, is sent to a land tile of
    /// exhibit `id` and its amphibious neighbours, one draw (tank fauna 5).
    fn back_to_land(&mut self, id: u32) {
        let was = match &self.unit.part {
            Part::Animal(a) => a.terrain,
            _ => return,
        };
        swim::set_terrain(self.unit, 0);
        let now = match &self.unit.part {
            Part::Animal(a) => a.terrain,
            _ => return,
        };
        if was == now || kind(self.world, self.here) == 0 {
            return;
        }
        let dry = self.of_kind(id, 0);
        if !dry.is_empty() {
            self.unit.sent = Some(dry[self.rng.pick(dry.len() as u32) as usize]);
        }
    }

    /// 6.1, the water mode: a water tile of the exhibit or an amphibious neighbour, one draw; no
    /// tile gives the land mode.
    fn water_tile(&mut self, id: u32) -> Option<usize> {
        let wet = self.of_kind(id, 1);
        if wet.is_empty() {
            self.back_to_land(id);
            return None;
        }
        Some(wet[self.rng.pick(wet.len() as u32) as usize])
    }

    /// 6.1, the tank mode: a tile of the animal's tank that costs it less than 20 and is not
    /// behind it (its own tile is kept), else any tile of the tank, one draw either way; for an
    /// animal on land a tank tile of its amphibious neighbours. The tile's level comes with it. No
    /// tile gives the land mode.
    fn tank_tile(&mut self, id: u32) -> Option<usize> {
        let tiles: Vec<usize> = match self.world.tank(id) {
            Some(_) => self.world.tiles_of(id),
            None => {
                let around =
                    (self.world.exhibits().get(id)).map_or(Vec::new(), |e| e.amphibious.clone());
                (around.into_iter().flat_map(|n| self.world.tiles_of(n)))
                    .filter(|&t| self.world.tank_at(t).is_some())
                    .collect()
            }
        };
        let behind = (self.facing + 4) % 8;
        let ahead = |s: &Self, t: usize| {
            let d = movement::direction(s.world, s.here, t);
            d < 0 || ![0, 1, 7].contains(&(d as u32 + 8 - behind).rem_euclid(8))
        };
        let near =
            |s: &Self, t: usize| path::terrain(s.world, &s.mover, t, None) < MAX && ahead(s, t);
        let open: Vec<usize> = match self.world.tank(id) {
            Some(_) => tiles.iter().copied().filter(|&t| near(self, t)).collect(),
            None => Vec::new(),
        };
        let from = if open.is_empty() { &tiles } else { &open };
        if from.is_empty() {
            self.back_to_land(id);
            return None;
        }
        let tile = from[self.rng.pick(from.len() as u32) as usize];
        self.goal = Some(swim::level_of(
            self.unit, self.world, self.rng, tile, self.level,
        ));
        Some(tile)
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

    /// The list of exhibits a staff member is assigned to.
    fn assignments(&self) -> Vec<u32> {
        match &self.unit.part {
            Part::Staff(s) => s.assigned.clone(),
            _ => Vec::new(),
        }
    }

    /// A keeper on the grounds (staff 7.10): the assigned exhibits, else every exhibit that gives
    /// 0 in `block_service` with the arguments `(false, true)`.
    fn keeper_gate(&mut self) -> Option<usize> {
        let mut list = self.assignments();
        if list.is_empty() {
            let who = type_of(self.cat, self.world.zoo.entities.get(self.at)?)?;
            let (world, cat) = (&*self.world, self.cat);
            list = (world.exhibits().list().iter().map(|x| x.id))
                .filter(|&x| assign::block_service(world, cat, x, who, (false, true)) == 0)
                .collect();
        }
        self.gate_of_one(&list)
    }

    /// A worker on the grounds (movement 10.3): the assigned exhibits only, no fallback list.
    fn worker_gate(&mut self) -> Option<usize> {
        let list = self.assignments();
        self.gate_of_one(&list)
    }

    /// A guide on the grounds (tour guide 12.4): the exhibit is chosen as for a leg.
    fn guide_gate(&mut self) -> Option<usize> {
        let list = view::tour_exhibits(self.world, self.cat, &self.assignments());
        self.gate_of_one(&list)
    }

    /// The tile outside the gate of one of the exhibits `list`, one draw when it has any; none
    /// when the exhibit has no gate or the unit stands on the tile (the ring serves then).
    fn gate_of_one(&mut self, list: &[u32]) -> Option<usize> {
        let x = *list.get(self.rng.pick(list.len() as u32) as usize)?;
        let (_, outside) = crate::staff::service::gate_tiles(self.world, x)?;
        Some(outside).filter(|&t| t != self.here)
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
                // The terrain mode: the tank mode takes tank tiles, the water mode water tiles, the
                // land mode neither.
                let mode = match &self.unit.part {
                    Part::Animal(a) => a.terrain,
                    _ => 0,
                };
                let ground = match mode {
                    2 => w.tank_at(t).is_some(),
                    1 => kind(w, t) == 1,
                    _ => !w.is_water(t) && w.tank_at(t).is_none(),
                };
                region == here
                    && w.in_zoo(t)
                    && (w.path(t).is_none() || lenient)
                    && ground
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
