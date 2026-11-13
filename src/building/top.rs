//! Taking a user out of a building and the top user of its slot (guest buildings 6.8, map drawing
//! 9.5.1 point 8).

use super::leave::show;
use crate::goal::{ENTER_BUILDING, USE};
use crate::unit::Unit;
use crate::world::World;

/// Guest buildings 6.8 for a listed unit: it leaves the users list of building `id` and the
/// slot's count, and the slot's top user is chosen again (9.5.1 point 8.2): when the unit was the
/// top user and other users of the slot are left, the first listed pair of the slot (the unit's own
/// still in the list) whose unit exists becomes the top user if its task is entering, leaving or
/// using; a refused one is taken out too when the list held more than one pair. `me` is the unit
/// that leaves and its task, when the unit table does not hold it. The slot, or none when the unit
/// was no listed user. No money and no change record.
pub(crate) fn yank(world: &mut World, id: u32, unit: u32, me: Option<(u32, u8)>) -> Option<usize> {
    let live = world.units.buildings.get(&id)?;
    let before = live.users.clone();
    let i = before.iter().position(|u| u.0 == unit)?;
    let slot = before[i].1;
    let was_top = live.top(slot) == Some(unit);
    let first = (before.iter())
        .find(|p| p.1 == slot && world.find(p.0).is_some())
        .map(|p| p.0);
    let task = first.and_then(|f| match me {
        Some((m, task)) if m == f => Some(task),
        _ => world.units.get(f).map(|u| u.goal.record.task),
    });
    let live = world.units.buildings.get_mut(&id)?;
    live.short = false;
    live.users.remove(i);
    if !live.users.iter().any(|u| u.1 == slot) {
        live.tops.remove(&slot);
        return Some(slot);
    }
    if let (true, Some(f)) = (was_top, first) {
        match task {
            Some(ENTER_BUILDING..=USE) => {
                live.tops.insert(slot, f);
            }
            _ if before.len() > 1 => {
                // The top user stays; the refused unit goes too.
                if f == unit {
                    lower(live);
                } else {
                    yank(world, id, f, me);
                }
            }
            _ => {}
        }
    }
    Some(slot)
}

/// Users now one lower, never below the list length minus one (guest buildings 6.8).
fn lower(live: &mut super::Live) {
    match live.extra {
        0 => live.short = !live.users.is_empty(),
        _ => live.extra -= 1,
    }
}

/// A building draws a top user whose task is none of entering, leaving and using: it takes the
/// unit out (map drawing 9.5.1 point 4.3) and shows it where it stands. A unit that is no listed
/// user only lowers users now; the building's top user stays.
pub(crate) fn take_out(world: &mut World, id: u32, unit: u32) {
    if yank(world, id, unit, None).is_none()
        && let Some(live) = world.units.buildings.get_mut(&id)
    {
        lower(live);
    }
    let (Some(at), Some(entry)) = (world.find(unit), world.units.get_mut(unit)) else {
        return;
    };
    let mut u = std::mem::replace(entry, Unit::new(unit, entry.kind));
    show(world, at, &mut u);
    if let Some(entry) = world.units.get_mut(unit) {
        *entry = u;
    }
}
