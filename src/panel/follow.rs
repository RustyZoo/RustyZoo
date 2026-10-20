//! The picture of the follow button of the unit panels.

use crate::entity::EntityType;
use crate::panel::exhibit::list_icon;
use crate::ui::interface::Interface;
use crate::ui::layout::Picture;
use crate::vfs::Vfs;

/// Gives the follow button `button` the picture of the unit's type (information panels 4.3.1):
/// its `idle` animation (else the type's base one) seen from the south-east at rotation 0, first
/// frame, never mirrored, whatever the unit does and the camera shows; the button centres it and
/// clips it. A type without such an animation keeps its list icon, if it has one.
pub fn set_follow_icon(ui: &mut Interface, vfs: &Vfs, button: i32, ty: Option<&EntityType>) {
    let member = ty.and_then(|t| {
        let idle = t.animations().into_iter().find(|a| a.name == "idle");
        idle.or_else(|| t.base_animation())?.member
    });
    let shown = member.and_then(|m| ui.tree.picture(vfs, &m));
    let Some(Picture::Animation(a)) = shown else {
        let list = ty
            .and_then(list_icon)
            .and_then(|n| ui.tree.picture(vfs, &n));
        ui.set_icon(button, list);
        return;
    };
    // Facing 4 is the screen direction 3; the view's reflection is not used.
    let file = ui.tree.pictures.animations[a].view_of(4, 0).file;
    ui.set_icon_file(button, shown, Some(file));
}

#[cfg(test)]
pub(crate) mod tests;
