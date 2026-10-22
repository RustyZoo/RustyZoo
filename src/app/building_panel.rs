//! The building panels in the game (information panels 4.1; guest buildings 7.2, 10.3): what
//! opens them (a guest building clicked on the map, [`App::select`]), the price buttons, and the
//! look at what the player did to them. The panels themselves are [`crate::panel::building`].

use super::*;
use crate::building::Kind;
use crate::panel::building as panel;
use crate::place::bulldozer::type_of;
use crate::text::to_cp1252;

impl App<'_> {
    /// The panel of entity `id`, if it is a guest building: the charging layout for a building
    /// that charges, the other one for the rest.
    pub(super) fn building_layout(&self, id: u32) -> Option<i32> {
        let (game, cat) = (self.game.as_ref()?, self.renderer.catalogue());
        let ty = type_of(cat, &game.world.zoo.entities[game.world.find(id)?])?;
        let charging = Kind::of(ty, game.world.units.prices.default).commerce;
        crate::month::is_guest_building(ty).then_some(match charging {
            true => panel::CHARGING,
            false => panel::PLAIN,
        })
    }

    /// Selects the building with entity id `id` and shows its panel in `layout`, filled at once.
    pub(super) fn open_building(&mut self, id: u32, layout: i32) {
        self.close_building();
        let (Some(game), Some(ui)) = (&mut self.game, &mut self.ui) else {
            return;
        };
        game.selected = Some(id);
        game.building.select(id, layout);
        ui.show(layout);
    }

    /// Hides the building panels: the next look clears the selection.
    pub(super) fn close_building(&mut self) {
        if let Some(ui) = &mut self.ui {
            ui.set_hidden(panel::CHARGING, true);
            ui.set_hidden(panel::PLAIN, true);
        }
    }

    /// Notices what the player did to the building panel and the selected building since the
    /// last look: a closed panel or a building that left the world clears the selection (`panel
    /// building none`), and the panel sits above the exhibit panel while it is shown (4.2).
    pub(super) fn building_sync(&mut self) {
        let (Some(game), Some(ui)) = (&mut self.game, &mut self.ui) else {
            return;
        };
        let gone = (game.building.selected).is_some_and(|id| game.world.find(id).is_none());
        if gone || !ui.shown(game.building.layout) {
            if game.building.selected.is_some() {
                self.panel_lines.push("building none".into());
            }
            game.building.deselect();
            ui.set_hidden(panel::CHARGING, true);
            ui.set_hidden(panel::PLAIN, true);
        }
        let above = ui.shown(exhibit::LAYOUT);
        ui.set_alt(panel::CHARGING, above);
        ui.set_alt(panel::PLAIN, above);
    }

    /// The up or down button of the price (7.2): the selected building's price moves by 0.25
    /// within its bounds, for every later customer, and the panel shows it at once. Logs
    /// `building price ID PRICE`.
    pub(super) fn building_price(&mut self, up: bool) {
        let max = self.game.as_ref().map_or(0.0, |g| g.world.units.prices.max);
        self.set_price(|kind, price| kind.pressed(price, max, up));
    }

    /// The player ended editing the name field of the panel (guest buildings 10.3): a text with a visible
    /// character is the building's name (exhibits and money 3.6), and the panel shows the name again.
    pub(super) fn building_rename(&mut self) {
        let (Some(game), Some(ui)) = (&mut self.game, &self.ui) else {
            return;
        };
        let field = match game.building.layout {
            panel::CHARGING => panel::NAME,
            _ => panel::PLAIN_NAME,
        };
        let bytes = ui.text(field).unwrap_or_default();
        let at = game.building.selected.and_then(|id| game.world.find(id));
        if let Some(c) = at.and_then(|at| game.world.zoo.entities[at].common.as_mut())
            && super::exhibits::visible(&bytes)
        {
            c.name = bytes.iter().map(|&b| crate::text::from_cp1252(b)).collect();
        }
        game.building.reselect();
    }

    /// The player ended editing the price field (7.2): the text is cleaned and read as a number
    /// ([`panel::typed`], no number is 0) and the price takes the up or down step by the
    /// difference, so it is clamped like a button press.
    pub(super) fn building_type_price(&mut self) {
        let (Some(game), Some(ui)) = (&self.game, &self.ui) else {
            return;
        };
        let text = ui.text(panel::PRICE).unwrap_or_default();
        let symbol = ui.string(SYMBOL);
        let symbol = if symbol.trim().is_empty() {
            "$"
        } else {
            &symbol
        };
        let value = panel::typed(&text, &to_cp1252(symbol));
        let max = game.world.units.prices.max;
        if game.building.layout == panel::CHARGING {
            self.set_price(|kind, price| kind.typed(price, max, value));
        }
    }

    /// Sets the price of the selected building to what `new` makes of its type and price.
    fn set_price(&mut self, new: impl FnOnce(&Kind, f32) -> f32) {
        let Some(game) = &mut self.game else { return };
        let cat = self.renderer.catalogue();
        let Some(id) = game.building.selected else {
            return;
        };
        let at = game.world.find(id);
        let ty = at.and_then(|at| type_of(cat, &game.world.zoo.entities[at]));
        if let Some(price) =
            ty.and_then(|ty| crate::building::set_price(&mut game.world, ty, id, new))
        {
            game.world
                .units
                .building_note(format!("price {id} {price:.2}"));
        }
        game.building.clock.force = true;
    }
}
