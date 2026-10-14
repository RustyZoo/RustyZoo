//! The interface the game runs with: the tree, what draws it and what tells whether a frame is
//! due again.
//!
//! The map view is an element of the tree. It is shown when the interface is made, and the game
//! draws its picture into the rectangle it is given while the tree is painted; the other layouts
//! are shown by [`Interface::show`] (the game code, a script). Whatever changes what the tree
//! looks like, an animation showing another frame included, marks the interface
//! [`dirty`](Interface::dirty); a paint clears that.

mod dialog;
mod input;
mod tooltip;
mod widgets;

pub use dialog::Buttons;
use std::collections::HashMap;
pub use tooltip::Settings;

use super::draw::{Painter, Rect4, Strings};
use super::layout::{DIRTY, FOCUSED, HIDDEN, Kind, TIMED, Tree};
use crate::Framebuffer;
use crate::app::Key;

/// What the interface tells the game's code besides the buttons selected (those are in
/// [`Interface::take_selected`]): the list, bar and field actions the player's input ran, in
/// order, to be taken with [`Interface::take_actions`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Action {
    /// The select action of a list box: its id and the row selected, -1 for none or a disabled
    /// one.
    Row(i32, i32),
    /// A scroll bar (a text or a list without one) moved: its id and its position.
    Scroll(i32, i32),
    /// A key a text field passed on to its layout (Enter or Escape ended the edit): the layout's
    /// id.
    Key(i32, Key),
    /// The open question was answered: 1 OK, 2 Cancel, 6 Yes, 7 No.
    Answer(i32),
}

/// The game's strings, with the texts the game code has put in place of some of them (a version
/// label, a tooltip with a file name in it).
struct Texts {
    base: Box<dyn Strings>,
    replaced: HashMap<i32, String>,
}

impl Strings for Texts {
    fn string(&self, id: i32) -> String {
        match self.replaced.get(&id) {
            Some(text) => text.clone(),
            None => self.base.string(id),
        }
    }

    fn text_entry(&self, name: &[u8]) -> Vec<u8> {
        self.base.text_entry(name)
    }
}

/// The least milliseconds between two runs of the interface update (main loop 8).
const UPDATE_GAP: u64 = 33;

pub struct Interface {
    pub tree: Tree,
    painter: Painter,
    strings: Texts,
    /// The first `ZTMapView` of the tree.
    map_view: Option<usize>,
    dirty: bool,
    /// The mouse: pointer, hover, capture, drag, repeat.
    input: input::Input,
    /// What the interface did in answer to the input since the last [`Interface::take_events`],
    /// one line each, and the sounds it asks for.
    events: Vec<String>,
    sounds: Vec<&'static str>,
    /// The buttons whose action ran (see [`Interface::take_selected`]).
    selected: Vec<i32>,
    actions: Vec<Action>,
    settings: Settings,
    tip: tooltip::Tip,
    /// The layout of the open question: only it receives input, and the game stands still.
    modal: Option<usize>,
    /// The clock of the last interface update; `None` before the first.
    updated: Option<u64>,
}

impl Interface {
    /// An interface on a tree resolved for the size of the window. The map view is shown.
    pub fn new(mut tree: Tree, strings: Box<dyn Strings>) -> Interface {
        let map_view = tree
            .elements
            .iter()
            .position(|e| matches!(e.kind, Kind::MapView));
        if let Some(map_view) = map_view {
            tree.elements[map_view].state &= !HIDDEN;
        }
        let mut ui = Interface {
            tree,
            painter: Painter::default(),
            strings: Texts {
                base: strings,
                replaced: HashMap::new(),
            },
            map_view,
            dirty: true,
            input: input::Input::default(),
            events: Vec::new(),
            sounds: Vec::new(),
            selected: Vec::new(),
            actions: Vec::new(),
            settings: Settings::default(),
            tip: tooltip::Tip::default(),
            modal: None,
            updated: None,
        };
        ui.start_tooltip();
        ui.sync_owners();
        ui
    }

    /// Shows the layout `id` (see [`Tree::show`]); `false` if there is none.
    pub fn show(&mut self, id: i32) -> bool {
        self.dirty = true;
        self.tree.show(id)
    }

    /// Whether something changed since the last [`paint`](Interface::paint).
    pub fn dirty(&self) -> bool {
        self.dirty
    }

    /// Marks the interface as changed.
    pub fn touch(&mut self) {
        self.dirty = true;
    }

    /// Whether the map view is on the screen and no other element holds the keyboard focus: the
    /// keys of the map view act only then.
    pub fn map_view_has_the_keys(&self) -> bool {
        self.map_view_shown()
            && self.modal.is_none()
            && self.tree.elements.iter().all(|e| e.state & FOCUSED == 0)
    }

    /// Whether the map view is on the screen.
    pub fn map_view_shown(&self) -> bool {
        self.map_view.is_some_and(|i| self.visible(i))
    }

    fn id(&self, i: usize) -> i32 {
        self.tree.elements[i].id
    }

    fn event(&mut self, line: String) {
        self.events.push(line);
        self.dirty = true;
    }

    /// Sets or clears state flags of an element; a change makes the interface dirty.
    fn flag(&mut self, i: usize, flag: u32, on: bool) {
        let state = &mut self.tree.elements[i].state;
        let was = *state;
        match on {
            true => *state |= flag,
            false => *state &= !flag,
        }
        if *state != was {
            self.dirty = true;
        }
    }

    /// Brings every scroll bar in line with its owner.
    fn sync_owners(&mut self) {
        for i in 0..self.tree.elements.len() {
            self.sync_owner(i);
        }
    }

    /// Whether the element and its layouts are all shown.
    fn visible(&self, mut i: usize) -> bool {
        loop {
            let e = &self.tree.elements[i];
            if e.state & HIDDEN != 0 {
                return false;
            }
            match e.parent {
                Some(p) => i = p,
                None => return true,
            }
        }
    }

    /// Paints the whole interface into `frame`. `map` draws the map into the rectangle of the map
    /// view when it is reached.
    pub fn paint(&mut self, frame: &mut Framebuffer, map: &mut dyn FnMut(&mut Framebuffer, Rect4)) {
        self.painter.draw(&mut self.tree, frame, &self.strings, map);
        self.dirty = false;
    }

    /// The text of string `id` as the elements show it.
    pub fn string(&self, id: i32) -> String {
        self.strings.string(id)
    }

    /// The text of string `id` as the game's libraries have it, whatever was set in its place.
    pub fn template(&self, id: i32) -> String {
        self.strings.base.string(id)
    }

    /// Shows `text` wherever string `id` is shown, from now on.
    pub fn set_string(&mut self, id: i32, text: String) {
        self.strings.replaced.insert(id, text);
        for e in &mut self.tree.elements {
            if e.look.text_id.first() == Some(&id) {
                e.state |= DIRTY;
            }
        }
        self.dirty = true;
    }

    /// The ids of the buttons whose action ran since the last call, in order.
    pub fn take_selected(&mut self) -> Vec<i32> {
        std::mem::take(&mut self.selected)
    }

    /// The lines that tell what the interface did since the last call (`hover 7111`, `press ...`,
    /// `sound uiclick`, `show 32`, ...), without the `ui ` the log puts before them.
    pub fn take_events(&mut self) -> Vec<String> {
        std::mem::take(&mut self.events)
    }

    /// The actions the input ran since the last call, for the game's code.
    pub fn take_actions(&mut self) -> Vec<Action> {
        std::mem::take(&mut self.actions)
    }

    /// The sounds the interface asked for since the last call, by file name without the
    /// extension (`uiclick`): the shell plays them.
    pub fn take_sounds(&mut self) -> Vec<&'static str> {
        std::mem::take(&mut self.sounds)
    }

    /// Advances the animations that are shown at a drawn frame, `now` ms on the clock (see
    /// [`Painter::animate`]): the interface is dirty when one of them changed its frame.
    pub fn animate(&mut self, now: u64, paused: bool) {
        if self.painter.animate(&self.tree, now, paused) {
            self.dirty = true;
        }
    }

    /// Tells the interface the clock (ms) at which the input that follows happens: a held button
    /// repeats from the time it was pressed.
    pub fn set_time(&mut self, now: u64) {
        self.input.clock = now;
    }

    /// The interface update at the clock `now` (ms). It reads the clock itself and works at most
    /// 30 times a second: a call less than 33 ms after the last time that worked does nothing.
    /// It repeats held buttons and scroll bar parts, runs the tooltip's delay, and hides a timed
    /// element `timerDuration` after it was shown (the first update after showing only starts
    /// the count).
    pub fn update(&mut self, now: u64) {
        let elapsed = match self.updated {
            Some(last) if now < last.saturating_add(UPDATE_GAP) => return,
            Some(last) => now - last,
            None => 0,
        };
        self.updated = Some(now);
        self.input.clock = now;
        self.repeat();
        self.tooltip_update(elapsed);
        let period = i64::try_from(elapsed).unwrap_or(i64::MAX);
        for i in 0..self.tree.elements.len() {
            if self.tree.elements[i].state & TIMED == 0 {
                continue;
            }
            if !self.visible(i) {
                self.tree.elements[i].countdown = None;
                continue;
            }
            let e = &mut self.tree.elements[i];
            let left = match e.countdown {
                // The update that starts the count has used up 1 ms of it.
                None => i64::from(e.timer_duration) - 1,
                Some(left) => left.saturating_sub(period),
            };
            if left <= 0 {
                e.countdown = None;
                e.state |= HIDDEN;
                self.dirty = true;
                let id = e.id;
                self.events.push(format!("hide {id}"));
            } else {
                e.countdown = Some(left);
            }
        }
    }
}

#[cfg(test)]
mod tests;
