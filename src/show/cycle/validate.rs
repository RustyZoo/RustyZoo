//! Whether the first animal of the species can do an item of the script in this show (shows 23.6).

use super::Cycle;
use crate::building::Species;
use crate::exhibit::GROUNDS;
use crate::show::{info_slot, toy::find_toy};
use crate::timing::file_time;
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
        let first = self.list(self.info.show.species).first().copied();
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

    /// Whether the show info of tank `x` (another than this one) has units of its scheduled
    /// keeper type; its record of that type is made when it has none (shows 32.7 point 1).
    fn other_has_keeper(&mut self, x: u32) -> bool {
        let date = file_time(self.world.units.now);
        let Some(n) = info_slot(self.world, x) else {
            return true;
        };
        let Some(scheduled) = (self.world.zoo.shows.info(n)).map(|i| i.scheduled().unwrap_or(0))
        else {
            return true;
        };
        let keeper = self.keeper_type(scheduled);
        let info = self.world.zoo.shows.infos[n].as_mut().expect("a show info");
        !info.lookup(keeper, date).units.is_empty()
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
            // Another tank's show reads the keeper type of its scheduled species (shows 3.2).
            Some(x) => self.other_has_keeper(x),
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
