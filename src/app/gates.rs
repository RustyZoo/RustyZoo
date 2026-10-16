//! The gate tool in the game (placing 6.4): a click on a valid fence piece moves the gate of its
//! exhibit there. The rules are in [`crate::place::gate`], the button in [`crate::build`].

use super::*;

impl App<'_> {
    /// A release on the map with the gate tool: the pending gate, if the last move left one, is
    /// bought. A move of the gate is told by `world gate fences/iron/g 13,43 side 2 id 8` and,
    /// when the exhibit had a gate, `world ungate fences/iron/f 13,41 side 6 id 3` first, then
    /// by the `exhibit gate` line; a place that is not valid does nothing, and so does one the cash did
    /// not cover when the pointer reached it; one the cash no longer covers at the click shows 10002
    /// in the message line (`tool message 10002`).
    pub(super) fn gate_release(&mut self) {
        let (Some(game), Some(ui)) = (&mut self.game, &mut self.ui) else {
            return;
        };
        let Some(gate) = game.gate.clone() else {
            return;
        };
        let world = &mut game.world;
        let named = |world: &World, id: u32| {
            let at = world.find(id)?;
            let e = &world.zoo.entities[at];
            let (t, d) = world.slot(at)?;
            let width = world.zoo.width as usize;
            Some(format!(
                "{}/{}/{} {},{} side {d} id {id}",
                e.class,
                e.type_name,
                e.subtype,
                t % width,
                t / width
            ))
        };
        let old = (world.exhibits().get(gate.exhibit))
            .and_then(|e| e.gate)
            .filter(|&g| g != gate.piece);
        let cat = self.renderer.catalogue();
        match place::gate::place(world, &mut game.money, cat, &gate) {
            Outcome::Placed(_) => {
                if let Some(line) = old.and_then(|id| named(world, id)) {
                    self.world_lines.push(format!("ungate {line}"));
                }
                if let Some(line) = named(world, gate.piece) {
                    self.world_lines.push(format!("gate {line}"));
                }
                self.ui_sounds.push("place");
                game.gate = None;
            }
            Outcome::Refused(Some(id)) => {
                self.ui_sounds.push("noplace");
                ui.message(&to_cp1252(&ui.string(id)));
                self.tool_lines.push(format!("tool message {id}"));
            }
            Outcome::Refused(None) => {}
        }
        self.exhibits_told();
        self.aim(false);
    }
}
