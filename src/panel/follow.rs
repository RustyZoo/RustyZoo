//! The picture of the follow button of the unit panels.

use crate::entity::EntityType;
use crate::panel::exhibit::list_icon;
use crate::ui::interface::Interface;
use crate::ui::layout::Picture;
use crate::vfs::Vfs;

/// The animation of an animal's follow button: its egg's, its crate's, else the idle one.
pub fn animal_animation(egg: bool, crated: bool) -> &'static str {
    match (egg, crated) {
        (true, _) => "egg_idle",
        (_, true) => "box_idle",
        _ => "idle",
    }
}

/// Gives the follow button `button` the picture of the unit's type (information panels 4.3.1):
/// its animation `name` (`idle`, or an animal's egg or crate one; else `idle`, else the type's
/// base one) seen from the south-east at rotation 0, never mirrored, whatever the unit does and
/// the camera shows; the button centres it, clips it and plays it from its first frame, looping,
/// on its own clock. A type without such an animation keeps its list icon, if it has one.
pub fn set_follow_icon(
    ui: &mut Interface,
    vfs: &Vfs,
    button: i32,
    ty: Option<&EntityType>,
    name: &str,
) {
    let member = ty.and_then(|t| {
        let all = t.animations();
        let by = |n: &str| all.iter().position(|a| a.name == n);
        let found = by(name).or_else(|| by("idle")).map(|i| &all[i]);
        match found {
            Some(a) => a.member.clone(),
            None => t.base_animation()?.member,
        }
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
