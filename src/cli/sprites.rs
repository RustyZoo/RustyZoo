//! The commands that dump, draw and check sprites and animation sets.

use std::fs::File;
use std::io;
use std::path::Path;

use super::{CliError, Report};
use rustyzoo::Framebuffer;
use rustyzoo::anim::{self, AnimSet, Animation};
use rustyzoo::sprite::{self, write_png};
use rustyzoo::vfs::Vfs;

fn frame_file_name(number: usize, shadow: bool, extra: bool) -> String {
    let shadow = if shadow { "-shadow" } else { "" };
    let extra = if extra { "-extra" } else { "" };
    format!("{number:03}{shadow}{extra}.png")
}

pub fn dump_sprite(vfs: &Vfs, member: &str, dir: &Path) -> Result<(), CliError> {
    let (sprite, palette, warning) = sprite::load(vfs, member)?;
    std::fs::create_dir_all(dir).map_err(|e| format!("cannot create {}: {e}", dir.display()))?;
    let mut report = Report::new();
    report.line(format_args!(
        "{member}: {} frames{}, delay {} ms, palette {:?}",
        sprite.frames.len(),
        if sprite.extra.is_some() {
            " + 1 extra"
        } else {
            ""
        },
        sprite.delay_ms,
        sprite.palette,
    ));
    if let Some(warning) = warning {
        report.line(format_args!("warning: {}", warning.escape_debug()));
    }
    for (number, frame) in sprite.all_frames().enumerate() {
        let name = frame_file_name(number, frame.shadow, number == sprite.frames.len());
        let empty = frame.width == 0 || frame.height == 0;
        report.line(format_args!(
            "  {name}: {}x{} x offset {} y offset {} unknown {}{}",
            frame.width,
            frame.height,
            frame.x_offset,
            frame.y_offset,
            frame.unknown,
            if empty { " (empty, no file)" } else { "" },
        ));
        if empty {
            continue;
        }
        let mut image = Framebuffer::new(frame.width.into(), frame.height.into());
        if frame.shadow {
            // A shadow frame only darkens what is behind it: show it on white.
            image.pixels.fill(255);
        }
        frame.draw(&palette, &mut image, 0, 0);
        let path = dir.join(&name);
        let file =
            File::create(&path).map_err(|e| format!("cannot create {}: {e}", path.display()))?;
        write_png(&image, io::BufWriter::new(file))
            .map_err(|e| format!("cannot write {}: {e}", path.display()))?;
    }
    report.finish()
}

/// Decodes every sprite file (a member whose last path component has no extension).
pub fn check_sprites(vfs: &Vfs) -> Result<(), CliError> {
    let mut report = Report::new();
    let (mut files, mut frames, mut shadows, mut extras, mut failed, mut warned) =
        (0, 0, 0, 0, 0, 0);
    for member in vfs.list(None) {
        if !sprite::is_sprite_member(&member) {
            continue;
        }
        files += 1;
        match sprite::load(vfs, &member) {
            Ok((sprite, _, warning)) => {
                if let Some(warning) = warning {
                    warned += 1;
                    report.line(format_args!(
                        "warning {}: {}",
                        member.escape_debug(),
                        warning.escape_debug()
                    ));
                }
                frames += sprite.frames.len();
                shadows += sprite.all_frames().filter(|frame| frame.shadow).count();
                extras += usize::from(sprite.extra.is_some());
            }
            Err(err) => {
                failed += 1;
                // Paths come from the files: print them with their control characters escaped.
                report.line(format_args!(
                    "FAILED {}: {}",
                    member.escape_debug(),
                    err.to_string().escape_debug()
                ));
            }
        }
    }
    report.line(format_args!(
        "{files} sprite files, {frames} frames, {shadows} shadow frames, \
         {extras} files with an extra frame, {failed} failed, {warned} with a warning"
    ));
    report.finish()?;
    match failed {
        0 => Ok(()),
        _ => Err(format!("{failed} of {files} sprite files failed").into()),
    }
}

/// The names of the eight screen directions, in the order of the numbers 0 to 7.
const DIRECTIONS: [&str; 8] = ["N", "NE", "E", "SE", "S", "SW", "W", "NW"];

/// The largest width or height of the canvas of `render-ani`.
const MAX_CANVAS: i64 = sprite::MAX_DIMENSION as i64;

/// Draws frame 0 of the set for each of the eight screen directions on a transparent canvas
/// that holds the box of the set, the box of its frames and their mirror images, with the
/// anchor at (`m`, `top`) where `m` is the largest distance of the box from the anchor to the
/// left or right and `top` its distance above.
pub fn render_ani(vfs: &Vfs, member: String, dir: &Path) -> Result<(), CliError> {
    let set = AnimSet::load(vfs, &member)?;
    let animation = Animation::open(vfs, set).map_err(|(err, _)| err)?;
    let set = &animation.set;
    let [e0, f0, e1, f1] = animation.extents().unwrap_or_default();
    let (x0, y0, x1, y1) = (
        i64::from(set.x0.min(e0)),
        i64::from(set.y0.min(f0)),
        i64::from(set.x1.max(e1)),
        i64::from(set.y1.max(f1)),
    );
    let reach = x0.abs().max(x1.abs());
    let (anchor_x, anchor_y) = (reach, (-y0).max(0));
    let (width, height) = (2 * reach + 1, (anchor_y + y1.max(0)).max(1));
    if width > MAX_CANVAS || height > MAX_CANVAS {
        return Err(format!("the box {x0} {y0} {x1} {y1} is too large to draw").into());
    }
    std::fs::create_dir_all(dir).map_err(|e| format!("cannot create {}: {e}", dir.display()))?;
    let mut report = Report::new();
    report.line(format_args!(
        "{}: {} files, box {x0} {y0} {x1} {y1}; canvas {width}x{height}, anchor ({anchor_x}, {anchor_y})",
        member.escape_debug(),
        animation.sprites.len(),
    ));
    if !matches!(animation.sprites.len(), 1 | 4 | 5) {
        report.line(format_args!(
            "note: a set of {} files is not chosen by screen direction; the images show the rule for it anyway",
            animation.sprites.len()
        ));
    }
    if let Some(warning) = &animation.warning {
        report.line(format_args!("warning: {}", warning.escape_debug()));
    }
    // A set of shadow frames only darkens what is behind it: show it on white.
    let on_white = animation
        .sprites
        .iter()
        .any(|loaded| loaded.0.frames.first().is_some_and(|frame| frame.shadow));
    for (direction, name) in DIRECTIONS.iter().enumerate() {
        let view = animation.view(direction as u8);
        let mut image = Framebuffer::new(width as u32, height as u32);
        if on_white {
            image.pixels.fill(255);
        }
        animation.draw(view, 0, &mut image, anchor_x as i32, anchor_y as i32);
        let file = format!("dir{direction}-{name}.png");
        let path = dir.join(&file);
        let out =
            File::create(&path).map_err(|e| format!("cannot create {}: {e}", path.display()))?;
        write_png(&image, io::BufWriter::new(out))
            .map_err(|e| format!("cannot write {}: {e}", path.display()))?;
        report.line(format_args!(
            "  {file}: file {} ({}){}",
            view.file,
            animation.set.files[view.file]
                .rsplit('/')
                .next()
                .unwrap_or_default()
                .escape_debug(),
            if view.mirror { ", mirrored" } else { "" },
        ));
    }
    report.finish()
}

/// Loads every `.ani` member and the sprite files it names. A set that cannot be read, or one
/// with a sprite file that cannot be found, fails, as in the original. A defective palette is a
/// warning: the set loads.
pub fn check_anis(vfs: &Vfs) -> Result<(), CliError> {
    let mut report = Report::new();
    let (mut sets, mut files, mut missing, mut mirrored, mut failed, mut warned) =
        (0, 0, 0, 0, 0, 0);
    let mut sizes = std::collections::BTreeMap::new();
    let (mut exact, mut larger, mut different) = (0, 0, 0);
    for member in vfs.list(None) {
        if !member.ends_with(".ani") {
            continue;
        }
        sets += 1;
        let result = AnimSet::load(vfs, &member).and_then(|set| {
            files += set.files.len();
            *sizes.entry(set.files.len()).or_insert(0) += 1;
            mirrored += (0..8)
                .filter(|&u| anim::view_for(set.files.len(), u).mirror)
                .count();
            Animation::open(vfs, set).map_err(|(err, lost)| {
                missing += lost;
                err
            })
        });
        match result {
            Ok(animation) => {
                let set = &animation.set;
                let boxed = [set.x0, set.y0, set.x1, set.y1];
                let extents = animation.extents().unwrap_or_default();
                let holds = boxed[0] <= extents[0]
                    && boxed[1] <= extents[1]
                    && boxed[2] >= extents[2]
                    && boxed[3] >= extents[3];
                match (boxed == extents, holds) {
                    (true, _) => exact += 1,
                    (_, true) => larger += 1,
                    _ => different += 1,
                }
                match &animation.warning {
                    Some(warning) => {
                        warned += 1;
                        report.line(format_args!(
                            "warning {}: {}",
                            member.escape_debug(),
                            warning.escape_debug()
                        ));
                    }
                    None => report.line(format_args!("ok {}", member.escape_debug())),
                }
            }
            Err(err) => {
                failed += 1;
                // Names come from the files: print them with their control characters escaped.
                report.line(format_args!(
                    "FAILED {}: {}",
                    member.escape_debug(),
                    err.to_string().escape_debug()
                ));
            }
        }
    }
    let sizes: Vec<String> = sizes.iter().map(|(n, c)| format!("{n}:{c}")).collect();
    report.line(format_args!(
        "{sets} animation sets, {files} sprite file references, {missing} missing files, \
         {mirrored} mirrored views by the file-count rule, {failed} failed, \
         {warned} with a warning; sets by file count {}; \
         box: {exact} equal to the frame extents, {larger} larger, {different} different",
        sizes.join(" ")
    ));
    report.finish()?;
    match failed {
        0 => Ok(()),
        _ => Err(format!("{failed} of {sets} animation sets failed").into()),
    }
}

#[cfg(test)]
mod tests {
    use super::frame_file_name;

    #[test]
    fn frame_files_are_named_by_number_shadow_and_extra() {
        assert_eq!(frame_file_name(0, false, false), "000.png");
        assert_eq!(frame_file_name(12, true, false), "012-shadow.png");
        assert_eq!(frame_file_name(1234, false, true), "1234-extra.png");
        assert_eq!(frame_file_name(21, true, true), "021-shadow-extra.png");
    }
}
