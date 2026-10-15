//! The bulldozer (placing 7): which object is under the pointer, which ones a dragged rectangle
//! takes, what each is refunded, what is asked before it goes, and the removal itself.
//!
//! Everything works on the world's lookups (the path of a tile, the holder of a quadrant, the
//! edge slot of a tile) and never on where an entity is stored, so a fence is found wherever
//! the world keeps it. An object is named by its position in `zoo.entities` while the world
//! stands still and by its id when something is asked in between.

use super::{LIFE, side};
use crate::build::{cost, number};
use crate::entity::{Catalogue, EntityType};
use crate::exhibit::{EXTERIOR, GROUNDS};
use crate::money::{CONSTRUCTION_REFUND, Money};
use crate::render::order::Kind;
use crate::world::World;
use crate::zoo::{Entity, Fence};

/// A point within this many world units of a tile side picks the fence on it. The original picks
/// by the picture, which the engine does not draw per object.
const EDGE: i32 = 16;

/// The type of an entity.
pub fn type_of<'a>(cat: &'a Catalogue, e: &Entity) -> Option<&'a EntityType> {
    let bytes = |s: &str| s.chars().map(|c| c as u8).collect::<Vec<u8>>();
    cat.find(&e.class, &bytes(&e.type_name), &bytes(&e.subtype))
}

/// Whether the bulldozer can remove the entity at position `e` (placing 7.1): its tile is in the
/// zoo and it is a path or one of the scenery classes whose type is deletable (`cDeletable` 1,
/// which is the default). Animals, guests and staff never are.
pub fn removable(world: &World, cat: &Catalogue, e: usize) -> bool {
    let (Some(entity), Some(shape)) = (world.zoo.entities.get(e), world.shape(e)) else {
        return false;
    };
    let (Some(c), Some(ty)) = (entity.common.as_ref(), type_of(cat, entity)) else {
        return false;
    };
    // A fence belongs to the tile of its slot, any other object to the tile under its position.
    let tile = match shape.kind {
        Kind::Edge | Kind::Wall => world.slot(e).map(|(t, _)| t),
        _ => world.tile_index(c.x.div_euclid(64), c.y.div_euclid(64)),
    };
    let in_zoo = tile.is_some_and(|t| world.in_zoo(t));
    in_zoo
        && match shape.kind {
            Kind::Path => true,
            Kind::Edge | Kind::Wall | Kind::Fixed => {
                number(ty, "cDeletable").is_none_or(|n| n == 1.0)
            }
            Kind::Unit | Kind::Overlay => false,
        }
}

/// The sell price of an entity (placing 7.2): 0.8 of its purchase cost; for a fence 0.4 once its
/// life is at or below the decayed life of its type, 0 once it is broken.
pub fn refund(cat: &Catalogue, e: &Entity) -> f32 {
    let Some(ty) = type_of(cat, e) else {
        return 0.0;
    };
    let rate = match Kind::of_class(&e.class) {
        Kind::Edge | Kind::Wall => {
            let life = e.common.as_ref().and_then(|c| c.fence).map_or_else(
                || number(ty, "cLife").unwrap_or(LIFE),
                |f: Fence| f.life as f32,
            );
            match life {
                l if l < 1.0 => 0.0,
                l if l <= number(ty, "cDecayedLife").unwrap_or(0.0) => 0.4,
                _ => 0.8,
            }
        }
        _ => 0.8,
    };
    cost(ty) * rate
}

/// The entity under the world point: the holder of the quarter tile there, else the fence on the
/// side of the tile nearest to the point when it is within [`EDGE`] units of it (in the tile's
/// own slot or in the one across the same line), else the path of the tile. Whether it can be
/// removed is another matter ([`removable`]).
pub fn under(world: &World, point: (i64, i64)) -> Option<usize> {
    let near = |v: i64| v.clamp(-(1 << 20), 1 << 20) as i32;
    let (x, y) = (near(point.0), near(point.1));
    let t = world.tile_index(x.div_euclid(64), y.div_euclid(64))?;
    let (lx, ly) = (x.rem_euclid(64), y.rem_euclid(64));
    if let Some(held) = world.quadrant(t, (ly / 32 * 2 + lx / 32) as usize) {
        return Some(held);
    }
    fence_near(world, t, lx, ly).or_else(|| world.path(t))
}

/// The fence on the side of tile `t` nearest to the point (`lx`, `ly`) of the tile when the point
/// is within [`EDGE`] units of it, in the tile's own slot or in the one across the same line.
pub fn fence_near(world: &World, t: usize, lx: i32, ly: i32) -> Option<usize> {
    let d = side(lx, ly) as usize;
    let distance = [ly, 0, 63 - lx, 0, 63 - ly, 0, lx][d];
    let across = world
        .neighbour(t, d)
        .and_then(|n| world.edge(n, (d + 4) % 8));
    (distance <= EDGE)
        .then(|| world.edge(t, d).or(across))
        .flatten()
}

/// The entities a rectangle of tiles collects, each once (placing 7.1 area drag): for each tile
/// of it that is on the map and in the zoo, its path, the holders of its four quarters and the
/// fences in its four edge slots. Not all of them can be removed.
pub fn area(world: &World, a: (i32, i32), b: (i32, i32)) -> Vec<usize> {
    let mut found = Vec::new();
    for ty in a.1.min(b.1)..=a.1.max(b.1) {
        for tx in a.0.min(b.0)..=a.0.max(b.0) {
            let Some(t) = world.tile_index(tx, ty).filter(|&t| world.in_zoo(t)) else {
                continue;
            };
            let objects = [world.path(t)]
                .into_iter()
                .chain((0..4).map(|q| world.quadrant(t, q)))
                .chain([0, 2, 4, 6].map(|d| world.edge(t, d)));
            for e in objects.flatten() {
                if !found.contains(&e) {
                    found.push(e);
                }
            }
        }
    }
    found
}

/// What is asked before an object is removed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Question {
    /// "Are you sure you want to sell ...?" (string 154): a type that needs confirming.
    Sell,
    /// The removal joins two exhibits (string 156): their numbers.
    Merge(u32, u32),
    /// The removal opens an exhibit to the zoo grounds (string 157): its number.
    Escape(u32),
}

impl Question {
    /// The two regions of an exhibit question (the grounds are 0), smaller first: a pair that
    /// was declined stays declined while others merge.
    pub fn pair(self) -> Option<(u32, u32)> {
        let (a, b) = match self {
            Question::Sell => return None,
            Question::Merge(x, y) => (x, y),
            Question::Escape(x) => (GROUNDS, x),
        };
        Some((a.min(b), a.max(b)))
    }
}

/// What the bulldozer does with an entity.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Check {
    /// Remove it.
    Go,
    /// Ask first.
    Ask(Question),
    /// Leave it: its exhibit question was answered No for these two regions already.
    Keep,
}

/// The exhibit question of a piece of exhibit fence (placing 7.1): its tile and the tile across
/// are in different regions and the slot across on the same line holds no exhibit fence, so
/// removing it opens the line. Both regions exhibits is a merge; one of them the zoo grounds an
/// escape.
fn exhibit_question(world: &World, e: usize) -> Option<Question> {
    if !world.shape(e)?.fence {
        return None;
    }
    let (t, d) = world.slot(e)?;
    let n = world.neighbour(t, d)?;
    let closed_across = world
        .edge(n, (d + 4) % 8)
        .is_some_and(|o| world.shape(o).is_some_and(|s| s.fence));
    let (x, y) = (world.exhibits().region(t)?, world.exhibits().region(n)?);
    let zoo = |r: u32| r == GROUNDS || r == EXTERIOR;
    if closed_across || x == y {
        return None;
    }
    match (zoo(x), zoo(y)) {
        (false, false) => Some(Question::Merge(x, y)),
        (false, true) => Some(Question::Escape(x)),
        (true, false) => Some(Question::Escape(y)),
        (true, true) => None,
    }
}

/// What to do with the entity at position `e` before removing it: the exhibit question comes
/// first and replaces the sell one; `sell` is the switch `[UI] confirmSell`; `declined` the pairs
/// of regions this press already answered No for.
pub fn check(
    world: &World,
    cat: &Catalogue,
    e: usize,
    sell: bool,
    declined: &[(u32, u32)],
) -> Check {
    if let Some(q) = exhibit_question(world, e) {
        return match q.pair().is_some_and(|p| declined.contains(&p)) {
            true => Check::Keep,
            false => Check::Ask(q),
        };
    }
    let needs = world
        .zoo
        .entities
        .get(e)
        .and_then(|entity| type_of(cat, entity))
        .is_some_and(|ty| number(ty, "cNeedsConfirm") == Some(1.0));
    match sell && needs {
        true => Check::Ask(Question::Sell),
        false => Check::Go,
    }
}

/// What a removal did, for the log.
#[derive(Debug, Clone, PartialEq)]
pub struct Removed {
    pub id: u32,
    /// `class/type/subtype`.
    pub name: String,
    pub tile: (i32, i32),
    /// The side of a fence.
    pub side: Option<u32>,
    pub refund: f32,
}

/// Removes the entity with this id and books its refund as a construction refund (placing 7.2).
/// The world gives an exhibit whose gate goes a new one and tells the exhibits what happened
/// (exhibits and money 3.3). `None` if there is no such entity.
pub fn remove(world: &mut World, money: &mut Money, cat: &Catalogue, id: u32) -> Option<Removed> {
    let at = world.find(id)?;
    let entity = &world.zoo.entities[at];
    let c = entity.common.as_ref()?;
    let fence = matches!(Kind::of_class(&entity.class), Kind::Edge | Kind::Wall);
    let mut removed = Removed {
        id,
        name: format!("{}/{}/{}", entity.class, entity.type_name, entity.subtype),
        tile: (c.x.div_euclid(64), c.y.div_euclid(64)),
        side: fence.then_some(c.rotation % 8),
        refund: refund(cat, entity),
    };
    let slot = world.slot(at);
    if let Some((t, d)) = slot {
        let width = world.zoo.width as usize;
        removed.tile = ((t % width) as i32, (t / width) as i32);
        removed.side = Some(d as u32);
    }
    world.remove(id);
    money.refund(CONSTRUCTION_REFUND, removed.refund);
    Some(removed)
}

#[cfg(test)]
mod tests;
