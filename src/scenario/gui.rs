//! What the scenario and the interface tell one another (scenarios 6, 7.1, 8.2, 8.3).
//!
//! The scenario lives in the simulation and the interface in the application, so they meet here:
//! the application tells which elements are shown, which were pressed and what the strings the
//! rules asked about say; the scenario leaves [`Effect`]s for the application to carry out.

use std::cell::RefCell;
use std::collections::{BTreeSet, HashMap};

use super::popup::{Op, Popup};

/// Something a trigger does to the interface, in order.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Effect {
    /// Shows a popup.
    Popup(Popup),
    /// Presses the element if it exists, then shows the popup.
    Press(i32, Option<Popup>),
    /// Does each operation; then shows the popup if something changed, or in any case when
    /// `always`.
    Set {
        ops: Vec<(i32, Op)>,
        popup: Option<Popup>,
        always: bool,
    },
}

/// A press the rules have not read yet: a list box adds the row selected and its text.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Press {
    pub row: Option<(usize, Vec<u8>)>,
}

#[derive(Debug, Default)]
struct Inner {
    /// The elements the rules asked about (8.2), and the press each one holds.
    pressed: HashMap<i32, Option<Press>>,
    /// The ids whose visibility, and the strings whose text, the application is to tell.
    asked: BTreeSet<i32>,
    strings: BTreeSet<i32>,
    shown: HashMap<i32, bool>,
    texts: HashMap<i32, Vec<u8>>,
    /// The string of the first goal in progress, for a hint of -1.
    goal_text: i32,
    out: Vec<Effect>,
}

/// The state shared by the rules and the triggers. Rules read it through `&self`.
#[derive(Debug, Default)]
pub struct Gui(RefCell<Inner>);

impl Gui {
    /// Rule 10: whether the element exists and is shown. Asking registers it.
    pub(super) fn shown(&self, id: i32) -> bool {
        let mut g = self.0.borrow_mut();
        g.asked.insert(id);
        g.shown.get(&id).copied().unwrap_or(false)
    }

    /// Rules 10 and 11: the press of the element since the last question, which consumes it.
    /// The first question makes the entry, not pressed.
    pub(super) fn take_press(&self, id: i32) -> Option<Press> {
        self.0.borrow_mut().pressed.entry(id).or_default().take()
    }

    /// The text of a string for a rule, empty until the application has told it.
    pub(super) fn text(&self, id: i32) -> Vec<u8> {
        let mut g = self.0.borrow_mut();
        g.strings.insert(id);
        g.texts.get(&id).cloned().unwrap_or_default()
    }

    /// The application tells what the rules asked about: whether each element is shown, what each
    /// string says.
    pub fn sync(&self, shown: &dyn Fn(i32) -> bool, text: &dyn Fn(i32) -> Vec<u8>) {
        let mut g = self.0.borrow_mut();
        let g = &mut *g;
        g.shown = g.asked.iter().map(|&id| (id, shown(id))).collect();
        g.texts = g.strings.iter().map(|&id| (id, text(id))).collect();
    }

    /// The element `id` was pressed (a list box: `row`, with its text). Only an element the rules
    /// asked about before notices.
    pub fn pressed(&self, id: i32, row: Option<(usize, Vec<u8>)>) {
        if let Some(entry) = self.0.borrow_mut().pressed.get_mut(&id) {
            *entry = Some(Press { row });
        }
    }

    pub(super) fn set_goal_text(&self, id: i32) {
        self.0.borrow_mut().goal_text = id;
    }

    /// Leaves an effect; the hint of -1 of its popups gets the text of the first goal in progress.
    pub(super) fn push(&self, mut effect: Effect) {
        let mut g = self.0.borrow_mut();
        let goal_text = g.goal_text;
        let popups = match &mut effect {
            Effect::Popup(p) => Some(p),
            Effect::Press(_, p) | Effect::Set { popup: p, .. } => p.as_mut(),
        };
        if let Some(p) = popups.filter(|p| p.hint.short == -1) {
            p.hint.short = goal_text;
        }
        g.out.push(effect);
    }

    /// The effects the triggers left since the last call.
    pub fn take_effects(&self) -> Vec<Effect> {
        std::mem::take(&mut self.0.borrow_mut().out)
    }
}

impl Effect {
    /// What a trigger run again after a load keeps (7.1): no popup, no press.
    pub(super) fn quiet(self) -> Option<Effect> {
        match self {
            Effect::Popup(_) | Effect::Press(..) => None,
            Effect::Set { ops, always, .. } => Some(Effect::Set {
                ops,
                popup: None,
                always,
            }),
        }
    }
}
