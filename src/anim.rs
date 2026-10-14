//! Animation sets (`.ani` files) and the way one of their pictures is chosen and placed.
//!
//! An `.ani` file is written in the INI dialect of [`crate::config`]:
//!
//! ```text
//! [animation]
//! dir0 = objects       ; required, with dir1; dir2 and dir3 are optional
//! dir1 = aviary
//! dir2 = idle
//! animation = NE       ; repeated: one sprite file per entry, in the order of the entries
//! animation = SE
//! x0 = -204            ; the box of all frames around the anchor; integers, default 0
//! y0 = -220
//! x1 = 204
//! y1 = 103
//! ```
//!
//! The sprite file of an entry is `dir0/dir1[/dir2[/dir3]]/<entry>` (no extension); a missing
//! directory ends the path. A set fails to load only if one of its sprite files cannot be found
//! or read; a defective palette is a warning (see [`Animation::warning`]).
//!
//! Which file is drawn depends on the screen direction (see [`view_for`]), and a frame is placed
//! relative to an anchor point (see [`Animation::draw`]). The box is kept for interface layout; the
//! map never uses it.

use std::collections::HashMap;
use std::error::Error;
use std::fmt;
use std::sync::Arc;

use crate::Framebuffer;
use crate::config::{Config, to_int};
use crate::sprite::{self, Blend, Palette, Sprite};
use crate::vfs::{Vfs, normalise};
use crate::zoom::Zoom;

/// Why an animation set could not be read or loaded. The message is a single line, but it may
/// hold names as they are in the files.
#[derive(Debug, PartialEq, Eq)]
pub struct AnimError(String);

impl fmt::Display for AnimError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl Error for AnimError {}

fn error<T>(message: impl Into<String>) -> Result<T, AnimError> {
    Err(AnimError(message.into()))
}

/// What an `.ani` file says: the sprite files in order, and the box.
#[derive(Debug, PartialEq, Eq)]
pub struct AnimSet {
    /// The members of the sprite files, in the order of the file indexes.
    pub files: Vec<String>,
    /// The box around the anchor of all frames, unmirrored: left, top, right, bottom.
    pub x0: i32,
    pub y0: i32,
    pub x1: i32,
    pub y1: i32,
}

impl AnimSet {
    /// Reads the text of an `.ani` file. Fails if the file is too large for the dialect reader,
    /// if `dir0` or `dir1` is missing or empty, or if there is no `animation` entry.
    pub fn parse(bytes: &[u8]) -> Result<AnimSet, AnimError> {
        let config = Config::parse(bytes).map_err(|e| AnimError(e.to_string()))?;
        let section = config.section("animation");
        let text = |key: &str| {
            section
                .and_then(|section| section.get(key))
                .filter(|value| !value.is_empty())
                .map(|value| String::from_utf8_lossy(value).into_owned())
        };
        let number = |key: &str| {
            section
                .and_then(|section| section.get(key))
                .and_then(to_int)
                .unwrap_or(0)
        };

        let mut directory = Vec::new();
        for key in ["dir0", "dir1", "dir2", "dir3"] {
            match text(key) {
                Some(dir) => directory.push(dir),
                None if key == "dir0" || key == "dir1" => {
                    return error(format!("{key} is missing or empty"));
                }
                // A missing directory ends the path: a later one is ignored.
                None => break,
            }
        }
        let files: Vec<String> = section
            .map(|section| section.get_all("animation"))
            .unwrap_or_default()
            .iter()
            .map(|name| format!("{}/{}", directory.join("/"), String::from_utf8_lossy(name)))
            .collect();
        if files.is_empty() {
            return error("no animation entry");
        }
        Ok(AnimSet {
            files,
            x0: number("x0"),
            y0: number("y0"),
            x1: number("x1"),
            y1: number("y1"),
        })
    }

    /// Reads and parses the `.ani` member `member`.
    pub fn load(vfs: &Vfs, member: &str) -> Result<AnimSet, AnimError> {
        let bytes = vfs.read(member).map_err(|e| AnimError(e.to_string()))?;
        AnimSet::parse(&bytes).map_err(|e| AnimError(format!("{member}: {e}")))
    }
}

/// The member of the `.ani` file for the animation value `value`, found in the animation section
/// of an entity type whose animation root is `root` (for example `animals/lion/f`): a value that
/// contains the text `.ani` is the whole path, any other `v` is `root/v/v.ani`.
pub fn resolve_path(root: &str, value: &str) -> String {
    match value.contains(".ani") {
        true => value.to_string(),
        false => format!("{}/{value}/{value}.ani", root.trim_end_matches('/')),
    }
}

/// The screen direction `0..8` (N, NE, E, SE, S, SW, W, NW) of an entity with `facing` (`0..8`,
/// numbered like the map directions) when the map is turned by `rotation` (`0..4`).
pub fn screen_direction(facing: u8, rotation: u8) -> u8 {
    (i32::from(facing) - 1 + 2 * i32::from(rotation)).rem_euclid(8) as u8
}

/// Which sprite file of a set is drawn, and whether it is reflected.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct View {
    /// The index of the sprite file in the set.
    pub file: usize,
    /// Reflect the pictures left to right about the anchor column.
    pub mirror: bool,
}

/// The view of a set with `file_count` sprite files for the screen direction `direction` (`0..8`).
///
/// * Four files (`NE SE SW NW`): entry `direction / 2`, never mirrored.
/// * Otherwise the entries are `N NE E SE S`: directions 0 to 4 use their own entry, directions
///   5, 6 and 7 the entries 3, 2 and 1, mirrored.
///
/// An index that is 0 or at least `file_count` becomes 0, but the mirror flag stays: a set with
/// a single file is mirrored for the directions 5 to 7.
pub fn view_for(file_count: usize, direction: u8) -> View {
    let direction = usize::from(direction % 8);
    let (index, mirror) = match file_count {
        4 => (direction / 2, false),
        _ if direction < 5 => (direction, false),
        _ => (8 - direction, true),
    };
    View {
        file: if index < file_count { index } else { 0 },
        mirror,
    }
}

/// A loaded set: the description and, for each sprite file, its sprite and its palette.
#[derive(Debug)]
pub struct Animation {
    pub set: AnimSet,
    /// One entry per sprite file of the set, in order. Entries that name the same member share
    /// one copy, so the memory is that of the distinct files.
    pub sprites: Vec<Arc<(Sprite, Palette)>>,
    /// The first defect of a palette among the sprite files, with the member it belongs to: the
    /// palette is missing, or lacks a colour the sprite draws. The set still loads and draws as
    /// the original does, see [`crate::sprite::Frame::draw`].
    pub warning: Option<String>,
}

impl Animation {
    /// Loads every sprite file of the set, each distinct member once. One that cannot be found
    /// or decoded makes the whole set fail, and the error names the first. The second value of a
    /// failure is the number of distinct sprite files that do not exist.
    pub fn open(vfs: &Vfs, set: AnimSet) -> Result<Animation, (AnimError, usize)> {
        let mut sprites = Vec::with_capacity(set.files.len());
        let (mut first, mut missing, mut warning) = (None, 0, None);
        let mut loaded: HashMap<String, Option<Arc<(Sprite, Palette)>>> = HashMap::new();
        for member in &set.files {
            let shared = loaded.entry(normalise(member)).or_insert_with(|| {
                match sprite::load(vfs, member) {
                    Ok((sprite, palette, defect)) => {
                        if let Some(defect) = defect {
                            warning.get_or_insert_with(|| format!("{member}: {defect}"));
                        }
                        Some(Arc::new((sprite, palette)))
                    }
                    Err(err) => {
                        missing += usize::from(vfs.read(member).is_err());
                        first.get_or_insert_with(|| AnimError(format!("{member}: {err}")));
                        None
                    }
                }
            });
            sprites.extend(shared.clone());
        }
        match first {
            Some(err) => Err((err, missing)),
            None => Ok(Animation {
                set,
                sprites,
                warning,
            }),
        }
    }

    /// The view for a screen direction (see [`view_for`]).
    pub fn view(&self, direction: u8) -> View {
        view_for(self.sprites.len(), direction)
    }

    /// The view for an entity with `facing` on a map turned by `rotation`.
    pub fn view_of(&self, facing: u8, rotation: u8) -> View {
        self.view(screen_direction(facing, rotation))
    }

    /// Draws frame `frame` of the sprite file of `view`, with the anchor at (`x`, `y`) of the
    /// framebuffer. The background (extra) frame of the file, if any, is drawn first, with the
    /// same anchor and its own offsets; the same holds for the mirror flag. Unmirrored, the top
    /// left corner of a frame is the anchor minus its offsets; mirrored, pixel (`c`, `r`) goes
    /// to (`x + x_offset - c`, `y - y_offset + r`).
    ///
    /// Draws nothing if the file index or the frame number is out of range.
    pub fn draw(&self, view: View, frame: usize, target: &mut Framebuffer, x: i32, y: i32) {
        self.draw_inner(view, frame, target, (x, y), None, None, Zoom::Normal);
    }

    /// As [`Animation::draw`], with `palette` in place of the palette of the sprite file.
    pub fn draw_with(
        &self,
        view: View,
        frame: usize,
        target: &mut Framebuffer,
        x: i32,
        y: i32,
        palette: Option<&Palette>,
    ) {
        self.draw_inner(view, frame, target, (x, y), palette, None, Zoom::Normal);
    }

    /// As [`Animation::draw`], blended with what is behind it.
    pub fn draw_blended(
        &self,
        view: View,
        frame: usize,
        target: &mut Framebuffer,
        at: (i32, i32),
        blend: Blend,
    ) {
        self.draw_inner(view, frame, target, at, None, Some(blend), Zoom::Normal);
    }

    /// As [`Animation::draw_with`] or [`Animation::draw_blended`] at a zoom, frame 0, `at` being
    /// the anchor already at the zoom ([`crate::sprite::Frame::draw_zoomed`]).
    pub fn draw_zoomed(
        &self,
        view: View,
        target: &mut Framebuffer,
        at: (i32, i32),
        palette: Option<&Palette>,
        blend: Option<Blend>,
        zoom: Zoom,
    ) {
        self.draw_inner(view, 0, target, at, palette, blend, zoom);
    }

    #[allow(clippy::too_many_arguments)]
    fn draw_inner(
        &self,
        view: View,
        frame: usize,
        target: &mut Framebuffer,
        at: (i32, i32),
        palette: Option<&Palette>,
        blend: Option<Blend>,
        zoom: Zoom,
    ) {
        let Some((sprite, own)) = self.sprites.get(view.file).map(|s| &**s) else {
            return;
        };
        let palette = palette.unwrap_or(own);
        let Some(frame) = sprite.frames.get(frame) else {
            return;
        };
        for frame in sprite.extra.iter().chain([frame]) {
            frame.draw_zoomed(palette, target, at, view.mirror, blend, zoom);
        }
    }

    /// The box of all frames of all files, background frames included, unmirrored and relative
    /// to the anchor, as an `.ani` file writes it: left, top, right, bottom. `None` if there is
    /// no frame. The box of the file should be this.
    pub fn extents(&self) -> Option<[i32; 4]> {
        self.sprites
            .iter()
            .flat_map(|loaded| loaded.0.all_frames())
            .map(|frame| {
                let (x, y) = (-i32::from(frame.x_offset), -i32::from(frame.y_offset));
                [
                    x,
                    y,
                    x + i32::from(frame.width),
                    y + i32::from(frame.height),
                ]
            })
            .reduce(|a, b| {
                [
                    a[0].min(b[0]),
                    a[1].min(b[1]),
                    a[2].max(b[2]),
                    a[3].max(b[3]),
                ]
            })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sprite::tests::{Install, sized, sprite_file};

    /// The frame data of a `width` by `height` frame with the given offsets and rows.
    fn frame(width: u16, height: u16, x_offset: i16, y_offset: i16, rows: &[u8]) -> Vec<u8> {
        let mut data = Vec::new();
        for field in [height, width, y_offset as u16, x_offset as u16, 0] {
            data.extend(field.to_le_bytes());
        }
        data.extend(rows);
        data
    }

    /// A palette file: entry 0 is the marker, then red, green and blue with a zero alpha byte.
    fn palette_file() -> Vec<u8> {
        let mut out = 4u32.to_le_bytes().to_vec();
        out.extend([255, 0, 255, 0, 255, 0, 0, 0, 0, 255, 0, 0, 0, 0, 255, 0]);
        out
    }

    /// A sprite of one one-row frame per entry of `frames`, with the palette of [`palette_file`].
    fn sprite_of(frames: &[Vec<u8>], extra: Option<Vec<u8>>) -> Sprite {
        let flag = extra.is_some().then_some(1);
        let mut all: Vec<Vec<u8>> = frames.iter().map(|f| sized(f)).collect();
        all.extend(extra.map(|f| sized(&f)));
        Sprite::parse(&sprite_file(flag, frames.len() as u32, &all)).unwrap()
    }

    fn animation(sprites: Vec<Sprite>) -> Animation {
        let palette = || Palette::parse(&palette_file()).unwrap();
        Animation {
            set: AnimSet {
                files: Vec::new(),
                x0: 0,
                y0: 0,
                x1: 0,
                y1: 0,
            },
            sprites: sprites
                .into_iter()
                .map(|s| Arc::new((s, palette())))
                .collect(),
            warning: None,
        }
    }

    fn pixel(image: &Framebuffer, x: usize, y: usize) -> [u8; 4] {
        let at = (y * image.width as usize + x) * 4;
        image.pixels[at..at + 4].try_into().unwrap()
    }

    const RED: [u8; 4] = [255, 0, 0, 255];
    const GREEN: [u8; 4] = [0, 255, 0, 255];
    const BLUE: [u8; 4] = [0, 0, 255, 255];
    const NONE: [u8; 4] = [0; 4];

    fn parse_message(text: &str) -> String {
        AnimSet::parse(text.as_bytes()).unwrap_err().to_string()
    }

    #[test]
    fn an_ani_file_gives_the_files_in_order_and_the_box() {
        let text = "[animation]\r\ndir0 = Objects\r\ndir1 = aviary\r\ndir2 = idle\r\n\
                    animation = NE\r\nanimation = SE\r\nx0 = -204\r\ny0 = -220\r\nx1 = 0x10\r\n\
                    y1 = 103x\r\n";
        let set = AnimSet::parse(text.as_bytes()).unwrap();
        assert_eq!(
            set.files,
            ["Objects/aviary/idle/NE", "Objects/aviary/idle/SE"]
        );
        assert_eq!((set.x0, set.y0, set.x1, set.y1), (-204, -220, 16, 103));
    }

    #[test]
    fn directories_end_at_the_first_missing_one() {
        let parse = |dirs: &str| {
            AnimSet::parse(format!("[animation]\n{dirs}animation = N\n").as_bytes()).unwrap()
        };
        assert_eq!(parse("dir0 = a\ndir1 = b\n").files, ["a/b/N"]);
        assert_eq!(parse("dir0 = a\ndir1 = b\ndir2 = c\n").files, ["a/b/c/N"]);
        assert_eq!(
            parse("dir0 = a\ndir1 = b\ndir2 = c\ndir3 = d\n").files,
            ["a/b/c/d/N"]
        );
        // dir3 without dir2, and dir2 with an empty value, end the path early.
        assert_eq!(parse("dir0 = a\ndir1 = b\ndir3 = d\n").files, ["a/b/N"]);
        assert_eq!(
            parse("dir0 = a\ndir1 = b\ndir2 =\ndir3 = d\n").files,
            ["a/b/N"]
        );
        let set = parse("dir0 = a\ndir1 = b\n");
        assert_eq!(
            (set.x0, set.y0, set.x1, set.y1),
            (0, 0, 0, 0),
            "box defaults"
        );
    }

    #[test]
    fn an_invalid_ani_file_is_an_error() {
        assert_eq!(parse_message(""), "dir0 is missing or empty");
        assert_eq!(
            parse_message("[animation]\ndir1 = b\nanimation = N\n"),
            "dir0 is missing or empty"
        );
        assert_eq!(
            parse_message("[animation]\ndir0 = a\ndir1 =\nanimation = N\n"),
            "dir1 is missing or empty"
        );
        assert_eq!(
            parse_message("[animation]\ndir0 = a\ndir1 = b\n"),
            "no animation entry"
        );
        assert_eq!(
            parse_message("[other]\ndir0 = a\ndir1 = b\nanimation = N\n"),
            "dir0 is missing or empty"
        );
        let huge = vec![b'x'; crate::config::MAX_INPUT_LEN + 1];
        assert!(AnimSet::parse(&huge).is_err());
    }

    #[test]
    fn a_name_becomes_the_path_of_its_ani_file() {
        assert_eq!(
            resolve_path("animals/lion/f", "walk"),
            "animals/lion/f/walk/walk.ani"
        );
        assert_eq!(
            resolve_path("animals/lion/f/", "walk"),
            "animals/lion/f/walk/walk.ani"
        );
        assert_eq!(
            resolve_path("animals/lion/f", "objects/dustcld/idle/idle.ani"),
            "objects/dustcld/idle/idle.ani"
        );
    }

    #[test]
    fn screen_direction_follows_facing_and_rotation() {
        // The table of the specification, one row per facing, one column per rotation.
        let table = [
            [7, 1, 3, 5],
            [0, 2, 4, 6],
            [1, 3, 5, 7],
            [2, 4, 6, 0],
            [3, 5, 7, 1],
            [4, 6, 0, 2],
            [5, 7, 1, 3],
            [6, 0, 2, 4],
        ];
        for (facing, row) in table.iter().enumerate() {
            for (rotation, &expected) in row.iter().enumerate() {
                assert_eq!(
                    screen_direction(facing as u8, rotation as u8),
                    expected,
                    "facing {facing} rotation {rotation}"
                );
            }
        }
        assert!(screen_direction(255, 255) < 8);
    }

    #[test]
    fn five_file_sets_mirror_the_western_views() {
        // N NE E SE S, then SW from SE, W from E, NW from NE.
        let views: Vec<_> = (0..8).map(|u| view_for(5, u)).collect();
        let expected = [
            (0, false),
            (1, false),
            (2, false),
            (3, false),
            (4, false),
            (3, true),
            (2, true),
            (1, true),
        ];
        for (u, (view, (file, mirror))) in views.iter().zip(expected).enumerate() {
            assert_eq!((view.file, view.mirror), (file, mirror), "direction {u}");
        }
    }

    #[test]
    fn four_file_sets_are_never_mirrored() {
        // NE SE SW NW: directions 0 and 1 use entry 0, 2 and 3 entry 1, and so on.
        for u in 0..8 {
            assert_eq!(
                view_for(4, u),
                View {
                    file: usize::from(u / 2),
                    mirror: false
                }
            );
        }
        // The table of the specification for objects with even facings, as entry names.
        let names = ["NE", "SE", "SW", "NW"];
        for (facing, row) in [
            (0, ["NW", "NE", "SE", "SW"]),
            (2, ["NE", "SE", "SW", "NW"]),
            (4, ["SE", "SW", "NW", "NE"]),
            (6, ["SW", "NW", "NE", "SE"]),
        ] {
            for (rotation, name) in row.iter().enumerate() {
                let u = screen_direction(facing, rotation as u8);
                assert_eq!(names[view_for(4, u).file], *name, "{facing} {rotation}");
            }
        }
    }

    #[test]
    fn a_missing_file_index_becomes_zero_but_the_mirror_stays() {
        // One file: unmirrored for 0 to 4, mirrored for 5 to 7.
        for u in 0..8 {
            assert_eq!(
                view_for(1, u),
                View {
                    file: 0,
                    mirror: u >= 5
                }
            );
        }
        // Two and three files: an index beyond the set (or 0) is 0.
        assert_eq!(view_for(2, 1).file, 1);
        assert_eq!(view_for(2, 2).file, 0);
        assert_eq!(view_for(3, 4).file, 0);
        assert_eq!(
            view_for(3, 6),
            View {
                file: 2,
                mirror: true
            }
        );
        // Eight files: only the first five are used, the rest would be mirrored copies.
        assert_eq!(
            view_for(8, 5),
            View {
                file: 3,
                mirror: true
            }
        );
        assert_eq!(
            view_for(20, 4),
            View {
                file: 4,
                mirror: false
            }
        );
        // No file at all: index 0, which draws nothing.
        assert_eq!(view_for(0, 3).file, 0);
    }

    /// A one-row frame: `pixels` are palette values from the left edge of the frame.
    fn row_frame(x_offset: i16, y_offset: i16, pixels: &[u8]) -> Vec<u8> {
        let mut rows = vec![1, 0, pixels.len() as u8];
        rows.extend(pixels);
        frame(pixels.len() as u16, 1, x_offset, y_offset, &rows)
    }

    #[test]
    fn placement_subtracts_the_offsets_and_mirroring_reflects_about_the_anchor() {
        // Three pixels red, green, blue; the anchor is the column of the green one, one row down.
        let anim = animation(vec![sprite_of(&[row_frame(1, 1, &[1, 2, 3])], None)]);
        let plain = View {
            file: 0,
            mirror: false,
        };
        let mut image = Framebuffer::new(7, 3);
        anim.draw(plain, 0, &mut image, 3, 1);
        assert_eq!(pixel(&image, 2, 0), RED);
        assert_eq!(pixel(&image, 3, 0), GREEN);
        assert_eq!(pixel(&image, 4, 0), BLUE);

        // Mirrored: the picture is reflected about the anchor column, the green one stays.
        let mirrored = View {
            file: 0,
            mirror: true,
        };
        let mut image = Framebuffer::new(7, 3);
        anim.draw(mirrored, 0, &mut image, 3, 1);
        assert_eq!(pixel(&image, 4, 0), RED);
        assert_eq!(pixel(&image, 3, 0), GREEN);
        assert_eq!(pixel(&image, 2, 0), BLUE);
        assert_eq!(image.pixels.iter().filter(|&&b| b != 0).count(), 3 * 2);

        // An offset of 0 puts the first pixel on the anchor; mirrored, on the anchor and leftwards.
        let anim = animation(vec![sprite_of(&[row_frame(0, 0, &[1, 2, 3])], None)]);
        let mut image = Framebuffer::new(7, 1);
        anim.draw(plain, 0, &mut image, 3, 0);
        assert_eq!([pixel(&image, 3, 0), pixel(&image, 5, 0)], [RED, BLUE]);
        let mut image = Framebuffer::new(7, 1);
        anim.draw(mirrored, 0, &mut image, 3, 0);
        assert_eq!([pixel(&image, 3, 0), pixel(&image, 1, 0)], [RED, BLUE]);

        // A negative offset moves the frame to the right of the anchor, and mirrored to the left.
        let anim = animation(vec![sprite_of(&[row_frame(-2, 0, &[1])], None)]);
        let mut image = Framebuffer::new(7, 1);
        anim.draw(plain, 0, &mut image, 3, 0);
        assert_eq!(pixel(&image, 5, 0), RED);
        let mut image = Framebuffer::new(7, 1);
        anim.draw(mirrored, 0, &mut image, 3, 0);
        assert_eq!(pixel(&image, 1, 0), RED);
    }

    #[test]
    fn each_frame_has_its_own_offsets() {
        let anim = animation(vec![sprite_of(
            &[row_frame(0, 0, &[1]), row_frame(1, 0, &[1])],
            None,
        )]);
        let view = View {
            file: 0,
            mirror: false,
        };
        for (frame, column) in [(0, 2), (1, 1)] {
            let mut image = Framebuffer::new(4, 1);
            anim.draw(view, frame, &mut image, 2, 0);
            assert_eq!(pixel(&image, column, 0), RED, "frame {frame}");
        }
    }

    #[test]
    fn the_background_frame_is_drawn_first_with_its_own_offsets_and_mirror() {
        // Background: green, blue, blue at offset 1 (so columns -1..2 around the anchor).
        // Animated frame: red, offset 0: on the anchor itself, over the background.
        let sprite = sprite_of(&[row_frame(0, 0, &[1])], Some(row_frame(1, 0, &[2, 3, 3])));
        let anim = animation(vec![sprite]);
        let plain = View {
            file: 0,
            mirror: false,
        };
        let mut image = Framebuffer::new(5, 1);
        anim.draw(plain, 0, &mut image, 2, 0);
        assert_eq!(
            [0, 1, 2, 3, 4].map(|x| pixel(&image, x, 0)),
            [NONE, GREEN, RED, BLUE, NONE],
            "the animated frame covers the background at the anchor"
        );
        // Mirrored: background columns 1 (blue), 2 (anchor) ... reflect: the offset 1 gives
        // x + 1 - c, so green at 3, blue at 2 and 1; the red frame at 2 on top.
        let mirrored = View {
            file: 0,
            mirror: true,
        };
        let mut image = Framebuffer::new(5, 1);
        anim.draw(mirrored, 0, &mut image, 2, 0);
        assert_eq!(
            [0, 1, 2, 3, 4].map(|x| pixel(&image, x, 0)),
            [NONE, BLUE, RED, GREEN, NONE]
        );
        // The background is not an animated frame: asking for frame 1 of a one-frame file
        // draws nothing at all, not even the background.
        let mut image = Framebuffer::new(5, 1);
        anim.draw(plain, 1, &mut image, 2, 0);
        assert_eq!(image.pixels, [0; 20]);
    }

    #[test]
    fn a_transparent_gap_in_a_frame_shows_the_background() {
        // Animated frame: value 0 (transparent under the marker), then red.
        let sprite = sprite_of(&[row_frame(0, 0, &[0, 1])], Some(row_frame(0, 0, &[3, 3])));
        let anim = animation(vec![sprite]);
        let mut image = Framebuffer::new(2, 1);
        anim.draw(
            View {
                file: 0,
                mirror: false,
            },
            0,
            &mut image,
            0,
            0,
        );
        assert_eq!([pixel(&image, 0, 0), pixel(&image, 1, 0)], [BLUE, RED]);
    }

    #[test]
    fn a_missing_file_or_frame_draws_nothing() {
        let anim = animation(vec![sprite_of(&[row_frame(0, 0, &[1])], None)]);
        let mut image = Framebuffer::new(2, 1);
        for (file, frame) in [(1, 0), (0, 1), (usize::MAX, 0), (0, usize::MAX)] {
            let view = View {
                file,
                mirror: false,
            };
            anim.draw(view, frame, &mut image, 0, 0);
            assert_eq!(image.pixels, [0; 8], "file {file} frame {frame}");
        }
        // A file without frames draws nothing either, even with a background frame.
        let anim = animation(vec![sprite_of(&[], Some(row_frame(0, 0, &[1])))]);
        anim.draw(
            View {
                file: 0,
                mirror: false,
            },
            0,
            &mut image,
            0,
            0,
        );
        assert_eq!(image.pixels, [0; 8]);
        // And no file at all.
        let anim = animation(Vec::new());
        anim.draw(anim.view(3), 0, &mut image, 0, 0);
        assert_eq!(image.pixels, [0; 8]);
    }

    #[test]
    fn a_shadow_frame_darkens_what_the_animal_stands_on() {
        // A shadow file: one frame of two covered pixels at offset 0 (bit 15 of the height).
        let mut data = frame(2, 1, 0, 0, &[1, 0, 2]);
        data[1] |= 0x80;
        let shadow = Sprite::parse(&sprite_file(None, 1, &[sized(&data)])).unwrap();
        let anim = animation(vec![shadow]);
        let mut image = Framebuffer::new(3, 1);
        image.pixels.fill(200);
        anim.draw(
            View {
                file: 0,
                mirror: false,
            },
            0,
            &mut image,
            1,
            0,
        );
        assert_eq!(pixel(&image, 0, 0), [200; 4]);
        assert_eq!(pixel(&image, 1, 0), [150, 150, 150, 200]);
        assert_eq!(pixel(&image, 2, 0), [150, 150, 150, 200]);
    }

    #[test]
    fn the_extents_cover_all_frames_and_background_frames() {
        let one = sprite_of(&[row_frame(2, 1, &[1, 1, 1]), row_frame(0, 3, &[1])], None);
        let two = sprite_of(&[row_frame(0, 0, &[1])], Some(row_frame(5, 0, &[1; 9])));
        // Frame 0: x -2..1, y -1..0; frame 1: x 0..1, y -3..-2; background: x -5..4, y 0..1.
        assert_eq!(animation(vec![one, two]).extents(), Some([-5, -3, 4, 1]));
        assert_eq!(animation(Vec::new()).extents(), None);
    }

    #[test]
    fn opening_a_set_loads_its_files_and_counts_the_missing_ones() {
        let ani = |entries: &str| {
            format!("[animation]\ndir0 = a\ndir1 = b\n{entries}x1 = 3\n").into_bytes()
        };
        let sprite = |frames: &[Vec<u8>]| sprite_file(None, frames.len() as u32, frames);
        let one = sized(&row_frame(0, 0, &[1, 2, 3]));
        let sprite_file_naming_palette = |path: &str, frame: &Vec<u8>| {
            let mut out = 125u32.to_le_bytes().to_vec();
            out.extend((path.len() as u32).to_le_bytes());
            out.extend(path.as_bytes());
            out.extend(1u32.to_le_bytes());
            out.extend(frame);
            out
        };
        let install = Install::new(
            "anim-open",
            &[
                ("animals/Lion/lion.pal", palette_file()),
                ("a/b/N", sprite(std::slice::from_ref(&one))),
                ("a/b/NE", sprite(&[one.clone(), one.clone()])),
                ("two.ani", ani("animation = N\nanimation = NE\n")),
                (
                    "gone.ani",
                    ani("animation = N\nanimation = X\nanimation = Y\n"),
                ),
                ("bad.ani", b"[animation]\ndir0 = a\n".to_vec()),
                ("a/b/short", sprite(&[sized(&row_frame(0, 0, &[9]))])),
                ("nopal.ani", ani("animation = short\n")),
                ("a/b/nopal", sprite_file_naming_palette("gone.pal", &one)),
                ("gonepal.ani", ani("animation = nopal\n")),
                (
                    "repeat.ani",
                    ani(&format!(
                        "{}animation = NE\nanimation = n\n",
                        "animation = N\n".repeat(3000)
                    )),
                ),
            ],
        );
        let vfs = Vfs::open(&install.0).unwrap();

        let set = AnimSet::load(&vfs, "TWO.ani").unwrap();
        assert_eq!(set.x1, 3);
        let anim = Animation::open(&vfs, set).unwrap();
        assert_eq!(anim.sprites.len(), 2);
        assert_eq!(anim.sprites[1].0.frames.len(), 2);
        assert_eq!(anim.warning, None);
        assert_eq!(
            anim.view_of(2, 0),
            View {
                file: 1,
                mirror: false
            }
        );

        // A missing sprite file fails the whole set; the first is named, all are counted.
        let set = AnimSet::load(&vfs, "gone.ani").unwrap();
        let (err, missing) = Animation::open(&vfs, set).unwrap_err();
        assert_eq!(missing, 2);
        assert!(
            err.to_string().starts_with("a/b/X: no such member"),
            "{err}"
        );

        // A palette index beyond the palette is a warning: the set loads.
        let set = AnimSet::load(&vfs, "nopal.ani").unwrap();
        let anim = Animation::open(&vfs, set).unwrap();
        assert_eq!(
            anim.warning.as_deref(),
            Some("a/b/short: palette index 9 but the palette has 4 colours")
        );
        // So is a palette file that does not exist, and the set draws nothing for it.
        let set = AnimSet::load(&vfs, "gonepal.ani").unwrap();
        let anim = Animation::open(&vfs, set).unwrap();
        let warning = anim.warning.as_deref().unwrap();
        assert!(
            warning.starts_with("a/b/nopal: palette \"gone.pal\""),
            "{warning}"
        );
        assert!(anim.sprites[0].1.is_empty());

        // Entries that name the same member, in any letter case, share one decoded copy.
        let set = AnimSet::load(&vfs, "repeat.ani").unwrap();
        let anim = Animation::open(&vfs, set).unwrap();
        assert_eq!(anim.sprites.len(), 3002);
        assert!(
            anim.sprites[..3000]
                .iter()
                .all(|s| Arc::ptr_eq(s, &anim.sprites[0]))
        );
        assert!(!Arc::ptr_eq(&anim.sprites[3000], &anim.sprites[0]));
        assert!(Arc::ptr_eq(&anim.sprites[3001], &anim.sprites[0]));

        let message = AnimSet::load(&vfs, "bad.ani").unwrap_err().to_string();
        assert_eq!(message, "bad.ani: dir1 is missing or empty");
        let message = AnimSet::load(&vfs, "nothing.ani").unwrap_err().to_string();
        assert!(message.contains("no such member: nothing.ani"), "{message}");
    }

    /// The facts below were measured on Zoo Tycoon: Complete Collection.
    #[test]
    #[ignore = "needs the game: set ZT_DATA_DIR to the install folder"]
    fn full_game_facts() {
        let dir = std::env::var_os("ZT_DATA_DIR").expect("ZT_DATA_DIR is not set");
        let vfs = Vfs::open(std::path::Path::new(&dir)).unwrap();
        let open = |member: &str| {
            let set = AnimSet::load(&vfs, member).unwrap();
            Animation::open(&vfs, set).unwrap()
        };

        let walk = open("animals/lion/f/walk/walk.ani");
        assert_eq!(walk.sprites.len(), 5);
        assert_eq!(walk.extents(), Some([-26, -28, 22, 13]));
        assert_eq!(
            (walk.set.x0, walk.set.y0, walk.set.x1, walk.set.y1),
            (-26, -28, 22, 13)
        );
        // Facing 3 with rotation 2 is the screen direction 6: the east file, mirrored.
        assert_eq!(
            walk.view_of(3, 2),
            View {
                file: 2,
                mirror: true
            }
        );

        let aviary = open("objects/aviary/idle/idle.ani");
        assert_eq!(aviary.sprites.len(), 4);
        assert_eq!(aviary.extents(), Some([-204, -220, 204, 103]));
        assert_eq!(
            aviary.view_of(2, 0),
            View {
                file: 0,
                mirror: false
            }
        );
    }
}
