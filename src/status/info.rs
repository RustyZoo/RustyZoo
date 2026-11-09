//! The zoo info tab of the zoo status window (zoo status 21.2): the zoo name, the admission price
//! and the counts.

use crate::entity::{Catalogue, is_of_class, type_of};
use crate::world::World;

/// The zoo name field, the price field and its two buttons, and the adult and child prices.
pub(crate) const NAME: i32 = 4199;
pub(crate) const PRICE: i32 = 4190;
pub(crate) const UP: i32 = 4195;
pub(crate) const DOWN: i32 = 4191;
pub(crate) const ADULT: i32 = 4193;
pub(crate) const CHILD: i32 = 4194;
/// The count texts: animals, exhibits, attractions, guests, staff and zoo members.
pub(crate) const COUNTS: [i32; 5] = [4115, 4117, 4119, 4121, 4122];
pub(crate) const MEMBERS: i32 = 4171;

/// How far a button moves the price.
pub(crate) const STEP: f32 = 0.25;

/// The counts of the tab (21.2 step 4), each a plain decimal number.
#[derive(Debug, Default, PartialEq, Eq)]
pub(crate) struct Counts {
    pub animals: usize,
    pub exhibits: usize,
    pub attractions: usize,
    pub guests: usize,
    pub staff: usize,
}

impl Counts {
    /// One walk over the entities by the class of their type (a subclass counts), the exhibits
    /// of the habitat list less the show tanks, and the attractions: the types whose own
    /// `[Satisfies]` section holds the key `fun`, byte for byte.
    pub(crate) fn of(world: &World, cat: &Catalogue) -> Counts {
        let mut counts = Counts {
            exhibits: (world.exhibits().list().iter())
                .filter(|x| !x.tank.as_ref().is_some_and(|t| t.show))
                .count(),
            ..Counts::default()
        };
        for e in &world.zoo.entities {
            let class = |name: &str| is_of_class(&e.class, name);
            counts.animals += usize::from(class("animals"));
            counts.guests += usize::from(class("guests"));
            counts.staff += usize::from(class("staff"));
            let fun = type_of(cat, e)
                .and_then(|ty| ty.config.as_deref()?.section("Satisfies"))
                .is_some_and(|s| s.keys().any(|k| k == b"fun"));
            counts.attractions += usize::from(fun);
        }
        counts
    }

    /// The numbers in the order of [`COUNTS`].
    pub(crate) fn list(&self) -> [usize; 5] {
        [
            self.animals,
            self.exhibits,
            self.attractions,
            self.guests,
            self.staff,
        ]
    }
}

/// The entity of the zoo entrance building: the one that holds the last quarter of the entrance
/// tile. The zoo name is its name (21.2 step 1).
pub(crate) fn entrance(world: &World) -> Option<usize> {
    let (x, y) = world.zoo.entrance;
    let tile = world.tile_index(i32::try_from(x).ok()?, i32::try_from(y).ok()?)?;
    world.quadrant(tile, 3)
}

/// The number of zoo members: the net member count of the total table, cut toward zero.
pub(crate) fn members(total: f32) -> i64 {
    total as i64
}

/// Whether a name typed in the field is kept: at least one byte other than a space (a tab is
/// one, 21.2 step 3).
pub(crate) fn kept(text: &[u8]) -> bool {
    text.iter().any(|&b| b != b' ')
}

#[cfg(test)]
mod tests;
