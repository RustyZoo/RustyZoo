//! The users of a building in a zoo just loaded (guest buildings 4.1, zoo format 6.9): each pair
//! (unit id, slot) of the record is checked against the world, and a unit that is there is back in
//! its slot with the time it entered at (guest buildings 4.1, 4.2).

use super::{Kind, Live};
use crate::entity::Catalogue;
use crate::unit::Part;
use crate::world::World;

/// Puts back the users of every guest building, after all the guests are made (`building::load`
/// made the live states). A pair whose unit does not exist, is neither a guest nor an animal
/// (staff use no guest building here), is a user already, or names a slot the type does not have
/// is dropped. A kept guest or animal is listed in entering order with the entity stamp of its
/// record as its entry time (0: `now`, the AI tick of the load), hidden when the type hides its
/// users, and goes on with the task of using the building, so it leaves after the time it has
/// left (a guest pays then).
/// A file before version 80 restores none of the users of a type whose users are visible inside
/// (a bench, a picnic table): their pairs are skipped and counted off the guest figures
/// (guest buildings 4.2).
pub fn restore(world: &mut World, cat: &Catalogue, now: u32) {
    for at in 0..world.zoo.entities.len() {
        let e = &world.zoo.entities[at];
        let Some(record) = e.common.as_ref().and_then(|c| c.building.as_ref()) else {
            continue;
        };
        let (id, pairs) = (e.common.as_ref().map_or(0, |c| c.id), record.users.clone());
        let Some(kind) = super::kind_of(cat, e, world.units.prices.default) else {
            continue;
        };
        if !world.units.buildings.contains_key(&id) {
            continue;
        }
        // A file before version 80 restores no user of a type whose users are visible inside: the
        // list is emptied, and the guest figures go down by the pairs that were kept.
        if world.zoo.version < 80 && !kind.hide && !kind.outside {
            let kept = pairs.iter().filter(|p| world.find(p.0).is_some()).count();
            if let Some(live) = world.units.buildings.get_mut(&id) {
                let kept = u32::try_from(kept).unwrap_or(u32::MAX);
                live.guests = live.guests.map(|g| g.saturating_sub(kept));
            }
            continue;
        }
        for (unit, slot) in pairs {
            seat(world, id, &kind, unit, slot as usize, now);
        }
    }
}

/// One pair of `restore`.
fn seat(world: &mut World, id: u32, kind: &Kind, unit: u32, slot: usize, now: u32) {
    let listed = (world.units.buildings.values()).any(|l| l.users.iter().any(|u| u.0 == unit));
    let (Some(at), false, true) = (world.find(unit), listed, slot < kind.slots.len()) else {
        return;
    };
    let stamp = (world.zoo.entities[at].common.as_ref()).and_then(|c| c.stamp);
    let Some(u) = world.units.get_mut(unit) else {
        return;
    };
    if !matches!(u.part, Part::Guest(_) | Part::Animal(_)) {
        return;
    }
    crate::goal::resume_use(u, id, slot, now);
    u.hidden = kind.hide;
    if kind.hide {
        world.hide_unit(at);
    }
    let entered = stamp.unwrap_or(now);
    if let Some(Live { users, .. }) = world.units.buildings.get_mut(&id) {
        users.push((unit, slot, entered));
    }
}
