//! The animal side of a show (shows 18 to 25), a stub until Shows 2: no animal answers the show,
//! so no item ever finishes and a show that starts ends only by abort. Each function is the
//! place the cycle calls the animal at.

use crate::world::World;
use crate::zoo::ItemState;

/// Whether the animals can do item `item` of the script of `species` in this show (shows 23.6);
/// 0 for yes, else the code that aborts the item.
pub(super) fn validate_item(_world: &World, _species: u32, _item: i16) -> i32 {
    0
}

/// Reads the events addressed to `animal` and sets the flags of its item state (shows 24).
pub(super) fn read_events(_world: &mut World, _animal: u32, _state: &mut ItemState) {}

/// Asks `animal` to start the trick of item `item` of show `show` (shows 23.2); 0 when it did.
pub(super) fn start_trick(_world: &mut World, _animal: u32, _item: u16, _show: u16) -> i32 {
    0
}

/// The show is over for `animal`: its goal is reset and it is sent home (shows 5.6, 21).
pub(super) fn release(_world: &mut World, _animal: u32) {}
