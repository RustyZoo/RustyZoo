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
                if !self.rng.rand().is_multiple_of(self.ripples.wet) {
                    continue;
                }
                let still = [(1, 0), (-1, 0), (0, 1), (0, -1)]
                    .into_iter()
                    .all(|(dx, dy)| {
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
                if let Some(id) = world.add_wave(ripple.clone(), shape) {
                    self.ripples.slots[slot] = (0, Some(id));
                    return;
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fixtures::ty;
    use crate::unit::testing::world_with;
    use crate::zoo::Clocks;

    fn catalogue() -> Catalogue {
        let mut cat = Catalogue::default();
        let mut wave = ty(
            "objects",
            "other",
            "waterwv1",
            "[Characteristics/Integers]\n",
        );
        wave.footprint = [2, 2, 1];
        cat.push(wave);
        cat
    }

    fn sim(ini: &str) -> Sim {
        let mut sim = Sim::new(ini.as_bytes());
        sim.set_seed(1);
        sim.start_game(0, Clocks::default());
        sim
    }

    /// A map of `w` by `h` tiles whose tiles are water where `wet(x, y)`.
    fn lake((w, h): (i32, i32), wet: impl Fn(i32, i32) -> bool) -> World {
        let mut world = world_with((w, h), |zoo| {
            for (t, tile) in zoo.tiles.iter_mut().enumerate() {
                let (x, y) = (t as i32 % w, t as i32 / w);
                tile.terrain = u8::from(wet(x, y));
            }
        });
        world.set_water(vec![false, true]);
        world
    }

    fn ripples(world: &World) -> Vec<(i32, i32, u32)> {
        waves(&world.zoo)
    }

    fn waves(zoo: &crate::zoo::Zoo) -> Vec<(i32, i32, u32)> {
        let found = zoo.entities.iter().filter(|e| e.subtype == "waterwv1");
        let at = |e: &Entity| e.common.as_ref().map(|c| (c.x / 64, c.y / 64, c.rotation));
        found.filter_map(at).collect()
    }

    /// The scan reaches every tile of a map that is not square, whichever way it is not, and
    /// never asks for a tile outside it: only the far corner can take a ripple here.
    #[test]
    fn the_scan_covers_a_map_that_is_not_square() {
        for size in [(3, 7), (7, 3), (2, 8), (8, 2)] {
            let (w, h) = size;
            let mut world = lake(size, |x, y| x == w - 1 || y == h - 1);
            let mut sim = sim("");
            // The count of the water tiles sees the whole map.
            sim.water_effects(&mut world, &catalogue(), 3004);
            assert_eq!(sim.ripples.wet, (w + h - 1) as u32, "{size:?}");
            sim.ripples.wet = 1;
            sim.scan(&mut world, &catalogue());
            let [(x, y, _)] = ripples(&world)[..] else {
                panic!("{size:?}: {:?}", ripples(&world));
            };
            assert_eq!((x, y), (w - 1, h - 1));
        }
    }

    #[test]
    fn a_ripple_takes_a_still_empty_tile_and_two_draws() {
        let (cat, mut world) = (catalogue(), lake((1, 2), |_, _| true));
        let mut sim = sim("");
        sim.ripples.wet = 1;
        let mut rng = sim.rng.clone();
        let _ = rng.rand();
        let facing = rng.rand() & 7;
        sim.scan(&mut world, &cat);
        assert_eq!(ripples(&world), [(0, 0, facing)]);
        assert_eq!(sim.rng.state(), rng.state());
        // The tile now holds an entity: the next scan takes the next one.
        sim.scan(&mut world, &cat);
        assert_eq!(ripples(&world).len(), 2);
        assert_eq!(ripples(&world)[1].1, 1);
        sim.scan(&mut world, &cat);
        assert_eq!(ripples(&world).len(), 2, "no tile is left");
    }

    #[test]
    fn a_tile_beside_land_takes_none() {
        let mut world = lake((3, 1), |x, _| x != 2);
        let mut sim = sim("");
        sim.ripples.wet = 1;
        sim.scan(&mut world, &catalogue());
        assert_eq!(ripples(&world), [(0, 0, ripples(&world)[0].2)]);
        sim.scan(&mut world, &catalogue());
        assert_eq!(ripples(&world).len(), 1, "(1, 0) has land beside it");
    }

    #[test]
    fn a_ripple_goes_after_four_seconds_and_the_setting_turns_it_off() {
        let (cat, mut world) = (catalogue(), lake((2, 2), |_, _| true));
        let mut sim = sim("");
        sim.ripples.wet = 1;
        sim.scan(&mut world, &cat);
        let id = sim.ripples.slots.iter().find_map(|s| s.1).unwrap();
        sim.ripples.slots.iter_mut().for_each(|s| s.0 = 0);
        sim.ripples.scan = 0;
        sim.water_effects(&mut world, &cat, 4000);
        assert!(!sim.removals.contains(&id));
        sim.water_effects(&mut world, &cat, 1);
        assert!(sim.removals.contains(&id));
        assert!(sim.ripples.slots.iter().all(|s| s.1.is_none()));

        let mut off = self::sim("animatedwater=0\n");
        let mut dry = lake((2, 2), |_, _| true);
        off.ai.animated_water = false;
        let state = off.rng.state();
        off.water_effects(&mut dry, &cat, 10_000);
        assert!(ripples(&dry).is_empty() && off.rng.state() == state && off.ripples.wet == 0);
    }

    #[test]
    fn a_save_keeps_none_of_them() {
        let (cat, mut world) = (catalogue(), lake((2, 2), |_, _| true));
        let mut sim = sim("");
        sim.ripples.wet = 1;
        sim.scan(&mut world, &cat);
        assert_eq!(ripples(&world).len(), 1);
        assert!(waves(&world.saved(|_| true)).is_empty());
    }

    #[test]
    fn a_run_puts_a_few_ripples_on_a_lake() {
        let (cat, mut world) = (catalogue(), lake((20, 20), |_, _| true));
        let mut sim = sim("");
        let money = &mut crate::money::Money::default();
        let data = crate::behaviour::NoData;
        let mut peak = 0;
        for _ in 0..600 {
            sim.step_game(&data, &cat, &mut world, money, 100, |_, _, _| {});
            peak = peak.max(ripples(&world).len());
        }
        // About one every 0.7 s, each shown 4 s.
        assert!((3..=SLOTS).contains(&peak), "{peak}");
    }
}
