//! The marketing part of the first tab of the zoo status window (marketing 6): the
//! funding text and the two funding buttons, written from the marketing of the game.

use crate::marketing::Marketing;
use crate::status::{GAP, WINDOW, Words};
use crate::text::format::{self, Arg};
use crate::text::to_cp1252;
use crate::ui::interface::Interface;

/// The "less" and "more" buttons and the funding text.
pub(crate) const LESS: i32 = 4052;
pub(crate) const MORE: i32 = 4053;
const TEXT: i32 = 4054;

/// When the panel is written next, and what it told last.
#[derive(Debug, Default)]
pub(crate) struct Funding {
    due: u64,
    shown: bool,
    /// A button asked for a refresh at once (6.2).
    pub force: bool,
    told: String,
}

impl Funding {
    /// Writes the panel when the window has just been shown, after a button, and while the window
    /// is shown at the interface update (`updated`) at clock `now` ms when `GAP` ms have passed
    /// since the last run, as the finance cells do (6.2). Returns the log line when what it shows
    /// changed.
    pub(crate) fn refresh(
        &mut self,
        ui: &mut Interface,
        marketing: &Marketing,
        now: u64,
        updated: bool,
    ) -> Option<String> {
        let opened = ui.shown(WINDOW) && !self.shown;
        self.shown = ui.shown(WINDOW);
        let run = updated && self.shown && now >= self.due;
        if run {
            self.due = now.saturating_add(GAP);
        }
        if !(opened || run || std::mem::take(&mut self.force)) {
            return None;
        }
        let config = marketing.config()?;
        // An index out of range gives an empty text (6.2).
        let level = config.levels.get(config.level);
        let words = Words::new(|id| ui.string(id as i32));
        let text = level.map_or(String::new(), |l| {
            let cost = format::whole_units(f64::from(l.cost));
            let money = format::money(&words, cost, false);
            format::expand(&words, &ui.string(l.name), &[Arg::Text(&money)])
        });
        ui.set_text(TEXT, &to_cp1252(&text));
        let (less, more) = (config.level > 0, config.level + 1 < config.levels.len());
        ui.set_enabled(LESS, less);
        ui.set_enabled(MORE, more);
        let told = format!("funding {text:?} less {less} more {more}");
        (told != self.told).then(|| {
            self.told.clone_from(&told);
            told
        })
    }
}

#[cfg(test)]
mod tests;
