//! The dead state of a building (guest buildings 11.1.1). A building is dead when its type's
//! keys say so for the ground it stands on and the water of the tank under it. It is decided
//! when the building is placed, put down or loaded, and again each time the water level or the
//! wall height of its tank moves, a tank is made or goes, the ground of a tile changes and the
//! view zooms or turns. A building found dead outside a load puts all its users out (a forced
//! leave, 11.1) and refuses new ones (6.1). The tank toys and show objects carry the keys too and
//! are decided the same way, and so do the scenery types with a dead key (no users).

use super::{Kind, Live, leave::push_out};
use crate::entity::{Catalogue, EntityType};
use crate::place::bulldozer::type_of;
use crate::world::World;

/// The type keys that decide the dead state: `cDeadOnLand`, `cDeadOnFlatWater`,
/// `cDeadUnderwater`, `cSurface` and `cHeight`.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct Keys {
    pub on_land: bool,
    pub on_flat_water: bool,
    pub underwater: bool,
    pub surface: bool,
    pub height: i32,
}

/// Whether an object with `keys` is dead: `level` is the water level of the tank under its own
/// tile (none outside a tank) and `water` tells for each tile of its footprint whether the
/// ground is water.
pub(crate) fn is_dead(keys: &Keys, level: Option<u32>, water: &[bool]) -> bool {
    let land = water.iter().any(|&w| !w);
    let wet = water.iter().any(|&w| w);
    let level = level.map(i64::from);
    let on_land = keys.on_land
        && match level {
            None => land,
            Some(l) if keys.surface => l < 1,
            Some(l) => l <= i64::from(keys.height),
        };
    on_land || (keys.on_flat_water && wet) || (keys.underwater && level.is_some_and(|l| l >= 1))
}

/// Decides the dead state of the building at position `at` of the entity list and stores it;
/// returns it. A building that is not a guest building has no live state and is never dead.
fn decide_at(world: &mut World, kind: &Kind, at: usize) -> bool {
    let Some((own, tiles)) = world.footprint(at) else {
        return false;
    };
    let level = (own.and_then(|t| world.tank_at(t))).and_then(|id| world.tank(id).map(|t| t.level));
    let water: Vec<bool> = tiles.iter().map(|&t| world.is_water(t)).collect();
    let dead = is_dead(&kind.dead, level, &water);
    let id = world.zoo.entities[at].common.as_ref().map_or(0, |c| c.id);
    // An object that is alive and has no figures keeps none (a toy).
    if dead || world.units.buildings.contains_key(&id) {
        (world.units.buildings.entry(id))
            .or_insert_with(|| Live::new(kind))
            .dead = dead;
    }
    dead
}

/// Whether objects of type `ty` can be dead: a guest building, or a type with a dead key (the
/// tank toys and show objects).
fn decides(ty: &EntityType) -> bool {
    crate::month::is_guest_building(ty)
        || ["cDeadOnLand", "cDeadOnFlatWater", "cDeadUnderwater"]
            .iter()
            .any(|key| crate::build::number(ty, key) == Some(1.0))
}

/// Whether `ty` is a toy or show object: not a guest building, with a dead key. Its live state
/// is only the dead state, never saved (11.1.1), so the save keeps the record it was read with.
pub(crate) fn is_toy(ty: &EntityType) -> bool {
    !crate::month::is_guest_building(ty) && decides(ty)
}

/// Decides the dead state of the new or set down building `id` of type `ty`: no user is
/// there to put out.
pub(crate) fn decide(world: &mut World, ty: &EntityType, id: u32) {
    if !decides(ty) {
        return;
    }
    let kind = Kind::of(ty, world.units.prices.default);
    if let Some(at) = world.find(id) {
        decide_at(world, &kind, at);
    }
}

/// A zoo has just been loaded: the dead state of every guest building is decided, and no user is
/// put out (the loader's flag, 11.1.1).
pub(crate) fn decide_all(world: &mut World, cat: &Catalogue) {
    for at in 0..world.zoo.entities.len() {
        if let Some(kind) = decider(world, cat, at) {
            decide_at(world, &kind, at);
        }
    }
}

/// Validates again the buildings on every tank whose water level or wall height moved since the
/// last call: the simulation's water step and the app's edits both call it, each tank once. The
/// units of a tank whose water, base level, walls or ground changed get the AI reset of their
/// kind (guest buildings 12.2 point 22).
pub(crate) fn wet_changed(world: &mut World, cat: &Catalogue, rng: &mut crate::sim::Rng) {
    wet_changed_then(world, cat, rng, |_, _, _| {});
}

/// [`wet_changed`], calling `then` with each stirred tank right after its units were reset (the
/// ripples of a tank that got water are set up there, map drawing 9.7.8 point 4).
pub(crate) fn wet_changed_then(
    world: &mut World,
    cat: &Catalogue,
    rng: &mut crate::sim::Rng,
    mut then: impl FnMut(&mut World, &mut crate::sim::Rng, u32),
) {
    for tank in world.take_wet() {
        tank_changed(world, cat, tank);
    }
    if world.take_reshaped() {
        revalidate(world, cat);
    }
    for tank in world.take_stirred() {
        crate::goal::stir_units(world, cat, rng, tank);
        then(world, rng, tank);
    }
}

/// A tank was made or went, or the ground of a tile or the view changed (11.1.1): every object
/// is validated again, and each one found dead puts all its users out. The spec validates the
/// objects of the tiles concerned; the state depends on nothing else, so the pass over all of
/// them differs only for a building that was loaded dead.
pub(crate) fn revalidate(world: &mut World, cat: &Catalogue) {
    validate(world, cat, None);
}

/// The water level or the wall height of tank `tank` moved: the buildings standing on its tiles
/// are validated again, and each one found dead puts all its users out.
pub(crate) fn tank_changed(world: &mut World, cat: &Catalogue, tank: u32) {
    validate(world, cat, Some(tank));
}

/// Validates the objects of tank `only`, or all of them, again.
fn validate(world: &mut World, cat: &Catalogue, only: Option<u32>) {
    for at in 0..world.zoo.entities.len() {
        let Some(kind) = decider(world, cat, at) else {
            continue;
        };
        let on = only.is_none_or(|tank| {
            (world.footprint(at))
                .and_then(|(own, _)| own)
                .is_some_and(|t| world.tank_at(t) == Some(tank))
        });
        if on
            && decide_at(world, &kind, at)
            && let Some(id) = world.zoo.entities[at].common.as_ref().map(|c| c.id)
        {
            push_out(world, cat, id);
        }
    }
}

/// The kind of the entity at `at` when its dead state is decided (see [`decides`]).
fn decider(world: &World, cat: &Catalogue, at: usize) -> Option<Kind> {
    let ty = type_of(cat, &world.zoo.entities[at]).filter(|ty| decides(ty))?;
    Some(Kind::of(ty, world.units.prices.default))
}

#[cfg(test)]
mod tests;
