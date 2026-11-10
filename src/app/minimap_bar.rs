//! The minimap of the main bar in the game (map drawing 10): its picture at each interface pass and
//! the click that centres the view.

use super::panel::stop_following;
use super::*;
use crate::minimap::{self, Minimap};

impl App<'_> {
    /// The minimap of a game: the colour file the layout names and the ini file's refresh
    /// interval.
    fn new_minimap(&self) -> Minimap {
        let vfs = self.renderer.vfs();
        let colors = (self.ui.as_ref().and_then(Interface::minimap_file))
            .and_then(|file| vfs.read(&String::from_utf8_lossy(file)).ok())
            .unwrap_or_default();
        Minimap::new(&colors, vfs.ini())
    }

    /// The interface pass of the map view (10.7 point 4): the picture is made again when it is
    /// due, and shown when it differs.
    pub(super) fn minimap_pass(&mut self, now: u64) {
        let slot =
            (self.ui.as_ref()).and_then(|ui| ui.minimap_slot().filter(|_| ui.map_view_shown()));
        let (Some((size, back)), true) = (slot, self.game.is_some()) else {
            return;
        };
        if self.game.as_ref().is_some_and(|g| g.minimap.is_none()) {
            let minimap = self.new_minimap();
            self.game.as_mut().map(|g| g.minimap = Some(minimap));
        }
        playing!(self, game, ui);
        let Some(minimap) = &mut game.minimap else {
            return;
        };
        if minimap.due(now, &game.camera) {
            let entrance = minimap::entrance_of(&game.world, self.renderer.catalogue());
            ui.set_minimap(minimap.draw(&game.world, &game.camera, entrance, size, back));
        }
    }

    /// A click at `at` in the minimap: the view centres on the tile of the cell there, with no range
    /// check (10.8 point 1), and stops following a unit (camera 10.1).
    pub(super) fn minimap_clicked(&mut self, at: (i32, i32)) {
        playing!(self, game, ui);
        let Some((x, y)) = game.minimap.as_ref().and_then(|m| m.tile_at(at)) else {
            return;
        };
        match (u32::try_from(x), u32::try_from(y)) {
            (Ok(x), Ok(y)) => game.camera.centre_on(&game.world.zoo, x, y),
            _ => {
                let point = |t: i64| (64 * t + 32).clamp(i32::MIN.into(), i32::MAX.into()) as i32;
                game.camera.centre_on_point((point(x), point(y), 0));
            }
        }
        stop_following(ui);
    }
}
