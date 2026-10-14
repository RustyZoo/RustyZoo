//! The one kind of TGA image the terrain textures use: uncompressed true colour.

/// A decoded texture: RGB, three bytes per pixel, rows from top to bottom.
#[derive(Debug, PartialEq, Eq)]
pub struct Texture {
    pub width: usize,
    pub height: usize,
    pub rgb: Vec<u8>,
}

/// The largest width or height accepted.
const MAX_DIMENSION: usize = 4096;

impl Texture {
    /// Decodes an uncompressed 24 or 32 bit true colour TGA file (an alpha channel is dropped).
    /// Rows are turned top to bottom whatever their stored order.
    ///
    /// Fails if the file is another kind of image, has no pixels, is too large or is truncated.
    pub fn parse(bytes: &[u8]) -> Result<Texture, String> {
        let header = bytes.get(..18).ok_or("truncated TGA header")?;
        let (id_len, color_map, kind, bits, descriptor) =
            (header[0], header[1], header[2], header[16], header[17]);
        let width = usize::from(u16::from_le_bytes([header[12], header[13]]));
        let height = usize::from(u16::from_le_bytes([header[14], header[15]]));
        if color_map != 0 || kind != 2 || !matches!(bits, 24 | 32) || descriptor & 0x10 != 0 {
            return Err(format!(
                "unsupported TGA (colour map {color_map}, type {kind}, {bits} bits, descriptor {descriptor:#04x})"
            ));
        }
        if width == 0 || height == 0 || width > MAX_DIMENSION || height > MAX_DIMENSION {
            return Err(format!("bad TGA size {width}x{height}"));
        }
        let step = usize::from(bits / 8);
        let pixels = bytes
            .get(18 + usize::from(id_len)..)
            .and_then(|data| data.get(..width * height * step))
            .ok_or("truncated TGA pixels")?;
        let mut rgb = Vec::with_capacity(width * height * 3);
        let top_down = descriptor & 0x20 != 0;
        for row in 0..height {
            let stored = if top_down { row } else { height - 1 - row };
            for pixel in pixels[stored * width * step..][..width * step].chunks_exact(step) {
                rgb.extend_from_slice(&[pixel[2], pixel[1], pixel[0]]);
            }
        }
        Ok(Texture { width, height, rgb })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A 2x2 file with a 1 byte image id; blue/green/red/white are the pixels in stored order.
    fn file(descriptor: u8, bits: u8) -> Vec<u8> {
        let mut data = vec![
            1, 0, 2, 0, 0, 0, 0, 0, 0, 0, 0, 0, 2, 0, 2, 0, bits, descriptor, 99,
        ];
        let step = usize::from(bits / 8);
        for bgr in [[255, 0, 0], [0, 255, 0], [0, 0, 255], [255, 255, 255]] {
            data.extend_from_slice(&bgr);
            data.resize(data.len() + step - 3, 7);
        }
        data
    }

    #[test]
    fn stored_rows_are_bottom_up_unless_the_descriptor_says_otherwise() {
        // Stored order: row 0 = blue, green; row 1 = red, white.
        let bottom_up = Texture::parse(&file(0, 24)).unwrap();
        assert_eq!(
            bottom_up.rgb,
            [255, 0, 0, 255, 255, 255, 0, 0, 255, 0, 255, 0]
        );
        let top_down = Texture::parse(&file(0x20, 32)).unwrap();
        assert_eq!(
            top_down.rgb,
            [0, 0, 255, 0, 255, 0, 255, 0, 0, 255, 255, 255]
        );
        assert_eq!((top_down.width, top_down.height), (2, 2));
    }

    #[test]
    fn malformed_files_are_errors() {
        let good = file(0, 24);
        for len in [0, 17, 18, 30] {
            assert!(Texture::parse(&good[..len]).is_err(), "length {len}");
        }
        let mut compressed = good.clone();
        compressed[2] = 10;
        assert!(Texture::parse(&compressed).is_err());
        let mut mapped = good.clone();
        mapped[1] = 1;
        assert!(Texture::parse(&mapped).is_err());
        let mut empty = good.clone();
        empty[12] = 0;
        assert!(Texture::parse(&empty).is_err());
        assert!(Texture::parse(&file(0, 16)).is_err());
        assert!(Texture::parse(&file(0x10, 24)).is_err(), "right to left");
    }
}
