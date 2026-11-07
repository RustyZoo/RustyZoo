//! What the show panel's trick editor does to the world (information panels 19.9, shows 19, 5.6):
//! the trick lists of a species, the script a list is written to, the benefit of a list, and the
//! satisfactions the editor leaves in the type's tricks. The window itself is
//! [`crate::panel::show`].

use std::collections::BTreeMap;

use super::info_slot;
use super::trick::{FOOD, available, default_items, tricks};
use crate::entity::{Catalogue, EntityType, default_subtype};
use crate::timing::file_time;
use crate::world::World;
use crate::zoo::{Item, Script, TypeRecord};

/// The current satisfaction of a trick of a type: the type's name, subtype and the trick id. The
/// items of a type are shared by every show tank and not saved.
pub(crate) type Current = (Vec<u8>, Vec<u8>, u16);

/// No script, as a record stores it.
const NONE: u16 = u16::MAX;

/// The animal type of species `id`: the first one of the catalogue with this name id.
pub(crate) fn species_type(cat: &Catalogue, id: u32) -> Option<&EntityType> {
    let named = |t: &&EntityType| t.class == "animals" && u32::try_from(t.name_id) == Ok(id);
    cat.types.iter().find(named)
}

/// The trick list of `ty` with the current satisfactions the editor left in it (19.9.8 point 2).
pub(crate) fn own_items(world: &World, ty: &EntityType) -> Vec<Item> {
    let mut list = tricks(ty);
    for t in &mut list {
        let key = (ty.type_name.clone(), ty.subtype.clone(), t.id);
        if let Some(&current) = world.units.trick_current.get(&key) {
            t.current = current;
        }
    }
    list
}

/// Keeps the current satisfactions of `list`, the tricks of `ty`.
fn keep_current(world: &mut World, ty: &EntityType, list: &[Item]) {
    for t in list {
        let key = (ty.type_name.clone(), ty.subtype.clone(), t.id);
        match t.current == t.satisfaction {
            true => world.units.trick_current.remove(&key),
            false => world.units.trick_current.insert(key, t.current),
        };
    }
}

/// The default subtype's type of the species of `ty`, whose list the available tricks come from
/// (19.9.4).
fn default_type<'a>(cat: &'a Catalogue, ty: &EntityType) -> Option<&'a EntityType> {
    let default = ty.config.as_deref().map_or(&[][..], default_subtype);
    let same = |t: &&EntityType| {
        t.class == ty.class && t.type_name == ty.type_name && t.subtype[..] == *default
    };
    cat.types.iter().find(same)
}

/// A trick of the available list and whether the editor offers it now.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Avail {
    pub item: Item,
    pub ok: bool,
}

/// The available tricks of species type `s` in show tank `x` (19.9.4 point 1, 19.9.9 point 1): the
/// default subtype's list but the food trick, each with whether it is valid now.
pub(crate) fn available_list(world: &World, cat: &Catalogue, x: u32, s: &EntityType) -> Vec<Avail> {
    let list = default_type(cat, s).map(tricks).unwrap_or_default();
    (list.into_iter().filter(|t| t.id != FOOD))
        .map(|item| Avail {
            ok: available(world, cat, x, s, &item),
            item,
        })
        .collect()
}

/// The lists after a fill (19.9.4).
#[derive(Debug, Default)]
pub(crate) struct Filled {
    pub avail: Vec<Avail>,
    /// The tricks of the script that are in the show list.
    pub shown: Vec<Item>,
    /// Items were dropped, and the shorter list is written (the script-changed flag stays set).
    pub changed: bool,
}

/// `fill_trick_lists` (19.9.4) for species type `s` in show tank `x`: the available list, and the
/// pending script or the species' script as the list in the show, the default script made once
/// for a record whose script has no item. A list that lost items is written back.
pub(crate) fn fill(world: &mut World, cat: &Catalogue, x: u32, s: &EntityType) -> Filled {
    let avail = available_list(world, cat, x, s);
    let mut out = Filled {
        avail,
        ..Filled::default()
    };
    let (Some(slot), false) = (info_slot(world, x), out.avail.is_empty()) else {
        return out;
    };
    let key = s.name_id as u32;
    let date = file_time(world.units.now);
    let info = world.zoo.shows.infos[slot].as_mut().expect("a show info");
    let record = info.lookup(key, date);
    let id = match record.pending {
        0 | NONE => record.script,
        pending => pending,
    };
    let default = record.default_flag;
    let script = world.zoo.shows.scripts.iter().find(|sc| sc.id == id);
    let mut items = script.map(|sc| sc.items.clone()).unwrap_or_default();
    if items.is_empty() && default {
        let params = world.units.show;
        let Some(made) = default_items(world, cat, &params, x, s) else {
            return out;
        };
        if !made.is_empty()
            && let Some(info) = world.zoo.shows.infos[slot].as_mut()
        {
            info.lookup(key, date).default_flag = false;
        }
        let id = new_script(world, key, made.clone());
        add_script(world, slot, key, id);
        items = made;
    }
    for it in items.into_iter().filter(|it| it.visible != 0) {
        match out.avail.iter().any(|a| a.item.id == it.id) && available(world, cat, x, s, &it) {
            true => out.shown.push(it),
            false => out.changed = true,
        }
    }
    if out.changed {
        let ids: Vec<u16> = out.shown.iter().map(|i| i.id).collect();
        write_script(world, x, s, &ids);
    }
    out
}

/// A new script of species `key` with the next script id (zoo format 4.2).
pub(super) fn new_script(world: &mut World, key: u32, items: Vec<Item>) -> u16 {
    let shows = &mut world.zoo.shows;
    let id = shows.next_script;
    shows.next_script = id.wrapping_add(1);
    shows.scripts.push(Script {
        id,
        marker: u32::MAX,
        kind: key,
        items,
    });
    id
}

/// `copy_list_to_script` (19.9.7): the tricks `ids` of species type `s`, in order, become the
/// items of the pending script (or of a new one) with the food trick after each run that adds up
/// to the threshold, their satisfactions are calculated, and the script is given to the species.
/// Returns whether the script has items, and the pass over them.
pub(crate) fn write_script(
    world: &mut World,
    x: u32,
    s: &EntityType,
    ids: &[u16],
) -> (bool, Recalc) {
    let none = Recalc {
        count: 0,
        benefit: 0,
    };
    let Some(slot) = info_slot(world, x) else {
        return (false, none);
    };
    let key = s.name_id as u32;
    let date = file_time(world.units.now);
    let info = world.zoo.shows.infos[slot].as_mut().expect("a show info");
    let pending = info.lookup(key, date).pending;
    let scripts = &world.zoo.shows.scripts;
    let waiting = scripts.iter().any(|sc| sc.id == pending) && !matches!(pending, 0 | NONE);
    let id = match waiting {
        true => pending,
        false => new_script(world, key, Vec::new()),
    };
    let mut own = own_items(world, s);
    let k = world.units.show.threshold;
    let (mut items, mut sum) = (Vec::new(), 0i32);
    for it in ids.iter().filter_map(|t| own.iter().find(|i| i.id == *t)) {
        items.push(it.clone());
        sum = sum.wrapping_add(it.complexity as i32);
        if sum >= k {
            items.extend(own.iter().find(|i| i.id == FOOD).cloned());
            sum = 0;
        }
    }
    let found = pass(k, &mut own, &[], Some(&mut items));
    keep_current(world, s, &own);
    let filled = !items.is_empty();
    if let Some(sc) = world.zoo.shows.scripts.iter_mut().find(|sc| sc.id == id) {
        sc.items = items;
    }
    add_script(world, slot, key, id);
    (filled, found)
}

/// `add_script(t, n)` (shows 3.2, 5.6, information panels 19.9.7): script `id` is the pending one
/// of species `key` in show info `slot`; while the show is not started it takes the place of the
/// species' script at once, and the species joins the schedule when that script has items, else
/// leaves it. A new record takes the default admission and the date.
pub(super) fn add_script(world: &mut World, slot: usize, key: u32, id: u16) {
    if matches!(id, 0 | NONE) || key == 0 {
        return;
    }
    let (admission, date) = (world.units.show.admission, file_time(world.units.now));
    let shows = &mut world.zoo.shows;
    let info = shows.infos[slot].as_mut().expect("a show info");
    let scheduled = (info.schedule.get(info.index as usize))
        .and_then(|s| info.records.iter().find(|r| r.key == *s))
        .map_or(0, |r| r.script);
    let started = info.show.running && info.show.script == scheduled;
    let at = match info.records.binary_search_by_key(&key, |r| r.key) {
        Ok(at) => at,
        Err(at) => {
            let mut new = TypeRecord::new(key, NONE, date);
            new.admission = admission.to_bits();
            info.records.insert(at, new);
            at
        }
    };
    let record = &mut info.records[at];
    let mut gone = Vec::new();
    if !matches!(record.pending, 0 | NONE) && record.pending != id {
        gone.push(record.pending);
    }
    record.pending = id;
    if !started {
        gone.push(std::mem::replace(&mut record.script, id));
        record.pending = NONE;
    }
    let script = record.script;
    shows
        .scripts
        .retain(|sc| sc.id == id || !gone.contains(&sc.id));
    let filled = shows
        .scripts
        .iter()
        .any(|sc| sc.id == script && !sc.items.is_empty());
    match (filled, info.schedule.contains(&key)) {
        (true, false) => info.schedule.push(key),
        (false, true) => {
            info.schedule.retain(|&t| t != key);
            if info.index as usize >= info.schedule.len() {
                info.index = 0;
            }
        }
        _ => {}
    }
}

/// What a pass of [`recalc`] found: how many tricks it went over and the benefit.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Recalc {
    pub count: usize,
    pub benefit: i32,
}

/// The count step of a trick (19.9.8): its first time it takes its satisfaction; a repeat right
/// after itself (`prev`), and every third time, takes its delta off.
fn tally(count: &mut BTreeMap<u16, u32>, it: &mut Item, prev: Option<u16>) {
    let n = count.entry(it.id).or_insert(0);
    *n = n.wrapping_add(1);
    if *n == 1 {
        it.current = it.satisfaction;
    } else if n.is_multiple_of(3) || prev == Some(it.id) {
        it.current = it.current.wrapping_sub(it.delta);
    }
}

/// One pass of `recalc` (19.9.8) over the tricks `rows` of `own`, or over the items of `script`;
/// `k` is the return-to-keeper threshold. The food trick of `own` is added at the threshold.
fn pass(k: i32, own: &mut [Item], rows: &[u16], mut script: Option<&mut [Item]>) -> Recalc {
    let count_all = script.as_ref().map_or(rows.len(), |sc| sc.len());
    let mut count = BTreeMap::new();
    let (mut prev, mut sum, mut benefit) = (0u16, 0i32, 0i32);
    for i in 0..count_all {
        let it = match (script.as_deref_mut(), rows.get(i)) {
            (Some(sc), _) => sc.get_mut(i),
            (None, Some(id)) => own.iter_mut().find(|t| t.id == *id),
            _ => None,
        };
        let Some(it) = it else { continue };
        tally(&mut count, it, Some(prev));
        prev = it.id;
        sum = sum.wrapping_add(it.complexity as i32);
        benefit = benefit.wrapping_add(it.current);
        if sum >= k {
            if let Some(f) = own.iter_mut().find(|t| t.id == FOOD) {
                tally(&mut count, f, None);
                benefit = benefit.wrapping_add(f.current);
            }
            sum = 0;
        }
    }
    Recalc {
        count: count_all,
        benefit,
    }
}

/// `recalc(none)` (19.9.8) for the tricks `ids` of species type `s`: the benefit shown, with the
/// satisfactions left in the type's tricks.
pub(crate) fn recalc(world: &mut World, s: &EntityType, ids: &[u16]) -> Recalc {
    let mut own = own_items(world, s);
    let out = pass(world.units.show.threshold, &mut own, ids, None);
    keep_current(world, s, &own);
    out
}
