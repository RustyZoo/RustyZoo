//! The name hook (cheats 2): the text of a name set on an animal, a guest, a staff member or an
//! exhibit is offered to the cheats before the name is stored.

use super::App;
use crate::cheat::{Cheat, Subject};
use crate::text::from_cp1252;

impl App<'_> {
    /// Runs the cheat whose text is exactly `text` in `class`, if there is one.
    pub(super) fn name_hook(&mut self, class: Subject, text: &str) {
        match self.cheats.find(class, text) {
            Some(Cheat::ResearchCategory(name)) => self.complete_category(name),
            Some(cheat) => self.guest_cheat(cheat),
            None => {}
        }
    }

    /// The text in the name field `id` as typed, before the visible-character test.
    pub(super) fn typed_name(&self, id: i32) -> Option<String> {
        let bytes = self.ui.as_ref()?.text(id)?;
        Some(bytes.iter().map(|&b| from_cp1252(b)).collect())
    }

    /// The name hook of the entity whose name field `id` was just edited. A game must be open.
    pub(super) fn entity_hook(&mut self, class: Subject, id: i32) {
        if let Some(text) = self.typed_name(id).filter(|_| self.game.is_some()) {
            self.name_hook(class, &text);
        }
    }
}
