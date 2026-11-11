//! Who takes part in a show (shows 20): the unit lists of the type records, the default script a
//! species gets when it is met first (19.2) and the moves that feed them.
//!
//! The world notes each move (an animal on the map or off it, a show-neighbour link made or
//! dropped) and [`settle`] applies them at the start of the next step, where the catalogue is at
//! hand. A first rating of an exhibit adds the animals it counts ([`counted`]). The marine
//! specialist joins with his job ([`join`]) and leaves with it ([`leave`]).

use super::trick::{default_items, performs};
use crate::entity::{Catalogue, EntityType};
use crate::timing::file_time;
use crate::world::World;
use crate::zoo::{Item, TypeRecord};

/// What happened to the show lists, as the world tells it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Move {
    /// The animal came onto the map.
    On(u32),
    /// The animal left the map: its type and the exhibit it stood in.
    Off {
        id: u32,
        type_name: String,
        subtype: String,
        exhibit: Option<u32>,
    },
    /// A show-neighbour link between a plain tank and a show tank was made (`on`) or dropped.
    Link { plain: u32, show: u32, on: bool },
    /// A show decision ended: the grandstands facing a show tank take its show id (shows 31.2).
    Associate,
    /// An exhibit that held a show info is gone: its show id and the tiles it held (shows 30.4).
    Gone { show: u16, tiles: Vec<usize> },
}

/// The animal type of the entity with this name, if it performs.
fn performer<'a>(cat: &'a Catalogue, type_name: &str, subtype: &str) -> Option<&'a EntityType> {
    let bytes = |s: &str| s.chars().map(|c| c as u8).collect::<Vec<u8>>();
    let ty = cat.find("animals", &bytes(type_name), &bytes(subtype))?;
    performs(ty).then_some(ty)
}

/// Applies the moves the world noted.
pub(crate) fn settle(world: &mut World, cat: &Catalogue) {
    for m in world.take_moves() {
        match m {
            Move::On(id) => {
                let at = world.find(id);
                let e = at.map(|at| &world.zoo.entities[at]);
                let ty = e.and_then(|e| performer(cat, &e.type_name, &e.subtype));
                let exhibit = at.and_then(|at| world.exhibit_of(at));
                if let (Some(ty), Some(x)) = (ty, exhibit) {
                    walk(world, cat, x, id, ty, true, &mut Vec::new());
                }
            }
            Move::Off {
                id,
                type_name,
                subtype,
                exhibit,
            } => {
                if let (Some(ty), Some(x)) = (performer(cat, &type_name, &subtype), exhibit) {
                    walk(world, cat, x, id, ty, false, &mut Vec::new());
                }
            }
            Move::Associate => super::associate(world, cat),
            Move::Gone { show, tiles } => super::exhibit_gone(world, cat, show, &tiles),
            Move::Link { plain, show, on } => {
                let Some(n) = super::info_slot(world, show) else {
                    continue;
                };
                let near = world.exhibits().get(plain).map(|e| e.amphibious.clone());
                let from = std::iter::once(plain).chain(near.unwrap_or_default());
                let mut animals = Vec::new();
                for x in from {
                    animals.extend(performers(world, cat, x));
                }
                for (id, ty) in animals {
                    match on {
                        true => add_unit(world, cat, n, show, id, ty),
                        false => remove_unit(world, n, id, ty),
                    }
                }
            }
        }
    }
}

/// The first rating of exhibit `x` counted these animals: each performing one joins (shows 20.1).
pub(crate) fn counted(world: &mut World, cat: &Catalogue, x: u32, animals: &[u32]) {
    for &id in animals {
        let Some(e) = world.find(id).map(|at| &world.zoo.entities[at]) else {
            continue;
        };
        if let Some(ty) = performer(cat, &e.type_name, &e.subtype) {
            walk(world, cat, x, id, ty, true, &mut Vec::new());
        }
    }
}

/// The specialist `id` joins the show of exhibit `x` for the length of his job (shows 20 caller 4).
pub(crate) fn join(world: &mut World, cat: &Catalogue, x: u32, id: u32, ty: &EntityType) {
    walk(world, cat, x, id, ty, true, &mut Vec::new());
}

/// He leaves the list of his type in the show info of `show`, when there is one (shows 22.1 point
/// 6); a missing record is made empty.
pub(crate) fn leave(world: &mut World, show: u16, id: u32, key: u32) {
    if let Some(n) = super::habitat_of(world, show) {
        record(world, n, key).units.retain(|&u| u != id);
    }
}

/// The performing animals standing in exhibit `x`, with their types.
fn performers<'a>(world: &World, cat: &'a Catalogue, x: u32) -> Vec<(u32, &'a EntityType)> {
    (world.tiles_of(x).into_iter())
        .flat_map(|t| world.units_on(t))
        .filter_map(|at| {
            let e = &world.zoo.entities[at];
            let ty = performer(cat, &e.type_name, &e.subtype)?;
            Some((e.common.as_ref()?.id, ty))
        })
        .collect()
}

/// `add_show_unit` and `remove_show_unit` (shows 20): the animal goes to the show info of exhibit
/// `x`; an exhibit with none passes it on to its show neighbours, and a land exhibit with none to
/// its amphibious neighbours (shows 34.2). `seen` stops a walk that comes back to an exhibit.
fn walk(
    world: &mut World,
    cat: &Catalogue,
    x: u32,
    id: u32,
    ty: &EntityType,
    add: bool,
    seen: &mut Vec<u32>,
) {
    if seen.contains(&x) {
        return;
    }
    seen.push(x);
    if let Some(n) = super::info_slot(world, x) {
        match add {
            true => add_unit(world, cat, n, x, id, ty),
            false => remove_unit(world, n, id, ty),
        }
        return;
    }
    let Some(e) = world.exhibits().get(x) else {
        return;
    };
    let land = e.tank.is_none() && e.show_near.is_empty();
    let near = if land { &e.amphibious } else { &e.show_near };
    for y in near.clone() {
        walk(world, cat, y, id, ty, add, seen);
    }
}

/// `add_unit` (shows 20): a species the info has no record for gets its default script first; the
/// animal joins the record's list unless it is there.
fn add_unit(world: &mut World, cat: &Catalogue, n: usize, tank: u32, id: u32, ty: &EntityType) {
    let key = ty.name_id as u32;
    let known =
        (world.zoo.shows.infos[n].as_ref()).is_some_and(|i| i.records.iter().any(|r| r.key == key));
    if !known && let Some(items) = default_items(world, cat, &world.units.show, tank, ty) {
        add_script(world, n, key, items);
    }
    let record = record(world, n, key);
    if !record.units.contains(&id) {
        record.units.push(id);
    }
}

/// `remove_unit` (shows 20): the animal leaves its record's list; a missing record is made empty.
fn remove_unit(world: &mut World, n: usize, id: u32, ty: &EntityType) {
    record(world, n, ty.name_id as u32)
        .units
        .retain(|&u| u != id);
}

/// The record of type `key` in show info `n`; a type with none gets an empty one (shows 3.2).
fn record(world: &mut World, n: usize, key: u32) -> &mut TypeRecord {
    let date = file_time(world.units.now);
    let info = world.zoo.shows.infos[n].as_mut().expect("a show info");
    info.lookup(key, date)
}

/// `create_default_script` and `add_script` (shows 19.2, 5.6): the items become a script with the
/// next script id, and the type's record (a new one takes the default admission and the date)
/// takes it; while the show runs the record only holds it as pending. A script with items puts
/// the species on the schedule.
fn add_script(world: &mut World, n: usize, key: u32, items: Vec<Item>) {
    let id = super::edit::new_script(world, key, items);
    super::edit::add_script(world, n, key, id);
}

#[cfg(test)]
mod tests;
