//! The users of a building in a zoo just loaded (guest buildings 4.1, zoo format 6.9): each pair
//! (unit id, slot) of the record is checked against the world, and a unit that is there is back in
//! its slot with the time it entered at (guest buildings 4.1, 4.2).

use super::{Kind, Live};
use crate::entity::Catalogue;
use crate::goal::{ENTER_BUILDING, EXIT_BUILDING, USE};
use crate::unit::Part;
use crate::world::World;

/// Puts back the users of every guest building, after all the guests are made (`building::load`
/// made the live states). A pair whose unit does not exist, is neither a guest nor an animal
/// (staff use no guest building here), is a user already, or names a slot the type does not have
/// is dropped. A kept guest or animal is listed in entering order with the entity stamp of its
/// record as its entry time (0: `now`, the AI tick of the load), hidden when the type hides its
/// users, and goes on with the task its unit block stores when that is entering, using or leaving,
/// so a user leaves after the time it has left (a guest pays then); a unit saved in another task
/// ends its use at once. A user that keeps its task goes on with its saved set
/// and its pending set name (behaviours 4.7); a guest whose record has no unit block leaves at once.
/// The stored count of users is not brought back to the list: the pairs dropped stay counted
/// (`Live::extra`).
/// A file before version 80 restores none of the users of a type whose users are visible inside
/// (a bench, a picnic table): the list stays empty, the pairs are counted off the guest figures and
/// each unit in a building task restarts its use set, which adds it again (guest buildings 4.2).
pub fn restore(world: &mut World, cat: &Catalogue, now: u32) {
    for at in 0..world.zoo.entities.len() {
        let e = &world.zoo.entities[at];
        let Some(record) = e.common.as_ref().and_then(|c| c.building.as_ref()) else {
            continue;
        };
        let (id, pairs) = (e.common.as_ref().map_or(0, |c| c.id), record.users.clone());
        let record_now = record.users_now;
        let Some(kind) = super::kind_of(cat, e, world.units.prices.default) else {
            continue;
        };
        if !world.units.buildings.contains_key(&id) {
            continue;
        }
        // A file before version 80 restores no user of a type whose users are visible inside: the
        // list is emptied, the guest figures go down by the pairs that were kept, and each kept
        // unit restarts its use set.
        if world.zoo.version < 80 && !kind.hide && !kind.outside {
            let key = world.zoo.entities[at].subtype.clone();
            let kept = (pairs.iter())
                .filter(|&&(unit, slot)| restart(world, id, &key, unit, slot as usize, now))
                .count();
            if let Some(live) = world.units.buildings.get_mut(&id) {
                let kept = u32::try_from(kept).unwrap_or(u32::MAX);
                live.guests = live.guests.map(|g| g.saturating_sub(kept));
            }
            continue;
        }
        for (unit, slot) in pairs {
            seat(world, id, &kind, unit, slot as usize, now);
        }
        // The count is not brought back to the list: a dropped pair stays counted.
        if let Some(live) = world.units.buildings.get_mut(&id) {
            let counted = live.users.len() as u32;
            live.extra = record_now.saturating_sub(counted);
        }
    }
}

/// A kept pair of a file before version 80 for a type visible inside: the unit goes on in its
/// task and starts its use set again from the first call (guest buildings 4.2 point 3, 6.4.1);
/// the set adds it as a user. Whether the unit exists.
fn restart(world: &mut World, id: u32, key: &str, unit: u32, slot: usize, now: u32) -> bool {
    let Some(at) = world.find(unit) else {
        return false;
    };
    let block = world.zoo.entities[at]
        .common
        .as_ref()
        .and_then(|c| c.unit.as_ref());
    let saved = block.map(|b| b.target.goal);
    let need = block.and_then(|b| crate::guest::NEED_LIST.get(usize::from(b.need)).copied());
    let inside = [ENTER_BUILDING, EXIT_BUILDING, USE];
    let kept = saved
        .and_then(|t| u8::try_from(t).ok())
        .filter(|t| inside.contains(t));
    let (Some(u), Some(_)) = (world.units.get_mut(unit), kept) else {
        return true;
    };
    let need = match &mut u.part {
        Part::Guest(_) => need,
        Part::Animal(_) => Some(crate::animal::NEED),
        _ => return true,
    };
    crate::goal::resume_use(u, id, slot, now, saved);
    if let Part::Guest(g) = &mut u.part {
        g.need = need;
    }
    u.use_set = Some(format!("b{key}{}{slot}", need.unwrap_or_default()));
    true
}

/// One pair of `restore`.
fn seat(world: &mut World, id: u32, kind: &Kind, unit: u32, slot: usize, now: u32) {
    let listed = (world.units.buildings.values()).any(|l| l.users.iter().any(|u| u.0 == unit));
    let (Some(at), false, true) = (world.find(unit), listed, slot < kind.slots.len()) else {
        return;
    };
    let common = world.zoo.entities[at].common.as_ref();
    let stamp = common.and_then(|c| c.stamp);
    let block = common.and_then(|c| c.unit.as_ref());
    let saved = block.map(|b| b.target.goal);
    let set = block.map(|b| b.behaviour.clone());
    let pending = block.map(|b| b.pending.clone());
    let Some(u) = world.units.get_mut(unit) else {
        return;
    };
    if !matches!(u.part, Part::Guest(_) | Part::Animal(_)) {
        return;
    }
    let (inside, guest) = (
        !kind.hide && !kind.outside,
        matches!(u.part, Part::Guest(_)),
    );
    // A record without a unit block has no use set: a guest on a seat leaves at once.
    let saved = match saved {
        None if inside && guest => Some(i32::from(EXIT_BUILDING)),
        _ => saved,
    };
    crate::goal::resume_use(u, id, slot, now, saved);
    // A user that keeps its task goes on with its saved set: the current call again from its
    // start, then the rest; for a user visible inside the set's `fExitBuilding` releases it
    // (behaviours 4.7, guest buildings 6.5).
    let kept = (saved.and_then(|t| u8::try_from(t).ok()))
        .is_some_and(|t| [ENTER_BUILDING, EXIT_BUILDING, USE].contains(&t));
    if let (true, Some(set), Some(name)) = (kept, &set, &pending) {
        u.runner.restore(set);
        crate::unit::resume::keep_pending(u, name);
    }
    u.hidden = kind.hide;
    if kind.hide {
        world.hide_unit(at);
    }
    let entered = stamp.unwrap_or(now);
    if let Some(Live { users, .. }) = world.units.buildings.get_mut(&id) {
        users.push((unit, slot, entered));
    }
}
