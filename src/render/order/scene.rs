//! The scene's construction and queries: placing objects on the tiles, moving units, tanks.

use super::*;

impl Scene {
    /// Places the objects on the tiles. `tiles` has `width * height` entries; the elevations
    /// are cut to [`MAX_ELEVATION`].
    pub(crate) fn new(
        width: usize,
        height: usize,
        rotation: u8,
        tiles: &[Tile],
        objects: Vec<Object>,
    ) -> Scene {
        assert_eq!(tiles.len(), width * height);
        let n = tiles.len();
        let mut scene = Scene {
            width,
            height,
            rotation: rotation % 4,
            tiles: tiles
                .iter()
                .map(|t| Tile {
                    elevation: t.elevation.clamp(-MAX_ELEVATION, MAX_ELEVATION),
                    ..*t
                })
                .collect(),
            edge: vec![[None; 4]; n],
            quad: vec![[None; 4]; n],
            list: vec![Vec::new(); n],
            overlay: vec![Vec::new(); n],
            flights: Vec::new(),
            transient: vec![Vec::new(); n],
            covers: vec![0; objects.len()],
            path: vec![None; n],
            big: vec![None; n],
            placed: vec![false; objects.len()],
            tanks: Vec::new(),
            tank: vec![None; n],
            scum: Vec::new(),
            face_scum: Vec::new(),
            objects,
        };
        for o in 0..scene.objects.len() {
            scene.place(o);
        }
        scene
    }

    /// Adds an object after the others and places it: the scene [`Scene::new`] makes of the longer
    /// list, without building it again.
    pub(crate) fn push(&mut self, object: Object) {
        self.objects.push(object);
        self.covers.push(0);
        self.placed.push(false);
        self.place(self.objects.len() - 1);
    }

    /// The tile index under a world point, if it is on the map.
    pub(crate) fn tile_at(&self, x: i32, y: i32) -> Option<usize> {
        let (tx, ty) = (x.div_euclid(64), y.div_euclid(64));
        self.index(tx, ty)
    }

    /// The height a shadow at a world point is placed at: the ground of its tile (16 times the
    /// base elevation plus the slope), never the water level, so over a tank it is the floor
    /// (ambients 5.3 point 4).
    pub(crate) fn shadow_height(&self, x: i32, y: i32) -> Option<i32> {
        let t = self.tile_at(x, y)?;
        Some(ground_height(
            &self.tiles[t],
            x - 64 * (t % self.width) as i32,
            y - 64 * (t / self.width) as i32,
        ))
    }

    pub(crate) fn index(&self, tx: i32, ty: i32) -> Option<usize> {
        (tx >= 0 && ty >= 0 && (tx as usize) < self.width && (ty as usize) < self.height)
            .then(|| ty as usize * self.width + tx as usize)
    }

    /// Whether the object stands on the map (its position is over a tile).
    pub(crate) fn is_placed(&self, o: usize) -> bool {
        self.placed[o]
    }

    /// Adds an object to the map as the original's loader does. One that finds no place is
    /// dropped: it is not placed and never painted.
    fn place(&mut self, o: usize) {
        let obj = self.objects[o];
        let Some(t) = self.tile_at(obj.x, obj.y) else {
            return;
        };
        self.placed[o] = match obj.kind {
            Kind::Path if self.path[t].is_none() => {
                self.path[t] = Some(o);
                true
            }
            Kind::Edge | Kind::Wall
                if obj.facing.is_multiple_of(2)
                    && self.edge[t][obj.facing as usize / 2].is_none() =>
            {
                self.edge[t][obj.facing as usize / 2] = Some(o);
                true
            }
            Kind::Fixed => self.place_fixed(o),
            Kind::Unit => {
                self.list[t].push(o);
                true
            }
            Kind::Overlay => {
                self.overlay[t].push(o);
                true
            }
            Kind::Flight => {
                self.flights.push(o);
                true
            }
            Kind::Sparkle => {
                self.transient[t].push(o);
                true
            }
            Kind::Path | Kind::Edge | Kind::Wall => false,
        };
    }

    /// The tiles under an object bigger than one tile in x or in y (a footprint above 2 half
    /// tiles after the turn), in index order; none for another object (terrain editing 5.5). They
    /// are the tiles that contain the points `(x - 16 fx + 64 a, y - 16 fy + 64 b)` for `0 <= a <
    /// ceil(fx / 2)` and `0 <= b < ceil(fy / 2)`, a point's tile being its coordinates divided by
    /// 64 and cut towards zero; points off the map are left out. Not the quadrants of the object.
    pub(crate) fn large_tiles(&self, o: usize) -> Vec<usize> {
        let obj = &self.objects[o];
        self.tiles_under(obj.x, obj.y, self.turned(o))
    }

    /// The footprint of object `o` in half tiles after its turn.
    fn turned(&self, o: usize) -> (i32, i32) {
        let obj = &self.objects[o];
        match obj.facing.is_multiple_of(4) {
            true => obj.footprint,
            false => (obj.footprint.1, obj.footprint.0),
        }
    }

    /// The tiles of [`Scene::large_tiles`] for something at `(x, y)` whose footprint, already
    /// turned with its facing, is `(fx, fy)`.
    pub(crate) fn tiles_under(&self, x: i32, y: i32, (fx, fy): (i32, i32)) -> Vec<usize> {
        match fx <= 2 && fy <= 2 {
            true => Vec::new(),
            false => self.tiles_of(x, y, (fx, fy)),
        }
    }

    /// The tiles under object `o`, whatever its size: the points of [`Scene::large_tiles`].
    pub(crate) fn footprint_tiles(&self, o: usize) -> Vec<usize> {
        let obj = &self.objects[o];
        self.tiles_of(obj.x, obj.y, self.turned(o))
    }

    fn tiles_of(&self, x: i32, y: i32, (fx, fy): (i32, i32)) -> Vec<usize> {
        let at = |v: i32, f: i32, k: i32| {
            i32::try_from((i64::from(v) - 16 * i64::from(f) + 64 * i64::from(k)) / 64).ok()
        };
        let mut tiles: Vec<usize> = (0..(fy + 1) / 2)
            .flat_map(|b| (0..(fx + 1) / 2).map(move |a| (a, b)))
            .filter_map(|(a, b)| self.index(at(x, fx, a)?, at(y, fy, b)?))
            .collect();
        tiles.sort_unstable();
        tiles.dedup();
        tiles
    }

    /// The units standing on tile `t`.
    pub(crate) fn units_on(&self, t: usize) -> &[usize] {
        &self.list[t]
    }

    /// Puts the unit object `o` at a new point and facing; its tile list follows.
    pub(crate) fn move_unit(&mut self, o: usize, (x, y, z): (i32, i32, i32), facing: u32) {
        let (from, to) = (
            self.tile_at(self.objects[o].x, self.objects[o].y),
            self.tile_at(x, y),
        );
        let obj = &mut self.objects[o];
        (obj.x, obj.y, obj.z, obj.facing) = (x, y, z, facing);
        if obj.kind == Kind::Unit {
            // Off every list and not painted (lifted): back on at once. Painted but off its list
            // (see `unlist_unit`): back on at the first change of tile.
            let listed = |l: &[Vec<usize>]| from.is_some_and(|f| l[f].contains(&o));
            let relist = from != to || !self.placed[o] && !listed(&self.list);
            if relist {
                if let Some(f) = from {
                    self.list[f].retain(|&u| u != o);
                }
                if let Some(t) = to {
                    self.list[t].push(o);
                }
            }
            if relist || !self.placed[o] {
                self.placed[o] = to.is_some();
            }
        }
    }

    /// Takes unit object `o` off the map: it is on no tile list and is not painted, until
    /// [`Scene::move_unit`] puts it down.
    pub(crate) fn lift_unit(&mut self, o: usize) {
        if let Some(t) = self.tile_at(self.objects[o].x, self.objects[o].y) {
            self.list[t].retain(|&u| u != o);
        }
        self.placed[o] = false;
    }

    /// Stops painting unit object `o`, which stays on its tile's list (a user hidden in a
    /// building); [`Scene::move_unit`] paints it again.
    pub(crate) fn hide_unit(&mut self, o: usize) {
        self.placed[o] = false;
    }

    /// Takes unit object `o` off its tile's list only: it stays painted if it was, and goes back on
    /// a list at its first change of tile ([`Scene::move_unit`]).
    pub(crate) fn unlist_unit(&mut self, o: usize) {
        if let Some(t) = self.tile_at(self.objects[o].x, self.objects[o].y) {
            self.list[t].retain(|&u| u != o);
        }
    }

    /// The unit objects held off the map, with how (see [`Held`]).
    pub(crate) fn held(&self) -> impl Iterator<Item = (usize, Held)> + '_ {
        (0..self.objects.len()).filter_map(|o| {
            let obj = &self.objects[o];
            if obj.kind != Kind::Unit {
                return None;
            }
            let listed = self
                .tile_at(obj.x, obj.y)
                .is_some_and(|t| self.list[t].contains(&o));
            match (self.placed[o], listed) {
                (true, true) => None,
                (true, false) => Some((o, Held::Unlisted)),
                (false, true) => Some((o, Held::Hidden)),
                (false, false) => Some((o, Held::Lifted)),
            }
        })
    }

    /// Paints unit object `o` where it is without putting it on a tile list.
    pub(crate) fn paint_unit(&mut self, o: usize) {
        self.placed[o] = true;
    }

    /// Takes the quadrants of the footprint, all or none: it wants the `fx * fy` quadrants
    /// that contain the points `(x - 16 (fx - 1) + 32 i, y - 16 (fy - 1) + 32 j)`. Returns
    /// whether it got them.
    fn place_fixed(&mut self, o: usize) -> bool {
        let obj = self.objects[o];
        let (fx, fy) = obj.footprint;
        // The footprint is given along the object's own axes; it turns with the facing.
        let (fx, fy) = if !obj.facing.is_multiple_of(4) {
            (fy, fx)
        } else {
            (fx, fy)
        };
        if !(1..=MAX_FOOTPRINT).contains(&fx) || !(1..=MAX_FOOTPRINT).contains(&fy) {
            return false;
        }
        let mut wanted = Vec::with_capacity((fx * fy) as usize);
        for j in 0..fy {
            for i in 0..fx {
                let px = i64::from(obj.x) - 16 * i64::from(fx - 1) + 32 * i64::from(i);
                let py = i64::from(obj.y) - 16 * i64::from(fy - 1) + 32 * i64::from(j);
                let (cx, cy) = (px.div_euclid(32), py.div_euclid(32));
                let tile = i32::try_from(cx >> 1)
                    .ok()
                    .zip(i32::try_from(cy >> 1).ok())
                    .and_then(|(tx, ty)| self.index(tx, ty));
                match tile {
                    Some(tile) if self.quad[tile][((cy & 1) * 2 + (cx & 1)) as usize].is_none() => {
                        wanted.push((tile, ((cy & 1) * 2 + (cx & 1)) as usize))
                    }
                    _ => return false,
                }
            }
        }
        let large = fx > 2 || fy > 2;
        for &(tile, q) in &wanted {
            self.quad[tile][q] = Some(o);
            if q == 3 && large {
                self.big[tile] = Some(o);
            }
        }
        self.covers[o] = (fx * fy) as u32;
        true
    }

    /// The state of the edge of tile `t` in map direction `d` (even): 0 and 1 mean the ground
    /// is joined, 2 and 3 a step. The file stores the four states in the order of the
    /// directions 6, 4, 2, 0 from the low bits up.
    pub(crate) fn edge_state(&self, t: usize, d: usize) -> u8 {
        (self.tiles[t].edges >> (2 * (3 - d / 2))) & 3
    }

    /// Whether the four quadrant slots of tile `t` are empty (an edge piece, a path piece or a unit
    /// does not count).
    pub(crate) fn is_empty(&self, t: usize) -> bool {
        self.quad[t].iter().all(Option::is_none)
    }

    /// The object on the edge of tile `t` in map direction `d`.
    pub(crate) fn edge_object(&self, t: usize, d: usize) -> Option<usize> {
        self.edge[t][d / 2]
    }

    /// The number of quadrants the object holds.
    #[cfg(test)]
    pub(crate) fn quadrants(&self, o: usize) -> u32 {
        self.covers[o]
    }

    /// The path piece of a tile.
    pub(crate) fn path_of(&self, t: usize) -> Option<usize> {
        self.path[t]
    }

    /// The object that holds quadrant `q` (`qy * 2 + qx`) of tile `t`.
    pub(crate) fn quadrant_object(&self, t: usize, q: usize) -> Option<usize> {
        self.quad[t][q]
    }

    /// Adds a transient entity on tile `t` of a tank (map drawing 9.7.4); returns its index.
    pub(crate) fn add_transient(&mut self, t: usize, obj: Object) -> usize {
        self.objects.push(obj);
        self.covers.push(0);
        self.placed.push(true);
        self.transient[t].push(self.objects.len() - 1);
        self.objects.len() - 1
    }

    /// Declares the tank exhibits. `fences[o]` tells whether object `o` is a habitat fence (a
    /// fence or tank wall whose type has a `[HabitatFence]` or `[TankFence]` section). The area
    /// of a tank is what the original fills from the first tile of its record: every tile
    /// reachable through edges where neither slot holds a habitat fence, whatever the heights. A
    /// tile that is already in another exhibit is taken over.
    pub(crate) fn set_tanks(&mut self, tanks: &[TankSpec], fences: &[bool]) {
        self.tanks = tanks.to_vec();
        self.tank = vec![None; self.tiles.len()];
        let fence = |t: usize, d: usize| {
            self.edge[t][d / 2].is_some_and(|o| fences.get(o).copied().unwrap_or(false))
        };
        for (k, spec) in tanks.iter().enumerate() {
            let Some(first) = self.index(spec.tile.0, spec.tile.1) else {
                continue;
            };
            let mut seen = vec![false; self.tiles.len()];
            seen[first] = true;
            let mut todo = vec![first];
            while let Some(t) = todo.pop() {
                self.tank[t] = Some(k);
                for d in [0, 2, 4, 6] {
                    let Some(n) = self.neighbour(t, d) else {
                        continue;
                    };
                    if !seen[n] && !fence(t, d) && !fence(n, (d + 4) % 8) {
                        seen[n] = true;
                        todo.push(n);
                    }
                }
            }
        }
        // The pseudo-random bytes of the water: one per tile, then one per face level.
        let mut x = 50u32;
        let mut next = move || {
            x = x.wrapping_mul(214013).wrapping_add(2531011);
            ((x >> 16) & 0x7fff) as u8 % 255
        };
        self.scum = (0..self.tiles.len()).map(|_| next()).collect();
        self.face_scum = (0..255).map(|_| next()).collect();
    }

    /// The tank exhibit (an index in [`Scene::tanks`]) that tile `t` belongs to.
    pub(crate) fn tank_of(&self, t: usize) -> Option<usize> {
        self.tank[t]
    }

    /// The tank an active wall belongs to: a tank wall of a tank taller than 0, standing on a
    /// tile of the tank or on the edge of one.
    pub(crate) fn wall_tank(&self, o: usize) -> Option<usize> {
        let obj = &self.objects[o];
        if obj.kind != Kind::Wall || !self.placed[o] {
            return None;
        }
        let t = self.tile_at(obj.x, obj.y)?;
        let across = self.neighbour(t, obj.facing as usize % 8);
        self.tank_of(t)
            .or_else(|| across.and_then(|n| self.tank_of(n)))
            .filter(|&k| self.tanks[k].height > 0)
    }

    /// The height of the water surface over tile `t`: the ground at its centre plus the level.
    pub(crate) fn surface(&self, t: usize) -> Option<i32> {
        let tank = &self.tanks[self.tank_of(t)?];
        Some(ground_height(&self.tiles[t], 32, 32) + 16 * tank.level)
    }

    /// The tile across the edge or corner in map direction `d`.
    pub(crate) fn neighbour(&self, t: usize, d: usize) -> Option<usize> {
        let (dx, dy) = STEP[d % 8];
        self.index((t % self.width) as i32 + dx, (t / self.width) as i32 + dy)
    }
}

