//! The tabs of the zoo status window in the game (zoo status 21): the zoo name and the admission
//! price, the graphs, the commerce building list and the completed research list.

use super::exhibits::name_limit;
use super::*;
use crate::panel::exhibit::Clock;
use crate::status::completed::{self, Completed};
use crate::status::{GAP, WINDOW, commerce, graphs, info};
use crate::text::format::{integer, money_cents};

/// What the tabs hold between two looks.
#[derive(Debug, Default)]
pub(super) struct Window {
    clock: Clock,
    /// The window and the commerce tab were shown at the last look.
    shown: bool,
    commerce_shown: bool,
    sort: commerce::Sort,
    /// The entity of each row of the commerce list, and the one that was selected.
    ids: Vec<u32>,
    selected: Option<u32>,
    /// How many buildings the world held at the last look.
    buildings: usize,
    /// The last zoo name the field showed or accepted.
    copy: Vec<u8>,
    pub(super) completed: Completed,
}

/// The tab buttons of the window and of its Graphs tab: the first of each set is the one a game
/// starts with.
const TABS: [i32; 6] = [4104, 4105, 4153, 4154, 4180, 4181];
const GRAPH_TABS: [i32; 4] = [4106, 13982, 13983, 13984];

/// A new game or a load puts the tabs back to the first of each set: the next opening shows the
/// zoo info tab and the rating graph (21.1 step 2).
pub(super) fn reset_tabs(ui: &mut Interface) {
    for set in [&TABS[..], &GRAPH_TABS[..]] {
        for (n, &button) in set.iter().enumerate() {
            ui.set_button(button, n == 0, true);
        }
    }
}

impl Window {
    /// The interface is new: the research list is written again.
    pub(super) fn fresh(&mut self) {
        self.completed.changed = true;
        self.shown = false;
    }
}

impl App<'_> {
    /// Runs the tabs' updates: the research list when it changed, and, while the window is shown,
    /// the refresh at its opening, each ten seconds, and when a building came or went (21.1).
    pub(super) fn zoo_window_pass(&mut self, now: u64, ticking: bool) {
        playing!(self, game, ui);
        let w = &mut game.window;
        if std::mem::take(&mut w.completed.changed) {
            let vfs = self.renderer.vfs();
            let rows = (w.completed.rows.iter())
                .map(|(name, icon)| crate::ui::widgets::Row {
                    text: to_cp1252(&ui.string(*name)),
                    icon: (!icon.is_empty())
                        .then(|| ui.tree.picture(vfs, icon))
                        .flatten(),
                    ..Default::default()
                })
                .collect();
            ui.refill_rows(completed::LIST, rows);
        }
        let buildings = (game.world.zoo.entities.iter())
            .filter(|e| e.class == "building")
            .count();
        let moved = std::mem::replace(&mut w.buildings, buildings) != buildings;
        let shown = ui.shown(WINDOW);
        let opened = shown && !w.shown;
        w.shown = shown;
        let on = shown && ui.shown(commerce::LAYOUT);
        w.clock.force |= opened || moved;
        let refreshed = shown && w.clock.due(now, ticking, GAP);
        let tab = on && !w.commerce_shown;
        w.commerce_shown = on;
        if opened {
            self.zoo_name_fill();
        }
        if refreshed {
            self.zoo_info();
            self.graphs_fill();
        }
        if on && (refreshed || tab) {
            self.commerce_fill();
        }
    }

    /// The zoo name field shows the name of the entrance building and takes the length limit of
    /// the name fields; with no such building it keeps its text (21.2 step 2).
    fn zoo_name_fill(&mut self) {
        playing!(self, game, ui);
        ui.set_char_limit(info::NAME, name_limit(self.renderer.vfs().ini()));
        if let Some(at) = info::entrance(&game.world) {
            let name = game.world.zoo.entities[at].common.as_ref().map(|c| &c.name);
            let bytes = to_cp1252(name.map_or("", String::as_str));
            ui.set_text(info::NAME, &bytes);
            game.window.copy = bytes;
        }
    }

    /// The zoo name field was edited (21.2 step 3): a text with a character other than a space
    /// is the entrance building's name; any other puts the previous name back.
    pub(super) fn zoo_rename(&mut self) {
        playing!(self, game, ui);
        let text = ui.text(info::NAME).unwrap_or_default();
        if !info::kept(&text) {
            ui.set_text(info::NAME, &game.window.copy);
        } else if let Some(at) = info::entrance(&game.world) {
            game.world.rename_entity(at, &text);
            game.window.copy = text;
        }
    }

    /// The counts and the prices of the zoo info tab (21.2 steps 4 and 5).
    fn zoo_info(&mut self) {
        playing!(self, game, ui);
        let counts = info::Counts::of(&game.world, self.renderer.catalogue());
        for (id, n) in info::COUNTS.into_iter().zip(counts.list()) {
            ui.set_text(id, n.to_string().as_bytes());
        }
        let members = info::members(game.money.total[crate::money::MEMBERS]);
        ui.set_text(info::MEMBERS, integer(members, true).as_bytes());
        self.zoo_prices();
    }

    /// The adult and child prices and the two buttons; the price field only while it has not the
    /// keyboard focus (21.2 step 5).
    fn zoo_prices(&mut self) {
        playing!(self, game, ui);
        let _ = game;
        let status = &self.sim.status;
        let (price, min, max) = (
            status.price,
            status.settings.min_price,
            status.settings.max_price,
        );
        let symbol = Symbol::new(ui.string(SYMBOL));
        let text = |p: f32| to_cp1252(&money_cents(&symbol, f64::from(p), false));
        ui.set_text(info::ADULT, &text(price));
        ui.set_text(info::CHILD, &text(price * 0.5));
        if !ui.has_focus(info::PRICE) {
            ui.set_text(info::PRICE, &text(price));
        }
        ui.set_enabled(info::UP, price < max);
        ui.set_enabled(info::DOWN, price > min);
    }

    /// The up or down button of the price (21.2 step 6).
    pub(super) fn zoo_price_step(&mut self, up: bool) {
        let status = &mut self.sim.status;
        let step = if up { info::STEP } else { -info::STEP };
        status.set_price(status.price + step);
        self.zoo_prices();
    }

    /// The price field was edited (21.2 step 6): the typed number is the price, limited; the
    /// field then shows the typed number, not the limited price, until the next refresh.
    pub(super) fn zoo_price_typed(&mut self) {
        let Some(ui) = &self.ui else { return };
        let typed = building_panel::typed_number(ui, info::PRICE);
        self.sim.status.set_price(typed as f32);
        self.zoo_prices();
        if let Some(ui) = &mut self.ui {
            let symbol = Symbol::new(ui.string(SYMBOL));
            ui.set_text(info::PRICE, &to_cp1252(&money_cents(&symbol, typed, false)));
        }
    }

    /// The four graphs are written again from the accounts, shown or not (21.3 step 2).
    fn graphs_fill(&mut self) {
        playing!(self, game, ui);
        let words = Words::new(|id| ui.string(id as i32));
        let symbol = ui.string(SYMBOL);
        for (n, id) in graphs::GRAPHS.into_iter().enumerate() {
            let points = graphs::points(&game.money, n, &words);
            ui.set_graph(id, points, &symbol);
        }
    }

    /// A line or bar button of a graph (21.3 step 3).
    pub(super) fn graph_switch(&mut self, button: i32) {
        if let (Some(ui), Some((graph, style))) = (&mut self.ui, graphs::switched(button)) {
            ui.set_graph_style(graph, style);
        }
    }

    /// A sort button of the commerce list (21.4 step 3): the list is made again.
    pub(super) fn commerce_sorted(&mut self, button: i32) {
        if let Some(game) = &mut self.game
            && game.window.sort.press(button)
        {
            self.commerce_fill();
        }
    }

    /// The commerce list is made again: the buildings that sell in the order of the sort, the
    /// header of the key coloured, the row of the entity that was selected selected again.
    fn commerce_fill(&mut self) {
        playing!(self, game, ui);
        let w = &mut game.window;
        w.selected = (ui.selected_row(commerce::LIST))
            .and_then(|r| w.ids.get(r).copied())
            .or(w.selected);
        let now = self.sim.calendar().elapsed_ms();
        let mut rows = commerce::rows(
            &game.world,
            self.renderer.catalogue(),
            |id| ui.string(id),
            now,
        );
        commerce::order(&mut rows, &w.sort);
        w.ids = rows.iter().map(|r| r.id).collect();
        let symbol = Symbol::new(ui.string(SYMBOL));
        ui.refill_rows(commerce::LIST, commerce::list(&rows, &symbol));
        let again = w
            .selected
            .and_then(|id| w.ids.iter().position(|&i| i == id));
        ui.select_row(commerce::LIST, again);
        for (key, header) in commerce::HEADERS.into_iter().enumerate() {
            let colour = if key == w.sort.key {
                commerce::CURRENT
            } else {
                commerce::OTHER
            };
            ui.set_fore_color(header, colour);
        }
    }

    /// A row of the commerce list was selected (21.4 step 6): a building that is still there and
    /// not the selected one is centred in the view and selected with its panel.
    pub(super) fn commerce_row(&mut self, row: i32) {
        playing!(self, game);
        let id = usize::try_from(row)
            .ok()
            .and_then(|r| game.window.ids.get(r).copied());
        let Some(id) = id else { return };
        game.window.selected = Some(id);
        if game.world.find(id).is_some() && game.selected != Some(id) {
            self.centre_select(id);
        }
    }

    /// A double click on the commerce list (21.4 step 7): with a building selected the window is
    /// hidden.
    pub(super) fn commerce_double(&mut self) {
        let selected = self.game.as_ref().and_then(|g| g.selected);
        if selected.is_some_and(|id| self.building_layout(id).is_some())
            && let Some(ui) = &mut self.ui
        {
            ui.set_hidden(WINDOW, true);
        }
    }

    /// Whether the commerce list has the keyboard focus in the shown window.
    pub(super) fn commerce_focused(&self) -> bool {
        (self.ui.as_ref()).is_some_and(|ui| ui.shown(WINDOW) && ui.has_focus(commerce::LIST))
    }

    /// The Delete key in the commerce list (21.4 step 8): the selected building is sold as the
    /// sell button of its panel does, and the list gets the keyboard focus back.
    pub(super) fn commerce_sell(&mut self) {
        let selected = self.game.as_ref().and_then(|g| g.selected);
        if let Some(id) = selected.filter(|&id| self.building_layout(id).is_some()) {
            self.remove_one(id);
        }
        if let Some(ui) = &mut self.ui {
            ui.focus(commerce::LIST);
        }
    }
}
