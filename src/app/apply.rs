//! The commands of a pass: what each [`Command`] does to the game.

use super::main_buttons::{ROTATE_CCW, ROTATE_CW, ZOOM_IN, ZOOM_OUT};
use super::*;

impl App<'_> {
    pub(super) fn apply(&mut self, command: Command) {
        if self.intercept(command) {
            return;
        }
        let keys = self.map_keys();
        let following = self.following();
        let shown = self.ui.as_ref().is_none_or(Interface::map_view_shown);
        let modal = self.modal();
        let listing = self.listing_open();
        match (command, &mut self.game) {
            (Command::Pointer(at), _) => self.pointer = at,
            (Command::Quit, _) => self.quit = true,
            (Command::Mouse(mouse), _) => self.mouse(mouse, listing),
            (Command::Scroll(x, y), Some(game)) if keys && !following => {
                game.camera.key_scroll(&game.world.zoo, (x, y));
            }
            (Command::Rotate(cw), Some(_)) if keys => {
                self.hotkey(if cw { ROTATE_CW } else { ROTATE_CCW })
            }
            (Command::Zoom(zoom_in), Some(_)) if keys => {
                self.hotkey(if zoom_in { ZOOM_IN } else { ZOOM_OUT })
            }
            (Command::Grid, Some(game)) if keys => {
                game.grid = !game.grid;
                game.choice = game.grid;
            }
            (Command::DevMode, Some(_)) if keys => {
                if self.timers.speed.toggle() {
                    let on = self.timers.speed.developer();
                    self.lines.dev.push(if on { "on" } else { "off" }.into());
                    self.painted = None;
                    // The same press shows or hides the developer button (main loop 5.2).
                    if let Some(ui) = &mut self.ui {
                        ui.reveal(research::DEV_BUTTON, on);
                    }
                }
            }
            // The speed keys go before the interface's characters (input 3.2, main loop 5.2).
            (Command::Char(byte @ (b'.' | b',' | b'/')), Some(_))
                if keys && self.timers.speed.key(byte) =>
            {
                self.lines
                    .dev
                    .push(format!("speed {}", self.timers.speed.factor()));
            }
            (Command::Fullscreen(on), Some(game)) => {
                let at = game.selected_point();
                game.camera.set_fullscreen(&game.world.zoo, at, on);
                self.zoom_check();
            }
            // The Pause key works through a question too, but the question gives the flag back as
            // it was, so it is left alone there.
            (Command::Pause, Some(_)) if shown && !modal => self.pause_key(),
            (Command::Space, Some(_)) if keys => self.space(),
            // The brush keys press the plus and minus buttons of the terrain panel (input 10).
            (Command::Char(b'+' | b'-'), Some(_)) if keys => {
                if let (Some(ui), Command::Char(key)) = (&mut self.ui, command) {
                    ui.press_button(if key == b'+' { PLUS } else { MINUS });
                }
            }
            // The Delete key presses the clear button, the bulldozer (input 10).
            (Command::Key(Key::Delete, ..), Some(_)) if keys => {
                if let Some(ui) = &mut self.ui {
                    ui.press_button(BULLDOZER);
                }
            }
            // The Backspace key presses the undo button (input 10).
            (Command::Key(Key::Backspace, ..), Some(_)) if keys => {
                if let Some(ui) = &mut self.ui {
                    ui.press_button(undo::UNDO);
                }
            }
            // The keys and characters go to the focused field or list.
            (Command::Key(key, _, shift), _) => {
                if let Some(ui) = &mut self.ui {
                    ui.key(key, shift);
                }
            }
            // The Z key presses the button of the zoo status window (input 10).
            (Command::Char(b'z' | b'Z'), Some(_)) if keys => {
                if let Some(ui) = &mut self.ui {
                    ui.press_button(STATUS_BUTTON);
                }
            }
            // The R key presses the research button (input 10).
            (Command::Char(b'r' | b'R'), Some(_)) if keys => {
                if let Some(ui) = &mut self.ui {
                    ui.press_button(research_panel::BUTTON);
                }
            }
            // The O key presses the Scenario button (input 10).
            (Command::Char(b'o' | b'O'), Some(_)) if keys => {
                if let Some(ui) = &mut self.ui {
                    ui.press_button(scenario::BUTTON);
                }
            }
            // The `$` key donates (input 4.8).
            (Command::Char(b'$'), Some(game)) if keys && !game.paused => {
                game.money.refund(DONATIONS, DOLLAR_KEY);
            }
            (Command::Char(byte), _) => {
                if let Some(ui) = &mut self.ui {
                    ui.char(byte);
                }
            }
            // The map view's keys while it does not have them or there is no game.
            _ => {}
        }
        // A press of the screen's own buttons (a click, a hotkey) runs at once.
        self.main_pressed();
    }

    /// The commands that act on the game as a whole, before the interface and the map see them:
    /// closing, the escape key, saving and loading, awards. Gives whether the command is done (a
    /// full screen change is noted and goes on to the camera).
    fn intercept(&mut self, command: Command) -> bool {
        match command {
            Command::Close => self.close(),
            // Escape closes the expansion selector's list without a choice (entity types 10.5).
            Command::Key(Key::Escape, ..) if self.selector_open() && !self.question_open() => {
                if let (Some(game), Some(ui)) = (&mut self.game, &mut self.ui) {
                    game.build.close_selector(ui);
                }
            }
            Command::Key(Key::Escape, ..) if self.listing_open() && !self.question_open() => {
                self.close_list();
            }
            Command::Key(Key::Escape, ..)
                if self.game.is_some() && self.ui.is_some() && self.map_keys() =>
            {
                self.cancel(true);
            }
            Command::Fullscreen(on) => {
                self.fullscreen = on;
                return false;
            }
            Command::Load(file) => {
                self.load_saved(file);
            }
            Command::Save(file) => self.save_game(file),
            Command::Award(id) => {
                if let Some(game) = &mut self.game {
                    crate::award::add(&mut game.world.zoo.awards, id);
                }
            }
            Command::Key(Key::S, true, _) if self.game.is_some() && self.map_keys() => {
                self.save_key();
            }
            Command::Key(Key::L, true, _) if self.map_keys() => self.load_pressed(),
            _ => return false,
        }
        true
    }

    /// A mouse event. A press or release that belongs to the map view is logged with the tile it
    /// is on, and the terrain tool, if it is the map view's tool, acts on the mouse.
    fn mouse(&mut self, mouse: Mouse, listing: bool) {
        let game = &mut self.game;
        let on_map = self.ui.as_mut().is_some_and(|ui| ui.mouse(mouse)) && !listing;
        let phase = match mouse {
            Mouse::Down | Mouse::Double => Some("down"),
            Mouse::Up => Some("up"),
            _ => None,
        };
        if on_map && mouse == Mouse::Right {
            return self.cancel(false);
        }
        if let (true, Mouse::Down | Mouse::Double, Some(game)) = (on_map, mouse, game.as_mut()) {
            // The next operation starts a new undo list.
            game.world.begin();
            if let Some(ui) = &self.ui {
                let tile = game.camera.tile_under(&game.world.zoo, ui.pointer());
                let (x, y, _) = game.camera.pick(&game.world.zoo, ui.pointer());
                game.build.press(self.renderer.catalogue(), tile, (x, y));
                game.build.sweep_press(tile, ui.pointer());
                if game.build.dragging() {
                    game.selection = None;
                }
            }
        }
        // A release that does not end a press on the map ends a drag without buying.
        if let (false, Mouse::Up, Some(game)) = (on_map, mouse, game.as_mut()) {
            game.build.end_drag();
            game.build.end_sweep();
            game.placing.run.clear();
        }
        if let (true, Some(phase), Some(game), Some(ui)) = (on_map, phase, game.as_mut(), &self.ui)
        {
            let tile = game.camera.tile_under(&game.world.zoo, ui.pointer());
            let tile = tile.map_or("none".to_string(), |(x, y)| format!("{x},{y}"));
            self.lines.tool.push(format!("tool {phase} {tile}"));
        }
        if let Mouse::Move(..) = mouse {
            if let Some(game) = game.as_mut() {
                game.placing.hover = on_map;
            }
            self.aim(true);
        } else if on_map && mouse == Mouse::Up {
            self.release();
        }
        self.terrain_mouse(mouse, on_map);
    }

    /// Turns the view and stages the map from the new side; if that fails the view stays as it was.
    pub(super) fn rotate(&mut self, clockwise: bool) {
        let Some(game) = &mut self.game else {
            return;
        };
        let before = game.camera.clone();
        let at = game.selected_point();
        game.camera.rotate(&game.world.zoo, at, clockwise);
        let extra = Extra {
            marks: game.marks.clone(),
            shown: game.shown(),
            filters: game.world.filter_looks(),
            ripples: self.sim.ripple_ages(),
            ..Extra::default()
        };
        match self
            .renderer
            .stage_with(&game.world.zoo, game.camera.rotation, false, &extra)
        {
            Ok(staged) => {
                game.staged = staged;
                game.extra = extra;
            }
            Err(_) => game.camera = before,
        }
    }
}
