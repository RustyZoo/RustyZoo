//! Adopting animals and hiring staff (unit placing 7.1): the release over a pending unit buys it.
//! The rules are in [`crate::place::unit`], the panels in [`crate::build`].

use super::*;
use crate::staff;

impl App<'_> {
    /// The text of the name id of each type of the catalogue, as `ui` shows it: the units name
    /// their new animals, staff and objects with it. The name of a rubble piece goes with them.
    pub(super) fn type_names(&self, ui: &Interface) -> std::collections::HashMap<i32, String> {
        let piece = crate::rubble::PIECE_NAME;
        (self.renderer.catalogue().types.iter())
            .filter(|t| t.name_id != 0)
            .map(|t| (t.name_id, ui.string(t.name_id)))
            .chain([(piece, ui.string(piece))])
            .collect()
    }

    /// A release on the map with an animal or a staff member as the tool: a valid place gets the
    /// unit, paid and booked as animal purchase or in the wages of its class, and logs `world add
    /// CLASS/TYPE/SUBTYPE X,Y id N name=NAME` and `money book CATEGORY PRICE`; the placement
    /// sound plays, and a new pending unit of the same item follows the mouse. A hire draws the
    /// sex of the next one and makes the list again. A place that is not valid, or too dear,
    /// plays the refusal sound and shows its message (`tool message N`, or `tool refused`
    /// without one).
    pub(super) fn adopt(&mut self) {
        playing!(self, game, ui);
        let Some(p) = game.placing.pending.clone() else {
            return;
        };
        let renderer = &self.renderer;
        let ty = &renderer.catalogue().types[p.ty];
        let text = ui.string(ty.name_id);
        // The animals of the exhibit react to a unit placed (habitat rating 9.4, animals 10).
        let cat = renderer.catalogue();
        let tile = game.world.tile_index(p.tile.0, p.tile.1);
        let rand = &mut || self.sim.rng.rand();
        let before = match p.verdict != place::Verdict::Invalid && game.money.can_pay(p.price) {
            true => rating::entity(&mut game.world, cat, tile, place::unit::is_animal(ty), rand),
            false => Vec::new(),
        };
        let made = game.placing.made.clone().filter(|m| m.ty == p.ty);
        let data = crate::behaviour::Library {
            catalogue: cat,
            vfs: renderer.vfs(),
            timings: &self.timings,
        };
        let outcome = place::unit::place(
            &mut game.world,
            &mut game.money,
            ty,
            &text,
            &p,
            |e| renderer.shape(e),
            (made, &data),
        );
        match outcome {
            Outcome::Placed(id) => {
                game.placing.made = None;
                // An animal decides by the compatible animals of the species; a staff member
                // by nothing but the sums.
                let change = place::unit::is_animal(ty).then(|| rating::Change::Animal {
                    ids: rating::Likes::of(ty).ids,
                    sign: 1,
                });
                rating::react(&mut game.world, cat, before, change, rand);
                let name = game
                    .world
                    .find(id)
                    .and_then(|at| game.world.zoo.entities[at].common.as_ref())
                    .map(|c| c.name.clone())
                    .unwrap_or_default();
                self.lines.world.push(format!(
                    "add {}/{}/{} {},{} id {id} name={name}",
                    ty.class,
                    String::from_utf8_lossy(&ty.type_name),
                    String::from_utf8_lossy(&ty.subtype),
                    p.tile.0,
                    p.tile.1,
                ));
                self.money_events
                    .push(format!("book {} {}", place::unit::category(ty), p.price));
                // The drop rule of its kind runs for a new unit too (8.5), then the sound, then
                // the next pending unit of the same item is made (7.1 items 7, 3 and 6).
                self.drop_rule(id);
                self.drop_sound(id);
                self.next_pending(staff::is_staff(ty));
            }
            Outcome::Refused(message) => {
                self.ui_sounds.push("noplace");
                super::carry::refusal(ui, &mut self.lines.tool, message, super::kinds::REFUSAL);
            }
        }
        self.aim(false);
    }

    /// The click made the next pending unit of the same item. A hire draws the sex of the next one
    /// and makes the list again, which makes another one in its place.
    fn next_pending(&mut self, hire: bool) {
        playing!(self, game);
        let Some(tool) = game.build.tool() else {
            return;
        };
        game.build.chose(tool);
        self.make_pending();
        if hire {
            self.sim.draw_hire();
            if let (Some(game), Some(ui)) = (&mut self.game, &mut self.ui) {
                game.build.set_hire(self.sim.hire_male);
                let (vfs, cat) = (self.renderer.vfs(), self.renderer.catalogue());
                game.build.rehire(ui, vfs, cat, &mut self.lines.tool);
            }
            self.make_pending();
        }
    }
}
