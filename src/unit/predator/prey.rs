//! The prey search (animals 11.8.1): the kept prey while it stays within `cPreyRadius` tiles on
//! both axes, else the square of that many tiles round the hunter, no circle, in the hunter's own
//! region.

use super::*;

/// `cPreyRadius` of the unit class file, 5 when it has none.
fn prey_radius(cat: &Catalogue) -> i32 {
    cat.prey_radius.unwrap_or(5)
}

/// The prey test of the entity at position `e` for the animal `hunter` standing at
/// `at`. Mode 0 asks for a prey, mode 1 for an animal of the hunter's own type that is not itself.
fn prey(world: &World, cat: &Catalogue, hunter: &Unit, at: usize, (e, mode): (usize, u8)) -> bool {
    let (me, entity) = (&world.zoo.entities[at], &world.zoo.entities[e]);
    let Some(id) = entity.common.as_ref().map(|c| c.id) else {
        return false;
    };
    let Some(other) = world.units.get(id) else {
        return false;
    };
    let (Some(mine), Some(ty)) = (type_of(cat, me), type_of(cat, entity)) else {
        return false;
    };
    if mode == 1 {
        return matches!(other.part, Part::Animal(_))
            && ty.name_id == mine.name_id
            && id != hunter.id;
    }
    let flag = |key| crate::build::number(mine, key).is_some_and(|v| v as i32 == 1);
    let adult = me.subtype.starts_with(['m', 'f']) || flag("cBabiesAttack");
    let listed = preys_on(mine, ty.name_id);
    match &other.part {
        Part::Animal(a) if a.crated => false,
        Part::Animal(a) if a.egg => flag("cEatsEggs") && ty.name_id != mine.name_id,
        Part::Animal(_) => adult && listed,
        Part::Guest(g) => adult && listed && !g.seized,
        Part::Staff(_) if matches!(other.kind, Kind::Keeper | Kind::Worker | Kind::Guide) => {
            let grounds = tile_of(world, e).and_then(|t| world.exhibits().region(t));
            adult && listed && matches!(grounds, Some(GROUNDS | EXTERIOR))
        }
        Part::Staff(_) => false,
    }
}

/// `find_prey(unit, mode)`: the id of the nearest prey of the animal `hunter` standing at position
/// `at`, the first found of equal distances, 0 for none. The kept prey inside the square decides
/// alone. The first prey of a tile counts; a prey in the building chain is skipped unless the
/// hunter rampages.
pub(crate) fn find_prey(world: &World, cat: &Catalogue, hunter: &Unit, at: usize, mode: u8) -> u32 {
    let Some(here) = tile_of(world, at) else {
        return 0;
    };
    let (w, h) = (world.zoo.width as i32, world.zoo.height as i32);
    let (tx, ty) = (here as i32 % w, here as i32 / w);
    let r = prey_radius(cat);
    let kept = match &hunter.part {
        Part::Animal(a) => a.prey,
        _ => 0,
    };
    if let Some(k) = world.find(kept).filter(|_| kept != 0) {
        let near = tile_of(world, k)
            .is_some_and(|t| ((t as i32 % w) - tx).abs() <= r && ((t as i32 / w) - ty).abs() <= r);
        if near {
            return if prey(world, cat, hunter, at, (k, mode)) {
                kept
            } else {
                0
            };
        }
    }
    let Some(region) = world.exhibits().region(here) else {
        return 0;
    };
    let rampaging = matches!(&hunter.part, Part::Animal(a) if a.rampaging);
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
            let id = |e: usize| world.zoo.entities[e].common.as_ref().map_or(0, |c| c.id);
            let chain = |e: usize| {
                let task = world.units.get(id(e)).map_or(0, |u| u.goal.record.task);
                !rampaging && crate::goal::IN_BUILDING.contains(&task)
            };
            let wanted =
                |&e: &usize| prey(world, cat, hunter, at, (e, mode)) && id(e) != kept && !chain(e);
            let d = (cx - tx).pow(2) + (cy - ty).pow(2);
            if let Some(e) = world.units_on(t).find(wanted)
                && (found == 0 || d < best)
            {
                (best, found) = (d, id(e));
            }
        }
    }
    found
}
