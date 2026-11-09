//! The animal, guest and staff list windows in the game (information panels 20): what shows and
//! hides them, the refill on the interface clock, the filter buttons and the row clicks. The
//! lists themselves are [`crate::panel::unitlist`].

use super::*;
use crate::panel::unitlist::{self as lists, HIDES, PLAQUES, SPECS, STAFF};
use crate::unit::Kind;

impl App<'_> {
    /// Looks at the three windows (20.7): one that was shown hides the panels it replaces and
    /// takes its filter again, one hidden releases its main bar button; a shown one is refilled
    /// when a unit came or went, or when due (a rename asks for it, the clock's 10 s); and the
    /// selected entity's row follows the selection, shown or not (4.1 point 4).
    pub(super) fn lists_pass(&mut self, now: u64, ticking: bool) {
        playing!(self, game, ui);
        let (vfs, cat) = (self.renderer.vfs(), self.renderer.catalogue());
        let selected = game.selected;
        for (which, spec) in SPECS.iter().enumerate() {
            let w = &mut self.lists.windows[which];
            let shown = ui.shown(spec.layout);
            if shown != std::mem::replace(&mut w.shown, shown) {
                ui.set_button(spec.button, shown, true);
                if shown {
                    let plaques = if which == 0 {
                        &PLAQUES[..0]
                    } else {
                        &PLAQUES[..]
                    };
                    for id in HIDES.iter().chain(plaques) {
                        ui.set_hidden(*id, true);
                    }
                    let pressed =
                        (0..lists::filters(which)).find(|&i| ui.is_down(lists::button(which, i)));
                    w.choose(which, pressed.unwrap_or(w.filter), ui);
                }
            }
            w.follow(which, ui, selected);
            if !shown {
                continue;
            }
            let list = lists::entries(which, &game.world);
            if w.changed(&list) {
                w.now = true;
            } else if w.renamed(&list) {
                w.force = true;
            }
            if w.due(now, ticking) {
                w.refill(which, list, (ui, &game.world, cat, vfs), selected);
            }
        }
    }

    /// What the player did to the windows since the last look: the filter buttons selected
    /// (20.5) and the rows of the lists (20.6).
    pub(super) fn lists_acted(&mut self) {
        let Some(ui) = &mut self.ui else { return };
        for id in ui.take_selected_of(&lists::buttons()) {
            if let Some((which, i)) = lists::button_of(id) {
                self.lists.windows[which].choose(which, i, ui);
            }
        }
        let rows: Vec<(usize, i32)> = (self.ui_actions[self.acted..].iter())
            .filter_map(|a| match *a {
                Action::Row(id, row) => Some((lists::list_of(id)?, row)),
                _ => None,
            })
            .collect();
        for (which, row) in rows {
            self.list_row(which, row);
        }
    }

    /// A row of window `which` was selected (20.6): no row clears the selection; a unit of the
    /// list becomes the selected entity, with its panel, and the view centres on it, which ends
    /// the follow. In the staff list the helicopter stands for its base, and a view that follows
    /// it while the base is selected stays.
    pub(super) fn list_row(&mut self, which: usize, row: i32) {
        let following = self.following();
        playing!(self, game, ui);
        let Ok(row) = usize::try_from(row) else {
            game.selected = None;
            return;
        };
        let w = &self.lists.windows[which];
        let Some(id) = (w.unit_of(row)).filter(|&id| game.world.find(id).is_some()) else {
            return;
        };
        let base = game.world.units.get(id).and_then(|u| match &u.part {
            crate::unit::Part::Staff(s) if u.kind == Kind::Heli => Some(s.heli.base),
            _ => None,
        });
        let heli = which == STAFF && base.is_some();
        let target = match base.filter(|_| heli) {
            Some(base) => game
                .world
                .find(base)
                .filter(|&at| game.world.zoo.entities[at].class == "building")
                .map(|_| base),
            None => Some(id),
        };
        let Some(target) = target else { return };
        ui.set_button(BULLDOZER, false, true);
        let stay = heli && following && game.selected == Some(target);
        if game.selected != Some(target) {
            game.selected = Some(target);
        }
        let tile = (game.world.find(target))
            .and_then(|at| game.world.zoo.entities[at].common.as_ref())
            .map(|c| (c.x.div_euclid(64), c.y.div_euclid(64)));
        if let (false, Some((x, y))) = (stay, tile)
            && let (Ok(x), Ok(y)) = (u32::try_from(x), u32::try_from(y))
        {
            game.camera.centre_on(&game.world.zoo, x, y);
            for follow in [animal::FOLLOW, staff::FOLLOW, guest::FOLLOW] {
                if ui.is_down(follow) {
                    ui.set_button(follow, false, true);
                }
            }
        }
    }
}
