//! What the scenarios do to elements by id: disable, hide, and read a row of a list.

use super::Interface;
use crate::ui::layout::{DISABLED, HIDDEN, Kind, SELECTED};

impl Interface {
    /// Disables (`off`) or enables the element `id`; whether it changed. A selected member of a
    /// radio set is deselected as it is disabled, and so are the others of its set. `false` for an
    /// element that does not exist.
    pub fn disable(&mut self, id: i32, off: bool) -> bool {
        let Some(i) = self.tree.find(id) else {
            return false;
        };
        let state = self.tree.elements[i].state;
        if (state & DISABLED != 0) == off {
            return false;
        }
        if off && state & SELECTED != 0 && !self.radio_members(i).is_empty() {
            self.radio_deselect_all(i);
        }
        self.flag(i, DISABLED, off);
        true
    }

    /// Hides (`hide`) or shows the element `id` on its own, a layout with what it holds; whether
    /// it changed. `false` for an element that does not exist.
    pub fn conceal(&mut self, id: i32, hide: bool) -> bool {
        let Some(i) = self.tree.find(id) else {
            return false;
        };
        if (self.tree.elements[i].state & HIDDEN != 0) == hide {
            return false;
        }
        match (&self.tree.elements[i].kind, hide) {
            (Kind::Layout, true) => self.hide_element(i),
            (Kind::Layout, false) => {
                self.show(id);
            }
            _ => self.flag(i, HIDDEN, hide),
        }
        true
    }

    /// The text of row `row` of the list box `id`.
    pub fn row_text(&self, id: i32, row: usize) -> Option<&[u8]> {
        match &self.tree.elements[self.tree.find(id)?].kind {
            Kind::ListBox(list) => list.rows.rows.get(row).map(|r| r.text.as_slice()),
            _ => None,
        }
    }
}
