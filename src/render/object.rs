//! The pictures of the objects: the animation an object shows, the faces of a fence, the markers
//! and the object itself.

use super::placed::{Pic, Placed};
use super::*;

impl<'a> MapRenderer<'a> {
    /// The `.ani` member an object shows at rest.
    pub(super) fn member(&self, scene: &Scene, o: usize, meta: &Meta) -> Option<String> {
        let (obj, ty) = (&scene.objects[o], meta.ty);
        let wanted = match ty.class {
            // A tank filter shows `off` without water, else `broken`, `decayed` or `idle` by its
            // health (sprite drawing 3).
            "filter" => {
                let decayed = crate::build::number(ty, "cDecayedHealth").unwrap_or(5.0) as i64;
                match meta.life.map(i64::from) {
                    _ if meta.off => "off",
                    Some(h) if h < 1 => "broken",
                    Some(h) if h <= decayed => "decayed",
                    _ => "idle",
                }
                .to_string()
            }
            "fences" | "tankwall" => {
                let gate = ty.subtype.first() == Some(&b'g');
                // A gate shows the intact row whatever its life; the others by life.
                let decayed = ty
                    .config
                    .as_deref()
                    .and_then(|c| {
                        c.section([&ty.subtype[..], b"/Characteristics/Integers"].concat())
                    })
                    .and_then(|s| s.get("cDecayedLife"))
                    .and_then(to_int)
                    .unwrap_or(0);
                let row = match meta.life {
                    Some(life) if !gate && life < 1 => "broke",
                    Some(life) if !gate && life <= decayed => "det",
                    _ => "idle",
                };
                match scene.tile_at(obj.x, obj.y) {
                    Some(t) => slope_name(row, &scene.tiles[t], obj.facing),
                    None => row.to_string(),
                }
            }
            // An animal inside a tank swims: below the water it shows `subidle`, at the surface
            // `surfidle` (sprite drawing 3.6).
            "animals" if meta.mode == 3 => {
                let t = scene.tile_at(obj.x, obj.y);
                match t.and_then(|t| scene.surface(t)) {
                    Some(surface) if obj.z >= surface => "surfidle".to_string(),
                    Some(_) => "subidle".to_string(),
                    None => "idle".to_string(),
                }
            }
            "animals" | "guests" | "keeper" | "maint" | "tour" => {
                meta.shown.map_or("idle", |(name, _)| name).to_string()
            }
            // A dung pile shows `idle`, `small` or `giant` by its size (animals-needs 8.2).
            "objects" if ty.type_name == b"other" && ty.subtype.ends_with(b"poo") => {
                ["idle", "small", "giant"][meta.food.clamp(0, 2) as usize].into()
            }
            // A trash can shows `empty` up to 12 pieces and `half` from 13; `full` is never shown
            // (guest buildings 8). Its counter holds the pieces.
            "building" if ty.subtype == b"trshcan" => {
                ["empty", "half"][usize::from(meta.food >= 13)].into()
            }
            // Food shows `empty`, `half` or `full` by its units left (animals-needs 5.1).
            "food" => {
                let max = crate::build::number(ty, "cMaxFoodUnits").unwrap_or(0.0) as i32;
                crate::animal::food::STATES[crate::animal::food::fullness(meta.food, max)].into()
            }
            _ => String::new(),
        };
        // Without the swimming pictures an animal in a tank shows its rest picture.
        let tank = ty.class == "animals" && meta.mode == 3;
        let found = [wanted.as_str(), "idle"]
            .into_iter()
            .take(if tank || meta.shown.is_some() { 2 } else { 1 })
            .filter(|w| !w.is_empty())
            .find_map(|w| ty.animation_named(w));
        found.or_else(|| ty.base_animation()).and_then(|a| a.member)
    }

    /// The number of sprite files of the base animation set of a type: 0 when it has none or it
    /// does not load (ambients 3.2).
    pub(crate) fn set_files(&mut self, ty: &EntityType) -> usize {
        let member = ty.base_animation().and_then(|a| a.member);
        let anim = member.and_then(|m| self.animation(&m));
        anim.map_or(0, |a| a.set.files.len())
    }

    /// Places the face of a reaction above the unit whose anchor is `(x, y)` (animals 10, sprite
    /// drawing 9.3): `objects/smile` when it likes the change, `objects/frown` when not, up by the
    /// type's `cFacesYOffset` pixels of the view's zoom (not scaled by it), at the frame its age in
    /// milliseconds has reached.
    pub(super) fn face(
        &mut self,
        ty: &EntityType,
        (x, y): (i32, i32),
        likes: bool,
        age: u32,
        out: &mut Vec<Placed>,
    ) {
        let name = if likes { "smile" } else { "frown" };
        let Some(anim) = self.animation(&format!("objects/{name}/idle/idle.ani")) else {
            return;
        };
        let Some(sprite) = anim.sprites.first().map(|s| &s.0) else {
            return;
        };
        let last = sprite.frames.len().saturating_sub(1);
        let frame = (age.checked_div(sprite.delay_ms)).map_or(0, |f| (f as usize).min(last));
        let up = crate::build::number(ty, "cFacesYOffset").unwrap_or(0.0) as i32;
        let view = View {
            file: 0,
            mirror: false,
        };
        out.push(Placed {
            x,
            y,
            piece: None,
            grid: false,
            lift: up,
            pic: Pic::Anim(anim, view, None, frame),
        });
    }

    /// Places the selection marker after the picture of the entity that was placed last (sprite
    /// drawing 9.2): `selsmall` for a footprint of at most 1 x 1 half tiles, `selmed` for at most
    /// 4 in each direction, else `sellarge`; an egg or a crate has its own footprint. Its anchor
    /// is the entity's, up and left by a quarter of the x offset of the entity's frame, and by its
    /// y offset, 5 and half the height of the marker's own frame (the type's height plays no
    /// part). The frame is the file's background frame when it has one, else the shown frame. The marker has one frame, so it needs no clock.
    pub(super) fn marker(&mut self, ty: &EntityType, form: Form, out: &mut Vec<Placed>) {
        let Some(Placed {
            x,
            y,
            pic: Pic::Anim(anim, view, _, at),
            ..
        }) = out.last()
        else {
            return;
        };
        // The background frame of the file when it has one (9.2), else the frame shown.
        let Some(sprite) = anim.sprites.get(view.file).map(|s| &s.0) else {
            return;
        };
        let Some(frame) = sprite.extra.as_ref().or_else(|| sprite.frames.get(*at)) else {
            return;
        };
        let (x, y, xo, yo) = (*x, *y, i32::from(frame.x_offset), i32::from(frame.y_offset));
        let [fx, fy] = match form {
            Form::Egg => ty.form_footprint(true),
            Form::Crate => ty.form_footprint(false),
            Form::Plain => [ty.footprint[0], ty.footprint[1]],
        };
        let name = match fx.max(fy) {
            ..=1 => "selsmall",
            2..=4 => "selmed",
            _ => "sellarge",
        };
        let Some(anim) = self.animation(&format!("ui/select/{name}/{name}.ani")) else {
            return;
        };
        let view = View {
            file: 0,
            mirror: false,
        };
        // Up by the unit frame's y offset, 5 and half the marker's own frame height.
        let half = anim.sprites[0]
            .0
            .frames
            .first()
            .map_or(0, |f| i32::from(f.height) / 2);
        out.push(Placed {
            x: x - xo / 4,
            y: y - (yo + 5 + half),
            piece: None,
            grid: false,
            lift: 0,
            pic: Pic::Anim(anim, view, None, 0),
        });
    }

    /// Places the picture of one object.
    pub(super) fn object(
        &mut self,
        scene: &Scene,
        o: usize,
        meta: &Meta,
        summary: &mut Summary,
        out: &mut Vec<Placed>,
    ) {
        let obj = scene.objects[o];
        if !scene.is_placed(o) {
            return;
        }
        let ty = meta.ty;
        let anim = self
            .member(scene, o, meta)
            .and_then(|member| self.animation(&member));
        let Some(anim) = anim else {
            summary.objects_missing += 1;
            return;
        };
        // The pending piece of a path is an overlay that shows what a placed one would.
        let view = match obj.kind {
            Kind::Path | Kind::Overlay if ty.class == "paths" => View {
                file: path_view(scene, o, ty).min(anim.sprites.len().saturating_sub(1)),
                mirror: false,
            },
            _ => anim.view_of(obj.facing as u8, scene.rotation),
        };
        let (x, y) = project(scene.rotation, obj.x, obj.y, obj.z);
        summary.objects_drawn += 1;
        // A unit's frame, as far as the picture of its view has them.
        let last = anim
            .sprites
            .get(view.file)
            .map_or(0, |s| s.0.frames.len().saturating_sub(1));
        let mut frame = meta.shown.map_or(0, |(_, frame)| frame.min(last));
        // The bubbles go round their frames from the start of the cycle.
        if let (Some(age), Some(sprite)) = (meta.bubbles, anim.sprites.get(view.file)) {
            frame = bubble_frame(age, sprite.0.delay_ms, sprite.0.frames.len());
        }
        let palette = anim.sprites.get(view.file).and_then(|sprite| {
            let own = self.recoloured(ty, sprite, meta.colors);
            match meta.tint {
                Some(tint) => Some(own.unwrap_or_else(|| sprite.1.clone()).tinted(tint)),
                None => own,
            }
        });
        out.push(Placed {
            x,
            y,
            piece: None,
            grid: false,
            lift: 0,
            pic: Pic::Anim(anim, view, palette, frame),
        });
    }
}
