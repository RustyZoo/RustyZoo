//! Pixel helpers of the painter: rectangles, copies and cuts of pictures.

use super::*;

/// Without an icon, `colorasicon` fills the inner rectangle with the text colour.
pub(super) fn color_as_icon(
    frame: &mut Framebuffer,
    e: &Element,
    (sx, sy, w, h): (i32, i32, i32, i32),
) {
    if e.look.color_as_icon {
        let (bx, by) = (e.x_border, e.y_border);
        let rect = (
            sx.saturating_add(bx),
            sy.saturating_add(by),
            sx.saturating_add(w).saturating_sub(bx),
            sy.saturating_add(h).saturating_sub(by),
        );
        fill_rect(frame, rect, fill_color(e.look.fore_color));
    }
}

/// Copies a canvas to the frame; with `transparent` the pixels equal to the canvas pixel `key`
/// are skipped.
pub(super) fn copy_canvas(
    frame: &mut Framebuffer,
    canvas: &Framebuffer,
    at: (i32, i32),
    e: &Element,
    key: (usize, usize),
) {
    let key = e.transparent.then(|| {
        let i = (key.1 * canvas.width as usize + key.0) * 4;
        let p = &canvas.pixels[i..i + 3];
        [p[0], p[1], p[2]]
    });
    blit(frame, canvas, at, key);
}

/// Whether pixel (x, y) of `image` is not the colour of its pixel `key`; a point outside is not.
pub(super) fn pixel_differs(image: &Framebuffer, (x, y): (i32, i32), key: (usize, usize)) -> bool {
    let at = |x: usize, y: usize| &image.pixels[(y * image.width as usize + x) * 4..][..3];
    match (usize::try_from(x), usize::try_from(y)) {
        (Ok(x), Ok(y)) if x < image.width as usize && y < image.height as usize => {
            at(x, y)
                != at(
                    key.0.min(image.width as usize - 1),
                    key.1.min(image.height as usize - 1),
                )
        }
        _ => false,
    }
}

/// Whether pixel (x, y) of a cut animation is drawn; a point outside is not.
pub(super) fn alpha_at(image: &Framebuffer, (x, y): (i32, i32)) -> bool {
    match (usize::try_from(x), usize::try_from(y)) {
        (Ok(x), Ok(y)) if x < image.width as usize && y < image.height as usize => {
            image.pixels[(y * image.width as usize + x) * 4 + 3] != 0
        }
        _ => false,
    }
}

pub(super) fn top_left(image: &Framebuffer) -> [u8; 3] {
    let p = &image.pixels;
    [p[0], p[1], p[2]]
}

/// Copies the pixels of `src` with alpha to `dst` with the top left corner at `at`, clipped;
/// pixels whose colour is `key` are skipped.
pub(super) fn blit(dst: &mut Framebuffer, src: &Framebuffer, at: (i32, i32), key: Option<[u8; 3]>) {
    let (dw, dh) = (i64::from(dst.width), i64::from(dst.height));
    for row in 0..i64::from(src.height) {
        let y = i64::from(at.1) + row;
        if !(0..dh).contains(&y) {
            continue;
        }
        let line = &src.pixels[row as usize * src.width as usize * 4..][..src.width as usize * 4];
        for (col, pixel) in line.as_chunks::<4>().0.iter().enumerate() {
            let x = i64::from(at.0) + col as i64;
            if !(0..dw).contains(&x)
                || pixel[3] == 0
                || (pixel[3] != FORCED && key == Some([pixel[0], pixel[1], pixel[2]]))
            {
                continue;
            }
            let to = (y * dw + x) as usize * 4;
            dst.pixels[to..to + 4].copy_from_slice(&[pixel[0], pixel[1], pixel[2], 255]);
        }
    }
}

/// Draws a one-pixel frame just inside the rectangle (left, top, right, bottom), clipped.
pub(super) fn frame_rect(
    dst: &mut Framebuffer,
    (l, t, r, b): (i32, i32, i32, i32),
    color: [u8; 4],
) {
    fill_rect(dst, (l, t, r, t + 1), color);
    fill_rect(dst, (l, b - 1, r, b), color);
    fill_rect(dst, (l, t, l + 1, b), color);
    fill_rect(dst, (r - 1, t, r, b), color);
}

/// Fills the rectangle (left, top, right, bottom), clipped.
pub(super) fn fill_rect(dst: &mut Framebuffer, (l, t, r, b): (i32, i32, i32, i32), color: [u8; 4]) {
    let (w, h) = (dst.width as i32, dst.height as i32);
    for y in t.max(0)..b.min(h) {
        for x in l.max(0)..r.min(w) {
            let to = (y as usize * w as usize + x as usize) * 4;
            dst.pixels[to..to + 4].copy_from_slice(&color);
        }
    }
}

/// Cuts a picture for drawing: an image reduced to 5-6-5; a frame of an animation's set drawn once
/// on a transparent picture of its box (drawing reduces the pixels).
pub(super) fn make_cut(
    pictures: &Pictures,
    picture: Picture,
    set: usize,
    frame: usize,
) -> Option<Cut> {
    match picture {
        Picture::Image(m) => {
            let image = pictures.images.get(m)?;
            let mut pixels = Framebuffer::new(image.width as u32, image.height as u32);
            for (to, from) in pixels
                .pixels
                .as_chunks_mut::<4>()
                .0
                .iter_mut()
                .zip(image.rgb.as_chunks::<3>().0)
            {
                *to = fill_color(*from);
            }
            Some(Cut {
                pixels,
                origin: (0, 0),
                animation: false,
            })
        }
        Picture::Animation(a) => {
            let animation = pictures.animations.get(a)?;
            let s = &animation.set;
            let (w, h) = (
                i64::from(s.x1) - i64::from(s.x0),
                i64::from(s.y1) - i64::from(s.y0),
            );
            if !(1..=i64::from(MAX_SIDE)).contains(&w) || !(1..=i64::from(MAX_SIDE)).contains(&h) {
                return None;
            }
            let mut pixels = Framebuffer::new(w as u32, h as u32);
            let view = View {
                file: set,
                mirror: false,
            };
            animation.draw(view, frame, &mut pixels, -s.x0, -s.y0);
            Some(Cut {
                pixels,
                origin: (s.x0, s.y0),
                animation: true,
            })
        }
    }
}
