//! The state a saved game takes from the live world and gives back (save games 7, 9): the
//! thoughts, and the statistics and last service of each exhibit.

use super::World;
use crate::exhibit::{EXTERIOR, GROUNDS};
use crate::guest::thought::Thought;
use crate::zoo::{Entity, Zoo};
use std::collections::HashSet;

impl World {
    /// Takes the thoughts of the file, in file order, as the original's load does: one whose
    /// thinker or object is not an entity of the zoo is dropped; the exhibit is the one at the
    /// stored tile, if there is a tile.
    pub(super) fn load_thoughts(&mut self) {
        let ids: HashSet<u32> = (self.zoo.entities.iter())
            .filter_map(|e| e.common.as_ref().map(|c| c.id))
            .collect();
        for &[text, thinker, object, x, y] in self.zoo.thoughts() {
            let known = |id: u32| id == 0 || ids.contains(&id);
            if !known(thinker) || !known(object) {
                continue;
            }
            let tile = (x as i32 != -1 && y as i32 != -1)
                .then(|| self.tile_index(x as i32, y as i32))
                .flatten();
            let exhibit = tile
                .and_then(|t| self.exhibits.region(t))
                .filter(|&r| r != GROUNDS && r != EXTERIOR);
            self.units.thoughts.push_back(Thought {
                text: text as i32,
                thinker,
                object: Some(object).filter(|&o| o != 0),
                exhibit,
                gate: tile,
            });
        }
    }

    /// The zoo as a file stores it: without the entities `known` does not find (the original never
    /// makes them, so it never saves them or their thoughts), with the live thoughts and the live
    /// statistics and last service of the exhibits. An exhibit is matched with the record that
    /// names its tile; a record whose tile is in no exhibit keeps what it stored.
    pub fn saved(&self, known: impl Fn(&Entity) -> bool) -> Zoo {
        let mut zoo = self.zoo.clone();
        zoo.drop_unknown(known);
        let ids: HashSet<u32> = (zoo.entities.iter())
            .filter_map(|e| e.common.as_ref().map(|c| c.id))
            .collect();
        let tile = |t: Option<usize>| match t {
            Some(t) => [t as u32 % zoo.width, t as u32 / zoo.width],
            None => [u32::MAX; 2],
        };
        let thoughts = (self.units.thoughts.iter())
            .filter(|t| t.thinker == 0 || ids.contains(&t.thinker))
            .filter(|t| t.object.is_none_or(|o| ids.contains(&o)))
            .map(|t| {
                let [x, y] = tile(t.gate);
                [t.text as u32, t.thinker, t.object.unwrap_or(0), x, y]
            })
            .collect();
        zoo.set_thoughts(thoughts);
        let records = self.zoo.habitat_tiles.iter().zip(&mut zoo.habitat_info);
        for (&(x, y), info) in records {
            let exhibit = (self.tile_index(x, y))
                .and_then(|t| self.exhibits.region(t))
                .and_then(|id| self.exhibits.get(id));
            let Some(e) = exhibit else { continue };
            // Kept last month, this month, total; stored this month, last month, total.
            let order = |v: [u32; 3]| [v[1], v[0], v[2]];
            let (d, u) = (e.stats.donations, e.stats.upkeep);
            let bits = |v: [f32; 3]| order(v.map(f32::to_bits));
            let words = [bits(d), bits(u), order(e.stats.count)];
            info.stats = words.concat().try_into().unwrap_or(info.stats);
            info.serviced = e.rating.serviced as u32;
        }
        zoo
    }
}
