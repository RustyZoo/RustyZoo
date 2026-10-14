//! The interface the game runs with: the tree, what draws it and what tells whether a frame is
//! due again.
//!
//! The map view is an element of the tree. It is shown when the interface is made, and the game
//! draws its picture into the rectangle it is given while the tree is painted; the other layouts
//! are shown by [`Interface::show`] (the game code, a script). Whatever changes what the tree
//! looks like marks the interface [`dirty`](Interface::dirty); a paint clears that.

mod input;

use std::collections::HashMap;

use super::draw::{Painter, Rect4, Strings};
use super::layout::{DIRTY, FOCUSED, HIDDEN, Kind, TIMED, Tree};
use crate::Framebuffer;

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
        Interface {
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
        }
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
        let shown = self.map_view.is_some_and(|i| self.visible(i));
        shown && self.tree.elements.iter().all(|e| e.state & FOCUSED == 0)
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

    /// The sounds the interface asked for since the last call, by file name without the
    /// extension (`uiclick`): the shell plays them.
    pub fn take_sounds(&mut self) -> Vec<&'static str> {
        std::mem::take(&mut self.sounds)
    }

    /// The interface update, `period` milliseconds after the last one: a timed element hides
    /// itself `timerDuration` after it was shown (the first update after showing only starts
    /// the count).
    pub fn update(&mut self, period: u32) {
        self.repeat(period);
        let period = i64::from(period);
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
                None => i64::from(e.timer_duration),
                Some(left) => left.saturating_sub(period),
            };
            if left <= 0 {
                e.countdown = None;
                e.state |= HIDDEN;
                self.dirty = true;
            } else {
                e.countdown = Some(left);
            }
        }
    }
}

#[cfg(test)]
mod tests;
