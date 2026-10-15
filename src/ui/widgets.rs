//! The state and the rules of the scroll bar, the list box and the text field (ui layout 4.5,
//! 4.7, 4.8). Pure numbers and bytes: the tree, the pictures and the mouse are elsewhere.

/// A part of a scroll bar, along its length.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Part {
    Up,
    Down,
    Track,
    Thumb,
}

/// The lengths along a bar: the bar, its two arrows and its thumb, in pixels.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Lengths {
    pub bar: i32,
    pub up: i32,
    pub down: i32,
    pub thumb: i32,
}

/// The value of a scroll bar and what it shows of the mouse.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Bar {
    pub min: i32,
    pub max: i32,
    /// How far a press on the track moves.
    pub page: i32,
    pub position: i32,
    /// Where the thumb starts while it is dragged: it follows the pointer, within its travel,
    /// and snaps to the position when the button is released.
    pub dragged: Option<i32>,
    /// The part under the pointer, and the part held with the left button.
    pub hover: Option<Part>,
    pub held: Option<Part>,
}

impl Default for Bar {
    fn default() -> Bar {
        Bar {
            min: 0,
            max: 100,
            page: 10,
            position: 0,
            dragged: None,
            hover: None,
            held: None,
        }
    }
}

impl Bar {
    /// Sets the range (`max` is at least `min`, `page` at least 1) and keeps the position in it.
    pub fn set_range(&mut self, min: i32, max: i32, page: i32) {
        (self.min, self.max, self.page) = (min, max.max(min), page.max(1));
        self.position = self.position.clamp(self.min, self.max);
    }

    /// Whether there is anything to scroll: a bar whose range is empty is not shown.
    pub fn scrollable(&self) -> bool {
        self.max > self.min
    }

    /// Moves to `position` within the range; whether that changed it.
    pub fn set_position(&mut self, position: i64) -> bool {
        let to = position.clamp(i64::from(self.min), i64::from(self.max)) as i32;
        std::mem::replace(&mut self.position, to) != to
    }

    /// Moves by `delta`; whether that changed the position.
    pub fn step(&mut self, delta: i64) -> bool {
        self.set_position(i64::from(self.position) + delta)
    }

    /// How far the thumb can slide: the bar between the arrows less the thumb.
    fn travel(&self, l: &Lengths) -> i64 {
        (i64::from(l.bar) - i64::from(l.down) - i64::from(l.up) - i64::from(l.thumb)).max(0)
    }

    /// The first and the last place where the thumb can start: under the up arrow, and its
    /// travel further.
    pub fn thumb_range(&self, l: &Lengths) -> (i32, i32) {
        let last = i64::from(l.up) + self.travel(l);
        (l.up, last.min(i64::from(i32::MAX)) as i32)
    }

    /// Where the thumb starts along the bar: just after the up arrow, plus its share of the
    /// travel by the position.
    pub fn thumb_start(&self, l: &Lengths) -> i32 {
        let range = i64::from(self.max) - i64::from(self.min);
        let share = match range {
            0 => 0,
            _ => self.travel(l) * (i64::from(self.position) - i64::from(self.min)) / range,
        };
        (i64::from(l.up) + share).clamp(0, i64::from(i32::MAX)) as i32
    }

    /// The part at a distance `at` from the start of the bar.
    pub fn part_at(&self, l: &Lengths, at: i32) -> Part {
        let thumb = i64::from(self.thumb_start(l));
        let at = i64::from(at);
        if at < i64::from(l.up) {
            Part::Up
        } else if at >= i64::from(l.bar) - i64::from(l.down) {
            Part::Down
        } else if (thumb..thumb + i64::from(l.thumb)).contains(&at) {
            Part::Thumb
        } else {
            Part::Track
        }
    }

    /// The position for a thumb that starts at `start`: the one it has reached, rounded down.
    pub fn position_for(&self, l: &Lengths, start: i32) -> i64 {
        let (travel, range) = (self.travel(l), i64::from(self.max) - i64::from(self.min));
        if travel == 0 {
            return i64::from(self.min);
        }
        let from = (i64::from(start) - i64::from(l.up)).clamp(0, travel);
        i64::from(self.min) + from * range / travel
    }

    /// The step of a press at `at` on the track: a page toward it.
    pub fn page_toward(&self, l: &Lengths, at: i32) -> i64 {
        match at < self.thumb_start(l) {
            true => -i64::from(self.page),
            false => i64::from(self.page),
        }
    }
}

/// One row of a list box.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Row {
    pub text: Vec<u8>,
    /// The number of the help string shown as tooltip while the row is hovered; 0 for none.
    pub help: i32,
    pub disabled: bool,
    /// The small picture in front of the text, in the slot of `miniconwidth` by `miniconheight`.
    pub icon: Option<super::layout::Picture>,
}

/// A key a list reads.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ListKey {
    Up,
    Down,
    PageUp,
    PageDown,
    Home,
    End,
}

/// The rows of a list box, the first one shown, the selected one and the hovered one.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Rows {
    pub rows: Vec<Row>,
    pub top: usize,
    pub selected: Option<usize>,
    pub hover: Option<usize>,
}

/// The heights of the rows of a list, the space between two rows and the height inside the
/// borders: how many whole rows are shown from a first one, and which row is at a distance.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Heights {
    pub rows: Vec<i32>,
    pub spacing: i32,
    pub inner: i32,
}

impl Heights {
    /// How many whole rows fit from `top` on, at least one.
    pub fn fit_from(&self, top: usize) -> usize {
        self.fit(self.rows.get(top..).unwrap_or_default().iter())
    }

    /// The first row of the last screenful: the largest `top` there is.
    pub fn max_top(&self) -> usize {
        let last = self.fit(self.rows.iter().rev());
        self.rows.len().saturating_sub(last)
    }

    fn fit<'a>(&self, rows: impl Iterator<Item = &'a i32>) -> usize {
        let (mut used, mut n) = (0i64, 0usize);
        for &h in rows {
            let next = used + i64::from(h) + if n > 0 { i64::from(self.spacing) } else { 0 };
            if next > i64::from(self.inner) {
                break;
            }
            (used, n) = (next, n + 1);
        }
        n.max(1)
    }

    /// The row at distance `y` below the top of the list when `top` is the first one shown; `None`
    /// above the first, between two rows and below the last.
    pub fn row_at(&self, top: usize, y: i32) -> Option<usize> {
        let mut y = i64::from(y);
        if y < 0 {
            return None;
        }
        for (n, &h) in self.rows.iter().enumerate().skip(top) {
            if y < i64::from(h) {
                return Some(n);
            }
            y -= i64::from(h) + i64::from(self.spacing);
            if y < 0 {
                return None;
            }
        }
        None
    }
}

impl Rows {
    /// Scrolls the least that shows the selected row.
    pub fn show_selection(&mut self, fit: &Heights) {
        if let Some(row) = self.selected {
            if row < self.top {
                self.top = row;
            }
            while row >= self.top.saturating_add(fit.fit_from(self.top)) {
                self.top += 1;
            }
            self.top = self.top.min(fit.max_top());
        }
    }

    /// Moves the selection by a key: one row, or a page of the rows shown less one, or to an end.
    /// With nothing selected, Down and the page keys down start at the first row, Up and End at
    /// the last. Returns whether the selection changed.
    pub fn select_by_key(&mut self, key: ListKey, fit: &Heights) -> bool {
        let Some(last) = self.rows.len().checked_sub(1) else {
            return false;
        };
        let page = fit.fit_from(self.top).saturating_sub(1).max(1);
        let to = match (key, self.selected) {
            (ListKey::Home, _) | (ListKey::Down | ListKey::PageDown, None) => 0,
            (ListKey::End, _) | (ListKey::Up | ListKey::PageUp, None) => last,
            (ListKey::Up, Some(r)) => r.saturating_sub(1),
            (ListKey::Down, Some(r)) => (r + 1).min(last),
            (ListKey::PageUp, Some(r)) => r.saturating_sub(page),
            (ListKey::PageDown, Some(r)) => r.saturating_add(page).min(last),
        };
        let changed = self.selected != Some(to);
        self.selected = Some(to);
        self.show_selection(fit);
        changed
    }

    /// The value the select action gets: the selected row, or -1 for none and for a disabled one.
    pub fn action_value(&self) -> i32 {
        match self.selected.and_then(|r| self.rows.get(r)) {
            Some(row) if !row.disabled => self
                .selected
                .map_or(-1, |r| r.min(i32::MAX as usize) as i32),
            _ => -1,
        }
    }
}

/// The caret of a text field: where it is, and where the selection started. They are equal when
/// nothing is selected; the selection is between them, whichever is first.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Caret {
    pub at: usize,
    pub anchor: usize,
}

impl Caret {
    pub fn at_end(len: usize) -> Caret {
        Caret {
            at: len,
            anchor: len,
        }
    }

    /// The selected range of a text of `len` bytes.
    pub fn range(&self, len: usize) -> (usize, usize) {
        let (a, b) = (self.at.min(len), self.anchor.min(len));
        (a.min(b), a.max(b))
    }
}

/// A key a text field reads.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EditKey {
    Left,
    Right,
    Home,
    End,
    Delete,
    Backspace,
    /// A typed byte.
    Char(u8),
}

/// What a key did to a field.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Edited {
    /// The field does not handle it (a control character the field refuses, a character over the
    /// limit or not a digit in a numeric field).
    Ignored,
    /// The caret or the selection moved.
    Moved,
    /// The text changed.
    Changed,
}

/// What a field accepts: a number only; at most `limit` bytes (0: any number).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Limits {
    pub numeric: bool,
    pub limit: usize,
}

/// Whether a numeric field takes `byte` between the text before it and the text after it: a digit,
/// a minus sign at the very start, one decimal point; nothing in front of a minus sign.
fn numeric_byte(before: &[u8], byte: u8, after: &[u8]) -> bool {
    if before.is_empty() && after.first() == Some(&b'-') {
        return false;
    }
    match byte {
        b'0'..=b'9' => true,
        b'-' => before.is_empty(),
        b'.' => !before.contains(&b'.') && !after.contains(&b'.'),
        _ => false,
    }
}

/// Applies a key to a field. Left and Right move one byte, or to the edge of the selection when
/// there is one and Shift is not held; with Shift they extend the selection. Home and End go to
/// the ends (extending with Shift). Delete and Backspace remove the selection, or the byte after
/// or before the caret. A printable byte replaces the selection; a numeric field takes digits, a
/// leading minus and one point.
pub fn edit(
    text: &mut Vec<u8>,
    caret: &mut Caret,
    key: EditKey,
    shift: bool,
    limits: Limits,
) -> Edited {
    let len = text.len();
    let (from, to) = caret.range(len);
    let selecting = from != to;
    let go = |caret: &mut Caret, at: usize| {
        caret.at = at;
        if !shift {
            caret.anchor = at;
        }
        Edited::Moved
    };
    match key {
        EditKey::Left => match (selecting, shift) {
            (true, false) => go(caret, from),
            _ => go(caret, caret.at.min(len).saturating_sub(1)),
        },
        EditKey::Right => match (selecting, shift) {
            (true, false) => go(caret, to),
            _ => go(caret, (caret.at.min(len) + 1).min(len)),
        },
        EditKey::Home => go(caret, 0),
        EditKey::End => go(caret, len),
        EditKey::Delete | EditKey::Backspace => {
            let (start, end) = match (selecting, key) {
                (true, _) => (from, to),
                (false, EditKey::Delete) => (from, (from + 1).min(len)),
                (false, _) => (from.saturating_sub(1), from),
            };
            if start == end {
                return Edited::Moved;
            }
            text.drain(start..end);
            *caret = Caret {
                at: start,
                anchor: start,
            };
            Edited::Changed
        }
        EditKey::Char(byte) => {
            let printable = byte >= 32 && byte != 127;
            let wanted = !limits.numeric || numeric_byte(&text[..from], byte, &text[to..]);
            let room = limits.limit == 0 || len - (to - from) < limits.limit;
            if !(printable && wanted && room) {
                return Edited::Ignored;
            }
            text.splice(from..to, [byte]);
            *caret = Caret {
                at: from + 1,
                anchor: from + 1,
            };
            Edited::Changed
        }
    }
}

/// The caret position for a click at distance `x` from the start of the text: the byte boundary
/// nearest to it, given the advance of each byte.
pub fn caret_for(text: &[u8], advance: impl Fn(u8) -> i32, x: i32) -> usize {
    let mut pen = 0i64;
    for (i, &byte) in text.iter().enumerate() {
        let next = pen + i64::from(advance(byte));
        if i64::from(x) < (pen + next + 1) / 2 {
            return i;
        }
        pen = next;
    }
    text.len()
}

#[cfg(test)]
mod tests;
