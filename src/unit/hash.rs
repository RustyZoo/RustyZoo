//! The state hash of the unit table.

use super::*;

impl Units {
    /// A hash of the table (FNV-1a, in list order) for the state hash of a run. It walks a chosen
    /// projection, never the `Debug` text: the positions are in the entities of the zoo, here go the
    /// movement, the goal, the animation and the needs. A new field moves the hash only when it is
    /// added here.
    pub fn hash(&self) -> u64 {
        let mut h = Fnv(0xcbf2_9ce4_8422_2325);
        h.word(self.period as u32);
        h.word(self.service_interval);
        for v in self.finder.state() {
            h.word(v as u32);
            h.word((v >> 32) as u32);
        }
        for (&id, &level) in &self.upgrade_index {
            h.word(id);
            h.word(level);
        }
        for &(id, likes, age) in &self.reactions {
            h.word(id);
            h.word(u32::from(likes));
            h.word(age);
        }
        for u in &self.list {
            h.word(u.id);
            h.word(u.kind as u32);
            h.word(u32::from(u.moving));
            h.word(u.speed);
            h.word(u32::from(u.anim_speed));
            for v in u.destination.iter().flat_map(|&(x, y, z)| [x, y, z]) {
                h.word(v as u32);
            }
            h.word(u32::from(u.destination.is_some()));
            h.word(u.path.len() as u32);
            for &t in &u.path {
                h.word(t as u32);
            }
            h.word(u.levels.len() as u32);
            for &l in &u.levels {
                h.word(l as u32);
            }
            h.word(u32::from(u.tank) | (u.target as u32) << 8);
            h.word(u.vertical as u32);
            h.word(u.travel as u32);
            h.word(u32::from(u.walking) | u32::from(u.flip) << 1 | (u.band as u32) << 2);
            h.word(u.gate.map_or(0, |g| g.wrapping_add(1)));
            h.word(u32::from(u.through_building) | u32::from(u.aborted) << 1);
            h.bytes(u.animation.name.as_bytes());
            h.word(u.animation.frame);
            let r = &u.goal.record;
            h.word(u32::from(r.task) | u32::from(r.mode) << 8 | u32::from(r.job) << 16);
            h.word(r.target.id);
            h.word(u.status_timer as u32);
            h.word(u.wait as u32);
            h.word(
                u32::from(u.carried)
                    | u32::from(u.repositioning) << 1
                    | u32::from(u.started) << 2
                    | u32::from(u.typed) << 3
                    | u32::from(u.hidden) << 4
                    | u32::from(u.held) << 5,
            );
            match &u.part {
                Part::Animal(a) => {
                    for v in [a.happiness, a.hunger, a.health, a.energy, a.dung, a.life] {
                        h.word(v as u32);
                    }
                    h.word(
                        u32::from(a.egg)
                            | u32::from(a.dying) << 1
                            | u32::from(a.escaped) << 2
                            | u32::from(a.rampaging) << 3
                            | u32::from(a.tranq) << 4,
                    );
                    if a.claim != 0 {
                        h.word(a.claim);
                    }
                }
                Part::Staff(s) => {
                    h.word(s.work as u32);
                    h.word(s.chase as u32);
                    h.word(s.assigned.len() as u32);
                    s.assigned.iter().for_each(|&e| h.word(e));
                    h.word(s.off.iter().fold(0, |m, &off| m << 1 | u32::from(off)));
                    h.word(s.lost_tiles.len() as u32);
                    s.lost_tiles.iter().for_each(|&t| h.word(t as u32));
                    h.word(s.lost_animals.len() as u32);
                    s.lost_animals.iter().for_each(|&a| h.word(a));
                    h.word(s.food_kind as u32);
                    h.word(s.target.map_or(u32::MAX, |t| t as u32));
                    h.word(s.members.len() as u32);
                    s.members.iter().for_each(|&m| h.word(m));
                    h.word(s.crowd as u32);
                    h.word(u32::from(s.leg));
                    h.word(s.task);
                    h.word(s.clear as u32);
                    if s.captured != 0 {
                        h.word(s.captured);
                    }
                    if u.kind == Kind::Heli {
                        let t = &s.heli;
                        h.word(t.base);
                        h.word(u32::from(t.flying) | (t.dir as u32) << 1);
                        h.word(t.level().map_or(u32::MAX, |l| l as u32));
                    }
                }
                Part::Guest(g) => {
                    g.values.iter().for_each(|&v| h.word(v as u32));
                    g.items
                        .iter()
                        .for_each(|i| h.word(i.map_or(u32::MAX, |i| i as u32)));
                    h.word(g.building);
                    h.word(u32::from(g.leave));
                }
            }
            h.word(u.events.len() as u32);
            for e in &u.events {
                for v in [e.sender, e.target, e.data] {
                    h.word(v);
                }
                for v in [
                    e.extra,
                    e.kind,
                    e.delay,
                    e.sender_channel.into(),
                    e.channel.into(),
                ] {
                    h.word(v.into());
                }
            }
        }
        h.0
    }
}

struct Fnv(u64);

impl Fnv {
    fn bytes(&mut self, bytes: &[u8]) {
        for &b in bytes {
            self.0 = (self.0 ^ u64::from(b)).wrapping_mul(0x0000_0100_0000_01b3);
        }
        self.bytes_end();
    }

    /// A separator, so that two names do not run into each other.
    fn bytes_end(&mut self) {
        self.0 = (self.0 ^ 0xff).wrapping_mul(0x0000_0100_0000_01b3);
    }

    fn word(&mut self, v: u32) {
        for b in v.to_le_bytes() {
            self.0 = (self.0 ^ u64::from(b)).wrapping_mul(0x0000_0100_0000_01b3);
        }
    }
}
