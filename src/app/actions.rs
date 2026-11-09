//! What the player did in the interface since the last look: the actions the menu and the game
//! act on, and what the menu asked of the game.

use super::*;
use crate::ui::interface::HISTORY_LIST;

impl App<'_> {
    /// The menu and the game act on what the player did since the last look: the answers to the
    /// questions, the actions of the lists and the cash field, the buttons selected.
    pub(super) fn acts(&mut self) {
        self.main_pressed();
        let Some(ui) = &mut self.ui else { return };
        // The list of saved games was closed by its own Back button, or by a game that started.
        if self.listing.is_some() && !ui.shown(crate::menu::FREEFORM_LAYOUT) {
            self.listing = None;
        }
        let selected = ui.take_selected();
        self.build(None);
        let actions: Vec<Action> = self.ui_actions[self.acted..].to_vec();
        self.acted = self.ui_actions.len();
        self.scenario_pressed(&selected, &actions);
        let mut actions = actions.into_iter().peekable();
        while let Some(action) = actions.next() {
            match action {
                Action::Answer(code) => self.answered(code),
                action if self.listing_open() => self.listed(action),
                action if self.game.is_none() => {
                    if let (Some(ui), Some(menu)) = (&mut self.ui, &mut self.menu) {
                        let lines = menu.action(self.renderer.vfs(), ui, action);
                        self.menu_events.extend(lines);
                    }
                    self.requested();
                }
                Action::Row(HISTORY_LIST, row) | Action::Message(row) => self.message_clicked(row),
                Action::Row(exhibit::LIST, row) if self.game.is_some() => self.panel_row(row),
                Action::Row(exhibit::ANIMALS, row) if self.game.is_some() => {
                    self.panel_animal(row);
                }
                Action::Edited(exhibit::NAME) if self.game.is_some() => self.panel_rename(),
                Action::Row(l, r) if show::LISTS.contains(&l) && self.game.is_some() => {
                    self.show_row(l, r);
                }
                Action::Edited(show::ADMISSION) if self.game.is_some() => self.show_typed(),
                Action::Edited(animal::NAME) if self.game.is_some() => self.animal_rename(),
                Action::Row(crate::build::SELECTOR_LIST, row) if self.game.is_some() => {
                    self.expansion_row(row);
                }
                Action::Row(research_panel::LIST, row) if self.game.is_some() => {
                    self.research_tick(row);
                }
                Action::Row(staff::LIST, row) if self.game.is_some() => self.staff_row(row),
                Action::Row(staff::DUTIES, row) if self.game.is_some() => self.staff_duty(row),
                Action::Edited(staff::NAME) if self.game.is_some() => self.staff_rename(),
                Action::Edited(guest::NAME) if self.game.is_some() => self.guest_rename(),
                Action::Edited(research::DEV_CASH) if self.game.is_some() => {
                    // Escape takes the edit back: its key follows the edit at once.
                    if !matches!(actions.peek(), Some(Action::Key(_, Key::Escape))) {
                        self.developer_cash();
                    }
                }
                Action::Edited(
                    panel_building::NAME | panel_building::PLAIN_NAME | panel_building::FILTER_NAME,
                ) if self.game.is_some() => {
                    self.building_rename();
                }
                Action::Edited(panel_building::PRICE) if self.game.is_some() => {
                    self.building_type_price();
                }
                _ => {}
            }
        }
        for id in selected {
            match id {
                SAVE_BUTTON if self.game.is_some() => self.save_pressed(),
                scenario::BUTTON if self.game.is_some() => self.objectives(),
                LOAD_BUTTON if self.game.is_some() => self.ask(Asked::Load),
                crate::menu::SAVED_LOAD if self.listing_open() => self.load_listed(),
                crate::menu::LOAD if self.game.is_none() => self.open_list(),
                MAIN_MENU_BUTTON | EXIT_BUTTON if self.game.is_some() => {
                    let kind = if id == EXIT_BUTTON {
                        Asked::Exit
                    } else {
                        Asked::Leave
                    };
                    self.ask(kind);
                }
                undo::UNDO if self.game.is_some() => self.undo(),
                t if show::BUTTONS.contains(&t) && self.game.is_some() => self.show_pressed(t),
                animal::EXHIBIT if self.game.is_some() => self.animal_exhibit(),
                animal::SELL if self.game.is_some() => self.animal_sell(),
                _ if self.game.is_some() && self.tank_button(id) => {}
                exhibit::TANK_TAB | exhibit::ANIMALS_TAB if self.game.is_some() => {
                    if let Some(game) = &mut self.game {
                        game.panel.clock.force = true;
                    }
                }
                marketing_panel::LESS | marketing_panel::MORE if self.game.is_some() => {
                    self.marketing_fund(id == marketing_panel::MORE);
                }
                research_panel::LESS | research_panel::MORE if self.game.is_some() => {
                    self.research_fund(id == research_panel::MORE);
                }
                t if research_panel::TABS.contains(&t) && self.game.is_some() => {
                    if let Some(game) = &mut self.game {
                        game.research.clock.force = true;
                    }
                }
                research::DEV_RESEARCH if self.game.is_some() => self.developer_research(),
                staff::FIRE if self.game.is_some() => self.staff_fire(),
                staff::PICK_UP if self.game.is_some() => self.staff_pick_up(),
                guest::PICK_UP if self.game.is_some() => self.guest_pick_up(),
                panel_building::UP | panel_building::DOWN if self.game.is_some() => {
                    self.building_price(id == panel_building::UP);
                }
                panel_building::SELL | panel_building::PLAIN_SELL | panel_building::FILTER_SELL
                    if self.game.is_some() =>
                {
                    self.building_sell();
                }
                panel_building::PICK_UP if self.game.is_some() => self.building_pick_up(),
                t if panel_building::UPGRADES.contains(&t) && self.game.is_some() => {
                    self.building_upgrade(t);
                }
                t if panel_building::TABS.contains(&t) && self.game.is_some() => {
                    if let Some(game) = &mut self.game {
                        game.building.clock.force = true;
                    }
                }
                staff::REMOVE | staff::REMOVE_ALL if self.game.is_some() => {
                    self.staff_remove(id == staff::REMOVE_ALL);
                }
                t if staff::TABS.contains(&t) && self.game.is_some() => {
                    if let Some(game) = &mut self.game {
                        game.staff.clock.force = true;
                    }
                }
                t if guest::TABS.contains(&t) && self.game.is_some() => {
                    if let Some(game) = &mut self.game {
                        game.guest.clock.force = true;
                    }
                }
                t if animal::TABS.contains(&t) && self.game.is_some() => {
                    if let Some(game) = &mut self.game {
                        game.animal.clock.force = true;
                    }
                }
                _ if self.game.is_some() => self.build(Some(id)),
                _ if self.game.is_none() => {
                    if let (Some(ui), Some(menu)) = (&mut self.ui, &mut self.menu) {
                        let (lines, exit) = menu.selected(self.renderer.vfs(), ui, id);
                        self.menu_events.extend(lines);
                        self.quit |= exit;
                    }
                    self.requested();
                }
                _ => {}
            }
        }
        // What the menu did to the lists and the fields may have made more to tell.
        if let Some(ui) = &mut self.ui {
            self.ui_events.extend(ui.take_events());
        }
    }

    /// What the menu's last button asked of the game: open a file, or tell the player something.
    pub(super) fn requested(&mut self) {
        match self.menu.as_mut().and_then(Menu::take_request) {
            Some(Request::Open(start)) => {
                self.start_busy();
                let Some(menu) = &self.menu else { return };
                let zoo = menu.load(&start.path, self.renderer.catalogue());
                self.open(&start, zoo);
            }
            Some(Request::Message(text)) => self.say(&text),
            None => {}
        }
    }
}
