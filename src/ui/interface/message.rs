//! The message line's queue and the history list it mirrors (ui layout 4.13 to 4.15).
//!
//! One list holds the waiting messages, the shown one and the older ones, the newest first; the
//! rows of the history list 7201 are its entries one for one. A count says how many of the newest
//! entries wait for the line.

use super::{Action, Interface};
use crate::ui::layout::{HIDDEN, Kind, Picture, SELECTED};
use crate::ui::widgets::Row;

/// The history list, the layout that holds it and the game screen's button that toggles that
/// layout (4.14 points 1 and 6).
pub(crate) const LIST: i32 = 7201;
const LAYOUT: i32 = 29;
const BUTTON: i32 = 1006;

/// A message to post.
pub(crate) struct Message<'a> {
    pub text: &'a [u8],
    pub kind: i32,
    /// The entity the message is about and the icon it shows in the history, if the world found it.
    entity: Option<(u32, Option<Picture>)>,
    /// Put on the line; a dialog's text is only listed (4.15 point 2).
    show: bool,
    /// Taken out of the list when the line is done with it (the pause string, 4.14 point 5).
    replace: bool,
}

impl<'a> Message<'a> {
    pub fn new(text: &'a [u8], kind: i32) -> Self {
        Message {
            text,
            kind,
            entity: None,
            show: true,
            replace: false,
        }
    }

    pub fn about(mut self, entity: u32, icon: Option<Picture>) -> Self {
        self.entity = Some((entity, icon));
        self
    }

    pub fn listed_only(mut self) -> Self {
        self.show = false;
        self
    }

    pub fn replacing(mut self) -> Self {
        self.replace = true;
        self
    }
}

/// One entry of the list.
pub(super) struct Entry {
    pub(super) text: Vec<u8>,
    pub(super) kind: i32,
    /// The clock (ms, 32 bits) of the post.
    pub(super) at: u32,
    pub(super) entity: Option<u32>,
    pub(super) icon: Option<Picture>,
    pub(super) show: bool,
    pub(super) replace: bool,
}

impl Interface {
    /// Posts `text` of kind `kind` to the message line (`ZTMessageQueue`; ui layout 4.13 points 5
    /// to 12). A text of a kind below 3 that the list already holds from less than
    /// `minimumMessageInterval` seconds ago is dropped. A hidden line shows it at once with a fresh
    /// count. A shown line queues it (at most `queueSize - 1` wait, the oldest is dropped) and
    /// empties its count, so the text shown gives way at the next update. The text takes the
    /// colour of its kind modulo 3. `false` if the tree has no message line, or the text is dropped.
    pub(crate) fn message(&mut self, text: &[u8], kind: i32) -> bool {
        self.post(Message::new(text, kind))
    }

    /// [`Interface::message`] with the flags and the entity of 4.14 and 4.15. `MessageDisplay=0`
    /// drops every post.
    pub(crate) fn post(&mut self, m: Message) -> bool {
        let Some(i) = self.line().filter(|_| self.settings.message_display) else {
            return false;
        };
        // The clock is 32 bits, the age an unsigned difference and the interval a 32-bit product
        // read as unsigned (4.13 point 12): 0 never drops, a negative interval drops every equal
        // text still listed.
        let now = self.input.clock as u32;
        let within = self.settings.message_interval.wrapping_mul(1000) as u32;
        let repeated = |e: &Entry| e.text == m.text && now.wrapping_sub(e.at) < within;
        if m.kind < 3 && self.messages.iter().any(repeated) {
            return false;
        }
        let shown = self.visible(i);
        // A pause string at the front goes before the new entry comes in (4.14 point 5).
        let removed = shown && self.messages.front().is_some_and(|e| e.replace);
        if removed {
            self.messages.pop_front();
        }
        // The list holds `queueSize` entries and drops its oldest when full; a negative size never
        // drops (4.13 point 9).
        let size = match self.tree.elements[i].queue.size {
            n if n < 0 => usize::MAX,
            n => usize::try_from(n).unwrap_or(0),
        };
        if self.messages.len() >= size {
            self.messages.pop_back();
        }
        let (entity, icon) = m.entity.map_or((None, None), |(e, icon)| (Some(e), icon));
        self.messages.push_front(Entry {
            text: m.text.to_vec(),
            kind: m.kind,
            at: now,
            entity,
            icon,
            show: m.show,
            replace: m.replace,
        });
        if shown {
            // The removal does not lower the count (4.14 point 5): the cut sees the list as it was
            // before it. One place less than the list wait, for the shown message.
            let places = (self.messages.len() + usize::from(removed)).saturating_sub(1);
            self.waiting = (self.waiting + 1).min(places);
            self.tree.elements[i].countdown = Some(0);
        } else if m.show && !m.text.is_empty() {
            self.put_message(i, 0);
            self.tree.elements[i].countdown = None;
            self.flag(i, HIDDEN, false);
        }
        self.sync_history(i);
        true
    }

    /// The message line, if the tree has one.
    fn line(&self) -> Option<usize> {
        (self.tree.elements.iter()).position(|e| matches!(e.kind, Kind::MessageQueue))
    }

    /// Puts the entry `n` on the message line `i` in the colour of its kind modulo 3, with its icon.
    fn put_message(&mut self, i: usize, n: usize) {
        let Some(entry) = self.messages.get(n) else {
            return;
        };
        let (text, icon) = (entry.text.clone(), entry.icon);
        let e = &mut self.tree.elements[i];
        // The C remainder: a negative kind is the general colour (4.13 point 11).
        e.look.fore_color = e.queue.colors[(entry.kind % 3).max(0) as usize];
        e.look.icon = icon;
        self.put_text(i, &text);
    }

    /// Makes the rows of the history list the entries of the list.
    fn sync_history(&mut self, line: usize) {
        let queue = self.tree.elements[line].queue;
        let rows = (self.messages.iter())
            .map(|e| Row {
                text: e.text.clone(),
                icon: e.icon,
                ..Default::default()
            })
            .collect();
        let colors = (self.messages.iter())
            .map(|e| Some(queue.colors[(e.kind % 3).max(0) as usize]))
            .collect();
        self.refill_rows(queue.list, rows);
        self.set_row_colors(queue.list, colors);
    }

    /// Tells the message line whether the game is paused: then a line with nothing waiting keeps
    /// its text and restarts its count instead of expiring.
    pub(crate) fn hold_message(&mut self, paused: bool) {
        self.message_paused = paused;
    }

    /// Message id 0 (the unpause): empties the message line's count, so that its shown message
    /// gives way at the next update.
    pub(crate) fn end_message(&mut self) {
        if !self.settings.message_display {
            return;
        }
        for e in &mut self.tree.elements {
            if matches!(e.kind, Kind::MessageQueue) {
                e.countdown = Some(0);
            }
        }
    }

    /// The count of the message line `i` is up (ui layout 4.13 points 4 and 5, 4.15 point 5): with
    /// nothing waiting and the game paused the count starts again and the text stays; otherwise
    /// the oldest waiting message to show is shown with a fresh count (the others stop waiting);
    /// with none, a pause string goes and the line empties and hides.
    pub(super) fn message_due(&mut self, i: usize) {
        if self.message_paused && self.waiting == 0 {
            self.tree.elements[i].countdown = None;
            return;
        }
        while self.waiting > 0 {
            self.waiting -= 1;
            if self.messages.get(self.waiting).is_some_and(|e| e.show) {
                self.put_message(i, self.waiting);
                self.tree.elements[i].countdown = None;
                return;
            }
        }
        if self.messages.front().is_some_and(|e| e.replace) {
            self.messages.pop_front();
            self.sync_history(i);
        }
        let e = &mut self.tree.elements[i];
        e.countdown = None;
        e.text = None;
        e.look.icon = None;
        e.state |= HIDDEN;
        let id = e.id;
        self.dirty = true;
        self.events.push(format!("hide {id}"));
    }

    /// Empties the queue, the list and the line, for a world load and a stop (4.14 point 9); the
    /// list's layout closes.
    pub(crate) fn clear_messages(&mut self) {
        self.messages.clear();
        self.waiting = 0;
        if let Some(i) = self.line() {
            let e = &mut self.tree.elements[i];
            (e.text, e.countdown, e.look.icon) = (None, None, None);
            e.state |= HIDDEN;
            self.sync_history(i);
        }
        if let Some(i) = self.tree.find(LAYOUT) {
            self.tree.hide(i);
            self.history_hidden(LAYOUT);
        }
    }

    /// The history of the game screen: its button toggles the layout 29 like a button with
    /// `action=3` (`target=29`), no layout file naming it (4.14 point 6).
    pub(super) fn bind_history(&mut self) {
        let layout = self.tree.find(LAYOUT).filter(|_| self.line().is_some());
        if let (Some(_), Some(b)) = (layout, self.tree.find(BUTTON)) {
            let e = &mut self.tree.elements[b];
            if matches!(e.kind, Kind::Button(_)) && e.action == 0 {
                (e.action, e.target) = (3, LAYOUT);
            }
        }
    }

    /// The history opens: the list scrolls to the message on the line (4.14 point 6).
    pub(super) fn history_shown(&mut self, id: i32) {
        if id != LAYOUT {
            return;
        }
        let list = self.line().map(|i| self.tree.elements[i].queue.list);
        if let Some(l) = list.and_then(|id| self.tree.find(id)) {
            self.scroll_owner(l, self.waiting as i64);
        }
    }

    /// The history closes by any way: its button comes up without running its action.
    pub(super) fn history_hidden(&mut self, id: i32) {
        if id != LAYOUT {
            return;
        }
        if let Some(b) = self.tree.find(BUTTON) {
            self.flag(b, SELECTED, false);
        }
    }

    /// A click on message `i` of the history, or on the line (4.14 points 7 and 8): the message
    /// on the line gives way at the next update. The entity of the message, if any, for the game
    /// to go to; `None` if there is no such message.
    pub(crate) fn message_click(&mut self, i: i32) -> Option<Option<u32>> {
        let entity = self.messages.get(usize::try_from(i).ok()?)?.entity;
        if let Some(line) = self.line() {
            self.tree.elements[line].countdown = Some(0);
        }
        Some(entity)
    }

    /// The release of the left button on the line `line` that was pressed on it (4.14 point 8).
    pub(super) fn line_clicked(&mut self) {
        self.actions.push(Action::Message(self.waiting as i32));
    }
}
