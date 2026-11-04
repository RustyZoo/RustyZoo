//! The mirrored and the blended draw of a frame, for tests.

use super::{Blend, Frame, Framebuffer, Palette, Zoom};

impl Frame {
    /// Like [`Frame::draw`], reflected left to right: pixel (`c`, `r`) of the frame goes to
    /// (`x - c`, `y + r`), so the frame covers the columns `x - (width - 1)` to `x`.
    pub(crate) fn draw_mirrored(
        &self,
        palette: &Palette,
        target: &mut Framebuffer,
        x: i32,
        y: i32,
    ) {
        self.draw_at(
            palette,
            target,
            self.anchor(x, y, true),
            true,
            None,
            Zoom::Normal,
        );
    }

    /// Like [`Frame::draw`] or [`Frame::draw_mirrored`], but blended with what is behind. Each
    /// channel code of a normal frame becomes `floor((dst + src) / 2)` at strength 0.5 and
    /// `floor((floor((dst + src) / 2) + dst) / 2)` at 0.25 (sprite drawing 7.5); a shadow frame
    /// darkens as in [`Frame::draw`] but at the strength given.
    pub(crate) fn draw_blended(
        &self,
        palette: &Palette,
        target: &mut Framebuffer,
        (x, y): (i32, i32),
        mirror: bool,
        blend: Blend,
    ) {
        let anchor = self.anchor(x, y, mirror);
        self.draw_at(palette, target, anchor, mirror, Some(blend), Zoom::Normal);
    }
}
