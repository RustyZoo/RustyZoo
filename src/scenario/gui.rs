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

/// Which types an availability trigger reaches (7.3).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Which {
    Name(i32),
    Names(Vec<i32>),
    Animals,
    All,
    /// Trigger 4 of `truleb` 10: the type is unlocked for good, like an ini reward.
    Unlock(i32),
}

/// What an availability trigger tells when it really changed a flag (fresh runs only).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Tell {
    /// The message line: the string, with the name id of the type for its `%r` (trigger 4 only;
    /// trigger 7 shows the string as it is).
    Message(i32, Option<i32>),
    Popup(Box<Popup>),
}

/// An availability trigger: the types it reaches are made available or not.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Avail {
    pub which: Which,
    pub on: bool,
    pub tell: Option<Tell>,
}

impl Avail {
    /// Whether the trigger reaches the type.
    pub fn reaches(&self, ty: &crate::entity::EntityType) -> bool {
        match &self.which {
            Which::Name(id) => ty.name_id == *id,
            Which::Names(ids) => ids.contains(&ty.name_id),
            Which::Animals => ty.class == "animals",
            Which::All => true,
            Which::Unlock(_) => false,
        }
    }
}

/// What triggers 1, 3, 5, 8 and 9 ask of the game (7.2, 7.4, 7.5). The popup comes after it, when
/// there is one and the run is fresh.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Deed {
    /// Trigger 1: the message of string `text` built with `value`, of kind `kind`.
    Say { kind: i32, text: i32, value: i32 },
    /// Trigger 3: `how` 0 a donation, 1 cash added, 2 cash set to `amount`; then the message of
    /// string `text` (kind 4) with the money text of `amount`, when `text` is above 0.
    Cash { how: i32, amount: i32, text: i32 },
    /// Trigger 5: an animal of name id `name` and subtype `sub`, in its crate near the gate; the
    /// popup only when there was a place for it.
    Boxed {
        sub: u8,
        name: i32,
        popup: Option<Popup>,
    },
    /// Trigger 8: every entity of the name id (`by` 0), family (1), genus (2) or species (3) `id`
    /// goes.
    Remove {
        by: i32,
        id: i32,
        popup: Option<Popup>,
    },
    /// Trigger 9: whether the animal panel's sell button works.
    Sale { allowed: bool, popup: Option<Popup> },
}

impl Deed {
    /// What a trigger run again after a load keeps (7): the removal and the switch, no popup.
    pub(super) fn quiet(self) -> Option<Deed> {
        match self {
            Deed::Remove { by, id, .. } => Some(Deed::Remove {
                by,
                id,
                popup: None,
            }),
            Deed::Sale { allowed, .. } => Some(Deed::Sale {
                allowed,
                popup: None,
            }),
            _ => None,
        }
    }
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
    out: Vec<Effect>,
    avail: Vec<Avail>,
    deeds: Vec<Deed>,
}

/// The state shared by the rules and the triggers. Rules read it through `&self`.
#[derive(Debug, Default)]
pub struct Gui(RefCell<Inner>);

impl Gui {
    /// Rule 10: whether the element exists and is shown, as the last sync told.
    pub(super) fn shown(&self, id: i32) -> bool {
        let mut g = self.0.borrow_mut();
        g.asked.insert(id);
        g.shown.get(&id).copied().unwrap_or(false)
    }

    /// The application is to tell whether the element is shown from the next sync on.
    pub(super) fn watch(&self, id: i32) {
        self.0.borrow_mut().asked.insert(id);
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

    pub(super) fn push(&self, effect: Effect) {
        self.0.borrow_mut().out.push(effect);
    }

    /// Shows a popup after the effects already left.
    pub fn popup(&self, popup: Popup) {
        self.push(Effect::Popup(popup));
    }

    pub(super) fn push_avail(&self, avail: Avail) {
        self.0.borrow_mut().avail.push(avail);
    }

    pub(super) fn push_deed(&self, deed: Deed) {
        self.0.borrow_mut().deeds.push(deed);
    }

    /// The deeds the triggers left since the last call.
    pub fn take_deeds(&self) -> Vec<Deed> {
        std::mem::take(&mut self.0.borrow_mut().deeds)
    }

    /// The availability changes the triggers left since the last call.
    pub fn take_avail(&self) -> Vec<Avail> {
        std::mem::take(&mut self.0.borrow_mut().avail)
    }

    /// The effects the triggers left since the last call.
    pub fn take_effects(&self) -> Vec<Effect> {
        std::mem::take(&mut self.0.borrow_mut().out)
    }
}

impl Effect {
    /// The popup the effect shows, if it shows one.
    pub(super) fn popup_mut(&mut self) -> Option<&mut Popup> {
        match self {
            Effect::Popup(p) => Some(p),
            Effect::Press(_, p) | Effect::Set { popup: p, .. } => p.as_mut(),
        }
    }

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
