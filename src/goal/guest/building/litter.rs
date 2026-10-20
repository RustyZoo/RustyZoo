//! A guest's litter (guests 6.8, 10; guest buildings 8): the search for a trash can, dropping the
//! litter on the ground when none will take it, and the can taking the litter at the entrance.

use super::super::super::Cx;
use crate::building;
use crate::building::items::LITTER;
use crate::guest::litter::{self, CAN_FILLING};

/// The thought of a guest that holds litter (it is also the litter item's own).
const HAS_LITTER: i32 = 10237;
/// The pieces above which a can is too full to be chosen, and the fill at which a worker is called.
const FULL: u32 = 23;
const CALL: u32 = 13;

impl Cx<'_, '_> {
    /// The pieces in trash can `id`; none when there is no such building.
    fn fill_of(&self, id: u32) -> Option<u32> {
        self.building(id)?;
        Some(self.world.units.buildings.get(&id).map_or(0, |l| l.fill))
    }

    /// Guests 10 point 2: one trash can drawn among every can of the guest's area with an open
    /// slot within `radius` tiles (whole tiles, no price scaling), in list order; none, no draw.
    fn in_reach(&mut self, radius: i64) -> Option<u32> {
        let here = self.tile()?;
        let width = self.world.zoo.width as usize;
        let area = self.world.exhibits().region(here);
        let prices = self.world.units.prices;
        let mut kept = Vec::new();
        for e in &self.world.zoo.entities {
            let (Some(kind), Some(c)) = (
                building::kind_of(self.ctx.cat, e, prices.default),
                e.common.as_ref(),
            ) else {
                continue;
            };
            let Some(t) = self.tile_at((c.x, c.y)).filter(|_| kind.satisfies("trash")) else {
                continue;
            };
            let live = self.world.units.buildings.get(&c.id);
            let dx = (t % width) as i64 - (here % width) as i64;
            let dy = (t / width) as i64 - (here / width) as i64;
            if self.world.exhibits().region(t) == area
                && building::has_open_slot(self.world, e, &kind, live)
                && dx * dx + dy * dy <= radius * radius
            {
                kept.push(c.id);
            }
        }
        let n = kept.len() as u32;
        (n > 0).then(|| kept[self.ctx.rng.pick(n) as usize])
    }

    /// Guests 10: the seek part of the trash check for a guest that holds litter. It looks for a
    /// trash can within its search radius; with none, or when the one it set out for is gone or
    /// holds more than 23 pieces, it looks again among the cans with room and, finding none or a
    /// full one, drops the litter where it stands.
    pub(crate) fn seek_trash(&mut self) {
        let Some(k) = crate::guest::keys(self.world, self.ctx.cat, self.at) else {
            return;
        };
        self.think(HAS_LITTER, None);
        let radius = i64::from(k.search_radius);
        let found = self.nearest("trash", radius, false);
        if !found.is_ok_and(|b| self.head_for(b, "trash")) {
            self.reset();
            self.unit.goal.wandered = true;
        }
        let target = self.guest().map_or(0, |g| g.building);
        if self.fill_of(target).is_some_and(|f| f <= FULL) {
            return;
        }
        let failed = self.guest().is_some_and(|g| g.littered);
        match self.in_reach(radius) {
            Some(b) if failed && self.fill_of(b).is_some_and(|f| f < FULL + 1) => {}
            Some(b) if !failed && self.head_for(b, "trash") => {}
            _ => {
                self.drop_litter();
                self.reset();
                self.unit.goal.wandered = true;
            }
        }
    }

    /// Guests 10 point 3: the litter is put down on the guest's tile. The failure is remembered;
    /// a tile that takes no litter changes nothing.
    fn drop_litter(&mut self) {
        let (cat, at, id) = (self.ctx.cat, self.at, self.unit.id);
        let Some(dropped) = litter::drop(self.world, cat, at, id, self.ctx.rng) else {
            return;
        };
        if let Some(g) = self.guest() {
            g.littered = !dropped;
            if dropped {
                g.items[4] = None;
            }
        }
    }

    /// Guest buildings 7.6 and 8: a guest that enters a building that removes litter puts its
    /// litter in: the fill grows, a worker is called at 13 pieces and the guest loses the item.
    /// A can above 24 pieces is full and the litter is dropped as with no can.
    pub(super) fn bin_litter(&mut self, id: u32, takes: bool) {
        if !takes || self.guest().is_none_or(|g| g.items[4] != Some(LITTER)) {
            return;
        }
        let fill = self.fill_of(id).unwrap_or(0);
        if fill > FULL + 1 {
            return self.drop_litter();
        }
        let tile = (self.building(id))
            .and_then(|(e, _)| e.common.map(|c| (c.x, c.y)))
            .and_then(|p| self.tile_at(p));
        building::set_fill(self.world, id, fill + 1);
        if fill + 1 == CALL {
            let event = crate::world::piece_event(id, CAN_FILLING, tile.unwrap_or(0) as u32);
            self.world.units.posts.push(event);
        }
        if let Some(g) = self.guest() {
            g.items[4] = None;
        }
    }
}
