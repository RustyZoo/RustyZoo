//! What a building draws besides its own picture: its users, and whether it draws the picture at
//! all (map drawing 9.5.1).

use super::kind_of;
use crate::entity::Catalogue;
use crate::goal::{ENTER_BUILDING, USE};
use crate::render::Hide;
use crate::world::World;

/// What the buildings draw of their users in one frame.
#[derive(Debug, Default)]
pub(crate) struct Drawn {
    /// The pairs (building id, unit id) of the users drawn right after their building, in order.
    pub pad: Vec<(u32, u32)>,
    /// The buildings that draw no picture of their own: `cHideBuilding` 1 with a user.
    pub bare: Vec<u32>,
    /// The pairs (building id, unit id) of a top user in another task than entering, leaving or
    /// using: drawing the building takes it out (point 4.3).
    pub leave: Vec<(u32, u32)>,
}

/// The users drawn over their building (9.5.1): for a type with `cDrawUser` 1, or visible inside
/// (`cHideUser` 0 and `cUserStaysOutside` 0), the units on the draw places of a slot that has some,
/// in the order list of the building's rotation, else the slot's top user when its task is
/// entering, leaving or using. The users are the live list of a guest building, else the record
/// of the zoo (the helicopter of its base). A building the buttons of the main bar hide draws
/// nothing.
pub(crate) fn drawn_users(world: &World, cat: &Catalogue, hide: Hide) -> Drawn {
    let mut out = Drawn::default();
    for e in &world.zoo.entities {
        let Some(c) = e.common.as_ref() else { continue };
        let Some(record) = c.building.as_ref() else {
            continue;
        };
        let Some(kind) = kind_of(cat, e, world.units.prices.default) else {
            continue;
        };
        if crate::entity::type_of(cat, e).is_none_or(|ty| hide.hides(ty)) {
            continue;
        }
        let live = world.units.buildings.get(&c.id);
        let listed = live.map_or(record.users.len(), |l| l.users.len());
        if listed == 0 {
            continue;
        }
        if kind.hide_building {
            out.bare.push(c.id);
        }
        if !(kind.draw_user || !kind.hide && !kind.outside) {
            continue;
        }
        let rotation = match c.rotation % 8 {
            r @ (2 | 4 | 6) => r as usize / 2,
            _ => 0,
        };
        for (slot, info) in kind.slots.iter().enumerate() {
            if info.places.is_some() {
                let seats = live.map_or(&[][..], |l| &l.seats);
                for &place in &info.orders[rotation] {
                    let seated = seats
                        .iter()
                        .find(|s| s.slot == slot && s.place as i32 == place);
                    out.pad.extend(seated.map(|s| (c.id, s.unit)));
                }
                continue;
            }
            let top = match live {
                Some(l) => l.top(slot),
                None => record
                    .users
                    .iter()
                    .rev()
                    .find(|u| u.1 as usize == slot)
                    .map(|u| u.0),
            };
            let Some((top, u)) = top.and_then(|t| Some((t, world.units.get(t)?))) else {
                continue;
            };
            match (u.goal.record.task, live) {
                (ENTER_BUILDING..=USE, _) => out.pad.push((c.id, top)),
                (_, Some(_)) => out.leave.push((c.id, top)),
                _ => {}
            }
        }
    }
    out
}
