//! The animal's trips to and from a show tank (shows 21.2 to 21.4): the ladder rung that sends
//! it, the go-to-show goal (task 133) and the return goal (task 134), and the crossing of the
//! show portal they share. The portal itself is [`crate::show::portal`].

use super::{AVOID, CHASE, Cx, GO_TO_SHOW, RETURN_FROM_SHOW};
use crate::animal::home::home;
use crate::behaviour::Target;
use crate::exhibit::rating::Likes;
use crate::place::bulldozer::type_of;
use crate::show::{abort_show_for, event, exhibit_of, in_show_now, info_of, portal};
use crate::sim::{Event, SHOW_CHANNEL, UNIT_CHANNEL, WORLD_CHANNEL};
use crate::staff::ladder::edge;
use crate::unit::Part;
use crate::unit::movement::direction;
use crate::unit::walk;

/// The messages: the animal has trouble getting to the show portal, and the same for an animal
/// held by a keeper.
const TROUBLE: i32 = 11004;
const HELD: i32 = 11005;

/// The building tasks that keep a call or a return waiting (the building chain, task 104 to 106).
fn in_building(task: u8) -> bool {
    (104..=106).contains(&task)
}

impl Cx<'_, '_> {
    /// 10.1 point 3 (shows 21.2): an animal on its way to a show or home, or fleeing, ends the
    /// ladder; a called one starts the go-to-show task, one sent home (or one that stands in a
    /// show tank with no show of its own) the return task, unless it is in a building task. True
    /// when the ladder ends here.
    pub(super) fn animal_show_rung(&mut self, task: u8) -> bool {
        let playing = matches!(&self.unit.part, Part::Animal(a) if a.playing);
        if [CHASE, AVOID, GO_TO_SHOW, RETURN_FROM_SHOW].contains(&task)
            || self.unit.frightened
            || playing
        {
            return true;
        }
        let busy = in_building(task);
        if self.unit.called && !busy {
            self.set_task(GO_TO_SHOW, 1, 0, None);
            return true;
        }
        if (self.unit.back || self.stray()) && !busy {
            self.set_task(RETURN_FROM_SHOW, 1, 0, None);
            return true;
        }
        false
    }

    /// The animal stands in a show tank that has a show neighbour, and is not in a show.
    fn stray(&self) -> bool {
        let Some(h) = self.exhibit() else {
            return false;
        };
        let near = self.world.exhibits().get(h);
        let near = near.is_some_and(|e| e.tank.is_some() && !e.show_near.is_empty());
        near && info_of(self.world, h).is_some()
            && !in_show_now(self.world, self.ctx.cat, self.unit)
    }

    /// Check done of both tasks (shows 21.3): done when the animal reached the tile it swims to
    /// (its destination), once the decide has set its target.
    pub(super) fn show_check_done(&mut self) {
        if self.record().target.tile.is_none() {
            return;
        }
        let dest = self.unit.destination;
        let tile =
            dest.and_then(|(x, y, _)| self.world.tile_index(x.div_euclid(64), y.div_euclid(64)));
        if self.reached_at(tile, dest) {
            self.record().done = true;
        }
    }

    /// The target tile of the task is the tile `t` (the setter of the original, flag 1).
    fn aim_at(&mut self, t: Option<usize>) {
        let world = &*self.world;
        self.unit.goal.record.target.set_tile(world, t);
    }

    /// Decide of the go-to-show task (shows 21.3).
    pub(super) fn go_show_decide(&mut self) {
        if self.record().target.tile.is_some() {
            return;
        }
        let s = exhibit_of(self.world, self.unit.show);
        if let (Some(s), Some(h)) = (s, self.exhibit()) {
            if h == s {
                let gate = crate::place::unit::gate_inside(self.world, s);
                self.aim_at(gate);
                self.record().done = true;
                return;
            }
            if self.swim_to_portal(h, s) {
                return;
            }
        }
        self.unit.called = false;
        if let Some(s) = s {
            self.show_trouble(s, self.unit.held);
        }
        self.abort_show();
        self.reset();
    }

    /// The path to the tile of `h`'s portal towards `s`, at the level of the water less two, and
    /// the walk; the target tile is the one across the wall. False when any step fails.
    fn swim_to_portal(&mut self, h: u32, s: u32) -> bool {
        let Some((t, across)) = portal(self.world, h, s) else {
            return false;
        };
        let Some(level) = self.swim_level(h) else {
            return false;
        };
        if !self.path_to_level(t, false, Some(level)) || !self.start_walk(true, false, true) {
            return false;
        }
        self.aim_at(Some(across));
        true
    }

    /// The level the animals swim at in tank `x`: its floor plus its water level, less two.
    fn swim_level(&self, x: u32) -> Option<i32> {
        Some(self.world.floor(x)? + self.world.tank(x)?.steps().1 - 2)
    }

    /// The message about an animal that cannot reach exhibit `x`.
    fn show_trouble(&mut self, x: u32, held: bool) {
        let names = vec![self.name_of(self.unit.id), self.exhibit_name(x)];
        let message = if held { HELD } else { TROUBLE };
        self.world.units.post_named(message, self.unit.id, names);
    }

    /// `abort_show` (shows 21.6): the show of the animal's show id is aborted. The unit goes back
    /// into the table while the show lets go of its animals, itself among them.
    pub(crate) fn abort_show(&mut self) {
        let show = self.unit.show;
        abort_show_for(self.world, self.ctx.cat, self.unit, show);
    }

    /// The crossing, shared by the arrival and the return (shows 21.3): an animal at the portal is
    /// put across the wall, at the edge of the tile it aimed at and the height of the water
    /// there, facing into that tank. The exhibit of the tile it aimed at is given back.
    fn cross(&mut self) -> Option<u32> {
        let across = self.record().target.tile;
        let world = &*self.world;
        let there = across.and_then(|t| crate::animal::food::exhibit_of(world, t));
        let (here, h) = (self.tile(), self.exhibit());
        let linked = |s: u32| {
            let e = h.and_then(|h| world.exhibits().get(h));
            Some(s) != h && e.is_some_and(|e| e.show_near.contains(&s))
        };
        if let (Some(t), Some(s), Some(here)) = (across, there, here)
            && linked(s)
            && let Some(level) = world
                .tank(s)
                .filter(|k| k.level > 0)
                .and(self.swim_level(s))
        {
            let d = direction(world, t, here);
            if d >= 0 {
                let (x, y) = edge(world, t, d as usize);
                let facing = (d - 4) & 7;
                self.world
                    .place_unit(self.at, (x, y, 16 * level), facing as u32);
            }
        }
        there
    }

    /// Complete of the go-to-show task (shows 21.3): across the portal the animal reports in, and
    /// goes for a random walk.
    pub(super) fn go_show_complete(&mut self) {
        let there = self.cross();
        self.unit.called = false;
        if self.exhibit() == there {
            let (id, show) = (self.unit.id, u32::from(self.unit.show));
            for (target, data, channel) in [
                (id, 0, UNIT_CHANNEL),
                (0, 0, WORLD_CHANNEL),
                (show, 0, SHOW_CHANNEL),
            ] {
                let delay = 1;
                let sender_channel = UNIT_CHANNEL;
                let event = Event {
                    sender: id,
                    target,
                    data,
                    extra: 0,
                    kind: event::ARRIVED,
                    delay,
                    sender_channel,
                    channel,
                };
                self.world.units.posts.push(event);
            }
        }
        self.reset();
        self.unit.target = Target::Surface;
        let walked = walk::random_walk(self.unit, self.world, self.ctx.cat, self.ctx.rng, self.at);
        if let Some(to) = self.unit.sent.take() {
            return self.send_to(to);
        }
        if walked {
            self.start_walk(false, false, true);
        }
    }

    /// The home exhibit of the animal: the one its exhibit gives it (tank fauna 12.3).
    fn home(&self) -> Option<u32> {
        let x = self.exhibit()?;
        let ty = type_of(self.ctx.cat, &self.world.zoo.entities[self.at])?;
        home(self.world, self.ctx.cat, x, &Likes::of(ty))
    }

    /// Whether `h` is a tank with a show info.
    fn show_tank(&self, h: u32) -> bool {
        self.world.tank(h).is_some() && info_of(self.world, h).is_some()
    }

    /// Decide of the return task (shows 21.4).
    pub(super) fn return_decide(&mut self) {
        if self.record().target.tile.is_some() {
            return;
        }
        let (home, here) = (self.home(), self.exhibit());
        let already = match (home, here) {
            (None, Some(h)) => Some(h),
            (Some(home), Some(h)) if h == home || !self.show_tank(h) => Some(home),
            (Some(home), Some(h)) if self.swim_to_portal(h, home) => return,
            _ => None,
        };
        match already {
            Some(x) => {
                let gate = crate::place::unit::gate_inside(self.world, x);
                self.aim_at(gate);
                self.record().done = true;
            }
            None => {
                self.unit.back = false;
                if let Some(home) = home {
                    self.show_trouble(home, false);
                }
                self.reset();
            }
        }
    }

    /// Complete of the return task (shows 21.4): the crossing, then the animal is home.
    pub(super) fn return_complete(&mut self) {
        self.cross();
        self.unit.back = false;
        self.reset();
    }
}
