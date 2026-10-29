//! The tank filter panel (information panels 18): layout 46 with its status page (the state, the
//! months since the last service and the months in operation) and its finance page (the upkeep of
//! the last period, the current one and since the filter was built). It is [`Panel`]'s layout
//! beside the two building panels: filled when shown and every second of the interface clock while
//! the game runs, never by a click on a tab.

use super::{Panel, info_image};
use crate::entity::Catalogue;
use crate::money::{SYMBOL, Symbol};
use crate::panel::exhibit::type_at;
use crate::status::Words;
use crate::text::format::{self, Arg};
use crate::text::to_cp1252;
use crate::ui::interface::Interface;
use crate::ui::layout::FOCUSED;
use crate::vfs::Vfs;
use crate::world::{Wear, World, filter_months_since, filter_wear};

/// The layout of the panel, its name field and its sell button.
pub const LAYOUT: i32 = 46;
pub const NAME: i32 = 4904;
pub const SELL: i32 = 4906;
const PICTURE: i32 = 4903;
const STATE: i32 = 4913;
const SERVICE: i32 = 4916;
const MONTHS: i32 = 4918;
/// The upkeep of the last period, of the current one and since the filter was built.
const UPKEEP: [i32; 3] = [4921, 4923, 4925];
/// The strings of a broken, a decayed and a healthy filter.
const STATES: [i32; 3] = [4999, 4914, 4913];

impl Panel {
    /// Writes the filter panel for the selected filter at game time `now` (ms), in the order of
    /// the original (18.5), and returns the log line when something it shows changed. A selection
    /// that is no filter writes nothing.
    pub(super) fn filter(
        &mut self,
        ui: &mut Interface,
        world: &World,
        cat: &Catalogue,
        vfs: &Vfs,
        now: i64,
    ) -> Option<String> {
        let id = self.selected?;
        let at = world.find(id)?;
        let common = world.zoo.entities[at].common.as_ref()?;
        let f = common.filter?;
        let ty = type_at(world, cat, at)?;
        let fresh = std::mem::take(&mut self.fresh);
        let symbol = Symbol::new(ui.string(SYMBOL));
        let upkeep = f
            .upkeep
            .map(|n| format::money(&symbol, i64::from(n), false));
        for (&element, text) in UPKEEP.iter().zip(&upkeep) {
            ui.set_text(element, &to_cp1252(text));
        }
        let wear = filter_wear(ty, f.health);
        let state = ui.template(STATES[wear as usize]);
        ui.set_text(STATE, &to_cp1252(&state));
        // The type's information image, the same whatever the state.
        let picture = info_image(ty).and_then(|name| ui.tree.picture(vfs, &name));
        ui.set_picture(PICTURE, picture);
        let ago = filter_months_since(f.serviced, now);
        let none = Words::new(|_| String::new());
        let ago_text = format::expand(&none, &ui.template(SERVICE), &[Arg::Int(i64::from(ago))]);
        ui.set_text(SERVICE, &to_cp1252(&ago_text));
        ui.set_text(MONTHS, f.months.to_string().as_bytes());
        // The name is not written over a field the player is typing in (open point O11).
        let typing = (ui.tree.find(NAME)).is_some_and(|i| ui.tree.elements[i].state & FOCUSED != 0);
        let name = match common.name.is_empty() {
            true => ui.string(ty.name_id),
            false => common.name.clone(),
        };
        if fresh || !typing {
            ui.set_text(NAME, &to_cp1252(&name));
        }
        ui.set_enabled(SELL, true);
        let word = match wear {
            Wear::Broken => "broken",
            Wear::Decayed => "minimum",
            Wear::Healthy => "maximum",
        };
        let told = format!(
            "filter {id} {name:?} {word} service {ago} months {} upkeep {} {} {}",
            f.months, upkeep[0], upkeep[1], upkeep[2]
        );
        match told == self.told {
            true => None,
            false => {
                self.told.clone_from(&told);
                Some(told)
            }
        }
    }
}

#[cfg(test)]
mod tests;
