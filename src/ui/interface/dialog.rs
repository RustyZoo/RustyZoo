//! The question layouts (ui layout 8, input 9): a message, the buttons the question needs, and
//! an answer. While one is open only it receives input and the game is paused.
//!
//! The layouts `ui/confirm*.lyt` have fixed ids from the layout's id `D`: `D + 6` the message
//! text, `D + 10`, `D + 11`, `D + 12`, `D + 13` the buttons Yes, No, OK, Cancel, whose built-in
//! action 5 answers with their `target`: 6, 7, 1, 2 (Windows' message box codes).

use super::{Action, Interface};
use crate::app::Key;
use crate::ui::layout::{HIDDEN, Kind};

#[cfg(test)]
mod tests;

/// The answers.
pub const OK: i32 = 1;
pub const CANCEL: i32 = 2;
pub const YES: i32 = 6;
pub const NO: i32 = 7;

/// Which buttons a question shows.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Buttons {
    pub yes: bool,
    pub no: bool,
    pub ok: bool,
    pub cancel: bool,
}

impl Buttons {
    pub const OK: Buttons = Buttons {
        yes: false,
        no: false,
        ok: true,
        cancel: false,
    };
    pub const OK_CANCEL: Buttons = Buttons {
        ok: true,
        cancel: true,
        ..Buttons::NONE
    };
    pub const YES_NO: Buttons = Buttons {
        yes: true,
        no: true,
        ..Buttons::NONE
    };
    pub const YES_NO_CANCEL: Buttons = Buttons {
        yes: true,
        no: true,
        cancel: true,
        ok: false,
    };
    const NONE: Buttons = Buttons {
        yes: false,
        no: false,
        ok: false,
        cancel: false,
    };

    fn count(self) -> usize {
        [self.yes, self.no, self.ok, self.cancel]
            .into_iter()
            .filter(|&b| b)
            .count()
    }
}

/// The offsets of the buttons from the layout's id, in the order Yes, No, OK, Cancel.
const BUTTONS: [i32; 4] = [10, 11, 12, 13];
const MESSAGE: i32 = 6;

impl Interface {
    /// Whether a question is open: the game stands still meanwhile.
    pub fn modal_open(&self) -> bool {
        self.modal.is_some()
    }

    /// Opens the question layout `layout` (`ui/confirm.lyt` is 45000) with a message and the
    /// buttons it needs: the other buttons are hidden, and when exactly two are shown they take
    /// their alternative positions. The layout is put in the middle of the window and in front,
    /// and the focus leaves the map view. A question that is already open is answered with
    /// Cancel first. `false` if there is no such layout.
    pub fn ask(&mut self, layout: i32, message: &[u8], buttons: Buttons) -> bool {
        self.open_dialog(layout, message, buttons, true)
    }

    /// Opens the same layout without the question: the game runs on, the pointer is not
    /// restricted to it, and an answer closes it with no [`Action::Answer`]. It closes by itself
    /// after `[UI] NonmodalDialogDelay` ms of interface updates (0 or less: never), and at once
    /// when a modal question opens or waits. One popup at a time: the one that was open is closed.
    pub fn notify(&mut self, layout: i32, message: &[u8], buttons: Buttons) -> bool {
        self.open_dialog(layout, message, buttons, false)
    }

    fn open_dialog(&mut self, layout: i32, message: &[u8], buttons: Buttons, modal: bool) -> bool {
        let Some(d) = self
            .tree
            .find(layout)
            .filter(|&d| matches!(self.tree.elements[d].kind, Kind::Layout))
        else {
            return false;
        };
        if modal && self.modal.is_some() {
            self.answer(CANCEL);
        }
        // A new popup replaces the old one; a modal question closes it at once.
        if let Some((open, _)) = self.popup.take() {
            self.hide_element(open);
        }
        self.set_focus(None);
        if let Some(text) = self.tree.find(layout + MESSAGE) {
            self.put_text(text, message);
        }
        self.tree.show(layout);
        let wanted = [buttons.yes, buttons.no, buttons.ok, buttons.cancel];
        for (offset, wanted) in BUTTONS.into_iter().zip(wanted) {
            let Some(b) = self.tree.find(layout + offset) else {
                continue;
            };
            self.tree.elements[b].alt = wanted && buttons.count() == 2;
            if !wanted {
                self.tree.elements[b].state |= HIDDEN;
            }
        }
        let window = self.tree.elements[0]
            .placed
            .map_or((0, 0), |p| (p.dx, p.dy));
        if let Some(p) = self.tree.elements[d].placed {
            let centre = |window: i32, size: i32| (window.saturating_sub(size)) / 2;
            self.tree
                .set_screen_position(d, (centre(window.0, p.dx), centre(window.1, p.dy)));
        }
        self.tree.bring_to_front(d);
        match modal {
            true => self.modal = Some(d),
            false => self.popup = Some((d, i64::from(self.settings.popup_delay.max(0)))),
        }
        // The hover element is unhighlighted; the tooltip stays until the next mouse message.
        self.hover_to(None);
        self.event(format!("{} {layout}", if modal { "ask" } else { "notify" }));
        true
    }

    /// Answers the open question with `code` (values above 9 count as 1) and closes it; with no
    /// question open, closes the popup. Nothing if none is open.
    pub fn answer(&mut self, code: i32) {
        let Some(d) = self.modal.take() else {
            if let Some((d, _)) = self.popup.take() {
                self.hide_element(d);
            }
            return;
        };
        let code = if code > 9 { OK } else { code };
        self.set_focus(None);
        self.hide_element(d);
        self.event(format!("answer {code}"));
        self.actions.push(Action::Answer(code));
    }

    /// Whether the button at `offset` of the open question's layout is shown.
    fn button_shown(&self, d: usize, offset: i32) -> bool {
        let id = self.tree.elements[d].id + offset;
        self.tree
            .find(id)
            .is_some_and(|b| self.tree.elements[b].state & HIDDEN == 0)
    }

    /// A key for the open question: Enter answers Yes if that button is shown, else OK; Escape
    /// Cancel, or No when No is shown and Cancel is not; Y and N answer Yes and No when shown.
    /// Whether it answered.
    pub(super) fn dialog_key(&mut self, key: Key) -> bool {
        let Some(d) = self.modal else { return false };
        let [yes, no, _, cancel] = BUTTONS.map(|o| self.button_shown(d, o));
        let code = match key {
            Key::Enter if yes => YES,
            Key::Enter => OK,
            Key::Escape if no && !cancel => NO,
            Key::Escape => CANCEL,
            Key::Y if yes => YES,
            Key::N if no => NO,
            _ => return false,
        };
        self.answer(code);
        true
    }
}
