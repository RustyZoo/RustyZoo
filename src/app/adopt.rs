//! Adopting animals (unit placing 7.1): the release over a pending animal buys it. The rules are
//! in [`crate::place::unit`], the panel in [`crate::build`].

use super::*;

impl App<'_> {
    /// A release on the map with an animal as the tool: a valid place gets the animal, paid and
    /// booked as animal purchase, and logs `world add animals/TYPE/SUBTYPE X,Y id N name=NAME`;
    /// the placement sound plays, and a new pending animal of the same item follows the mouse.
    /// A place that is not valid, or too dear, plays the refusal sound and shows its message
    /// (`tool message N`, or `tool refused` without one).
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
        let outcome = place::unit::place(&mut game.world, &mut game.money, ty, &text, &p, |e| {
            renderer.shape(e)
        });
        match outcome {
            Outcome::Placed(id) => {
                let name = game
                    .world
                    .find(id)
                    .and_then(|at| game.world.zoo.entities[at].common.as_ref())
                    .map(|c| c.name.clone())
                    .unwrap_or_default();
                self.world_lines.push(format!(
                    "add {}/{}/{} {},{} id {id} name={name}",
                    ty.class,
                    String::from_utf8_lossy(&ty.type_name),
                    String::from_utf8_lossy(&ty.subtype),
                    p.tile.0,
                    p.tile.1,
                ));
                self.ui_sounds.push("place");
            }
            Outcome::Refused(message) => {
                self.ui_sounds.push("noplace");
                match message {
                    Some(id) => {
                        ui.message(&to_cp1252(&ui.string(id)));
                        self.tool_lines.push(format!("tool message {id}"));
                    }
                    None => self.tool_lines.push("tool refused".into()),
                }
            }
        }
        self.aim(false);
    }
}
