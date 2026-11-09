//! Readers of what stands in an exhibit and what it shows: the type of an entity, the animals of
//! an exhibit, the exhibit an animal stands in and the suitability it shows. The simulation and
//! the panels both read them; they live here so that the simulation never reads a panel.

use super::{EXTERIOR, GROUNDS, rating};
use crate::entity::{Catalogue, EntityType};
use crate::world::World;

/// The type of the entity at position `at` of the entity list.
pub(crate) fn type_at<'a>(world: &World, cat: &'a Catalogue, at: usize) -> Option<&'a EntityType> {
    crate::entity::type_of(cat, world.zoo.entities.get(at)?)
}

/// The entities of class animals that stand in exhibit `id`, in the order of the entity list.
pub(crate) fn animals_in(world: &World, id: u32) -> Vec<usize> {
    let width = i64::from(world.zoo.width);
    let on = |c: &crate::zoo::Common| {
        let (x, y) = (i64::from(c.x.div_euclid(64)), i64::from(c.y.div_euclid(64)));
        let on_map = (0..width).contains(&x) && y >= 0;
        on_map
            .then(|| world.exhibits().region((y * width + x) as usize))
            .flatten()
    };
    (world.zoo.entities.iter().enumerate())
        .filter(|(_, e)| e.class == "animals")
        .filter(|(_, e)| e.common.as_ref().and_then(on) == Some(id))
        .filter(|(_, e)| !e.common.as_ref().is_some_and(|c| world.is_carried(c.id)))
        .map(|(at, _)| at)
        .collect()
}

/// The exhibit that the entity at `at` stands in, if it is an exhibit and not the grounds.
pub(crate) fn animals_exhibit(world: &World, at: usize) -> Option<u32> {
    let c = world.zoo.entities.get(at)?.common.as_ref()?;
    let width = i64::from(world.zoo.width);
    let (x, y) = (i64::from(c.x.div_euclid(64)), i64::from(c.y.div_euclid(64)));
    let t = ((0..width).contains(&x) && y >= 0).then(|| (y * width + x) as usize)?;
    world
        .exhibits()
        .region(t)
        .filter(|&r| r != GROUNDS && r != EXTERIOR)
}

/// The rating the animal at `at` shows (habitat rating 6): its species' in the exhibit under it
/// with its neighbours followed, 0 on the grounds. It need not be the home's.
pub(crate) fn suitability(world: &World, cat: &Catalogue, at: usize) -> i32 {
    let Some(exhibit) = animals_exhibit(world, at) else {
        return 0;
    };
    suitability_in(world, cat, exhibit, &world.zoo.entities[at].type_name)
}

/// The rating species `key` shows in `exhibit`, its amphibious neighbours followed.
pub(crate) fn suitability_in(world: &World, cat: &Catalogue, exhibit: u32, key: &str) -> i32 {
    let near = world
        .exhibits()
        .get(exhibit)
        .map_or(Vec::new(), |e| e.amphibious.clone());
    let read = &mut |x| rating::rate(&rating::input(world, cat, x));
    rating::rating(exhibit, &near, key, read)
        .map_or(0, |r| rating::shown(r).min(i32::MAX as u32) as i32)
}
