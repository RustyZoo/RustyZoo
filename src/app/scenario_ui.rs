//! The scenario's meeting with the interface (scenarios 6, 7.1, 8): what is shown and pressed goes
//! to the scenario, and what its triggers asked of the interface is done here.

use super::{Action, App};
use crate::scenario::{Avail, Tell, Which};
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
        let unlocks: Vec<i32> = made
            .iter()
            .flatten()
            .filter_map(|a| match a.which {
                Which::Unlock(id) => Some(id),
                _ => None,
            })
            .collect();
        // An unlock (truleb 10) changes flags only for the ids the table newly lists (9.2).
        let newly = match &mut self.menu {
            Some(menu) => {
                let listed = menu.unlock(&unlocks);
                self.menu_events
                    .extend(listed.iter().map(|id| format!("scenario unlock {id}")));
                listed
            }
            None => unlocks,
        };
        let (mut changed, mut told) = (false, Vec::new());
        for a in made.iter().flatten() {
            if matches!(a.which, Which::Unlock(id) if !newly.contains(&id)) {
                continue;
            }
            let did = game.build.change(cat, std::slice::from_ref(a));
            changed |= did;
            told.extend(a.tell.clone().filter(|_| did));
        }
        if rewards {
            changed |= game
                .build
                .after_triggers(cat, &reward_ini(&self.menu, vfs), &[]);
        }
        let Some(ui) = &mut self.ui else {
            return;
        };
        let mut log = Vec::new();
        game.build.refill(changed, ui, vfs, cat, &mut log);
        for tell in told {
            match tell {
                Tell::Message(template, name) => {
                    let text = match name {
                        Some(name) => ui.string(template).replacen("%r", &ui.string(name), 1),
                        None => ui.string(template),
                    };
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

    /// The scenario ended (entity types 10.4): a win runs the unlock step once for each of its own
    /// ids that `Menu::unlock` newly listed (`unlocks`, empty after a loss); an id already listed
    /// changes no flag. The purchase lists follow.
    pub(super) fn end_rewards(&mut self, unlocks: &[i32]) {
        let (Some(game), vfs, cat) = (
            &mut self.game,
            self.renderer.vfs(),
            self.renderer.catalogue(),
        ) else {
            return;
        };
        let made: Vec<_> = unlocks
            .iter()
            .map(|&id| Avail {
                which: Which::Unlock(id),
                on: true,
                tell: None,
            })
            .collect();
        let changed = game.build.change(cat, &made);
        if let Some(ui) = &mut self.ui {
            let mut log = Vec::new();
            game.build.refill(changed, ui, vfs, cat, &mut log);
            for line in log {
                self.sim.note(line);
            }
        }
    }

    /// A game opens or stops: the interface is given back what the last scenario changed (8.4).
    pub(super) fn scenario_clean(&mut self) {
        if let Some(ui) = &mut self.ui {
            self.marks.clean(ui);
        }
    }
}

/// The ini rewards to apply: the settings of the menu, which hold what this run unlocked, else
/// those of the install.
pub(super) fn reward_ini(
    menu: &Option<crate::menu::Menu>,
    vfs: &crate::vfs::Vfs,
) -> crate::config::Config {
    match menu {
        Some(menu) => menu.settings().clone(),
        None => crate::config::Config::parse(vfs.ini()).unwrap_or_default(),
    }
}
