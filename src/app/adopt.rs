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
        let outcome = place::unit::place(&mut game.world, &mut game.money, ty, &text, &p, |e| {
            renderer.shape(e)
        });
        match outcome {
            Outcome::Placed(id) => {
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
                // The hired unit is the pending staff member, with the counters it drew.
                let made = game.placing.made.take_if(|m| m.0 == p.ty);
                if let (Some((_, made)), Some(at)) = (&made, game.world.find(id)) {
                    let c = game.world.zoo.entities[at].common.as_mut();
                    c.expect("placed").colors.clone_from(&made.colours);
                }
                if let (Some((_, made)), Some(unit)) = (made, game.world.units.get_mut(id)) {
                    unit.part = crate::unit::Part::Staff(made);
                    unit.started = true;
                }
                self.lines.world.push(format!(
                    "add {}/{}/{} {},{} id {id} name={name}",
                    ty.class,
                    String::from_utf8_lossy(&ty.type_name),
                    String::from_utf8_lossy(&ty.subtype),
                    p.tile.0,
                    p.tile.1,
                ));
                self.ui_sounds.push("place");
                self.money_events
                    .push(format!("book {} {}", place::unit::category(ty), p.price));
                // A hire makes the new pending staff member of the same item, draws the sex,
                // and the rebuilt list's choice makes another one (unit placing 7.1 step 6).
                if staff::is_staff(ty) {
                    game.build.chose(p.ty);
                    self.make_pending();
                    self.sim.draw_hire();
                    if let (Some(game), Some(ui)) = (&mut self.game, &mut self.ui) {
                        game.build.set_hire(self.sim.hire_male);
                        let (vfs, cat) = (self.renderer.vfs(), self.renderer.catalogue());
                        game.build.rehire(ui, vfs, cat, &mut self.lines.tool);
                    }
                    self.make_pending();
                }
            }
            Outcome::Refused(message) => {
                self.ui_sounds.push("noplace");
                super::carry::refusal(ui, &mut self.lines.tool, message, super::kinds::REFUSAL);
            }
        }
        self.aim(false);
    }
}
