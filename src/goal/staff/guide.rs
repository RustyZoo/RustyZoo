//! A tour guide's goals and the guest's side of its group (tour guide 5 to 10, guests visit 10).
//!
//! A guide repeats legs (task 0x76): it picks an exhibit, a viewing area of it on which guests
//! stand and a tile of that area, and walks there. At the start of a leg it releases its group and
//! gathers a new one, and it gathers again every `cCrowdCheck` status ticks while it walks or
//! talks. On its tile it talks (task 0x77) when it has a group, else it starts the next leg. A
//! guest that follows keeps its done task 0x76 until the guide releases it, and gets the bonus of
//! the viewing area when it arrives while the guide talks. The log lines are `staff job <id>
//! va|speak|random` and `guest follow <id> <guide>` (0 when it leaves the group).

use super::super::{Cx, LEG, NONE, RANDOM, TALK};
use crate::animal::food::nearest_direction;
use crate::behaviour::{Data, NoData};
use crate::entity::Catalogue;
use crate::exhibit::GROUNDS;
use crate::guest::{self, HAPPINESS, view};
use crate::sim::{Events, Rng};
use crate::staff::Crowd;
use crate::unit::{Ctx, Kind, Part, Unit, movement};
use crate::world::World;

/// The tasks that keep a guest from following (guests visit 10.1): the building chain, finding
/// food and eating, leaving, avoiding a predator, being sick and the way back to a building.
fn busy(task: u8) -> bool {
    (103..=109).contains(&task) || [113, 117, 121, 130].contains(&task)
}

/// Runs `f` on the goal update context of the guest `id`, taken out of the table for it.
fn with_guest<R>(
    world: &mut World,
    ctx: &mut Ctx,
    data: &dyn Data,
    id: u32,
    f: impl FnOnce(&mut Cx) -> R,
) -> Option<R> {
    let at = world.find(id)?;
    let slot = world.units.get_mut(id)?;
    let mut unit = std::mem::replace(slot, Unit::new(id, slot.kind));
    let r = f(&mut Cx {
        unit: &mut unit,
        world: &mut *world,
        ctx,
        at,
        data,
    });
    if let Some(slot) = world.units.get_mut(id) {
        *slot = unit;
    }
    Some(r)
}

/// The guests on the map, in entity order, with the tile each stands on.
fn guests(w: &World) -> Vec<(u32, usize)> {
    let on_map = |(at, e): (usize, &crate::zoo::Entity)| {
        let id = e.common.as_ref()?.id;
        let u = w.units.get(id).filter(|u| !u.carried && !u.hidden)?;
        (e.class == "guests" && u.kind == Kind::Guest).then_some((id, view::tile_of(w, at)?))
    };
    w.zoo
        .entities
        .iter()
        .enumerate()
        .filter_map(on_map)
        .collect()
}

/// 9.1: every guest that follows `guide` is released, in entity order, one draw each (every caller
/// passes 100 per cent).
fn release_followers(world: &mut World, ctx: &mut Ctx, data: &dyn Data, guide: u32) {
    let follows = |u: &Unit| matches!(&u.part, Part::Guest(g) if g.guide == guide);
    let ids: Vec<u32> = (guests(world).into_iter())
        .filter(|&(id, _)| world.units.get(id).is_some_and(follows))
        .map(|g| g.0)
        .collect();
    for id in ids {
        ctx.rng.chance(100);
        with_guest(world, ctx, data, id, |cx| cx.reset());
    }
}

/// 12.1: the guide `id` is taken off the map for good (fired) or picked up: it releases its group
/// at once, its list and leg-start flag go, and the draws come from `rng`.
pub fn release_group(world: &mut World, cat: &Catalogue, rng: &mut Rng, id: u32) {
    if world.units.get(id).is_none_or(|u| u.kind != Kind::Guide) {
        return;
    }
    let (events, mut removals) = (Events::default(), Vec::new());
    let mut ctx = Ctx {
        rng,
        events: &events,
        removals: &mut removals,
        step: 0,
        cat,
    };
    release_followers(world, &mut ctx, &NoData, id);
    if let Some(Part::Staff(s)) = world.units.get_mut(id).map(|u| &mut u.part) {
        s.members.clear();
        s.leg = false;
    }
}

impl Cx<'_, '_> {
    fn with_guest<R>(&mut self, id: u32, f: impl FnOnce(&mut Cx) -> R) -> Option<R> {
        with_guest(self.world, self.ctx, self.data, id, f)
    }

    fn guests(&self) -> Vec<(u32, usize)> {
        guests(self.world)
    }

    // ---- the ladder and the status tick

    /// 5: a frightened guide lets its group go and avoids its predator; else with no task, or a
    /// done one, a guide starts a leg; the leg-start flag goes down.
    pub(in crate::goal) fn guide_ladder(&mut self, task: u8, done: bool) {
        if self.staff_fright(task) {
            return;
        }
        if task == NONE || done {
            self.guide_state(|s| s.leg = false);
            self.set_task(LEG, 4, 0, None);
        }
    }

    /// 9.2: the guide's own reset empties its list without touching the guests.
    pub(in crate::goal) fn guide_reset(&mut self) {
        self.guide_state(|s| (s.members.clear(), s.leg = false));
    }

    fn guide_state<R>(&mut self, f: impl FnOnce(&mut crate::unit::Staff) -> R) -> Option<R> {
        self.staff().map(f)
    }

    /// 4: the countdown of the gathering, lowered at each status tick; at 0 it starts again and,
    /// while the guide talks or walks a leg, a gathering runs.
    pub(in crate::goal) fn crowd_check(&mut self) {
        let check = Crowd::of(self.who(), &self.world.research).check;
        let Some(due) = self.guide_state(|s| {
            let was = s.crowd;
            s.crowd = was.saturating_sub(1);
            if was == 0 {
                s.crowd = check.saturating_sub(1);
            }
            was == 0
        }) else {
            return;
        };
        let task = self.record().task;
        if due && (task == TALK || (task == LEG && self.unit.moving)) {
            self.gather();
        }
    }

    // ---- 0x76, a leg

    /// 6.3, check done: once per leg the walk has started, the group is released and a new one
    /// gathered; done on the target tile.
    pub(in crate::goal) fn leg_check_done(&mut self) {
        let fresh = self.guide_state(|s| !s.leg).unwrap_or(false);
        if fresh && self.unit.moving {
            self.release();
            self.gather();
            self.guide_state(|s| s.leg = true);
        }
        if self.reached(false) {
            self.record().done = true;
        }
    }

    /// 6.1, decide, with no target tile: an exhibit, a viewing area with guests on it and a tile
    /// of it; failing that the guide walks at random.
    pub(in crate::goal) fn leg_decide(&mut self) {
        if self.record().target.tile.is_some() {
            return;
        }
        if self.leg_walk().is_none() {
            self.reset();
            self.set_task(RANDOM, 4, 0, None);
        }
    }

    fn leg_walk(&mut self) -> Option<()> {
        let assigned = self.guide_state(|s| s.assigned.clone())?;
        let list = view::tour_exhibits(self.world, self.ctx.cat, &assigned);
        let x = *list.get(self.ctx.rng.pick(list.len() as u32) as usize)?;
        let guests = self.guests();
        let areas = view::viewing_areas(self.world, self.ctx.cat, x);
        let stood = |a: &&Vec<usize>| !a.is_empty() && guests.iter().any(|g| a.contains(&g.1));
        let areas: Vec<&Vec<usize>> = areas.iter().filter(stood).collect();
        let area = match self.ctx.rng.rand() & 1 {
            0 => areas
                .get(self.ctx.rng.pick(areas.len() as u32) as usize)
                .copied()?,
            _ => {
                let here = self.tile()?;
                let mut best: Option<(i64, &Vec<usize>)> = None;
                for &a in &areas {
                    let t = a[self.ctx.rng.pick(a.len() as u32) as usize];
                    let d = crate::staff::service::distance(self.world, here, t);
                    if best.is_none_or(|(b, _)| d < b) {
                        best = Some((d, a));
                    }
                }
                best?.1
            }
        };
        let tile = area[self.ctx.rng.pick(area.len() as u32) as usize];
        (self.path_to(tile, false) && self.start_walk(false, false, false)).then_some(())
    }

    /// 6.4, complete: the group listens to a talk; with none the guide goes on to the next leg.
    pub(in crate::goal) fn leg_complete(&mut self) {
        self.abort();
        match self.guide_state(|s| !s.members.is_empty()) {
            Some(true) => self.set_task(TALK, 0, 0, None),
            _ => self.reset(),
        }
    }

    // ---- 0x77, the talk

    /// 7, init: the set `bSpeak` starts; the guests gathered during the talk come to the guide's
    /// own tile.
    pub(in crate::goal) fn talk_init(&mut self) -> bool {
        if !self.start_set("bSpeak", false) {
            return false;
        }
        movement::stop(self.unit);
        let tile = self.tile();
        let world = &*self.world;
        self.unit.goal.record.target.set_tile(world, tile);
        true
    }

    /// 7, check done: the talk is over when the set `bSpeak` is no longer the top one.
    pub(in crate::goal) fn talk_check_done(&mut self) {
        let over = self.unit.runner.top() != Some("bSpeak");
        self.record().done |= over;
    }

    pub(in crate::goal) fn talk_complete(&mut self) {
        self.set_task(NONE, 0, 0, None);
    }

    // ---- gathering and releasing

    /// 8: every guest within `cCrowdRadius` tiles draws `cFollowChance` and, while the group has
    /// room, is offered to follow. The sex bonus of the original never applies.
    fn gather(&mut self) {
        let k = Crowd::of(self.who(), &self.world.research);
        // A member that left the map is forgotten.
        let members = self.guide_state(|s| s.members.clone()).unwrap_or_default();
        let alive: Vec<u32> = (members.into_iter())
            .filter(|&m| self.world.units.get(m).is_some())
            .collect();
        let full = |n: usize| i64::try_from(n).is_ok_and(|n| n >= i64::from(k.max));
        self.guide_state(|s| s.members = alive);
        let Some(here) = self.tile() else {
            return;
        };
        if self.guide_state(|s| full(s.members.len())).unwrap_or(true) {
            return;
        }
        let width = self.world.zoo.width as usize;
        let (gx, gy) = ((here % width) as i64, (here / width) as i64);
        let near = |t: usize| {
            let (x, y) = ((t % width) as i64, (t / width) as i64);
            ((x - gx).abs().max((y - gy).abs()) <= i64::from(k.radius)).then_some((x, y))
        };
        // The square is scanned x outer, y inner; the guests of a tile in entity order.
        let mut found: Vec<((i64, i64), u32)> = (self.guests().into_iter())
            .filter_map(|(id, t)| near(t).map(|xy| (xy, id)))
            .collect();
        found.sort_by_key(|f| f.0);
        for (_, id) in found {
            let room = self
                .guide_state(|s| !full(s.members.len()))
                .unwrap_or(false);
            if self.ctx.rng.chance(k.chance) && room {
                self.offer(id);
            }
        }
    }

    /// 9.1: every guest that follows this guide is released, one draw each (every caller passes
    /// 100 per cent), and the list is emptied.
    pub(in crate::goal) fn release(&mut self) {
        release_followers(self.world, self.ctx, self.data, self.unit.id);
        self.guide_reset();
    }

    // ---- the guest's side

    /// 10.1, the guest `id` is offered to follow: it must be free, in no other group and without
    /// a pressing need; one draw sends it to the guide's target tile or one beside it.
    fn offer(&mut self, id: u32) {
        let (task, guide) = (self.record().task, self.unit.id);
        let Some(target) = self
            .record()
            .target
            .tile
            .filter(|_| [LEG, TALK].contains(&task))
        else {
            return;
        };
        let (Some(at), Some(Part::Guest(g))) = (
            self.world.find(id),
            self.world.units.get(id).map(|u| &u.part),
        ) else {
            return;
        };
        let Some(k) = guest::keys(self.world, self.ctx.cat, at) else {
            return;
        };
        let occupied = busy(self.world.units.get(id).map_or(0, |u| u.goal.record.task));
        let pressing = (0..4).any(|i| g.values[1 + i] >= k.needs[i][1]);
        let (known, member) = (g.guide == 0 || g.guide == guide, g.guide == guide);
        if !known || occupied || g.sick || g.leave || pressing {
            return;
        }
        let beside = self.ctx.rng.pick(10);
        let started = self.with_guest(id, |cx| {
            cx.follow_tile(target, beside == 0)?;
            cx.set_task(LEG, 3, 0, None);
            Some(cx.start_walk(false, false, false))
        });
        if started.flatten() != Some(true) {
            return;
        }
        if let Some(Part::Guest(g)) = self.world.units.get_mut(id).map(|u| &mut u.part) {
            (g.guide, g.listening) = (guide, false);
        }
        if !member {
            self.guide_state(|s| s.members.push(id));
        }
        self.world.units.guest_note(format!("follow {id} {guide}"));
    }

    /// The guest looks for its place by the guide's target tile (guests visit 10.1): the tile
    /// itself, or the first of the tiles beside it from a drawn one (the original never tries the
    /// ones before it, #315) that is on the zoo grounds and has a path.
    fn follow_tile(&mut self, target: usize, itself: bool) -> Option<()> {
        if itself {
            return self.path_to(target, false).then_some(());
        }
        let beside: Vec<usize> = (0..8)
            .filter_map(|d| self.world.neighbour(target, d))
            .collect();
        let start = self.ctx.rng.pick(beside.len() as u32) as usize;
        let tries = (start..beside.len())
            .chain([0])
            .filter_map(|i| beside.get(i).copied());
        let tries: Vec<usize> = tries
            .filter(|&t| self.world.exhibits().region(t) == Some(GROUNDS))
            .collect();
        tries
            .into_iter()
            .find(|&t| self.path_to(t, false))
            .map(|_| ())
    }

    /// 11.7, complete: a guest that arrives while its guide talks faces it and gets the bonus of
    /// the viewing area it looks at, once. A guest whose guide is gone or picked up is released.
    pub(in crate::goal) fn follow_complete(&mut self) {
        let Part::Guest(g) = &self.unit.part else {
            return;
        };
        let (guide, listening, va) = (g.guide, g.listening, g.va);
        let talking = (self.world.units.get(guide)).filter(|u| u.kind == Kind::Guide && !u.carried);
        let Some(talking) = talking.map(|u| u.goal.record.task == TALK && !u.goal.record.done)
        else {
            return self.reset();
        };
        if listening || !talking {
            return;
        }
        let (me, there) = (
            self.tile(),
            self.world
                .find(guide)
                .and_then(|a| view::tile_of(self.world, a)),
        );
        let width = self.world.zoo.width as i32;
        if let (Some(me), Some(there)) = (me, there)
            && let Some(face) = nearest_direction(
                (there as i32 % width) - (me as i32 % width),
                (there as i32 / width) - (me as i32 / width),
            )
            && let Some(c) = self.world.zoo.entities[self.at].common.as_ref()
        {
            let (point, old) = ((c.x, c.y, c.z), c.rotation);
            self.world.place_unit(self.at, point, face);
            if face != old {
                let id = self.unit.id;
                self.world.units.note(format!("face {id} {face}"));
            }
        }
        let bonus = (me.zip(va)).and_then(|(t, k)| view::sight(self.world, self.ctx.cat, t, k));
        if let Part::Guest(g) = &mut self.unit.part {
            g.pending[HAPPINESS] =
                g.pending[HAPPINESS].saturating_add(bonus.map_or(0, |s| s.bonus));
            g.listening = true;
        }
    }

    /// 10.3: any reset takes a guest out of its group.
    pub(in crate::goal) fn unfollow(&mut self) {
        let id = self.unit.id;
        let Part::Guest(g) = &mut self.unit.part else {
            return;
        };
        let guide = std::mem::take(&mut g.guide);
        g.listening = false;
        if guide == 0 {
            return;
        }
        if let Some(Part::Staff(s)) = self.world.units.get_mut(guide).map(|u| &mut u.part) {
            s.members.retain(|&m| m != id);
        }
        self.world.units.guest_note(format!("follow {id} 0"));
    }
}

#[cfg(test)]
mod tests;
