//! Who comes to a show and what the audience pays (shows 7.1, 7.3): the guest's search for a
//! grandstand with a show soon, and the booking of the price of a seat.

use super::{faces, habitat_of, info_of};
use crate::entity::Catalogue;
use crate::exhibit::viewing::{self, Ground};
use crate::sim::Rng;
use crate::world::World;

/// The grandstand a guest at tile `tile` with the tiles `seen` goes to (shows 7.1 point 2), the
/// nearest of those with a show soon that it has not seen, if within `reach` (`maxDistToGoToShow`):
/// the distance in tiles plus one draw of 0 to 14, drawn for each grandstand with its flag up
/// before it is asked whether the guest has seen the show.
pub(crate) fn find(
    world: &World,
    cat: &Catalogue,
    rng: &mut Rng,
    tile: Option<usize>,
    seen: &[usize],
    reach: i32,
) -> Option<u32> {
    let default = world.units.prices.default;
    let mut flagged: Vec<(usize, u32)> = (world.units.buildings.iter())
        .filter(|(_, l)| l.fill as i32 > 0)
        .filter_map(|(&id, _)| Some((world.find(id)?, id)))
        .filter(|&(at, _)| {
            crate::building::kind_of(cat, &world.zoo.entities[at], default)
                .is_some_and(|k| k.satisfies("grandstand"))
        })
        .collect();
    flagged.sort_unstable();
    let (mut ground, mut best, mut best_d) = (None, None, i32::MAX);
    for (at, id) in flagged {
        // A missing tile makes the distance the largest number, and the sum wraps (shows 7.1 point 3).
        let d = match (tile, tile_of(world, at)) {
            (Some(a), Some(b)) => {
                let width = world.zoo.width as usize;
                let (dx, dy) = (
                    (a % width).abs_diff(b % width),
                    (a / width).abs_diff(b / width),
                );
                i32::try_from(dx + dy).unwrap_or(i32::MAX)
            }
            _ => i32::MAX,
        };
        let d = d.wrapping_add(rng.pick(15) as i32);
        if d < best_d
            && !seen_show(
                world,
                ground.get_or_insert_with(|| Ground::of(world, viewing::bodies(cat))),
                at,
                seen,
            )
        {
            (best, best_d) = (Some(id), d);
        }
    }
    best.filter(|_| best_d <= reach)
}

/// The tile of the entity at position `at` of the entity list.
fn tile_of(world: &World, at: usize) -> Option<usize> {
    let c = world.zoo.entities[at].common.as_ref()?;
    world.tile_index(c.x.div_euclid(64), c.y.div_euclid(64))
}

/// Whether the guest has seen the show tank grandstand `at` faces (shows 7.1 point 4): the first
/// tank with a show info (whatever its show id) that the grandstand faces, and a seen tile in it.
fn seen_show(world: &World, ground: &Ground, at: usize, seen: &[usize]) -> bool {
    let tank = (world.exhibits().list().iter()).find(|x| {
        x.tank.is_some() && info_of(world, x.id).is_some() && faces(world, ground, x.id, at)
    });
    tank.is_some_and(|x| {
        seen.iter()
            .any(|&t| world.exhibits().region(t) == Some(x.id))
    })
}

/// The price `r` of a seat in a grandstand of show `show` is booked in the show's figures (shows
/// 7.3 point 3 steps 1 and 2). Whether the show exists, so that the caller books the income.
pub(crate) fn pay(world: &mut World, show: u16, r: f32) -> bool {
    let date = crate::timing::file_time(world.units.now);
    let Some(info) = habitat_of(world, show).and_then(|n| world.zoo.shows.infos[n].as_mut()) else {
        return false;
    };
    info.book(r, date);
    true
}
