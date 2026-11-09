//! The water sparkles of the tanks (map drawing 9.7.10): in each tank's update, after the level
//! step and the purity decay, the sparkles age, a timer runs and, when it is due, one sparkle is
//! put on a random tile of the tank. A sparkle is a short-lived entity, never saved.

use super::Sim;
use crate::entity::Catalogue;
use crate::render::order::ground_height;
use crate::render::shape_of;
use crate::world::World;
use crate::zoo::{Common, Entity};

impl Sim {
    /// The update of every tank, one after the other (exhibits and money 3.10, map drawing 9.7.8
    /// point 4 and 9.7.10 point 7): its level step, the buildings on it validated again (guest
    /// buildings 11.1.1), its purity decay and the ripples they owe, then its sparkles, `dt` ms
    /// long. Whether a level moved.
    pub(super) fn tank_updates(
        &mut self,
        world: &mut World,
        cat: &Catalogue,
        data: &dyn crate::behaviour::Data,
        dt: u32,
    ) -> bool {
        let events = world.event_count();
        for id in world.tank_ids() {
            world.step_tank_water(id);
            self.wet_changed(world, cat, data);
            world.decay_tank_water(id, dt);
            self.draw_ripples(world, cat, data);
            if world.sparkle_tick(id, dt) {
                self.sparkle(world, cat, id);
            }
        }
        // What the edits of the player left owed (and the case of no tank at all).
        self.wet_changed(world, cat, data);
        world.event_count() > events
    }

    /// The entities to remove in this pass: those asked for, then the sparkles flagged.
    pub(super) fn removal_pass(&mut self, world: &mut World) -> Vec<u32> {
        let mut ids = std::mem::take(&mut self.removals);
        ids.extend(world.take_doomed());
        ids
    }

    /// One attempt (point 4): nothing unless the water has some level and is not murky.
    fn sparkle(&mut self, world: &mut World, cat: &Catalogue, id: u32) {
        let murky = world.tank_config().murky;
        let Some(tank) = world
            .tank(id)
            .filter(|t| t.level != 0 && i64::from(t.purity) >= i64::from(murky))
        else {
            return;
        };
        let subtype = if tank.salt { "twaterwv" } else { "frshwav" };
        if cat.find("objects", b"other", subtype.as_bytes()).is_none() {
            return;
        }
        let tiles = world.exhibits().get(id).map_or(&[][..], |e| &e.order[..]);
        if tiles.is_empty() {
            return;
        }
        let t = tiles[self.rng.rand() as usize % tiles.len()];
        let mut sparkle = Entity {
            class: "objects".into(),
            type_name: "other".into(),
            subtype: subtype.into(),
            body_size: None,
            common: Some(Common::default()),
        };
        let Some(shape) = shape_of(cat, &sparkle) else {
            return;
        };
        let (x, y) = (
            t as i32 % world.zoo.width as i32,
            t as i32 / world.zoo.width as i32,
        );
        // A type that floats at the surface is lifted to it; the other stays on the floor, which
        // is where the fresh water ones sit (original bug, kept).
        let lift = if shape.tank.surface {
            16 * tank.level as i32
        } else {
            0
        };
        sparkle.common = Some(Common {
            x: x * 64 + 32,
            y: y * 64 + 32,
            z: ground_height(&world.zoo.tiles[t], 32, 32).saturating_add(lift),
            rotation: self.rng.rand() & 7,
            ..Default::default()
        });
        if let Some(entity) = world.add_wave(sparkle, shape) {
            world.push_sparkle(id, entity);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fixtures::ty;
    use crate::money::Money;
    use crate::unit::testing::tank_world;

    /// The two sparkle types: the salt one floats at the surface, the fresh one has no flag.
    fn catalogue() -> Catalogue {
        let mut cat = Catalogue::default();
        for (name, keys) in [("twaterwv", "cSurface=1\n"), ("frshwav", "")] {
            let text = format!("[Characteristics/Integers]\ncIsTransient=1\n{keys}");
            cat.push(ty("objects", "other", name, &text));
        }
        cat
    }

    /// A tank of 12 tiles (height 5, level 4) and a sim; `salt` makes its water salt.
    fn setup(salt: bool) -> (World, Sim, u32) {
        let mut world = tank_world(5, 4, &[]);
        let id = world.exhibits().list()[0].id;
        world
            .exhibits_mut()
            .get_mut(id)
            .unwrap()
            .tank
            .as_mut()
            .unwrap()
            .salt = salt;
        let mut sim = Sim::new(b"");
        sim.set_seed(1);
        (world, sim, id)
    }

    /// The sparkles in the zoo: subtype and height.
    fn found(world: &World) -> Vec<(String, i32)> {
        (world.zoo.entities.iter())
            .filter(|e| matches!(e.subtype.as_str(), "twaterwv" | "frshwav"))
            .map(|e| (e.subtype.clone(), e.common.as_ref().unwrap().z))
            .collect()
    }

    fn run(world: &mut World, sim: &mut Sim, cat: &Catalogue, steps: u32) {
        for _ in 0..steps {
            sim.step_in(world, cat, 67, |_, _, _| {});
        }
    }

    #[test]
    fn fresh_sparkles_sit_on_the_floor_and_salt_ones_at_the_surface() {
        let cat = catalogue();
        for (salt, name, lift) in [(false, "frshwav", 0), (true, "twaterwv", 16)] {
            let (mut world, mut sim, id) = setup(salt);
            run(&mut world, &mut sim, &cat, 200);
            let list = found(&world);
            assert!(!list.is_empty(), "{name}");
            let level = world.tank(id).unwrap().level as i32;
            let floor = ground_height(&world.zoo.tiles[2 * 12 + 2], 32, 32);
            let ok = |(n, h): &(String, i32)| n == name && *h == floor + lift * level;
            assert!(list.iter().all(ok), "{list:?} on {floor}");
        }
    }

    #[test]
    fn a_tank_keeps_about_a_tenth_of_its_tiles_each_for_four_seconds() {
        let (mut world, mut sim, id) = setup(true);
        let cat = catalogue();
        let mut peak = 0;
        for _ in 0..300 {
            run(&mut world, &mut sim, &cat, 1);
            let (_, s) = world.sparkles().find(|s| s.0 == id).unwrap();
            assert!(s.list.iter().all(|&(_, age)| age <= 4000));
            peak = peak.max(found(&world).len());
        }
        // 12 tiles: one every 3333 ms, each for 4000 ms.
        assert!((1..=3).contains(&peak), "{peak}");
    }

    #[test]
    fn an_attempt_draws_the_tile_then_the_facing_and_stops_early_without_a_draw() {
        let cat = catalogue();
        let draws = |world: &mut World, sim: &mut Sim, cat: &Catalogue| {
            let before = sim.rng.clone();
            sim.sparkle(world, cat, world.exhibits().list()[0].id);
            let mut rng = before;
            let mut n = 0;
            while rng.state() != sim.rng.state() && n < 5 {
                rng.rand();
                n += 1;
            }
            n
        };
        let (mut world, mut sim, id) = setup(false);
        assert_eq!(draws(&mut world, &mut sim, &cat), 2);
        assert_eq!(found(&world).len(), 1);
        // No type: no draw, no sparkle.
        assert_eq!(draws(&mut world, &mut sim, &Catalogue::default()), 0);
        assert_eq!(found(&world).len(), 1);
        // Murky water, and no water: no draw.
        let purity = world.tank(id).unwrap().purity;
        fn tank(w: &mut World, id: u32) -> &mut crate::exhibit::Tank {
            w.exhibits_mut().get_mut(id).unwrap().tank.as_mut().unwrap()
        }
        tank(&mut world, id).purity = 0;
        assert_eq!(draws(&mut world, &mut sim, &cat), 0);
        tank(&mut world, id).purity = purity;
        tank(&mut world, id).level = 0;
        assert_eq!(draws(&mut world, &mut sim, &cat), 0);
        assert_eq!(found(&world).len(), 1);
    }

    #[test]
    fn the_timer_restarts_without_the_overshoot_and_takes_odd_steps() {
        let (mut world, _, id) = setup(false);
        // A new tank: 0 - 0 is not below 0; any step below 0 is due, then 40000 / 12.
        assert!(!world.sparkle_tick(id, 0));
        assert!(world.sparkle_tick(id, 5));
        assert_eq!(world.sparkles().next().unwrap().1.timer, 3333);
        assert!(!world.sparkle_tick(id, 3333));
        assert!(world.sparkle_tick(id, 1));
        // A huge step wraps as a signed 32-bit value and never panics; the ages saturate.
        assert!(!world.sparkle_tick(id, u32::MAX));
        assert!(world.sparkle_tick(id, 1 << 31));
        world.push_sparkle(id, 7);
        world.sparkle_tick(id, u32::MAX);
        assert_eq!(world.doomed(), [7]);
        assert_eq!(world.hidden_sparkles(), [7], "aged out: hidden at once");
    }

    #[test]
    fn the_level_back_to_0_and_a_save_flag_every_sparkle_for_removal_without_hiding_it() {
        let cat = catalogue();
        let (mut world, mut sim, id) = setup(true);
        run(&mut world, &mut sim, &cat, 100);
        let made = found(&world).len();
        assert!(made > 0);
        world.set_paused(true);
        world.drain(id);
        assert!(world.sparkles().all(|s| s.1.list.is_empty()));
        assert_eq!(world.doomed().len(), made);
        assert!(world.hidden_sparkles().is_empty(), "flagged, not hidden");
        // The next removal pass takes them out.
        sim.step_in(&mut world, &cat, 0, |_, _, _| {});
        assert!(found(&world).is_empty() && world.doomed().is_empty());
        assert!(world.hidden_sparkles().is_empty());
        // A save removes them, with animatedwater on.
        world.set_paused(false);
        world.fill(id);
        run(&mut world, &mut sim, &cat, 200);
        assert!(!found(&world).is_empty());
        sim.save_started(&mut world);
        assert!(world.sparkles().all(|s| s.1.list.is_empty()));
        assert!(!world.doomed().is_empty());
        let _ = Money::default();
    }

    #[test]
    fn each_tank_updates_in_turn_so_a_tank_sparkles_before_the_next_ones_ripples() {
        let cat = catalogue();
        // The first tank is full; the second is empty and filled, so its ripples are set up in
        // this step, with draws of their own. The first tank's tile comes from the first draw.
        for seed in 1..=6 {
            let mut world = crate::unit::testing::show_world(&[]);
            let ids = world.tank_ids();
            assert_eq!(ids.len(), 2);
            let empty = world.exhibits_mut().get_mut(ids[1]).unwrap();
            empty.tank.as_mut().unwrap().level = 0;
            let tiles = world.exhibits().get(ids[0]).unwrap().order.clone();
            let mut sim = Sim::new(b"");
            sim.set_seed(seed);
            // The set-ups owed since the world was made are done first.
            sim.wet_changed(&mut world, &cat, &crate::behaviour::NoData);
            let first = sim.rng.clone().rand() as usize;
            sim.tank_updates(&mut world, &cat, &crate::behaviour::NoData, 67);
            let sparkle = (world.zoo.entities.iter())
                .find(|e| matches!(e.subtype.as_str(), "twaterwv" | "frshwav"))
                .and_then(|e| e.common.as_ref())
                .expect("the first tank made a sparkle");
            let t = tiles[first % tiles.len()] as i32;
            let w = world.zoo.width as i32;
            assert_eq!(
                (sparkle.x, sparkle.y),
                (t % w * 64 + 32, t / w * 64 + 32),
                "{seed}"
            );
        }
    }

    #[test]
    fn a_deleted_tank_flags_its_sparkles_at_once_without_hiding_them() {
        let cat = catalogue();
        let (mut world, mut sim, id) = setup(false);
        run(&mut world, &mut sim, &cat, 100);
        let made = world.sparkles().map(|s| s.1.list.len()).sum::<usize>();
        assert!(made > 0);
        let wall = world.zoo.entities.iter().find(|e| e.class == "tankwall").unwrap();
        assert!(world.remove(wall.common.as_ref().unwrap().id));
        assert!(world.exhibits().get(id).is_none(), "the tank is gone");
        assert_eq!(world.doomed().len(), made, "flagged for removal at once");
        assert!(world.hidden_sparkles().is_empty(), "flagged, not hidden");
        assert!(world.sparkles().next().is_none());
    }
}
