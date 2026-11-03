//! The animal side of a show (shows 18 to 25): the events it reads and its release.
//! The trick it starts is `goal::start_trick`. Each function is the place the cycle calls the animal at.

use super::event;
use crate::entity::Catalogue;
use crate::goal::reset_unit;
use crate::sim::{Events, UNIT_CHANNEL};
use crate::world::World;
use crate::zoo::ItemState;

/// Reads the events addressed to `animal` and sets the flags of its item state (shows 24), in
/// order, each row's effects in its own order: the call raises its called flag and lowers its
/// return flag; its arrival resets its goal and sets it finished and ready.
pub(super) fn read_events(
    world: &mut World,
    cat: &Catalogue,
    events: &Events,
    animal: u32,
    state: &mut ItemState,
) {
    let heard = events.listen(animal, UNIT_CHANNEL);
    let heard: Vec<u16> = heard
        .filter(|e| e.target == animal)
        .map(|e| e.kind)
        .collect();
    for kind in heard {
        match kind {
            event::CALL => {
                if let Some(u) = world.units.get_mut(animal) {
                    (u.back, u.called) = (false, true);
                }
            }
            event::ARRIVED => {
                reset_unit(world, cat, animal);
                (state.finished, state.ready) = (true, true);
            }
            event::CUE => state.ready = true,
            event::FINISHED => {
                state.finished = true;
                reset_unit(world, cat, animal);
            }
            event::FAILED => (state.finished, state.aborted) = (true, true),
            event::ITEM_ABORTED => reset_unit(world, cat, animal),
            _ => {}
        }
    }
}

/// The show is over for `animal`: its goal is reset and it is sent home (shows 5.6 `reinit`).
pub(super) fn release(world: &mut World, cat: &Catalogue, animal: u32) {
    reset_unit(world, cat, animal);
    if let Some(u) = world.units.get_mut(animal) {
        (u.back, u.called) = (true, false);
    }
}
