//! The predator search (animals 11.8), one routine for guests and staff: the kept predator while it
//! stays within `cPredatorRadius` tiles on both axes, else the nearest threat strictly inside the
//! circle of that radius; on the zoo grounds an escaped animal wherever it stands, inside an
//! exhibit a unit on a tile of the same exhibit. "In sight" is a distance, and nothing is drawn. A
//! unit with no region under it only keeps its kept predator (O12).

use crate::config::to_int;
use crate::entity::{Catalogue, EntityType};
use crate::exhibit::{EXTERIOR, GROUNDS};
use crate::guest::view::tile_of;
use crate::place::bulldozer::type_of;
use crate::unit::{Kind, Part, Unit};
use crate::world::World;

/// The type name id of the man: a species whose prey list holds it frightens every guest.
pub(crate) const MAN: i32 = 9503;

/// `cPredatorRadius` of the unit class file, 5 when it has none. Everything computed from it is
/// 32-bit wrapping arithmetic, as in the original (animals 11.8).
pub(crate) fn radius(cat: &Catalogue) -> i32 {
    cat.predator_radius.unwrap_or(5)
}

/// The escaped list (animals 11.1), as entity positions: the animals still loose and on the map,
/// those loose at the load first in file order, then the others in the order they escaped.
pub(crate) fn escaped(world: &World) -> Vec<usize> {
    let loose = |&e: &usize| {
        let id = world.zoo.entities[e].common.as_ref().map_or(0, |c| c.id);
        matches!(world.units.get(id).map(|u| &u.part), Some(Part::Animal(a)) if a.escaped)
    };
    let list = world.units.escapes.iter();
    list.filter_map(|&id| world.find(id))
        .filter(loose)
        .collect()
}

/// Whether the `cPrey` lines of an animal type hold `id`: the lines of its subtype, else of its
/// default subtype, else the plain ones.
fn preys_on(ty: &EntityType, id: i32) -> bool {
    let Some(config) = ty.config.as_deref() else {
        return false;
    };
    let default = (config.section("Global"))
        .and_then(|g| g.get("DefaultSubtype"))
        .unwrap_or_default();
    let list = [&ty.subtype[..], default, b""]
        .into_iter()
        .find_map(|prefix| {
            let name = match prefix.is_empty() {
                true => b"Characteristics/Integers".to_vec(),
                false => [prefix, b"/Characteristics/Integers"].concat(),
            };
            let all = config.section(name)?.get_all("cPrey");
            (!all.is_empty()).then_some(all)
        });
    list.is_some_and(|all| all.iter().any(|v| to_int(v) == Some(id)))
}

/// The unit that looks: its kind, the type name id a predator lists (the man for a guest) and
/// whether it stands on the zoo grounds.
struct Looker<'a> {
    kind: Kind,
    prey: i32,
    /// The type of the looker, for the prey list of an animal.
    ty: Option<&'a EntityType>,
    grounds: bool,
}

/// The mode of `find` that asks an animal for a prey of its own (shows 30.2 point 4.6): a unit it
/// hunts, as a threat is one that hunts it.
pub(crate) const PREY: u8 = 2;

/// The threat test (`checkPredator`) of the entity at position `e` in `mode` (0 a threat, 1 a
/// playmate that chases the unit, [`PREY`] a prey of an animal: an animal whose type its prey list
/// holds, inferred as the mirror of the threat). An animal that is crated or an egg is none. For a guest the mode is ignored and a species whose prey list holds the man is one,
/// whatever the animal does. For staff and animals in mode 0 it must be an adult (or of a species
/// with `cBabiesAttack` 1) whose prey list holds the looker's type; a staff member must also stand
/// on the grounds and the animal must not be darted. Mode 1 asks for an animal chasing the unit,
/// which no staff member ever is.
fn threat(world: &World, cat: &Catalogue, looker: &Looker, e: usize, mode: u8) -> bool {
    let entity = &world.zoo.entities[e];
    let Some(id) = entity.common.as_ref().map(|c| c.id) else {
        return false;
    };
    let Some(Part::Animal(a)) = world.units.get(id).map(|u| &u.part) else {
        return false;
    };
    let Some(ty) = type_of(cat, entity).filter(|_| !a.crated && !a.egg) else {
        return false;
    };
    match looker.kind {
        Kind::Guest => preys_on(ty, MAN),
        _ if mode == 1 => false,
        Kind::Animal if mode == PREY => looker.ty.is_some_and(|me| preys_on(me, ty.name_id)),
        kind => {
            let adult = entity.subtype.starts_with(['m', 'f']);
            let babies = crate::build::number(ty, "cBabiesAttack").is_some_and(|v| v as i32 == 1);
            // Only a staff member must stand on the grounds, and the animal must not be darted.
            let kept_off = kind != Kind::Animal && !(looker.grounds && !a.tranq);
            (adult || babies) && preys_on(ty, looker.prey) && !kept_off
        }
    }
}

/// `find_predator(unit, mode)`: the id of the threat the unit standing at position `at` sees, 0 for
/// none. `removed` holds the units asked to go, which no longer count as the kept predator.
pub(crate) fn find(
    world: &World,
    cat: &Catalogue,
    removed: &[u32],
    unit: &Unit,
    at: usize,
    mode: u8,
) -> u32 {
    let Some(here) = tile_of(world, at) else {
        return 0;
    };
    let (w, h) = (world.zoo.width as i32, world.zoo.height as i32);
    let xy = |t: usize| (t as i32 % w, t as i32 / w);
    let (tx, ty) = xy(here);
    let region = world.exhibits().region(here);
    let me = type_of(cat, &world.zoo.entities[at]);
    let looker = Looker {
        kind: unit.kind,
        prey: match unit.kind {
            Kind::Guest => MAN,
            _ => me.map_or(0, |t| t.name_id),
        },
        ty: me,
        grounds: matches!(region, Some(GROUNDS | EXTERIOR)),
    };
    let r = radius(cat);
    let kept = Some(unit.predator)
        .filter(|&p| p != 0 && !removed.contains(&p))
        .and_then(|p| world.find(p));
    if let Some(p) = kept
        && let Some((px, py)) = tile_of(world, p).map(xy)
        && (px - tx).abs() <= r
        && (py - ty).abs() <= r
        && threat(world, cat, &looker, p, mode)
    {
        return unit.predator;
    }
    let d2 = |t: usize| {
        let (x, y) = xy(t);
        (x - tx)
            .wrapping_pow(2)
            .wrapping_add((y - ty).wrapping_pow(2))
    };
    let id = |e: usize| world.zoo.entities[e].common.as_ref().map_or(0, |c| c.id);
    let (mut best, mut found) = (r.wrapping_mul(r), 0);
    match region {
        None => {}
        Some(x) if !looker.grounds => {
            // A bound that overflows leaves the scan empty.
            let (x0, x1) = (tx.wrapping_sub(r).max(0), tx.wrapping_add(r).min(w - 1));
            let (y0, y1) = (ty.wrapping_sub(r).max(0), ty.wrapping_add(r).min(h - 1));
            for cx in x0..=x1 {
                for cy in y0..=y1 {
                    let Some(t) = world.tile_index(cx, cy) else {
                        continue;
                    };
                    if world.exhibits().region(t) != Some(x) {
                        continue;
                    }
                    let first = world
                        .units_on(t)
                        .find(|&e| threat(world, cat, &looker, e, mode));
                    if let Some(e) = first
                        && d2(t) < best
                    {
                        (best, found) = (d2(t), id(e));
                    }
                }
            }
        }
        _ => {
            for e in escaped(world) {
                if threat(world, cat, &looker, e, mode) {
                    let d = tile_of(world, e).map_or(0x7fff_ffff, d2);
                    if d < best {
                        (best, found) = (d, id(e));
                    }
                }
            }
        }
    }
    found
}

#[cfg(test)]
mod tests;
