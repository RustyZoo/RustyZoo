//! The research step's meeting with the interface (research 5.4): what a completed program does.

use super::App;
use crate::text::to_cp1252;

/// The string of "%r is now available."
const COMPLETED: i32 = 23020;

impl App<'_> {
    /// Applies the effect of each program the step completed (an availability here) and says so
    /// in the message line.
    pub(super) fn research_done(&mut self) {
        for d in self.sim.take_research_done() {
            let (Some(game), Some(ui)) = (&mut self.game, &mut self.ui) else {
                continue;
            };
            if d.effect == 0 {
                let (vfs, cat) = (self.renderer.vfs(), self.renderer.catalogue());
                let mut log = Vec::new();
                game.build
                    .set_available(ui, vfs, cat, d.target, true, &mut log);
                for line in log {
                    self.sim.note(line);
                }
            }
            let text = ui.string(COMPLETED).replacen("%r", &ui.string(d.name), 1);
            ui.message(&to_cp1252(&text));
        }
    }
}
