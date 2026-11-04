//! A guest out of a return building (guest buildings 12.2, goals 11.13): the ride on the water of
//! task 0x82, the search for a building of the same type to come back to, the walk back, and the
//! guest's AI reset. The ride is a number of legs, each a walk to a random water tile with the
//! guest's return string as the set; nothing of it is saved.

use super::*;
use crate::entity::Catalogue;
use crate::exhibit::{EXTERIOR, GROUNDS};
use crate::goal::{Goal, NONE, RECOVER, stop_all};
use crate::guest::{HAPPINESS, view};
use crate::render::order::STEP;
use crate::world::World;

/// The thought of a viewer that sees overjoyed animals, and the set of a treading idler.
const OVERJOYED: i32 = 10098;
const TREAD: &str = "bSuitIdle";
/// The diagonal neighbours tried last.
const DIAGONALS: [(i32, i32); 4] = [(-1, -1), (-1, 1), (1, -1), (1, 1)];
/// Rounds of the search for a water tile, and tries in a round.
const ROUNDS: u32 = 20;
const TRIES: u32 = 30;

/// The object of task 0x82 (goals 11.13 point 1).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(in crate::goal) struct Ride {
    pub(super) legs: i32,
    pub(super) until: u32,
    pub(super) idler: bool,
    pub(super) idle: bool,
    pub(super) viewer: bool,
}

/// The idler and viewer flags of the building type found by a return name id.
fn flags(world: &World, cat: &Catalogue, name: Option<i32>) -> (bool, bool) {
    let default = world.units.prices.default;
    let ty = (cat.types.iter()).find(|t| t.class == "building" && name == Some(t.name_id));
    let keys = ty.map_or_else(Default::default, |t| building::Kind::of(t, default).ret);
    (keys.idler, keys.viewer)
}

/// A load (goals 11.13 point 9): the ride of a guest whose saved task is 0x82 is built and never
/// initialised: no legs, the AI clock `now`, and the flags of the type of its return name id.
pub(crate) fn rides_loaded(world: &mut World, cat: &Catalogue, now: u32) {
    let riders: Vec<u32> = (world.units.iter())
        .filter(|u| u.goal.object == Some(Goal::Ride))
        .map(|u| u.id)
        .collect();
    for id in riders {
        let name = match world.units.get(id).map(|u| &u.part) {
            Some(Part::Guest(g)) => g.returning,
            _ => continue,
        };
        let (idler, viewer) = flags(world, cat, name);
        if let Some(u) = world.units.get_mut(id) {
            u.goal.ride = Ride {
                until: now,
                idler,
                viewer,
                ..Ride::default()
            };
        }
    }
}

impl Cx<'_, '_> {
    /// A walk of the building chain or of the ride: a guest that is returning (or, with `always`,
    /// that holds a return string) walks with the return string as its set, any other with the
    /// default walk (guest buildings 12.2 point 24).
    pub(in crate::goal) fn return_walk(&mut self, force: bool, always: bool) -> bool {
        let set = match self.guest() {
            Some(g) if always || g.returning.is_some() => g.return_set.clone(),
            _ => None,
        };
        match set.filter(|s| !s.is_empty()) {
            Some(set) => self.walk_set(&set, false, force),
            None => self.start_walk(false, false, force),
        }
    }

    /// Init (goals 11.13 points 1 and 2): the target is a building; the legs come from its water
    /// area, the flags from the type found by the guest's return name id.
    pub(in crate::goal) fn ride_init(&mut self) -> bool {
        let id = self.record().target.id;
        let Some(at) = self.world.find(id) else {
            return false;
        };
        let Some((_, kind)) = self.building(id) else {
            return false;
        };
        let name = self.guest().and_then(|g| g.returning);
        let (idler, viewer) = flags(self.world, self.ctx.cat, name);
        let legs = building::ret::legs(self.world, at, &kind);
        let now = self.now();
        self.unit.goal.ride = Ride {
            legs,
            until: now,
            idler,
            idle: false,
            viewer,
        };
        true
    }

    /// Decide (goals 11.13 point 3): nothing while a leg is walked; else one leg less, a pause for
    /// an idler, or the next leg, and for a viewer a look at the animals ahead.
    pub(in crate::goal) fn ride_decide(&mut self) {
        if self.record().target.tile.is_some() {
            return;
        }
        let now = self.now();
        let ride = &mut self.unit.goal.ride;
        ride.legs -= 1;
        if ride.idler {
            ride.idle = !ride.idle && ride.legs >= 1;
            if ride.idle {
                ride.until = now.wrapping_add(self.ctx.rng.pick(7) + 6);
                if let Some(t) = self.tile() {
                    self.path_to(t, false);
                }
                self.walk_set(TREAD, false, true);
            }
        }
        if !self.unit.goal.ride.idle && !self.ride_leg() {
            self.record().done = true;
        }
        if self.unit.goal.ride.viewer {
            self.ride_view();
        }
    }

    /// Point 3.3: a path to a water tile near the guest, and the walk. Whether it started.
    fn ride_leg(&mut self) -> bool {
        let Some(here) = self.tile() else {
            return false;
        };
        let width = self.world.zoo.width as i32;
        let (x, y) = (here as i32 % width, here as i32 / width);
        let reach = if self.unit.goal.ride.idler { 6 } else { 10 };
        for _ in 0..ROUNDS {
            let mut tile = None;
            for _ in 0..TRIES {
                let dx = self.ctx.rng.pick(reach) as i32 - reach as i32 / 2;
                let dy = self.ctx.rng.pick(reach) as i32 - reach as i32 / 2;
                // The last try's tile is requested as it is when every try failed (goals O12:
                // none, off the map, is a failed request).
                tile = self.world.tile_index(x + dx, y + dy);
                if tile.is_some_and(|t| self.swims_to(here, t)) {
                    break;
                }
            }
            if tile.is_some_and(|t| self.path_to(t, false)) {
                return self.return_walk(true, true);
            }
        }
        for (dx, dy) in DIAGONALS {
            let tile = self.world.tile_index(x + dx, y + dy);
            if tile.is_some_and(|t| self.swims_to(here, t) && self.path_to(t, false)) {
                return self.return_walk(true, true);
            }
        }
        false
    }

    /// Whether tile `t` is water the guest on tile `here` may enter.
    fn swims_to(&self, here: usize, t: usize) -> bool {
        crate::unit::walk::fits(self.unit, self.world, here, t, true) && self.world.is_water(t)
    }

    /// Point 7: the viewing step. The habitat three tiles ahead, when it holds animals, makes a
    /// guest happy if one is overjoyed: one draw.
    pub(super) fn ride_view(&mut self) {
        let Some(here) = self.tile() else { return };
        let facing = self.world.zoo.entities[self.at].common.as_ref();
        let (dx, dy) = STEP[facing.map_or(0, |c| c.rotation as usize % 8)];
        let width = self.world.zoo.width as i32;
        let ahead = self
            .world
            .tile_index(here as i32 % width + 3 * dx, here as i32 / width + 3 * dy);
        let world = &*self.world;
        let habitat = ahead
            .and_then(|t| world.exhibits().region(t))
            .filter(|&x| x != GROUNDS && x != EXTERIOR);
        let Some(x) = habitat else { return };
        let held = view::beasts(world, self.ctx.cat);
        let all: Vec<(u32, i32)> = crate::animal::exhibit_animals(world, x)
            .into_iter()
            .filter_map(|at| {
                let id = world.zoo.entities[at].common.as_ref()?.id;
                held.iter().find(|b| b.id == id).map(|b| (b.id, b.happy))
            })
            .collect();
        if all.is_empty() {
            return;
        }
        let animal = view::pick(self.ctx.rng, &all, 81..=100);
        let Some(k) = crate::guest::keys(self.world, self.ctx.cat, self.at) else {
            return;
        };
        if let (Some(animal), Part::Guest(g)) = (animal, &mut self.unit.part) {
            g.pending[HAPPINESS] = g.pending[HAPPINESS].saturating_add(k.view.happy[0]);
            self.think(OVERJOYED, Some(animal));
        }
    }

    /// Check done (goals 11.13 point 4): an idler's pause ends with its time; a leg ends when its
    /// tile is reached, the last one when the legs have run out.
    pub(in crate::goal) fn ride_check_done(&mut self) {
        let ride = &self.unit.goal.ride;
        let (idle, until, legs) = (ride.idle, ride.until, ride.legs);
        let over = match idle {
            true => self.now() >= until,
            false => self.reached(false),
        };
        if over {
            let world = &*self.world;
            self.unit.goal.record.target.set_tile(world, None);
            self.record().done = !idle && legs < 0;
        }
    }

    /// Complete (goals 11.13 point 5): the guest looks for a building of the type it left; found,
    /// the building becomes its target and the ladder walks it there, else it recovers from the
    /// water.
    pub(in crate::goal) fn ride_complete(&mut self) {
        let found = self.back_building();
        self.set_task(NONE, 0, 0, None);
        match (found, self.guest()) {
            (Some(b), Some(g)) => (g.need, g.building) = (Some("fun"), b),
            _ => self.set_task(RECOVER, 0, 0, None),
        }
    }

    /// Invalidate target (goals 11.13 point 6): the target building, when it still stands and keeps
    /// track of its guests, loses this one from its members; the guest recovers from the water.
    pub(in crate::goal) fn ride_invalidate(&mut self) {
        let (id, unit) = (self.record().target.id, self.unit.id);
        if self.building(id).is_some_and(|(_, k)| k.ret.tracker)
            && let Some(live) = self.world.units.buildings.get_mut(&id)
        {
            live.members.retain(|&m| m != unit);
        }
        self.set_task(RECOVER, 0, 0, None);
    }

    /// Guest buildings 12.2 point 18: a building of the guest's return type, in its habitat, with
    /// a slot open to it; one draw among them, none for none.
    fn back_building(&mut self) -> Option<u32> {
        let name = self.guest()?.returning?;
        let (here, who) = (self.tile()?, self.species());
        let world = &*self.world;
        let area = world.exhibits().region(here);
        let default = world.units.prices.default;
        let near: Vec<u32> = (world.zoo.entities.iter())
            .filter_map(|e| {
                let (kind, c) = (
                    building::kind_of(self.ctx.cat, e, default)?,
                    e.common.as_ref()?,
                );
                let t = world.tile_index(c.x.div_euclid(64), c.y.div_euclid(64))?;
                let live = world.units.buildings.get(&c.id);
                let found = kind.name_id == name && kind.satisfies("fun");
                (found && world.exhibits().region(t) == area).then_some(())?;
                building::has_open_slot(world, e, &kind, live, who.as_ref()).then_some(c.id)
            })
            .collect();
        (!near.is_empty()).then(|| near[self.ctx.rng.pick(near.len() as u32) as usize])
    }

    /// The building tiles update of a returning guest (guest buildings 12.2 point 19): its
    /// building target is walked to; if that fails it leaves the building's members and recovers
    /// from the water, mode 1. Whether the guest is returning with a target and no task.
    pub(in crate::goal) fn return_ladder(&mut self) -> bool {
        let unit = self.unit.id;
        let (building, returning) = match self.guest() {
            Some(g) => (g.building, g.returning.is_some()),
            None => return false,
        };
        if !returning || building == 0 || self.record().task != NONE {
            return false;
        }
        if !self.set_out(building) {
            if let Some(live) = self.world.units.buildings.get_mut(&building) {
                live.members.retain(|&m| m != unit);
            }
            self.set_task(RECOVER, 1, 0, None);
        }
        true
    }

    /// The guest's AI reset (guest buildings 12.2 point 21, simulation 8.1): a guest in the
    /// building tasks leaves its building by a normal leave, everything stops, it waits 1000 to
    /// 4999 ms, and its return name id is 0; the string and any membership stay. Called by the
    /// stir of a tank (point 22).
    pub(crate) fn guest_reset_ai(&mut self) {
        if [ENTER_BUILDING, EXIT_BUILDING, USE].contains(&self.record().task) {
            self.remove_user();
        }
        stop_all(self.unit);
        self.unit.wait = 1000 + self.ctx.rng.pick(4000) as i32;
        if let Some(g) = self.guest() {
            g.returning = None;
        }
    }
}
