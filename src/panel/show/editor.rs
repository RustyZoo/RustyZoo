//! The Show Script tab's trick editor (information panels 19.9): the list of the tricks a species
//! can do and the list of the tricks in its show, the four buttons that move tricks between and
//! in them, the benefit shown, and the script the list is written to. What it does to the world
//! is [`crate::show::edit`].

use super::{
    ADD, AVAILABLE, BENEFIT, EARLIER, FACE, FREQUENCY_BUTTON, LATER, Panel, REMOVE, SHOWN,
};
use crate::entity::{Catalogue, EntityType};
use crate::show::edit::{self, Avail, Recalc};
use crate::text::to_cp1252;
use crate::ui::interface::Interface;
use crate::ui::widgets::Row;
use crate::vfs::Vfs;
use crate::world::World;
use crate::zoo::Item;

/// The message of a full show list (19.9.6) and its kind.
const FULL: i32 = 11493;
const FULL_KIND: i32 = 3;

/// What the editor keeps between its calls (19.9.2).
#[derive(Debug, Default)]
pub(super) struct Editor {
    /// `V`: the tricks of the available list, in the order of its rows, and whether each is on.
    avail: Vec<Avail>,
    /// The tricks of the rows of the show list, in order.
    shown: Vec<Item>,
    /// `F`: the show list changed and is not written yet.
    pub(super) changed: bool,
    /// Layout 42 was shown at the last look.
    script_shown: bool,
}

/// The row of trick `it` (19.9.3): its name, icon and help.
fn row(ui: &mut Interface, vfs: &Vfs, it: &Item) -> Row {
    let mut picture = |name: &str| {
        (!name.is_empty())
            .then(|| ui.tree.picture(vfs, name))
            .flatten()
    };
    let (icon, grayed_icon) = (picture(&it.icons[0]), picture(&it.icons[1]));
    Row {
        text: to_cp1252(&ui.string(i32::from(it.id))),
        help: it.help[0] as i32,
        disabled: false,
        icon,
        grayed_icon,
    }
}

/// The row of an available trick, grayed with its grayed help when it is off.
fn avail_row(ui: &mut Interface, vfs: &Vfs, a: &Avail) -> Row {
    let mut row = row(ui, vfs, &a.item);
    if !a.ok {
        (row.disabled, row.help) = (true, a.item.help[1] as i32);
    }
    row
}

impl Panel {
    /// The type of the panel's species.
    fn species_type<'a>(&self, cat: &'a Catalogue) -> Option<&'a EntityType> {
        edit::species_type(cat, self.species?)
    }

    /// Empties both lists and the editor's state (`fill_exhibit`, 19.9.4 point 5).
    pub(super) fn clear_tricks(&mut self, ui: &mut Interface) {
        for list in [AVAILABLE, SHOWN] {
            ui.set_rows(list, Vec::new());
        }
        self.editor.avail.clear();
        self.editor.shown.clear();
    }

    /// `fill_trick_lists` (19.9.4), the call of `set_species`.
    pub(super) fn fill_tricks(
        &mut self,
        ui: &mut Interface,
        world: &mut World,
        cat: &Catalogue,
        vfs: &Vfs,
    ) {
        let (Some(x), Some(s)) = (self.exhibit, self.species_type(cat)) else {
            return;
        };
        let filled = edit::fill(world, cat, x, s);
        let rows: Vec<Row> = filled.avail.iter().map(|a| avail_row(ui, vfs, a)).collect();
        ui.set_rows(AVAILABLE, rows);
        let rows: Vec<Row> = filled.shown.iter().map(|i| row(ui, vfs, i)).collect();
        ui.set_rows(SHOWN, rows);
        self.editor.changed |= filled.changed;
        self.editor.avail = filled.avail;
        self.editor.shown = filled.shown;
        self.recalc(ui, world, cat);
    }

    /// `recalc(none)` (19.9.8): the benefit of the show list.
    fn recalc(&self, ui: &mut Interface, world: &mut World, cat: &Catalogue) {
        let ids: Vec<u16> = self.editor.shown.iter().map(|i| i.id).collect();
        let found = match self.species_type(cat) {
            Some(s) => edit::recalc(world, s, &ids),
            None => Recalc {
                count: ids.len(),
                benefit: 0,
            },
        };
        benefit(ui, found);
    }

    /// `copy_list_to_script` (19.9.7): the show list is written to the species' script. Returns
    /// whether the script has items.
    pub(super) fn write_tricks(
        &mut self,
        ui: &mut Interface,
        world: &mut World,
        cat: &Catalogue,
    ) -> bool {
        let (Some(x), Some(s)) = (self.exhibit, self.species_type(cat)) else {
            return false;
        };
        let ids: Vec<u16> = self.editor.shown.iter().map(|i| i.id).collect();
        let (filled, found) = edit::write_script(world, x, s, &ids);
        benefit(ui, found);
        filled
    }

    /// Writes the list when it changed and is not written yet (`set_species`, hiding the window).
    pub(super) fn flush_tricks(&mut self, ui: &mut Interface, world: &mut World, cat: &Catalogue) {
        if self.editor.changed {
            self.write_tricks(ui, world, cat);
            self.editor.changed = false;
        }
    }

    /// The window is hidden (19.3 point 6): a changed list is written.
    pub(crate) fn hidden(&mut self, ui: &mut Interface, world: &mut World, cat: &Catalogue) {
        self.flush_tricks(ui, world, cat);
    }

    /// Layout 42 is shown (19.3 point 5): the frequency button shows the selected frequency, the
    /// four buttons are off and nothing is changed. Called at every look, it acts when the layout
    /// has just been shown.
    pub(crate) fn look_script(&mut self, ui: &mut Interface) {
        let shown = ui.shown(super::SCRIPT_LAYOUT);
        if !std::mem::replace(&mut self.editor.script_shown, shown) && shown {
            if let Some(r) = ui.selected_row(super::FREQUENCY_LIST) {
                let text = ui.row_text(super::FREQUENCY_LIST, r).map(<[u8]>::to_vec);
                ui.set_text(FREQUENCY_BUTTON, &text.unwrap_or_default());
            }
            ui.set_hidden(super::FREQUENCY_LAYOUT, true);
            for button in super::TRICK_BUTTONS {
                ui.set_enabled(button, false);
            }
            self.editor.changed = false;
        }
    }

    /// The select action of the available list (19.9.5): add is on for a selected, enabled row.
    pub(crate) fn avail_select(&self, ui: &mut Interface, row: Option<usize>) {
        let on = row
            .and_then(|r| self.editor.avail.get(r))
            .is_some_and(|a| a.ok);
        ui.set_enabled(ADD, on && !self.disabled);
    }

    /// The select action of the show list (19.9.5): remove for a selected row, earlier unless on
    /// the first, later unless on the last.
    pub(crate) fn show_select(&self, ui: &mut Interface, row: Option<usize>) {
        let row = row.filter(|&r| r < self.editor.shown.len());
        let on = row.is_some() && !self.disabled;
        let last = self.editor.shown.len().checked_sub(1);
        ui.set_enabled(REMOVE, on);
        ui.set_enabled(EARLIER, on && row != Some(0));
        ui.set_enabled(LATER, on && row != last);
    }

    /// Both lists on their current selections (`enable_all`).
    pub(super) fn select_current(&self, ui: &mut Interface) {
        self.avail_select(ui, ui.selected_row(AVAILABLE));
        self.show_select(ui, ui.selected_row(SHOWN));
    }

    /// The rows of the show list again from the tricks held.
    fn refill_shown(&self, ui: &mut Interface, vfs: &Vfs) {
        let rows: Vec<Row> = self.editor.shown.iter().map(|i| row(ui, vfs, i)).collect();
        ui.refill_rows(SHOWN, rows);
    }

    /// The add button (19.9.6): the selected available trick goes to the end of the show list,
    /// unless the show is full. Returns the line to log.
    pub(crate) fn add(
        &mut self,
        ui: &mut Interface,
        world: &mut World,
        cat: &Catalogue,
        vfs: &Vfs,
    ) -> Option<String> {
        let a = ui.selected_row(AVAILABLE)?;
        let (rows, limit) = (self.editor.shown.len(), world.units.show.length);
        if limit > 0 && i64::try_from(rows) == Ok(i64::from(limit)) {
            ui.message(&to_cp1252(&ui.string(FULL)), FULL_KIND);
            return Some(format!("show message {FULL}"));
        }
        let (x, s) = (self.exhibit?, self.species_type(cat)?);
        let id = self.editor.avail.get(a)?.item.id;
        let t = edit::own_items(world, s).into_iter().find(|t| t.id == id)?;
        if !crate::show::trick::available(world, cat, x, s, &t) {
            return None;
        }
        self.editor.shown.push(t);
        self.refill_shown(ui, vfs);
        let last = self.editor.shown.len() - 1;
        ui.select_row(SHOWN, Some(last));
        self.show_select(ui, Some(last));
        self.recalc(ui, world, cat);
        self.editor.changed = true;
        None
    }

    /// The remove button (19.9.6).
    pub(crate) fn remove(
        &mut self,
        ui: &mut Interface,
        world: &mut World,
        cat: &Catalogue,
        vfs: &Vfs,
    ) -> Option<String> {
        let i = ui
            .selected_row(SHOWN)
            .filter(|&i| i < self.editor.shown.len())?;
        self.remove_row(ui, vfs, i);
        if i > 0 {
            ui.select_row(SHOWN, Some(i - 1));
        }
        self.show_select(ui, ui.selected_row(SHOWN));
        self.recalc(ui, world, cat);
        self.editor.changed = true;
        None
    }

    /// Takes row `i` out of the show list: a selection after it moves up by one, the selection
    /// on it stays on the same place (now the next row).
    fn remove_row(&mut self, ui: &mut Interface, vfs: &Vfs, i: usize) {
        let selected = ui.selected_row(SHOWN);
        self.editor.shown.remove(i);
        self.refill_shown(ui, vfs);
        let at = selected.map(|s| if s > i { s - 1 } else { s });
        ui.select_row(SHOWN, at.filter(|&s| s < self.editor.shown.len()));
    }

    /// The earlier and later buttons (19.9.6): the selected trick swaps places with its
    /// neighbour; the buttons, the benefit and the flag are updated even when nothing moved.
    pub(crate) fn shift(
        &mut self,
        ui: &mut Interface,
        world: &mut World,
        cat: &Catalogue,
        vfs: &Vfs,
        earlier: bool,
    ) -> Option<String> {
        let n = self.editor.shown.len();
        let i = ui.selected_row(SHOWN);
        let from = match (earlier, i) {
            (true, Some(i)) if 0 < i && i < n => Some(i - 1),
            (false, Some(i)) if i + 1 < n => Some(i),
            _ => None,
        };
        if let (Some(a), Some(i)) = (from, i) {
            self.editor.shown.swap(a, a + 1);
            self.refill_shown(ui, vfs);
            ui.select_row(SHOWN, Some(if earlier { i - 1 } else { i + 1 }));
        }
        self.show_select(ui, ui.selected_row(SHOWN));
        self.recalc(ui, world, cat);
        self.editor.changed = true;
        None
    }

    /// `update_available` (19.9.9 point 1): the available tricks are asked again, for a research
    /// that made a trick of species `researched` available (only when it is the panel's) or for
    /// a toy added to the map (none).
    pub(crate) fn update_available(
        &mut self,
        ui: &mut Interface,
        world: &World,
        cat: &Catalogue,
        vfs: &Vfs,
        researched: Option<u32>,
    ) {
        let (Some(x), Some(s)) = (self.exhibit, self.species_type(cat)) else {
            return;
        };
        if researched.is_some_and(|t| Some(t) != self.species) {
            return;
        }
        for now in edit::available_list(world, cat, x, s) {
            if let Some(a) = self
                .editor
                .avail
                .iter_mut()
                .find(|a| a.item.id == now.item.id)
            {
                a.ok = now.ok;
            }
        }
        let selected = ui.selected_row(AVAILABLE);
        let rows: Vec<Row> = (self.editor.avail.clone().iter())
            .map(|a| avail_row(ui, vfs, a))
            .collect();
        ui.refill_rows(AVAILABLE, rows);
        ui.select_row(AVAILABLE, selected);
        self.avail_select(ui, selected);
    }

    /// A toy was removed (19.9.9 point 2): the available tricks are asked again, then the tricks
    /// of the show list that are not valid any more go, from the last row to the first. The
    /// benefit keeps its old value.
    pub(crate) fn toy_removed(
        &mut self,
        ui: &mut Interface,
        world: &World,
        cat: &Catalogue,
        vfs: &Vfs,
    ) {
        self.update_available(ui, world, cat, vfs, None);
        let (Some(x), Some(s)) = (self.exhibit, self.species_type(cat)) else {
            return;
        };
        let own = edit::own_items(world, s);
        for i in (0..self.editor.shown.len()).rev() {
            let id = self.editor.shown[i].id;
            let gone = own
                .iter()
                .find(|t| t.id == id)
                .is_some_and(|t| !crate::show::trick::available(world, cat, x, s, t));
            if gone {
                self.remove_row(ui, vfs, i);
                self.editor.changed = true;
            }
        }
        self.show_select(ui, ui.selected_row(SHOWN));
    }

    /// The tricks of the lists for the log: the available ones (a dash for one that is off) and
    /// the ones in the show, with the benefit shown.
    pub(super) fn tricks_told(&self, ui: &Interface) -> String {
        let avail = self.editor.avail.iter();
        let avail: Vec<String> = avail
            .map(|a| format!("{}{}", if a.ok { "" } else { "-" }, a.item.id))
            .collect();
        let shown: Vec<String> = self.editor.shown.iter().map(|i| i.id.to_string()).collect();
        let text = String::from_utf8_lossy(&ui.text(BENEFIT).unwrap_or_default()).into_owned();
        format!(
            "tricks [{}] show [{}] benefit {text}",
            avail.join(" "),
            shown.join(" ")
        )
    }
}

/// Writes the benefit and its face (19.9.8): the frequency button is off with an empty list.
fn benefit(ui: &mut Interface, found: Recalc) {
    ui.set_enabled(FREQUENCY_BUTTON, found.count != 0);
    let b = found.benefit;
    let text = match b > 0 {
        true => format!("+{b}"),
        false => b.to_string(),
    };
    ui.set_text(BENEFIT, text.as_bytes());
    ui.set_image(
        FACE,
        match b {
            b if b > 0 => 0,
            0 => 1,
            _ => 2,
        },
    );
}
