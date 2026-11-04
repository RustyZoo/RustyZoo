//! The selector of the panels, the unlocked lists and the hire list: the methods of [`Build`] that
//! choose which types the lists show.

use super::*;

impl Build {
    /// Sets the selector's value: the one of a row, else All (entity types 10.5).
    pub(super) fn set(&mut self, value: i32) {
        let known = self.entries.iter().any(|e| e.id + 1 == value);
        self.selector = if known { value } else { 0 };
    }

    /// A scenario's start sets the entries it disables and the selector, if it has a value for it
    /// (research 3.5). It comes before a panel opens.
    pub(crate) fn set_scenario(&mut self, off: [bool; 4], selection: Option<i32>) {
        self.off = off;
        self.rowed = false;
        if let Some(value) = selection {
            self.set(value);
        }
    }

    /// The selector's value: 0 for All, else the entry id plus 1 of one of its rows. It is what a
    /// file stores.
    pub(crate) fn value(&self) -> i32 {
        self.selector
    }

    /// What the purchase lists filter by now.
    pub(super) fn filter(&self) -> Filter {
        let mut on = 0;
        for (i, (_, bit)) in EXPANSIONS.iter().enumerate() {
            let installed = *bit == EXPANSIONS[0].1 || self.expansions & bit != 0;
            if installed && !self.off[i + 1] {
                on |= bit;
            }
        }
        Filter {
            on,
            pick: usize::try_from(self.value()).ok().filter(|&v| v > 0),
        }
    }

    /// The text of the selector: the string of the selected row.
    pub(crate) fn label(&self, ui: &Interface) -> String {
        let entry = self.entries.iter().find(|e| e.id + 1 == self.value());
        ui.string(entry.map_or(ALL, |e| e.list))
    }

    /// The selector's list gets its rows: All, then the entries; the selected one is marked.
    pub(super) fn fill_selector(&mut self, ui: &mut Interface) {
        // A row of an entry the scenario disables is disabled (entity types 10.5).
        let row = |(id, value): (i32, usize)| Row {
            text: to_cp1252(&ui.string(id)),
            disabled: self.off[value],
            ..Row::default()
        };
        let rows = (std::iter::once((ALL, 0)))
            .chain(self.entries.iter().map(|e| (e.list, (e.id + 1) as usize)))
            .map(row)
            .collect();
        ui.refill_rows(SELECTOR_LIST, rows);
        self.mark_selector(ui);
    }

    pub(super) fn mark_selector(&self, ui: &mut Interface) {
        let at = self.entries.iter().position(|e| e.id + 1 == self.value());
        ui.select_row(SELECTOR_LIST, Some(at.map_or(0, |i| i + 1)));
    }

    /// Whether the selector's list is open and has the focus.
    pub(crate) fn selector_open(&self, ui: &Interface) -> bool {
        ui.shown(SELECTOR) && ui.has_focus(SELECTOR_LIST)
    }

    /// Hides the selector's list and takes the focus from it.
    pub(crate) fn close_selector(&mut self, ui: &mut Interface) {
        if let Some(i) = ui.tree.find(SELECTOR) {
            ui.tree.hide(i);
            ui.touch();
        }
        ui.unfocus(SELECTOR_LIST);
        self.taken = false;
    }

    /// The list takes the focus when it opens and closes when it loses it (a press on another
    /// control, the map view included; entity types 10.5).
    pub(super) fn watch_selector(&mut self, ui: &mut Interface) {
        if !ui.shown(SELECTOR) {
            self.taken = false;
            ui.unfocus(SELECTOR_LIST);
        } else if !self.taken {
            self.taken = ui.focus(SELECTOR_LIST);
        } else if !ui.has_focus(SELECTOR_LIST) {
            self.close_selector(ui);
        }
    }

    /// A row of the selector's list was chosen (-1 for none, which leaves the list open). A row
    /// closes it and sets the value, whether new or not: every tab forgets its item, both panels'
    /// rotation steps are 0 and the panels that are not open reopen on their first tab (placing
    /// 3.5), and the open list is made again (placing 3.4).
    pub(crate) fn pick_row(
        &mut self,
        ui: &mut Interface,
        vfs: &Vfs,
        cat: &Catalogue,
        row: i32,
        log: &mut Vec<String>,
    ) {
        let value = match usize::try_from(row) {
            Ok(0) => Some(0),
            Ok(r) => self.entries.get(r - 1).map(|e| e.id + 1),
            Err(_) => None,
        };
        let Some(value) = value else {
            return self.mark_selector(ui);
        };
        self.close_selector(ui);
        self.selector = value;
        self.chosen = [[None; 4]; PANEL_COUNT];
        self.step = [0; PANEL_COUNT];
        for (p, panel) in PANELS.iter().enumerate() {
            if self.open != Some(p) {
                Self::first_tab(ui, panel);
            }
        }
        log.push(format!("tool expansion {value}"));
        if let Some(p) = self.open {
            self.fill(ui, vfs, cat, p, log);
        }
        self.mark_selector(ui);
    }

    /// The tree of a new game is built (research 7.4): its programs not done make their targets
    /// unavailable. The start triggers and the ini rewards ([`Build::after_triggers`]) follow.
    pub(crate) fn start_research(&mut self, cat: &Catalogue, flags: &[(i32, bool)]) {
        for &(target, on) in flags {
            self.avail.set(cat, target, on);
        }
    }

    /// Makes every entity type available (the developer panel's research button, research 6.8).
    /// Whether a flag changed.
    pub(crate) fn make_all_available(&mut self, cat: &Catalogue) -> bool {
        self.avail.set_where(cat, |_| true, true)
    }

    /// The start triggers made these types available or not, then the ini rewards apply last
    /// (research 7.4). Whether a flag changed.
    pub(crate) fn after_triggers(&mut self, cat: &Catalogue, ini: &Config, made: &[Avail]) -> bool {
        let mut changed = self.change(cat, made);
        changed |= self.avail.unlock(cat, ini);
        changed
    }

    /// Whether the type at `index` of the catalogue is available.
    #[cfg(test)]
    pub(crate) fn is_available(&self, cat: &Catalogue, index: usize) -> bool {
        self.avail.is(cat, index)
    }

    /// Applies availability triggers (scenarios 7.3), in order. Whether a flag changed.
    pub(crate) fn change(&mut self, cat: &Catalogue, made: &[Avail]) -> bool {
        let mut changed = false;
        for a in made {
            changed |= match a.which {
                Which::Unlock(id) => self.avail.unlock_id(cat, id),
                _ => self.avail.set_where(cat, |ty| a.reaches(ty), a.on),
            };
        }
        changed
    }

    /// Makes every type with this name id available or not, whatever its subtype, and makes the
    /// open list again when a flag changed (research 6.2). Whether a flag changed.
    pub(crate) fn set_available(
        &mut self,
        ui: &mut Interface,
        vfs: &Vfs,
        cat: &Catalogue,
        name_id: i32,
        on: bool,
        log: &mut Vec<String>,
    ) -> bool {
        let changed = self.avail.set(cat, name_id, on);
        self.refill(changed, ui, vfs, cat, log);
        changed
    }

    /// Makes the open list again when `changed`.
    pub(crate) fn refill(
        &mut self,
        changed: bool,
        ui: &mut Interface,
        vfs: &Vfs,
        cat: &Catalogue,
        log: &mut Vec<String>,
    ) {
        if let (true, Some(p)) = (changed, self.open) {
            self.fill(ui, vfs, cat, p, log);
        }
    }

    /// The sex of the staff the Hire panel lists.
    pub(crate) fn set_hire(&mut self, male: bool) {
        self.hire_male = male;
    }

    /// A staff member was hired: the list of the open Hire panel is made again for the new sex.
    pub(crate) fn rehire(
        &mut self,
        ui: &mut Interface,
        vfs: &Vfs,
        cat: &Catalogue,
        log: &mut Vec<String>,
    ) {
        if self.open == Some(HIRE_PANEL) {
            self.fill(ui, vfs, cat, HIRE_PANEL, log);
        }
    }
}
