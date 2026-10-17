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
