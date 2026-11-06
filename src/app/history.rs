//! The message history's meeting with the game (ui layout 4.14): the icon of a message about an
//! entity and the click that goes to it.

use super::*;
use crate::ui::interface::Message;
use crate::vfs::Vfs;

/// The picture in front of a message about a tile or an entity.
const ICON: &str = "ui/sharedui/mescros/mescros";

/// `m`, about the entity `id` that the world found: it has the icon in the history and the line.
pub(super) fn about<'a>(m: Message<'a>, ui: &mut Interface, vfs: &Vfs, id: u32) -> Message<'a> {
    m.about(id, ui.tree.picture(vfs, ICON))
}

impl App<'_> {
    /// A click on message `row` of the history, or on the line (point 7): the view centres on the
    /// entity of the message, which becomes the selected one with its panel, the bulldozer
    /// released. The message on the line gives way at the next update, whichever row it was.
    pub(super) fn message_clicked(&mut self, row: i32) {
        playing!(self, game, ui);
        let Some(Some(id)) = ui.message_click(row) else {
            return;
        };
        let tile = game.world.find(id).and_then(|at| {
            let c = game.world.zoo.entities[at].common.as_ref()?;
            Some((c.x.div_euclid(64), c.y.div_euclid(64)))
        });
        let Some((Ok(x), Ok(y))) = tile.map(|(x, y)| (u32::try_from(x), u32::try_from(y))) else {
            return;
        };
        game.camera.centre_on(&game.world.zoo, x, y);
        if game.selected != Some(id) {
            ui.set_button(BULLDOZER, false, true);
            game.selected = Some(id);
        }
    }
}
