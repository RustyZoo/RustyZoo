//! The thoughts of guests (guests visit 8): one list for the whole zoo, newest first, which keeps
//! the last 1000. A thought is a string id of the language files and the guest who thinks it, and
//! the entity it is about when it has one.

use std::collections::VecDeque;

/// The thoughts the list keeps, and those the guest panel shows.
pub const KEPT: usize = 1000;
pub const SHOWN: usize = 5;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Thought {
    pub text: i32,
    pub thinker: u32,
    pub object: Option<u32>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Thoughts(VecDeque<Thought>);

impl Thoughts {
    /// 8.2: a new thought goes in front; the oldest are dropped past [`KEPT`].
    pub fn add(&mut self, thinker: u32, text: i32, object: Option<u32>) {
        self.0.push_front(Thought {
            text,
            thinker,
            object,
        });
        self.0.truncate(KEPT);
    }

    /// 8.4: the newest thoughts of a guest, newest first (what the guest panel lists).
    pub fn of(&self, thinker: u32) -> Vec<&Thought> {
        let mine = self.0.iter().filter(|t| t.thinker == thinker);
        mine.take(SHOWN).collect()
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
