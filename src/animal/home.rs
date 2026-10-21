//! The home exhibit of an animal (tank fauna 12.3, unit placing 7.3) and the rewrite of its
//! recommendation bits (habitat rating 5.6).
//!
//! [`settle`] runs after exhibit `x` was rated. For each animal on its tiles it recomputes the
//! home: the exhibit itself, none in a show tank, and for an amphibious animal in a tank with land
//! neighbours the neighbour whose land rating of the species is the best (the first on ties).
//! Then, unless the animal is an egg, it takes the bits of the pass from the record of `x` when
//! `x` is a land exhibit, or the species is not amphibious, or the home is `x`. So an amphibious
//! animal that swims in a linked tank keeps the bits it got in its land home.

use crate::entity::Catalogue;
use crate::exhibit::rating::{self, Record};
use crate::unit::Part;
use crate::world::World;

/// The bits the animal checks own (animals 8): 0 to 7, 10, 14, 15, 16 and 62. The pass rewrites
/// all the others, 56 included (the pair of terrain 17).
const CHECKS: u64 = 0xFF | 1 << 10 | 1 << 14 | 1 << 15 | 1 << 16 | 1 << 62;

/// The home of an animal standing in exhibit `x` whose species record there is `own`.
fn home(world: &World, cat: &Catalogue, x: u32, own: &Record) -> Option<u32> {
    let e = world.exhibits().get(x)?;
    let show = |n: u32| {
        world
            .exhibits()
            .get(n)
            .is_some_and(|e| e.tank.is_some_and(|t| t.show))
    };
    if show(x) {
        return None;
    }
    if e.tank.is_none() || !own.amphibious || e.amphibious.is_empty() {
        return Some(x);
    }
    let mut best: Option<(u32, f32)> = None;
    for &n in &e.amphibious {
        let records = rating::rate(&rating::input(world, cat, n));
        let found = records.iter().find(|r| r.key == own.key);
        if let Some(r) = found.filter(|r| best.is_none_or(|(_, s)| r.rating() > s)) {
            best = Some((n, r.rating()));
        }
    }
    best.map(|(n, _)| n).filter(|&n| !show(n))
}

/// Settles the home and the bits of the animals of exhibit `x`, rated just now.
pub fn settle(world: &mut World, cat: &Catalogue, x: u32) {
    let Some(e) = world.exhibits().get(x) else {
        return;
    };
    let (land, records) = (e.tank.is_none(), e.rating.records.clone());
    for (at, _) in rating::animals_at(world, cat, &e.order.clone()) {
        let entity = &world.zoo.entities[at];
        let (Some(c), Some(own)) = (
            &entity.common,
            records.iter().find(|r| r.key == entity.type_name),
        ) else {
            continue;
        };
        let id = c.id;
        let h = home(world, cat, x, own);
        let Some(Part::Animal(a)) = world.units.get_mut(id).map(|u| &mut u.part) else {
            continue;
        };
        a.home = h;
        if !a.egg && (land || !own.amphibious || h == Some(x)) {
            a.advice = a.advice & CHECKS | own.advice & !CHECKS;
            a.important = a.important & CHECKS | own.important & !CHECKS;
        }
    }
}
