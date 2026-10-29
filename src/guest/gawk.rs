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
/// the tile it stands on and whether it is gawked at from its front only.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Judged {
    pub id: u32,
    pub member: bool,
    pub weight: i32,
    pub help: Option<i32>,
    pub tile: usize,
    /// `cGawkOnlyFromFront` is 1.
    pub front: bool,
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
pub fn help_id(cat: &Catalogue, e: &Entity) -> Option<i32> {
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
            front: type_of(cat, e).and_then(|ty| number(ty, "cGawkOnlyFromFront")) == Some(1.0),
        });
    }
    world.units.gawk = Gawk {
        edits: Some(world.edits),
        list,
    };
}

/// 3.2: the ids and the help ids of the entities held by the last quadrant (offsets 32 or more) of
/// the tiles of the seen list.
pub fn marks(world: &World, cat: &Catalogue, g: &Guest) -> (HashSet<u32>, HashSet<i32>) {
    let (mut ids, mut helps) = (HashSet::new(), HashSet::new());
    for &t in &g.seen {
        if let Some(e) = world.quadrant(t, 3) {
            let e = &world.zoo.entities[e];
            ids.extend(e.common.as_ref().map(|c| c.id));
            helps.extend(help_id(cat, e));
        }
    }
    (ids, helps)
}

/// 3.2: the members of the list the guest has not seen, in list order. An object is seen when
/// the entity held by the last quadrant of some tile of the seen list is that object or has the
/// same help id; its own tile in the list counts for nothing.
pub fn unseen(world: &World, cat: &Catalogue, g: &Guest) -> Vec<Judged> {
    let (ids, helps) = marks(world, cat, g);
    (world.units.gawk.members())
        .filter(|j| !(ids.contains(&j.id) || j.help.is_some_and(|h| helps.contains(&h))))
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

/// 3.7, the viewing place: the tiles on the ring around the footprint of the object (the
/// footprint box grown by one tile, corners included, those on the map), x outer and y inner. The
/// tiles with a path piece come first as one list, else the others; with `cGawkOnlyFromFront`
/// only the side the object faces is kept (an odd rotation keeps the whole ring).
pub fn ring(world: &World, object: &Judged) -> Vec<usize> {
    let Some((e, shape)) = world
        .find(object.id)
        .and_then(|e| Some((e, world.shape(e)?)))
    else {
        return Vec::new();
    };
    let Some(c) = world.zoo.entities[e].common.as_ref() else {
        return Vec::new();
    };
    let (mut fx, mut fy) = shape.footprint;
    if c.rotation % 4 != 0 {
        (fx, fy) = (fy, fx);
    }
    let (n, m) = ((fx + 1) / 2, (fy + 1) / 2);
    let ax = (c.x - fx * 16).div_euclid(64);
    let ay = (c.y - fy * 16).div_euclid(64);
    let mut with = Vec::new();
    let mut without = Vec::new();
    for x in ax - 1..=ax + n {
        for y in ay - 1..=ay + m {
            let edge = x == ax - 1 || x == ax + n || y == ay - 1 || y == ay + m;
            let side = match (object.front, c.rotation % 8) {
                (true, 0) => y == ay - 1,
                (true, 4) => y == ay + m,
                (true, 2) => x == ax + n,
                (true, 6) => x == ax - 1,
                _ => true,
            };
            if let (true, true, Some(t)) = (edge, side, world.tile_index(x, y)) {
                match world.path(t) {
                    Some(_) => with.push(t),
                    None => without.push(t),
                }
            }
        }
    }
    if with.is_empty() { without } else { with }
}

#[cfg(test)]
mod tests;
