//! Blending and the 5-6-5 screen of the original, on the pixels of a frame.

/// A blended draw: `strength` is the weight of the sprite (0.25 or 0.5), `black` draws every
/// pixel of a normal frame in black instead of its palette colour (the shadow of a unit).
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Blend {
    pub strength: f32,
    pub black: bool,
}

/// The low bits each channel drops on the original's 5-6-5 surface: red 8 bits to 5, green 8 to
/// 6, blue 8 to 5. The codes run to `255 >> SHIFT`: 31, 63 and 31.
pub(crate) const SHIFT: [u32; 3] = [3, 2, 3];

/// The 5-6-5 code of the 8 bit channel `c` (0 red, 1 green, 2 blue): `value * max / 255`, rounded
/// down (sprite drawing 7.1). Not `value >> shift`: 254 gives 30.
pub(crate) const fn code(c: usize, value: u8) -> u16 {
    value as u16 * (255 >> SHIFT[c]) / 255
}

/// What the display shows for a code: its high bits repeated (map drawing 7.2).
pub(crate) const fn show(c: usize, code: u16) -> u8 {
    let s = SHIFT[c];
    (code << s | code >> (8 - 2 * s)) as u8
}

/// A colour as the 16 bit screen shows it: reduced to codes, then shown (map drawing 7.2).
/// Colours that were reduced before (palettes) come out one code lower; that is the original's.
pub fn screen(rgb: [u8; 3]) -> [u8; 3] {
    [0, 1, 2].map(|c| SCREENED[c][usize::from(rgb[c])])
}

/// What the screen shows for each value of each channel.
const SCREENED: [[u8; 256]; 3] = {
    let mut table = [[0; 256]; 3];
    let mut c = 0;
    while c < 3 {
        let mut value = 0;
        while value < 256 {
            table[c][value] = show(c, code(c, value as u8));
            value += 1;
        }
        c += 1;
    }
    table
};

/// The code of a channel of a frame pixel, which always holds a shown value.
fn shown_code(c: usize, value: u8) -> u16 {
    u16::from(value >> SHIFT[c])
}

/// Blends the colour `src` into `pixel` (R, G, B, then alpha, which stays) on the codes of the
/// frame: the colour is reduced, blended with the code behind it and shown again.
pub(super) fn mix(pixel: &mut [u8], src: [u8; 3], blend: Blend) {
    for (c, channel) in pixel[..3].iter_mut().enumerate() {
        let dst = shown_code(c, *channel);
        let src = code(c, src[c]) * u16::from(!blend.black);
        let half = (dst + src) / 2;
        let out = if blend.strength == 0.25 {
            (half + dst) / 2
        } else {
            half
        };
        *channel = show(c, out);
    }
}

/// Darkens the codes of `pixel` (R, G, B, then alpha, which stays) by `strength`, the share of
/// the destination that is covered. At 0.25 three quarters stay (`floor(3c / 4)`), at 0.5 one
/// half (`floor(c / 2)`), as the original computes them; other strengths keep `1 - strength`.
pub(super) fn darken(pixel: &mut [u8], strength: f32) {
    for (c, channel) in pixel[..3].iter_mut().enumerate() {
        let dst = shown_code(c, *channel);
        let out = if strength == 0.25 {
            dst * 3 / 4
        } else if strength == 0.5 {
            dst / 2
        } else {
            (f32::from(dst) * (1.0 - strength)) as u16
        };
        *channel = show(c, out);
    }
}
