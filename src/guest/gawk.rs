//! The gawk list and the scenery a guest can still see (guests visit 3.1 to 3.3, 3.7, 8.7).
//!
//! A scenery object (class `objects` or `building`) whose type has a positive `cEstheticWeight`
//! joins the list when it is placed inside the zoo wall, and leaves it when it is removed; one
//! placed outside and enclosed later never joins. [`update`] judges each new object once, when the
//! map changed.

use std::collections::{HashMap, HashSet};

use super::Guest;
use crate::build::number;
use crate::config::to_int;
use crate::entity::{Catalogue, EntityType};
use crate::status::zoo::{GUESTS, bonus};
use crate::world::World;
use crate::zoo::Entity;

/// A scenery object the list has judged: whether it joined, its `cEstheticWeight` and `cHelpID`,
/// and the tile it stands on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Judged {
    pub id: u32,
    pub member: bool,
    pub weight: i32,
    pub help: Option<i32>,
    pub tile: usize,
}

/// The objects judged so far in entity order, and the edit count they were judged at.
#[derive(Debug, Default)]
pub struct Gawk {
    edits: Option<u64>,
    list: Vec<Judged>,
}

impl Gawk {
    /// The members of the list in order.
    pub fn members(&self) -> impl Iterator<Item = &Judged> {
        self.list.iter().filter(|j| j.member)
    }
}

fn type_of<'a>(cat: &'a Catalogue, e: &Entity) -> Option<&'a EntityType> {
    let bytes = |s: &str| s.chars().map(|c| c as u8).collect::<Vec<u8>>();
    cat.find(&e.class, &bytes(&e.type_name), &bytes(&e.subtype))
}

/// The `cHelpID` of the type of `e`.
fn help_id(cat: &Catalogue, e: &Entity) -> Option<i32> {
    number(type_of(cat, e)?, "cHelpID").map(|v| v as i32)
}

/// The gawk list after a change of the map: the objects it knows keep their verdict, the new
/// ones are judged now, the removed ones are dropped.
pub fn update(world: &mut World, cat: &Catalogue) {
    if world.units.gawk.edits == Some(world.edits) {
        return;
    }
    let old: HashMap<u32, Judged> = (world.units.gawk.list.iter()).map(|j| (j.id, *j)).collect();
    let mut list = Vec::new();
    for (at, e) in world.zoo.entities.iter().enumerate() {
        let (Some(c), true) = (
            &e.common,
            ["objects", "building"].contains(&e.class.as_str()),
        ) else {
            continue;
        };
        if let Some(j) = old.get(&c.id) {
            list.push(*j);
            continue;
        }
        let Some(tile) = super::view::tile_of(world, at) else {
            continue;
        };
        let weight = type_of(cat, e)
            .and_then(|ty| number(ty, "cEstheticWeight"))
            .map_or(0, |v| v as i32);
        list.push(Judged {
            id: c.id,
            member: weight > 0 && world.in_zoo(tile),
            weight,
            help: help_id(cat, e),
            tile,
        });
    }
    world.units.gawk = Gawk {
        edits: Some(world.edits),
        list,
    };
}

/// 3.2: the members of the list the guest has not seen, in list order. An object is seen when
/// the entity on some tile of the seen list is that object or has the same help id.
pub fn unseen(world: &World, cat: &Catalogue, g: &Guest) -> Vec<Judged> {
    let (mut ids, mut helps) = (HashSet::new(), HashSet::new());
    for &t in &g.seen {
        for e in (0..4).filter_map(|q| world.quadrant(t, q)) {
            let e = &world.zoo.entities[e];
            ids.extend(e.common.as_ref().map(|c| c.id));
            helps.extend(help_id(cat, e));
        }
    }
    (world.units.gawk.members())
        .filter(|j| {
            let own = g.seen.contains(&j.tile) || ids.contains(&j.id);
            !(own || j.help.is_some_and(|h| helps.contains(&h)))
        })
        .copied()
        .collect()
}

/// 3.3: S, the weight of the unseen objects as a number of sites.
pub fn sites(unseen: &[Judged]) -> usize {
    let w: i64 = unseen.iter().map(|j| i64::from(j.weight)).sum();
    if w <= 0 { 0 } else { (w / 100).max(1) as usize }
}

/// 3.7, `pickGawkScenery`: the unseen object with the best score, the first on a tie; `d` is the
/// squared distance in tiles from the tile `here` of a map `width` tiles wide.
pub fn pick(unseen: &[Judged], here: usize, width: usize) -> Option<Judged> {
    let at = |t: usize| ((t % width) as i64, (t / width) as i64);
    let mut best: Option<(i64, Judged)> = None;
    for j in unseen {
        let (dx, dy) = (at(j.tile).0 - at(here).0, at(j.tile).1 - at(here).1);
        let d = dx * dx + dy * dy;
        let score = ((i64::from(j.weight) + 100) * 10 - d) / 200;
        if d < 1000 && score > -1 && best.is_none_or(|(s, _)| score > s) {
            best = Some((score, *j));
        }
    }
    best.map(|(_, j)| j)
}

/// 8.7: the set a guest of the type with name id `guest` plays at the scenery `ty`, by the
/// esthetic bonus the object has for that type (0 for a type it lists not).
pub fn gawk_set(ty: &EntityType, guest: i32) -> &'static str {
    let v = GUESTS.iter().position(|&id| id == guest);
    match v.map_or(0.0, |i| bonus(ty)[i]) as i32 {
        ..0 => "bGawkObject1",
        6..=10 => "bGawkObject3",
        11..=15 => "bGawkObject4",
        16..=25 => "bGawkObject5",
        26..=100 => "bGawkObject6",
        _ => "bGawkObject2",
    }
}

/// 8.7: the `cUsedThought` ids of the scenery type, in file order.
pub fn thoughts(ty: &EntityType) -> Vec<i32> {
    let ints = (ty.config.as_deref()).and_then(|c| c.section("Characteristics/Integers"));
    let all = ints.map(|s| s.get_all("cUsedThought")).unwrap_or_default();
    all.iter().filter_map(|v| to_int(v)).collect()
}

/// The type of the entity at position `at` of the entity list.
pub fn kind<'a>(world: &World, cat: &'a Catalogue, at: usize) -> Option<&'a EntityType> {
    type_of(cat, world.zoo.entities.get(at)?)
}

/// The tiles beside the object, outside exhibits, that a guest can stand on to look at it: the
/// neighbours of the tiles it holds that it does not hold itself.
pub fn beside(world: &World, object: &Judged) -> Vec<usize> {
    let width = world.zoo.width as i32;
    let (ox, oy) = (object.tile as i32 % width, object.tile as i32 / width);
    let held = |t: usize| {
        let at = |q| world.quadrant(t, q);
        (0..4).any(|q| {
            at(q)
                .and_then(|e| world.zoo.entities[e].common.as_ref())
                .is_some_and(|c| c.id == object.id)
        })
    };
    let near = |x: i32, y: i32| world.tile_index(x, y);
    let mut out = Vec::new();
    for (x, y) in (-4..=4).flat_map(|y| (-4..=4).map(move |x| (ox + x, oy + y))) {
        let Some(t) = near(x, y).filter(|&t| !held(t)) else {
            continue;
        };
        let touches = (-1..=1).any(|dy| (-1..=1).any(|dx| near(x + dx, y + dy).is_some_and(held)));
        if touches && world.exhibits().region(t) == Some(crate::exhibit::GROUNDS) {
            out.push(t);
        }
    }
    out
}

#[cfg(test)]
mod tests;
