//! The pictures of tank exhibits (map drawing 9.7, sprite drawing 3.6, 7.5 to 7.8): wall stacks,
//! glass panes, water, underwater pictures and the shadows of units on the floor.

use super::order::{Glass, ground_height};
use super::*;

const HALF: Blend = Blend {
    strength: 0.5,
    black: false,
};

/// The water and glass sets of one tank (9.7.1).
pub(super) struct Look {
    water: String,
    glass: String,
    /// With scum: the noise thresholds for the surface pictures `tscum1` to `tscum3`, and the
    /// threshold for the faces. `None` for clean water.
    scum: Option<([u8; 3], u8)>,
}

impl Look {
    /// Whether the water is clean: no scum pictures.
    pub(super) fn is_clean(&self) -> bool {
        self.scum.is_none()
    }
}

/// Whether the type has the integer `key` set to 1 in its plain characteristics.
fn flag(ty: &EntityType, key: &str) -> bool {
    ty.config
        .as_deref()
        .and_then(|c| c.section("Characteristics/Integers"))
        .and_then(|s| s.get(key))
        .and_then(to_int)
        == Some(1)
}

/// Whether a unit shows a shadow on the floor: its type asks for one and its state does not opt
/// out. An animal outside the tank movement mode opts out.
pub(super) fn casts_shadow(meta: &Meta) -> bool {
    flag(meta.ty, "cUsesRealShadows") && !(meta.ty.class == "animals" && meta.mode != 3)
}

/// The same in the characteristics of the subtype, which is where the tank walls keep theirs.
fn subtype_flag(ty: &EntityType, key: &str) -> bool {
    ty.config
        .as_deref()
        .and_then(|c| c.section([&ty.subtype[..], b"/Characteristics/Integers"].concat()))
        .and_then(|s| s.get(key))
        .and_then(to_int)
        == Some(1)
}

/// The directory a member lives in: `animals/x/m/idle/idle.ani` gives `idle`.
fn directory(member: &str) -> &str {
    member.rsplit('/').nth(1).unwrap_or("")
}

/// The ends of a wall of a tank: `l` and `r` for the sides where the wall line turns or changes
/// type, `lr` for both, `m` for none (sprite drawing 3.6).
fn ends(scene: &Scene, metas: &[Meta], o: usize) -> &'static str {
    let obj = &scene.objects[o];
    let (Some(tank), Some(t)) = (scene.wall_tank(o), scene.tile_at(obj.x, obj.y)) else {
        return "m";
    };
    let f = obj.facing as usize % 8;
    let wall_on = |tile: Option<usize>, d: usize| {
        let w = scene.edge_object(tile?, d)?;
        (scene.wall_tank(w) == Some(tank)).then_some(w)
    };
    let end = |s: usize| {
        let diagonal = scene.neighbour(t, f).and_then(|a| scene.neighbour(a, s));
        wall_on(Some(t), s).is_some()
            || wall_on(diagonal, (s + 4) % 8).is_some()
            || wall_on(scene.neighbour(t, s), f)
                .is_some_and(|w| metas[w].ty.type_name != metas[o].ty.type_name)
    };
    match (end((f + 2) % 8), end((f + 6) % 8)) {
        (true, true) => "lr",
        (true, false) => "l",
        (false, true) => "r",
        _ => "m",
    }
}

impl MapRenderer<'_> {
    /// The water and glass sets of a tank of this salinity and purity.
    pub(super) fn look(&self, salt: bool, purity: u32) -> Look {
        let (murky, very_murky) = self.murky;
        let purity = i64::from(purity);
        let water = if salt { "salt" } else { "fresh" };
        let (suffix, glass, scum) = if purity >= i64::from(murky) {
            ("", "glass", None)
        } else if purity >= i64::from(very_murky) {
            ("ls", "glassls", Some(([220, 230, 240], 200)))
        } else if purity > 1 {
            ("hs", "glasshs", Some(([200, 220, 240], 180)))
        } else {
            ("es", "glasses", Some(([180, 210, 230], 150)))
        };
        Look {
            water: format!("{water}{suffix}"),
            glass: glass.to_string(),
            scum,
        }
    }

    /// Places frame 0 of a single-file animation of a water or glass set, blended at one half.
    fn water_picture(&mut self, set: &str, name: &str, at: (i32, i32), out: &mut Vec<Placed>) {
        let member = format!("water/{set}/{name}/{name}.ani");
        if let Some(anim) = self.animation(&member) {
            let view = View {
                file: 0,
                mirror: false,
            };
            out.push(Placed {
                x: at.0,
                y: at.1,
                piece: None,
                grid: false,
                pic: Pic::Blended(anim, view, HALF),
            });
        }
    }

    /// Places the pictures of one item of a tank exhibit.
    pub(super) fn tank_item(
        &mut self,
        scene: &Scene,
        metas: &[Meta],
        looks: &[Look],
        item: Item,
        summary: &mut Summary,
        out: &mut Vec<Placed>,
    ) {
        let r = scene.rotation;
        match item {
            Item::Edge(o) => self.wall(scene, metas, o, summary, out),
            Item::Pane { wall, k, glass } => {
                let Some(k_tank) = scene.wall_tank(wall) else {
                    return;
                };
                let front = matches!(glass, Glass::LowerRight | Glass::LowerLeft);
                if self.no_water(&metas[wall], front) {
                    return;
                }
                let name = match glass {
                    Glass::UpperLeft => "lgrey",
                    Glass::UpperRight => "bgrey",
                    Glass::LowerRight => "rgrey",
                    Glass::LowerLeft => "fgrey",
                };
                let obj = &scene.objects[wall];
                let at = project(r, obj.x, obj.y, obj.z + 16 * k);
                self.water_picture(&looks[k_tank].glass, name, at, out);
            }
            Item::Face {
                wall,
                k,
                right,
                noise,
            } => {
                let Some(k_tank) = scene.wall_tank(wall) else {
                    return;
                };
                if self.no_water(&metas[wall], true) {
                    return;
                }
                let look = &looks[k_tank];
                let side = if right { 'r' } else { 'f' };
                let name = match look.scum {
                    Some((_, limit)) if noise >= limit => format!("{side}scum{}", noise % 3 + 1),
                    _ if right => "right".to_string(),
                    _ => "front".to_string(),
                };
                let obj = &scene.objects[wall];
                let at = project(r, obj.x, obj.y, obj.z + 16 * k);
                self.water_picture(&look.water, &name, at, out);
            }
            Item::Surface { tile, z, noise } => {
                let Some(k) = scene.tank_of(tile) else { return };
                let look = &looks[k];
                let name = match look.scum {
                    Some(([a, b, c], _)) if noise >= a => match () {
                        _ if noise < b => "tscum1",
                        _ if noise < c => "tscum2",
                        _ => "tscum3",
                    },
                    _ => "top",
                };
                let (tx, ty) = ((tile % scene.width) as i32, (tile / scene.width) as i32);
                let at = project(r, 64 * tx + 32, 64 * ty + 32, z);
                self.water_picture(&look.water, name, at, out);
            }
            Item::Quarter { tile, z, quad } => {
                let Some(k) = scene.tank_of(tile) else { return };
                let name = ["topn", "tope", "topw", "tops"][usize::from(quad) % 4];
                let (tx, ty) = ((tile % scene.width) as i32, (tile / scene.width) as i32);
                let at = project(r, 64 * tx + 32, 64 * ty + 32, z);
                self.water_picture(&looks[k].water, name, at, out);
            }
            Item::Under(o) => self.under(scene, &metas[o], o, out),
            Item::Shadow(o) => self.shadow(scene, &metas[o], o, out),
            Item::Platform(tile) => {
                for d in [0, 2, 4, 6] {
                    let Some(w) = scene.edge_object(tile, d) else {
                        continue;
                    };
                    let (ty, Some(k)) = (metas[w].ty, scene.wall_tank(w)) else {
                        continue;
                    };
                    if ty.class != "tankwall" || ty.subtype.first() != Some(&b'g') {
                        continue;
                    }
                    let obj = &scene.objects[w];
                    let member = ty.animations().into_iter().find(|a| a.name == "platform");
                    if let Some(anim) = member
                        .and_then(|a| a.member)
                        .and_then(|m| self.animation(&m))
                    {
                        let (x, y) = project(r, obj.x, obj.y, obj.z + 16 * scene.tanks[k].height);
                        let view = anim.view_of(obj.facing as u8, r);
                        let palette = anim
                            .sprites
                            .get(view.file)
                            .and_then(|sprite| self.recoloured(ty, sprite, metas[w].colors));
                        out.push(Placed {
                            x,
                            y,
                            piece: None,
                            grid: false,
                            pic: Pic::Anim(anim, view, palette),
                        });
                    }
                }
            }
            Item::Terrain(_) | Item::Path(_) | Item::Object(_) => {}
        }
    }

    /// Whether a wall draws no glass and no water face: its type asks for none, unless a front
    /// wall is a portal.
    fn no_water(&self, meta: &Meta, front: bool) -> bool {
        subtype_flag(meta.ty, "cNoDrawWater") && !(front && meta.portal)
    }

    /// Places the stack of pieces of a tank wall, bottom to top (sprite drawing 3.6).
    fn wall(
        &mut self,
        scene: &Scene,
        metas: &[Meta],
        o: usize,
        summary: &mut Summary,
        out: &mut Vec<Placed>,
    ) {
        let Some(k) = scene.wall_tank(o) else {
            return;
        };
        let (obj, meta) = (scene.objects[o], &metas[o]);
        let height = scene.tanks[k].height;
        let e = ends(scene, metas, o);
        let mut pieces: Vec<(String, i32)> = Vec::new();
        if height == 1 {
            let name = if e == "lr" {
                "idle".to_string()
            } else {
                format!("tb{e}idle")
            };
            pieces.push((name, 0));
        } else {
            pieces.push((format!("b{e}idle"), 0));
            for step in 1..height - 1 {
                if !(meta.portal && step >= height - 4) {
                    pieces.push((format!("m{e}idle"), step));
                }
            }
            pieces.push((format!("t{e}idle"), height - 1));
        }
        if meta.portal {
            pieces.push(("portal".to_string(), height - 1));
        }
        if meta.ty.subtype.first() == Some(&b'g') {
            pieces.extend((0..=height).map(|step| ("ladder".to_string(), step)));
        }
        let animations = meta.ty.animations();
        let before = out.len();
        for (name, step) in pieces {
            let member = animations
                .iter()
                .find(|a| a.name == name)
                .and_then(|a| a.member.as_deref());
            if let Some(anim) = member.and_then(|m| self.animation(m)) {
                let (x, y) = project(scene.rotation, obj.x, obj.y, obj.z + 16 * step);
                let view = anim.view_of(obj.facing as u8, scene.rotation);
                let palette = anim
                    .sprites
                    .get(view.file)
                    .and_then(|sprite| self.recoloured(meta.ty, sprite, meta.colors));
                out.push(Placed {
                    x,
                    y,
                    piece: None,
                    grid: false,
                    pic: Pic::Anim(anim, view, palette),
                });
            }
        }
        match out.len() > before {
            true => summary.objects_drawn += 1,
            // No piece of the stack exists: the wall shows what a fence of its type shows.
            false => self.object(scene, o, meta, summary, out),
        }
    }

    /// The animation and view an unit or object shows now.
    fn current(&mut self, scene: &Scene, meta: &Meta, o: usize) -> Option<(Rc<Animation>, View)> {
        let anim = self
            .member(scene, o, meta)
            .and_then(|member| self.animation(&member))?;
        let view = anim.view_of(scene.objects[o].facing as u8, scene.rotation);
        Some((anim, view))
    }

    /// The underwater picture of an afloat object (sprite drawing 7.8).
    fn under(&mut self, scene: &Scene, meta: &Meta, o: usize, out: &mut Vec<Placed>) {
        let obj = scene.objects[o];
        if (obj.kind == Kind::Unit && meta.mode != 3) || !flag(meta.ty, "cHasUnderwaterSection") {
            return;
        }
        let Some((anim, view)) = self.current(scene, meta, o) else {
            return;
        };
        let dir = anim.set.files.get(view.file).map_or("", |f| directory(f));
        let name = format!("under{dir}");
        let member = meta.ty.animations().into_iter().find(|a| a.name == name);
        let Some(under) = member
            .and_then(|a| a.member)
            .and_then(|m| self.animation(&m))
        else {
            return;
        };
        if view.file >= under.sprites.len() {
            return;
        }
        let (x, y) = project(scene.rotation, obj.x, obj.y, obj.z);
        let palette = under
            .sprites
            .get(view.file)
            .and_then(|sprite| self.recoloured(meta.ty, sprite, meta.colors));
        out.push(Placed {
            x,
            y,
            piece: None,
            grid: false,
            pic: Pic::Anim(under, view, palette),
        });
    }

    /// The shadow of a unit on the floor of a tank (sprite drawing 7.6).
    fn shadow(&mut self, scene: &Scene, meta: &Meta, o: usize, out: &mut Vec<Placed>) {
        let obj = scene.objects[o];
        let ty = meta.ty;
        if !self.tank_shadows || !casts_shadow(meta) {
            return;
        }
        let Some(t) = scene.tile_at(obj.x, obj.y) else {
            return;
        };
        let Some((anim, view)) = self.current(scene, meta, o) else {
            return;
        };
        let (anim, black) = if flag(ty, "cHasShadowImages") {
            let member = self.member(scene, o, meta).unwrap_or_default();
            let name = format!("shadow{}", directory(&member));
            let found = ty.animations().into_iter().find(|a| a.name == name);
            let Some(shadow) = found
                .and_then(|a| a.member)
                .and_then(|m| self.animation(&m))
            else {
                return;
            };
            (shadow, flag(ty, "cForceShadowBlack"))
        } else {
            (anim, true)
        };
        let (tx, ty_) = ((t % scene.width) as i32, (t / scene.width) as i32);
        let floor = ground_height(&scene.tiles[t], obj.x - 64 * tx, obj.y - 64 * ty_);
        let (x, y) = project(scene.rotation, obj.x, obj.y, floor);
        let blend = Blend {
            strength: 0.25,
            black,
        };
        out.push(Placed {
            x,
            y,
            piece: None,
            grid: false,
            pic: Pic::Blended(anim, view, blend),
        });
    }
}
