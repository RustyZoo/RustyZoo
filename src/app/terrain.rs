//! The terrain tool in the game: a terrain tab of the construction panel becomes the map view's
//! tool, the mouse on the map reaches it, the buttons of the terrain panel act on it, and what it
//! highlights is drawn with the map. The rules are in [`crate::terrain`], the widgets in
//! [`crate::build::terrain`].

use super::*;
use crate::build::terrain::{self as panel, Press};
use crate::entity::Catalogue;
use crate::terrain::{Left, Mode, habit};
use crate::zoo::Entity;
use crate::zoom::Zoom;

/// The string of the message that a session which the cash cannot cover is cancelled with.
const SHORT: i32 = 3363;

/// What the terrain panel showed last, to write it again only when it changes.
#[derive(Debug, Clone, PartialEq)]
pub(super) struct Shown {
    mode: Mode,
    size: usize,
    kind: u8,
    cost: i64,
    brush: i64,
    cash: i64,
}

impl Game {
    /// Tells in the log when the amount next to the pointer changed: `terrain amount -120`, or
    /// `terrain amount none` when none is written.
    fn log_amount(&mut self, lines: &mut Vec<String>) {
        let amount = self.cost();
        if amount != self.pointed {
            self.pointed = amount;
            lines.push(format!(
                "amount {}",
                amount.map_or("none".to_string(), |a| a.to_string())
            ));
        }
    }

    /// The highlights of the tool, which the next draw stages when they changed.
    pub(super) fn terrain_marks(&mut self, cat: &Catalogue) {
        self.marks = self.terrain.marks(&self.world, &|e: &Entity| habit(cat, e));
    }
}

impl App<'_> {
    /// The tool follows the tab the construction panel has selected: the tool left is accepted
    /// when the cash covers it and cancelled, with a message, when it does not (terrain editing
    /// 8.6); the tool entered clears its session. The tools force the grid on and give it back
    /// to the player's choice (input 4.8).
    pub(super) fn terrain_sync(&mut self) {
        let (Some(game), Some(ui)) = (&mut self.game, &mut self.ui) else {
            return;
        };
        let wanted = game.build.terrain_mode();
        let mut short = false;
        if wanted != game.terrain.mode() {
            if game.terrain.mode().is_some() {
                let lines = &mut self.terrain_lines;
                let cat = self.renderer.catalogue();
                let habits = |e: &Entity| habit(cat, e);
                let left = game
                    .terrain
                    .leave(&mut game.world, &habits, &mut game.money, lines);
                lines.push("leave".into());
                short = left == Left::Short;
                game.grid = game.choice;
            }
            if let Some(mode) = wanted {
                game.terrain.enter(mode);
                game.grid = true;
                panel::show(ui, self.renderer.vfs(), &game.terrain, mode);
                self.terrain_lines.push(format!("enter {}", mode.name()));
            }
            game.shown = None;
        }
        let cat = self.renderer.catalogue();
        game.terrain_marks(cat);
        self.terrain_refresh();
        if short {
            let text = self.ui.as_ref().map(|ui| ui.string(SHORT));
            self.terrain_lines.push("short".into());
            self.say(&text.unwrap_or_default());
        }
    }

    /// Writes the panel again when what it shows changed.
    pub(super) fn terrain_refresh(&mut self) {
        let (Some(game), Some(ui)) = (&mut self.game, &mut self.ui) else {
            return;
        };
        let Some(mode) = game.terrain.mode() else {
            return;
        };
        let cost = game.terrain.cost(&game.world);
        let whole = |v: f32| crate::text::format::whole_units(f64::from(v));
        let shown = Some(Shown {
            mode,
            size: game.terrain.size(),
            kind: game.terrain.kind(),
            cost: whole(cost),
            brush: whole(game.terrain.brush_cost()),
            cash: whole(game.money.cash),
        });
        if game.shown != shown {
            panel::refresh(ui, &game.terrain, cost, &game.money);
            game.shown = shown;
        }
    }

    /// The mouse on the map reaches the tool (terrain editing 5.6 and 6.3). `on_map` says the
    /// message belongs to the map view.
    pub(super) fn terrain_mouse(&mut self, mouse: Mouse, on_map: bool) {
        let (Some(game), Some(ui)) = (&mut self.game, &mut self.ui) else {
            return;
        };
        if game.terrain.mode().is_none() {
            return;
        }
        if !on_map {
            // The amount next to the pointer goes away with the pointer.
            ui.touch();
            game.log_amount(&mut self.terrain_lines);
            return;
        }
        let cat = self.renderer.catalogue();
        let habits = |e: &Entity| habit(cat, e);
        let at = ui.pointer();
        let camera = &game.camera;
        let zoom = Zoom::from_value(camera.zoom);
        let lines = &mut self.terrain_lines;
        let under = |world: &World, at| camera.tile_under(&world.zoo, at);
        match mouse {
            Mouse::Down | Mouse::Double => {
                let tile = under(&game.world, at);
                game.terrain
                    .press(&mut game.world, &habits, at, tile, lines);
            }
            Mouse::Move(..) => {
                let before = game.terrain.tiles(&game.world);
                game.terrain
                    .moved(&mut game.world, &habits, at, &under, zoom, lines);
                let after = game.terrain.tiles(&game.world);
                if before != after
                    && let Some(first) = after.first()
                {
                    let width = game.world.zoo.width as usize;
                    lines.push(format!(
                        "brush {} {},{}",
                        game.terrain.size(),
                        first % width,
                        first / width
                    ));
                }
                // The amount next to the pointer follows it.
                ui.touch();
            }
            Mouse::Up => game.terrain.release(),
            _ => {}
        }
        game.log_amount(lines);
        game.terrain_marks(cat);
        self.terrain_refresh();
    }

    /// Every frame, button up, the tile under the pointer is looked at: a new one shows the help of
    /// its terrain type (terrain editing 7.3). A stroke released on another tile shows it too.
    pub(super) fn terrain_help(&mut self) {
        let (Some(game), Some(ui)) = (&mut self.game, &mut self.ui) else {
            return;
        };
        if !game.hover {
            return;
        }
        let tile = game.camera.tile_under(&game.world.zoo, ui.pointer());
        if let Some(help) = game.terrain.help_at(&game.world, tile) {
            ui.help_tip(help);
        }
    }

    /// The button `id` of the interface was pressed: if it belongs to the terrain panel, it acts.
    pub(super) fn terrain_pressed(&mut self, id: i32) {
        let (Some(game), Some(ui)) = (&mut self.game, &mut self.ui) else {
            return;
        };
        let Some(mode) = game.terrain.mode() else {
            return;
        };
        let Some(press) = panel::pressed(ui, id) else {
            return;
        };
        let cat = self.renderer.catalogue();
        let habits = |e: &Entity| habit(cat, e);
        let lines = &mut self.terrain_lines;
        let size = game.terrain.size();
        match press {
            Press::Plus | Press::Minus => {
                let wanted = if press == Press::Plus {
                    size.saturating_add(1)
                } else {
                    size.saturating_sub(1)
                };
                if game.terrain.set_size(wanted) {
                    lines.push(format!("size {wanted}"));
                    game.terrain.refresh(&game.world, &habits);
                }
            }
            Press::Accept => {
                game.terrain
                    .accept(&mut game.world, &habits, &mut game.money, lines);
            }
            Press::Cancel => game.terrain.cancel(&mut game.world, &habits, lines),
            Press::Mode(id, contiguous, level) => {
                panel::mode_text(ui, id);
                game.terrain.set_shaping(contiguous, level);
                lines.push(format!("mode {id}"));
            }
            Press::Type(at) => {
                let kind = game.terrain.listed().get(at).map(|t| t.kind);
                if mode == Mode::Paint
                    && let Some(kind) = kind
                    && game.terrain.set_kind(kind)
                {
                    lines.push(format!("type {kind}"));
                    game.terrain.refresh(&game.world, &habits);
                }
            }
        }
        game.terrain_marks(cat);
        self.terrain_refresh();
    }
}
