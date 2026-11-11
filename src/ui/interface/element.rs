//! What the scenarios do to elements by id: disable, hide, and read a row of a list.

use super::Interface;
use crate::ui::layout::{DISABLED, HIDDEN, Kind, LOCKED, NEVER_SHOWN, SELECTED};

impl Interface {
    /// The ordinary disable (`off`) or enable of the element `id`; whether it changed. Enabling a
    /// locked element does nothing. `false` for an element that does not exist.
    pub(crate) fn disable(&mut self, id: i32, off: bool) -> bool {
        let Some(i) = self.tree.find(id) else {
            return false;
        };
        let was = self.tree.elements[i].state;
        self.flag(i, DISABLED, off);
        self.tree.elements[i].state != was
    }

    /// The scenario disable: locks and disables the element `id` unless it is locked, taking a
    /// selected radio member off its set; whether it changed. `false` for an element that does not
    /// exist.
    pub(crate) fn lock(&mut self, id: i32) -> bool {
        let Some(i) = self.tree.find(id) else {
            return false;
        };
        let state = self.tree.elements[i].state;
        if state & LOCKED != 0 {
            return false;
        }
        if state & SELECTED != 0 && !self.radio_members(i).is_empty() {
            self.radio_deselect_all(i);
        }
        self.flag(i, DISABLED | LOCKED, true);
        true
    }

    /// Clears the lock and the disabled bit of the element `id`; with `only_locked` an element
    /// that is not locked is left as it is. Whether it changed.
    pub(crate) fn unlock(&mut self, id: i32, only_locked: bool) -> bool {
        let Some(i) = self.tree.find(id) else {
            return false;
        };
        let was = self.tree.elements[i].state;
        if only_locked && was & LOCKED == 0 {
            return false;
        }
        self.flag(i, DISABLED | LOCKED, false);
        self.tree.elements[i].state != was
    }

    /// Hides (`hide`) or shows the element `id` on its own, a layout with what it holds; whether
    /// it changed. `false` for an element that does not exist.
    pub(crate) fn conceal(&mut self, id: i32, hide: bool) -> bool {
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

    /// Shows (`on`) or hides the element `id` by the program, as an element kept out of its layout
    /// by the never-shown flag: that flag goes with the hidden one, so the layout's own shows leave
    /// it alone and a hidden one is never hit. `false` for an element that does not exist.
    pub(crate) fn reveal(&mut self, id: i32, on: bool) -> bool {
        let Some(i) = self.tree.find(id) else {
            return false;
        };
        self.flag(i, HIDDEN | NEVER_SHOWN, !on);
        true
    }

    /// The text of row `row` of the list box `id`.
    pub(crate) fn row_text(&self, id: i32, row: usize) -> Option<&[u8]> {
        match &self.tree.elements[self.tree.find(id)?].kind {
            Kind::ListBox(list) => list.rows.rows.get(row).map(|r| r.text.as_slice()),
            _ => None,
        }
    }
}
