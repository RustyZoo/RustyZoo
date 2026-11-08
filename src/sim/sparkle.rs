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
    /// One simulation step of the sparkles of every tank, `dt` ms long; in the original this is
    /// part of each tank's update, so the draws of one tank come after those of its water.
    pub(super) fn sparkle_step(&mut self, world: &mut World, cat: &Catalogue, dt: u32) {
        for id in world.sparkle_tanks() {
            if world.sparkle_tick(id, dt) {
                self.sparkle(world, cat, id);
            }
        }
    }

    /// The removals asked for since the last pass join the pass.
    pub(super) fn sparkles_doomed(&mut self, world: &mut World) {
        self.removals.extend(world.take_doomed());
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
