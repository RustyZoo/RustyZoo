//! Scroll bars, list boxes and text fields at work: what the mouse and the keys do to them, and
//! what the game's code sets and reads (ui layout 4.5, 4.7, 4.8, 7.8, 7.9).
//!
//! A scroll bar that a text or a list names scrolls its owner, and the owner keeps the bar's
//! range and position; a bar with no owner only tells where it is. Every change of position is
//! an `Action::Scroll` and a `scroll` line of the log.

use super::{Action, Interface};
use crate::app::Key;
use crate::text::from_cp1252;
use crate::ui::draw::{Strings, element_text};
use crate::ui::layout::{
    DIRTY, DISABLED, FOCUSED, HIDDEN, Kind, Picture, READ_ONLY, SELECT_ON_KEYS, SELECTED,
    TAKES_FOCUS,
};
use crate::ui::widgets::{Bar, Caret, EditKey, Edited, Heights, Limits, ListKey, Part, Row, edit};

/// Rows a notch of the wheel scrolls (a notch is 120 units of the original's wheel).
const WHEEL_ROWS: i64 = 2;
/// Milliseconds until a held button, arrow or track page repeats, and between repeats.
pub(super) const REPEAT_FIRST: u64 = 500;
pub(super) const REPEAT: u64 = 30;

/// What lies under a point of a scroll bar.
struct Spot {
    part: Part,
    /// The distance along the bar from its start.
    along: i32,
}

impl Interface {
    // ---- What the game's code sets and reads

    /// Gives the text of a text, a text field or a button; `false` if there is no such element.
    /// A field's caret goes to the end of it, a scrolled text back to its first line.
    pub fn set_text(&mut self, id: i32, text: &[u8]) -> bool {
        let Some(i) = self.tree.find(id) else {
            return false;
        };
        self.put_text(i, text);
        true
    }

    /// Shows `text` in the message line (`ZTMessageQueue`) for the `timerDuration` the element
    /// has: a text already there is replaced and the count starts again. `false` if the tree has
    /// no message line.
    pub fn message(&mut self, text: &[u8]) -> bool {
        let Some(i) = self
            .tree
            .elements
            .iter()
            .position(|e| matches!(e.kind, Kind::MessageQueue))
        else {
            return false;
        };
        self.put_text(i, text);
        self.tree.elements[i].countdown = None;
        self.flag(i, HIDDEN, false);
        true
    }

    pub(super) fn put_text(&mut self, i: usize, text: &[u8]) {
        let e = &mut self.tree.elements[i];
        e.text = Some(text.to_vec());
        match &mut e.kind {
            Kind::EditableText(f) => f.caret = Caret::at_end(text.len()),
            Kind::Text(t) => t.top = 0,
            _ => {}
        }
        self.flag(i, DIRTY, true);
        self.sync_owner(i);
    }

    /// Gives an element the colour of its text; `false` if there is no such element.
    pub fn set_fore_color(&mut self, id: i32, color: crate::ui::layout::Rgb) -> bool {
        let Some(i) = self.tree.find(id) else {
            return false;
        };
        self.tree.elements[i].look.fore_color = color;
        self.flag(i, DIRTY, true);
        true
    }

    /// The text of an element as it is shown now; `None` if there is no such element.
    pub fn text(&self, id: i32) -> Option<Vec<u8>> {
        let e = &self.tree.elements[self.tree.find(id)?];
        Some(element_text(e, &self.strings))
    }

    /// Shows `picture` in the picture element `id`, which takes its size; none hides the element.
    /// `false` if there is no such element.
    pub fn set_picture(&mut self, id: i32, picture: Option<Picture>) -> bool {
        let Some(i) = self
            .tree
            .find(id)
            .filter(|&i| matches!(self.tree.elements[i].kind, Kind::Image))
        else {
            return false;
        };
        let size = picture.map(|p| self.tree.pictures.size(p));
        let e = &mut self.tree.elements[i];
        match (picture, size) {
            (Some(p), Some((w, h))) => {
                let animated = matches!(p, Picture::Animation(_));
                e.look.animation = if animated { vec![Some(p)] } else { vec![] };
                e.look.normal = (!animated).then_some(p);
                if let Some(placed) = &mut e.placed {
                    (placed.dx, placed.dy) = (w, h);
                }
                e.state &= !HIDDEN;
            }
            _ => e.state |= HIDDEN,
        }
        self.dirty = true;
        true
    }

    /// Hides or shows an element on its own; `false` if there is none.
    pub fn set_hidden(&mut self, id: i32, hidden: bool) -> bool {
        let Some(i) = self.tree.find(id) else {
            return false;
        };
        self.flag(i, HIDDEN, hidden);
        true
    }

    /// Selects or deselects the button `id` without running its actions, and enables or disables
    /// it; `false` if there is no such element.
    pub fn set_button(&mut self, id: i32, selected: bool, enabled: bool) -> bool {
        let Some(i) = self.tree.find(id) else {
            return false;
        };
        self.flag(i, SELECTED, selected);
        self.flag(i, DISABLED, !enabled);
        true
    }

    /// The text resource of the game under this name (empty if there is none).
    pub fn text_entry(&self, name: &[u8]) -> Vec<u8> {
        Strings::text_entry(&self.strings, name)
    }

    /// Gives a list box its rows. The first row is shown and none is selected. `false` if the
    /// element is not a list box.
    pub fn set_rows(&mut self, id: i32, rows: Vec<Row>) -> bool {
        let Some(i) = self.tree.find(id) else {
            return false;
        };
        let Kind::ListBox(list) = &mut self.tree.elements[i].kind else {
            return false;
        };
        list.rows = crate::ui::widgets::Rows {
            rows,
            ..Default::default()
        };
        self.flag(i, DIRTY, true);
        self.sync_owner(i);
        true
    }

    /// Selects a row of a list box, or none, and scrolls to show it. Does not run the select
    /// action. `false` if the element is not a list box or has no such row.
    pub fn select_row(&mut self, id: i32, row: Option<usize>) -> bool {
        let Some(i) = self.tree.find(id) else {
            return false;
        };
        let fit = self.list_heights(i);
        let Kind::ListBox(list) = &mut self.tree.elements[i].kind else {
            return false;
        };
        if row.is_some_and(|r| r >= list.rows.rows.len()) {
            return false;
        }
        list.rows.selected = row;
        list.rows.show_selection(&fit);
        self.flag(i, DIRTY, true);
        self.sync_owner(i);
        true
    }

    /// The selected row of a list box.
    pub fn selected_row(&self, id: i32) -> Option<usize> {
        match &self.tree.elements[self.tree.find(id)?].kind {
            Kind::ListBox(list) => list.rows.selected,
            _ => None,
        }
    }

    /// Gives the keyboard focus to a text field or a list box; `false` if the element is not one
    /// or cannot take it.
    pub fn focus(&mut self, id: i32) -> bool {
        match self.tree.find(id).filter(|&i| self.takes_focus(i)) {
            Some(i) => {
                self.set_focus(Some(i));
                true
            }
            None => false,
        }
    }

    // ---- Owners and their bars

    /// The scroll bar a text or a list names.
    fn bar_of(&self, owner: usize) -> Option<usize> {
        let id = match &self.tree.elements[owner].kind {
            Kind::Text(t) => t.scroll_bar,
            Kind::ListBox(l) => l.scroll_bar,
            Kind::ScrollingRegion(r) => r.scroll_bar,
            _ => return None,
        };
        self.tree
            .find(id)
            .filter(|&b| matches!(self.tree.elements[b].kind, Kind::ScrollBar(_)))
    }

    /// The text or list a bar belongs to.
    fn owner_of(&self, bar: usize) -> Option<usize> {
        let id = self.tree.elements[bar].id;
        (0..self.tree.elements.len()).find(|&o| {
            self.bar_of(o)
                .is_some_and(|b| self.tree.elements[b].id == id)
        })
    }

    /// The heights of the rows of list `i`, and what fits in it.
    fn list_heights(&mut self, i: usize) -> Heights {
        let Interface {
            tree,
            painter,
            strings,
            ..
        } = self;
        let e = &tree.elements[i];
        match &e.kind {
            Kind::ListBox(l) => painter.row_heights(&tree.pictures, e, l, &*strings),
            _ => Heights::default(),
        }
    }

    /// The largest first line or row of a scrolled owner, and how many it shows from the first
    /// one it shows now.
    fn extent(&mut self, owner: usize) -> Option<(usize, usize)> {
        match &self.tree.elements[owner].kind {
            Kind::ListBox(l) => {
                let top = l.rows.top;
                let fit = self.list_heights(owner);
                Some((fit.max_top(), fit.fit_from(top)))
            }
            Kind::ScrollingRegion(_) => self.region_extent(owner),
            Kind::Text(_) => {
                let Interface {
                    tree,
                    painter,
                    strings,
                    ..
                } = self;
                let e = &tree.elements[owner];
                let (total, visible) =
                    painter.text_lines(e, &*strings, &element_text(e, &*strings));
                Some((total.saturating_sub(visible), visible))
            }
            _ => None,
        }
    }

    fn top_mut(&mut self, owner: usize) -> Option<&mut usize> {
        match &mut self.tree.elements[owner].kind {
            Kind::ListBox(l) => Some(&mut l.rows.top),
            Kind::Text(t) => Some(&mut t.top),
            Kind::ScrollingRegion(r) => Some(&mut r.top),
            _ => None,
        }
    }

    /// Brings the range and the position of the owner's bar in line with what the owner holds:
    /// from 0 to the lines or rows beyond the first screenful, a page being a screenful less one.
    pub(super) fn sync_owner(&mut self, owner: usize) {
        let Some((max_top, visible)) = self.extent(owner) else {
            return;
        };
        let Some(top) = self.top_mut(owner) else {
            return;
        };
        *top = (*top).min(max_top);
        let top = *top;
        if let Some(bar) = self.bar_of(owner)
            && let Kind::ScrollBar(s) = &mut self.tree.elements[bar].kind
        {
            let clamp = |n: usize| i32::try_from(n).unwrap_or(i32::MAX);
            s.bar
                .set_range(0, clamp(max_top), clamp(visible.saturating_sub(1)));
            s.bar.position = clamp(top);
            self.flag(bar, DIRTY, true);
        }
    }

    /// Scrolls the owner so that `top` is its first line or row, within what it holds. Tells the
    /// game's code where the bar is, or the owner if it has none, when that moved it.
    pub(super) fn scroll_owner(&mut self, owner: usize, top: i64) {
        let Some((max_top, _)) = self.extent(owner) else {
            return;
        };
        let to = top.clamp(0, max_top as i64) as usize;
        let Some(first) = self.top_mut(owner) else {
            return;
        };
        if std::mem::replace(first, to) == to {
            return;
        }
        self.place_region(owner);
        self.sync_owner(owner);
        self.flag(owner, DIRTY, true);
        let shown = self.bar_of(owner).unwrap_or(owner);
        self.scrolled(shown, to as i64);
    }

    fn scrolled(&mut self, i: usize, position: i64) {
        let id = self.id(i);
        let position = position.clamp(0, i64::from(i32::MAX)) as i32;
        self.event(format!("scroll {id} {position}"));
        self.actions.push(Action::Scroll(id, position));
    }

    // ---- Scroll bars (4.7)

    fn with_bar<R>(&mut self, i: usize, f: impl FnOnce(&mut Bar) -> R) -> Option<R> {
        match &mut self.tree.elements[i].kind {
            Kind::ScrollBar(s) => Some(f(&mut s.bar)),
            _ => None,
        }
    }

    /// The part of bar `i` at a point of the screen.
    fn spot(&self, i: usize, (x, y): (i32, i32)) -> Option<Spot> {
        let Kind::ScrollBar(s) = &self.tree.elements[i].kind else {
            return None;
        };
        let (lengths, vertical) = self.tree.bar_lengths(i);
        let (sx, sy) = self.tree.screen(i);
        let along = if vertical {
            y.saturating_sub(sy)
        } else {
            x.saturating_sub(sx)
        };
        Some(Spot {
            part: s.bar.part_at(&lengths, along),
            along,
        })
    }

    /// Moves a bar to a position and tells its owner.
    fn set_bar(&mut self, i: usize, position: i64) {
        if self.with_bar(i, |b| b.set_position(position)) != Some(true) {
            return;
        }
        self.flag(i, DIRTY, true);
        let now = self.with_bar(i, |b| i64::from(b.position)).unwrap_or(0);
        match self.owner_of(i) {
            Some(owner) => self.scroll_owner(owner, now),
            None => self.scrolled(i, now),
        }
    }

    /// One step of a held part: an arrow moves by 1, the track by a page toward the pointer.
    fn bar_step(&mut self, i: usize, spot: &Spot) {
        let (lengths, _) = self.tree.bar_lengths(i);
        let delta = self.with_bar(i, |b| match spot.part {
            Part::Up => -1,
            Part::Down => 1,
            Part::Track => b.page_toward(&lengths, spot.along),
            Part::Thumb => 0,
        });
        if let Some(delta) = delta.filter(|&d| d != 0) {
            let at = self.with_bar(i, |b| i64::from(b.position)).unwrap_or(0);
            self.set_bar(i, at + delta);
        }
    }

    /// The left button goes down on a bar: an arrow or the track steps now and repeats while held
    /// on the same part; the thumb waits for the pointer to move.
    pub(super) fn bar_down(&mut self, i: usize, at: (i32, i32)) {
        let Some(spot) = self.spot(i, at) else { return };
        let (lengths, _) = self.tree.bar_lengths(i);
        self.input.grab = self
            .with_bar(i, |b| spot.along - b.thumb_start(&lengths))
            .unwrap_or(0);
        self.with_bar(i, |b| b.held = Some(spot.part));
        self.flag(i, DIRTY, true);
        if spot.part != Part::Thumb {
            self.bar_step(i, &spot);
            self.input.bar_repeat = Some((i, spot.part, self.input.clock + REPEAT_FIRST));
        }
    }

    /// The pointer moves with the left button held on a bar: the thumb follows it.
    pub(super) fn bar_dragged(&mut self, i: usize, at: (i32, i32)) {
        let Some(spot) = self.spot(i, at) else { return };
        if self.with_bar(i, |b| b.held) != Some(Some(Part::Thumb)) {
            return;
        }
        let (lengths, _) = self.tree.bar_lengths(i);
        let start = spot.along.saturating_sub(self.input.grab);
        // The thumb follows the pointer within its travel; the position is the one it has reached.
        let to = self.with_bar(i, |b| {
            let (first, last) = b.thumb_range(&lengths);
            b.dragged = Some(start.clamp(first, last));
            b.position_for(&lengths, start)
        });
        self.flag(i, DIRTY, true);
        if let Some(to) = to {
            self.set_bar(i, to);
        }
    }

    /// The left button goes up after a press on a bar.
    pub(super) fn bar_up(&mut self, i: usize) {
        self.with_bar(i, |b| (b.held, b.dragged) = (None, None));
        self.input.bar_repeat = None;
        self.flag(i, DIRTY, true);
    }

    /// The part under the pointer is the bar's hover part.
    fn bar_hover(&mut self, i: usize, at: (i32, i32)) {
        let part = self.spot(i, at).map(|s| s.part);
        if self.with_bar(i, |b| std::mem::replace(&mut b.hover, part)) != Some(part) {
            self.flag(i, DIRTY, true);
        }
    }

    /// A held arrow or track page repeats 30 ms after the last time, once 500 ms have passed,
    /// while the pointer is on the same part; if it is not, it looks again 1 ms later.
    pub(super) fn bar_repeat(&mut self) {
        let Some((i, part, due)) = self.input.bar_repeat else {
            return;
        };
        if self.tree.elements[i].state & (crate::ui::layout::HIDDEN | DISABLED) != 0 {
            self.input.bar_repeat = None;
            self.bar_up(i);
            return;
        }
        if self.input.clock < due {
            return;
        }
        let pointer = self.input.pointer;
        let on = super::input::inside(self.tree.rect(i), pointer);
        match self.spot(i, pointer).filter(|s| on && s.part == part) {
            Some(spot) => {
                self.bar_step(i, &spot);
                self.input.bar_repeat = Some((i, part, self.input.clock + REPEAT));
            }
            None => self.input.bar_repeat = Some((i, part, self.input.clock + 1)),
        }
    }

    // ---- List boxes (4.8)

    /// The row of list `i` under a point of the screen.
    fn list_row_at(&mut self, i: usize, (_, y): (i32, i32)) -> Option<usize> {
        let fit = self.list_heights(i);
        let top = self
            .tree
            .screen(i)
            .1
            .saturating_add(self.tree.elements[i].y_border);
        match &self.tree.elements[i].kind {
            Kind::ListBox(l) => fit.row_at(l.rows.top, y.saturating_sub(top)),
            _ => None,
        }
    }

    fn list_select(&mut self, i: usize, row: Option<usize>) {
        let Kind::ListBox(l) = &mut self.tree.elements[i].kind else {
            return;
        };
        if std::mem::replace(&mut l.rows.selected, row) == row {
            return;
        }
        self.flag(i, DIRTY, true);
        let id = self.id(i);
        let name = row.map_or("none".to_string(), |r| r.to_string());
        self.event(format!("row {id} {name}"));
    }

    /// The select action of a list: its id and the selected row, -1 for none or a disabled one.
    fn list_action(&mut self, i: usize) {
        let Kind::ListBox(l) = &self.tree.elements[i].kind else {
            return;
        };
        let (id, row) = (self.id(i), l.rows.action_value());
        self.event(format!("select {id} {row}"));
        self.actions.push(Action::Row(id, row));
    }

    /// The left button goes down on a list: the row under the pointer is selected, or none
    /// below the last row; with `toggleselect`, pressing the selected row selects none.
    pub(super) fn list_down(&mut self, i: usize, at: (i32, i32)) {
        let row = self.list_row_at(i, at);
        let Kind::ListBox(l) = &self.tree.elements[i].kind else {
            return;
        };
        let row = match l.toggle_select && row == l.rows.selected {
            true => None,
            false => row,
        };
        self.list_select(i, row);
    }

    /// The pointer moves with the button held: the selection follows it.
    pub(super) fn list_dragged(&mut self, i: usize, at: (i32, i32)) {
        let row = self.list_row_at(i, at);
        self.list_select(i, row);
    }

    /// The button goes up after a press on a list: the select action runs.
    pub(super) fn list_up(&mut self, i: usize) {
        self.list_action(i);
    }

    fn list_hover(&mut self, i: usize, at: (i32, i32)) {
        let row = match self.tree.elements[i].state & READ_ONLY {
            0 => self.list_row_at(i, at),
            _ => None,
        };
        let Kind::ListBox(l) = &mut self.tree.elements[i].kind else {
            return;
        };
        if std::mem::replace(&mut l.rows.hover, row) != row {
            self.flag(i, DIRTY, true);
        }
    }

    /// The keys of a focused list; whether it handled the key.
    fn list_key(&mut self, i: usize, key: Key) -> bool {
        let key = match key {
            Key::Up => ListKey::Up,
            Key::Down => ListKey::Down,
            Key::PageUp => ListKey::PageUp,
            Key::PageDown => ListKey::PageDown,
            Key::Home => ListKey::Home,
            Key::End => ListKey::End,
            _ => return false,
        };
        let fit = self.list_heights(i);
        let Kind::ListBox(l) = &mut self.tree.elements[i].kind else {
            return false;
        };
        let (before, top) = (l.rows.selected, l.rows.top);
        l.rows.select_by_key(key, &fit);
        let (after, now) = (l.rows.selected, l.rows.top);
        self.flag(i, DIRTY, true);
        self.sync_owner(i);
        if after != before {
            let id = self.id(i);
            let name = after.map_or("none".to_string(), |r| r.to_string());
            self.event(format!("row {id} {name}"));
        }
        if now != top {
            let shown = self.bar_of(i).unwrap_or(i);
            self.scrolled(shown, now as i64);
        }
        if self.tree.elements[i].state & SELECT_ON_KEYS != 0 {
            self.list_action(i);
        }
        true
    }

    // ---- Text fields (4.5)

    pub(super) fn takes_focus(&self, i: usize) -> bool {
        let e = &self.tree.elements[i];
        e.state & DISABLED == 0
            && (e.state & TAKES_FOCUS != 0
                || matches!(e.kind, Kind::EditableText(_))
                || (matches!(e.kind, Kind::ListBox(_)) && e.state & READ_ONLY == 0))
    }

    /// Moves the keyboard focus to `to` (or to nothing). A field that gets it remembers its text
    /// for Escape and selects all of it, the caret at the end. Whether `to` was not focused
    /// before.
    pub(super) fn set_focus(&mut self, to: Option<usize>) -> bool {
        for i in 0..self.tree.elements.len() {
            if Some(i) != to && self.tree.elements[i].state & FOCUSED != 0 {
                self.flag(i, FOCUSED, false);
                self.flag(i, DIRTY, true);
                let id = self.id(i);
                self.event(format!("unfocus {id}"));
                if matches!(self.tree.elements[i].kind, Kind::EditableText(_)) {
                    self.actions.push(Action::Edited(id));
                }
            }
        }
        let Some(t) = to else { return false };
        if self.tree.elements[t].state & FOCUSED != 0 {
            return false;
        }
        self.flag(t, FOCUSED, true);
        self.flag(t, DIRTY, true);
        let id = self.id(t);
        self.event(format!("focus {id}"));
        if matches!(self.tree.elements[t].kind, Kind::EditableText(_)) {
            let text = self.field_text(t);
            let len = text.len();
            let e = &mut self.tree.elements[t];
            e.text = Some(text.clone());
            if let Kind::EditableText(f) = &mut e.kind {
                f.saved = text;
                f.caret = Caret { at: len, anchor: 0 };
            }
        }
        true
    }

    fn field_text(&self, i: usize) -> Vec<u8> {
        element_text(&self.tree.elements[i], &self.strings)
    }

    /// The byte of a field's text that a point of the screen is nearest to.
    fn caret_at(&mut self, i: usize, text: &[u8], (x, _): (i32, i32)) -> usize {
        let left = self.tree.screen(i).0;
        let Interface {
            tree,
            painter,
            strings,
            ..
        } = self;
        painter.caret_at(&tree.elements[i], &*strings, text, x.saturating_sub(left))
    }

    /// The left button goes down on a field that already has the focus: the caret goes there. (The
    /// press that gives it the focus only selects all of its text.)
    pub(super) fn field_down(&mut self, i: usize, at: (i32, i32), gained: bool) {
        if !gained {
            let text = self.field_text(i);
            let caret = self.caret_at(i, &text, at);
            self.move_caret(i, caret, false);
        }
    }

    /// The pointer moves with the button held on a field: the selection runs from where the
    /// press put the caret to the pointer.
    pub(super) fn field_dragged(&mut self, i: usize, at: (i32, i32)) {
        let text = self.field_text(i);
        let caret = self.caret_at(i, &text, at);
        self.move_caret(i, caret, true);
    }

    fn move_caret(&mut self, i: usize, to: usize, extend: bool) {
        if let Kind::EditableText(f) = &mut self.tree.elements[i].kind {
            let before = f.caret;
            f.caret.at = to;
            if !extend {
                f.caret.anchor = to;
            }
            if f.caret != before {
                self.flag(i, DIRTY, true);
            }
        }
    }

    /// Whether a text does not fit on the line of field `i`, inside its borders.
    fn too_wide(&mut self, i: usize, text: &[u8]) -> bool {
        let Interface {
            tree,
            painter,
            strings,
            ..
        } = self;
        let e = &tree.elements[i];
        let room = e.placed.map_or(0, |p| p.dx) - e.x_border.saturating_mul(2);
        painter.text_width(e, &*strings, text) > room
    }

    /// Tells the log what a field holds now.
    fn field_changed(&mut self, i: usize) {
        self.flag(i, DIRTY, true);
        let id = self.id(i);
        let text: String = self.field_text(i).iter().map(|&b| from_cp1252(b)).collect();
        self.event(format!("text {id} {text}"));
    }

    /// A key for the focused field; whether it handled it. Enter ends the edit and keeps the
    /// text, Escape ends it with the text the field had when it got the focus; both take the
    /// focus away and pass the key on to the field's layout.
    fn field_key(&mut self, i: usize, key: Key, shift: bool) -> bool {
        let edit_key = match key {
            Key::Left => EditKey::Left,
            Key::Right => EditKey::Right,
            Key::Home => EditKey::Home,
            Key::End => EditKey::End,
            Key::Delete => EditKey::Delete,
            Key::Backspace => EditKey::Backspace,
            Key::Enter | Key::Escape => {
                if key == Key::Escape
                    && let Kind::EditableText(f) = &self.tree.elements[i].kind
                {
                    let saved = f.saved.clone();
                    self.put_text(i, &saved);
                    self.field_changed(i);
                }
                self.set_focus(None);
                let parent = self.tree.elements[i].parent;
                let layout = parent.map_or(0, |p| self.tree.elements[p].id);
                self.event(format!("key {layout} {}", key.name()));
                self.actions.push(Action::Key(layout, key));
                // In a question the key answers it.
                if parent.is_some() && parent == self.modal {
                    self.dialog_key(key);
                }
                return true;
            }
            // The letters Y and N come as typed characters too: the field takes the character.
            Key::Y | Key::N => return true,
            _ => return false,
        };
        self.edit_field(i, |text, caret, limits| {
            edit(text, caret, edit_key, shift, limits)
        });
        true
    }

    /// A typed character (a Windows-1252 byte) for the focused field; whether it handled it.
    pub fn char(&mut self, byte: u8) -> bool {
        let Some(i) = self
            .focused()
            .filter(|&i| matches!(self.tree.elements[i].kind, Kind::EditableText(_)))
        else {
            return false;
        };
        self.edit_field(i, |text, caret, limits| {
            edit(text, caret, EditKey::Char(byte), false, limits)
        });
        true
    }

    /// Runs an edit on the text and caret of field `i`, and tells what came of it.
    fn edit_field(&mut self, i: usize, f: impl FnOnce(&mut Vec<u8>, &mut Caret, Limits) -> Edited) {
        let mut text = self.field_text(i);
        let Kind::EditableText(field) = &mut self.tree.elements[i].kind else {
            return;
        };
        let limits = Limits {
            numeric: field.numeric,
            limit: usize::try_from(field.char_limit).unwrap_or(0),
        };
        let mut caret = field.caret;
        let before = caret;
        let old = text.clone();
        let mut done = f(&mut text, &mut caret, limits);
        // A character that would make the text wider than the field is refused.
        if done == Edited::Changed && text.len() > old.len() && self.too_wide(i, &text) {
            done = Edited::Ignored;
        }
        match done {
            Edited::Changed => {
                let Kind::EditableText(field) = &mut self.tree.elements[i].kind else {
                    return;
                };
                field.caret = caret;
                self.tree.elements[i].text = Some(text);
                self.field_changed(i);
            }
            Edited::Moved if caret != before => {
                if let Kind::EditableText(field) = &mut self.tree.elements[i].kind {
                    field.caret = caret;
                }
                self.flag(i, DIRTY, true);
            }
            Edited::Ignored => {
                let id = self.id(i);
                self.event(format!("beep {id}"));
            }
            Edited::Moved => {}
        }
    }

    // ---- Keys and the wheel

    /// The element that has the keyboard focus.
    fn focused(&self) -> Option<usize> {
        (0..self.tree.elements.len()).find(|&i| self.tree.elements[i].state & FOCUSED != 0)
    }

    /// A key of the interface, `shift` meaning the Shift key is held: the focused text field or
    /// list box reads it. Whether it handled it.
    pub fn key(&mut self, key: Key, shift: bool) -> bool {
        let handled = match self.focused() {
            Some(i) => match self.tree.elements[i].kind {
                Kind::EditableText(_) => self.field_key(i, key, shift),
                Kind::ListBox(_) => self.list_key(i, key),
                _ => false,
            },
            None => false,
        };
        // An open question gets what the focused element leaves, and nothing goes further.
        if !handled && self.modal.is_some() {
            self.dialog_key(key);
            return true;
        }
        handled
    }

    /// The wheel turns by `notches` over element `i` (positive: away from the player): a list, a
    /// text or a scrolling region with a bar scrolls two rows per notch, up for a positive number,
    /// when it holds more than it shows (without a bar it does not scroll).
    pub(super) fn wheel(&mut self, i: usize, notches: i32) {
        // The wheel over a button of a scrolling region turns the region.
        let i = self.region_of(i).unwrap_or(i);
        let scrolls = match &self.tree.elements[i].kind {
            Kind::ListBox(_) | Kind::Text(_) | Kind::ScrollingRegion(_) => self.bar_of(i).is_some(),
            _ => false,
        };
        let Some((max_top, _)) = self.extent(i).filter(|_| scrolls) else {
            return;
        };
        if max_top == 0 {
            return;
        }
        let top = self.top_mut(i).map_or(0, |t| *t as i64);
        self.scroll_owner(i, top - WHEEL_ROWS * i64::from(notches));
    }

    // ---- Hover

    /// The pointer is at `at` over element `target` (or none): a bar highlights its part, a list
    /// its row.
    pub(super) fn widget_hover(&mut self, target: Option<usize>, at: (i32, i32)) {
        let Some(t) = target else { return };
        match self.tree.elements[t].kind {
            Kind::ScrollBar(_) => self.bar_hover(t, at),
            Kind::ListBox(_) => self.list_hover(t, at),
            _ => {}
        }
    }

    /// The pointer left element `i`.
    pub(super) fn widget_unhover(&mut self, i: usize) {
        let had = match &mut self.tree.elements[i].kind {
            Kind::ScrollBar(s) => s.bar.hover.take().is_some(),
            Kind::ListBox(l) => l.rows.hover.take().is_some(),
            _ => false,
        };
        if had {
            self.flag(i, DIRTY, true);
        }
    }
}

#[cfg(test)]
mod tests;
