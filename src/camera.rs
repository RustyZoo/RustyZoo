//! The map view's camera: position, rotation, zoom, the scroll range and the scroll arithmetic of
//! the original (camera spec). No window and no input events: the caller says which key or
//! pointer position it saw. The checks of 6.2 (inactive window, modal dialog, terraform drag,
//! following an entity) are the caller's too.
//!
//! Full screen (C1) changes two rules: the edge band is 2 pixels, not 7, and the view of 800
//! pixels in width cannot zoom in.

use crate::config::Config;
use crate::render::order::{ground_height, project};
use crate::timing::ini_whole;
use crate::zoo::{View, Zoo};

/// The scroll settings of the `[UI]` section.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Settings {
    /// `keyScrollX`, `keyScrollY`: the step of a key, in view pixels.
    pub key: (i32, i32),
    /// `mouseScrollX`: the edge step on both axes (the original overwrites `mouseScrollY` with
    /// it, 9.2).
    pub mouse: i32,
    /// `mouseScrollDelay`: the minimum time between two edge steps, in ms.
    pub delay: i32,
}

impl Default for Settings {
    /// The shipped values, also used for a missing key (open point C2).
    fn default() -> Self {
        Settings {
            key: (64, 64),
            mouse: 32,
            delay: 1,
        }
    }
}

impl Settings {
    pub fn from_config(config: &Config) -> Self {
        let get = |key: &str, default: i32| ini_whole(config, "UI", key).unwrap_or(default);
        let d = Settings::default();
        Settings {
            key: (get("keyScrollX", d.key.0), get("keyScrollY", d.key.1)),
            mouse: get("mouseScrollX", d.mouse),
            delay: get("mouseScrollDelay", d.delay),
        }
    }
}

/// The width of the edge band in a window (9.2): `0..=BAND` on the left and top, the last `BAND`
/// pixels on the right and bottom. In full screen it is [`FULL_BAND`].
const BAND: i32 = 7;

/// The width of the edge band in full screen (9.2).
const FULL_BAND: i32 = 2;

/// The width of the view at which full screen does not offer zoom 2 (8.3).
const NARROW: i32 = 800;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Camera {
    /// -2, 0 or 2 from the interface; a file may store anything.
    pub zoom: i32,
    /// 0 to 3.
    pub rotation: u8,
    /// The view-space position of the window's top-left pixel.
    pub scroll: (i32, i32),
    /// The size of the map view, the window's client size.
    pub size: (i32, i32),
    pub settings: Settings,
    /// The window is full screen: the band and the zoom limit differ.
    fullscreen: bool,
    /// The terraform brush size, 1 to 5, also the size of the selection.
    brush: i32,
    /// The first corner of the map selection, a square of `brush` tiles.
    selection: (i32, i32),
    /// The earliest time of the next edge step, ms.
    next_edge: i64,
}

/// A view-space position at normal zoom, with the zoom of 2.4 applied: the zoom value is the
/// factor, `trunc(p / -z)` below -1 and `p * z` above 1.
pub fn zoom_view(zoom: i32, (px, py): (i32, i32)) -> (i64, i64) {
    let (px, py) = (i64::from(px), i64::from(py));
    let z = i64::from(zoom);
    match zoom {
        ..=-2 => (px / -z, py / -z),
        2.. => (px * z, py * z),
        _ => (px, py),
    }
}

/// The projection of 2.2 with the zoom of 2.4, in view space.
fn project_zoomed(rotation: u8, zoom: i32, wx: i32, wy: i32, wz: i32) -> (i64, i64) {
    zoom_view(zoom, project(rotation, wx, wy, wz))
}

fn to_i32(n: i64) -> i32 {
    n.clamp(i64::from(i32::MIN), i64::from(i32::MAX)) as i32
}

impl Camera {
    /// The camera of a freshly opened map: the stored values as they are, no recentring and no
    /// range check (4.2). A rotation outside 0 to 3 is ignored.
    pub fn open(view: &View, size: (i32, i32), settings: Settings) -> Self {
        Camera {
            zoom: view.zoom,
            rotation: u8::try_from(view.rotation)
                .ok()
                .filter(|&r| r < 4)
                .unwrap_or(0),
            scroll: view.scroll,
            size,
            settings,
            fullscreen: false,
            brush: ((view.brush & 0xff) as i32).clamp(1, 5),
            selection: (0, 0),
            next_edge: 0,
        }
    }

    /// The view as a file stores it.
    pub fn view(&self) -> View {
        View {
            zoom: self.zoom,
            rotation: i32::from(self.rotation),
            scroll: self.scroll,
            brush: self.brush as u32,
        }
    }

    /// The world point on the ground under the centre of the view (5), including its quirks.
    pub fn centre_point(&self, map: &Zoo) -> (i64, i64, i32) {
        self.pick(map, (self.size.0 / 2, self.size.1 / 2))
    }

    /// The tile under a window point, if it is on the map.
    pub fn tile_under(&self, map: &Zoo, at: (i32, i32)) -> Option<(u32, u32)> {
        let (wx, wy, _) = self.pick(map, at);
        let tile = |w: i64| u32::try_from(w.div_euclid(64)).ok();
        let (x, y) = (tile(wx)?, tile(wy)?);
        map.tile(x, y).map(|_| (x, y))
    }

    /// The view-space point at normal zoom of a window point: the scroll plus the point, with
    /// the zoom undone (map drawing 2.5, step 1; camera 5). Saturating.
    pub fn view_point(&self, (x, y): (i32, i32)) -> (i64, i64) {
        let px = i64::from(self.scroll.0) + i64::from(x);
        let py = i64::from(self.scroll.1) + i64::from(y);
        let z = i64::from(self.zoom);
        match self.zoom {
            ..=-2 => (px.saturating_mul(-z), py.saturating_mul(-z)),
            2.. => (px / z, py / z),
            _ => (px, py),
        }
    }

    /// The world point on the ground under a window point (map drawing 2.5): the trial heights
    /// from +192 down to -192 in steps of 4 are undone through the projection, and the one whose
    /// height is nearest to the ground at the point it gives wins.
    pub fn pick(&self, map: &Zoo, at: (i32, i32)) -> (i64, i64, i32) {
        let bound = 1 << 40;
        let (px, py) = self.view_point(at);
        let (px, py) = (px.clamp(-bound, bound), py.clamp(-bound, bound));
        let (c, s) = [(1, 0), (0, -1), (-1, 0), (0, 1)][usize::from(self.rotation)];
        let at = |h: i64| {
            let (f1, f2) = (px - 2 * (py + h), px + 2 * (py + h));
            (c * f1 - s * f2, s * f1 + c * f2)
        };
        let (mut best, mut wz, mut diff) = (at(0), 0, i64::MAX);
        for h in (-192..=192).rev().step_by(4) {
            let (wx, wy) = at(h);
            let Some(tile) = tile_at(map, wx, wy) else {
                continue;
            };
            let ground = ground_height(tile, (wx % 64) as i32, (wy % 64) as i32);
            let d = (i64::from(ground) - h).abs();
            if d < diff {
                // The height comes from this tile at the offset of the previous answer.
                wz = ground_height(tile, (best.0 % 64) as i32, (best.1 % 64) as i32);
                (best, diff) = ((wx, wy), d);
                if d <= 4 {
                    break;
                }
            }
        }
        (best.0, best.1, wz)
    }

    /// The range check (6.1): the centre point stays `k` tiles inside the map.
    pub fn check_range(&mut self, map: &Zoo) {
        let (wx, wy, wz) = self.centre_point(map);
        let k = match self.zoom {
            ..=-2 => 4,
            2.. => 1,
            _ => 2,
        };
        let limit = |w: i64, tiles: u32| w.max(64 * k).min(64 * (i64::from(tiles) - k) - 1);
        let (cx, cy) = (limit(wx, map.width), limit(wy, map.height));
        if (cx, cy) != (wx, wy) {
            let (px, py) = project_zoomed(self.rotation, self.zoom, to_i32(cx), to_i32(cy), wz);
            self.scroll = self.scroll_for(px, py);
        }
    }

    /// The scroll that puts the view-space point at the centre of the view.
    fn scroll_for(&self, px: i64, py: i64) -> (i32, i32) {
        (
            (px - i64::from(self.size.0 / 2)) as i32,
            (py - i64::from(self.size.1 / 2)) as i32,
        )
    }

    /// Centres the view on the middle of tile `(tx, ty)` (10.1): no range check. A tile off the
    /// map is the height 0.
    pub fn centre_on(&mut self, map: &Zoo, tx: u32, ty: u32) {
        let z = map.tile(tx, ty).map_or(0, |t| ground_height(t, 32, 32));
        let (wx, wy) = (64 * i64::from(tx) + 32, 64 * i64::from(ty) + 32);
        let (px, py) = project_zoomed(self.rotation, self.zoom, to_i32(wx), to_i32(wy), z);
        self.scroll = self.scroll_for(px, py);
    }

    /// Centres the view on the world point `(x, y, z)`, with no range check: the view follows an
    /// entity (10.3).
    pub fn centre_on_point(&mut self, (x, y, z): (i32, i32, i32)) {
        let (px, py) = project_zoomed(self.rotation, self.zoom, x, y, z);
        self.scroll = self.scroll_for(px, py);
    }

    /// A scroll by (0, 0) does nothing, not even the range check (6.1).
    fn scroll_by(&mut self, map: &Zoo, dx: i32, dy: i32) {
        if (dx, dy) == (0, 0) {
            return;
        }
        self.scroll = (
            self.scroll.0.wrapping_add(dx),
            self.scroll.1.wrapping_add(dy),
        );
        self.check_range(map);
    }

    /// One key-down message (9.1); `dir` is -1, 0 or 1 on each axis.
    pub fn key_scroll(&mut self, map: &Zoo, dir: (i32, i32)) {
        let (kx, ky) = self.settings.key;
        self.scroll_by(map, dir.0.wrapping_mul(kx), dir.1.wrapping_mul(ky));
    }

    /// One interface pass at time `now` (ms) with the pointer at `pointer` in the window, if it
    /// is inside it (9.2). Call it on every interface pass, with `None` while a terraform drag,
    /// the follow mode or an inactive window forbid scrolling (6.2): the edge timer advances
    /// then too. Only an open modal dialog stops the pass before the timer, so skip the call
    /// for that.
    pub fn edge_scroll(&mut self, map: &Zoo, now: u64, pointer: Option<(i32, i32)>) {
        let now = i64::try_from(now).unwrap_or(i64::MAX);
        if now < self.next_edge {
            return;
        }
        self.next_edge = now.saturating_add(i64::from(self.settings.delay));
        let Some((x, y)) = pointer else { return };
        let (w, h) = self.size;
        if x < 0 || y < 0 || x >= w || y >= h {
            return;
        }
        let step = self.settings.mouse;
        let band = if self.fullscreen { FULL_BAND } else { BAND };
        let axis = |p: i32, len: i32| match p {
            _ if p <= band => -step,
            _ if p >= len - band => step,
            _ => 0,
        };
        let (dx, dy) = (axis(x, w), axis(y, h));
        self.scroll_by(map, dx, dy);
    }

    /// A quarter turn: clockwise is rotation + 1 (8.2). `selected` is the world position
    /// `(x, y)` of the selected animal, guest or staff member, if any.
    pub fn rotate(&mut self, map: &Zoo, selected: Option<(i32, i32)>, clockwise: bool) {
        let turn = if clockwise { 1 } else { 3 };
        self.change(map, selected, |c| c.rotation = (c.rotation + turn) % 4);
    }

    /// Zoom in (+2) or out (-2); nothing at the limit, as the disabled button (8.3). Full screen
    /// at a width of 800 also refuses zoom in at zoom 0.
    pub fn zoom(&mut self, map: &Zoo, selected: Option<(i32, i32)>, zoom_in: bool) {
        let (limit, step) = if zoom_in { (2, 2) } else { (-2, -2) };
        let refused = self.zoom == limit || (zoom_in && self.zoom == 0 && self.narrow());
        if !refused {
            self.change(map, selected, |c| c.zoom += step);
        }
    }

    /// Full screen with a view of 800 pixels in width (8.3).
    fn narrow(&self) -> bool {
        self.fullscreen && self.size.0 == NARROW
    }

    /// The window goes to full screen or back. A view at zoom 2 that full screen does not offer
    /// is zoomed out to 0 with the steps of 8.1 (8.3).
    pub fn set_fullscreen(&mut self, map: &Zoo, fullscreen: bool) {
        self.fullscreen = fullscreen;
        if self.narrow() && self.zoom == 2 {
            self.change(map, None, |c| c.zoom = 0);
        }
    }

    /// The steps of 8.1.
    fn change(&mut self, map: &Zoo, selected: Option<(i32, i32)>, apply: impl FnOnce(&mut Self)) {
        // 1. Remember the tile.
        let (rx, ry) = match selected {
            Some((x, y)) => (i64::from(x), i64::from(y)),
            None => {
                let p = self.centre_point(map);
                (p.0, p.1)
            }
        };
        let (tx, ty) = (rx / 64, ry / 64);
        let n = self.brush;
        let offset = n / 2 - i32::from(n % 2 == 0);
        let on_map =
            (0..i64::from(map.width)).contains(&tx) && (0..i64::from(map.height)).contains(&ty);
        if on_map
            && (tx, ty)
                != (
                    i64::from(self.selection.0 + offset),
                    i64::from(self.selection.1 + offset),
                )
        {
            let first =
                |t: i64, tiles: u32| to_i32(t - i64::from(offset)).min(tiles as i32 - n).max(0);
            self.selection = (first(tx, map.width), first(ty, map.height));
        }
        // 2. Change.
        apply(self);
        // 3. Locate the tile.
        let (tx, ty) = (self.selection.0 + offset, self.selection.1 + offset);
        let z = u32::try_from(tx)
            .ok()
            .zip(u32::try_from(ty).ok())
            .and_then(|(x, y)| map.tile(x, y))
            .map_or(0, |t| ground_height(t, 32, 32));
        let (px, py) = project_zoomed(self.rotation, self.zoom, 64 * tx + 32, 64 * ty + 32, z);
        let (lx, ly) = (px - i64::from(self.scroll.0), py - i64::from(self.scroll.1));
        // 4. Keep or centre.
        let (w, h) = (i64::from(self.size.0), i64::from(self.size.1));
        if (0..=w).contains(&lx) && (0..=h).contains(&ly) {
            let dx = if lx < 64 {
                -64
            } else if lx > w - 64 {
                64
            } else {
                0
            };
            let dy = if ly < 32 {
                -32
            } else if ly > h - 32 {
                32
            } else {
                0
            };
            self.scroll = (
                self.scroll.0.wrapping_add(dx),
                self.scroll.1.wrapping_add(dy),
            );
        } else {
            self.scroll = self.scroll_for(px, py);
        }
        // 5. Range check.
        self.check_range(map);
    }
}

/// The tile under a world position; a coordinate from -63 to -1 counts as tile 0 (5).
fn tile_at(map: &Zoo, wx: i64, wy: i64) -> Option<&crate::zoo::Tile> {
    let (tx, ty) = (wx / 64, wy / 64);
    if tx < 0 || ty < 0 {
        return None;
    }
    map.tile(u32::try_from(tx).ok()?, u32::try_from(ty).ok()?)
}

#[cfg(test)]
mod tests;
