//! The home exhibit of an animal (tank fauna 12.3, unit placing 7.3) and the rewrite of its
//! recommendation bits (habitat rating 5.6).
//!
//! [`home`] is read when asked, as the original does: the exhibit under the animal, or for an
//! amphibious animal in a tank with land neighbours the neighbour with the best [`land_score`]
//! (the first on ties), or for an animal in a show tank with show neighbours the best rated of
//! those, and only when that exhibit is not a show tank and has a gate. [`settle`]
//! runs after exhibit `x` was rated: for each animal on its tiles that is not an egg it takes the
//! bits of the pass from the record of `x` when `x` is a land exhibit, or the species is not
//! amphibious, or the home is `x`. So an amphibious animal that swims in a linked tank keeps the
//! bits it got in its land home, and an animal with no home keeps them too.

use crate::entity::Catalogue;
use crate::exhibit::rating::{self, Likes, Record};
use crate::unit::Part;
use crate::world::World;

/// The bits the animal checks own (animals 8): 0 to 7, 10, 14, 15, 16 and 62. The pass rewrites
/// all the others, 56 included (the pair of terrain 17).
const CHECKS: u64 = 0xFF | 1 << 10 | 1 << 14 | 1 << 15 | 1 << 16 | 1 << 62;

/// The score that picks the land neighbour a tank animal calls home (tank fauna 12.3): 0.3
/// terrain + 0.2 objects + 0.1 foliage + 0.1 rocks + 0.1 elevation, no shelters and no toys. The
/// first term is stored as a single-precision float, each next product is added to the sum before
/// that is rounded again (the product is not rounded first), and the last sum is not rounded at
/// all.
pub fn land_score(r: &Record) -> f64 {
    let term = |w: f32, p: f32| f64::from(w) * f64::from(p);
    let mut sum = f64::from(0.3f32 * r.terrain);
    for (w, p) in [(0.2, r.objects), (0.1, r.foliage), (0.1, r.rocks)] {
        sum = f64::from((sum + term(w, p)) as f32);
    }
    sum + term(0.1, r.elevation)
}

/// The home of an animal of species `likes` that stands in exhibit `x`, `None` when the candidate
/// is a show tank or has no gate (no other is tried).
pub fn home(world: &World, cat: &Catalogue, x: u32, likes: &Likes) -> Option<u32> {
    let e = world.exhibits().get(x)?;
    let mut home = x;
    if e.tank.is_some() && likes.amphibious && !e.amphibious.is_empty() {
        let mut best: Option<(u32, f64)> = None;
        for &n in &e.amphibious {
            let records = rating::rate(&rating::input(world, cat, n));
            let score = (records.iter().find(|r| r.key == likes.key)).map_or(0.0, land_score);
            if best.is_none_or(|(_, s)| score > s) {
                best = Some((n, score));
            }
        }
        home = best.map_or(x, |(n, _)| n);
    } else if e.tank.is_some_and(|t| t.show) && !e.show_near.is_empty() {
        // A show tank: the best rated of its show neighbours, each following its own neighbours.
        let mut records = |n| rating::rate(&rating::input(world, cat, n));
        let mut best: Option<(u32, f32)> = None;
        for &n in &e.show_near {
            let near = world
                .exhibits()
                .get(n)
                .map_or(Vec::new(), |e| e.amphibious.clone());
            let score = rating::rating(n, &near, &likes.key, &mut records).unwrap_or_default();
            if best.is_none_or(|(_, s)| score > s) {
                best = Some((n, score));
            }
        }
        home = best.map_or(x, |(n, _)| n);
    }
    let h = world.exhibits().get(home)?;
    (h.gate.is_some() && !h.tank.is_some_and(|t| t.show)).then_some(home)
}

/// Settles the bits of the animals of exhibit `x`, rated just now.
pub fn settle(world: &mut World, cat: &Catalogue, x: u32) {
    let Some(e) = world.exhibits().get(x) else {
        return;
    };
    let (land, records) = (e.tank.is_none(), e.rating.records.clone());
    for (at, ty) in rating::animals_at(world, cat, &e.order.clone()) {
        let entity = &world.zoo.entities[at];
        let (Some(c), Some(own)) = (
            &entity.common,
            records.iter().find(|r| r.key == entity.type_name),
        ) else {
            continue;
        };
        let id = c.id;
        if !(land || !own.amphibious || home(world, cat, x, &Likes::of(ty)) == Some(x)) {
            continue;
        }
        if let Some(Part::Animal(a)) = world.units.get_mut(id).map(|u| &mut u.part)
            && !a.egg
        {
            a.advice = a.advice & CHECKS | own.advice & !CHECKS;
            a.important = a.important & CHECKS | own.important & !CHECKS;
        }
    }
}
