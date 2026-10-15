//! The map as the tools change it: the zoo with the flag "in the zoo" of every tile, an index of
//! who stands where, the edit primitives, the undo list of the last operation and a dirty mark.
//!
//! The zoo stays the live world: an edit changes `zoo.tiles` and `zoo.entities` in place and
//! nothing is written to disk. After every edit the index is built again from the entities with
//! the placement rules of the paint order ([`Scene`]), so a query never scans the entity list.
//! An entity is named by its position in `zoo.entities`, which an edit may change (use the id,
//! `Common::id`, to keep one). The game's rules (what may be placed where, what it costs) are not
//! here.

use crate::render::corner_elevation;
use crate::render::order::{Kind, Object, Scene};
use crate::zoo::{Entity, Tile, Zoo};

/// How an entity takes its place on the map, from its type.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Shape {
    pub kind: Kind,
    /// `cFootprintX/Y` in half tiles along the entity's own axes.
    pub footprint: (i32, i32),
    /// A fence or tank wall that closes exhibits (the type has a habitat or tank fence section).
    pub fence: bool,
    /// A zoo wall (the `[Member]` list of the type holds `zoowall`): it stops the exterior fill.
    pub wall: bool,
    /// A zoo gate (the `[Member]` list holds `zoogate`): the exterior fill never enters it.
    pub gate: bool,
}

/// What an operation did, to undo it.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Action {
    /// An entity was added: its id.
    Placed(u32),
    /// An entity was removed from this position.
    Removed(usize, Entity, Option<Shape>),
    /// A tile had this state: its index.
    Tile(usize, Tile),
}

pub struct World {
    pub zoo: Zoo,
    /// The shape of every entity of `zoo.entities`; `None` for one that has no body or no type.
    shapes: Vec<Option<Shape>>,
    scene: Scene,
    /// The entity (position in `zoo.entities`) of every object of `scene`.
    owner: Vec<usize>,
    /// Per tile, whether it is inside the zoo wall.
    in_zoo: Vec<bool>,
    undo: Vec<Action>,
    /// The next action starts a new undo list.
    fresh: bool,
    dirty: bool,
}

impl World {
    /// The world of a zoo, with the exterior of the zoo marked (exhibits and money 3.1). `shape`
    /// tells how each entity takes its place.
    pub fn new(zoo: Zoo, shape: impl Fn(&Entity) -> Option<Shape>) -> World {
        let shapes: Vec<_> = zoo.entities.iter().map(shape).collect();
        let (scene, owner) = index(&zoo, &shapes);
        let mut world = World {
            in_zoo: vec![true; zoo.tiles.len()],
            zoo,
            shapes,
            scene,
            owner,
            undo: Vec::new(),
            fresh: false,
            dirty: false,
        };
        world.mark_exterior();
        world
    }

    /// The index of tile (`tx`, `ty`), if it is on the map.
    pub fn tile_index(&self, tx: i32, ty: i32) -> Option<usize> {
        self.scene.index(tx, ty)
    }

    /// Whether tile `t` is inside the zoo wall.
    pub fn in_zoo(&self, t: usize) -> bool {
        self.in_zoo.get(t).copied().unwrap_or(false)
    }

    /// The entity of the path piece of tile `t`.
    pub fn path(&self, t: usize) -> Option<usize> {
        self.scene.path_of(t).map(|o| self.owner[o])
    }

    /// The entity that holds quadrant `q` (`qy * 2 + qx`, 0 to 3) of tile `t`.
    pub fn quadrant(&self, t: usize, q: usize) -> Option<usize> {
        self.scene.quadrant_object(t, q).map(|o| self.owner[o])
    }

    /// The fence on the edge of tile `t` in map direction `d` (0, 2, 4 or 6).
    pub fn edge(&self, t: usize, d: usize) -> Option<usize> {
        self.scene.edge_object(t, d).map(|o| self.owner[o])
    }

    /// The position of the entity with this id.
    pub fn find(&self, id: u32) -> Option<usize> {
        self.zoo
            .entities
            .iter()
            .position(|e| e.common.as_ref().is_some_and(|c| c.id == id))
    }

    /// The next press starts a new undo list: the first operation after it empties the old one.
    pub fn begin(&mut self) {
        self.fresh = true;
    }

    /// Adds an entity with a fresh id and returns the id; `None` for one without a body.
    pub fn add(&mut self, mut entity: Entity, shape: Option<Shape>) -> Option<u32> {
        let common = entity.common.as_mut()?;
        let id = self.zoo.id_counter.checked_add(1)?;
        (self.zoo.id_counter, common.id) = (id, id);
        self.zoo.entities.push(entity);
        self.shapes.push(shape);
        self.record(Action::Placed(id));
        self.changed();
        Some(id)
    }

    /// Removes the entity with this id; whether there was one.
    pub fn remove(&mut self, id: u32) -> bool {
        let Some(at) = self.find(id) else {
            return false;
        };
        let entity = self.zoo.entities.remove(at);
        let shape = self.shapes.remove(at);
        self.record(Action::Removed(at, entity, shape));
        self.changed();
        true
    }

    /// Sets the elevation, shape and terrain type of tile (`tx`, `ty`) and writes the edge states
    /// of it and of its four neighbours again (map drawing 3.5); whether the tile is on the map.
    pub fn set_tile(&mut self, tx: i32, ty: i32, elevation: i32, shape: u8, terrain: u8) -> bool {
        let Some(t) = self.tile_index(tx, ty) else {
            return false;
        };
        let old = self.zoo.tiles[t];
        self.record(Action::Tile(t, old));
        let tile = &mut self.zoo.tiles[t];
        (tile.elevation, tile.shape, tile.terrain) = (elevation, shape, terrain);
        self.edges_around(t);
        self.changed();
        true
    }

    /// Takes back the operations of the last press, last first; whether there were any.
    pub fn undo(&mut self) -> bool {
        let actions = std::mem::take(&mut self.undo);
        for action in actions.iter().rev() {
            match action.clone() {
                Action::Placed(id) => {
                    if let Some(at) = self.find(id) {
                        self.zoo.entities.remove(at);
                        self.shapes.remove(at);
                    }
                }
                Action::Removed(at, entity, shape) => {
                    let at = at.min(self.zoo.entities.len());
                    self.zoo.entities.insert(at, entity);
                    self.shapes.insert(at, shape);
                }
                Action::Tile(t, tile) => {
                    self.zoo.tiles[t] = tile;
                    self.edges_around(t);
                }
            }
        }
        self.fresh = false;
        self.changed();
        !actions.is_empty()
    }

    /// Whether anything changed since the last call, and clears the mark: the picture is due.
    pub fn take_dirty(&mut self) -> bool {
        std::mem::take(&mut self.dirty)
    }

    fn record(&mut self, action: Action) {
        if std::mem::take(&mut self.fresh) {
            self.undo.clear();
        }
        self.undo.push(action);
    }

    fn changed(&mut self) {
        (self.scene, self.owner) = index(&self.zoo, &self.shapes);
        self.dirty = true;
    }

    /// Writes the edge states of tile `t` and of the tiles beside it again.
    fn edges_around(&mut self, t: usize) {
        let (tx, ty) = ((t % self.scene.width) as i32, (t / self.scene.width) as i32);
        for (dx, dy) in [(0, 0), (0, -1), (1, 0), (0, 1), (-1, 0)] {
            if let Some(n) = self.tile_index(tx + dx, ty + dy) {
                self.zoo.tiles[n].edges = edge_states(&self.zoo, n);
            }
        }
    }

    /// Marks the tiles outside the zoo wall (exhibits and money 3.1). The entrance tile's quadrant
    /// 3 (local 32.., 32..) must hold the entrance building; the program walks from the entrance
    /// in the building's facing direction while the tile reached holds it in that quadrant, and
    /// fills from the first tile that does not. The fill never enters a tile whose quadrant 3
    /// holds a zoo gate and never crosses a zoo wall.
    fn mark_exterior(&mut self) {
        let (ex, ey) = self.zoo.entrance;
        let Some(mut t) = i32::try_from(ex)
            .ok()
            .zip(i32::try_from(ey).ok())
            .and_then(|(x, y)| self.tile_index(x, y))
        else {
            return;
        };
        let Some(building) = self.scene.quadrant_object(t, 3) else {
            return;
        };
        let facing = self.scene.objects[building].facing as usize;
        while self.scene.quadrant_object(t, 3) == Some(building) {
            match self.scene.neighbour(t, facing) {
                Some(next) => t = next,
                None => return,
            }
        }
        let shape = |w: &World, o: usize| w.shapes[w.owner[o]];
        let wall = |w: &World, t: usize, d: usize| {
            w.scene
                .edge_object(t, d)
                .is_some_and(|o| shape(w, o).is_some_and(|s| s.wall))
        };
        let mut todo = vec![t];
        self.in_zoo[t] = false;
        while let Some(t) = todo.pop() {
            for d in [0, 2, 4, 6] {
                let Some(n) = self.scene.neighbour(t, d) else {
                    continue;
                };
                let gate = self
                    .scene
                    .quadrant_object(n, 3)
                    .is_some_and(|o| shape(self, o).is_some_and(|s| s.gate));
                if self.in_zoo[n] && !gate && !wall(self, t, d) && !wall(self, n, (d + 4) % 8) {
                    self.in_zoo[n] = false;
                    todo.push(n);
                }
            }
        }
    }
}

/// The index of the entities of a zoo: the paint order's scene of the ones that have a shape, and
/// the entity of each of its objects.
fn index(zoo: &Zoo, shapes: &[Option<Shape>]) -> (Scene, Vec<usize>) {
    let (mut objects, mut owner) = (Vec::new(), Vec::new());
    for (at, (entity, shape)) in zoo.entities.iter().zip(shapes).enumerate() {
        if let (Some(c), Some(s)) = (&entity.common, shape) {
            objects.push(Object {
                kind: s.kind,
                x: c.x,
                y: c.y,
                z: c.z,
                facing: c.rotation % 8,
                footprint: s.footprint,
                late: false,
                height: 0,
            });
            owner.push(at);
        }
    }
    let scene = Scene::new(
        zoo.width as usize,
        zoo.height as usize,
        0,
        &zoo.tiles,
        objects,
    );
    (scene, owner)
}

/// The four edge states of tile `t` as the file packs them, computed with map drawing 3.5.
fn edge_states(zoo: &Zoo, t: usize) -> u8 {
    let width = zoo.width as usize;
    let (tx, ty) = ((t % width) as i32, (t / width) as i32);
    let mut edges = 0;
    // For the edges 0, 2, 4, 6: the step to the neighbour and the two corners on the edge, of
    // the tile and of the neighbour, in the order of increasing coordinate.
    for (d, (dx, dy), own, other) in [
        (0, (0, -1), (7, 1), (5, 3)),
        (2, (1, 0), (1, 3), (7, 5)),
        (4, (0, 1), (5, 3), (7, 1)),
        (6, (-1, 0), (7, 5), (1, 3)),
    ] {
        let beside = u32::try_from(tx + dx)
            .ok()
            .zip(u32::try_from(ty + dy).ok())
            .and_then(|(x, y)| zoo.tile(x, y));
        let state = match beside {
            Some(n) => {
                let tile = &zoo.tiles[t];
                let (t0, t1) = (corner_elevation(tile, own.0), corner_elevation(tile, own.1));
                let (n0, n1) = (corner_elevation(n, other.0), corner_elevation(n, other.1));
                if t1 - t0 == n1 - n0 {
                    match (t0 == n0, t1 == t0) {
                        (true, true) => 0,
                        (true, false) => 1,
                        (false, _) if n0 > t0 => 2,
                        _ => 3,
                    }
                // The slopes differ: the edges 2 and 4 take the state 2 also when they meet at
                // the first corner, as the tile later in row order sees them (3.5).
                } else if t0 < n0 || (t0 == n0 && (d == 2 || d == 4)) {
                    2
                } else {
                    3
                }
            }
            _ => 0,
        };
        edges |= state << (2 * (3 - d / 2));
    }
    edges
}

#[cfg(test)]
mod tests;
