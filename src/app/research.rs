//! The research step's meeting with the interface (research 5.4): what a completed program does.

use super::{App, EXCLAIM_PICTURE, QUESTION};
use crate::panel::research::branch_of_tab;
use crate::research::Tree;
use crate::text::to_cp1252;
use crate::ui::interface::{Buttons, Interface};
use crate::vfs::Vfs;

/// The message strings: a program made available, and a branch with no program left.
const COMPLETED: i32 = 23020;
const ALL_DONE: i32 = 23021;

/// The exclamation picture on the icon of the completion dialog, set before it is shown (5.4.1).
fn exclaim(ui: &mut Interface, vfs: &Vfs) {
    let picture = ui.tree.picture(vfs, EXCLAIM_PICTURE);
    ui.set_question_icon(QUESTION, picture);
}

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
            let vfs = self.renderer.vfs();
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
            exclaim(ui, vfs);
            ui.notify(QUESTION, &to_cp1252(&text), Buttons::OK);
            // A branch with no program left says so in a second dialog (5.4).
            let idle = self.sim.research().and_then(|t| t.branch_of(d.name));
            if let Some(branch) = idle.filter(|b| b.current.is_none()) {
                let text = ui
                    .string(ALL_DONE)
                    .replacen("%s", &ui.string(branch.name), 1);
                ui.message(&to_cp1252(&text));
                exclaim(ui, vfs);
                ui.notify(QUESTION, &to_cp1252(&text), Buttons::OK);
            }
        }
    }

    /// Completes every research program with no dialog (research 6.8): the targets become
    /// available, the building upgrades and the changes of the types are laid, as for a program
    /// the step completed. With `progress` the progress is also set to the cost.
    pub(super) fn force_research(&mut self, progress: bool) {
        let Some(game) = &mut self.game else {
            return;
        };
        let (vfs, cat) = (self.renderer.vfs(), self.renderer.catalogue());
        let (forced, life) = self
            .sim
            .force_research(cat, &mut game.world.research, progress);
        crate::animal::lengthen(&mut game.world, &life);
        let mut flags = Vec::new();
        for d in &forced {
            if d.effect == 0 {
                flags.push((d.target, true));
            } else if d.effect == 1
                && let Some((target, up)) = self.sim.research_upgrade(d.name)
            {
                crate::building::upgrade::add(&mut game.world.units.upgrades, target, up);
            }
        }
        game.build.start_research(cat, &flags);
        // The open lists and panels follow.
        game.building.clock.force = true;
        game.staff.clock.force = true;
        game.research.clock.force = true;
        if let Some(ui) = &mut self.ui {
            let mut log = Vec::new();
            game.build.refill(!flags.is_empty(), ui, vfs, cat, &mut log);
            log.into_iter().for_each(|line| self.sim.note(line));
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

    /// A row of the expansion selector was chosen (entity types 10.2): the purchase list is made
    /// again and the research panel follows (research 8.3).
    pub(super) fn expansion_row(&mut self, row: i32) {
        let (Some(game), Some(ui)) = (&mut self.game, &mut self.ui) else {
            return;
        };
        let (vfs, cat) = (self.renderer.vfs(), self.renderer.catalogue());
        game.build.pick_row(ui, vfs, cat, row, &mut self.tool_lines);
        game.research.clock.force = true;
    }

    /// Whether the expansion selector's list is open (entity types 10.5).
    pub(super) fn selector_open(&self) -> bool {
        matches!((&self.game, &self.ui), (Some(game), Some(ui)) if game.build.selector_open(ui))
    }

    /// A row of the category list was selected (8.3): the tick of that category flips.
    pub(super) fn research_tick(&mut self, row: i32) {
        let branch = self.ui.as_ref().and_then(branch_of_tab);
        if let (Some(b), Some(tree), Some(game)) = (branch, self.sim.research_mut(), &mut self.game)
        {
            if let Ok(row) = usize::try_from(row) {
                tree.toggle(b, row, game.build.value());
            }
            game.research.clock.force = true;
        }
        // A second click on the row would deselect it and say -1: no row stays selected.
        if let Some(ui) = &mut self.ui {
            ui.select_row(crate::panel::research::LIST, None);
        }
    }
}
