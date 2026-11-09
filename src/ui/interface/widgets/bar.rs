//! Scroll bars and the owners that name them.

use super::*;

impl Interface {
    // ---- Owners and their bars

    /// The scroll bar a text or a list names.
    pub(super) fn bar_of(&self, owner: usize) -> Option<usize> {
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
    pub(super) fn owner_of(&self, bar: usize) -> Option<usize> {
        let id = self.tree.elements[bar].id;
        (0..self.tree.elements.len()).find(|&o| {
            self.bar_of(o)
                .is_some_and(|b| self.tree.elements[b].id == id)
        })
    }

    /// The heights of the rows of list `i`, and what fits in it.
    pub(super) fn list_heights(&mut self, i: usize) -> Heights {
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
    pub(super) fn extent(&mut self, owner: usize) -> Option<(usize, usize)> {
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

    pub(super) fn top_mut(&mut self, owner: usize) -> Option<&mut usize> {
        match &mut self.tree.elements[owner].kind {
            Kind::ListBox(l) => Some(&mut l.rows.top),
            Kind::Text(t) => Some(&mut t.top),
            Kind::ScrollingRegion(r) => Some(&mut r.top),
            _ => None,
        }
    }

    /// Brings the range and the position of the owner's bar in line with what the owner holds:
    /// from 0 to the lines or rows beyond the first screenful, a page being a screenful less one.
    pub(in crate::ui::interface) fn sync_owner(&mut self, owner: usize) {
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
    pub(in crate::ui::interface) fn scroll_owner(&mut self, owner: usize, top: i64) {
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

    pub(super) fn scrolled(&mut self, i: usize, position: i64) {
        let id = self.id(i);
        let position = position.clamp(0, i64::from(i32::MAX)) as i32;
        self.event(format!("scroll {id} {position}"));
        self.actions.push(Action::Scroll(id, position));
    }

    // ---- Scroll bars (4.7)

    pub(super) fn with_bar<R>(&mut self, i: usize, f: impl FnOnce(&mut Bar) -> R) -> Option<R> {
        match &mut self.tree.elements[i].kind {
            Kind::ScrollBar(s) => Some(f(&mut s.bar)),
            _ => None,
        }
    }

    /// Puts the scroll bar `id` at `position` (held to its range) without telling anyone; `false`
    /// if it is not a scroll bar.
    pub(crate) fn set_slider(&mut self, id: i32, position: i64) -> bool {
        let Some(i) = self.tree.find(id) else {
            return false;
        };
        let moved = self.with_bar(i, |b| b.set_position(position));
        if moved == Some(true) {
            self.flag(i, DIRTY, true);
        }
        moved.is_some()
    }

    /// The part of bar `i` at a point of the screen.
    pub(super) fn spot(&self, i: usize, (x, y): (i32, i32)) -> Option<Spot> {
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
    pub(super) fn set_bar(&mut self, i: usize, position: i64) {
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
    pub(super) fn bar_step(&mut self, i: usize, spot: &Spot) {
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
    pub(in crate::ui::interface) fn bar_down(&mut self, i: usize, at: (i32, i32)) {
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
    pub(in crate::ui::interface) fn bar_dragged(&mut self, i: usize, at: (i32, i32)) {
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
    pub(in crate::ui::interface) fn bar_up(&mut self, i: usize) {
        self.with_bar(i, |b| (b.held, b.dragged) = (None, None));
        self.input.bar_repeat = None;
        self.flag(i, DIRTY, true);
    }

    /// The part under the pointer is the bar's hover part.
    pub(super) fn bar_hover(&mut self, i: usize, at: (i32, i32)) {
        let part = self.spot(i, at).map(|s| s.part);
        if self.with_bar(i, |b| std::mem::replace(&mut b.hover, part)) != Some(part) {
            self.flag(i, DIRTY, true);
        }
    }

    /// A held arrow or track page repeats 30 ms after the last time, once 500 ms have passed,
    /// while the pointer is on the same part; if it is not, it looks again 1 ms later.
    pub(in crate::ui::interface) fn bar_repeat(&mut self) {
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
        let on = super::super::input::inside(self.tree.rect(i), pointer);
        match self.spot(i, pointer).filter(|s| on && s.part == part) {
            Some(spot) => {
                self.bar_step(i, &spot);
                self.input.bar_repeat = Some((i, part, self.input.clock + REPEAT));
            }
            None => self.input.bar_repeat = Some((i, part, self.input.clock + 1)),
        }
    }
}
