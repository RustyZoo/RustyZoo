//! The drown task (0x84) of a guest (tank fauna 13.1, 13.2): a man drowns and is removed when the
//! drown set ends; a woman or a child is put back on land instead. Nothing is drawn or saved.

use super::{Cx, NONE};
use crate::exhibit::GROUNDS;
use crate::render::order::ground_height;
use crate::unit::{Part, movement};

/// The warning at the tile of a drowning man, the thought, and the message when he is gone.
const WARNING: i32 = 10528;
const THOUGHT: i32 = 10566;
const GONE: i32 = 10251;
/// The sets of the drowning, and the return set of a guest that came out of a changing room.
const SURF: &str = "bSurfDrown";
const SUIT: &str = "bSuitDrown";
const CHANGING_ROOM: &str = "bReturnchroom0";
/// Rounds of the rescue's square scan.
const ROUNDS: i32 = 150;

impl Cx<'_, '_> {
    /// Init (13.1 points 2 to 4): fails for a unit that is not a guest; a child or a woman is
    /// rescued, a man posts, thinks and starts the drown set of his return set.
    pub(super) fn drown_init(&mut self) -> bool {
        let Part::Guest(g) = &self.unit.part else {
            return false;
        };
        if g.child || g.female {
            self.rescue();
            return true;
        }
        let id = self.unit.id;
        self.world.units.post(WARNING, id);
        self.think(THOUGHT, None);
        if let Part::Guest(g) = &mut self.unit.part {
            let set = if g.return_set == Some(CHANGING_ROOM) {
                SUIT
            } else {
                SURF
            };
            (g.return_set, g.returning) = (Some(set), None);
            self.start_with(set, false, true);
        }
        true
    }

    /// Check done (13.1 point 5): the drown set is over.
    pub(super) fn drown_check_done(&mut self) {
        let drowning = [SURF, SUIT].map(Some).contains(&self.unit.runner.top());
        self.record().done = !drowning;
    }

    /// Complete (13.1 point 6): the record is cleared, the man is removed and no longer drawn
    /// (the removal pass takes him off the list), and the message names him.
    pub(super) fn drown_complete(&mut self) {
        let id = self.unit.id;
        let entity = &self.world.zoo.entities[self.at];
        let name = entity
            .common
            .as_ref()
            .map_or_else(String::new, |c| c.name.clone());
        self.set_task(NONE, 0, 0, None);
        self.ctx.remove(self.world, id);
        self.world.hide_unit(self.at);
        self.world.units.post_named(GONE, id, vec![name]);
    }

    /// The rescue (13.2): the guest goes to the first tile of the scan that passes, if any, then
    /// stops everything. The recovering state follows the task, which the stop clears.
    fn rescue(&mut self) {
        if let Some(t) = self.tile().and_then(|here| self.rescue_tile(here)) {
            if let Part::Guest(g) = &mut self.unit.part {
                g.returning = None;
            }
            movement::stop(self.unit);
            let width = self.world.zoo.width as usize;
            let (x, y) = ((t % width) as i32 * 64 + 32, (t / width) as i32 * 64 + 32);
            let z = ground_height(&self.world.zoo.tiles[t], 32, 32);
            let rotation = self.world.zoo.entities[self.at]
                .common
                .as_ref()
                .map_or(0, |c| c.rotation);
            self.world.place_unit(self.at, (x, y, z), rotation);
        }
        super::stop_all(self.unit);
        self.unit.animation = Default::default();
    }

    /// 13.2 point 1: round `k` scans the square from `-k` to `k - 1` on both axes, x outer, and
    /// each round rescans the inner tiles, which cannot pass if they failed before: only the new
    /// ring of each round is tested, in the same order.
    fn rescue_tile(&self, here: usize) -> Option<usize> {
        let width = self.world.zoo.width as i32;
        let (x, y) = (here as i32 % width, here as i32 / width);
        (1..=ROUNDS)
            .flat_map(|k| {
                (-k..k).flat_map(move |dx| {
                    let inner = (1 - k..k - 1).contains(&dx);
                    (-k..k)
                        .filter(move |&dy| !inner || dy == -k || dy == k - 1)
                        .map(move |dy| (x + dx, y + dy))
                })
            })
            .filter_map(|(tx, ty)| self.world.tile_index(tx, ty))
            .find(|&t| self.rescue_passes(t))
    }

    /// 13.2 point 2: in the zoo, dry, in the zoo grounds (so not a tank tile) and the guest's
    /// footprint fits at the tile's centre (placing 5.2).
    fn rescue_passes(&self, t: usize) -> bool {
        let world = &*self.world;
        world.in_zoo(t)
            && !world.is_water(t)
            && world.exhibits().region(t) == Some(GROUNDS)
            && self.footprint_fits(t)
    }

    /// The footprint check of placing 5.2 for the guest on the ground at the centre of tile `t`,
    /// the guest herself ignored: its quarters are on the map, in the zoo and free, flat at one
    /// height and fence-free when large, and no other unit's rectangle meets its own. The ground
    /// test of item 2 is not repeated: the tile is dry and a guest does not only swim.
    fn footprint_fits(&self, t: usize) -> bool {
        let world = &*self.world;
        let Some((fx, fy)) = world.unit_footprint(self.at) else {
            return false;
        };
        let rotation = world.zoo.entities[self.at]
            .common
            .as_ref()
            .map_or(0, |c| c.rotation);
        let (fx, fy) = if rotation.is_multiple_of(4) {
            (fx, fy)
        } else {
            (fy, fx)
        };
        let width = world.zoo.width as i32;
        let (px, py) = ((t as i32 % width) * 64 + 32, (t as i32 / width) * 64 + 32);
        let (x0, y0) = (px - 16 * (fx - 1), py - 16 * (fy - 1));
        let mut tiles = Vec::new();
        for (i, j) in (0..fy).flat_map(|j| (0..fx).map(move |i| (i, j))) {
            let (qx, qy) = ((x0 + 32 * i).div_euclid(32), (y0 + 32 * j).div_euclid(32));
            let Some(at) = world.tile_index(qx.div_euclid(2), qy.div_euclid(2)) else {
                return false;
            };
            let q = (qy.rem_euclid(2) * 2 + qx.rem_euclid(2)) as usize;
            if !world.in_zoo(at) || world.quadrant(at, q).is_some() {
                return false;
            }
            tiles.push(at);
        }
        let uneven = (fx > 1 || fy > 1) && crate::place::uneven(world, &tiles);
        if uneven || (fx > 2 || fy > 2) && crate::place::fenced(world, x0, y0, (fx, fy)) {
            return false;
        }
        // Units in the air and rampaging animals let the object through (placing 5.2 item 4).
        let through = |i: usize| {
            let c = world.zoo.entities[i].common.as_ref();
            let part = c.and_then(|c| world.units.get(c.id)).map(|u| &u.part);
            c.is_some_and(|c| c.movement_mode == 1)
                || matches!(part, Some(Part::Animal(a)) if a.rampaging)
        };
        let meets = |centre: i32, own: i32, other: i32, reach: i32| {
            i64::from((centre - other).abs()) < i64::from(own) + i64::from(reach)
        };
        !crate::place::unit::units_on(world, &tiles)
            .into_iter()
            .any(|(i, x, y, (hx, hy))| {
                i != self.at
                    && !through(i)
                    && meets(px, 16 * fx, x, hx)
                    && meets(py, 16 * fy, y, hy)
            })
    }
}
