//! Fence wear (placing 7.4): every fence piece carries a strength, a life, an interval and a
//! countdown. A piece made healthy draws its interval from the game's generator; on every step
//! the countdown of a piece that wears loses the step, and at 0 the piece loses one life and
//! some strength, becoming decayed and then broken. Tank walls and the zoo wall only decay in
//! the original's data and do not take part here.

use super::World;
use crate::build::number;
use crate::config::{Config, to_int};
use crate::entity::{Catalogue, EntityType};
use crate::place::bulldozer::type_of;
use crate::place::{LIFE, STRENGTH};
use crate::vfs::Vfs;

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

    /// Makes the fence piece with this id healthy (placing 7.4): the strength and life of its
    /// type, a new interval drawn with `rand`, and the countdown starting equal to it. Returns the
    /// interval; `None`, drawing nothing, for an entity that is no fence piece or has no type.
    pub fn heal(&mut self, cat: &Catalogue, id: u32, rand: &mut dyn FnMut() -> u32) -> Option<u32> {
        let at = self.find(id)?;
        let ty = type_of(cat, &self.zoo.entities[at])?;
        let strength = number(ty, "cStrength").unwrap_or(STRENGTH) as u32;
        let life = number(ty, "cLife").unwrap_or(LIFE) as u32;
        let fence = self.zoo.entities[at].common.as_mut()?.fence.as_mut()?;
        let interval = self.decay.interval(rand());
        (fence.strength, fence.life, fence.timers) = (strength, life, [interval; 2]);
        self.dirty = true;
        Some(interval)
    }

    /// One step of `ms` milliseconds of wear (placing 7.4): each piece that wears loses `ms` from
    /// its countdown; at 0 or below it loses one life and `cDecayDelta` strength (not below 1),
    /// restarts the countdown at its interval and, when its life is below 1, breaks: life and
    /// strength 0. A line tells each wearing piece. The events 10033 and 10034, the break sound
    /// and the exhibits' escape check wait for the staff and animal milestones.
    pub fn wear(&mut self, cat: &Catalogue, ms: u32, lines: &mut Vec<String>) {
        for at in 0..self.zoo.entities.len() {
            let Some(ty) = self.wearing(cat, at) else {
                continue;
            };
            let decayed = number(ty, "cDecayedLife").unwrap_or(0.0) as i64;
            let delta = number(ty, "cDecayDelta").unwrap_or(8.0) as u32;
            let Some(c) = self.zoo.entities[at].common.as_mut() else {
                continue;
            };
            let id = c.id;
            let Some(f) = c.fence.as_mut() else { continue };
            // A broken piece does not wear, and a life of 500 cannot deteriorate.
            if f.life == 0 || f.life == 500 {
                continue;
            }
            if f.timers[1] > ms {
                f.timers[1] -= ms;
                continue;
            }
            f.timers[1] = f.timers[0];
            f.life -= 1;
            f.strength = f.strength.saturating_sub(delta).max(1);
            let state = match (f.life, i64::from(f.life) == decayed) {
                (0, _) => {
                    f.strength = 0;
                    " broken"
                }
                (_, true) => " decayed",
                _ => "",
            };
            lines.push(format!(
                "fence {id} life {} strength {}{state}",
                f.life, f.strength
            ));
            self.dirty = true;
        }
    }
}
