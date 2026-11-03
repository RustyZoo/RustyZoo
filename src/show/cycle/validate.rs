//! Whether the first animal of the species can do an item of the script in this show (shows 23.6).

use super::Cycle;
use crate::building::Species;
use crate::exhibit::GROUNDS;
use crate::show::{KEEPER, info_of, toy::find_toy};
use crate::unit::Part;

impl Cycle<'_> {
    /// 0 when the animal can do item `i` (-1 always), else the code that aborts or removes it:
    /// 4 for no animal, 9 for a tank with no marine specialist, 3 for a trick the tank cannot
    /// hold, -1 for an item past the end of the script. A toy item draws once (shows 27 points 1
    /// and 2).
    pub(super) fn validate_item(&mut self, i: i16) -> i32 {
        if i == -1 {
            return 0;
        }
        let first = self.info.units(self.info.show.species).first().copied();
        let Some(a) = first.filter(|&a| self.is_unit(a)) else {
            return 4;
        };
        let script = self.info.show.script;
        let scripts = self.world.zoo.shows.scripts.iter();
        let item = scripts.filter(|s| s.id == script).flat_map(|s| &s.items);
        let Some(trick) = item.map(|t| t.id).nth(usize::from(i as u16)) else {
            return -1;
        };
        self.can_do(a, trick)
    }

    /// `validate` of 23.6 for animal `a` and the trick `trick`.
    fn can_do(&mut self, a: u32, trick: u16) -> i32 {
        let Some(at) = self.world.find(a) else {
            return 4;
        };
        // The keeper test reads the animal's own tank; the depth and the toy, the show's.
        let own = self.world.exhibit_of(at);
        let keeper = match own {
            Some(x) if Some(x) == self.exhibit => self.has_keeper(),
            Some(x) => info_of(self.world, x).is_none_or(|i| !i.units(KEEPER).is_empty()),
            None => true,
        };
        if !keeper {
            return 9;
        }
        let Some(it) = self.first_item(trick) else {
            return 3;
        };
        let tank = self.exhibit.or(own);
        let deep = tank.and_then(|x| self.world.tank(x));
        if !deep.is_some_and(|t| i64::from(it.depth) <= i64::from(t.level)) {
            return 3;
        }
        match it.marker {
            0 | 3 => 0,
            1 => {
                let tile = |c: &crate::zoo::Common| {
                    self.world
                        .tile_index(c.x.div_euclid(64), c.y.div_euclid(64))
                };
                let e = &self.world.zoo.entities[at];
                let gate = (self.exhibit.filter(|&x| x != GROUNDS))
                    .and_then(|x| crate::place::unit::gate_inside(self.world, x));
                let Some(from) = gate.or_else(|| e.common.as_ref().and_then(tile)) else {
                    return 3;
                };
                let egg = matches!(&self.world.units.get(a).map(|u| &u.part), Some(Part::Animal(x)) if x.egg);
                let who = Species::of(self.cat, e, egg);
                let (world, cat) = (&*self.world, self.cat);
                let found = find_toy(
                    world,
                    cat,
                    self.rng,
                    who.as_ref(),
                    from,
                    i64::from(it.building),
                );
                if found.is_some() { 0 } else { 3 }
            }
            _ => 3,
        }
    }
}
