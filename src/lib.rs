//! Platform-free core of RustyZoo. It draws into memory; the shell in `main.rs` presents it.

pub mod anim;
pub mod bitmap;
pub mod camera;
pub mod config;
pub mod entity;
pub mod ground;
pub mod pe;
pub mod render;
pub mod sprite;
pub mod strings;
pub mod tga;
pub mod timing;
pub mod vfs;
pub mod zoo;

/// An RGBA8 image: four bytes per pixel in the order R, G, B, A, rows from top to bottom.
pub struct Framebuffer {
    pub width: u32,
    pub height: u32,
    pub pixels: Vec<u8>,
}

impl Framebuffer {
    /// A transparent black framebuffer.
    pub fn new(width: u32, height: u32) -> Self {
        Self {
            width,
            height,
            pixels: vec![0; width as usize * height as usize * 4],
        }
    }

    /// Fills the framebuffer with an opaque gradient: red grows to the right,
    /// green grows downwards, blue is constant.
    pub fn fill_test_pattern(&mut self) {
        let (w, h) = (self.width as usize, self.height as usize);
        let (pixels, _) = self.pixels.as_chunks_mut::<4>();
        for (i, px) in pixels.iter_mut().enumerate() {
            let (x, y) = (i % w, i / w);
            *px = [(x * 256 / w) as u8, (y * 256 / h) as u8, 128, 255];
        }
    }
}

#[cfg(test)]
mod tests {
    use super::Framebuffer;

    #[test]
    fn test_pattern_is_a_gradient() {
        let mut fb = Framebuffer::new(4, 2);
        fb.fill_test_pattern();
        assert_eq!(fb.pixels.len(), 4 * 2 * 4);
        assert_eq!(&fb.pixels[..4], [0, 0, 128, 255], "top left");
        assert_eq!(&fb.pixels[28..], [192, 128, 128, 255], "bottom right");
    }
}
