//! The research step's meeting with the interface (research 5.4): what a completed program does.

use super::App;
use crate::research::Tree;
use crate::text::to_cp1252;
use crate::zoo::{Zoo, ZooError};

/// The string of "%r is now available."
const COMPLETED: i32 = 23020;

impl App<'_> {
    /// The tree of a saved game's scenario with the file's research section read (7.2); none when
    /// the file names no scenario. A scenario that cannot be read refuses the load with the message
    /// of an unsupported old version (zoo-format 7.2).
    pub(super) fn loaded_research(&mut self, zoo: &Zoo) -> Result<Option<Tree>, String> {
        let Some(file) = zoo.scenario_file() else {
            return Ok(None);
        };
        let rng = &mut self.sim.rng;
        let tree = Tree::of_scenario(self.renderer.vfs(), &file, &mut || rng.rand());
        let mut tree = tree.ok_or_else(|| ZooError::TooOld(zoo.version).to_string())?;
        tree.load(zoo.research(), &mut || rng.rand());
        Ok(Some(tree))
    }

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
