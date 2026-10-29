//! The research step's meeting with the interface (research 5.4): what a completed program does.

use super::App;
use crate::research::Tree;
use crate::text::to_cp1252;

/// The string of "%r is now available."
const COMPLETED: i32 = 23020;

impl App<'_> {
    /// The tree of a saved game's scenario with the file's research section read (7.2); none, with
    /// a warning, when the scenario cannot be read: the section is then written as it was read.
    pub(super) fn loaded_research(&mut self, file: String, section: &[u8]) -> Option<Tree> {
        let rng = &mut self.sim.rng;
        let tree = Tree::of_scenario(self.renderer.vfs(), &file, &mut || rng.rand());
        let mut tree = tree.or_else(|| {
            eprintln!(
                "rustyzoo: warning: scenario {file} cannot be read: its research is kept as read"
            );
            None
        })?;
        tree.load(section, &mut || rng.rand());
        Some(tree)
    }

    /// Applies the effect of each program the step completed (an availability, a change of the
    /// types or a discount) and says so in the message line.
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
            } else {
                let cat = self.renderer.catalogue();
                let life = self.sim.research_effect(cat, &mut game.world.research, &d);
                crate::animal::lengthen(&mut game.world, &life);
                // The open staff panel shows the new training (6.4).
                game.staff.clock.force = true;
            }
            let text = ui.string(COMPLETED).replacen("%r", &ui.string(d.name), 1);
            ui.message(&to_cp1252(&text));
        }
    }
}
