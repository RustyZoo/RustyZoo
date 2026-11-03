//! A unit picked up by hand and put down again (unit placing 8.2, 8.5, 8.6). The pick-up stops
//! the goal of the unit; the drop (and the put-back after a cancelled move) runs the rule of its
//! kind: a guest on water outside a tank starts the recovery from water and thinks 10568, a staff
//! member forgets the targets it could not reach, a keeper in an exhibit starts its duties.

use super::{Cx, DUTIES, RECOVER};
use crate::behaviour::{Data, NoData};
use crate::entity::Catalogue;
use crate::sim::{Events, Rng};
use crate::unit::{Ctx, Kind, Part, Unit};
use crate::world::World;

/// The thought of a guest that finds itself in the water.
const IN_WATER: i32 = 10568;
/// The needs a building may serve, in the order a dropped guest tests them.
const NEEDS: [&str; 4] = ["drink", "food", "bathroom", "energy"];

/// Runs `f` on the goal update context of the unit `id`, taken out of the table for it.
pub(super) fn with_unit<R>(
    world: &mut World,
    ctx: &mut Ctx,
    data: &dyn Data,
    id: u32,
    f: impl FnOnce(&mut Cx) -> R,
) -> Option<R> {
    let at = world.find(id)?;
    let slot = world.units.get_mut(id)?;
    let mut unit = std::mem::replace(slot, Unit::new(id, slot.kind));
    let r = f(&mut Cx {
        unit: &mut unit,
        world: &mut *world,
        ctx,
        at,
        data,
    });
    if let Some(slot) = world.units.get_mut(id) {
        *slot = unit;
    }
    Some(r)
}

/// Runs `f` on the unit `id` outside a step: nothing it does can ask for a removal.
fn run(
    world: &mut World,
    cat: &Catalogue,
    rng: &mut Rng,
    data: &dyn Data,
    id: u32,
    f: impl FnOnce(&mut Cx),
) {
    let (events, mut removals) = (Events::default(), Vec::new());
    let mut ctx = Ctx {
        rng,
        events: &events,
        removals: &mut removals,
        step: 0,
        cat,
    };
    with_unit(world, &mut ctx, data, id, f);
}

/// The goal of unit `id` is reset (goals 12.1) by code outside a unit's turn, as the show does for
/// its animals.
pub(crate) fn reset_unit(world: &mut World, cat: &Catalogue, id: u32) {
    run(world, cat, &mut Rng::default(), &NoData, id, |cx| {
        cx.reset()
    });
}

/// Unit placing 8.2 point 3: the unit picked up stops its goal (the reset of goals 12.1) and
/// loses its fright (the clearing of the state variables, simulation 8.2).
pub(crate) fn picked_up(
    world: &mut World,
    cat: &Catalogue,
    rng: &mut Rng,
    data: &dyn Data,
    id: u32,
) {
    run(world, cat, rng, data, id, |cx| {
        cx.reset();
        cx.unit.frightened = false;
        cx.unit.afraid = false;
        cx.unit.predator = 0;
    });
    // Shows 21.6 point 2: any unit picked up aborts the show of its show id.
    let show = world.units.get(id).map_or(0, |u| u.show);
    crate::show::abort_show(world, cat, show);
}

/// Unit placing 8.5: the drop rule of the unit `id`, put down on the map.
pub(crate) fn dropped(world: &mut World, cat: &Catalogue, rng: &mut Rng, data: &dyn Data, id: u32) {
    run(world, cat, rng, data, id, |cx| cx.drop_rule());
}

impl Cx<'_, '_> {
    fn drop_rule(&mut self) {
        match self.unit.kind {
            Kind::Guest => self.guest_dropped(),
            Kind::Keeper => {
                self.forget_unreached();
                self.keeper_dropped();
            }
            Kind::Worker | Kind::Guide => self.forget_unreached(),
            Kind::Animal | Kind::Heli => {}
        }
    }

    /// A guest dropped on water outside a tank leaves its leave flag down, starts the recovery
    /// from water in mode 1, thinks 10568 and is done (tank fauna 13). Any other drop: with a
    /// pressing need it takes the nearest satisfier within 10 tiles as its building target, else
    /// it chooses a new goal (guests 8: the wandered flag).
    fn guest_dropped(&mut self) {
        let Some(tile) = self.tile() else { return };
        let region = self.world.exhibits().region(tile);
        let tank = region.is_some_and(|r| self.world.tank(r).is_some());
        if self.world.is_water(tile) && !tank {
            if let Part::Guest(g) = &mut self.unit.part {
                g.leave = false;
            }
            self.set_task(RECOVER, 1, 0, None);
            self.think(IN_WATER, None);
            return;
        }
        if !self.pressing() || !self.take_satisfier(tile) {
            self.unit.goal.wandered = true;
        }
    }

    /// Whether the guest has a pressing need: hunger, thirst, energy or bathroom at or above its
    /// threshold.
    fn pressing(&self) -> bool {
        let (Part::Guest(g), Some(k)) = (
            &self.unit.part,
            crate::guest::keys(self.world, self.ctx.cat, self.at),
        ) else {
            return false;
        };
        g.pressing(&k)
    }

    /// Unit placing 8.5 point 3: the nearest building within 10 tiles in the guest's exhibit, with
    /// an open slot, that serves one of `drink`, `food`, `bathroom`, `energy` (the first in that
    /// order is the need); one draw among the equally near ones. It becomes the building target.
    fn take_satisfier(&mut self, here: usize) -> bool {
        let (world, width) = (&*self.world, self.world.zoo.width as usize);
        let (area, who) = (world.exhibits().region(here), self.species());
        let prices = world.units.prices;
        let (mut best, mut near) = (101, Vec::new());
        for e in &world.zoo.entities {
            let (Some(kind), Some(c)) = (
                crate::building::kind_of(self.ctx.cat, e, prices.default),
                e.common.as_ref(),
            ) else {
                continue;
            };
            let Some(t) = world.tile_index(c.x.div_euclid(64), c.y.div_euclid(64)) else {
                continue;
            };
            let Some(need) = NEEDS.into_iter().find(|n| kind.satisfies(n)) else {
                continue;
            };
            let live = world.units.buildings.get(&c.id);
            if world.exhibits().region(t) != area
                || !crate::building::has_open_slot(world, e, &kind, live, who.as_ref())
            {
                continue;
            }
            let dx = (t % width) as i64 - (here % width) as i64;
            let dy = (t / width) as i64 - (here / width) as i64;
            let d = dx * dx + dy * dy;
            if d < best {
                (best, near) = (d, Vec::new());
            }
            if d == best {
                near.push((c.id, need));
            }
        }
        if near.is_empty() {
            return false;
        }
        let (id, need) = near[self.ctx.rng.pick(near.len() as u32) as usize];
        if let Part::Guest(g) = &mut self.unit.part {
            (g.need, g.building) = (Some(need), id);
        }
        true
    }

    /// A staff member forgets its two lists of targets it could not reach; the assigned exhibits
    /// stay (unit placing 8.5).
    fn forget_unreached(&mut self) {
        if let Part::Staff(s) = &mut self.unit.part {
            (s.lost_tiles, s.lost_animals) = (Vec::new(), Vec::new());
        }
    }

    /// A keeper dropped in an exhibit starts its duties there, unless two keepers work in it
    /// (staff 7.10).
    fn keeper_dropped(&mut self) {
        let Some(x) = self.exhibit() else { return };
        if crate::staff::service::present(self.world, x) < 2 {
            self.set_task(DUTIES, 0, 0, None);
        }
    }
}
