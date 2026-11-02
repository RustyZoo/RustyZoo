//! A unit picked up by hand and put down again (unit placing 8.2, 8.5, 8.6). The pick-up stops
//! the goal of the unit; the drop (and the put-back after a cancelled move) runs the rule of its
//! kind: a guest on water outside a tank starts the recovery from water and thinks 10568, a staff
//! member forgets the targets it could not reach, a keeper in an exhibit starts its duties.

use super::{Cx, DUTIES, RECOVER};
use crate::behaviour::Data;
use crate::entity::Catalogue;
use crate::sim::{Events, Rng};
use crate::unit::{Ctx, Kind, Part, Unit};
use crate::world::World;

/// The thought of a guest that finds itself in the water.
const IN_WATER: i32 = 10568;

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

/// Unit placing 8.2 point 3: the unit picked up stops its goal (the reset of goals 12.1).
pub fn picked_up(world: &mut World, cat: &Catalogue, rng: &mut Rng, data: &dyn Data, id: u32) {
    run(world, cat, rng, data, id, |cx| cx.reset());
}

/// Unit placing 8.5: the drop rule of the unit `id`, put down on the map.
pub fn dropped(world: &mut World, cat: &Catalogue, rng: &mut Rng, data: &dyn Data, id: u32) {
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
            Kind::Animal => {}
        }
    }

    /// A guest dropped on water outside a tank starts the recovery from water and thinks 10568
    /// (tank fauna 13); it then chooses a new goal (guests 8: the wandered flag).
    fn guest_dropped(&mut self) {
        let Some(tile) = self.tile() else { return };
        let region = self.world.exhibits().region(tile);
        let tank = region.is_some_and(|r| self.world.tank(r).is_some());
        if self.world.is_water(tile) && !tank {
            self.set_task(RECOVER, 0, 0, None);
            self.think(IN_WATER, None);
        }
        self.unit.goal.wandered = true;
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
