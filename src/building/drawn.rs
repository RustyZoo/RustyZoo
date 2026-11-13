//! The users a building draws itself, right after its own picture (map drawing 9.5.1).

use super::kind_of;
use crate::entity::Catalogue;
use crate::goal::{ENTER_BUILDING, EXIT_BUILDING, USE};
use crate::world::World;

/// The pairs (building id, unit id) of the users drawn over their building: for a type with
/// `cDrawUser` 1, the last user of each slot without draw places, when its task is entering,
/// leaving or using. The users are the live list of a guest building, else the record of the zoo
/// (the helicopter of its base). The types visible inside (`cHideUser` 0, `cUserStaysOutside` 0)
/// are left out until their building picture can be hidden (`cHideBuilding`) as well.
pub(crate) fn drawn_users(world: &World, cat: &Catalogue) -> Vec<(u32, u32)> {
    let mut out = Vec::new();
    for e in &world.zoo.entities {
        let Some(c) = e.common.as_ref() else { continue };
        let Some(record) = c.building.as_ref() else {
            continue;
        };
        let Some(kind) = kind_of(cat, e, world.units.prices.default) else {
            continue;
        };
        if !kind.draw_user {
            continue;
        }
        let users: Vec<(u32, usize)> = match world.units.buildings.get(&c.id) {
            Some(live) => live.users.iter().map(|&(u, s, _)| (u, s)).collect(),
            None => record.users.iter().map(|&(u, s)| (u, s as usize)).collect(),
        };
        for (slot, info) in kind.slots.iter().enumerate() {
            let top = users.iter().rev().find(|u| u.1 == slot);
            if let (None, Some(&(unit, _))) = (&info.places, top)
                && let Some(u) = world.units.get(unit)
                && [ENTER_BUILDING, EXIT_BUILDING, USE].contains(&u.goal.record.task)
            {
                out.push((c.id, unit));
            }
        }
    }
    out
}
