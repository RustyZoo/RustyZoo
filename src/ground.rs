//! Ground pictures: the 64x64 picture of one tile, rendered from the terrain textures when the
//! terrain cache of the data (`ztats/`, `ztatb/`) has no picture for it.
//!
//! A picture is the textured mesh of one tile (five vertices, four triangles) seen through a
//! fixed orthographic camera, with one more translucent pass for every different terrain type
//! around the tile. Like the original's run-time renderer it does not light the mesh: a pixel
//! is the texel itself, so a slope is as bright as flat ground. (The shipped plain cache,
//! `ztats/`, was made with lighting and is therefore darker or brighter on slopes.) Colours stay RGBA: the original reduces its picture to at most 256
//! colours before it caches it, but how it does so is not specified and no player would see a
//! difference, so nothing is reduced here.

use std::error::Error;
use std::fmt;

use crate::Framebuffer;
use crate::config::{self, Config};
use crate::sprite::{Frame, Palette};
use crate::tga::Texture;
use crate::vfs::Vfs;

/// The width and height of the image a picture is rendered into, before it is cut.
pub const SIZE: usize = 64;

/// The shape bytes that tiles have; the game never asks for another.
pub const SHAPES: [u8; 19] = [
    0x00, 0x01, 0x04, 0x05, 0x10, 0x11, 0x14, 0x15, 0x19, 0x40, 0x41, 0x44, 0x45, 0x46, 0x50, 0x51,
    0x54, 0x64, 0x91,
];

/// Why a picture cannot be made.
#[derive(Debug)]
pub struct GroundError(String);

impl fmt::Display for GroundError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl Error for GroundError {}

fn error<T>(message: impl Into<String>) -> Result<T, GroundError> {
    Err(GroundError(message.into()))
}

/// What a ground picture depends on: the terrain type of the tile, its shape as seen from the
/// view (not the stored shape byte), the variation 0 to 8 and the terrain type seen in each of
/// the eight screen directions, starting at the upper left edge and going clockwise (edges are
/// the even numbers, corners the odd ones).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Key {
    pub terrain: u8,
    pub shape: u8,
    pub variation: u8,
    pub neighbours: [u8; 8],
}

/// `N` bytes written as `2*N` hexadecimal digits.
fn hex_bytes<const N: usize>(text: &str) -> Option<[u8; N]> {
    if text.len() != 2 * N || !text.bytes().all(|b| b.is_ascii_hexdigit()) {
        return None;
    }
    let mut bytes = [0; N];
    for (i, byte) in bytes.iter_mut().enumerate() {
        *byte = u8::from_str_radix(&text[2 * i..2 * i + 2], 16).ok()?;
    }
    Some(bytes)
}

impl Key {
    /// Reads a key written as the cache names it: `ztats/<type><shape><variation>` for a tile
    /// without different neighbours, `ztatb/<type>/<n0..n3>/<n4..n7>/<shape><variation>`
    /// otherwise, all in hexadecimal pairs. The leading `ztats/` or `ztatb/` and the trailing
    /// `.lle` may be left out, and `\` stands for `/`.
    pub fn parse(name: &str) -> Result<Key, GroundError> {
        let lower = name.to_ascii_lowercase();
        let lower = lower.strip_suffix(".lle").unwrap_or(&lower);
        let parts: Vec<&str> = lower.split(['/', '\\']).collect();
        let key = match parts.as_slice() {
            ["ztats", file] | [file] => hex_bytes(file).map(|[terrain, shape, variation]| Key {
                terrain,
                shape,
                variation,
                neighbours: [terrain; 8],
            }),
            ["ztatb", dir, near, far, file] | [dir, near, far, file] => hex_bytes(dir)
                .zip(hex_bytes(file))
                .zip(hex_bytes::<4>(near).zip(hex_bytes::<4>(far)))
                .map(|(([terrain], [shape, variation]), (near, far))| {
                    let mut neighbours = [0; 8];
                    neighbours[..4].copy_from_slice(&near);
                    neighbours[4..].copy_from_slice(&far);
                    Key {
                        terrain,
                        shape,
                        variation,
                        neighbours,
                    }
                }),
            _ => None,
        };
        match key {
            Some(key) if key.variation < 9 && SHAPES.contains(&key.shape) => Ok(key),
            Some(key) if key.variation < 9 => error(format!(
                "shape {:02x} of {name:?} is not one of the 19 tile shapes",
                key.shape
            )),
            _ => error(format!("not a ground picture key: {name:?}")),
        }
    }

    /// Whether no neighbour has another terrain type.
    pub fn is_uniform(&self) -> bool {
        self.neighbours.iter().all(|&n| n == self.terrain)
    }

    /// The name of the picture's file in the cache, for example `ztats/000003.lle`.
    pub fn member(&self) -> String {
        let Key {
            terrain,
            shape,
            variation,
            neighbours: n,
        } = *self;
        if self.is_uniform() {
            format!("ztats/{terrain:02x}{shape:02x}{variation:02x}.lle")
        } else {
            format!(
                "ztatb/{terrain:02x}/{:02x}{:02x}{:02x}{:02x}/{:02x}{:02x}{:02x}{:02x}/{shape:02x}{variation:02x}.lle",
                n[0], n[1], n[2], n[3], n[4], n[5], n[6], n[7]
            )
        }
    }
}

impl Key {
    /// The name of the palette file that goes with the cached picture: `ztats/<type>.pal`,
    /// or for a blended picture the name of its file with `.pal` in place of `.lle`.
    pub fn palette(&self) -> String {
        if self.is_uniform() {
            format!("ztats/{:02x}.pal", self.terrain)
        } else {
            self.member().replace(".lle", ".pal")
        }
    }
}

/// A rendered or cached picture, cut to the bounding box of its visible pixels.
pub struct Picture {
    pub image: Framebuffer,
    /// Columns between the left of the image and the middle of the 64x64 canvas it was cut from.
    pub x_offset: i32,
    /// Rows between the top of the image and the middle of that canvas.
    pub y_offset: i32,
}

impl Picture {
    /// The picture put back on the canvas it was cut from, transparent where it draws nothing.
    pub fn on_canvas(&self) -> Framebuffer {
        let mut canvas = Framebuffer::new(SIZE as u32, SIZE as u32);
        let (left, top) = (32 - self.x_offset, 32 - self.y_offset);
        let width = self.image.width as usize;
        for (row, line) in self.image.pixels.chunks_exact(width.max(1) * 4).enumerate() {
            let y = top + row as i32;
            for (column, pixel) in line.as_chunks::<4>().0.iter().enumerate() {
                let x = left + column as i32;
                if (0..SIZE as i32).contains(&x) && (0..SIZE as i32).contains(&y) {
                    let at = (y as usize * SIZE + x as usize) * 4;
                    canvas.pixels[at..at + 4].copy_from_slice(pixel);
                }
            }
        }
        canvas
    }
}

/// Reads a picture of the cache by the name of its sprite member (`ztats/000003.lle`), with its
/// key. The palette is the one beside it: `ztats/<type>.pal` for a plain picture, the same name
/// with `.pal` for a blended one.
pub fn load_cached(vfs: &Vfs, member: &str) -> Result<(Key, Picture), GroundError> {
    let key = Key::parse(member)?;
    let palette = key.palette();
    let read = |path: &str| {
        vfs.read(path)
            .map_err(|e| GroundError(format!("{member}: {e}")))
    };
    let wrap = |e: crate::sprite::SpriteError| GroundError(format!("{member}: {e}"));
    // A cached picture is a bare frame, without the header of a sprite file.
    let frame = Frame::parse(&read(member)?).map_err(wrap)?;
    let palette = Palette::parse(&read(&palette)?).map_err(wrap)?;
    let mut image = Framebuffer::new(frame.width.into(), frame.height.into());
    frame.draw(&palette, &mut image, 0, 0);
    let picture = Picture {
        image,
        x_offset: frame.x_offset.into(),
        y_offset: frame.y_offset.into(),
    };
    Ok((key, picture))
}

type Vec3 = [f64; 3];

fn sub(a: Vec3, b: Vec3) -> Vec3 {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

fn dot(a: Vec3, b: Vec3) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

fn cross(a: Vec3, b: Vec3) -> Vec3 {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}

/// The vector scaled to length 1; the zero vector stays zero.
fn unit(a: Vec3) -> Vec3 {
    let length = dot(a, a).sqrt();
    if length == 0.0 {
        a
    } else {
        a.map(|c| c / length)
    }
}

/// Height of one step of a corner in the mesh, in units of its 3x3 square.
const STEP: f64 = 1.225;

const LEFT: usize = 0;
const TOP: usize = 1;
const RIGHT: usize = 2;
const BOTTOM: usize = 3;
const CENTRE: usize = 4;

/// The five vertices for a shape as seen from the view: the left, top, right and bottom
/// corners and the centre; y points up.
fn mesh(shape: u8) -> [Vec3; 5] {
    let field = |shift: u32| f64::from((shape >> shift) & 3);
    let (left, bottom, right, top) = (field(0), field(2), field(4), field(6));
    let height = |corner: f64| STEP * (corner - left);
    let mut v = [
        [1.5, 0.0, -1.5],
        [1.5, height(top), 1.5],
        [-1.5, height(right), 1.5],
        [-1.5, height(bottom), -1.5],
        [0.0, 0.0, 0.0],
    ];
    v[CENTRE][1] = if left == right && !(top == bottom && left > top) {
        // Fold along the left-right diagonal.
        0.0
    } else if top == bottom {
        // Fold along the top-bottom diagonal.
        v[TOP][1]
    } else {
        (v[TOP][1] + v[RIGHT][1] + v[BOTTOM][1]) / 4.0
    };
    v
}

/// The place of a mesh point in the 64x64 image, in pixels.
fn project(p: Vec3) -> [f64; 2] {
    // Orthographic camera at (-39.17, 33, -39.17) looking at the origin, y up, right handed.
    let eye = [-39.17, 33.0, -39.17];
    let forward = unit(sub([0.0; 3], eye));
    let right = unit(cross(forward, [0.0, 1.0, 0.0]));
    let up = cross(right, forward);
    // The view volume is 4.206 units across.
    let scale = SIZE as f64 / 2.0 / 2.103;
    [
        SIZE as f64 / 2.0 + scale * dot(right, p),
        SIZE as f64 / 2.0 - scale * dot(up, p),
    ]
}

/// Per-vertex data that the rasteriser interpolates: alpha (0 to 255), u, v.
type Attributes = [f64; 3];

/// Calls `put(x, y, attributes)` for each pixel of a `width` x `height` image whose centre,
/// at integer coordinates, is inside the triangle. A centre exactly on an edge belongs to the
/// triangle only if the edge is a top edge (horizontal, with the triangle below it) or a left
/// edge (with the triangle to its right), so two triangles that share an edge never draw a
/// pixel twice or leave a gap. Either winding is accepted.
///
/// The corners are rounded to 1/256 of a pixel first, which makes the edge tests exact.
fn triangle(
    corners: [([f64; 2], Attributes); 3],
    width: i64,
    height: i64,
    mut put: impl FnMut(i64, i64, Attributes),
) {
    let fixed = |c: &([f64; 2], Attributes)| c.0.map(|v| (v * 256.0).round() as i64);
    let edge = |a: [i64; 2], b: [i64; 2], p: [i64; 2]| {
        (b[0] - a[0]) * (p[1] - a[1]) - (b[1] - a[1]) * (p[0] - a[0])
    };
    let mut corners = corners;
    let mut points = corners.each_ref().map(fixed);
    let mut area = edge(points[0], points[1], points[2]);
    if area < 0 {
        corners.swap(1, 2);
        points.swap(1, 2);
        area = -area;
    }
    if area == 0 {
        return;
    }
    // Edge i runs from point i to point i+1; the point opposite to it weighs the attributes.
    let owned = |a: [i64; 2], b: [i64; 2]| b[1] < a[1] || (b[1] == a[1] && b[0] > a[0]);
    let edges =
        [(0, 1), (1, 2), (2, 0)].map(|(a, b)| (points[a], points[b], owned(points[a], points[b])));
    let first = |f: fn(&[i64; 2]) -> i64| points.iter().map(f).min().unwrap_or(0);
    let last = |f: fn(&[i64; 2]) -> i64| points.iter().map(f).max().unwrap_or(0);
    let (x0, x1) = (
        (first(|p| p[0]) + 255).div_euclid(256).max(0),
        last(|p| p[0]).div_euclid(256).min(width - 1),
    );
    let (y0, y1) = (
        (first(|p| p[1]) + 255).div_euclid(256).max(0),
        last(|p| p[1]).div_euclid(256).min(height - 1),
    );
    for y in y0..=y1 {
        for x in x0..=x1 {
            let centre = [x * 256, y * 256];
            let e = edges.map(|(a, b, owned)| (edge(a, b, centre), owned));
            if e.iter()
                .all(|&(value, owned)| value > 0 || (value == 0 && owned))
            {
                // The weight of corner i is the edge test of the edge opposite to it.
                let weights = [e[1].0, e[2].0, e[0].0].map(|w| w as f64 / area as f64);
                let mut attributes = [0.0; 3];
                for (weight, corner) in weights.iter().zip(&corners) {
                    for (sum, value) in attributes.iter_mut().zip(corner.1) {
                        *sum += weight * value;
                    }
                }
                put(x, y, attributes);
            }
        }
    }
}

/// Everything needed to render ground pictures: one texture per terrain type.
#[derive(Debug)]
pub struct Renderer {
    /// Indexed by terrain type; the error says why a type cannot be drawn.
    textures: Vec<Result<Texture, String>>,
}

impl Renderer {
    /// Reads the terrain type table (every `terrain/tilete*.cfg`), the textures it names. A section whose `type` is not below the number of sections of all
    /// the files is ignored. A texture that is missing or malformed only fails the pictures
    /// that use it.
    ///
    /// Fails if there is no terrain type or a file cannot be read.
    pub fn load(vfs: &Vfs) -> Result<Renderer, GroundError> {
        let config = |path: &str| {
            let bytes = vfs.read(path).map_err(|e| GroundError(e.to_string()))?;
            Config::parse(&bytes).map_err(|e| GroundError(format!("{path}: {e}")))
        };
        let mut types = Vec::new();
        let mut count = 0;
        for path in vfs.list(Some("terrain/tilete*.cfg")) {
            for (_, section) in config(&path)?.sections() {
                count += 1;
                if let (Some(number), Some(texture)) = (section.get("type"), section.get("texture"))
                {
                    types.push((
                        config::to_int(number),
                        String::from_utf8_lossy(texture).into_owned(),
                    ));
                }
            }
        }
        if count == 0 {
            return error("the data has no terrain type");
        }
        let mut textures: Vec<_> = (0..count)
            .map(|n| Err(format!("no terrain type {n}")))
            .collect();
        for (number, path) in types {
            if let Some(slot) = number
                .and_then(|n| usize::try_from(n).ok())
                .and_then(|n| textures.get_mut(n))
            {
                *slot = vfs
                    .read(&path)
                    .map_err(|e| e.to_string())
                    .and_then(|bytes| Texture::parse(&bytes).map_err(|e| format!("{path}: {e}")));
            }
        }
        Ok(Renderer { textures })
    }

    fn texture(&self, terrain: u8) -> Result<&Texture, GroundError> {
        match self.textures.get(usize::from(terrain)) {
            Some(Ok(texture)) => Ok(texture),
            Some(Err(message)) => error(message.clone()),
            None => error(format!("no terrain type {terrain}")),
        }
    }

    /// Renders the picture of a key: the tile's own texture, then one translucent pass for each
    /// other terrain type around it, in increasing type number, then the cut.
    ///
    /// A terrain type at or above the number of types is drawn as type 0, as in the original.
    ///
    /// Fails if the key names a terrain type that has no usable texture.
    pub fn render(&self, key: &Key) -> Result<Picture, GroundError> {
        let known = |t: u8| {
            if usize::from(t) < self.textures.len() {
                t
            } else {
                0
            }
        };
        let key = &Key {
            terrain: known(key.terrain),
            neighbours: key.neighbours.map(known),
            ..*key
        };
        let screen = mesh(key.shape).map(project);
        let (i, j) = (f64::from(key.variation / 3), f64::from(key.variation % 3));
        let uv = [
            [i / 3.0, j / 3.0],
            [(i + 1.0) / 3.0, j / 3.0],
            [(i + 1.0) / 3.0, (j + 1.0) / 3.0],
            [i / 3.0, (j + 1.0) / 3.0],
            [i / 3.0 + 1.0 / 6.0, j / 3.0 + 1.0 / 6.0],
        ];
        let mut image = vec![0u8; SIZE * SIZE * 4];
        let mut pass = |texture: &Texture, alphas: [u32; 5]| {
            let vertex = |v: usize| (screen[v], [f64::from(alphas[v]), uv[v][0], uv[v][1]]);
            for [a, b, c] in [
                [LEFT, CENTRE, TOP],
                [TOP, CENTRE, RIGHT],
                [RIGHT, CENTRE, BOTTOM],
                [BOTTOM, CENTRE, LEFT],
            ] {
                // The original culls triangles that turn clockwise on the screen, but none of
                // the 19 shapes has one, so nothing is culled here.
                triangle(
                    [vertex(a), vertex(b), vertex(c)],
                    SIZE as i64,
                    SIZE as i64,
                    |x, y, at| {
                        // The nearest texel; u and v of 1 belong to the last one.
                        let column = ((at[1] * texture.width as f64).floor().max(0.0) as usize)
                            .min(texture.width - 1);
                        let row = ((at[2] * texture.height as f64).floor().max(0.0) as usize)
                            .min(texture.height - 1);
                        let texel = &texture.rgb[(row * texture.width + column) * 3..][..3];
                        let alpha = (at[0] / 255.0).clamp(0.0, 1.0);
                        let pixel = &mut image[(y as usize * SIZE + x as usize) * 4..][..4];
                        for c in 0..3 {
                            pixel[c] = (f64::from(texel[c]) * alpha
                                + f64::from(pixel[c]) * (1.0 - alpha))
                                .round() as u8;
                        }
                        // The first pass is opaque everywhere the mesh is, and the later ones cover
                        // the same pixels, so a drawn pixel is always opaque.
                        pixel[3] = 255;
                    },
                );
            }
        };

        pass(self.texture(key.terrain)?, [255; 5]);
        let mut counters = [2u32; 5];
        let around = key.neighbours;
        for terrain in (0..=255u8).filter(|&t| t != key.terrain && around.contains(&t)) {
            let alphas = pass_alphas(&around, terrain, &counters);
            if alphas.iter().any(|&a| a != 0) {
                pass(self.texture(terrain)?, alphas);
                for (counter, alpha) in counters.iter_mut().zip(alphas) {
                    *counter += u32::from(alpha != 0);
                }
            }
        }
        Ok(cut(&image))
    }
}

/// The vertex alphas (left, top, right, bottom, centre) of the pass for the terrain type
/// `terrain`, given how many passes each vertex has had (the counters start at 2). A vertex
/// that has been blended before gets less of the next type: with the counter `c` a corner gets
/// `255 / c` and the centre `127 / c`, so the first foreign type is 127 and 63, the second 85
/// and 42.
fn pass_alphas(neighbours: &[u8; 8], terrain: u8, counters: &[u32; 5]) -> [u32; 5] {
    let mut alphas = [0; 5];
    for (direction, from, to) in [
        (0, LEFT, TOP),
        (2, TOP, RIGHT),
        (4, RIGHT, BOTTOM),
        (6, BOTTOM, LEFT),
    ] {
        if neighbours[direction] == terrain {
            alphas[from] = 255 / counters[from];
            alphas[to] = 255 / counters[to];
            alphas[CENTRE] = 127 / counters[CENTRE];
        }
    }
    for (direction, vertex) in [(1, TOP), (3, RIGHT), (5, BOTTOM), (7, LEFT)] {
        if neighbours[direction] == terrain {
            alphas[vertex] = 255 / counters[vertex];
        }
    }
    alphas
}

/// The part of a 64x64 RGBA image inside the bounding box of its non-transparent pixels. An
/// image with no such pixel gives an empty picture.
fn cut(image: &[u8]) -> Picture {
    let visible = |i: &usize| image[i * 4 + 3] != 0;
    let columns = || (0..SIZE * SIZE).filter(visible).map(|i| i % SIZE);
    let rows = || (0..SIZE * SIZE).filter(visible).map(|i| i / SIZE);
    let (Some(left), Some(right), Some(top), Some(bottom)) =
        (columns().min(), columns().max(), rows().min(), rows().max())
    else {
        return Picture {
            image: Framebuffer::new(0, 0),
            x_offset: 0,
            y_offset: 0,
        };
    };
    let mut cut = Framebuffer::new((right - left + 1) as u32, (bottom - top + 1) as u32);
    for (row, line) in cut
        .pixels
        .chunks_exact_mut(cut.width as usize * 4)
        .enumerate()
    {
        line.copy_from_slice(&image[((top + row) * SIZE + left) * 4..][..line.len()]);
    }
    Picture {
        image: cut,
        x_offset: 32 - left as i32,
        y_offset: 32 - top as i32,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A texture of one colour.
    fn solid(rgb: [u8; 3]) -> Result<Texture, String> {
        Ok(Texture {
            width: 6,
            height: 6,
            rgb: rgb.repeat(36),
        })
    }

    /// A renderer whose terrain types 0, 1 and 2 are red, green and blue.
    fn renderer() -> Renderer {
        Renderer {
            textures: vec![solid([255, 0, 0]), solid([0, 255, 0]), solid([0, 0, 255])],
        }
    }

    fn key(shape: u8) -> Key {
        Key {
            terrain: 0,
            shape,
            variation: 4,
            neighbours: [0; 8],
        }
    }

    /// The red channel at a pixel of the canvas.
    fn red(picture: &Picture, x: usize, y: usize) -> u8 {
        picture.on_canvas().pixels[(y * SIZE + x) * 4]
    }

    #[test]
    fn keys_round_trip_through_the_cache_names() {
        let uniform = Key::parse("ztats/0a1403.lle").unwrap();
        assert_eq!(
            uniform,
            Key {
                terrain: 10,
                shape: 0x14,
                variation: 3,
                neighbours: [10; 8]
            }
        );
        assert_eq!(Key::parse("0A1403").unwrap(), uniform);
        assert_eq!(uniform.member(), "ztats/0a1403.lle");
        assert_eq!(uniform.palette(), "ztats/0a.pal");

        let blended = Key::parse("ztatb/00/00000000/00000001/0003.lle").unwrap();
        assert_eq!(
            blended,
            Key {
                terrain: 0,
                shape: 0,
                variation: 3,
                neighbours: [0, 0, 0, 0, 0, 0, 0, 1]
            }
        );
        assert_eq!(blended.member(), "ztatb/00/00000000/00000001/0003.lle");
        assert_eq!(blended.palette(), "ztatb/00/00000000/00000001/0003.pal");
        assert_eq!(Key::parse("00\\00000000\\00000001\\0003").unwrap(), blended);

        for bad in [
            "",
            "ztats/00",
            "ztats/000009",
            "ztats/00000g",
            "ztatb/00/0000/00000001/0003",
            "a/b/c/d/e/f",
        ] {
            assert!(Key::parse(bad).is_err(), "{bad:?}");
        }
    }

    #[test]
    fn a_flat_tile_is_the_texture_in_a_diamond() {
        let picture = renderer().render(&key(0)).unwrap();
        assert_eq!((picture.image.width, picture.image.height), (64, 33));
        assert_eq!((picture.x_offset, picture.y_offset), (32, 16));
        // No lighting: the texel itself, in the middle and elsewhere; the canvas corner is empty.
        assert_eq!(red(&picture, 32, 32), 255);
        assert_eq!(red(&picture, 20, 30), 255);
        assert_eq!(picture.on_canvas().pixels[3], 0);
    }

    #[test]
    fn a_slope_is_as_bright_as_flat_ground_but_flatter() {
        // Shape 14: the right and bottom corners one step up.
        let slope = renderer().render(&key(0x14)).unwrap();
        assert_eq!(red(&slope, 32, 24), 255);
        assert_eq!(
            slope.image.height, 17,
            "two corners up: a lower, wider picture"
        );
        // The steep shape 19 is a single row.
        assert_eq!(renderer().render(&key(0x19)).unwrap().image.height, 1);
    }

    #[test]
    fn the_centre_follows_the_fold() {
        let step = STEP;
        let centre = |shape| mesh(shape)[CENTRE][1];
        // Left-right fold: the centre is level with the left and right corners.
        assert_eq!(centre(0x04), 0.0, "left and right level");
        assert_eq!(centre(0x44), 0.0, "ridge between two corners that are down");
        // Top-bottom fold: the centre is level with the top and bottom corners.
        assert_eq!(centre(0x01), -step, "left up");
        assert_eq!(centre(0x11), -step, "saddle with left and right up");
        // No pair of opposite corners is level: a quarter of the sum.
        assert_eq!(centre(0x05), -step / 2.0);
    }

    #[test]
    fn the_top_left_rule_draws_each_pixel_of_a_shared_edge_once() {
        // A square from (0,0) to (4,4) cut along its diagonal, which passes through the centres
        // of (1,1), (2,2) and (3,3), in both windings and both ways round.
        let corner = |x, y| ([x, y], [0.0; 3]);
        let (a, b, c, d) = (
            corner(0.0, 0.0),
            corner(4.0, 0.0),
            corner(4.0, 4.0),
            corner(0.0, 4.0),
        );
        for (first, second) in [
            ([a, b, c], [a, c, d]),
            ([a, c, b], [a, d, c]),
            ([c, a, b], [d, a, c]),
        ] {
            let mut drawn = [[0; 6]; 6];
            for half in [first, second] {
                triangle(half, 6, 6, |x, y, _| drawn[y as usize][x as usize] += 1);
            }
            for (y, row) in drawn.iter().enumerate() {
                for (x, &count) in row.iter().enumerate() {
                    assert_eq!(count, i32::from(x < 4 && y < 4), "pixel {x},{y}");
                }
            }
        }
    }

    #[test]
    fn the_rasteriser_interpolates_and_clips() {
        let corner = |x, y, value| ([x, y], [value, 0.0, 0.0]);
        let mut seen = Vec::new();
        triangle(
            [
                corner(-2.0, 0.0, 0.0),
                corner(6.0, 0.0, 8.0),
                corner(-2.0, 8.0, 0.0),
            ],
            3,
            3,
            |x, y, at| {
                seen.push((x, y, at[0]));
            },
        );
        // Value 0 at x = -2 and 8 at x = 6: the value is x + 2, and only 3x3 pixels exist.
        assert!(seen.iter().all(|&(x, y, v)| (0..3).contains(&x)
            && (0..3).contains(&y)
            && (v - (x + 2) as f64).abs() < 1e-9));
        assert!(seen.contains(&(2, 0, 4.0)));
    }

    #[test]
    fn foreign_types_get_the_alpha_sequence() {
        let mut counters = [2; 5];
        // The first foreign type touches the left corner and the upper left edge.
        let n = [1, 0, 0, 0, 0, 0, 0, 1];
        let first = pass_alphas(&n, 1, &counters);
        assert_eq!(first, [127, 127, 0, 0, 63]);
        for (counter, alpha) in counters.iter_mut().zip(first) {
            *counter += u32::from(alpha != 0);
        }
        // The second touches the same left corner.
        let n = [1, 0, 0, 0, 0, 0, 0, 2];
        assert_eq!(pass_alphas(&n, 2, &counters), [85, 0, 0, 0, 0]);
        let n = [1, 2, 2, 0, 0, 0, 0, 0];
        // The top vertex has been blended once, the right one not yet.
        assert_eq!(pass_alphas(&n, 2, &counters), [0, 85, 127, 0, 42]);
        assert_eq!(pass_alphas(&n, 3, &counters), [0; 5]);
    }

    #[test]
    fn a_foreign_corner_blends_in_near_that_corner_only() {
        let picture = renderer()
            .render(&Key {
                neighbours: [0, 0, 0, 0, 0, 0, 0, 1],
                ..key(0)
            })
            .unwrap();
        let canvas = picture.on_canvas();
        let at = |x: usize, y: usize| &canvas.pixels[(y * SIZE + x) * 4..][..3];
        // Next to the left corner about half of the green texture shows through.
        let [r, g, _] = *at(1, 32) else {
            unreachable!()
        };
        assert!(r > 100 && g > 100, "{r} {g}");
        assert_eq!(at(62, 32)[1], 0, "the right corner stays red");
        assert!(at(32, 30)[1] < 40, "the middle gets a lot less");
    }

    #[test]
    fn types_past_the_table_are_type_zero_and_broken_textures_are_errors() {
        // Type 9 is not below the 3 types: it is grass (red), as own type and as neighbour.
        let own = renderer()
            .render(&Key {
                terrain: 9,
                ..key(0)
            })
            .unwrap();
        assert_eq!(red(&own, 32, 32), 255);
        let around = Key {
            neighbours: [9; 8],
            ..key(0)
        };
        assert_eq!(red(&renderer().render(&around).unwrap(), 1, 32), 255);

        let broken = Renderer {
            textures: vec![Err("broken".into())],
        };
        assert_eq!(broken.render(&key(0)).err().unwrap().to_string(), "broken");
    }

    #[test]
    fn shape_bytes_the_game_never_uses_are_refused() {
        assert!(Key::parse("ztats/000400").is_ok());
        let message = Key::parse("ztats/020f00").err().unwrap().to_string();
        assert!(message.contains("not one of the 19"), "{message}");
        for shape in 0..=255u8 {
            let valid = Key::parse(&format!("ztats/00{shape:02x}00")).is_ok();
            assert_eq!(valid, SHAPES.contains(&shape));
            if SHAPES.contains(&shape) {
                let picture = renderer().render(&key(shape)).unwrap();
                assert!(picture.image.width > 0, "shape {shape:02x}");
            }
        }
    }

    #[test]
    fn a_picture_goes_back_where_it_was_cut() {
        let mut image = vec![0u8; SIZE * SIZE * 4];
        image[(10 * SIZE + 20) * 4..][..4].copy_from_slice(&[1, 2, 3, 255]);
        image[(12 * SIZE + 23) * 4..][..4].copy_from_slice(&[4, 5, 6, 255]);
        let picture = cut(&image);
        assert_eq!((picture.image.width, picture.image.height), (4, 3));
        assert_eq!((picture.x_offset, picture.y_offset), (12, 22));
        assert_eq!(picture.on_canvas().pixels, image);
        assert_eq!(cut(&vec![0; SIZE * SIZE * 4]).image.width, 0);
    }

    /// Needs the game: the covered pixels of rendered pictures are the cached ones.
    #[test]
    #[ignore]
    fn rendered_pictures_cover_the_pixels_of_the_cache() {
        let dir = std::env::var_os("ZT_DATA_DIR").expect("ZT_DATA_DIR");
        let vfs = Vfs::open(std::path::Path::new(&dir)).unwrap();
        let renderer = Renderer::load(&vfs).unwrap();
        let palettes: std::collections::HashSet<_> =
            vfs.list(Some("ztat*.pal")).into_iter().collect();
        for member in vfs
            .list(Some("ztats/*.lle"))
            .iter()
            .chain(&vfs.list(Some("ztatb/00/00000000/*.lle")))
        {
            // Two sprites of the game have no palette: the original renders those.
            if !palettes.contains(&Key::parse(member).unwrap().palette()) {
                continue;
            }
            let (key, cached) = load_cached(&vfs, member).unwrap();
            let rendered = renderer.render(&key).unwrap();
            let covered = |p: &Picture| {
                p.on_canvas()
                    .pixels
                    .chunks(4)
                    .map(|c| c[3] != 0)
                    .collect::<Vec<_>>()
            };
            assert_eq!(covered(&cached), covered(&rendered), "{member}");
        }
    }
}
