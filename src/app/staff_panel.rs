//! The staff panel in the game (information panels 4, 9; staff 6): what opens it (a staff member
//! selected on the map), the buttons and lists of its tabs, and the assign mode that sends a
//! keeper to the exhibit the player clicks. The panel itself is [`crate::panel::staff`], the
//! rules of the assignment [`crate::staff::assign`].

use super::*;
use crate::exhibit::{EXTERIOR, GROUNDS};
use crate::panel::staff;
use crate::staff::assign::{self, Outcome};
use crate::unit::{Kind, Part};

/// The help text of the assign mode, and the colours of the exhibit under the pointer: green when
/// the keeper may serve it, red when not.
const ASSIGN_HELP: i32 = 3590;
const GOOD: [u8; 3] = crate::terrain::SELECTION;
const BAD: [u8; 3] = [0xEC, 0, 0];

/// What the selected unit does to the unit panels.
enum Show {
    Animal(u32),
    Staff(u32),
    Nothing,
}

impl Game {
    /// The highlight of the exhibit under the pointer in the assign mode.
    pub(super) fn assign_marks(&self) -> Vec<Mark> {
        let Some((exhibit, good)) = self.staff.over else {
            return Vec::new();
        };
        let color = if good { GOOD } else { BAD };
        (self.world.tiles_of(exhibit).into_iter())
            .map(|tile| Mark { tile, color })
            .collect()
    }
}

impl App<'_> {
    /// Keeps the unit panels and the selection together (4.1): the selected unit shows its panel,
    /// the panel the player closed clears the selection, a selection that is no unit with a panel
    /// closes them.
    pub(super) fn units_sync(&mut self) {
        let (Some(game), Some(ui)) = (&mut self.game, &mut self.ui) else {
            return;
        };
        let world = &game.world;
        let live = |id: &u32| {
            world.find(*id).is_some()
                && !matches!(world.units.get(*id).map(|u| &u.part), Some(Part::Animal(a)) if a.dying)
        };
        let mut want = game.selected.filter(live);
        let closed =
            |open: Option<u32>, layout| open.is_some() && open == want && !ui.shown(layout);
        if closed(game.animal.selected, animal::LAYOUT)
            || closed(game.staff.selected, staff::LAYOUT)
        {
            (game.selected, want) = (None, None);
        }
        let show = match want.and_then(|id| world.units.get(id)) {
            Some(u) if u.kind == Kind::Animal => Show::Animal(u.id),
            Some(u) if matches!(u.kind, Kind::Keeper | Kind::Worker | Kind::Guide) => {
                Show::Staff(u.id)
            }
            _ => Show::Nothing,
        };
        let (animal_open, staff_open) = (game.animal.selected, game.staff.selected);
        match show {
            Show::Animal(id) => {
                if animal_open != Some(id) {
                    self.open_animal(id);
                }
                self.close_staff();
            }
            Show::Staff(id) => {
                if staff_open != Some(id) {
                    self.open_staff(id);
                }
                self.close_animal();
            }
            Show::Nothing => {
                self.close_animal();
                self.close_staff();
            }
        }
    }

    /// Selects the staff member with entity id `id` and shows its panel, filled at once (4.3): a
    /// worker has the duties tab and no assign button, anyone else the exhibits tab; the tab that
    /// was shown last is mapped to the class's own.
    pub(super) fn open_staff(&mut self, id: u32) {
        let (Some(game), Some(ui)) = (&mut self.game, &mut self.ui) else {
            return;
        };
        let worker = game
            .world
            .units
            .get(id)
            .is_some_and(|u| u.kind == Kind::Worker);
        game.selected = Some(id);
        game.staff.select(id);
        ui.show(staff::LAYOUT);
        let (job, other) = match worker {
            true => (staff::TABS[2], staff::TABS[1]),
            false => (staff::TABS[1], staff::TABS[2]),
        };
        ui.set_hidden(other, true);
        ui.set_hidden(job, false);
        ui.set_hidden(staff::ASSIGN, worker);
        if ui.is_down(other) {
            ui.press_button(job);
        }
    }

    /// Hides the staff panel: the next look clears the selection.
    fn close_staff(&mut self) {
        if let Some(ui) = &mut self.ui {
            ui.set_hidden(staff::LAYOUT, true);
        }
    }

    /// Notices what the player did to the staff panel and the selected staff member since the
    /// last look: a closed panel or a member that left the world clears the selection (`panel
    /// staff none`), the assign button starts the mode on its exhibits tab and ends with it, and
    /// the panel sits above the exhibit panel while it is shown (4.2).
    pub(super) fn staff_sync(&mut self) {
        let (Some(game), Some(ui)) = (&mut self.game, &mut self.ui) else {
            return;
        };
        let gone = (game.staff.selected).is_some_and(|id| game.world.find(id).is_none());
        if gone || !ui.shown(staff::LAYOUT) {
            if game.staff.selected.is_some() {
                self.panel_lines.push("staff none".into());
            }
            game.staff.deselect();
            ui.set_hidden(staff::LAYOUT, true);
        }
        let down = ui.is_down(staff::ASSIGN) && game.staff.selected.is_some();
        if down && !game.staff.assigning && !ui.shown(staff::EXHIBITS_LAYOUT) {
            ui.press_button(staff::TABS[1]);
        }
        // Another tab hides the exhibits layout and releases the button.
        if down && !ui.shown(staff::EXHIBITS_LAYOUT) {
            ui.set_button(staff::ASSIGN, false, true);
        }
        if game.staff.selected.is_none() {
            ui.set_button(staff::ASSIGN, false, true);
        }
        game.staff.assigning = ui.is_down(staff::ASSIGN);
        if !game.staff.assigning && game.staff.over.take().is_some() {
            ui.help_tip(0);
            ui.touch();
        }
        ui.set_alt(staff::LAYOUT, ui.shown(exhibit::LAYOUT));
    }

    /// The staff member the assign mode works for: the selected one while the assign button is
    /// down.
    pub(super) fn assigning(&self) -> Option<u32> {
        let (game, ui) = (self.game.as_ref()?, self.ui.as_ref()?);
        game.staff.selected.filter(|_| ui.is_down(staff::ASSIGN))
    }

    /// Ends the assign mode; whether it was on.
    pub(super) fn end_assign(&mut self) -> bool {
        let was = self.assigning().is_some();
        if let Some(ui) = &mut self.ui {
            ui.set_button(staff::ASSIGN, false, true);
        }
        was
    }

    /// The exhibit under the pointer (not the grounds) and whether the keeper may serve it, for
    /// the assign mode (staff 6 point 1).
    fn exhibit_under(&self, id: u32) -> Option<(u32, bool)> {
        let (game, ui) = (self.game.as_ref()?, self.ui.as_ref()?);
        let point = game.camera.pick(&game.world.zoo, ui.pointer());
        let tile = game
            .world
            .tile_index(point.0.div_euclid(64) as i32, point.1.div_euclid(64) as i32)?;
        let exhibit =
            (game.world.exhibits().region(tile)).filter(|&r| r != GROUNDS && r != EXTERIOR)?;
        let cat = self.renderer.catalogue();
        let at = game.world.find(id)?;
        let who = crate::place::bulldozer::type_of(cat, &game.world.zoo.entities[at])?;
        let good = assign::block_service(&game.world, cat, exhibit, who, (true, true)) == 0;
        Some((exhibit, good))
    }

    /// The pointer moved in the assign mode: the exhibit under it is highlighted, with the help
    /// text, and the change is told (`tool assign over N good|bad|none`).
    pub(super) fn assign_aim(&mut self) {
        let over = match (
            self.assigning(),
            self.game.as_ref().is_some_and(|g| g.hover),
        ) {
            (Some(id), true) => self.exhibit_under(id),
            _ => None,
        };
        let (Some(game), Some(ui)) = (&mut self.game, &mut self.ui) else {
            return;
        };
        if std::mem::replace(&mut game.staff.over, over) == over {
            return;
        }
        ui.help_tip(if over.is_some() { ASSIGN_HELP } else { 0 });
        ui.touch();
        self.tool_lines.push(match over {
            Some((x, good)) => {
                format!("tool assign over {x} {}", if good { "good" } else { "bad" })
            }
            None => "tool assign over none".into(),
        });
    }

    /// A release on the map in the assign mode (staff 6 point 1): the exhibit under the pointer
    /// is appended to the list, or the player is told why not. The mode stays on.
    pub(super) fn assign_release(&mut self, id: u32) {
        let Some((exhibit, _)) = self.exhibit_under(id) else {
            return;
        };
        let (Some(game), Some(ui)) = (&mut self.game, &mut self.ui) else {
            return;
        };
        let cat = self.renderer.catalogue();
        match assign::assign(&mut game.world, cat, id, exhibit) {
            Outcome::Assigned => {
                game.staff.clock.force = true;
                self.tool_lines.push(format!("tool assign {id} {exhibit}"));
            }
            Outcome::Refused(message) => {
                self.ui_sounds.push("noplace");
                carry::refusal(ui, &mut self.tool_lines, message);
            }
        }
    }

    /// A row of the exhibits list was selected (9.2): the assign mode ends; a row centres the view
    /// on the gate of that exhibit and makes it the current one of the exhibit panel, which is
    /// shown if hidden.
    pub(super) fn staff_row(&mut self, row: i32) {
        self.end_assign();
        let Some(game) = &mut self.game else { return };
        if let Some(ui) = &mut self.ui {
            ui.set_enabled(staff::REMOVE, row >= 0);
        }
        let exhibit = (game.staff.selected.and_then(|id| game.world.units.get(id))).and_then(|u| {
            match &u.part {
                Part::Staff(s) => usize::try_from(row)
                    .ok()
                    .and_then(|r| s.assigned.get(r))
                    .copied(),
                _ => None,
            }
        });
        let Some(exhibit) = exhibit else { return };
        if let Some((tile, _)) = game.world.gate_slot(exhibit) {
            let width = game.world.zoo.width as usize;
            game.camera.centre_on(
                &game.world.zoo,
                (tile % width) as u32,
                (tile / width) as u32,
            );
        }
        self.open_exhibit(exhibit);
    }

    /// Remove (the selected row) or remove all (9.3): the mode ends and the panel is refreshed.
    pub(super) fn staff_remove(&mut self, all: bool) {
        self.end_assign();
        let (Some(game), Some(ui)) = (&mut self.game, &self.ui) else {
            return;
        };
        let Some(id) = game.staff.selected else {
            return;
        };
        let row = match all {
            true => None,
            false => match ui.selected_row(staff::LIST) {
                Some(row) => Some(row),
                None => return,
            },
        };
        assign::unassign(&mut game.world, id, row);
        game.staff.clock.force = true;
        self.tool_lines.push(match row {
            Some(r) => format!("tool unassign {id} row {r}"),
            None => format!("tool unassign {id} all"),
        });
    }

    /// A row of the worker's duty list was selected (9.4): the switch of that kind flips.
    pub(super) fn staff_duty(&mut self, row: i32) {
        let (Some(game), Some(ui)) = (&mut self.game, &mut self.ui) else {
            return;
        };
        let cat = self.renderer.catalogue();
        let kinds = crate::staff::duty_kinds(cat);
        let worker = game
            .staff
            .selected
            .and_then(|id| game.world.units.get_mut(id));
        if let (Some(Part::Staff(s)), Ok(k)) = (worker.map(|u| &mut u.part), usize::try_from(row))
            && k < kinds
        {
            s.off[k] = !s.off[k];
            game.staff.clock.force = true;
        }
        // A second click on the row would deselect it and say -1: no row stays selected.
        ui.select_row(staff::DUTIES, None);
    }

    /// The fire button: the staff member is removed as the bulldozer removes it, after the
    /// question with its monthly salary when `[UI] confirmSell` asks for one (unit placing 10.1).
    pub(super) fn staff_fire(&mut self) {
        if let Some(id) = self.game.as_ref().and_then(|g| g.staff.selected) {
            self.end_assign();
            self.fire(id);
        }
    }

    /// The pick-up button: the selected staff member is picked up as a second click does.
    pub(super) fn staff_pick_up(&mut self) {
        if let Some(id) = self.game.as_ref().and_then(|g| g.staff.selected) {
            self.end_assign();
            self.carry_up(id);
        }
    }

    /// The player ended editing the name field: a text with a visible character is the name.
    pub(super) fn staff_rename(&mut self) {
        let (Some(game), Some(ui)) = (&mut self.game, &self.ui) else {
            return;
        };
        let at = game.staff.selected.and_then(|id| game.world.find(id));
        let bytes = ui.text(staff::NAME).unwrap_or_default();
        if let Some(c) = at.and_then(|at| game.world.zoo.entities[at].common.as_mut())
            && exhibits::visible(&bytes)
        {
            c.name = bytes.iter().map(|&b| crate::text::from_cp1252(b)).collect();
        }
        game.staff.reselect();
    }
}
