//! The zoom, rotate, pause and play buttons of the game screen (camera 8.4, ui layout 7.11) and
//! the keys that press them (input 4.4, 4.5).

use super::*;

pub(super) const ZOOM_IN: i32 = 1007;
pub(super) const ZOOM_OUT: i32 = 1023;
pub(super) const ROTATE_CCW: i32 = 1008;
pub(super) const ROTATE_CW: i32 = 1009;
const PAUSE: i32 = 1071;
const PLAY: i32 = 1072;
const ALL: [i32; 6] = [ZOOM_IN, ZOOM_OUT, ROTATE_CCW, ROTATE_CW, PAUSE, PLAY];

impl App<'_> {
    /// The hotkey of the button `id`: a press of the button when the screen has it (a disabled one
    /// ignores it, the click sound plays), else, with no such button (a bare game), its action.
    pub(super) fn hotkey(&mut self, id: i32) {
        match &mut self.ui {
            Some(ui) if ui.shown(id) => {
                ui.press_button(id);
            }
            _ => self.main_button(id),
        }
    }

    /// Space presses the pause button when running and the play button when paused; with neither
    /// on the screen it toggles the pause.
    pub(super) fn space(&mut self) {
        let Some(ui) = self.ui.as_mut().filter(|u| u.shown(PAUSE) || u.shown(PLAY)) else {
            return self.pause_key();
        };
        if !ui.press_button(PAUSE) {
            ui.press_button(PLAY);
        }
    }

    /// The Pause key: the pause or the unpause itself, no press and no click.
    pub(super) fn pause_key(&mut self) {
        let paused = self.game.as_ref().is_some_and(|g| g.paused);
        self.main_button(if paused { PLAY } else { PAUSE });
    }

    /// Runs the action of every one of these buttons the interface selected since the last look.
    pub(super) fn main_pressed(&mut self) {
        let ids = match &mut self.ui {
            Some(ui) => ui.take_selected_of(&ALL),
            None => return,
        };
        for id in ids {
            self.main_button(id);
        }
    }

    /// The action of the button `id`, whatever pressed it (a game is shown).
    fn main_button(&mut self, id: i32) {
        if self.game.is_none() {
            return;
        }
        match id {
            PAUSE | PLAY => self.pause_to(id == PAUSE),
            ROTATE_CCW | ROTATE_CW => {
                self.start_busy();
                self.rotate(id == ROTATE_CW);
                self.aim(false);
            }
            _ => self.zoom_pressed(id == ZOOM_IN),
        }
    }

    /// Zoom in or out, then the buttons as the press leaves them (camera 8.4 point 4).
    pub(super) fn zoom_pressed(&mut self, zoom_in: bool) {
        self.start_busy();
        let (Some(game), Some(ui)) = (&mut self.game, &mut self.ui) else {
            return self.zoom_blind(zoom_in);
        };
        let at = game.selected_point();
        let camera = &mut game.camera;
        camera.zoom(&game.world.zoo, at, zoom_in);
        let (zoom, narrow) = (camera.zoom, camera.narrow());
        if zoom_in {
            ui.disable(ZOOM_OUT, false);
            ui.disable(ZOOM_IN, zoom == 2 || (zoom == 0 && narrow));
        } else {
            ui.disable(ZOOM_IN, false);
            ui.disable(ZOOM_OUT, zoom == -2);
        }
        self.aim(false);
    }

    /// The zoom of a game with no interface.
    fn zoom_blind(&mut self, zoom_in: bool) {
        if let Some(game) = &mut self.game {
            let at = game.selected_point();
            game.camera.zoom(&game.world.zoo, at, zoom_in);
        }
        self.aim(false);
    }

    /// The zoom check (camera 8.4 point 5): the zoom buttons for the zoom the game has, after a
    /// game opens or the screen mode changes; full screen at a width of 800 zooms 2 out to 0.
    pub(super) fn zoom_check(&mut self) {
        let Some(game) = &mut self.game else { return };
        let (zoom, narrow) = (game.camera.zoom, game.camera.narrow());
        if narrow {
            game.camera.set_fullscreen(&game.world.zoo, true);
        }
        if let Some(ui) = &mut self.ui {
            ui.disable(ZOOM_OUT, zoom < -1);
            ui.disable(ZOOM_IN, zoom >= 2 || (zoom >= -1 && narrow));
        }
    }

    /// Pauses or unpauses: the flag, and the one of the two buttons the player can press (ui
    /// layout 7.11 points 2 and 3).
    pub(super) fn pause_to(&mut self, paused: bool) {
        if let Some(game) = &mut self.game {
            game.set_paused(paused);
        }
        if let Some(ui) = &mut self.ui {
            ui.reveal(PAUSE, !paused);
            ui.reveal(PLAY, paused);
        }
    }

    /// The map view's own update (input 4.5): while the game is paused, a modal's forced pause
    /// too, string 400 is on the message line and stays; after the unpause the line goes at its
    /// next update.
    pub(super) fn pause_message(&mut self) {
        let paused = self.game.is_some() && self.paused();
        // A bare game shows the map view alone: no message line either.
        let Some(ui) = self.ui.as_mut().filter(|_| !self.bare) else {
            return;
        };
        ui.hold_message(paused);
        if paused && !self.pause_said {
            let text = ui.string(400);
            ui.message(&to_cp1252(&text), kinds::PAUSE);
        } else if !paused && self.pause_said {
            ui.end_message();
        }
        self.pause_said = paused;
    }
}
