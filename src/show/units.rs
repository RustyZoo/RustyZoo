//! Who takes part in a show (shows 20): the unit lists of the type records, the default script a
//! species gets when it is met first (19.2) and the moves that feed them.
//!
//! The world notes each move (an animal on the map or off it, a show-neighbour link made or
//! dropped) and [`settle`] applies them at the start of the next step, where the catalogue is at
//! hand. A first rating of an exhibit adds the animals it counts ([`counted`]). The marine
//! specialist joins with his job (Shows 2b).

use super::trick::{default_items, performs};
use crate::entity::{Catalogue, EntityType};
use crate::timing::file_time;
use crate::world::World;
use crate::zoo::{Item, Script, TypeRecord};

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
            Move::Link { plain, show, on } => {
                let Some(n) = super::info_slot(world, show) else {
                    continue;
                };
                for (id, ty) in performers(world, cat, plain) {
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
/// `x`; an exhibit with none passes it on to its show neighbours. `seen` stops a walk that comes
/// back to an exhibit.
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
    let near = world.exhibits().get(x).map(|e| e.show_near.clone());
    for y in near.unwrap_or_default() {
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
    let at = match info.records.binary_search_by_key(&key, |r| r.key) {
        Ok(at) => at,
        Err(at) => {
            info.records
                .insert(at, TypeRecord::new(key, u16::MAX, date));
            at
        }
    };
    &mut info.records[at]
}

/// `create_default_script` and `add_script` (shows 19.2, 5.6): the items become a script with the
/// next script id, and the type's record (a new one takes the default admission and the date)
/// takes it; while the show runs the record only holds it as pending. A script with items puts
/// the species on the schedule.
fn add_script(world: &mut World, n: usize, key: u32, items: Vec<Item>) {
    let (admission, date) = (world.units.show.admission, file_time(world.units.now));
    let shows = &mut world.zoo.shows;
    let id = shows.next_script;
    shows.next_script = id.wrapping_add(1);
    let listed = !items.is_empty();
    shows.scripts.push(Script {
        id,
        marker: u32::MAX,
        kind: key,
        items,
    });
    let info = shows.infos[n].as_mut().expect("a show info");
    let scheduled = (info.schedule.get(info.index as usize))
        .and_then(|s| info.records.iter().find(|r| r.key == *s))
        .map_or(0, |r| r.script);
    let started = info.show.running && info.show.script == scheduled;
    let at = match info.records.binary_search_by_key(&key, |r| r.key) {
        Ok(at) => at,
        Err(at) => {
            let mut new = TypeRecord::new(key, u16::MAX, date);
            new.admission = admission.to_bits();
            info.records.insert(at, new);
            at
        }
    };
    match started {
        true => info.records[at].pending = id,
        false => {
            info.records[at].script = id;
            if listed && !info.schedule.contains(&key) {
                info.schedule.push(key);
            }
        }
    }
}

#[cfg(test)]
mod tests;
