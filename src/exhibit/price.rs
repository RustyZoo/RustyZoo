//! The prices of the tank tools (exhibits and money 4.7): a wall step, the water of a fill.

use crate::entity::Catalogue;
use crate::panel::exhibit::type_at;
use crate::world::World;

/// The numbers a tank's prices are made of, recomputed from the world whenever they are shown or
/// used.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Prices {
    /// The price of one wall step: the purchase costs of the wall pieces over the divisor.
    pub wall: f32,
    /// The price of one cubic unit of water of the tank's salinity, times its tiles.
    pub water: f32,
    /// What a fill with salt and one with fresh water cost at the wall height now.
    pub fill: [f32; 2],
}

impl Prices {
    /// The prices of tank `id`, or none when it is not a tank.
    pub fn of(world: &World, cat: &Catalogue, id: u32) -> Option<Prices> {
        let tank = world.tank(id)?;
        let config = world.tank_config();
        let tiles = world.tiles_of(id).len() as f32;
        let pieces: f32 = (world.wall_pieces(id).into_iter())
            .filter_map(|at| type_at(world, cat, at))
            .map(crate::build::cost)
            .sum();
        let wall = if config.divisor == 0.0 {
            0.0
        } else {
            pieces / config.divisor
        };
        let height = tank.height as f32;
        Some(Prices {
            wall,
            water: tiles * config.water[usize::from(!tank.salt)],
            fill: config.water.map(|w| height * tiles * w),
        })
    }

    /// The price of the next wall step of a tank at `height`, up or down: the steps above the
    /// floor (the initial height) cost, the steps at or below it are free; a filled tank adds the
    /// water. A lowering gives the same back (negative).
    pub fn step(&self, height: u32, floor: u32, up: bool, filled: bool) -> f32 {
        let next = if up {
            height.saturating_add(1)
        } else {
            height.saturating_sub(1)
        };
        let delta = i64::from(next.max(floor)) - i64::from(height.max(floor));
        let per = self.wall + if filled { self.water } else { 0.0 };
        per * delta as f32
    }
}
