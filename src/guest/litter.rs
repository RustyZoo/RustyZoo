//! Litter on the ground (guests 10 point 3). A guest whose litter no trash can will take drops it
//! where it stands: a scenery object of the type `objects/other/trash` on the tile, which tells
//! the maintenance task pool. The workers that sweep it are the staff's; the pieces are plain
//! entities of that type, the trash cans' fill is [`crate::building::Live::fill`].

use crate::animal::dirt;
use crate::entity::Catalogue;
use crate::exhibit::{EXTERIOR, GROUNDS};
use crate::sim::Rng;
use crate::world::{World, piece_event};

/// The event a guest posts for the maintenance task pool when litter lands, and the event a trash
/// can posts when it fills up (guest buildings 8).
pub const DROPPED: u16 = 10031;
pub const CAN_FILLING: u16 = 10030;

/// The guest `id` at position `at` of the entity list drops a piece of litter at its feet. It
/// lands on a free quarter of a tile that is not in an exhibit or a tank and not water, facing a
/// drawn direction (`rand() & 7`, drawn before the add as for dung). Whether it did; `None` when
/// the tile takes no litter (nothing is drawn or done).
pub fn drop(world: &mut World, cat: &Catalogue, at: usize, id: u32, rng: &mut Rng) -> Option<bool> {
    let Some((x, y)) = world
        .zoo
        .entities
        .get(at)
        .and_then(|e| e.common.as_ref())
        .map(|c| (c.x, c.y))
    else {
        return Some(false);
    };
    let Some(tile) = world.tile_index(x.div_euclid(64), y.div_euclid(64)) else {
        return Some(false);
    };
    let inside = world
        .exhibits()
        .region(tile)
        .is_some_and(|r| r != GROUNDS && r != EXTERIOR);
    if inside {
        return None;
    }
    let quarter = (y.rem_euclid(64) / 32 * 2 + x.rem_euclid(64) / 32) as usize;
    let Some(ty) = cat.find("objects", b"other", b"trash") else {
        return Some(false);
    };
    let names = ("objects", "other", "trash");
    if world.is_water(tile) {
        return Some(false);
    }
    let facing = rng.rand() & 7;
    if world.quadrant(tile, quarter).is_some() || !dirt::put_at(world, ty, names, (x, y), facing, 0)
    {
        return Some(false);
    }
    world.mark_rating(tile);
    let width = world.zoo.width.max(1);
    world
        .units
        .posts
        .push(piece_event(id, DROPPED, tile as u32));
    world.units.guest_note(format!(
        "litter {id} {},{}",
        tile as u32 % width,
        tile as u32 / width
    ));
    Some(true)
}

/// The eight quarter positions round a can, west to east and north to south, the centre left out
/// (guest buildings 8 point 4, open point 5), in quarters of a tile.
const AROUND: [(i32, i32); 8] = [
    (-1, -1),
    (-1, 0),
    (-1, 1),
    (0, -1),
    (0, 1),
    (1, -1),
    (1, 0),
    (1, 1),
];

/// A trash can that is deleted puts its fill back on the ground (guest buildings 8 point 4): one
/// pass over the eight quarter positions round it, at most one piece of litter for each (so at most
/// 8; the rest is lost), until the fill is out. Each position tried draws the piece's facing
/// (`rand() & 7`) first, whether the add then succeeds or not; each piece added tells the task pool.
/// A can that holds nothing, and anything that is no can, changes nothing. The pieces are not part
/// of the deletion an undo takes back.
pub fn spill(world: &mut World, cat: &Catalogue, id: u32, rng: &mut Rng) {
    let default = world.units.prices.default;
    let Some(at) = world.find(id) else {
        return;
    };
    let e = &world.zoo.entities[at];
    let can = crate::building::kind_of(cat, e, default).is_some_and(|k| k.satisfies("trash"));
    let mut left = world.units.buildings.get(&id).map_or(0, |l| l.fill);
    let (Some(c), Some(ty), true) = (
        e.common.as_ref(),
        cat.find("objects", b"other", b"trash"),
        can,
    ) else {
        return;
    };
    let (x, y) = (c.x, c.y);
    crate::building::set_fill(world, id, 0);
    for &(dx, dy) in &AROUND {
        if left == 0 {
            break;
        }
        let at = (x.saturating_add(dx * 32), y.saturating_add(dy * 32));
        let names = ("objects", "other", "trash");
        let facing = rng.rand() & 7;
        let tile = world.tile_index(at.0.div_euclid(64), at.1.div_euclid(64));
        let Some(tile) =
            tile.filter(|_| world.unrecorded(|w| dirt::put_at(w, ty, names, at, facing, 0)))
        else {
            continue;
        };
        left -= 1;
        world.mark_rating(tile);
        world
            .units
            .posts
            .push(piece_event(id, DROPPED, tile as u32));
    }
}
