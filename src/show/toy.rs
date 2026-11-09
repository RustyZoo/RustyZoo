//! The show toys (shows 23.4): the building a toy trick sends an animal into.

use crate::building::{Species, has_open_slot, kind_of};
use crate::entity::Catalogue;
use crate::entity::type_of;
use crate::sim::Rng;
use crate::world::World;

/// The largest squared distance in tiles a toy may be at.
const FAR: i64 = 46340 * 46340;

/// `find_toy`: among the buildings of the `showtrick` kind (all when `kind` is -1, else the ones
/// of the type named `kind`) that stand in the exhibit of tile `from` and have a slot open to
/// `who`, within the distance, one is drawn (one draw, none when no building qualifies).
pub(crate) fn find_toy(
    world: &World,
    cat: &Catalogue,
    rng: &mut Rng,
    who: Option<&Species>,
    from: usize,
    kind: i64,
) -> Option<u32> {
    let width = world.zoo.width as usize;
    let area = world.exhibits().region(from);
    let mut found = Vec::new();
    for e in &world.zoo.entities {
        let (Some(k), Some(c)) = (
            kind_of(cat, e, world.units.prices.default),
            e.common.as_ref(),
        ) else {
            continue;
        };
        let named = type_of(cat, e).is_some_and(|t| kind == -1 || t.name_id as i64 == kind);
        let tile = world.tile_index(c.x.div_euclid(64), c.y.div_euclid(64));
        let Some(t) = tile.filter(|_| k.satisfies("showtrick") && named) else {
            continue;
        };
        let live = world.units.buildings.get(&c.id);
        if world.exhibits().region(t) != area || !has_open_slot(world, e, &k, live, who) {
            continue;
        }
        let dx = (t % width) as i64 - (from % width) as i64;
        let dy = (t / width) as i64 - (from / width) as i64;
        if dx * dx + dy * dy <= FAR {
            found.push(c.id);
        }
    }
    (!found.is_empty()).then(|| found[rng.pick(found.len() as u32) as usize])
}
