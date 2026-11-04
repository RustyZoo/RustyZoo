//! The prey search (animals 11.8.1): the square of `cPreyRadius` tiles round the hunter, no circle,
//! in the hunter's own region. The hunter's kept prey is always none: nothing chases yet (#1127).

use super::*;

/// `cPreyRadius` of the unit class file, 5 when it has none.
fn prey_radius(cat: &Catalogue) -> i32 {
    cat.prey_radius.unwrap_or(5)
}

/// The prey test (`checkPrey`, mode 0) of the entity at position `e` for the animal `hunter`
/// standing at `at`. Staff and guests are never prey here: nothing hunts them yet.
fn prey(world: &World, cat: &Catalogue, hunter: &Unit, at: usize, e: usize) -> bool {
    let (me, entity) = (&world.zoo.entities[at], &world.zoo.entities[e]);
    let Some(id) = entity.common.as_ref().map(|c| c.id) else {
        return false;
    };
    let Some(Part::Animal(a)) = world.units.get(id).map(|u| &u.part) else {
        return false;
    };
    let (Some(mine), Some(ty)) = (type_of(cat, me), type_of(cat, entity)) else {
        return false;
    };
    let flag = |key| crate::build::number(mine, key).is_some_and(|v| v as i32 == 1);
    match (a.crated, a.egg) {
        (true, _) => false,
        (_, true) => flag("cEatsEggs") && ty.name_id != mine.name_id,
        _ => {
            let adult = me.subtype.starts_with(['m', 'f']);
            let rampaging = matches!(&hunter.part, Part::Animal(h) if h.rampaging);
            let chain = world
                .units
                .get(id)
                .is_some_and(|u| crate::goal::IN_BUILDING.contains(&u.goal.record.task));
            (adult || flag("cBabiesAttack")) && preys_on(mine, ty.name_id) && (rampaging || !chain)
        }
    }
}

/// `find_prey(unit, 0)`: the id of the nearest prey of the animal `hunter` standing at position
/// `at`, the first found of equal distances, 0 for none. The first prey of a tile counts; a prey
/// in the building chain is skipped unless the hunter rampages.
pub(crate) fn find_prey(world: &World, cat: &Catalogue, hunter: &Unit, at: usize) -> u32 {
    let Some(here) = tile_of(world, at) else {
        return 0;
    };
    let (w, h) = (world.zoo.width as i32, world.zoo.height as i32);
    let (tx, ty) = (here as i32 % w, here as i32 / w);
    let Some(region) = world.exhibits().region(here) else {
        return 0;
    };
    let r = prey_radius(cat);
    // A bound that overflows leaves the scan empty.
    let (x0, x1) = (tx.wrapping_sub(r).max(0), tx.wrapping_add(r).min(w - 1));
    let (y0, y1) = (ty.wrapping_sub(r).max(0), ty.wrapping_add(r).min(h - 1));
    let (mut best, mut found) = (0, 0);
    for cx in x0..=x1 {
        for cy in y0..=y1 {
            let Some(t) = world.tile_index(cx, cy) else {
                continue;
            };
            if world.exhibits().region(t) != Some(region) {
                continue;
            }
            let first = world.units_on(t).find(|&e| prey(world, cat, hunter, at, e));
            let d = (cx - tx).pow(2) + (cy - ty).pow(2);
            if let Some(e) = first
                && (found == 0 || d < best)
            {
                let id = world.zoo.entities[e].common.as_ref().map_or(0, |c| c.id);
                (best, found) = (d, id);
            }
        }
    }
    found
}
