//! The scenario's meeting with the interface (scenarios 6, 7.1, 8): what is shown and pressed goes
//! to the scenario, and what its triggers asked of the interface is done here.

use super::{Action, App};
use crate::scenario::{Tell, Which};
use crate::text::to_cp1252;

impl App<'_> {
    /// Tells the scenario which of the buttons and rows it asked about were pressed, and the hint's
    /// button shows the last popup again.
    pub(super) fn scenario_pressed(&mut self, buttons: &[i32], actions: &[Action]) {
        let (Some(scenario), Some(ui)) = (self.sim.scenario(), &mut self.ui) else {
            return;
        };
        let gui = scenario.gui();
        for &id in buttons {
            gui.pressed(id, None);
            self.marks
                .again(id, ui, self.renderer.vfs(), &mut self.scenario_lines);
        }
        for action in actions {
            if let &(Action::Row(id, row) | Action::Double(id, row)) = action
                && let Ok(row) = usize::try_from(row)
                && let Some(text) = ui.row_text(id, row)
            {
                gui.pressed(id, Some((row, text.to_vec())));
            }
        }
    }

    /// Tells the scenario what the interface shows, for the rules that ask.
    pub(super) fn scenario_sync(&mut self) {
        if let (Some(scenario), Some(ui)) = (self.sim.scenario(), &self.ui) {
            let text = |id| to_cp1252(&ui.string(id));
            scenario.gui().sync(&|id| ui.shown(id), &text);
        }
    }

    /// Does what the triggers asked of the interface.
    pub(super) fn scenario_effects(&mut self) {
        self.scenario_avail(false);
        let effects = self.sim.scenario().map(|s| s.gui().take_effects());
        if let (Some(effects), Some(ui)) = (effects, &mut self.ui) {
            for effect in effects {
                self.marks
                    .apply(effect, ui, self.renderer.vfs(), &mut self.scenario_lines);
            }
        }
        for line in self.scenario_lines.drain(..) {
            self.sim.note(line);
        }
    }

    /// Carries out the availability triggers: with `rewards`, at the start of a game, the ini
    /// rewards apply after them (research 7.4). A trigger that changed a flag tells its message or
    /// popup, and an unlock (`truleb` 10) is stored with the rewards.
    pub(super) fn scenario_avail(&mut self, rewards: bool) {
        let made = self.sim.scenario().map(|s| s.gui().take_avail());
        let (Some(game), vfs, cat) = (
            &mut self.game,
            self.renderer.vfs(),
            self.renderer.catalogue(),
        ) else {
            return;
        };
        let (mut changed, mut told, mut unlocked) = (false, Vec::new(), Vec::new());
        for a in made.iter().flatten() {
            let did = game.build.change(cat, std::slice::from_ref(a));
            changed |= did;
            if let Which::Unlock(id) = a.which {
                unlocked.push(id);
            }
            told.extend(a.tell.clone().filter(|_| did));
        }
        if rewards {
            let ini = crate::config::Config::parse(vfs.ini()).unwrap_or_default();
            changed |= game.build.after_triggers(cat, &ini, &[]);
        }
        if let Some(menu) = &mut self.menu {
            self.menu_events.extend(menu.unlock(&unlocked));
        }
        let Some(ui) = &mut self.ui else {
            return;
        };
        let mut log = Vec::new();
        game.build.refill(changed, ui, vfs, cat, &mut log);
        for tell in told {
            match tell {
                Tell::Message(template, name) => {
                    let text = ui.string(template).replacen("%r", &ui.string(name), 1);
                    ui.message(&to_cp1252(&text));
                }
                Tell::Popup(popup) => {
                    if let Some(s) = self.sim.scenario() {
                        s.gui().popup(*popup);
                    }
                }
            }
        }
        for line in log {
            self.sim.note(line);
        }
    }

    /// A game opens or stops: the interface is given back what the last scenario changed (8.4).
    pub(super) fn scenario_clean(&mut self) {
        if let Some(ui) = &mut self.ui {
            self.marks.clean(ui);
        }
    }
}
