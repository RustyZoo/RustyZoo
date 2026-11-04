//! Palettes: the colours a sprite draws with, and their variants.

use super::blend::{SHIFT, code};
use super::{MAX_COLORS, Reader, SpriteError};

/// A palette: up to [`MAX_COLORS`] colours as R, G, B, A.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Palette {
    pub(super) colors: Vec<[u8; 4]>,
}

impl Palette {
    /// Decodes a palette file. A header that claims more colours than the file holds (a few
    /// palettes of the game do) is cut to the colours present; bytes after the colours the
    /// header counts (a few palettes of the game have them) are ignored. Fails if the file is
    /// shorter than its 4 byte header.
    pub(crate) fn parse(bytes: &[u8]) -> Result<Palette, SpriteError> {
        let mut reader = Reader { data: bytes };
        let count = reader.len("palette colour count")?;
        let colors = reader
            .data
            .as_chunks::<4>()
            .0
            .iter()
            .take(count.min(MAX_COLORS))
            .copied()
            .collect();
        Ok(Palette { colors })
    }

    /// The number of colours.
    pub(crate) fn len(&self) -> usize {
        self.colors.len()
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.colors.is_empty()
    }

    /// Whether pixel value 0 is transparent: only when entry 0 has red 255, green 0 and blue 255
    /// (the marker), whatever its fourth byte. The art never uses value 0 under the marker,
    /// apart from a few stray pixels that are plain defects; palettes that start with another
    /// colour (terrain, interface) use 0 as a colour.
    pub(crate) fn zero_is_transparent(&self) -> bool {
        matches!(self.color(0), Some([255, 0, 255, _]))
    }

    /// Overwrites the entries from `start` with the entries of `from` from `skip` on, at most
    /// `count` of them and none past the end of either palette.
    pub(crate) fn overwrite(&mut self, start: usize, from: &Palette, skip: usize, count: usize) {
        let source = from.colors.get(skip..).unwrap_or_default();
        for (slot, color) in self.colors.iter_mut().skip(start).zip(source).take(count) {
            *slot = *color;
        }
    }

    /// The palette with every colour replaced by `rgb`, the alpha bytes kept. The transparent
    /// marker (see [`Palette::zero_is_transparent`]) is kept as it is.
    pub(crate) fn solid(&self, rgb: [u8; 3]) -> Palette {
        let keep = usize::from(self.zero_is_transparent());
        let mut colors = self.colors.clone();
        for c in colors.iter_mut().skip(keep) {
            *c = [rgb[0], rgb[1], rgb[2], c[3]];
        }
        Palette { colors }
    }

    /// The colour of a palette index, as R, G, B, A.
    pub(crate) fn color(&self, index: u8) -> Option<[u8; 4]> {
        self.colors.get(usize::from(index)).copied()
    }

    /// The palette a tinted entity is drawn with (sprite drawing 7.4, 9.1): every colour becomes
    /// a value that drawing reduces to the per-channel average, rounded down, of its own code and
    /// the code of the tint (`R >> 3`, `G >> 2`, `B >> 3`). The marker of entry 0
    /// stays, so the pixels that were transparent still are.
    pub(crate) fn tinted(&self, tint: [u8; 3]) -> Palette {
        let marker = self.zero_is_transparent();
        let colors = self
            .colors
            .iter()
            .enumerate()
            .map(|(at, &[r, g, b, a])| {
                if marker && at == 0 {
                    return [r, g, b, a];
                }
                let mut out = [r, g, b];
                for (c, value) in out.iter_mut().enumerate() {
                    let max = 255 >> SHIFT[c];
                    let average = (code(c, *value) + u16::from(tint[c] >> SHIFT[c])) / 2;
                    // The smallest 8 bit value with that code: drawing reduces it to the code.
                    *value = (average * 255).div_ceil(max) as u8;
                }
                [out[0], out[1], out[2], a]
            })
            .collect();
        Palette { colors }
    }
}
