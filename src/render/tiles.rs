//! The ground of the tiles: the type of the terrain, which neighbours it opens to, and the picture
//! of a tile.

use super::placed::{Pic, Piece, Placed};
use super::*;

impl<'a> MapRenderer<'a> {
    /// The type of the terrain of a tile, as the drawing code uses it.
    pub(super) fn kind(&self, scene: &Scene, t: usize) -> u8 {
        let kind = scene.tiles[t].terrain;
        match usize::from(kind) < self.blend.len().max(1) {
            true => kind,
            false => 0,
        }
    }

    pub(super) fn blends(&self, kind: u8) -> bool {
        self.blending && self.blend.get(usize::from(kind)).copied().unwrap_or(true)
    }

    /// Whether the ground blends its neighbours.
    #[cfg(test)]
    pub(crate) fn blending(&self) -> bool {
        self.blending
    }

    /// Switches the blending of the ground on or off: off, every tile shows its own type.
    pub(crate) fn set_blending(&mut self, on: bool) {
        self.blending = on;
    }

    /// Whether the way from tile `t` across the edge in map direction `d` is open for blending.
    pub(super) fn open(&self, scene: &Scene, t: usize, d: usize) -> Option<usize> {
        let n = scene.neighbour(t, d)?;
        (scene.edge_state(t, d) < 2
            && scene.edge_object(t, d).is_none()
            && scene.edge_object(n, (d + 4) % 8).is_none())
        .then_some(n)
    }

    /// The terrain type seen in map direction `d` from tile `t`, or the tile's own.
    pub(super) fn seen(&self, scene: &Scene, t: usize, d: usize) -> u8 {
        let own = self.kind(scene, t);
        if !self.blends(own) {
            return own;
        }
        let neighbour = if d.is_multiple_of(2) {
            self.open(scene, t, d)
        } else {
            let (a, b) = ((d + 7) % 8, (d + 1) % 8);
            let target = scene.neighbour(t, d);
            // The way through the edge neighbour in direction `first`, then across `second`.
            let through = |first: usize, second: usize| {
                let e = self.open(scene, t, first)?;
                let n = scene.neighbour(e, second)?;
                (self.blends(self.kind(scene, e))
                    && scene.edge_state(e, second) < 2
                    && scene.edge_object(e, second).is_none()
                    && scene.edge_object(n, (second + 4) % 8).is_none())
                .then_some(n)
            };
            target.filter(|_| through(a, b).is_some() || through(b, a).is_some())
        };
        match neighbour {
            Some(n) if self.blends(self.kind(scene, n)) => self.kind(scene, n),
            _ => own,
        }
    }

    /// The ground picture of a tile: the cache's, or else one rendered at run time.
    pub(super) fn ground(&mut self, scene: &Scene, t: usize) -> Option<Pic> {
        let r = scene.rotation;
        let tile = scene.tiles[t];
        let (tx, ty) = ((t % scene.width) as i32, (t / scene.width) as i32);
        let kind = self.kind(scene, t);
        let (a, b) = [(0, 1), (1, 0), (0, -1), (-1, 0)][usize::from(r)];
        let umod3 = |k: i32| (k as u32) % 3;
        let key = Key {
            terrain: kind,
            shape: tile.shape.rotate_right(2 * u32::from(r)),
            variation: (umod3(tx * b - ty * a) * 3 + umod3(tx * a + ty * b)) as u8,
            neighbours: std::array::from_fn(|k| self.seen(scene, t, map_direction(r, k))),
        };
        let member = key.member();
        let stem = member.strip_suffix(".lle").unwrap_or(&member);
        let palette = key.palette();
        let palette = palette.strip_suffix(".pal").unwrap_or(&palette);
        if let Some(flat) = self.flat(stem, palette) {
            return Some(Pic::Flat(flat));
        }
        if !self.rendered.contains_key(&member) {
            let picture = self
                .ground
                .as_ref()
                .and_then(|g| g.render(&key).ok())
                .map(Rc::new);
            self.rendered.insert(member.clone(), picture);
        }
        self.rendered[&member].clone().map(Pic::Rendered)
    }

    /// Places the pictures of one tile: cliffs, ground and grid lines.
    pub(super) fn terrain(
        &mut self,
        scene: &Scene,
        marks: &HashMap<usize, [u8; 3]>,
        t: usize,
        summary: &mut Summary,
        out: &mut Vec<Placed>,
    ) {
        let r = scene.rotation;
        let tile = scene.tiles[t];
        let (tx, ty) = ((t % scene.width) as i32, (t / scene.width) as i32);
        let corner = corner_view(&tile, tx, ty, r);
        let (ox, oy) = origin(&tile, tx, ty, r);
        let piece_at = |offset: (i32, i32)| Piece { corner, offset };
        let shape = tile.shape.rotate_right(2 * u32::from(r));
        let (left, top) = (i32::from(shape & 3), i32::from(shape >> 6));
        let mut forced = [false; 2];
        for (e, forced) in forced.iter_mut().enumerate() {
            let d = map_direction(r, e * 2);
            let Some(n) = scene.neighbour(t, d) else {
                continue;
            };
            if scene.edge_state(t, d) < 2 {
                continue;
            }
            // The corners of the edge: left and top, or top and right.
            let corner = |s: usize| map_direction(r, s);
            let (ca, cb) = match e {
                0 => (corner(7), corner(1)),
                _ => (corner(1), corner(3)),
            };
            let nt = scene.tiles[n];
            let (nx, ny) = ((n % scene.width) as i32, (n / scene.width) as i32);
            let elevation = |c: usize| {
                let (cx, cy) = corner_xy(c);
                let own = corner_elevation(&tile, c);
                let (lx, ly) = (tx + cx - nx, ty + cy - ny);
                (own, corner_elevation(&nt, corner_at(lx, ly)))
            };
            let ((own_a, nb_a), (own_b, nb_b)) = (elevation(ca), elevation(cb));
            let gap = nb_a.min(nb_b) - own_a.max(own_b);
            let slope = own_a - own_b;
            let nslope = nb_a - nb_b;
            *forced = gap < 0 || slope < 0;
            if gap < 0 {
                continue;
            }
            let (x, mut y) = match e {
                0 => (16, 32),
                _ => (48, 32 + 16 * (left - top)),
            };
            // The pieces of the upper-left edge are numbered 11 to 15, those of the other 1 to 5.
            let base = if e == 0 { 10 } else { 0 };
            let mut pieces = Vec::new();
            if slope != 0 {
                pieces.push((base + if slope > 0 { 5 } else { 4 }, y));
                if slope < 0 {
                    y -= 16;
                }
            }
            for _ in 0..gap.min(2 * order::MAX_ELEVATION + 4) {
                pieces.push((base + 3, y));
                y -= 16;
            }
            if nslope != 0 {
                pieces.push((base + if nslope < 0 { 2 } else { 1 }, y));
            }
            for (piece, y) in pieces {
                match self
                    .fringe
                    .get(&piece)
                    .cloned()
                    .and_then(|s| self.piece(&s))
                {
                    Some(flat) => {
                        summary.cliffs_drawn += 1;
                        out.push(Placed {
                            x: ox + x,
                            y: oy + y,
                            piece: Some(piece_at((x, y))),
                            grid: false,
                            lift: 0,
                            pic: Pic::Flat(flat),
                        });
                    }
                    None => summary.cliffs_missing += 1,
                }
            }
        }
        match self.ground(scene, t) {
            Some(pic) => {
                match pic {
                    Pic::Rendered(_) => summary.tiles_rendered += 1,
                    _ => summary.tiles_cached += 1,
                }
                out.push(Placed {
                    x: ox + 32,
                    y: oy + 32,
                    piece: Some(piece_at((32, 32))),
                    grid: false,
                    lift: 0,
                    pic,
                });
            }
            None => summary.ground_missing += 1,
        }
        // The grid lines a cliff forces are always drawn; the others only with the grid on.
        let (north, east, highlight) = self
            .grid
            .get(&u32::from(shape))
            .cloned()
            .unwrap_or_default();
        for (e, (forced, name)) in [(forced[0], north), (forced[1], east)]
            .into_iter()
            .enumerate()
        {
            let behind = scene
                .neighbour(t, map_direction(r, e * 2))
                .is_some_and(|n| marks.contains_key(&n));
            if let Some(name) = name.filter(|_| !behind)
                && let Some(flat) = self.piece(&name)
            {
                out.push(Placed {
                    x: ox + 32,
                    y: oy + 32,
                    piece: Some(piece_at((32, 32))),
                    grid: !forced,
                    lift: 0,
                    pic: Pic::Flat(flat),
                });
            }
        }
        if let Some(color) = marks.get(&t)
            && let Some(name) = highlight
            && let Some(flat) = self.piece(&name)
        {
            let palette = flat.1.solid(*color);
            out.push(Placed {
                x: ox + 32,
                y: oy + 32,
                piece: Some(piece_at((32, 32))),
                grid: false,
                lift: 0,
                pic: Pic::Mark(flat, palette),
            });
        }
    }
}
