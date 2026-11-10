//! The minimap of the main bar (map drawing 10): a diamond of one pixel per tile, turned with the
//! view, with the frame of what the map view shows. The picture is made again from the world each
//! time it is due (10.7 point 5), so no tile keeps a colour or a mark.

use std::collections::HashMap;

use crate::Framebuffer;
use crate::camera::Camera;
use crate::config::{Config, to_int};
use crate::entity::{Catalogue, type_of};
use crate::text::fill_color;
use crate::timing::ini_whole;
use crate::world::World;

/// The name ids of the zoo entrances of the base game, Dinosaur Digs and Marine Mania (10.3 4.3).
const ENTRANCES: [i32; 3] = [6002, 6430, 6483];

/// The colour names of the terrain types 0 to 17 (10.2 point 5).
const TERRAIN: [&str; 18] = [
    "Grass",
    "SavannahGrass",
    "Sand",
    "Dirt",
    "ForestFloor",
    "BrownRock",
    "GrayRock",
    "Gravel",
    "Snow",
    "FreshWater",
    "SaltWater",
    "DeciduousForestFloor",
    "Waterfall",
    "ConiferousForestFloor",
    "Concrete",
    "Asphalt",
    "Trampled",
    "Gunnite",
];

/// The scroll, zoom and rotation a picture was made for.
type Look = ((i32, i32), i32, u8);

/// The colours of a colour file (10.2).
#[derive(Default)]
struct Colors(HashMap<Vec<u8>, [u8; 3]>);

impl Colors {
    /// A name is its section's first three `color` values as bytes; a section that repeats keeps
    /// its last three (the last section of the file) and one with fewer than three is ignored.
    fn parse(bytes: &[u8]) -> Colors {
        let Ok(file) = Config::parse(bytes) else {
            return Colors::default();
        };
        let mut names = HashMap::new();
        for (name, section) in file.sections() {
            let values = section.get_all("color");
            let end = values.len() / 3 * 3;
            if end >= 3 {
                let at = |i: usize| to_int(&values[end - 3 + i]).unwrap_or(0) as u8;
                names.insert(name.to_vec(), [at(0), at(1), at(2)]);
            }
        }
        Colors(names)
    }

    /// The colour of a name; black without a section (10.2 point 3).
    fn get(&self, name: &str) -> [u8; 3] {
        self.0.get(name.as_bytes()).copied().unwrap_or_default()
    }
}

/// The map size and the view's rotation: the cells and the tiles they show (10.4 point 2).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Grid {
    w: i64,
    h: i64,
    rotation: u8,
}

impl Grid {
    /// The tile that cell `(c, r)` shows; the turns use the sizes, not the sizes minus one (10.4
    /// point 3).
    fn tile(self, c: i64, r: i64) -> (i64, i64) {
        match self.rotation {
            1 => (r, self.w - c),
            2 => (self.w - c, self.h - r),
            3 => (self.h - r, c),
            _ => (c, r),
        }
    }

    /// The cell that shows tile `(x, y)`.
    fn cell(self, x: i64, y: i64) -> (i64, i64) {
        match self.rotation {
            1 => (self.h - y, x),
            2 => (self.w - x, self.h - y),
            3 => (y, self.w - x),
            _ => (x, y),
        }
    }

    /// The pixel `(px, py)` of cell `(c, r)` of the diamond.
    fn pixel(self, c: i64, r: i64) -> (i64, i64) {
        (c + r, self.h / 2 + (r - c).div_euclid(2))
    }
}

/// The block of cells that is drawn (10.4 point 4, 10.6 point 2).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Block {
    c0: i64,
    r0: i64,
    /// The cells drawn along the columns and the rows.
    cols: i64,
    rows: i64,
    /// The shift of a map narrower than the widget.
    sx: i64,
    /// The row of the widget the middle of the diamond is on.
    cy: i64,
}

impl Block {
    /// The block centred on cell `(gx, gy)` of a widget `dx` by `dy`, stopped at the map's edges.
    fn of(grid: Grid, (dx, dy): (i64, i64), (gx, gy): (i64, i64)) -> Block {
        let (hw, hh) = (dx / 2, dy);
        let (gx, gy) = (gx.min(grid.w - hw / 2 - 1), gy.min(grid.h - hh / 2 - 1));
        Block {
            c0: (gx - hw / 2).max(0),
            r0: (gy - hh / 2).max(0),
            cols: grid.w.min(hw),
            rows: grid.h.min(hh),
            sx: (hw - grid.w).max(0),
            cy: (dy - 1) / 2,
        }
    }

    /// The pixel of the widget cell `(c, r)` of the block is drawn at (10.4 point 4).
    fn at(self, c: i64, r: i64) -> (i64, i64) {
        (
            c + r - (self.c0 + self.r0) + self.sx,
            self.cy + (r - c).div_euclid(2) - (self.r0 - self.c0).div_euclid(2),
        )
    }

    /// The cell drawn at the widget pixel `(x, y)`, if the block has one there (10.8 point 1).
    fn cell_at(self, (x, y): (i64, i64)) -> Option<(i64, i64)> {
        let sum = x - self.sx + self.c0 + self.r0;
        let half = y - self.cy + (self.r0 - self.c0).div_euclid(2);
        // r - c is 2 * half or 2 * half + 1, whichever has the parity of c + r.
        let diff = 2 * half + sum.rem_euclid(2);
        let (c, r) = ((sum - diff) / 2, (sum + diff) / 2);
        let inside = (self.c0..self.c0 + self.cols).contains(&c)
            && (self.r0..self.r0 + self.rows).contains(&r);
        inside.then_some((c, r))
    }
}

/// The view frame in diamond pixels (10.5): a bound that was not found is `None`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Frame {
    l: Option<i64>,
    r: Option<i64>,
    t: Option<i64>,
    b: Option<i64>,
}

impl Frame {
    /// The frame of a map view `size` wide at `zoom`, centred on cell `(gx, gy)`.
    fn of(grid: Grid, (width, height): (i32, i32), zoom: i32, (gx, gy): (i64, i64)) -> Frame {
        let (a, b) = (i64::from(width) / 64, i64::from(height) / 32);
        let z = i64::from(zoom);
        let (a, b) = match zoom {
            2.. => (a / z, b / z),
            ..=-2 => (a * -z, b * -z),
            _ => (a, b),
        };
        let (ha, hb) = (a / 2, b / 2);
        // The cell at `(c, r)` once the walk along the line of constant `px` (`across`) or `py`
        // has brought it inside; its pixel is the bound.
        let found = |(c, r): (i64, i64), across: bool| {
            let steps = match across {
                true => (c - (grid.w - 1)).max(-r),
                false => (-c).max(-r),
            }
            .max(0);
            let (c, r) = match across {
                true => (c - steps, r + steps),
                false => (c + steps, r + steps),
            };
            ((0..grid.w).contains(&c) && r < grid.h).then(|| grid.pixel(c, r))
        };
        Frame {
            l: found((gx + hb - ha, gy - ha - hb), true).map(|p| p.0),
            r: found((gx + hb + ha, gy + ha - hb), true).map(|p| p.0),
            t: found((gx + hb - ha, gy - ha - hb), false).map(|p| p.1),
            b: found((gx - ha - hb, gy + hb - ha), false).map(|p| p.1),
        }
    }

    /// Whether the pixel `(px, py)` of the diamond is on the frame (10.5 point 3).
    fn on(self, (px, py): (i64, i64)) -> bool {
        let at = |bound: Option<i64>, v: i64| bound == Some(v);
        let within = |lo: Option<i64>, hi: Option<i64>, v: i64| {
            lo.is_none_or(|lo| lo <= v) && hi.is_none_or(|hi| v <= hi)
        };
        ((at(self.l, px) || at(self.r, px)) && within(self.t, self.b, py))
            || ((at(self.t, py) || at(self.b, py)) && within(self.l, self.r, px))
    }
}

/// The minimap of a game: the colours, when it is due, and what it last showed.
pub(crate) struct Minimap {
    colors: Colors,
    /// `[UI] minimapUpdate`: the milliseconds between two refreshes (10.7 point 4).
    interval: i64,
    /// The clock and the view of the last refresh.
    last: Option<(u64, Look)>,
    /// The grid and the block the last picture was made with, for the clicks.
    shown: Option<(Grid, Block)>,
}

impl Minimap {
    /// A minimap with the colour file `colors` (its bytes) and the refresh interval of the ini
    /// file, 0 when it has none (10.10 M1).
    pub(crate) fn new(colors: &[u8], ini: &[u8]) -> Minimap {
        let interval = Config::parse(ini)
            .ok()
            .and_then(|c| ini_whole(&c, "UI", "minimapUpdate"));
        Minimap {
            colors: Colors::parse(colors),
            interval: i64::from(interval.unwrap_or(0)),
            last: None,
            shown: None,
        }
    }

    /// Whether the picture is due at the clock `now` (ms): at the first pass after a view move and
    /// otherwise every `interval` milliseconds.
    pub(crate) fn due(&mut self, now: u64, camera: &Camera) -> bool {
        let look = (camera.scroll, camera.zoom, camera.rotation);
        let due = self.last.is_none_or(|(at, was)| {
            was != look || i64::try_from(now.saturating_sub(at)).unwrap_or(i64::MAX) >= self.interval
        });
        if due {
            self.last = Some((now, look));
        }
        due
    }

    /// The picture for a widget `size` pixels with the background `back`, which the pixels not
    /// drawn keep (10.1 point 3). `entrance` is the entity of the zoo entrance.
    pub(crate) fn draw(
        &mut self,
        world: &World,
        camera: &Camera,
        entrance: Option<usize>,
        size: (i32, i32),
        back: [u8; 3],
    ) -> Framebuffer {
        let mut canvas = Framebuffer::new(size.0.max(1) as u32, size.1.max(1) as u32);
        canvas.fill(fill_color(back));
        let grid = Grid {
            w: i64::from(world.zoo.width),
            h: i64::from(world.zoo.height),
            rotation: camera.rotation,
        };
        let (wx, wy, _) = camera.centre_point(&world.zoo);
        let (gx, gy) = grid.cell(wx / 64, wy / 64);
        let frame = Frame::of(grid, camera.size, camera.zoom, (gx, gy));
        let block = Block::of(grid, (size.0.into(), size.1.into()), (gx, gy));
        let view = self.colors.get("ScreenView");
        for r in block.r0..block.r0 + block.rows {
            for c in block.c0..block.c0 + block.cols {
                let (x, y) = block.at(c, r);
                if !(0..i64::from(canvas.width)).contains(&x)
                    || !(0..i64::from(canvas.height)).contains(&y)
                {
                    continue;
                }
                let colour = match frame.on(grid.pixel(c, r)) {
                    true => view,
                    false => self.cell_colour(world, entrance, grid.tile(c, r)),
                };
                let at = (y as usize * canvas.width as usize + x as usize) * 4;
                canvas.pixels[at..at + 4].copy_from_slice(&fill_color(colour));
            }
        }
        self.shown = Some((grid, block));
        canvas
    }

    /// The tile a click at the widget pixel `at` goes to (10.8 point 1); it may lie off the map.
    pub(crate) fn tile_at(&self, at: (i32, i32)) -> Option<(i64, i64)> {
        let (grid, block) = self.shown?;
        let (c, r) = block.cell_at((at.0.into(), at.1.into()))?;
        Some(grid.tile(c, r))
    }

    /// The colour of the cell that shows tile `(x, y)` (10.3); black off the map (10.7 point 1).
    fn cell_colour(&self, world: &World, entrance: Option<usize>, (x, y): (i64, i64)) -> [u8; 3] {
        let at = i32::try_from(x).ok().zip(i32::try_from(y).ok());
        let Some(t) = at.and_then(|(x, y)| world.tile_index(x, y)) else {
            return [0; 3];
        };
        let tile = world.zoo.tiles[t];
        let ex = world.exhibits().region(t).and_then(|id| world.exhibits().get(id));
        let mut colour = self
            .colors
            .get(TERRAIN.get(usize::from(tile.terrain)).copied().unwrap_or(""));
        if let Some(tank) = ex.and_then(|e| e.tank) {
            colour = self.colors.get(match (tank.show, tank.salt) {
                (true, _) => "showtank",
                (_, true) => "salttank",
                _ => "freshtank",
            });
        }
        let kinds: Vec<&str> = world
            .units_on(t)
            .map(|e| world.zoo.entities[e].class.as_str())
            .collect();
        let path = world.path(t).is_some();
        let edges = [0, 2, 4, 6].map(|d| world.edge(t, d));
        let wall = |f: &dyn Fn(crate::world::Shape) -> bool| {
            edges.iter().flatten().any(|&e| world.shape(e).is_some_and(f))
        };
        let colour = if kinds.iter().any(|k| !matches!(*k, "animals" | "guests")) {
            self.colors.get("keeper")
        } else if kinds.contains(&"animals") {
            self.colors.get("animal")
        } else if entrance.is_some() && world.quadrant(t, 3) == entrance {
            self.colors.get("fgate")
        } else if edges.iter().any(Option::is_some) {
            if wall(&|s| s.wall) {
                self.colors.get("zoowall")
            } else if wall(&|s| s.tank.wall) {
                self.colors.get("tankwall")
            } else if path {
                shift(colour, [-16; 3])
            } else {
                shift(colour, [16, 16, -16])
            }
        } else if path {
            shift(colour, [-16; 3])
        } else if ex.is_some_and(|e| e.tank.is_none()) {
            shift(colour, [-32, 32, -32])
        } else {
            colour
        };
        let up = 4 * tile.elevation;
        shift(colour, [up; 3])
    }
}

/// `colour` plus `by` on each channel, clamped to 0 to 255.
fn shift(colour: [u8; 3], by: [i32; 3]) -> [u8; 3] {
    [0, 1, 2].map(|i| (i32::from(colour[i]) + by[i]).clamp(0, 255) as u8)
}

/// The entity of the zoo entrance: the building that holds the middle of the entrance tile, if its
/// type is one of the entrances (10.3 step 4.3).
pub(crate) fn entrance_of(world: &World, cat: &Catalogue) -> Option<usize> {
    let (x, y) = world.zoo.entrance;
    let t = world.tile_index(i32::try_from(x).ok()?, i32::try_from(y).ok()?)?;
    let at = world.quadrant(t, 3)?;
    let ty = type_of(cat, &world.zoo.entities[at])?;
    ENTRANCES.contains(&ty.name_id).then_some(at)
}

#[cfg(test)]
mod tests;
