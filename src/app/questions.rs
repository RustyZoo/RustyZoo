//! The questions of the game screen (save before leaving, before exiting), the window closed,
//! Escape, and the stop of a game for the main menu.

use super::*;
use crate::panel::exhibit::PLAQUE;
use crate::panel::facts::SPECIES;

impl App<'_> {
    /// Tells the player something in a question box with an OK button.
    pub(super) fn say(&mut self, text: &str) {
        self.menu_events.push(format!("message {text}"));
        if let Some(ui) = &mut self.ui {
            ui.ask(QUESTION, &to_cp1252(text), Buttons::OK);
        }
    }

    /// Asks whether to save before leaving; without an interface to ask with, leaves.
    pub(super) fn ask(&mut self, kind: Asked) {
        let (id, name) = match kind {
            Asked::Leave => (ASK_LEAVE, "leave"),
            Asked::Exit => (ASK_EXIT, "exit"),
            Asked::Load => (ASK_LOAD, "load"),
            // The bulldozer and the tanks ask their own questions.
            Asked::Sweep | Asked::Tank | Asked::Restart => return,
        };
        // The questions ask only when saving is allowed (save games 3).
        if !self.can_save() {
            return self.proceed(kind);
        }
        let asked = self.ui.as_mut().is_some_and(|ui| {
            let text = to_cp1252(&ui.string(id));
            ui.ask(QUESTION, &text, Buttons::YES_NO_CANCEL)
        });
        match asked {
            true => {
                self.asked = Some(kind);
                self.menu_events.push(format!("game ask {name}"));
            }
            false => self.proceed(kind),
        }
    }

    /// The answer to the question that was asked: Yes saves first and goes on if it saved, No goes
    /// on, Cancel stays.
    pub(super) fn answered(&mut self, code: i32) {
        if self.warned {
            return self.warning_closed();
        }
        if self.ending {
            return self.end_answered(code);
        }
        if let Some(saving) = self.saving.take() {
            return self.save_answered(code, saving);
        }
        if self.game.as_ref().is_some_and(|g| g.naming.open) {
            return self.named();
        }
        let Some(kind) = self.asked.take() else {
            return;
        };
        if kind == Asked::Sweep {
            return self.sweep_answered(code);
        }
        if kind == Asked::Tank {
            return self.tank_answered(code);
        }
        if kind == Asked::Restart {
            return self.restart_answered();
        }
        match code {
            6 => self.begin_save(Some(kind)),
            7 => self.proceed(kind),
            _ => {}
        }
    }

    pub(super) fn proceed(&mut self, kind: Asked) {
        match kind {
            Asked::Sweep | Asked::Tank | Asked::Restart => {}
            Asked::Leave => self.stop_game(),
            Asked::Load => self.open_list(),
            Asked::Exit => {
                self.menu_events.push("game exit".to_string());
                self.quit = true;
            }
        }
    }

    /// The window is closed: from the menu the program ends at once; in a game it asks first. A
    /// question that is open is answered Cancel before (input 4.7).
    pub(super) fn close(&mut self) {
        // The scenario is over and its result stored: there is nothing left to save.
        if self.game.is_none() || self.ui.is_none() || self.ending {
            self.quit = true;
            return;
        }
        if let Some(ui) = &mut self.ui
            && ui.modal_open()
        {
            ui.answer(2);
            // The answer is not for the new question: what else is waiting stays.
            let waiting = ui.take_actions();
            self.ui_actions
                .extend(waiting.into_iter().filter(|a| *a != Action::Answer(2)));
            (self.asked, self.saving) = (None, None);
            if let Some(game) = &mut self.game {
                game.sweep = None;
            }
            if self
                .game
                .as_ref()
                .is_some_and(|g| g.naming.refused.is_some())
            {
                self.tank_no();
            }
            self.named();
        }
        self.ask(Asked::Exit);
    }

    /// Escape, with the map view having the keys, or the right button released over the map
    /// (`ask` is false): the first of these that applies acts (input 4.6): the tool of the
    /// building panels is dropped, then the open panels close, and only Escape then asks the
    /// question that leads to the menu.
    pub(super) fn cancel(&mut self, ask: bool) {
        if self.end_assign() {
            return;
        }
        if let (Some(game), Some(ui)) = (&mut self.game, &mut self.ui)
            && (game.build.cancel_modes(ui, &mut self.lines.tool) || close_plaques(ui))
        {
            self.terrain_sync();
            return self.aim(false);
        }
        // A unit carried by hand is the pending entity, after the plaques (step 7).
        if self.game.as_ref().is_some_and(|g| g.carry.is_some()) {
            return self.put_back();
        }
        if let (Some(game), Some(ui)) = (&mut self.game, &mut self.ui) {
            if game.build.cancel_held(ui, &mut self.lines.tool) {
                self.terrain_sync();
                return self.aim(false);
            }
            // The main panel is part of the game screen: only the panels opened from it close.
            if let Some(i) = ui.tree.find(OPTIONS_PANEL)
                && ui.tree.elements[i].state & crate::ui::layout::HIDDEN == 0
            {
                ui.tree.hide(i);
                ui.touch();
                return;
            }
        }
        if ask {
            self.ask(Asked::Leave);
        }
    }

    /// Stops the game and shows the main menu again: Continue is checked, and the music, if it is
    /// not playing at full level, starts from its beginning (main menu 9.2). Without a menu to
    /// go back to, the run ends.
    pub(super) fn stop_game(&mut self) {
        let (Some(ui), Some(menu)) = (&mut self.ui, &mut self.menu) else {
            self.menu_events.push("game exit".to_string());
            self.quit = true;
            return;
        };
        self.game = None;
        ui.clear_messages();
        self.marks.clean(ui);
        (self.painted, self.drawn_at) = (None, None);
        for id in GAME_LAYOUTS {
            if let Some(i) = ui.tree.find(id) {
                ui.tree.hide(i);
            }
        }
        self.menu_events.push("game stop".to_string());
        let shown = menu.show(ui);
        self.menu_events.extend(shown);
        if !self.music.as_ref().is_some_and(Music::playing) {
            self.music = menu.music();
            self.music_restarted = self.music.is_some();
            if self.music_restarted {
                self.menu_events.push("music restart".to_string());
            }
        }
    }
}

/// Step 6 of Escape and the right button (input 4.6): when the species or the recommendations
/// plaque is shown, closes both, the species one first (unit placing 17.3 point 13 b). The staff
/// plaque is left. Whether it did.
fn close_plaques(ui: &mut Interface) -> bool {
    let open = ui.shown(SPECIES) || ui.shown(PLAQUE);
    if open {
        ui.set_hidden(SPECIES, true);
        ui.set_hidden(PLAQUE, true);
    }
    open
}
