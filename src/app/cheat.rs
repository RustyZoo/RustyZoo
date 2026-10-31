//! The name hook (cheats 2): the text of a name set on an animal, a guest, a staff member or an
//! exhibit is offered to the cheats before the name is stored.

use super::App;
use crate::cheat::{Cheat, Named, Subject};
use crate::text::from_cp1252;

impl App<'_> {
    /// Runs the cheat whose text is exactly `text` in `class`, if there is one.
    pub(super) fn name_hook(&mut self, class: Subject, text: &str) {
        self.hook(class, &Named::default(), None, text);
    }

    /// As [`App::name_hook`] for the entity `who` that was named, whose type the conditions of
    /// the rows look at.
    fn hook(&mut self, class: Subject, named: &Named, who: Option<u32>, text: &str) {
        match self.cheats.find_named(class, named, text) {
            Some(Cheat::ResearchCategory(name)) => self.complete_category(name),
            Some(cheat) => {
                self.guest_cheat(cheat);
                self.exhibit_and_staff_cheat(cheat, who);
            }
            None => {}
        }
    }

    /// The text in the name field `id` as typed, before the visible-character test.
    pub(super) fn typed_name(&self, id: i32) -> Option<String> {
        let bytes = self.ui.as_ref()?.text(id)?;
        Some(bytes.iter().map(|&b| from_cp1252(b)).collect())
    }

    /// The name hook of the entity `selected` whose name field `id` was just edited, in the class
    /// of the entity (`fallback` when there is none). A game must be open.
    pub(super) fn entity_hook(&mut self, fallback: Subject, id: i32, selected: Option<u32>) {
        let Some(text) = self.typed_name(id).filter(|_| self.game.is_some()) else {
            return;
        };
        let cat = self.renderer.catalogue();
        let found = self.game.as_ref().and_then(|g| {
            let e = &g.world.zoo.entities[g.world.find(selected?)?];
            let ty = crate::place::bulldozer::type_of(cat, e);
            let first = |s: &str| s.bytes().next().unwrap_or(0);
            let (class, subtype) = match e.class.as_str() {
                "animals" => (Subject::Animal, first(&e.subtype)),
                // A guest's sex is the first letter of its type's name.
                "guests" => (Subject::Guest, first(&e.type_name)),
                "keeper" => (Subject::Keeper, first(&e.subtype)),
                "maint" => (Subject::Worker, first(&e.subtype)),
                "tour" => (Subject::Guide, first(&e.subtype)),
                _ => return None,
            };
            let name_id = ty.map_or(0, |t| t.name_id);
            Some((class, Named { subtype, name_id }))
        });
        let (class, named) = found.unwrap_or((fallback, Named::default()));
        self.hook(class, &named, selected, &text);
    }
}
