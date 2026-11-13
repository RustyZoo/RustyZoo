//! Fence wear (placing 7.4): every fence piece carries a strength, a life, an interval and a
//! countdown. A piece made healthy draws its interval from the game's generator; on every step
//! the countdown of a piece that wears loses the step, and at 0 the piece loses one life and
//! some strength, becoming decayed and then broken. Tank walls, the zoo wall and gates never
//! wear with time.

use super::World;
use crate::animal::Class;
use crate::build::number;
use crate::config::{Config, to_int};
use crate::entity::type_of;
use crate::entity::{Catalogue, EntityType};
use crate::exhibit::GROUNDS;
use crate::place::{LIFE, STRENGTH};
use crate::sim::TASK_CHANNEL;
use crate::single;
use crate::vfs::Vfs;
use crate::zoo::{Event, Fence};

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

    /// `min + round(f32(f32(n * (2^-15 + 2^-30)) * (max - min)))` for the 15 bit draw `n`, halves
    /// to even (placing 7.4).
    fn interval(self, rand: u32) -> u32 {
        let (min, max) = (f64::from(self.min), f64::from(self.max));
        let r = single(f64::from(rand.min(0x7fff)) * (1.0 / 32768.0 + 1.0 / 1_073_741_824.0));
        (min + single(r * (max - min)).round_ties_even()).clamp(0.0, f64::from(u32::MAX)) as u32
    }
}

impl World {
    /// The numbers of the install a loaded world needs before its units are created: the ini
    /// settings, the `water` of the terrain types, the animal class (with the `reset_interval`
    /// of the AI) and the wear interval. The game start and the static picture share it.
    pub fn set_install(&mut self, vfs: &Vfs, water: Vec<bool>, reset_interval: i32) {
        self.units.set_ini(vfs.ini());
        self.set_water(water);
        self.units.class = Class {
            reset_interval,
            ..Class::load(vfs)
        };
        self.set_decay(Decay::load(vfs));
    }

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
    pub(crate) fn wears(&self, cat: &Catalogue, at: usize) -> bool {
        self.wearing(cat, at).is_some()
    }

    /// Whether the piece at `at` is decayed or broken (staff 8.7): it wears with time and its life is at
    /// most the `cDecayedLife` of its type.
    pub(crate) fn decayed(&self, cat: &Catalogue, at: usize) -> bool {
        let life = (self.zoo.entities[at].common.as_ref()).and_then(|c| Some(c.fence?.life));
        let limit = |ty: &EntityType| number(ty, "cDecayedLife").unwrap_or(0.0) as i64;
        let ty = self.wearing(cat, at);
        life.zip(ty)
            .is_some_and(|(l, ty)| i64::from(l) <= limit(ty))
    }

    /// The life of the fence piece at `at`, if it carries one.
    pub(crate) fn fence_life(&self, at: usize) -> Option<i32> {
        (self.zoo.entities[at].common.as_ref()).and_then(|c| Some(c.fence?.life))
    }

    /// Whether the entity at `at` is a fence piece or a tank wall (the class of 6.7 of the zoo format).
    pub(crate) fn is_fence(&self, at: usize) -> bool {
        matches!(&*self.zoo.entities[at].class, "fences" | "tankwall")
    }

    /// Makes the fence piece with this id healthy (placing 7.4): the strength and life of its
    /// type, a new interval drawn with `rand`, and the countdown starting equal to it. It is a
    /// state change of the fence (`fence_changed`: the escape check flag beside a bashing species).
    /// Returns the interval; `None`, drawing nothing, for an entity that is no fence piece or has
    /// no type.
    pub(crate) fn heal(
        &mut self,
        cat: &Catalogue,
        id: u32,
        rand: &mut dyn FnMut() -> u32,
    ) -> Option<u32> {
        let at = self.find(id)?;
        let ty = type_of(cat, &self.zoo.entities[at])?;
        let strength = number(ty, "cStrength").unwrap_or(STRENGTH) as u32;
        let life = number(ty, "cLife").unwrap_or(LIFE) as i32;
        let piece = self.is_fence(at);
        let common = self.zoo.entities[at].common.as_mut()?;
        // A piece of a file older than version 55 has no condition yet.
        let fence = match (&mut common.fence, piece) {
            (Some(f), _) => f,
            (none, true) => none.insert(Fence {
                strength,
                life,
                timers: [0; 2],
                portal: false,
                open: false,
            }),
            _ => return None,
        };
        let interval = self.decay.interval(rand());
        (fence.strength, fence.life, fence.timers) = (strength, life, [interval; 2]);
        self.dirty = true;
        // Every piece made healthy, decayed or not, changes state (habitat rating 12).
        self.fence_changed(at, false);
        Some(interval)
    }

    /// The state of the fence piece at `at` changed (habitat rating 12): the exhibits on its two
    /// sides get the escape check flag (5.8) when it is broken, or when a species of their
    /// surrounding list bashes (`cBashStrength` above 0).
    pub(crate) fn fence_changed(&mut self, at: usize, broken: bool) {
        for t in self.rated_tiles(at) {
            let x = self.exhibits.region(t);
            if let Some(e) = x.and_then(|x| self.exhibits.get_mut(x))
                && (broken || e.rating.bash)
            {
                e.rating.escape = true;
            }
        }
    }

    /// One step of `ms` milliseconds of wear of the piece at `at` (placing 7.4), run at its place in
    /// the entity loop: a piece that wears loses `ms` from its countdown; at 0 or below it loses
    /// one life and `cDecayDelta` strength (not below 1), restarts the countdown at its interval
    /// and, when its life is below 1, breaks: life and strength 0. A line tells the piece when it
    /// wears; the events 10033 and 10034 it posts are returned. A piece that becomes decayed or
    /// broken sets the escape check flags of 12. A broken piece plays its break sound.
    pub(crate) fn wear(
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
        // Only an exhibit fence breaks; any other type keeps wearing below life 1.
        let breaks = deteriorates(ty);
        let Some(c) = self.zoo.entities[at].common.as_mut() else {
            return Vec::new();
        };
        let id = c.id;
        let Some(f) = c.fence.as_mut() else {
            return Vec::new();
        };
        // A broken piece does not wear (its life below 1, as a signed number).
        if breaks && f.life < 1 {
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
        // The piece's own life is below 1, never 500: when the type can deteriorate, it breaks.
        if breaks && f.life < 1 {
            f.strength = 0;
            kinds.push(BROKEN);
            state = " broken";
        }
        if i64::from(f.life) == decayed {
            kinds.push(if electrified { BROKEN } else { DECAYED });
            state = if breaks && f.life < 1 {
                state
            } else {
                " decayed"
            };
        }
        lines.push(format!(
            "fence {id} life {} strength {}{state}",
            f.life, f.strength
        ));
        let broken = breaks && f.life < 1;
        self.dirty = true;
        if broken {
            // The break sound plays once, at the piece (placing 7.4 "Breaking").
            let voice = format!("{id}/cBreakSound");
            let heard = crate::unit::gate::heard_of(ty, "cBreakSound", "cBreakSoundAtten", &voice);
            self.units.push_sound(id, heard);
        }
        if !state.is_empty() {
            self.fence_changed(at, broken);
        }
        let tile = self.worker_tile(at);
        kinds
            .into_iter()
            .map(|kind| piece_event(id, kind, tile))
            .collect()
    }

    /// The type of the piece at `at` when the break routine takes it (placing 7.4 "Breaking"): a
    /// piece with a condition whose life is not 500 and whose type can deteriorate. There is no
    /// test of the zoo, a gate subtype or the strength (those are the wear step's).
    pub(crate) fn breakable<'a>(&self, cat: &'a Catalogue, at: usize) -> Option<&'a EntityType> {
        let e = &self.zoo.entities[at];
        let f = e.common.as_ref()?.fence?;
        type_of(cat, e).filter(|&ty| f.life != 500 && deteriorates(ty))
    }

    /// Whether the piece at `at` starts an animation at a random time: a fence piece that is no
    /// gate (sprite drawing 3.6).
    pub(crate) fn loops(&self, at: usize) -> bool {
        !self.zoo.entities[at].subtype.starts_with('g')
    }

    /// Breaks the piece at `at` outside the wear step (an animal breaking out, the break cheat)
    /// with the break routine's own test ([`World::breakable`]) and a life of 1 or more: life and
    /// strength 0, the break sound, the exhibits beside it marked for the escape check, one event
    /// 10034 and a line. Whether it broke just now.
    pub(crate) fn break_piece(
        &mut self,
        cat: &Catalogue,
        at: usize,
        lines: &mut Vec<String>,
        events: &mut Vec<Event>,
    ) -> bool {
        let Some(ty) = self.breakable(cat, at) else {
            return false;
        };
        let Some(c) = self.zoo.entities[at].common.as_mut() else {
            return false;
        };
        let (id, Some(f)) = (c.id, c.fence.as_mut()) else {
            return false;
        };
        if f.life < 1 {
            return false;
        }
        (f.life, f.strength) = (0, 0);
        lines.push(format!("fence {id} life 0 strength 0 broken"));
        let voice = format!("{id}/cBreakSound");
        let heard = crate::unit::gate::heard_of(ty, "cBreakSound", "cBreakSoundAtten", &voice);
        self.units.push_sound(id, heard);
        self.dirty = true;
        self.fence_changed(at, true);
        events.push(piece_event(id, BROKEN, self.worker_tile(at)));
        true
    }

    /// The `fences_break` cheat (cheats 4.2 C): every piece that can deteriorate is broken with
    /// [`World::break_piece`]. A piece already broken stays so. Returns the events, and the id of
    /// each piece that broke just now and starts an animation.
    pub(crate) fn break_fences(
        &mut self,
        cat: &Catalogue,
        lines: &mut Vec<String>,
    ) -> (Vec<Event>, Vec<u32>) {
        let (mut events, mut broke) = (Vec::new(), Vec::new());
        for at in 0..self.zoo.entities.len() {
            if self.break_piece(cat, at, lines, &mut events)
                && self.loops(at)
                && let Some(c) = &self.zoo.entities[at].common
            {
                broke.push(c.id);
            }
        }
        (events, broke)
    }

    /// The pieces the `$` key may pick (input 4.8), in entity list order: an exhibit fence piece
    /// (habitat or tank fence) that is no zoo wall and no zoo gate, on a tile inside the zoo that is
    /// not water, whose neighbour across its side is not water and holds no zoo wall piece in the
    /// opposite slot.
    pub(crate) fn dollar_candidates(&self) -> Vec<usize> {
        (0..self.zoo.entities.len())
            .filter(|&at| {
                let fence = self.shapes[at].is_some_and(|s| s.fence && !s.wall && !s.gate);
                let Some((t, d)) = self.slot_of(at).filter(|_| fence) else {
                    return false;
                };
                let n = self.neighbour(t, d);
                let beside = n.is_some_and(|n| self.is_water(n) || self.wall(n, (d + 4) % 8));
                self.in_zoo(t) && !self.is_water(t) && !beside
            })
            .collect()
    }

    /// Halves the water purity of the tank of the tank wall at `at` under the `$` key (input 4.8):
    /// the tank of its own tile only (none: nothing changes). Whether the piece is a tank wall.
    pub(crate) fn halve_tank_water(&mut self, at: usize) -> bool {
        let wall = self.shapes[at].is_some_and(|s| s.tank.wall);
        let Some((t, _)) = self.slot_of(at).filter(|_| wall) else {
            return false;
        };
        if let Some(tank) = self.tank_at(t) {
            let purity = self.tank(tank).map_or(0, |k| i64::from(k.purity));
            self.set_purity(tank, purity / 2);
        }
        true
    }

    /// The end of a load breaks again the pieces with a life below 1 that can deteriorate (placing
    /// 7.4 "Breaking"), in entity list order: life and strength 0 (a stored -1 becomes 0) and the
    /// exhibits beside it are marked for the escape check; no sound. Returns the
    /// ids of those that start an animation; they draw two start times (sprite drawing 3.6 item 4).
    pub(crate) fn break_loaded(&mut self, cat: &Catalogue) -> Vec<u32> {
        let mut ids = Vec::new();
        for at in 0..self.zoo.entities.len() {
            if self.breakable(cat, at).is_none() {
                continue;
            }
            let Some(c) = self.zoo.entities[at].common.as_mut() else {
                continue;
            };
            let (id, Some(f)) = (c.id, c.fence.as_mut()) else {
                continue;
            };
            if f.life >= 1 {
                continue;
            }
            (f.life, f.strength) = (0, 0);
            // Silent at load (placing 7.4 "The break sound" 4); the exhibits beside the piece get
            // the escape check.
            self.fence_changed(at, true);
            if self.loops(at) {
                ids.push(id);
            }
        }
        ids
    }

    /// Whether the repair cheat makes the piece at `at` healthy (placing 7.4): a condition that is
    /// broken or at or below the `cDecayedLife` of its type, with no wear test.
    pub(crate) fn repairable(&self, cat: &Catalogue, at: usize) -> bool {
        let e = &self.zoo.entities[at];
        let limit = |ty| number(ty, "cDecayedLife").unwrap_or(0.0) as i64;
        let life = self.fence_life(at);
        life.zip(type_of(cat, e))
            .is_some_and(|(l, ty)| l < 1 || i64::from(l) <= limit(ty))
    }

    /// The events a zoo just loaded posts for its fence pieces (placing 7.4 "The events"), in
    /// entity list order: a piece with life below 1 that can break is broken again (10034), else one at or below
    /// the `cDecayedLife` of its type is decayed (10033, 10034 when electrified).
    pub(crate) fn loaded_events(&self, cat: &Catalogue) -> Vec<Event> {
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
    pub(crate) fn worker_tile(&self, at: usize) -> u32 {
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
pub(crate) fn deteriorates(ty: &EntityType) -> bool {
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
