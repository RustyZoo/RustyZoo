//! The facts plaques in the game (unit placing 17): what the map view holds and what the animal
//! panel selected, given to [`crate::panel::facts::Panel`] at each look.

use super::*;
use crate::exhibit::members::type_at;
use crate::panel::facts::{self, Look};

impl App<'_> {
    /// Looks at the species and staff plaques. When the plaque's keeper button went back to the
    /// recommendations plaque of an animal, that plaque is open for it, with the animal panel's
    /// button down.
    pub(super) fn facts_sync(&mut self) {
        playing!(self, game, ui);
        let (vfs, cat) = (self.renderer.vfs(), self.renderer.catalogue());
        let tool = game.build.tool();
        // The pending animal of the Adopt panel; a stale one of another tool is not held.
        let animal = tool.is_some_and(|t| place::unit::is_animal(&cat.types[t]));
        let held = (game.placing.made.as_ref().filter(|_| animal)).map(|m| (m.ty, m.id));
        let selected = game.animal.selected.and_then(|id| {
            let ty = type_at(&game.world, cat, game.world.find(id)?)?;
            Some((id, cat.position(ty.class, &ty.type_name, &ty.subtype)?))
        });
        let look = Look {
            held,
            staff: facts::staff_of(cat, tool),
            selected,
        };
        if let Some(id) = game.animal.facts.sync(ui, vfs, cat, &look) {
            ui.set_button(animal::RECOMMENDATIONS, true, true);
            game.panel.show_plaque(id);
        }
    }
}
