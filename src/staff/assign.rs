//! Assigning exhibits to staff (staff 6): may a keeper serve an exhibit, what the assign mode
//! says and does on a click, and what an exhibit that goes does to the lists.

use crate::build::number;
use crate::entity::{Catalogue, EntityType};
use crate::panel::exhibit::animals_in;
use crate::place::bulldozer::type_of;
use crate::unit::Part;
use crate::world::World;

/// The message of an exhibit that is too small, the one of an empty exhibit, and the message of a
/// refusal by the keeper's type id: the zookeeper's, the scientist's and the marine specialist's.
const SMALL: i32 = 10323;
const EMPTY: i32 = 10329;
const ZOOKEEPER: (i32, i32) = (9500, 10320);
const SCIENTIST: (i32, i32) = (9551, 10324);
const MARINE: (i32, i32) = (9552, 10328);

/// What a click of the assign mode did.
#[derive(Debug, PartialEq, Eq)]
pub enum Outcome {
    /// The exhibit is in the list now (it was, or it is the last).
    Assigned,
    /// Nothing changed; the message to show, if the type has one.
    Refused(Option<i32>),
}

/// Whether the unit is a keeper of the keeper class (the zookeeper, the scientist, the marine
/// specialist); the guide and the worker are not.
fn keeper(ty: &EntityType) -> bool {
    ty.class == "keeper"
}

/// Whether an animal of the exhibit is of the keeper's kind: its species names his type id in
/// `cKeeper` (9500 by default). A carried animal is in no exhibit.
fn has_his_animals(world: &World, cat: &Catalogue, exhibit: u32, keeper: &EntityType) -> bool {
    animals_in(world, exhibit).into_iter().any(|at| {
        let ty = type_of(cat, &world.zoo.entities[at]);
        let kind = ty
            .and_then(|t| number(t, "cKeeper"))
            .map_or(9500, |v| v as i32);
        kind == keeper.name_id
    })
}

/// `blockService(exhibit, keeper, kindOnly 1, checkKind 1)` (habitat rating 11.1): 0 when the
/// keeper has a reason to go to the exhibit, 1 when it has fewer than 2 tiles, 2 when it is of
/// another kind or has nothing for him. A unit that is not a keeper is always 0. The dung of the
/// keeper's type on a tile (step 5) is not looked at: nothing puts dung down yet.
pub fn block_service(world: &World, cat: &Catalogue, exhibit: u32, who: &EntityType) -> u8 {
    let Some(x) = world.exhibits().get(exhibit).filter(|_| keeper(who)) else {
        return 0;
    };
    let marine = number(who, "cUnderwater") == Some(1.0);
    match (x.tiles < 2, x.tank.is_some() == marine) {
        (true, _) => 1,
        (_, false) => 2,
        _ if has_his_animals(world, cat, exhibit, who) => 0,
        _ if x.tank.is_some_and(|t| t.show) && who.name_id == MARINE.0 => 0,
        _ => 2,
    }
}

/// The message of a refusal (staff 6 point 1): by the keeper's type, whether the exhibit is a tank or
/// has species, else the one of an empty exhibit; a type that has none (the guide) says nothing.
fn refusal(world: &World, cat: &Catalogue, exhibit: u32, who: &EntityType) -> Option<i32> {
    let tank = world.exhibits().get(exhibit)?.tank.is_some();
    let species = !crate::exhibit::rating::input(world, cat, exhibit)
        .species
        .is_empty();
    let (id, text) = [ZOOKEEPER, SCIENTIST, MARINE]
        .into_iter()
        .find(|&(id, _)| id == who.name_id)?;
    let fits = match id {
        9552 => !tank || species,
        _ => tank || species,
    };
    Some(if fits { text } else { EMPTY })
}

/// A click of the assign mode on exhibit `exhibit` for the staff member `id` (staff 6 point 1).
/// Result 0 appends the exhibit to the list (one that is in it keeps its place: no duplicates); the other results change nothing and give their message.
pub fn assign(world: &mut World, cat: &Catalogue, id: u32, exhibit: u32) -> Outcome {
    let Some(at) = world.find(id) else {
        return Outcome::Refused(None);
    };
    let Some(who) = type_of(cat, &world.zoo.entities[at]) else {
        return Outcome::Refused(None);
    };
    let code = block_service(world, cat, exhibit, who);
    let message = match code {
        0 => None,
        1 => Some(Some(SMALL)),
        _ => Some(refusal(world, cat, exhibit, who)),
    };
    if let Some(m) = message {
        return Outcome::Refused(m);
    }
    if let Some(Part::Staff(s)) = world.units.get_mut(id).map(|u| &mut u.part)
        && !s.assigned.contains(&exhibit)
    {
        s.assigned.push(exhibit);
    }
    Outcome::Assigned
}

/// Removes the exhibit of position `row` of the staff member's list, or all of them (info panels
/// 9.3).
pub fn unassign(world: &mut World, id: u32, row: Option<usize>) {
    if let Some(Part::Staff(s)) = world.units.get_mut(id).map(|u| &mut u.part) {
        match row {
            Some(r) if r < s.assigned.len() => {
                s.assigned.remove(r);
            }
            Some(_) => {}
            None => s.assigned.clear(),
        }
    }
}

/// `clearStaffHabitat`: the exhibit that is gone leaves the list of every staff member (staff 6
/// point 3).
pub fn clear_habitat(world: &mut World, exhibit: u32) {
    let ids: Vec<u32> = world.units.iter().map(|u| u.id).collect();
    for id in ids {
        if let Some(Part::Staff(s)) = world.units.get_mut(id).map(|u| &mut u.part) {
            s.assigned.retain(|&e| e != exhibit);
        }
    }
}

#[cfg(test)]
mod tests;
