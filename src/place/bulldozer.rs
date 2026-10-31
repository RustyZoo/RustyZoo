//! The bulldozer (placing 7): which objects a dragged rectangle takes, what each is refunded,
//! what is asked before it goes, and the removal itself. What is under the pointer is picked by
//! pixels ([`crate::render::Staged::pick`]).
//!
//! Everything works on the world's lookups (the path of a tile, the holder of a quadrant, the
//! edge slot of a tile) and never on where an entity is stored, so a fence is found wherever
//! the world keeps it. An object is named by its position in `zoo.entities` while the world
//! stands still and by its id when something is asked in between.

use super::{LIFE, unit};
use crate::build::{cost, number};
use crate::entity::{Catalogue, EntityType};
use crate::exhibit::{EXTERIOR, GROUNDS};
use crate::money::{ANIMAL_REFUND, CONSTRUCTION_REFUND, Money};
use crate::render::order::Kind;
use crate::sim::Rng;
use crate::staff;
use crate::world::World;
use crate::zoo::{Entity, Fence};

/// What the bulldozer and the gate tool add to the width of the pixel pick, which is
/// [`crate::render::Staged::pick`] (placing 7.1); the selection adds nothing.
pub const WIDEN: i32 = 5;

/// The type of an entity.
pub fn type_of<'a>(cat: &'a Catalogue, e: &Entity) -> Option<&'a EntityType> {
    let bytes = |s: &str| s.chars().map(|c| c as u8).collect::<Vec<u8>>();
    cat.find(&e.class, &bytes(&e.type_name), &bytes(&e.subtype))
}

/// The tied wall of the fence piece at position `e` (placing 7.1): the piece stands on a tile of
/// a tank, the tile across is not in that tank, and the slot across on the same line holds a tank
/// wall.
pub fn tied(world: &World, e: usize) -> Option<usize> {
    world.shape(e).filter(|s| s.fence)?;
    let (t, d) = world.slot(e)?;
    let n = world.neighbour(t, d)?;
    let tank = world.tank_at(t)?;
    let across = world
        .edge(n, (d + 4) % 8)
        .filter(|&q| world.shape(q).is_some_and(|s| s.tank.wall))?;
    (world.tank_at(n) != Some(tank)).then_some(across)
}

/// Whether the tank wall at position `e` has a wall height: it stands on a tile of a tank
/// (exhibits and money 4.7).
fn has_height(world: &World, e: usize) -> bool {
    world
        .slot(e)
        .is_some_and(|(t, _)| world.tank_at(t).is_some())
}

/// The entities a click on the fence piece at position `e` takes, in the order they go: the
/// piece and its tied wall, the piece first when the tied wall has a wall height, else the tied
/// wall first (placing 7.1).
pub fn with_tied(world: &World, e: usize) -> Vec<usize> {
    match tied(world, e) {
        Some(q) if has_height(world, q) => vec![e, q],
        Some(q) => vec![q, e],
        None => vec![e],
    }
}

/// Whether the entity at position `e` is a tank wall with a tied wall on a line between a show
/// tank and a tank that is not one: it stays (placing 7.1).
pub fn mixed_tanks(world: &World, e: usize) -> bool {
    let tank = |t: usize| world.tank_at(t).and_then(|id| world.tank(id));
    let Some((t, d)) = world
        .shape(e)
        .filter(|s| s.tank.wall)
        .and_then(|_| world.slot(e))
    else {
        return false;
    };
    let across = world.neighbour(t, d);
    tied(world, e).is_some()
        && matches!((tank(t), across.and_then(tank)), (Some(a), Some(b)) if a.show != b.show)
}

/// Whether the bulldozer can remove the entity at position `e` (placing 7.1): its tile is in the
/// zoo and it is a path or one of the scenery classes whose type is deletable (`cDeletable` 1,
/// which is the default); a tank wall between two tanks only when both are show tanks or
/// neither. Animals, guests and staff never are.
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
        && !mixed_tanks(world, e)
        && match shape.kind {
            Kind::Path => true,
            Kind::Edge | Kind::Wall | Kind::Fixed => {
                number(ty, "cDeletable").is_none_or(|n| n == 1.0)
            }
            // An animal is sold and a staff member fired (the unit's panel buttons, until the
            // panels exist).
            Kind::Unit => unit::is_unit(ty),
            Kind::Overlay => false,
        }
}

/// The sell price of an entity (placing 7.2): 0.8 of its purchase cost; for a fence 0.4 once its
/// life is at or below the decayed life of its type, 0 once it is broken; the same for a tank
/// filter by its health (tank staff 7.2).
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
        _ if e.class == "filter" => {
            let health = e.common.as_ref().and_then(|c| c.filter).map(|f| f.health);
            health.map_or(0.8, |h| crate::world::filter_sell_rate(ty, h))
        }
        _ if unit::is_animal(ty) => return unit::new_sell_price(ty),
        // Firing refunds nothing (unit placing 10.2).
        _ if staff::is_staff(ty) => return 0.0,
        _ => 0.8,
    };
    cost(ty) * rate
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
                // The tied wall that has no wall height is collected before the piece.
                let tied = tied(world, e).filter(|&q| !has_height(world, q));
                for e in tied.into_iter().chain([e]) {
                    if !found.contains(&e) {
                        found.push(e);
                    }
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
    /// "Are you sure you want to fire ...?" (string 155): a staff member.
    Fire,
    /// The removal joins two exhibits (string 156): their numbers.
    Merge(u32, u32),
    /// The removal opens an exhibit to the zoo grounds (string 157): its number.
    Escape(u32),
}

impl Question {
    /// The two regions of an exhibit question (the grounds are 0), smaller first: a pair that
    /// was asked stays asked (a silent No) while others merge.
    pub fn pair(self) -> Option<(u32, u32)> {
        let (a, b) = match self {
            Question::Sell | Question::Fire => return None,
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
    /// Leave it: these two regions were asked about already (a silent No).
    Keep,
}

/// The exhibit question of a piece of exhibit fence (placing 7.1): its tile and the tile across
/// are in different regions and the slot across on the same line holds no exhibit fence, so
/// removing it opens the line; on a line between two tanks it is the other way round: it asks
/// only when the slot across holds a piece. A tank wall of a tank asks when the slot across is
/// empty or holds a tank wall (its tied wall). Both regions exhibits is a merge; one of them the
/// zoo grounds an escape.
fn exhibit_question(world: &World, e: usize) -> Option<Question> {
    if !world.shape(e)?.fence {
        return None;
    }
    let (t, d) = world.slot(e)?;
    let n = world.neighbour(t, d)?;
    let across = world.edge(n, (d + 4) % 8);
    let closed_across = across.is_some_and(|o| world.shape(o).is_some_and(|s| s.fence));
    let tank_across = across.is_some_and(|o| world.shape(o).is_some_and(|s| s.tank.wall));
    let (x, y) = (world.exhibits().region(t)?, world.exhibits().region(n)?);
    let zoo = |r: u32| r == GROUNDS || r == EXTERIOR;
    let tanks = world.tank_at(t).is_some() && world.tank_at(n).is_some();
    let wall = world.shape(e)?.tank.wall && world.tank_at(t).is_some();
    let ask = match (tanks, wall) {
        (true, _) => closed_across,
        (false, true) => !closed_across || tank_across,
        (false, false) => !closed_across,
    };
    if !ask || x == y {
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
/// first and replaces the sell one; `sell` is the switch `[UI] confirmSell`; `asked` the pairs
/// of regions this press asked about already.
pub fn check(world: &World, cat: &Catalogue, e: usize, sell: bool, asked: &[(u32, u32)]) -> Check {
    if let Some(q) = exhibit_question(world, e) {
        return match q.pair().is_some_and(|p| asked.contains(&p)) {
            true => Check::Keep,
            false => Check::Ask(q),
        };
    }
    let ty = (world.zoo.entities.get(e)).and_then(|entity| type_of(cat, entity));
    let needs = ty.is_some_and(|ty| unit::is_unit(ty) || number(ty, "cNeedsConfirm") == Some(1.0));
    let fire = ty.is_some_and(staff::is_staff);
    match (sell && needs, fire) {
        (true, true) => Check::Ask(Question::Fire),
        (true, false) => Check::Ask(Question::Sell),
        (false, _) => Check::Go,
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
/// (exhibits and money 3.3). `None` if there is no such entity. A trash can's spill draws from `rng`.
pub fn remove(
    world: &mut World,
    money: &mut Money,
    cat: &Catalogue,
    id: u32,
    rng: &mut Rng,
) -> Option<Removed> {
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
    let (animal, staff) = type_of(cat, &world.zoo.entities[at]).map_or((false, false), |ty| {
        (unit::is_animal(ty), staff::is_staff(ty))
    });
    // A tour guide fired lets its group go at once (tour guide 12.1).
    crate::goal::release_group(world, cat, rng, id);
    crate::building::leave::push_out(world, cat, id);
    crate::guest::litter::spill(world, cat, id, rng);
    world.remove(id);
    world.priced(removed.refund);
    let category = if animal {
        ANIMAL_REFUND
    } else {
        CONSTRUCTION_REFUND
    };
    if !staff {
        money.refund(category, removed.refund);
    }
    Some(removed)
}

#[cfg(test)]
mod tests;
