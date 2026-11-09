//! The snapshot button and the three hide/show buttons of the main bar (map drawing 15): foliage
//! 1066, buildings 1067 and guests 1068 flip what the map view draws, 1099 writes a picture file.

use super::bulldozer::Plain;
use super::*;
use crate::config::Config;
use crate::render::Hide;
use crate::text::format::expand;
use crate::timing::ini_whole;

pub(super) const FOLIAGE: i32 = 1066;
pub(super) const BUILDINGS: i32 = 1067;
pub(super) const GUESTS: i32 = 1068;
pub(super) const SNAPSHOT: i32 = 1099;
/// The toggle buttons, and the tooltip each shows while its element is hidden ("show ...").
const TOGGLES: [(i32, i32); 3] = [(FOLIAGE, 1065), (BUILDINGS, 1069), (GUESTS, 1070)];
/// The strings of the snapshot: the folder name, and the message line's success (with the path)
/// and failure.
const FOLDER: i32 = 22006;
const WRITTEN: i32 = 10507;
const FAILED: i32 = 10508;

/// What the buttons of a game hold: what is hidden, and whether Shift is held.
#[derive(Default)]
pub(super) struct View {
    pub(super) hide: Hide,
    shift: bool,
}

impl View {
    /// The Shift key went down or up (`shift on`, `shift off`).
    pub(super) fn set_shift(&mut self, on: bool) {
        self.shift = on;
    }
}

#[cfg(test)]
thread_local! {
    /// The folder a test writes its snapshots to, in place of the user's.
    pub(super) static TEST_FOLDER: std::cell::RefCell<Option<PathBuf>> = const { std::cell::RefCell::new(None) };
}

impl App<'_> {
    /// Ctrl+B, Ctrl+F, Ctrl+V and Ctrl+P press the button `id`; with none on the screen, or one
    /// that is greyed, nothing happens. The press of the snapshot button reads `shift`, the state
    /// of Shift with the key.
    pub(super) fn press_view(&mut self, id: i32, shift: Option<bool>) {
        if let (Some(game), Some(ui)) = (&mut self.game, &mut self.ui) {
            if let Some(on) = shift {
                game.view.set_shift(on);
            }
            ui.press_button(id);
        }
    }

    /// The hide/show buttons and the snapshot button the interface selected or deselected since the
    /// last look: each toggle flips its bit (map drawing 15.1 point 2), the snapshot is taken.
    pub(super) fn view_pressed(&mut self) {
        let Some(ui) = &mut self.ui else { return };
        let mut ids = ui.take_selected_of(&[FOLIAGE, BUILDINGS, GUESTS, SNAPSHOT]);
        ids.extend(ui.take_deselected());
        for id in ids {
            if id == SNAPSHOT || TOGGLES.iter().any(|t| t.0 == id) {
                if let Some(scenario) = self.sim.scenario() {
                    scenario.gui().pressed(id, None);
                }
                match id {
                    SNAPSHOT => self.snapshot(),
                    _ => self.view_flip(id),
                }
            }
        }
    }

    /// The press of the toggle `id`: its bit flips, then the buttons follow (point 3).
    fn view_flip(&mut self, id: i32) {
        let Some(game) = &mut self.game else { return };
        let hide = &mut game.view.hide;
        match id {
            FOLIAGE => hide.foliage = !hide.foliage,
            BUILDINGS => hide.buildings = !hide.buildings,
            _ => hide.guests = !hide.guests,
        }
        self.view_sync();
    }

    /// Each of the three buttons as its bit leaves it: with the bit clear (hidden) down and with
    /// the tooltip that says "show", else up and with the tooltip that says "hide". A game that
    /// opens starts with all three set.
    pub(super) fn view_sync(&mut self) {
        let (Some(game), Some(ui)) = (&self.game, &mut self.ui) else {
            return;
        };
        let hide = game.view.hide;
        for ((id, show), hidden) in
            TOGGLES
                .into_iter()
                .zip([hide.foliage, hide.buildings, hide.guests])
        {
            ui.set_down(id, hidden);
            ui.set_help(id, if hidden { show } else { id });
        }
    }

    /// The snapshot button (point 8 to 12): the busy cursor, the picture, the file and the message.
    fn snapshot(&mut self) {
        if self.game.is_none() {
            return;
        }
        self.start_busy();
        let shift = self.game.as_ref().is_some_and(|g| g.view.shift);
        // Shift held: the screen as it is. Else the map view alone, drawn again.
        if shift {
            self.paint();
        } else {
            self.restage();
            if let Some(game) = &self.game {
                let (x, y) = game.camera.scroll;
                let target = if self.ui.is_some() {
                    &mut self.map
                } else {
                    &mut self.frame
                };
                game.staged.draw(target, x, y, game.camera.zoom, game.grid);
            }
        }
        let image = if shift || self.ui.is_none() {
            &self.frame
        } else {
            &self.map
        };
        let ini = Config::parse(self.renderer.vfs().ini()).unwrap_or_default();
        let jpeg = ini_whole(&ini, "UI", "useJPEG").is_none_or(|n| n != 0);
        let quality =
            ini_whole(&ini, "UI", "JPEGQuality").unwrap_or(crate::snapshot::DEFAULT_QUALITY);
        let name = self
            .ui
            .as_ref()
            .map(|ui| ui.string(FOLDER))
            .unwrap_or_default();
        #[cfg(test)]
        let test = TEST_FOLDER.with(|f| f.borrow().clone());
        #[cfg(not(test))]
        let test = None;
        let written = test
            .or_else(|| crate::userdir::screenshots(&name))
            .ok_or_else(|| "the system does not say where".to_string())
            .and_then(|dir| crate::snapshot::write(&dir, image, jpeg, quality));
        let Some(ui) = &mut self.ui else { return };
        let text = match written {
            Ok(path) => expand(
                &Plain,
                &ui.string(WRITTEN),
                &[Arg::Text(&path.to_string_lossy())],
            ),
            Err(why) => {
                eprintln!("rustyzoo: warning: snapshot: {why}");
                ui.string(FAILED)
            }
        };
        ui.message(&to_cp1252(&text), kinds::PLAIN);
    }
}
