//! The units of a map as the game shows them once loaded, for the static picture.

use super::{MapRenderer, Shown};
use crate::behaviour::{Library, Timings};
use crate::sim::Sim;
use crate::world::World;
use crate::zoo::Zoo;

impl MapRenderer<'_> {
    /// The animation each unit of `zoo` shows right after the game loads it (behaviours 5.10.1
    /// point 3): the zoo is loaded as a game does, with no step.
    pub(super) fn loaded_units(&self, zoo: &Zoo) -> Vec<Shown> {
        let (vfs, cat) = (self.vfs, self.catalogue);
        let language = crate::strings::Language::of(vfs.root());
        let mut world = World::with_language(zoo.clone(), |e| self.shape(e), language);
        let mut sim = Sim::new(vfs.ini());
        let water = crate::place::water(&crate::terrain::types(vfs));
        world.set_install(vfs, water, sim.ai.animal_reset_interval);
        sim.start_game(world.zoo.tiles.len(), world.zoo.clocks.clone());
        sim.load(&mut world, cat);
        world.set_tank_config(crate::exhibit::TankConfig::load(vfs));
        let timings = Timings::default();
        let data = Library {
            catalogue: cat,
            vfs,
            timings: &timings,
        };
        crate::unit::resume::settle(&mut world, &data);
        (world.units.iter())
            .filter(|u| !u.animation.name.is_empty())
            .map(|u| Shown {
                id: u.id,
                animation: u.animation.name.clone(),
                frame: u.animation.shown(),
            })
            .collect()
    }
}
