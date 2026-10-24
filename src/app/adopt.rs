//! Adopting animals and hiring staff (unit placing 7.1): the release over a pending unit buys it.
//! The rules are in [`crate::place::unit`], the panels in [`crate::build`].

use super::*;
use crate::staff;

impl App<'_> {
    /// A release on the map with an animal or a staff member as the tool: a valid place gets the
    /// unit, paid and booked as animal purchase or in the wages of its class, and logs `world add
    /// CLASS/TYPE/SUBTYPE X,Y id N name=NAME` and `money book CATEGORY PRICE`; the placement
    /// sound plays, and a new pending unit of the same item follows the mouse. A hire draws the
    /// sex of the next one and makes the list again. A place that is not valid, or too dear,
    /// plays the refusal sound and shows its message (`tool message N`, or `tool refused`
    /// without one).
    pub(super) fn adopt(&mut self) {
        let (Some(game), Some(ui)) = (&mut self.game, &mut self.ui) else {
            return;
        };
        let Some(p) = game.pending.clone() else {
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
                let made = game.made.take_if(|m| m.0 == p.ty);
                if let (Some((_, made)), Some(unit)) = (made, game.world.units.get_mut(id)) {
                    unit.part = crate::unit::Part::Staff(made);
                    unit.started = true;
                }
                self.world_lines.push(format!(
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
                if staff::is_staff(ty) {
                    self.sim.draw_hire();
                    game.build.set_hire(self.sim.hire_male);
                    let (vfs, cat) = (renderer.vfs(), renderer.catalogue());
                    game.build.rehire(ui, vfs, cat, &mut self.tool_lines);
                    self.make_pending();
                }
            }
            Outcome::Refused(message) => {
                self.ui_sounds.push("noplace");
                super::carry::refusal(ui, &mut self.tool_lines, message);
            }
        }
        self.aim(false);
    }
}
