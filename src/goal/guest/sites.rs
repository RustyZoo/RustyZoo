//! The site goal (100) and viewing (0x70) of a guest (goals 11.1, 11.5; guests visit 3, 5). The
//! walk to scenery it picks ends in [`super::scenery`].

use super::super::{Cx, HABITAT, SCENERY, SITE};
use crate::exhibit::GROUNDS;
use crate::guest::gawk::{self, Judged};
use crate::guest::view;
use crate::unit::{Part, movement};

/// The apes and the dinosaurs a guest mimics (guests visit 5.2), by name id.
const APES: [i32; 2] = [5003, 5021];
const DINOSAURS: [i32; 6] = [5085, 5097, 5098, 5099, 5326, 5327];

/// The score of an exhibit (goals 11.1): its attractiveness, 500 more when it holds the favourite
/// species, less a tenth of what is left of 1000 after the squared distance `d` (at most 990),
/// plus 100: the far exhibit wins.
pub(super) fn site_score(attractiveness: i32, loved: bool, d: i64) -> i64 {
    i64::from(attractiveness) + 500 * i64::from(loved) - (1000 - d) / 10 + 100
}

impl Cx<'_, '_> {
    /// Ladder point 10.2 and 10.3: a guest with an exhibit or scenery to see takes the site task,
    /// mode 4; with none the seen count goes and `checkLeaveZoo` may send it away. Whether the
    /// ladder ends there.
    pub(in crate::goal) fn next_site(&mut self) -> bool {
        let Part::Guest(g) = &self.unit.part else {
            return false;
        };
        let sites = view::sites(self.world, self.ctx.cat, g).1.len();
        if sites + gawk::sites(&gawk::unseen(self.world, self.ctx.cat, g)) > 0 {
            self.set_task(SITE, 4, 0, None);
            return true;
        }
        if let Part::Guest(g) = &mut self.unit.part {
            g.seen_count = 0;
        }
        self.check_leave_zoo()
    }

    /// 11.1, decide: a guest that has seen all the sites forgets them, stops and may leave;
    /// otherwise one draw picks an exhibit or the scenery it has not seen: the exhibit with the
    /// best score is walked to, or the scenery object `pickGawkScenery` chooses.
    pub(in crate::goal) fn site_decide(&mut self) {
        if self.record().target.tile.is_some() {
            return;
        }
        let Part::Guest(g) = &mut self.unit.part else {
            return;
        };
        let (n, unseen) = view::sites(self.world, self.ctx.cat, g);
        let favourite = g.favourite;
        let scenery = gawk::unseen(self.world, self.ctx.cat, g);
        if g.seen_count as usize >= n {
            g.forget();
            movement::stop(self.unit);
            self.record().target.clear();
            self.check_leave_zoo();
            return;
        }
        let total = unseen.len() + gawk::sites(&scenery);
        let (Some(here), false) = (self.tile(), total == 0) else {
            return;
        };
        // r = rand % (H + A + S), with no attraction as a site yet.
        if self.ctx.rng.pick(total as u32) as usize >= unseen.len() {
            return self.scenery_decide(&scenery, here);
        }
        let width = self.world.zoo.width as usize;
        let beasts = view::beasts(self.world, self.ctx.cat);
        let mut best: Option<(i64, u32)> = None;
        for &x in &unseen {
            let Some(t) = view::viewing_tile(self.world, self.ctx.cat, x, self.ctx.rng) else {
                continue;
            };
            let (dx, dy) = (
                (t % width) as i64 - (here % width) as i64,
                (t / width) as i64 - (here / width) as i64,
            );
            let d = (dx * dx + dy * dy).min(990);
            let loved = beasts
                .iter()
                .any(|b| b.exhibit == x && Some(b.name) == favourite);
            let a = view::attractiveness(self.world, self.ctx.cat, x);
            let score = site_score(a, loved, d);
            if score > -1 && best.is_none_or(|(s, _)| score > s) {
                best = Some((score, x));
            }
        }
        let Some((_, winner)) = best else {
            return;
        };
        let to = view::viewing_tile(self.world, self.ctx.cat, winner, self.ctx.rng);
        // A tile off the zoo grounds ends it: the draws are made, the task stays.
        let Some(to) = to.filter(|&t| self.world.exhibits().region(t) == Some(GROUNDS)) else {
            return;
        };
        if self.path_to(to, false) && self.start_walk(false, false, false) {
            self.set_task(HABITAT, 0, 0, Some(to));
        }
    }

    /// 11.1, scenery: the object `pickGawkScenery` chooses, then one draw over the tiles around
    /// its footprint (guests visit 3.7); only that tile is tried, and the task becomes 0x80 with
    /// it. Nothing within reach, or no way there: nothing happens this time.
    pub(in crate::goal) fn scenery_decide(&mut self, scenery: &[Judged], here: usize) {
        let width = self.world.zoo.width as usize;
        let Some(object) = gawk::pick(scenery, here, width) else {
            return;
        };
        let ring = gawk::ring(self.world, &object);
        if ring.is_empty() {
            return;
        }
        let place = ring[self.ctx.rng.pick(ring.len() as u32) as usize];
        if self.path_to(place, false) && self.start_walk(false, false, false) {
            self.set_task(SCENERY, 0, object.id, Some(place));
        }
    }

    /// 5.1, init: the guest looks for its viewing area and the exhibit behind it, which it must
    /// not have seen; the set `bGawk` starts for as long as the exhibit holds it (5.3); the
    /// exhibit and the species in view are remembered. False clears the goal.
    pub(in crate::goal) fn gawk_init(&mut self) -> bool {
        let Some(k) = crate::guest::keys(self.world, self.ctx.cat, self.at) else {
            return false;
        };
        let tile = self.tile();
        let found = view::lookup(self.world, self.ctx.cat, tile, self.ctx.rng);
        let Part::Guest(g) = &mut self.unit.part else {
            return false;
        };
        g.va = found;
        let Some(s) = tile
            .zip(found)
            .and_then(|(t, dir)| view::sight(self.world, self.ctx.cat, t, dir))
        else {
            return false;
        };
        if self.unit.runner.top() == Some("bGawk") {
            return true;
        }
        // The guest faces the direction of its area, also when viewing then fails.
        if let Some(c) = found
            .zip(self.world.zoo.entities[self.at].common.as_ref())
            .filter(|(k, c)| u32::from(*k) != c.rotation)
        {
            let (point, facing) = ((c.1.x, c.1.y, c.1.z), u32::from(c.0));
            self.world.place_unit(self.at, point, facing);
            let id = self.unit.id;
            self.world.units.note(format!("face {id} {facing}"));
        }
        let Part::Guest(g) = &mut self.unit.part else {
            return false;
        };
        if view::seen(self.world, g, s.exhibit) {
            g.forget();
            return false;
        }
        let Some(x) = self.world.exhibits().get(s.exhibit) else {
            return false;
        };
        let order = x.order.clone();
        // A random tile of the exhibit: the seen list gets it, the set gets no target.
        let Some(&drawn) = order.get(self.ctx.rng.pick(order.len() as u32) as usize) else {
            return false;
        };
        if !self.start_set("bGawk", false) {
            return false;
        }
        let rng = &mut *self.ctx.rng;
        let fallback = 10 + (rng.rand() & 3) as i32;
        let band = match s.average {
            -100..=-60 => Some((0, 0)),
            -59..=20 => Some((1, 6)),
            21..=60 => Some((2, 6)),
            61..=80 => Some((3, 6)),
            81..=100 => Some((4, 6)),
            _ => None,
        };
        let base = match band.and_then(|(i, n)| Some((*k.gawk.get(i)?, n))) {
            Some((time, n)) => time.saturating_add(rng.pick(n) as i32),
            None => fallback,
        };
        let attractiveness = view::attractiveness(self.world, self.ctx.cat, s.exhibit);
        let more = [20, 50, 90, 150, 200]
            .iter()
            .take_while(|&&limit| attractiveness >= limit)
            .count();
        let seconds = base.saturating_add(k.gawk_attr.get(more).copied().unwrap_or(0));
        self.record().duration = seconds.max(0) as u32;
        let (id, now) = (self.unit.id, self.world.units.now);
        let Part::Guest(g) = &mut self.unit.part else {
            return false;
        };
        g.viewed_at = now;
        view::see(g, drawn);
        let drop = (g.recent.len() + s.species.len().min(5)).saturating_sub(5);
        g.recent.drain(..drop);
        g.recent.extend(s.species.iter().take(5));
        self.world
            .units
            .guest_note(format!("view {id} {}", s.exhibit));
        movement::stop(self.unit);
        true
    }

    /// 5.2, decide: when the set has ended the guest may mimic an ape or a dinosaur it saw, test
    /// the fence or tap the tank (their draws are made; no fence or tank answers yet), else it
    /// gawks again.
    pub(in crate::goal) fn gawk_decide(&mut self) {
        if matches!(
            self.unit.runner.top(),
            Some("bGawk" | "bMimic" | "bMimic2" | "bTapWall")
        ) {
            return;
        }
        let (Some(k), Part::Guest(g)) = (
            crate::guest::keys(self.world, self.ctx.cat, self.at),
            &self.unit.part,
        ) else {
            return;
        };
        let model = g
            .recent
            .iter()
            .rev()
            .find(|s| APES.contains(s) || DINOSAURS.contains(s))
            .copied();
        if let Some(model) = model
            && self.ctx.rng.chance(k.reactions[0])
            && self.start_set(
                if APES.contains(&model) {
                    "bMimic"
                } else {
                    "bMimic2"
                },
                false,
            )
        {
            return;
        }
        self.ctx.rng.chance(k.reactions[1]);
        self.ctx.rng.chance(k.reactions[2]);
        self.start_set("bGawk", false);
    }

    /// 11.5, check done: the duration is over.
    pub(in crate::goal) fn gawk_check_done(&mut self) {
        let now = self.now();
        let over = self.record().duration_over(now);
        self.record().done |= over;
    }

    /// 11.5, complete: the guest leaves its area (the time and the donation), the goal is cleared
    /// and the wandered flag goes up.
    pub(in crate::goal) fn gawk_complete(&mut self) {
        let (id, tile) = (self.unit.id, self.tile());
        if let Part::Guest(g) = &mut self.unit.part {
            view::leave_area(g, id, self.world, self.ctx.cat, tile);
        }
        self.reset();
        self.unit.goal.wandered = true;
    }
}
