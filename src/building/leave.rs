//! A user leaving a building (guest buildings 6.6, 6.7 and 11.1): what a guest pays and is given,
//! and the forced leave of every user when the building is taken off the map. The guest's own
//! leave (task 0x69) is [`crate::goal`]'s and uses [`pay`] too.

use super::{Kind, Mood, exit_of, kind_of};
use crate::entity::Catalogue;
use crate::goal::{ENTER_BUILDING, EXIT_BUILDING, USE};
use crate::guest::{BATHROOM, ENERGY, HAPPINESS, HUNGER, THIRST};
use crate::render::order::ground_height;
use crate::unit::{Part, Unit};
use crate::world::World;
use crate::zoo::Entity;

/// The thoughts of a price that is really low and too high (guests 9.4).
const CHEAP: i32 = 10072;
const EXPENSIVE: i32 = 10073;

/// Guests 5.4: the change record is added to the guest's pending values.
pub(crate) fn apply(unit: &mut Unit, record: [i32; 6]) {
    if let Part::Guest(g) = &mut unit.part {
        let happy = if g.child { record[1] } else { record[0] };
        for (v, change) in [
            (HAPPINESS, happy),
            (HUNGER, record[2]),
            (THIRST, record[3]),
            (BATHROOM, record[4]),
            (ENERGY, record[5]),
        ] {
            g.pending[v] = g.pending[v].saturating_add(change);
        }
    }
}

/// The price reaction of the guest `unit`, whose position in the entity list is `at` (guests
/// 9.4): the happiness changes by the class key, and the thought is about the building.
pub(crate) fn react(
    world: &mut World,
    cat: &Catalogue,
    at: usize,
    unit: &mut Unit,
    mood: Mood,
    building: u32,
) {
    let Some(k) = crate::guest::keys(world, cat, at) else {
        return;
    };
    let change = match mood {
        Mood::Happy => k.price[0],
        Mood::Angry => k.price[1],
        Mood::Fair => 0,
    };
    let text = match mood {
        Mood::Happy => Some(CHEAP),
        Mood::Angry => Some(EXPENSIVE),
        Mood::Fair => None,
    };
    if let Some(text) = text {
        world.units.think(unit.id, text, Some(building), None);
    }
    if let Part::Guest(g) = &mut unit.part {
        g.pending[HAPPINESS] = g.pending[HAPPINESS].saturating_add(change);
    }
}

/// Guest buildings 6.6 points 2 and 3: the guest `unit` has left building `id` of kind `kind`. A
/// charging building books its price times its factor as concessions (a building that sells
/// nothing first makes the price reaction), then the type's change record is given.
pub(crate) fn pay(
    world: &mut World,
    cat: &Catalogue,
    at: usize,
    unit: &mut Unit,
    id: u32,
    kind: &Kind,
) {
    if !matches!(unit.part, Part::Guest(_)) {
        return;
    }
    if kind.commerce {
        if kind.sells.is_empty() {
            let mood = super::mood(world, id, kind);
            react(world, cat, at, unit, mood, id);
        }
        if let Some(live) = world.units.buildings.get_mut(&id) {
            // A hostile factor or price books nothing.
            let income = (kind.factor * live.price).max(0.0);
            live.income[0] += income;
            live.income[2] += income;
            world.units.concessions.push(income);
        }
    }
    apply(unit, kind.change);
}

/// Guest buildings 11.1: building `id` is being taken off the map (deleted, or picked up), so
/// every user is removed as a forced leave, in entering order: it pays, gets the change record
/// and is put at an exit (6.7), shown and with its goal stopped. The building keeps its figures.
/// A building that is no guest building has no users, and nothing happens.
pub fn push_out(world: &mut World, cat: &Catalogue, id: u32) {
    let Some(e) = world.find(id).map(|at| world.zoo.entities[at].clone()) else {
        return;
    };
    let Some(kind) = kind_of(cat, &e, world.units.prices.default) else {
        return;
    };
    let users = (world.units.buildings.get_mut(&id)).map(|l| std::mem::take(&mut l.users));
    for (unit, slot, _) in users.unwrap_or_default() {
        let (Some(at), Some(entry)) = (world.find(unit), world.units.get_mut(unit)) else {
            continue;
        };
        // The unit leaves the table for its leave, so that the world can be used freely.
        let mut u = std::mem::replace(entry, Unit::new(unit, entry.kind));
        pay(world, cat, at, &mut u, id, &kind);
        world.units.building_note(format!("push {id} {unit}"));
        crate::goal::stop_use(&mut u);
        put_out(world, at, &mut u, &e, &kind, slot);
        if let Some(entry) = world.units.get_mut(unit) {
            *entry = u;
        }
    }
}

/// Guest buildings 11.2: the unit `id`, about to be picked up by hand, leaves its building first.
/// A guest whose task is entering, using or leaving a building that exists is removed as a normal
/// leave when it is listed (it pays, gets the change record, and is put at the exit or shown); one
/// that is not listed (still walking in) is taken out unserved. Its goal then stops. Any other
/// unit is untouched.
pub fn pick_up(world: &mut World, cat: &Catalogue, id: u32) {
    let (Some(at), Some(u)) = (world.find(id), world.units.get(id)) else {
        return;
    };
    let task = u.goal.record.task;
    let building = u.goal.record.target.id;
    if !matches!(u.part, Part::Guest(_)) || ![ENTER_BUILDING, EXIT_BUILDING, USE].contains(&task) {
        return;
    }
    let Some(e) = world.find(building).map(|b| world.zoo.entities[b].clone()) else {
        return;
    };
    let Some(kind) = kind_of(cat, &e, world.units.prices.default) else {
        return;
    };
    let listed = (world.units.buildings.get_mut(&building)).and_then(|l| {
        let i = l.users.iter().position(|u| u.0 == id)?;
        Some(l.users.remove(i).1)
    });
    let Some(entry) = world.units.get_mut(id) else {
        return;
    };
    let mut u = std::mem::replace(entry, Unit::new(id, entry.kind));
    if let Some(slot) = listed {
        pay(world, cat, at, &mut u, building, &kind);
        match kind.uses_exit {
            true => put_out(world, at, &mut u, &e, &kind, slot),
            false => show(world, at, &mut u),
        }
    }
    crate::goal::stop_use(&mut u);
    if let Some(entry) = world.units.get_mut(id) {
        *entry = u;
    }
}

/// A hidden user is shown again where it stands.
pub(crate) fn show(world: &mut World, at: usize, unit: &mut Unit) {
    if std::mem::take(&mut unit.hidden) {
        let c = world.zoo.entities[at].common.as_ref();
        if let Some((p, r)) = c.map(|c| ((c.x, c.y, c.z), c.rotation)) {
            world.place_unit(at, p, r);
        }
    }
}

/// Guest buildings 6.7: the user at position `at` of the entity list is put at the exit of its
/// slot ([`exit_of`]), on the ground there with its own facing, and shown. With no exit it is
/// taken off the map and stays as it was, hidden or not.
pub(crate) fn put_out(
    world: &mut World,
    at: usize,
    unit: &mut Unit,
    e: &Entity,
    kind: &Kind,
    slot: usize,
) {
    let Some(c) = world.zoo.entities[at].common.as_ref() else {
        return;
    };
    let (z, facing) = (c.z, c.rotation);
    let Some((x, y)) = exit_of(world, e, kind, slot) else {
        return world.lift_unit(at);
    };
    let z = (world.tile_index(x.div_euclid(64), y.div_euclid(64))).map_or(z, |t| {
        ground_height(&world.zoo.tiles[t], x.rem_euclid(64), y.rem_euclid(64))
    });
    unit.hidden = false;
    world.place_unit(at, (x, y, z), facing);
}
