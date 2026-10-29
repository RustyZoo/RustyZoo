//! The research step's meeting with the interface (research 5.4): what a completed program does.

use super::{App, QUESTION};
use crate::panel::research::branch_of_tab;
use crate::research::Tree;
use crate::text::to_cp1252;
use crate::ui::interface::Buttons;

/// The message strings: a program made available, and a branch with no program left.
const COMPLETED: i32 = 23020;
const ALL_DONE: i32 = 23021;

/// The upgrade lists of the building types for a tree just built or loaded (research 6.3, 7.2).
pub(super) fn upgrades(tree: &Tree) -> crate::building::upgrade::Upgrades {
    let mut lists = Default::default();
    for (target, up) in tree.upgrades() {
        crate::building::upgrade::add(&mut lists, target, up);
    }
    lists
}

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

    /// Applies the effect of each program the step completed (an availability, a building
    /// upgrade, a change of the types or a discount) and says so in the message line.
    pub(super) fn research_done(&mut self) {
        for d in self.sim.take_research_done() {
            let (Some(game), Some(ui)) = (&mut self.game, &mut self.ui) else {
                continue;
            };
            if d.effect == 1
                && let Some((target, up)) = self.sim.research_upgrade(d.name)
            {
                crate::building::upgrade::add(&mut game.world.units.upgrades, target, up);
                // The open building panel shows the new button.
                game.building.clock.force = true;
            }
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
            ui.notify(QUESTION, &to_cp1252(&text), Buttons::OK);
            // A branch with no program left says so in a second dialog (5.4).
            let idle = self.sim.research().and_then(|t| t.branch_of(d.name));
            if let Some(branch) = idle.filter(|b| b.current.is_none()) {
                let text = ui
                    .string(ALL_DONE)
                    .replacen("%s", &ui.string(branch.name), 1);
                ui.notify(QUESTION, &to_cp1252(&text), Buttons::OK);
            }
        }
    }

    /// A funding button of the panel (8.5): the level of the tab's branch moves one step.
    pub(super) fn research_fund(&mut self, up: bool) {
        let branch = self.ui.as_ref().and_then(branch_of_tab);
        if let (Some(b), Some(tree), Some(game)) = (branch, self.sim.research_mut(), &mut self.game)
        {
            tree.fund(b, up);
            game.research.clock.force = true;
        }
    }

    /// A row of the category list was selected (8.3): the tick of that category flips.
    pub(super) fn research_tick(&mut self, row: i32) {
        let branch = self.ui.as_ref().and_then(branch_of_tab);
        if let (Some(b), Some(tree), Some(game)) = (branch, self.sim.research_mut(), &mut self.game)
        {
            if let Ok(row) = usize::try_from(row) {
                tree.toggle(b, row);
            }
            game.research.clock.force = true;
        }
        // A second click on the row would deselect it and say -1: no row stays selected.
        if let Some(ui) = &mut self.ui {
            ui.select_row(crate::panel::research::LIST, None);
        }
    }
}
