//! The tank tab of the exhibit panel (exhibits and money 3.10, 4.7): the wall buttons, the two
//! fill buttons and the drain button. Each press pays as the other tools do (refused with 10002
//! when the cash does not cover it, booked as construction), and a refusal of the water shows its
//! message in the message line (`tool message N`).

use super::*;
use crate::exhibit::price::Prices;
use crate::money::CONSTRUCTION;
use crate::panel::exhibit::type_at;
use crate::place::NO_FUNDS;

/// The strings that tell the water of a tank cannot be filled (the object's name goes in) and
/// cannot be drained.
const NOT_FILLED: i32 = 10550;
const NOT_DRAINED: i32 = 10551;

impl App<'_> {
    /// The tank of the exhibit the panel shows, with its prices.
    fn panel_tank(&self) -> Option<(u32, Prices)> {
        let game = self.game.as_ref()?;
        let id = game.panel.current?;
        let prices = Prices::of(&game.world, self.renderer.catalogue(), id)?;
        Some((id, prices))
    }

    /// Shows message `id` (with `name` in its `%s`) in the message line, and logs it.
    fn tank_message(&mut self, id: i32, name: &str) {
        self.tool_lines.push(format!("tool message {id}"));
        if let Some(ui) = &mut self.ui {
            let text = ui.string(id).replacen("%s", name, 1);
            ui.message(&to_cp1252(&text));
        }
    }

    /// The wall buttons (4.7): the walls go one step up or down; the price of the step is paid
    /// only when the height really changed, and a raise the cash does not cover is refused.
    pub(super) fn tank_wall(&mut self, up: bool) {
        let Some((id, prices)) = self.panel_tank() else {
            return;
        };
        let Some(game) = &mut self.game else { return };
        let (Some(tank), floor) = (game.world.tank(id), game.world.tank_config().height) else {
            return;
        };
        let price = prices.step(tank.height, floor[usize::from(tank.show)], up, tank.filled);
        if price > 0.0 && !game.money.can_pay(price) {
            return self.tank_message(NO_FUNDS, "");
        }
        let next = if up {
            tank.height.saturating_add(1)
        } else {
            tank.height.saturating_sub(1)
        };
        game.world.set_height(id, next);
        let moved = game.world.tank(id).is_some_and(|t| t.height != tank.height);
        if moved {
            game.money.charge(CONSTRUCTION, price);
        }
        game.panel.clock.force = true;
        self.exhibits_told();
    }

    /// The fill buttons (4.7), enabled while the tank is empty and not filled: the cash is
    /// checked, the salinity set (a change drains first; an animal in the tank refuses that with
    /// 10551, and the rest goes on), the price paid, then the tank filled. An object that may not be in water refuses the fill with 10550
    /// naming it; the price stays paid.
    pub(super) fn tank_fill(&mut self, salt: bool) {
        let Some((id, prices)) = self.panel_tank() else {
            return;
        };
        let Some(game) = &mut self.game else { return };
        if (game.world.tank(id)).is_none_or(|t| t.level > 0 || t.filled) {
            return;
        }
        let price = prices.fill[usize::from(!salt)];
        if price > 0.0 && !game.money.can_pay(price) {
            return self.tank_message(NO_FUNDS, "");
        }
        // An animal in the empty tank refuses the drain and says so, but the rest goes on.
        let told = !game.world.set_salinity(id, salt);
        game.money.charge(CONSTRUCTION, price);
        let refused = game.world.fill(id);
        game.panel.clock.force = true;
        if told {
            self.tank_message(NOT_DRAINED, "");
        }
        let Some(game) = &mut self.game else { return };
        if let Some(object) = refused {
            let cat = self.renderer.catalogue();
            let world = &game.world;
            let ty = world.find(object).and_then(|at| type_at(world, cat, at));
            let name = ty.and_then(|t| (t.name_id != 0).then_some(t.name_id));
            let name = name.map_or(String::new(), |n| {
                self.ui.as_ref().map_or(String::new(), |u| u.string(n))
            });
            self.tank_message(NOT_FILLED, &name);
        }
        self.exhibits_told();
    }

    /// The drain button (4.7): free; refused with 10551 while an animal is in the tank.
    pub(super) fn tank_drain(&mut self) {
        let Some((id, _)) = self.panel_tank() else {
            return;
        };
        let Some(game) = &mut self.game else { return };
        let drained = game.world.drain(id);
        game.panel.clock.force = true;
        if !drained {
            self.tank_message(NOT_DRAINED, "");
        }
        self.exhibits_told();
    }
}
