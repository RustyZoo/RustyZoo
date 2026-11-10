//! The live state of a building at its birth and at a load: built, loaded with a zoo, reset.

use super::*;
use crate::guest::litter::CAN_FILLING;
use crate::timing::Date;

/// Building `id` of type `ty` was built at calendar time `now` (ms of game time): a guest
/// building or a compost starts its live state, with the date its months open are counted from (guest
/// buildings 4).
pub(crate) fn built(world: &mut World, ty: &EntityType, id: u32, now: i64) {
    let kind = Kind::of(ty, world.units.prices.default);
    if keeps_figures(ty, &kind) {
        let live = Live::new(&kind);
        world
            .units
            .buildings
            .insert(id, Live { built: now, ..live });
    }
    dead::decide(world, ty, id);
}

/// Whether a building of this type has figures of its own: a guest building, and the compost,
/// which earns from the dung it recycles (zoo format 6.9).
fn keeps_figures(ty: &EntityType, kind: &Kind) -> bool {
    crate::month::is_guest_building(ty) || kind.satisfies("compost")
}

/// The building at position `at` of the entity list of a zoo just loaded takes the figures its
/// record stores (zoo format 6.9, guest buildings 4): the guests and the income of this month,
/// last month and since construction, the upkeep paid since construction (from version 68), the
/// trash can's fill, the construction date and the price. The users come after the guests are
/// made ([`users::restore`]).
pub(crate) fn load(world: &mut World, cat: &Catalogue, at: usize) {
    let e = &world.zoo.entities[at];
    let Some(record) = e.common.as_ref().and_then(|c| c.building.clone()) else {
        return;
    };
    let Some(ty) = crate::entity::type_of(cat, e) else {
        return;
    };
    let kind = Kind::of(ty, world.units.prices.default);
    if !keeps_figures(ty, &kind) {
        return;
    }
    let id = e.common.as_ref().map_or(0, |c| c.id);
    let built = Date::from_file_time(record.built).game_ms();
    let live = Live {
        guests: record.guests,
        income: record.income.map(f32::from_bits),
        upkeep: record.upkeep.map_or(0.0, f32::from_bits),
        price: f32::from_bits(record.price),
        built,
        ..Live::new(&kind)
    };
    world.units.buildings.insert(id, live);
    match record.upgrade {
        0 => world.units.upgrade_index.remove(&id),
        index => world.units.upgrade_index.insert(id, index),
    };
    if kind.satisfies("trash") {
        set_fill(world, id, record.fill);
    }
    // The "show soon" flag of a grandstand is stored as the fill (shows 3.5).
    if let (true, Some(live)) = (
        kind.satisfies("grandstand"),
        world.units.buildings.get_mut(&id),
    ) {
        live.fill = record.fill;
    }
}

/// A start with `reset` or `resetdate` resets every building (guest buildings 4.1): the fill, the
/// users and the figures go to 0 and the construction date becomes the game time `now` (ms since
/// the calendar's start); the price is kept. It runs after the load events, so a can that called a
/// worker is empty with its task still in the pool. Every unit a building listed as a user is
/// removed from the world, not released: the ids are returned for the caller to ask the removal.
pub(crate) fn reset_all(world: &mut World, now: i64) -> Vec<u32> {
    let ids: Vec<u32> = world.units.buildings.keys().copied().collect();
    let mut removed = Vec::new();
    for id in ids {
        set_fill(world, id, 0);
        if let Some(live) = world.units.buildings.get_mut(&id) {
            removed.extend(live.users.iter().map(|u| u.0));
            *live = Live {
                price: live.price,
                built: now,
                // A start with reset leaves the show id of a grandstand alone (shows 3.5).
                show_id: live.show_id,
                dead: live.dead,
                ..Live::default()
            };
        }
    }
    removed
}

/// The events a zoo just loaded posts for its trash cans (staff 8.2): a can above 12 pieces calls
/// a worker as it does when it reaches 13, with its tile, in entity list order.
pub(crate) fn loaded_events(world: &World) -> Vec<crate::sim::Event> {
    let mut events = Vec::new();
    for e in &world.zoo.entities {
        let Some(c) = e.common.as_ref() else {
            continue;
        };
        let full = world
            .units
            .buildings
            .get(&c.id)
            .is_some_and(|l| l.fill > 12);
        let tile = world.tile_index(c.x.div_euclid(64), c.y.div_euclid(64));
        if e.class == "building" && full {
            let tile = tile.unwrap_or(0) as u32;
            events.push(crate::world::piece_event(c.id, CAN_FILLING, tile));
        }
    }
    events
}
