//! The scenario's meeting with the interface (scenarios 6, 7.1, 8): what is shown and pressed goes
//! to the scenario, and what its triggers asked of the interface is done here.

use super::{Action, App, kinds};
use crate::money::{SYMBOL, Symbol};
use crate::scenario::{self, Act, Avail, Deed, Line, Popup, Tell, Which};
use crate::text::{format, to_cp1252};

impl App<'_> {
    /// Tells the scenario which of the buttons and rows it asked about were pressed, and the hint's
    /// button shows the last popup again.
    pub(super) fn scenario_pressed(&mut self, buttons: &[i32], actions: &[Action]) {
        let (Some(scenario), Some(ui)) = (self.sim.scenario(), &mut self.ui) else {
            return;
        };
        let gui = scenario.gui();
        let line = scenario.hint_line();
        for &id in buttons {
            gui.pressed(id, None);
            self.marks
                .again(id, line, ui, self.renderer.vfs(), &mut self.lines.scenario);
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

    /// Does what the triggers asked of the interface, one after the other in the order they ran: the
    /// award, its popup and its cash of one pass come in the order of the file (`awards.md` 6.2).
    /// A modal popup holds everything that comes after it until it is answered (`scenarios.md` 8.5).
    pub(super) fn scenario_effects(&mut self) {
        self.scenario_forced();
        loop {
            if self.question_open() {
                break;
            }
            if let Some(ui) = &mut self.ui {
                self.marks.settle(ui, &mut self.lines.scenario);
            }
            let Some(act) = self.sim.scenario().and_then(|s| s.next_act()) else {
                break;
            };
            match act {
                Act::Avail(made) => {
                    let popups = self.avail_run(made, false);
                    if let Some(s) = self.sim.scenario() {
                        s.gui().popups_next(popups);
                    }
                }
                Act::Deed(deeds) => self.scenario_deeds(deeds),
                Act::Effects(effects) => {
                    if let Some(ui) = &mut self.ui {
                        for effect in effects {
                            self.marks.apply(
                                effect,
                                ui,
                                self.renderer.vfs(),
                                &mut self.lines.scenario,
                            );
                        }
                    }
                }
            }
        }
        for line in self.lines.scenario.drain(..) {
            self.sim.note(line);
        }
    }

    /// Whether the triggers of a pass have left something that waits for a modal popup: the
    /// simulation stands still and the scenario does not end until it is done (`scenarios.md` 8.5).
    pub(super) fn scenario_held(&self) -> bool {
        self.sim.scenario().is_some_and(|s| s.gui().pending()) || self.marks.waiting()
    }

    /// Carries out trigger 10: every research program is completed (research 6.8).
    pub(super) fn scenario_forced(&mut self) {
        let forced = self.sim.scenario().map(|s| s.gui().take_forced());
        for progress in forced.into_iter().flatten() {
            self.force_research(progress);
        }
    }

    /// Carries out triggers 1, 3, 5, 6, 8 and 9 (scenarios 7.2, 7.4, 7.5): a message, the cash, a
    /// boxed animal, an award, a removal and the sell switch. A popup follows its deed, to be shown by the
    /// effects that come after.
    fn scenario_deeds(&mut self, deeds: Vec<Deed>) {
        let (Some(game), ui) = (&mut self.game, &mut self.ui) else {
            return;
        };
        let Some(scenario) = self.sim.scenario() else {
            return;
        };
        let (gui, cat, log) = (
            scenario.gui(),
            self.renderer.catalogue(),
            &mut self.lines.scenario,
        );
        let mut popups = Vec::new();
        for deed in deeds {
            let popup = match deed {
                Deed::Say { kind, text, value } => {
                    // A string of 0 or less is no string: nothing shows.
                    if let Some(ui) = ui.as_mut().filter(|_| text > 0) {
                        let line = Line {
                            text,
                            values: [value, 0, 0],
                            state: 0,
                        };
                        ui.message(&to_cp1252(&scenario::built_line(ui, line)), kind);
                        log.push(format!("scenario message {text} kind {kind}"));
                    }
                    None
                }
                Deed::Cash { how, amount, text } => {
                    scenario::cash(&mut game.money, how, amount);
                    log.push(format!("scenario cash {how} {amount}"));
                    if let Some(ui) = ui.as_mut().filter(|_| text > 0) {
                        let symbol = Symbol::new(ui.string(SYMBOL));
                        let money = format::money_cents(&symbol, f64::from(amount), true);
                        let text = ui.string(text).replacen("%s", &money, 1);
                        ui.message(&to_cp1252(&text), 4);
                    }
                    None
                }
                Deed::Boxed { sub, name, popup } => {
                    let renderer = &self.renderer;
                    let id =
                        scenario::boxed(&mut game.world, cat, name, sub, |e| renderer.shape(e));
                    log.push(match id {
                        Some(id) => format!("scenario boxed {name} id {id}"),
                        None => format!("scenario boxed {name} none"),
                    });
                    popup.filter(|_| id.is_some())
                }
                Deed::Award { give, id, text } => {
                    if give {
                        crate::award::add(&mut game.world.zoo.awards, id);
                        log.push(format!("scenario award {id}"));
                    }
                    // The lookup is made whatever `truleb` says (`awards.md` 5.1).
                    if let (Some(ui), Some(def), true) = (ui.as_mut(), cat.awards.get(id), text > 0)
                    {
                        let line = Line {
                            text,
                            values: [def.name, 0, 0],
                            state: 0,
                        };
                        ui.message(&to_cp1252(&scenario::built_line(ui, line)), 4);
                    }
                    None
                }
                Deed::Remove { by, id, popup } => {
                    let (carry, selected, pending) = (
                        &mut game.carry,
                        &mut game.selected,
                        &mut game.placing.pending,
                    );
                    let held = &mut game.held;
                    let n = scenario::remove(&mut game.world, cat, by, id, |gone| {
                        // The map view lets it go: its move is cancelled, it is no longer
                        // selected (7.5).
                        if carry.as_ref().is_some_and(|c| c.id == gone) {
                            (*carry, *held, *pending) = (None, None, None);
                        }
                        if *selected == Some(gone) {
                            *selected = None;
                        }
                    });
                    log.push(format!("scenario remove {by} {id} count {n}"));
                    popup
                }
                Deed::Sale { allowed, popup } => {
                    game.animal.no_sale = !allowed;
                    game.animal.clock.force = true;
                    log.push(format!(
                        "scenario sale {}",
                        if allowed { "on" } else { "off" }
                    ));
                    popup
                }
            };
            popups.extend(popup);
        }
        gui.popups_next(popups);
        self.sim.absorb(&mut game.world);
    }

    /// Carries out the availability triggers. A trigger that changed a flag tells its message or
    /// popup, and an unlock (`truleb` 10) is stored with the rewards.
    pub(super) fn scenario_avail(&mut self) {
        let made = self.sim.scenario().map(|s| s.gui().take_avail());
        let popups = self.avail_run(made.unwrap_or_default(), false);
        if let Some(s) = self.sim.scenario() {
            popups.into_iter().for_each(|p| s.gui().popup(p));
        }
    }

    /// The ini rewards of a start, which come after the triggers and the start hook (research 7.4,
    /// cheats 6.3 point 2).
    pub(super) fn scenario_rewards(&mut self) {
        self.avail_run(Vec::new(), true);
    }

    /// [`scenario_avail`](Self::scenario_avail) for these changes; with `rewards`, the ini rewards
    /// apply after them. The popups the changes tell are returned, to be shown next.
    fn avail_run(&mut self, made: Vec<Avail>, rewards: bool) -> Vec<Popup> {
        let (Some(game), vfs, cat) = (
            &mut self.game,
            self.renderer.vfs(),
            self.renderer.catalogue(),
        ) else {
            return Vec::new();
        };
        let unlocks: Vec<i32> = made
            .iter()
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
        for a in &made {
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
            return Vec::new();
        };
        let mut log = Vec::new();
        game.build.refill(changed, ui, vfs, cat, &mut log);
        let mut popups = Vec::new();
        for tell in told {
            match tell {
                Tell::Message(template, name) => {
                    let text = match name {
                        Some(name) => ui.string(template).replacen("%r", &ui.string(name), 1),
                        None => ui.string(template),
                    };
                    ui.message(&to_cp1252(&text), kinds::GENERAL);
                }
                Tell::Popup(popup) => popups.push(*popup),
            }
        }
        for line in log {
            self.sim.note(line);
        }
        popups
    }

    /// The lookup of the recorded rewards at a game load or a scenario start (entity types 10.4).
    pub(super) fn look_up_unlocks(&mut self) {
        let cat = self.renderer.catalogue();
        if let Some(menu) = &mut self.menu {
            menu.look_up(|id| cat.types.iter().any(|t| t.name_id == id));
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
