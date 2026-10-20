//! The picture of the follow button of the unit panels.

use crate::entity::EntityType;
use crate::panel::exhibit::list_icon;
use crate::ui::interface::Interface;
use crate::ui::layout::Picture;
use crate::unit::Unit;
use crate::vfs::Vfs;

/// Gives the follow button `button` the picture of `unit` (information panels 4.3, 6.1): its
/// current frame, in the animation it plays (its `idle` one, else the type's base one, when the
/// type has no such), the view it faces on a map turned by `rotation`. Without such a picture the
/// button shows the list icon of the type, if it has one.
pub fn set_follow_icon(
    ui: &mut Interface,
    vfs: &Vfs,
    button: i32,
    ty: Option<&EntityType>,
    unit: &Unit,
    (facing, rotation): (u32, u8),
) {
    let animations = ty.map(EntityType::animations).unwrap_or_default();
    let named = |n: &str| animations.iter().find(|a| a.name == n);
    let member = named(&unit.animation.name)
        .or_else(|| named("idle"))
        .and_then(|a| a.member.clone())
        .or_else(|| ty?.base_animation()?.member);
    let shown = member.and_then(|m| ui.tree.picture(vfs, &m));
    let Some(Picture::Animation(a)) = shown else {
        let list = ty
            .and_then(list_icon)
            .and_then(|n| ui.tree.picture(vfs, &n));
        ui.set_icon(button, list);
        return;
    };
    let view = ui.tree.pictures.animations[a].view_of((facing % 8) as u8, rotation);
    let frame = (view.file, unit.animation.shown() as usize, view.mirror);
    ui.set_icon_frame(button, shown, frame);
}

#[cfg(test)]
pub(crate) mod tests;
