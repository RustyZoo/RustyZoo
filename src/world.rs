//! The map as the tools change it: the zoo with the flag "in the zoo" of every tile, an index of
//! who stands where, the edit primitives, the undo list of the last operation and a dirty mark.
//!
//! The zoo stays the live world: an edit changes `zoo.tiles` and `zoo.entities` in place and
//! nothing is written to disk. After every edit the index is built again from the entities with
//! the placement rules of the paint order ([`Scene`]), so a query never scans the entity list.
//! An entity is named by its position in `zoo.entities`, which an edit may change (use the id,
//! `Common::id`, to keep one). The game's rules (what may be placed where, what it costs) are not
//! here.

use std::collections::BTreeMap;

use crate::entity::Catalogue;
use crate::exhibit::{EXTERIOR, Exhibits, GROUNDS, TankConfig};
use crate::path::{Finder, Settings};
use crate::render::corner_elevation;
use crate::render::order::{Held, Kind, Object, Scene, ground_height};
use crate::strings::Language;
use crate::unit::Units;
use crate::zoo::{Entity, Tile, Zoo};

mod amphibious;
mod exhibit;
mod filter;
mod save;
mod tank;
mod water;
mod wear;
pub use exhibit::Event;
pub use filter::{
    Loop as FilterLoop, Wear, months_since as filter_months_since, placed as placed_filter,
    sell_rate as filter_sell_rate, wear as filter_wear,
};
pub use wear::Decay;
pub(crate) use wear::piece_event;

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
    /// A fence type that has a gate of its own (a sibling type whose subtype is `g`).
    pub gated: bool,
    /// What the type says about tanks.
    pub tank: TankKind,
    /// `walkable` and `walkableByTall` (pathfinding 4.3): units walk through the object.
    pub walkable: (bool, bool),
}

/// What a type says about tanks (exhibits and money 3.9).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TankKind {
    /// A tank wall: the type's file has a `[TankFence]` section.
    pub wall: bool,
    /// A show wall: `cIsShowFence` is 1.
    pub show: bool,
    /// `cUnderwater` is 1 (a tank wall always counts), `cSurface` is 1, `cShow` is 1 (a show wall
    /// counts), `cIsTransient` is 1: what a tank allows inside it.
    pub underwater: bool,
    pub surface: bool,
    pub shows: bool,
    pub transient: bool,
    /// `cTall` is 1: the unit passes what is `walkableByTall` (pathfinding 4.3).
    pub tall: bool,
    /// `cLife` and `cStrength`: what a new piece of the type starts with.
    pub life: i32,
    pub strength: u32,
    /// `cDepth` and `cHeight`: the levels below and above its own that an object blocks in a
    /// tank (tank fauna 7.3).
    pub depth: i32,
    pub height: i32,
}

impl TankKind {
    /// A type that says nothing about tanks.
    pub const NONE: TankKind = TankKind {
        wall: false,
        show: false,
        underwater: false,
        surface: false,
        shows: false,
        transient: false,
        tall: false,
        life: 0,
        strength: 0,
        depth: 0,
        height: 0,
    };
}

/// A value of a tank that the removal of one of its walls records.
#[derive(Debug, Clone, Copy, PartialEq)]
enum TankValue {
    Purity(u32),
    Salt(bool),
    Filled(bool),
    Height(u32),
    /// The elevation of the tank's ground.
    Level(i32),
}

/// What an operation did, to undo it. The prices are what the action moved in the cash: an undo
/// gives them back (placing 8).
#[derive(Debug, Clone, PartialEq)]
enum Action {
    /// An entity was added: its id and the price paid for it.
    Placed { id: u32, price: f32 },
    /// A wall the wall step built for nothing: recorded as a placement, so the undo gives back
    /// the full price of its type (placing 8, kind 1).
    Copied { id: u32 },
    /// A value of the tank on this tile before one of its walls was removed (placing 8, kinds 4
    /// to 8).
    Tank { tile: usize, value: TankValue },
    /// An entity was removed from this position of the list, and the refund it gave.
    Removed {
        at: usize,
        entity: Entity,
        shape: Option<Shape>,
        price: f32,
    },
    /// The entity `id` took the place of `old` in the list (a path of another type, a fence piece
    /// laid on a held slot) at this price; `exhibit` is the exhibit that had `old` as its gate.
    Replaced {
        id: u32,
        old: Entity,
        shape: Option<Shape>,
        exhibit: Option<u32>,
        price: f32,
    },
    /// Tiles had this state (their index first), and the price of the change.
    Tiles {
        tiles: Vec<(usize, Tile)>,
        price: f32,
    },
    /// A fence piece stood here: its id, position `(x, y, z)` and rotation.
    Moved {
        id: u32,
        at: (i32, i32, i32),
        rotation: u32,
    },
    /// The exhibit on this tile had this name and this gate (the tile and side of its piece)
    /// before a fence cut it in two or opened it (placing 8, kind 11).
    Exhibit {
        tile: usize,
        name: String,
        gate: Option<(usize, usize)>,
    },
}

/// Where an entity stands, as the log tells it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Spot {
    /// `class/type/subtype`.
    pub name: String,
    pub tile: (i32, i32),
    /// The side of a fence.
    pub side: Option<u32>,
    pub id: u32,
}

/// One action of the undo list that was taken back, with the price it moved.
#[derive(Debug, Clone, PartialEq)]
pub enum Step {
    /// A placed object went; its price comes back (the construction spending goes down).
    Removed(Spot, f32),
    /// A wall the wall step built went: the full price of its type comes back, the entity tells
    /// the type.
    Copied(Spot, Box<Entity>),
    /// A removed object came back; its refund is taken back.
    Back(Spot, f32),
    /// A piece got its old type back; the price of the change comes back.
    Retyped(Spot, f32),
    /// This many tiles got their old state back; the price of the change comes back.
    Tiles(usize, f32),
    /// A removed object that could not be put back stays sold with its refund (one of several).
    Skipped(Spot),
}

/// What an undo did.
#[derive(Debug, Default, Clone, PartialEq)]
pub struct Undone {
    /// The actions taken back, the last recorded first.
    pub steps: Vec<Step>,
    /// A removed object that cannot be put back because something stands in its place: the undo
    /// stopped there and the rest of the list is kept.
    pub blocked: Option<Entity>,
    /// The names an undo gave back to exhibits, for the name hook (cheats 2.2).
    pub names: Vec<String>,
}

impl Undone {
    /// Whether nothing was taken back.
    pub fn is_empty(&self) -> bool {
        self.steps.is_empty() && self.blocked.is_none() && self.names.is_empty()
    }
}

pub struct World {
    pub zoo: Zoo,
    /// The live state of the animals, guests and staff of `zoo.entities`; it follows the list.
    pub units: Units,
    /// The shape of every entity of `zoo.entities`; `None` for one that has no body or no type.
    shapes: Vec<Option<Shape>>,
    scene: Scene,
    /// The entity (position in `zoo.entities`) of every object of `scene`.
    owner: Vec<usize>,
    /// Per tile, whether it is inside the zoo wall.
    in_zoo: Vec<bool>,
    /// The regions the exhibit fences make, built again after a fence is placed or removed.
    exhibits: Exhibits,
    /// What happened to the exhibits and nobody has taken yet.
    events: Vec<Event>,
    /// The new exhibits that wait for their gate while the player names them: the exhibit, the
    /// pair the walk starts at and the tile the piece was placed on.
    waiting: Vec<(u32, (usize, usize), usize)>,
    /// The words before the number of a default exhibit name: exhibit, tank, show tank.
    words: [String; 3],
    /// Which terrain types are water (the gate walk's keeper does not swim).
    water: Vec<bool>,
    /// The numbers that make a new tank.
    tank_config: TankConfig,
    /// The ids of the wall pieces that link amphibious neighbours (tank fauna 12), sorted.
    connectors: Vec<u32>,
    /// The bounds of the interval a fence piece made healthy draws.
    decay: Decay,
    undo: Vec<Action>,
    /// The next action starts a new undo list.
    fresh: bool,
    dirty: bool,
    /// How many times the map changed; what is derived from it (the viewing areas) is built again
    /// when this moved.
    pub edits: u64,
    /// The game is paused: a fill or drain of a tank is instant.
    paused: bool,
    /// The level and purity of each tank the last `world water` line told (not saved, not hashed).
    told_water: BTreeMap<u32, (u32, u32)>,
    /// What each tank filter shows besides its record: switched off, or showing bubbles (not
    /// saved, not hashed).
    filter_view: BTreeMap<u32, filter::View>,
    /// The install language: the exhibit names of a loaded zoo are kept when its stored locale is
    /// this one, and a save writes it.
    language: Language,
    /// What the completed research programs did to the entity types of this game (research 6.4).
    pub research: crate::research::Overlay,
}

impl World {
    /// The world of a test zoo in the shipped language (see [`World::with_language`]).
    #[cfg(test)]
    pub fn new(zoo: Zoo, shape: impl Fn(&Entity) -> Option<Shape>) -> World {
        World::with_language(zoo, shape, Language::shipped())
    }

    /// The world of a zoo in the install language `language`, with the exterior of the zoo marked
    /// (exhibits and money 3.1). `shape` tells how each entity takes its place.
    pub fn with_language(
        zoo: Zoo,
        shape: impl Fn(&Entity) -> Option<Shape>,
        language: Language,
    ) -> World {
        let shapes: Vec<_> = zoo.entities.iter().map(shape).collect();
        let (scene, owner) = index(&zoo, &shapes);
        let mut world = World {
            in_zoo: vec![true; zoo.tiles.len()],
            exhibits: Exhibits::default(),
            events: Vec::new(),
            waiting: Vec::new(),
            water: Vec::new(),
            tank_config: TankConfig::default(),
            connectors: Vec::new(),
            decay: Decay::default(),
            zoo,
            units: Units::default(),
            shapes,
            scene,
            owner,
            undo: Vec::new(),
            fresh: false,
            dirty: false,
            edits: 0,
            paused: false,
            told_water: BTreeMap::new(),
            filter_view: BTreeMap::new(),
            words: language.exhibit_words.clone(),
            language,
            research: Default::default(),
        };
        world.mark_exterior();
        world.exhibits = Exhibits::new(&world);
        world.load_gates();
        world.load_tanks();
        world.retank(false);
        world.name_loaded();
        world.relink();
        for at in 0..world.zoo.entities.len() {
            world.filter_added(at);
        }
        world.walk_loaded();
        world.units.sync(&world.zoo.entities);
        world.load_thoughts();
        world.dirty = false;
        world
    }

    /// Sets the settings of the path finder (the `[AI]` keys): the units and the gate choice
    /// search with them, the gate choice with the diagonal switch the last search left.
    pub fn set_finder(&mut self, settings: Settings) {
        self.units.finder = Finder::new(settings);
    }

    /// A loaded zoo whose stored locale is not the install language keeps no entity name either:
    /// each entity is named like a new one (type text in the install language, numbered by the
    /// counters of a new world in file order), then the file's counters are put back
    /// (localisation 10, zoo format 2 rule 5). Needs the type names of `units.names`, so it runs
    /// once the interface gave them.
    pub fn name_loaded_entities(&mut self, cat: &Catalogue) {
        if self.zoo.locale == self.language.number {
            return;
        }
        let kept = std::mem::take(&mut self.zoo.names);
        let mut entities = std::mem::take(&mut self.zoo.entities);
        for e in &mut entities {
            let name = match crate::place::bulldozer::type_of(cat, e) {
                Some(t) => crate::place::unit::named(self, t),
                None => String::new(),
            };
            if let Some(common) = e.common.as_mut() {
                common.name = name;
            }
        }
        self.zoo.entities = entities;
        self.zoo.names = kept;
    }

    /// The regions of the map (exhibits and money 3.1).
    pub fn exhibits(&self) -> &Exhibits {
        &self.exhibits
    }

    /// The regions again, to change an exhibit's own state (its rating).
    pub fn exhibits_mut(&mut self) -> &mut Exhibits {
        &mut self.exhibits
    }

    /// The tile across the edge of tile `t` in map direction `d` (0, 2, 4 or 6).
    pub fn neighbour(&self, t: usize, d: usize) -> Option<usize> {
        self.scene.neighbour(t, d)
    }

    /// Whether the edge of tile `t` in map direction `d` holds a fence of an exhibit, in its own
    /// slot or in the slot the neighbour has on the same line.
    pub fn closed(&self, t: usize, d: usize) -> bool {
        let fence = |t: usize, d: usize| {
            self.scene
                .edge_object(t, d)
                .is_some_and(|o| self.shapes[self.owner[o]].is_some_and(|s| s.fence))
        };
        fence(t, d) || self.neighbour(t, d).is_some_and(|n| fence(n, (d + 4) % 8))
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

    /// How many quadrants the entity at position `e` of `zoo.entities` holds.
    pub fn held(&self, e: usize) -> usize {
        (0..self.zoo.tiles.len())
            .flat_map(|t| (0..4).map(move |q| (t, q)))
            .filter(|&(t, q)| self.quadrant(t, q) == Some(e))
            .count()
    }

    /// The fence on the edge of tile `t` in map direction `d` (0, 2, 4 or 6).
    pub fn edge(&self, t: usize, d: usize) -> Option<usize> {
        self.scene.edge_object(t, d).map(|o| self.owner[o])
    }

    /// Whether the ground has a step on the edge of tile `t` in map direction `d` (edge state 2
    /// or 3).
    pub fn cliff(&self, t: usize, d: usize) -> bool {
        (self.zoo.tiles[t].edges >> (2 * (3 - d / 2))) & 3 >= 2
    }

    /// Whether the edge of tile `t` in map direction `d` holds a zoo wall piece in its own slot.
    pub fn wall(&self, t: usize, d: usize) -> bool {
        self.edge(t, d)
            .is_some_and(|e| self.shapes[e].is_some_and(|s| s.wall))
    }

    /// Whether the quarter of tile `t` at local (32.., 32..) is held by the zoo entrance.
    pub fn gate(&self, t: usize) -> bool {
        self.quadrant(t, 3)
            .is_some_and(|e| self.shapes[e].is_some_and(|s| s.gate))
    }

    /// Whether an object holding a quarter of tile `t` has a footprint other than exactly 1 by 1
    /// half tiles: the tile then keeps its shape when the ground is shaped.
    pub fn sized(&self, t: usize) -> bool {
        (0..4)
            .filter_map(|q| self.quadrant(t, q))
            .filter_map(|e| self.shapes[e])
            .any(|s| s.footprint != (1, 1))
    }

    /// Whether the object holding the quarter of tile `t` at local (32..64, 32..64) has a footprint
    /// above 2 half tiles in x or in y.
    pub fn big(&self, t: usize) -> bool {
        self.quadrant(t, 3)
            .and_then(|e| self.shapes[e])
            .is_some_and(|s| s.footprint.0 > 2 || s.footprint.1 > 2)
    }

    /// The tiles that a non-contiguous step on tile `t` takes along (terrain editing 5.5): the
    /// tiles under the object holding the quarter at local (0..32, 0..32) or, when that is empty,
    /// under each unit of the tile, when it is bigger than one tile; and, whatever the tile
    /// held, every tile of the tank it belongs to.
    pub fn carried(&self, t: usize) -> Vec<usize> {
        let objects = match self.scene.quadrant_object(t, 0) {
            Some(o) => vec![o],
            None => self.scene.units_on(t).to_vec(),
        };
        let mut tiles: Vec<usize> = objects
            .into_iter()
            .flat_map(|o| self.scene.large_tiles(o))
            .collect();
        if let Some(tank) = self.tank_at(t) {
            tiles.extend(self.tiles_of(tank));
        }
        tiles
    }

    /// The entities (positions in `zoo.entities`) of the units the map index holds on tile `t`.
    pub fn units_on(&self, t: usize) -> impl Iterator<Item = usize> + '_ {
        self.scene.units_on(t).iter().map(|&o| self.owner[o])
    }

    /// Puts the unit at position `at` of the entity list at a new point and facing, as the move of
    /// a step does; the map index follows and the picture is due.
    pub fn place_unit(&mut self, at: usize, point: (i32, i32, i32), rotation: u32) {
        let Some(c) = self
            .zoo
            .entities
            .get_mut(at)
            .and_then(|e| e.common.as_mut())
        else {
            return;
        };
        // The objects stand in the order of the entities.
        let object = self.owner.binary_search(&at).ok();
        let on_map = object.is_none_or(|o| self.scene.is_placed(o));
        if (c.x, c.y, c.z, c.rotation) == (point.0, point.1, point.2, rotation) && on_map {
            return;
        }
        (c.x, c.y, c.z, c.rotation) = (point.0, point.1, point.2, rotation);
        if let Some(o) = object {
            self.scene.move_unit(o, point, rotation % 8);
        }
        self.dirty = true;
    }

    /// Takes the unit at position `at` of the entity list off the map, as the pick-up does (unit
    /// placing 8.2): it stays in the list, stands on no tile and is not drawn, until
    /// [`World::place_unit`] puts it down.
    pub fn lift_unit(&mut self, at: usize) {
        if let Ok(o) = self.owner.binary_search(&at) {
            self.scene.lift_unit(o);
            self.dirty = true;
        }
    }

    /// Stops painting the unit at position `at` of the entity list; it stays on its tile's unit
    /// list (a user hidden in a building, guest buildings 6.7.1).
    pub fn hide_unit(&mut self, at: usize) {
        if let Ok(o) = self.owner.binary_search(&at) {
            self.scene.hide_unit(o);
            self.dirty = true;
        }
    }

    /// Takes the unit at position `at` of the entity list off its tile's unit list only (guest
    /// buildings 6.7.1): it keeps its place, is drawn as before and joins a list again at its
    /// first change of tile.
    pub fn unlist_unit(&mut self, at: usize) {
        if let Ok(o) = self.owner.binary_search(&at) {
            self.scene.unlist_unit(o);
            self.dirty = true;
        }
    }

    /// Whether the entity at position `at` is painted.
    #[cfg(test)]
    pub fn is_drawn(&self, at: usize) -> bool {
        self.fits(at)
    }

    /// Paints the unit at position `at` where it stands, on no tile list.
    pub fn paint_unit(&mut self, at: usize) {
        if let Ok(o) = self.owner.binary_search(&at) {
            self.scene.paint_unit(o);
            self.dirty = true;
        }
    }

    /// The tiles whose exhibits an entity placed or removed at position `at` changes (habitat
    /// rating 4): its own, and the one across for a piece on an edge; none for a guest, a staff
    /// member or an ambient creature, which the rating does not see.
    fn rated_tiles(&self, at: usize) -> Vec<usize> {
        let e = &self.zoo.entities[at];
        let unseen = [
            "guests",
            "keeper",
            "maint",
            "tour",
            "staff",
            "helicopter",
            "ambient",
        ];
        if unseen.contains(&e.class.as_str()) {
            return Vec::new();
        }
        if let Some((t, d)) = self.slot_of(at) {
            return [Some(t), self.neighbour(t, d)]
                .into_iter()
                .flatten()
                .collect();
        }
        let c = e.common.as_ref();
        (c.and_then(|c| self.tile_index(c.x.div_euclid(64), c.y.div_euclid(64))))
            .into_iter()
            .collect()
    }

    /// Sets the escape check flag of the exhibit that tile `t` belongs to (habitat rating 5.8);
    /// the zoo grounds have none.
    pub fn escape_check(&mut self, t: usize) {
        let x = self.exhibits.region(t);
        if let Some(e) = x.and_then(|x| self.exhibits.get_mut(x)) {
            e.rating.escape = true;
        }
    }

    /// Marks the exhibit that tile `t` belongs to for a new rating (something on it changed);
    /// the zoo grounds have none.
    pub fn mark_rating(&mut self, t: usize) {
        if let Some(r) = self.exhibits.region(t) {
            self.exhibits.mark(r);
        }
    }

    /// Sets the units left of the food entity at position `at`: its exhibit is rated again and the
    /// picture is due (animals-needs 5.1).
    pub fn set_food(&mut self, at: usize, left: i32) {
        let Some(c) = self
            .zoo
            .entities
            .get_mut(at)
            .and_then(|e| e.common.as_mut())
        else {
            return;
        };
        c.food = left;
        let (x, y) = (c.x.div_euclid(64), c.y.div_euclid(64));
        self.tile_index(x, y)
            .into_iter()
            .for_each(|t| self.mark_rating(t));
        self.dirty = true;
    }

    /// Whether the unit `id` is held by the player: it is off the map, in no exhibit (unit placing
    /// 8.2), whatever position it stores.
    pub fn is_carried(&self, id: u32) -> bool {
        self.units.get(id).is_some_and(|u| u.carried)
    }

    /// Whether an animal, guest or staff member stands on tile `t`.
    pub fn unit_on(&self, t: usize) -> bool {
        let width = self.zoo.width as usize;
        self.zoo
            .entities
            .iter()
            .zip(&self.shapes)
            .filter(|(_, s)| s.is_some_and(|s| s.kind == Kind::Unit))
            .filter_map(|(e, _)| e.common.as_ref())
            .filter(|c| !self.is_carried(c.id))
            .any(|c| {
                let (x, y) = (c.x.div_euclid(64), c.y.div_euclid(64));
                usize::try_from(y)
                    .ok()
                    .zip(usize::try_from(x).ok())
                    .is_some_and(|(y, x)| x < width && y * width + x == t)
            })
    }

    /// How the entity at position `e` of `zoo.entities` takes its place.
    pub fn shape(&self, e: usize) -> Option<Shape> {
        self.shapes.get(e).copied().flatten()
    }

    /// The edge slot a fence or tank wall holds: its tile and the map direction (0, 2, 4 or 6).
    /// Every question about where a piece stands goes through here and the slot lookup
    /// ([`World::edge`]), not through the piece's own position.
    pub fn slot(&self, e: usize) -> Option<(usize, usize)> {
        let o = self.owner.iter().position(|&owner| owner == e)?;
        let obj = &self.scene.objects[o];
        let t = self.scene.tile_at(obj.x, obj.y)?;
        let d = obj.facing as usize;
        (self.scene.edge_object(t, d) == Some(o)).then_some((t, d))
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

    /// Adds an entity with a fresh id and returns the id; `None` for one without a body. The
    /// price paid is told with [`World::priced`].
    pub fn add(&mut self, mut entity: Entity, shape: Option<Shape>) -> Option<u32> {
        let common = entity.common.as_mut()?;
        let id = self.zoo.id_counter.checked_add(1)?;
        (self.zoo.id_counter, common.id) = (id, id);
        self.zoo.entities.push(entity);
        self.shapes.push(shape);
        self.changed();
        let mut notes = Vec::new();
        let rated = self.rated_tiles(self.zoo.entities.len() - 1);
        if shape.is_some_and(|s| s.fence) {
            let slot = self.slot_of(self.zoo.entities.len() - 1);
            let seen = self.events.len();
            let before = self.keeping(slot);
            self.settle(slot, false);
            notes = self.changed_by(before, seen);
        }
        rated.into_iter().for_each(|t| self.mark_rating(t));
        notes.into_iter().for_each(|note| self.record(note));
        self.record(Action::Placed { id, price: 0.0 });
        self.filter_added(self.zoo.entities.len() - 1);
        Some(id)
    }

    /// The exhibits that a fence piece cut in two or replaced by one of the other kind (a split
    /// or a morph: the events since `seen` tell it), with their names and gates as they were
    /// `before`: the actions that give them back, to record before the piece itself, so that the
    /// undo takes the piece away first (placing 8, kind 11).
    fn changed_by(&self, before: Vec<(u32, Action)>, seen: usize) -> Vec<Action> {
        let hit = |id: u32| {
            self.events[seen..]
                .iter()
                .any(|e| matches!(e, Event::Split(s) | Event::Removed(s) if *s == id))
        };
        (before.into_iter())
            .filter(|(id, _)| hit(*id))
            .map(|(_, note)| note)
            .collect()
    }

    /// The unit at position `at` of the list takes another subtype of its type and the shape that
    /// goes with it (a young animal grows up).
    pub fn retype(&mut self, at: usize, subtype: &str, shape: Shape) {
        self.zoo.entities[at].subtype = subtype.into();
        self.shapes[at] = Some(shape);
        self.changed();
    }

    /// Removes the entity with this id; whether there was one. A fence piece that opens its line
    /// records the exhibits it opens, with their names and gates, first.
    pub fn remove(&mut self, id: u32) -> bool {
        let Some(at) = self.find(id) else {
            return false;
        };
        let slot = self.slot_of(at);
        let fence = self.shapes[at].is_some_and(|s| s.fence);
        if fence {
            self.note_tank(at);
            self.note_opening(slot, false);
        }
        let rated = self.rated_tiles(at);
        // A fence piece removed sets the escape check flag on both sides (habitat rating 5.8).
        if fence {
            rated.iter().for_each(|&t| self.escape_check(t));
        }
        let entity = self.zoo.entities.remove(at);
        let shape = self.shapes.remove(at);
        if let Some(c) = &entity.common {
            self.filter_view.remove(&c.id);
        }
        self.changed();
        if fence {
            self.settle(slot, false);
        }
        rated.into_iter().for_each(|t| self.mark_rating(t));
        self.record(Action::Removed {
            at,
            entity,
            shape,
            price: 0.0,
        });
        true
    }

    /// Puts back an entity that was taken out for the player to hold (guest buildings 11.1): it
    /// keeps its id and goes to position `at` of the list again, with nothing recorded for the
    /// undo. The exhibit it stands in is marked for a new rating.
    pub fn put_back(&mut self, at: usize, entity: Entity, shape: Option<Shape>) {
        let at = at.min(self.zoo.entities.len());
        self.zoo.entities.insert(at, entity);
        self.shapes.insert(at, shape);
        self.changed();
        self.rated_tiles(at)
            .into_iter()
            .for_each(|t| self.mark_rating(t));
    }

    /// Whether `entity` has the type of the one at `at` in the list.
    pub fn same_type(&self, at: usize, entity: &Entity) -> bool {
        let old = &self.zoo.entities[at];
        (&old.class, &old.type_name, &old.subtype)
            == (&entity.class, &entity.type_name, &entity.subtype)
    }

    /// Puts a new entity in place of the one with this id, as one change, in the same place of
    /// the entity list. An entity of the same type, and a path of another type (placing 5.3,
    /// 6.3), keeps the old one's id; a fence piece of another type gets a fresh id. An exhibit
    /// fence replaced by another stays closed and keeps its exhibit, and a gate stays the gate
    /// when the new type has one. Returns the id of the new entity.
    pub fn swap(&mut self, id: u32, mut entity: Entity, shape: Option<Shape>) -> Option<u32> {
        let at = self.find(id)?;
        let slot = self.slot_of(at);
        let was = self.shapes[at];
        let door = self.is_door(at);
        let same = self.same_type(at, &entity);
        let path = was.is_some_and(|s| s.kind == Kind::Path);
        let keep = same || path;
        let new = match keep {
            true => id,
            false => self.zoo.id_counter.checked_add(1)?,
        };
        let common = entity.common.as_mut()?;
        (self.zoo.id_counter, common.id) = (self.zoo.id_counter.max(new), new);
        let gate = door && shape.is_some_and(|s| s.gated && !s.wall);
        if gate {
            entity.subtype = "g".into();
        }
        let fence = (was.is_some_and(|s| s.fence), shape.is_some_and(|s| s.fence));
        if fence == (true, false) {
            self.note_opening(slot, true);
        }
        let before = self
            .keeping(slot)
            .into_iter()
            .filter(|_| fence == (true, true));
        let before: Vec<_> = before.collect();
        let seen = self.events.len();
        let exhibit = (self.exhibits.list().iter())
            .find(|e| e.gate == Some(id))
            .map(|e| e.id);
        let old = std::mem::replace(&mut self.zoo.entities[at], entity);
        self.shapes[at] = shape;
        if let Some(e) = exhibit.and_then(|x| self.exhibits.get_mut(x)) {
            e.gate = gate.then_some(new);
        }
        self.changed();
        if fence.0 || fence.1 {
            self.settle(slot, false);
        }
        let rated = self.rated_tiles(at);
        rated.into_iter().for_each(|t| self.mark_rating(t));
        // A fence replaced by one of another kind (a morph) records the exhibits it changes
        // (kind 11); a path laid on a path of the same type records nothing (placing 8).
        self.changed_by(before, seen)
            .into_iter()
            .for_each(|note| self.record(note));
        if !(same && path) {
            self.record(Action::Replaced {
                id: new,
                old,
                shape: was,
                exhibit,
                price: 0.0,
            });
        }
        Some(new)
    }

    /// Sets the elevation, shape and terrain type of tile (`tx`, `ty`) and writes the edge states
    /// of it and of its four neighbours again (map drawing 3.5); whether the tile is on the map.
    pub fn set_tile(&mut self, tx: i32, ty: i32, elevation: i32, shape: u8, terrain: u8) -> bool {
        let Some(t) = self.tile_index(tx, ty) else {
            return false;
        };
        self.put_tile(t, elevation, shape, terrain, true);
        true
    }

    /// The last two actions, when both are tiles with no price, become one: the levelling and the
    /// floor of a tank are one terrain action (placing 8, kind 3). A tile in both keeps its
    /// first state.
    pub(super) fn join_tiles(&mut self) {
        let free = |a: &Action| matches!(a, Action::Tiles { price, .. } if *price == 0.0);
        let n = self.undo.len();
        if n < 2 || !free(&self.undo[n - 1]) || !free(&self.undo[n - 2]) {
            return;
        }
        let Some(Action::Tiles { tiles, .. }) = self.undo.pop() else {
            return;
        };
        if let Some(Action::Tiles { tiles: first, .. }) = self.undo.last_mut() {
            for tile in tiles {
                if first.iter().all(|f| f.0 != tile.0) {
                    first.push(tile);
                }
            }
        }
    }

    /// Sets tile `t`; `record` puts it in the undo list.
    fn put_tile(&mut self, t: usize, elevation: i32, shape: u8, terrain: u8, record: bool) {
        let old = self.zoo.tiles[t];
        if record {
            self.record(Action::Tiles {
                tiles: vec![(t, old)],
                price: 0.0,
            });
        }
        let tile = &mut self.zoo.tiles[t];
        (tile.elevation, tile.shape, tile.terrain) = (elevation, shape, terrain);
        if (old.elevation, old.shape) != (elevation, shape) {
            self.reheight(t);
        }
        self.edges_around(t);
        self.changed();
        self.mark_rating(t);
        self.escape_check(t);
    }

    /// Tells the price of the action recorded last: what an undo gives back (a purchase) or takes
    /// back (a refund). A call after an edit that records nothing changes nothing.
    pub fn priced(&mut self, amount: f32) {
        if let Some(
            Action::Placed { price, .. }
            | Action::Removed { price, .. }
            | Action::Replaced { price, .. }
            | Action::Tiles { price, .. },
        ) = self.undo.last_mut()
        {
            *price = amount;
        }
    }

    /// Runs an edit that the undo list does not record: the simulation's own removals, and a test
    /// that needs something in the way.
    pub fn unrecorded<T>(&mut self, edit: impl FnOnce(&mut World) -> T) -> T {
        let (undo, fresh) = (std::mem::take(&mut self.undo), self.fresh);
        let result = edit(self);
        (self.undo, self.fresh) = (undo, fresh);
        result
    }

    /// Whether the undo list holds the placement of the entity `id`.
    pub fn undoes(&self, id: u32) -> bool {
        self.undo
            .iter()
            .any(|a| matches!(a, Action::Placed { id: i, .. } if *i == id))
    }

    /// Drops the actions of the undo list that concern the entity `id`, which the simulation
    /// removed: an undo must neither bring it back nor remove it.
    pub fn forget_undo(&mut self, id: u32) {
        self.undo.retain(|a| match a {
            Action::Placed { id: i, .. }
            | Action::Copied { id: i }
            | Action::Replaced { id: i, .. }
            | Action::Moved { id: i, .. } => *i != id,
            _ => true,
        });
    }

    /// Whether an undo would do anything: the Undo button is enabled.
    pub fn can_undo(&self) -> bool {
        !self.undo.is_empty()
    }

    /// Forgets the undo list.
    pub fn clear_undo(&mut self) {
        self.undo.clear();
    }

    /// An accepted terrain session: the undo list holds only this one action, which puts the
    /// tiles back as they were and gives the price back (terrain editing 8.4, 8.7).
    pub fn accepted(&mut self, tiles: Vec<(usize, Tile)>, price: f32) {
        self.undo.clear();
        self.undo.push(Action::Tiles { tiles, price });
    }

    /// Takes back the operations of the last press, last first (placing 8): a placed object goes,
    /// a removed one comes back in its place of the list, a piece laid on another gets its old
    /// type back, tiles get their old state, an exhibit its name and gate. The exhibits change as
    /// for a normal edit, quietly (no name is asked, no gate chosen, the counter stays).
    ///
    /// A removed object that cannot be put back is skipped when the list holds several; else the
    /// undo stops there and the actions not undone yet stay in the list (`blocked`). What was
    /// undone is told in `steps`, with the prices for the caller to move in the cash.
    pub fn undo(&mut self) -> Undone {
        let mut actions = std::mem::take(&mut self.undo);
        let several = actions
            .iter()
            .filter(|a| matches!(a, Action::Removed { .. }))
            .count()
            > 1;
        let mut done = Undone::default();
        let (mut stale, mut doors) = (false, Vec::new());
        while let Some(action) = actions.pop() {
            match action {
                Action::Placed { id, price } => {
                    if let Some(at) = self.find(id) {
                        let spot = self.spot(at);
                        self.zoo.entities.remove(at);
                        self.shapes.remove(at);
                        self.changed();
                        stale = true;
                        done.steps.push(Step::Removed(spot, price));
                    }
                }
                Action::Copied { id } => {
                    if let Some(at) = self.find(id) {
                        let spot = self.spot(at);
                        let entity = self.zoo.entities.remove(at);
                        self.shapes.remove(at);
                        self.changed();
                        stale = true;
                        done.steps.push(Step::Copied(spot, Box::new(entity)));
                    }
                }
                Action::Tank { tile, value } => {
                    if std::mem::take(&mut stale) {
                        self.settle(None, true);
                    }
                    self.restore_tank(tile, value);
                }
                Action::Removed {
                    at,
                    entity,
                    shape,
                    price,
                } => {
                    // It must find its place with the others standing: it is tried last, then
                    // moved to its old position in the list.
                    self.zoo.entities.push(entity.clone());
                    self.shapes.push(shape);
                    self.changed();
                    if !self.fits(self.zoo.entities.len() - 1) {
                        self.zoo.entities.pop();
                        self.shapes.pop();
                        self.changed();
                        if several {
                            done.steps.push(Step::Skipped(self.spot_of(&entity)));
                            continue;
                        }
                        done.blocked = Some(entity.clone());
                        actions.push(Action::Removed {
                            at,
                            entity,
                            shape,
                            price,
                        });
                        break;
                    }
                    let (entity, shape) = (self.zoo.entities.pop(), self.shapes.pop());
                    let at = at.min(self.zoo.entities.len());
                    if let (Some(entity), Some(shape)) = (entity, shape) {
                        self.zoo.entities.insert(at, entity);
                        self.shapes.insert(at, shape);
                    }
                    self.changed();
                    stale = true;
                    if self.is_door(at) {
                        doors.extend(self.zoo.entities[at].common.as_ref().map(|c| c.id));
                    }
                    done.steps.push(Step::Back(self.spot(at), price));
                }
                Action::Replaced {
                    id,
                    old,
                    shape,
                    exhibit,
                    price,
                } => {
                    if let Some(at) = self.find(id) {
                        let old_id = old.common.as_ref().map_or(id, |c| c.id);
                        self.zoo.entities[at] = old;
                        self.shapes[at] = shape;
                        if let Some(e) = exhibit.and_then(|x| self.exhibits.get_mut(x)) {
                            e.gate = Some(old_id);
                        }
                        self.changed();
                        stale = true;
                        done.steps.push(Step::Retyped(self.spot(at), price));
                    }
                }
                Action::Moved { id, at, rotation } => {
                    let c = self
                        .find(id)
                        .and_then(|i| self.zoo.entities[i].common.as_mut());
                    if let Some(c) = c {
                        (c.x, c.y, c.z, c.rotation) = (at.0, at.1, at.2, rotation);
                        self.changed();
                        stale = true;
                    }
                }
                Action::Tiles { tiles, price } => {
                    for &(t, tile) in &tiles {
                        let moved = (tile.elevation, tile.shape)
                            != (self.zoo.tiles[t].elevation, self.zoo.tiles[t].shape);
                        self.zoo.tiles[t] = tile;
                        if moved {
                            self.reheight(t);
                        }
                        self.edges_around(t);
                    }
                    self.changed();
                    stale = true;
                    done.steps.push(Step::Tiles(tiles.len(), price));
                }
                Action::Exhibit { tile, name, gate } => {
                    if std::mem::take(&mut stale) {
                        self.settle(None, true);
                    }
                    done.names.extend(self.restore_exhibit(tile, name, gate));
                }
            }
        }
        if stale {
            self.settle(None, true);
        }
        // A piece that was a gate and is not the gate of any exhibit now is a fence again.
        for id in doors {
            let Some(at) = self.find(id) else { continue };
            if !self.exhibits.list().iter().any(|e| e.gate == Some(id)) {
                self.make_fence(at);
            }
        }
        self.undo = actions;
        self.fresh = false;
        self.dirty = true;
        done
    }

    /// Whether the entity at `at` found its place on the map (or has none to find).
    fn fits(&self, at: usize) -> bool {
        (self.owner.iter().position(|&e| e == at)).is_none_or(|o| self.scene.is_placed(o))
    }

    /// Where the entity at `at` stands.
    fn spot(&self, at: usize) -> Spot {
        let mut spot = self.spot_of(&self.zoo.entities[at]);
        if let Some((t, d)) = self.slot(at) {
            let width = self.zoo.width as usize;
            spot.tile = ((t % width) as i32, (t / width) as i32);
            spot.side = Some(d as u32);
        }
        spot
    }

    /// Where an entity stands by its own position (a fence by its side).
    pub fn spot_of(&self, e: &Entity) -> Spot {
        let c = e.common.as_ref();
        let fence = matches!(e.class.as_str(), "fences" | "tankwall");
        Spot {
            name: format!("{}/{}/{}", e.class, e.type_name, e.subtype),
            tile: c.map_or((0, 0), |c| (c.x.div_euclid(64), c.y.div_euclid(64))),
            side: c.filter(|_| fence).map(|c| c.rotation % 8),
            id: c.map_or(0, |c| c.id),
        }
    }

    /// The exhibits on the two tiles of a slot, with the id of each: the action that gives one
    /// back its name and gate (placing 8, kind 11).
    fn keeping(&self, slot: Option<(usize, usize)>) -> Vec<(u32, Action)> {
        let Some((a, d)) = slot else {
            return Vec::new();
        };
        let mut found: Vec<(u32, Action)> = Vec::new();
        for t in [Some(a), self.neighbour(a, d)].into_iter().flatten() {
            let Some(e) = (self.exhibits.region(t)).and_then(|r| self.exhibits.get(r)) else {
                continue;
            };
            if found.iter().all(|(id, _)| *id != e.id) {
                found.push((
                    e.id,
                    Action::Exhibit {
                        tile: t,
                        name: e.name.clone(),
                        gate: self.gate_slot(e.id),
                    },
                ));
            }
        }
        found
    }

    /// Records the exhibits on the two sides of the fence piece on this slot, with their names and
    /// gates, before it goes, when its two tiles are in different regions. A removal records
    /// whether the line stays closed or opens; a replacement (`replaced`) only when no piece of
    /// the other tile holds the line too (placing 8, kind 11).
    fn note_opening(&mut self, slot: Option<(usize, usize)>, replaced: bool) {
        let Some((a, d)) = slot else { return };
        let Some(b) = self.neighbour(a, d) else {
            return;
        };
        let held = self
            .edge(b, (d + 4) % 8)
            .is_some_and(|o| self.shapes[o].is_some_and(|s| s.fence));
        if (replaced && held) || self.exhibits.region(a) == self.exhibits.region(b) {
            return;
        }
        for (_, note) in self.keeping(slot) {
            self.record(note);
        }
    }

    /// Records the values of the tank whose own wall at position `at` is about to go: the purity,
    /// salinity, filled flag, wall height and base level, in this order (placing 8, kinds 4 to 8).
    fn note_tank(&mut self, at: usize) {
        let Some((t, _)) = self.slot_of(at) else {
            return;
        };
        if !self.shapes[at].is_some_and(|s| s.tank.wall) {
            return;
        }
        let Some(id) = self.tank_at(t) else { return };
        let (Some(tank), Some(&first)) = (self.tank(id), self.tiles_of(id).first()) else {
            return;
        };
        let level = self.zoo.tiles[first].elevation;
        for value in [
            TankValue::Purity(tank.purity),
            TankValue::Salt(tank.salt),
            TankValue::Filled(tank.filled),
            TankValue::Height(tank.height),
            TankValue::Level(level),
        ] {
            self.record(Action::Tank { tile: t, value });
        }
    }

    /// Sets a value of the tank now on tile `tile`; none there, nothing happens.
    fn restore_tank(&mut self, tile: usize, value: TankValue) {
        let Some(id) = self.tank_at(tile) else { return };
        if let TankValue::Level(level) = value {
            for t in self.tiles_of(id) {
                let at = self.zoo.tiles[t];
                if at.elevation != level {
                    self.put_tile(t, level, at.shape, at.terrain, false);
                }
            }
        }
        if let Some(tank) = self.exhibits.get_mut(id).and_then(|e| e.tank.as_mut()) {
            match value {
                TankValue::Purity(v) => tank.purity = v,
                TankValue::Salt(v) => tank.salt = v,
                TankValue::Height(_) | TankValue::Level(_) | TankValue::Filled(_) => {}
            }
        }
        // The filled state goes through the fill change, which makes the show links again.
        if let TankValue::Filled(v) = value {
            self.edit_tank(id, |t, _| t.filled = v);
        }
        // The height goes through the wall height change: the water is cut to the new full level.
        if let TankValue::Height(v) = value {
            self.set_height(id, v);
        }
        self.events.push(Event::Tank(id));
        self.sync_tanks();
    }

    /// Gives the exhibit on tile `tile` its name, and its gate on the line of the recorded slot
    /// (the piece there, on either tile's slot); the gate it had becomes a fence again. Gives back
    /// the name it set.
    fn restore_exhibit(
        &mut self,
        tile: usize,
        name: String,
        gate: Option<(usize, usize)>,
    ) -> Option<String> {
        let id = self
            .exhibits
            .region(tile)
            .filter(|&r| r != GROUNDS && r != EXTERIOR)?;
        let piece = gate.and_then(|(t, d)| {
            let across = self.neighbour(t, d).and_then(|n| self.edge(n, (d + 4) % 8));
            let at = self.edge(t, d).or(across)?;
            self.shapes[at].is_some_and(|s| s.fence).then_some(at)
        });
        let current = self.exhibits.get(id).and_then(|e| e.gate);
        let new = piece
            .filter(|&at| self.make_door(at))
            .and_then(|at| self.zoo.entities[at].common.as_ref().map(|c| c.id));
        if let Some(old) = current
            .filter(|&g| Some(g) != new)
            .and_then(|g| self.find(g))
        {
            self.make_fence(old);
        }
        let named = self.exhibits.get_mut(id).map(|e| {
            e.name = name.clone();
            e.gate = new;
            name
        });
        self.dirty = true;
        if new.is_some() {
            self.events.push(Event::Gate(id));
        }
        named
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
        // The units held off the map keep how they are held across the rebuild: a carried unit is
        // on no list and not painted (simulation 6.2), a hidden user is not painted but stays on
        // its list (guest buildings 6.4 point 6), a unit off its list stays off it (6.7.1).
        let held: Vec<(u32, Held)> = self
            .scene
            .held()
            .filter_map(|(o, held)| {
                let id = self.zoo.entities[self.owner[o]].common.as_ref()?.id;
                Some((id, held))
            })
            .collect();
        (self.scene, self.owner) = index(&self.zoo, &self.shapes);
        for (id, held) in held {
            let Some(at) = self.find(id) else {
                continue;
            };
            match held {
                Held::Unlisted => self.unlist_unit(at),
                Held::Hidden => self.hide_unit(at),
                Held::Lifted => self.lift_unit(at),
            }
        }
        self.units.sync(&self.zoo.entities);
        self.dirty = true;
        self.edits += 1;
        self.relink();
    }

    /// The objects standing on tile `t` take their height from the ground again (not the ones
    /// that hold no place on the map, such as ambient animals).
    fn reheight(&mut self, t: usize) {
        let width = i64::from(self.zoo.width);
        let tile = self.zoo.tiles[t];
        for (entity, shape) in self.zoo.entities.iter_mut().zip(&self.shapes) {
            let (Some(c), Some(shape)) = (entity.common.as_mut(), shape) else {
                continue;
            };
            let (tx, ty) = (c.x.div_euclid(64), c.y.div_euclid(64));
            if shape.kind != Kind::Overlay
                && (0..width).contains(&i64::from(tx))
                && i64::from(ty) * width + i64::from(tx) == t as i64
            {
                c.z = ground_height(&tile, c.x.rem_euclid(64), c.y.rem_euclid(64));
            }
        }
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
