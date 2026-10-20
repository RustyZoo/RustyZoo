//! Fence wear (placing 7.4): every fence piece carries a strength, a life, an interval and a
//! countdown. A piece made healthy draws its interval from the game's generator; on every step
//! the countdown of a piece that wears loses the step, and at 0 the piece loses one life and
//! some strength, becoming decayed and then broken. Tank walls, the zoo wall and gates never
//! wear with time.

use super::World;
use crate::build::number;
use crate::config::{Config, to_int};
use crate::entity::{Catalogue, EntityType};
use crate::exhibit::GROUNDS;
use crate::place::bulldozer::type_of;
use crate::place::{LIFE, STRENGTH};
use crate::sim::TASK_CHANNEL;
use crate::vfs::Vfs;
use crate::zoo::Event;

/// The events a worn piece posts (placing 7.4): decayed, and broken (an urgent task).
const DECAYED: u16 = 10033;
const BROKEN: u16 = 10034;
/// The plain entity post's own channel.
const PIECE_CHANNEL: u8 = 0x45;

/// The bounds of a wear interval in milliseconds (`[Decay]` of `fences/fence.ai`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Decay {
    pub min: u32,
    pub max: u32,
}

impl Default for Decay {
    /// The values of the shipped file, for a file or a key that is missing.
    fn default() -> Decay {
        Decay {
            min: 480_000,
            max: 1_200_000,
        }
    }
}

impl Decay {
    /// The bounds of the install; a negative value reads as 0.
    pub fn load(vfs: &Vfs) -> Decay {
        let file = vfs
            .read("fences/fence.ai")
            .ok()
            .and_then(|b| Config::parse(&b).ok());
        let key = |name: &str, fallback: u32| {
            (file.as_ref())
                .and_then(|c| c.section("Decay")?.get(name))
                .and_then(to_int)
                .map_or(fallback, |v| v.max(0) as u32)
        };
        let Decay { min, max } = Decay::default();
        Decay {
            min: key("minDecayTime", min),
            max: key("maxDecayTime", max),
        }
    }

    /// `min + round(r * (max - min))` for the 15 bit draw `rand` (`r` is `rand / 32767`).
    fn interval(self, rand: u32) -> u32 {
        let (min, max) = (f64::from(self.min), f64::from(self.max));
        let r = f64::from(rand.min(0x7fff)) / 32767.0;
        (min + (r * (max - min)).round()).clamp(0.0, f64::from(u32::MAX)) as u32
    }
}

impl World {
    /// The bounds of the wear interval a piece made healthy draws from.
    pub fn set_decay(&mut self, decay: Decay) {
        self.decay = decay;
    }

    /// The type of the piece at `at` when it is one that wears with time (placing 7.4 items 1, 2
    /// and the types that can break): a fence piece of the zoo, not a zoo wall, a zoo gate, a tank
    /// wall or a gate, with a `cStrength` other than 500 and not `cIndestructible`. The test of
    /// the surroundings (item 4) is not made (placing 12.6).
    fn wearing<'a>(&self, cat: &'a Catalogue, at: usize) -> Option<&'a EntityType> {
        let e = &self.zoo.entities[at];
        e.common.as_ref()?.fence?;
        let s = self.shapes[at]?;
        let (tile, _) = self.slot(at)?;
        let ty = type_of(cat, e).filter(|_| self.in_zoo(tile))?;
        let gate = e.subtype.starts_with('g') || s.gate;
        let sturdy = s.wall || s.tank.wall || gate;
        let strength = number(ty, "cStrength").unwrap_or(STRENGTH);
        let indestructible = number(ty, "cIndestructible").unwrap_or(0.0) != 0.0;
        (!sturdy && strength != STRENGTH && !indestructible).then_some(ty)
    }

    /// Whether the piece at `at` wears with time.
    pub fn wears(&self, cat: &Catalogue, at: usize) -> bool {
        self.wearing(cat, at).is_some()
    }

    /// Whether the piece at `at` is decayed or broken (staff 8.7): it wears with time and its life is at
    /// most the `cDecayedLife` of its type.
    pub fn decayed(&self, cat: &Catalogue, at: usize) -> bool {
        let life = (self.zoo.entities[at].common.as_ref()).and_then(|c| Some(c.fence?.life));
        let limit = |ty: &EntityType| number(ty, "cDecayedLife").unwrap_or(0.0) as i64;
        let ty = self.wearing(cat, at);
        life.zip(ty)
            .is_some_and(|(l, ty)| i64::from(l) <= limit(ty))
    }

    /// Makes the fence piece with this id healthy (placing 7.4): the strength and life of its
    /// type, a new interval drawn with `rand`, and the countdown starting equal to it. Returns the
    /// interval; `None`, drawing nothing, for an entity that is no fence piece or has no type.
    pub fn heal(&mut self, cat: &Catalogue, id: u32, rand: &mut dyn FnMut() -> u32) -> Option<u32> {
        let at = self.find(id)?;
        let ty = type_of(cat, &self.zoo.entities[at])?;
        let strength = number(ty, "cStrength").unwrap_or(STRENGTH) as u32;
        let life = number(ty, "cLife").unwrap_or(LIFE) as i32;
        let fence = self.zoo.entities[at].common.as_mut()?.fence.as_mut()?;
        let interval = self.decay.interval(rand());
        (fence.strength, fence.life, fence.timers) = (strength, life, [interval; 2]);
        self.dirty = true;
        Some(interval)
    }

    /// One step of `ms` milliseconds of wear of the piece at `at` (placing 7.4), run at its place in
    /// the entity loop: a piece that wears loses `ms` from its countdown; at 0 or below it loses
    /// one life and `cDecayDelta` strength (not below 1), restarts the countdown at its interval
    /// and, when its life is below 1, breaks: life and strength 0. A line tells the piece when it
    /// wears; the events 10033 and 10034 it posts are returned. The break sound and the exhibits'
    /// escape check wait for the animal milestone.
    pub fn wear(
        &mut self,
        cat: &Catalogue,
        at: usize,
        ms: u32,
        lines: &mut Vec<String>,
    ) -> Vec<Event> {
        let Some(ty) = self.wearing(cat, at) else {
            return Vec::new();
        };
        let decayed = number(ty, "cDecayedLife").unwrap_or(0.0) as i64;
        let delta = number(ty, "cDecayDelta").unwrap_or(8.0) as u32;
        let electrified = number(ty, "cIsElectrified").unwrap_or(0.0) != 0.0;
        let Some(c) = self.zoo.entities[at].common.as_mut() else {
            return Vec::new();
        };
        let id = c.id;
        let Some(f) = c.fence.as_mut() else {
            return Vec::new();
        };
        // A broken piece does not wear (its life below 1, as a signed number).
        if f.life < 1 {
            return Vec::new();
        }
        if f.timers[1] > ms {
            f.timers[1] -= ms;
            return Vec::new();
        }
        f.timers[1] = f.timers[0];
        f.life -= 1;
        f.strength = f.strength.saturating_sub(delta).max(1);
        let mut kinds = Vec::new();
        let mut state = "";
        // The piece's own life is below 1, never 500: it can deteriorate.
        if f.life < 1 {
            f.strength = 0;
            kinds.push(BROKEN);
            state = " broken";
        }
        if i64::from(f.life) == decayed {
            kinds.push(if electrified { BROKEN } else { DECAYED });
            state = if f.life < 1 { state } else { " decayed" };
        }
        lines.push(format!(
            "fence {id} life {} strength {}{state}",
            f.life, f.strength
        ));
        self.dirty = true;
        let tile = self.worker_tile(at);
        kinds
            .into_iter()
            .map(|kind| piece_event(id, kind, tile))
            .collect()
    }

    /// The events a zoo just loaded posts for its fence pieces (placing 7.4 "The events"), in
    /// entity list order: a piece with life below 1 that can break is broken again (10034), else one at or below
    /// the `cDecayedLife` of its type is decayed (10033, 10034 when electrified).
    pub fn loaded_events(&self, cat: &Catalogue) -> Vec<Event> {
        let mut events = Vec::new();
        for (at, e) in self.zoo.entities.iter().enumerate() {
            let Some((c, f)) = e.common.as_ref().and_then(|c| Some((c, c.fence?))) else {
                continue;
            };
            let ty = type_of(cat, e);
            let key = |name| ty.and_then(|t| number(t, name)).unwrap_or(0.0);
            let kind = if f.life < 1 {
                if !ty.is_some_and(deteriorates) {
                    continue;
                }
                BROKEN
            } else if i64::from(f.life) <= key("cDecayedLife") as i64 {
                [DECAYED, BROKEN][usize::from(key("cIsElectrified") != 0.0)]
            } else {
                continue;
            };
            events.push(piece_event(c.id, kind, self.worker_tile(at)));
        }
        events
    }

    /// Whether tile `t` is open (placing 7.4 item 4): one of its four quadrants is empty or holds
    /// a `walkable` object.
    fn open(&self, t: usize) -> bool {
        (0..4).any(|q| {
            self.quadrant(t, q)
                .is_none_or(|e| self.shape(e).is_some_and(|s| s.walkable.0))
        })
    }

    /// The tile a worker stands on to mend the piece at `at` (placing 7.4 "The events"): its tile or
    /// the one across, the open one on the zoo grounds first.
    fn worker_tile(&self, at: usize) -> u32 {
        let Some((t, d)) = self.slot(at) else {
            return 0;
        };
        let n = self.neighbour(t, d);
        let grounds = |t: usize| self.exhibits.region(t) == Some(GROUNDS);
        let tiles = [Some(t), n];
        let pick = |f: &dyn Fn(usize) -> bool| tiles.into_iter().flatten().find(|&t| f(t));
        let found = pick(&|t| grounds(t) && self.open(t)).or_else(|| pick(&|t| self.open(t)));
        found.unwrap_or(t) as u32
    }
}

/// Whether a piece of this type can break (placing 7.4): an exhibit fence that is no zoo wall,
/// zoo gate, tank wall or `cIndestructible` type.
fn deteriorates(ty: &EntityType) -> bool {
    let habitat = (ty.config.as_deref()).is_some_and(|c| c.section("HabitatFence").is_some());
    let tank = (ty.config.as_deref()).is_some_and(|c| c.section("TankFence").is_some());
    let member = |name| crate::render::member(ty, name);
    habitat
        && !tank
        && !member("zoowall")
        && !member("zoogate")
        && number(ty, "cIndestructible").unwrap_or(0.0) == 0.0
}

/// The plain entity post of an entity (a fence piece, a guest, a trash can) to the task pool, with
/// the worker tile as data.
pub(crate) fn piece_event(id: u32, kind: u16, tile: u32) -> Event {
    Event {
        sender: id,
        target: 0,
        data: tile,
        extra: 0,
        kind,
        delay: 1,
        sender_channel: PIECE_CHANNEL,
        channel: TASK_CHANNEL,
    }
}
