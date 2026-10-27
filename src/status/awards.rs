//! The Awards tab of the zoo status window (`awards.md` 7): the list of the awards the zoo
//! holds, rebuilt each time the window is shown.

use super::WINDOW;
use crate::award::Definitions;
use crate::text::to_cp1252;
use crate::ui::interface::Interface;
use crate::ui::widgets::Row;
use crate::vfs::Vfs;

/// The list box of the page.
const LIST: i32 = 4124;

/// Whether the window was shown at the last look.
#[derive(Default)]
pub struct Awards {
    shown: bool,
}

impl Awards {
    /// Rebuilds the list when the window has just been shown, not while it stays open and not
    /// when its tab is clicked (7.3): one row for each held id that has a definition, in the
    /// order of the list, with the first frame of the icon, the name, and the help of the row
    /// (the tooltip layer adds the long-help offset).
    pub fn refresh(&mut self, ui: &mut Interface, held: &[i32], defs: &Definitions, vfs: &Vfs) {
        let shown = ui.shown(WINDOW);
        let opened = shown && !self.shown;
        self.shown = shown;
        if !opened {
            return;
        }
        let rows = held
            .iter()
            .filter_map(|&id| defs.get(id))
            .map(|d| Row {
                text: to_cp1252(&ui.string(d.name)),
                help: d.tooltip,
                icon: (!d.icon.is_empty())
                    .then(|| ui.tree.picture(vfs, &d.icon))
                    .flatten(),
                ..Row::default()
            })
            .collect();
        ui.set_rows(LIST, rows);
    }
}

#[cfg(test)]
mod tests;
