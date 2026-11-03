//! The animal side of a show (shows 18 to 25): the events it reads. The rest is a stub until Shows
//! 2: its calls, its validation and its release. The trick it starts is `goal::start_trick`. Each
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

/// Reads the events addressed to `animal` and sets the flags of its item state (shows 24), in
/// order, each row's effects in its own order.
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

/// The show is over for `animal`: its goal is reset and it is sent home (shows 5.6, 21).
pub(super) fn release(_world: &mut World, _animal: u32) {}
