//! The thoughts of guests (guests visit 8): one list for the whole zoo, newest first, which keeps
//! the last 1000. A thought is a string id of the language files and the guest who thinks it, the
//! entity it is about when it has one, and the exhibit it is about and that exhibit's gate tile.

use std::collections::VecDeque;

/// The thoughts the list keeps, and those the guest panel shows.
pub const KEPT: usize = 1000;
pub const SHOWN: usize = 5;
/// The thoughts the exhibit panel shows.
pub const SHOWN_EXHIBIT: usize = 20;

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Thought {
    pub text: i32,
    pub thinker: u32,
    pub object: Option<u32>,
    /// The exhibit it is about (never the zoo grounds), and the tile inside its gate.
    pub exhibit: Option<u32>,
    pub gate: Option<usize>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Thoughts(VecDeque<Thought>);

impl Thoughts {
    /// 8.2: a new thought goes in front; the oldest are dropped past [`KEPT`].
    pub fn add(&mut self, thought: Thought) {
        self.0.push_front(thought);
        self.0.truncate(KEPT);
    }

    /// Puts a thought of a loaded file at the back (save games 9).
    pub fn push_back(&mut self, thought: Thought) {
        self.0.push_back(thought);
    }

    /// The thoughts, newest first.
    pub fn iter(&self) -> impl Iterator<Item = &Thought> {
        self.0.iter()
    }

    /// 8.4: the newest `n` thoughts that match, newest first.
    fn newest(&self, n: usize, matches: impl Fn(&Thought) -> bool) -> Vec<&Thought> {
        self.0.iter().filter(|t| matches(t)).take(n).collect()
    }

    /// 8.4: the newest thoughts of a guest (what the guest panel lists).
    pub fn of(&self, thinker: u32) -> Vec<&Thought> {
        self.newest(SHOWN, |t| t.thinker == thinker)
    }

    /// 8.4: the newest thoughts about an entity (the animal panel).
    pub fn about(&self, object: u32) -> Vec<&Thought> {
        self.newest(SHOWN, |t| t.object == Some(object))
    }

    /// 8.4: the newest thoughts about an exhibit (the exhibit panel).
    pub fn in_exhibit(&self, exhibit: u32) -> Vec<&Thought> {
        self.newest(SHOWN_EXHIBIT, |t| t.exhibit == Some(exhibit))
    }

    /// 8.5: an exhibit that is gone takes the thoughts about it that have no object; the others
    /// lose the exhibit and keep the tile inside its gate.
    pub fn drop_exhibit(&mut self, exhibit: u32) {
        self.0
            .retain(|t| t.exhibit != Some(exhibit) || t.object.is_some());
        for t in self.0.iter_mut().filter(|t| t.exhibit == Some(exhibit)) {
            t.exhibit = None;
        }
    }

    /// 8.5: a unit that is gone takes its thoughts, and those about it, with it.
    pub fn forget(&mut self, id: u32) {
        self.0.retain(|t| t.thinker != id && t.object != Some(id));
    }

    /// 8.5: a scenery object or building that is gone takes the thoughts about it: only the
    /// thoughts about an object `exists` answers for stay.
    pub fn keep_objects(&mut self, exists: impl Fn(u32) -> bool) {
        self.0.retain(|t| t.object.is_none_or(&exists));
    }

    /// Whether any thought is about an entity.
    pub fn has_objects(&self) -> bool {
        self.0.iter().any(|t| t.object.is_some())
    }

    pub fn len(&self) -> usize {
        self.0.len()
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}
