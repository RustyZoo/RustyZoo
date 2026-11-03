//! The animal side of a show (shows 18 to 25): its calls and its release. The tricks are Shows
//! 2d's, a stub until then: no item ever finishes, so a show that starts ends only by abort. Each
//! function is the place the cycle calls the animal at.

use super::event;
use crate::entity::Catalogue;
use crate::goal::reset_unit;
use crate::sim::{Events, UNIT_CHANNEL};
use crate::world::World;
use crate::zoo::ItemState;

/// Whether the animals can do item `item` of the script of `species` in this show (shows 23.6);
/// 0 for yes, else the code that aborts the item.
pub(super) fn validate_item(_world: &World, _species: u32, _item: i16) -> i32 {
    0
}

/// Reads the events addressed to `animal` and sets the flags of its item state (shows 24): the
/// call raises its called flag and lowers its return flag; its arrival resets its goal and sets
/// it finished and ready. The other rows are Shows 2d's.
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
            _ => {}
        }
    }
}

/// Asks `animal` to start the trick of item `item` of show `show` (shows 23.2); 0 when it did.
pub(super) fn start_trick(_world: &mut World, _animal: u32, _item: u16, _show: u16) -> i32 {
    0
}

/// The show is over for `animal`: its goal is reset and it is sent home (shows 5.6 `reinit`).
pub(super) fn release(world: &mut World, cat: &Catalogue, animal: u32) {
    reset_unit(world, cat, animal);
    if let Some(u) = world.units.get_mut(animal) {
        (u.back, u.called) = (true, false);
    }
}
