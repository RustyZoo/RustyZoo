//! The animated water of the world manager (simulation 7.2): every 0.7 s a scan of the map puts
//! one ripple on a still-water tile, and each ripple goes after 4 s; at most 100 at once.
//!
//! The original bounds the two loops of the scan, and of the count of water tiles, by the height
//! and the width the wrong way round, which reads outside a map that is not square (original bug,
//! issue 305). Here both loops cover the whole map and never leave it.
//!
//! The two tile bytes of the condition (the flag 8 and the byte not identified) are not kept by
//! the engine: they count as clear.

use super::Sim;
use crate::entity::Catalogue;
use crate::render::order::ground_height;
use crate::render::shape_of;
use crate::world::World;
use crate::zoo::{Common, Entity};

/// The number of slots, and the age in ms above which a slot is free or its ripple goes.
const SLOTS: usize = 100;
const LIFE: u32 = 4000;

/// The slots (an age and the ripple's entity id) and the two counters of the water effects.
#[derive(Debug)]
pub(super) struct Ripples {
    slots: Vec<(u32, Option<u32>)>,
    /// Milliseconds since the last count of the water tiles, and that count (N).
    count: u32,
    wet: u32,
    /// Milliseconds since the last scan.
    scan: u32,
}

impl Default for Ripples {
    fn default() -> Ripples {
        Ripples {
            slots: vec![(LIFE + 1, None); SLOTS],
            count: 0,
            wet: 0,
            scan: 0,
        }
    }
}

impl Sim {
    /// One step of the water effects (7.2); nothing when `animatedwater` is off.
    pub(super) fn water_effects(&mut self, world: &mut World, cat: &Catalogue, step: u32) {
        if !self.ai.animated_water {
            return;
        }
        let r = &mut self.ripples;
        for (age, effect) in &mut r.slots {
            *age = age.saturating_add(step);
            if *age > LIFE
                && let Some(id) = effect.take()
            {
                self.removals.push(id);
            }
        }
        r.count = r.count.saturating_add(step);
        if r.count > 3003 {
            r.count = 0;
            let wet = (0..world.zoo.tiles.len()).filter(|&t| world.is_water(t));
            r.wet = wet.count() as u32;
        }
        r.scan = r.scan.saturating_add(step);
        if r.scan > 701 && r.wet > 0 {
            r.scan = 0;
            self.scan(world, cat);
        }
    }

    /// The scan of the whole map, x outer and y inner: it stops at the first ripple placed.
    fn scan(&mut self, world: &mut World, cat: &Catalogue) {
        let mut ripple = Entity {
            class: "objects".into(),
            type_name: "other".into(),
            subtype: "waterwv1".into(),
            body_size: None,
            common: Some(Common::default()),
        };
        let Some(shape) = shape_of(cat, &ripple) else {
            return;
        };
        let (w, h) = (world.zoo.width as i32, world.zoo.height as i32);
        for x in 0..w {
            for y in 0..h {
                let Some(t) = world.tile_index(x, y) else {
                    continue;
                };
                if !world.is_water(t) || !world.is_empty(t) {
                    continue;
                }
                if self.rng.rand() % self.ripples.wet != 0 {
                    continue;
                }
                let still = [(1, 0), (-1, 0), (0, 1), (0, -1)].into_iter().all(|(dx, dy)| {
                    (world.tile_index(x + dx, y + dy)).is_none_or(|n| world.is_water(n))
                });
                let slot = self.ripples.slots.iter().position(|s| s.0 > LIFE);
                let (true, Some(slot)) = (still, slot) else {
                    continue;
                };
                ripple.common = Some(Common {
                    x: x * 64 + 32,
                    y: y * 64 + 32,
                    z: ground_height(&world.zoo.tiles[t], 32, 32),
                    rotation: self.rng.rand() & 7,
                    ..Default::default()
                });
                if let Some(id) = world.unrecorded(|w| w.add(ripple.clone(), Some(shape))) {
                    self.ripples.slots[slot] = (0, Some(id));
                    return;
                }
            }
        }
    }
}
