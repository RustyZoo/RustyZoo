//! What triggers 3, 5 and 8 do to the game (scenarios 7.2, 7.4, 7.5).

use crate::build::number;
use crate::entity::Catalogue;
use crate::money::{self, Money};
use crate::place::bulldozer::type_of;
use crate::place::unit::{crate_spot_in, named};
use crate::render::order::ground_height;
use crate::world::{Shape, World};
use crate::zoo::{Common, Entity};

/// The crate goes this far from the entrance, in tiles of Manhattan distance (7.4).
const BOX_RANGE: i32 = 20;

/// Trigger 3 (7.2): `how` 0 books `amount` as a donation, 1 adds it to the cash unbooked, 2 sets
/// the cash to it; any other value does nothing.
pub fn cash(money: &mut Money, how: i32, amount: i32) {
    let amount = amount as f32;
    match how {
        0 => money.refund(money::DONATIONS, amount),
        1 => money.cash += amount,
        2 => money.cash = amount,
        _ => {}
    }
}

/// Trigger 8 (7.5): removes every entity whose name id (`by` 0), or for an animal whose family (1),
/// genus (2) or species (3), is `id`. Nothing is paid back and nothing is undone. Returns the
/// number removed. `before` is told each id just before its removal, while the entity is still
/// there: the map view lets it go first (7.5).
pub fn remove(
    world: &mut World,
    cat: &Catalogue,
    by: i32,
    id: i32,
    mut before: impl FnMut(u32),
) -> usize {
    let key = ["", "cFamily", "cGenus"].get(by as usize).copied();
    let hit = |e: &Entity| {
        let Some(ty) = type_of(cat, e) else {
            return false;
        };
        match (by, key) {
            (0, _) => ty.name_id == id,
            (3, _) => ty.class == "animals" && ty.name_id == id,
            (_, Some(key)) => ty.class == "animals" && number(ty, key) == Some(id as f32),
            _ => false,
        }
    };
    let ids: Vec<u32> = (world.zoo.entities.iter())
        .filter(|e| hit(e))
        .filter_map(|e| e.common.as_ref().map(|c| c.id))
        .collect();
    for &id in &ids {
        before(id);
        crate::building::leave::push_out(world, cat, id);
        world.unrecorded(|w| w.remove(id));
    }
    ids.len()
}

/// Trigger 5 (7.4): an animal of name id `name` and subtype `sub` (`m`, `f` or `y`) is made, put
/// near the zoo entrance and boxed on its first turn. Returns its id; `None` without that type, an
/// entrance tile or a place for the crate.
pub fn boxed(
    world: &mut World,
    cat: &Catalogue,
    name: i32,
    sub: u8,
    shape: impl Fn(&Entity) -> Option<Shape>,
) -> Option<u32> {
    let ty = (cat.types.iter())
        .find(|t| t.class == "animals" && t.name_id == name && t.subtype == [sub])?;
    let (ex, ey) = world.zoo.entrance;
    let start = world.tile_index(i32::try_from(ex).ok()?, i32::try_from(ey).ok()?)?;
    let t = crate_spot_in(world, start, BOX_RANGE)?;
    let width = world.zoo.width as usize;
    let text = |bytes: &[u8]| bytes.iter().map(|&b| char::from(b)).collect::<String>();
    let entity = Entity {
        class: "animals".into(),
        type_name: text(&ty.type_name),
        subtype: text(&ty.subtype),
        body_size: None,
        common: Some(Common {
            x: (t % width) as i32 * 64 + 32,
            y: (t / width) as i32 * 64 + 32,
            z: ground_height(&world.zoo.tiles[t], 32, 32),
            name: named(world, ty),
            ..Default::default()
        }),
    };
    let shape = shape(&entity);
    let id = world.unrecorded(|w| w.add(entity, shape))?;
    world.units.get_mut(id)?.boxed = true;
    Some(id)
}

#[cfg(test)]
mod tests;
