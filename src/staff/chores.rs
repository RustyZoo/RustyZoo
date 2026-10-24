//! What a maintenance worker looks at and changes on the map (staff 8.4 to 8.7): the trash cans
//! and the litter of a tile, the sweep, the dung and the compost, and the decayed fence piece near
//! a tile.

use crate::build::number;
use crate::building;
use crate::entity::Catalogue;
use crate::exhibit::{EXTERIOR, GROUNDS};
use crate::panel::exhibit::animals_in;
use crate::place::bulldozer::type_of;
use crate::unit::{Kind, Part};
use crate::world::World;
use crate::zoo::Entity;

/// Whether the entity is a piece of litter (`objects/other/trash`).
pub fn is_litter(e: &Entity) -> bool {
    (e.class.as_str(), e.type_name.as_str(), e.subtype.as_str()) == ("objects", "other", "trash")
}

/// The entities in the four quarter holders of tile `t`, each once, by position in the list.
fn held(world: &World, t: usize) -> Vec<usize> {
    let mut found: Vec<usize> = (0..4).filter_map(|q| world.quadrant(t, q)).collect();
    found.sort_unstable();
    found.dedup();
    found
}

/// Whether the entity is a pile of dung (`objects/other/poo` or `dinopoo`).
pub fn is_poo(e: &Entity) -> bool {
    (e.class.as_str(), e.type_name.as_str()) == ("objects", "other")
        && matches!(e.subtype.as_str(), "poo" | "dinopoo")
}

/// The tile of an entity: the one its centre lies on.
fn tile_of(world: &World, e: &Entity) -> Option<usize> {
    let c = e.common.as_ref()?;
    world.tile_index(c.x.div_euclid(64), c.y.div_euclid(64))
}

/// Whether the check done of a dung job goes on for worker `me` (staff 8.6): a pile of dung on a
/// tile of the zoo grounds, or food on the grounds or in an exhibit with no animals, unless it is
/// the target tile of another worker. A pile in an exhibit counts for nothing.
pub fn poo_left(world: &World, me: u32) -> bool {
    let others: Vec<usize> = (world.units.iter())
        .filter(|u| u.id != me && u.kind == Kind::Worker)
        .filter_map(|u| match &u.part {
            Part::Staff(s) => s.target,
            _ => None,
        })
        .collect();
    world.zoo.entities.iter().any(|e| {
        let Some(t) = tile_of(world, e) else {
            return false;
        };
        let region = world.exhibits().region(t);
        if is_poo(e) {
            return region == Some(GROUNDS);
        }
        let open = match region {
            Some(GROUNDS) => true,
            Some(EXTERIOR) | None => false,
            Some(x) => animals_in(world, x).is_empty(),
        };
        e.class == "food" && open && !others.contains(&t)
    })
}

/// The object in the centre slot of tile `t` (staff 8.6): its id, whether it is dinosaur dung and
/// whether it is food.
pub fn centre(world: &World, t: usize) -> Option<(u32, bool, bool)> {
    let e = &world.zoo.entities[world.quadrant(t, 3)?];
    let id = e.common.as_ref()?.id;
    Some((id, e.subtype == "dinopoo", e.class == "food"))
}

/// The compost pays for a pile of dung taken off tile `t` (staff 8.6): the nearest compost
/// building (Manhattan distance from the tile) gets the income of one use, and the cash the
/// recycling amount; both twice for dinosaur dung. With no compost, nothing happens.
pub fn recycle(world: &mut World, cat: &Catalogue, t: usize, dino: bool) {
    let w = world.zoo.width.max(1) as i64;
    let (x, y) = (t as i64 % w, t as i64 / w);
    let default = world.units.prices.default;
    let mut best: Option<(i64, u32, building::Kind)> = None;
    for e in &world.zoo.entities {
        let Some(c) = e.common.as_ref() else { continue };
        let Some(kind) = building::kind_of(cat, e, default).filter(|k| k.satisfies("compost"))
        else {
            continue;
        };
        let d =
            (i64::from(c.x.div_euclid(64)) - x).abs() + (i64::from(c.y.div_euclid(64)) - y).abs();
        if best.as_ref().is_none_or(|b| d < b.0) {
            best = Some((d, c.id, kind));
        }
    }
    let Some((_, id, kind)) = best else { return };
    let uses = if dino { 2.0 } else { 1.0 };
    let live = (world.units.buildings.entry(id)).or_insert_with(|| building::Live::new(&kind));
    // A hostile factor or price earns nothing, as for a guest.
    let income = (kind.factor * live.price).max(0.0) * uses;
    live.income[0] += income;
    live.income[2] += income;
    world
        .units
        .recycled
        .push(world.units.prices.recycling * uses);
}

/// The objects of a land exhibit's tiles that call a worker when its last animal goes (habitat
/// rating 5.1 point 5): what stinks, and food. Their ids and tiles, each object once, in tile order.
pub fn leftovers(world: &World, cat: &Catalogue, x: u32) -> Vec<(u32, usize)> {
    let mut found: Vec<(u32, usize)> = Vec::new();
    let tiles = world.exhibits().get(x).map_or(&[][..], |e| &e.order);
    for &t in tiles {
        for at in held(world, t) {
            let e = &world.zoo.entities[at];
            let stinks = type_of(cat, e)
                .and_then(|ty| number(ty, "cStink"))
                .is_some_and(|v| v > 0.0);
            let Some(id) = e.common.as_ref().map(|c| c.id) else {
                continue;
            };
            if (stinks || e.class == "food") && !found.iter().any(|f| f.0 == id) {
                found.push((id, t));
            }
        }
    }
    found
}

/// The ids of the trash cans of tile `t`.
pub fn cans(world: &World, cat: &Catalogue, t: usize) -> Vec<u32> {
    let default = world.units.prices.default;
    let can = |&at: &usize| {
        let e = &world.zoo.entities[at];
        building::kind_of(cat, e, default).is_some_and(|k| k.satisfies("trash"))
    };
    let ids = held(world, t).into_iter().filter(can);
    ids.filter_map(|at| Some(world.zoo.entities[at].common.as_ref()?.id))
        .collect()
}

/// The pieces of litter the cans of tile `t` hold.
pub fn trash_in(world: &World, cat: &Catalogue, t: usize) -> u32 {
    let fill = |id: &u32| world.units.buildings.get(id).map_or(0, |l| l.fill);
    cans(world, cat, t).iter().map(fill).sum()
}

/// Empties every trash can of tile `t` (staff 8.4, job 10).
pub fn empty(world: &mut World, cat: &Catalogue, t: usize) {
    for id in cans(world, cat, t) {
        building::set_fill(world, id, 0);
    }
}

/// The ids of the litter on tile `t`.
pub fn litter_in(world: &World, t: usize) -> Vec<u32> {
    let ids = held(world, t).into_iter().map(|at| &world.zoo.entities[at]);
    let litter = ids.filter(|e| is_litter(e));
    litter.filter_map(|e| Some(e.common.as_ref()?.id)).collect()
}

/// Whether any litter lies on the map.
pub fn litter_left(world: &World) -> bool {
    world.zoo.entities.iter().any(is_litter)
}

/// The tiles a sweep covers (staff 8.5): the tile and its four sides, or all its neighbours when
/// the worker's type has a `cCleanTrashRadius`.
pub fn swept(world: &World, t: usize, wide: bool) -> Vec<usize> {
    let w = world.zoo.width as i32;
    let (x, y) = (t as i32 % w, t as i32 / w);
    let near = (-1..=1).flat_map(|dx| (-1..=1).map(move |dy| (dx, dy)));
    let kept = near.filter(|&(dx, dy)| wide || dx == 0 || dy == 0);
    kept.filter_map(|(dx, dy)| world.tile_index(x + dx, y + dy))
        .collect()
}

/// The decayed fence piece to mend near tile `t` (staff 8.7): among the pieces on the four edges
/// of the tile and on the facing edges of its neighbours, the one of the lowest strength. Its id
/// and position in the entity list.
pub fn decayed_near(world: &World, cat: &Catalogue, t: usize) -> Option<(u32, usize)> {
    let mut pieces = Vec::new();
    for d in (0..8).step_by(2) {
        pieces.push(world.edge(t, d));
        pieces.push(
            world
                .neighbour(t, d)
                .and_then(|n| world.edge(n, (d + 4) % 8)),
        );
    }
    let strength = |&at: &usize| {
        world.zoo.entities[at]
            .common
            .as_ref()?
            .fence
            .map(|f| f.strength)
    };
    let worn = pieces
        .into_iter()
        .flatten()
        .filter(|&at| world.decayed(cat, at));
    let at = worn.min_by_key(|at| strength(at))?;
    Some((world.zoo.entities[at].common.as_ref()?.id, at))
}

/// The pieces a repair makes healthy (staff 8.7): the piece and the one across the same line, and
/// with `neighbours` the two lines at a right angle on the tile, each with its other side. Their
/// ids, the piece first.
pub fn mended(world: &World, at: usize, neighbours: bool) -> Vec<u32> {
    let Some((t, d)) = world.slot(at) else {
        return world.zoo.entities[at].common.iter().map(|c| c.id).collect();
    };
    let lines = [d, (d + 6) % 8, (d + 2) % 8];
    let take = if neighbours { 3 } else { 1 };
    let mut found = Vec::new();
    for &line in &lines[..take] {
        found.push(world.edge(t, line));
        found.push(
            world
                .neighbour(t, line)
                .and_then(|n| world.edge(n, (line + 4) % 8)),
        );
    }
    let ids = found.into_iter().flatten();
    ids.filter_map(|at| Some(world.zoo.entities[at].common.as_ref()?.id))
        .collect()
}
