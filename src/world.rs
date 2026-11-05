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
#[cfg(test)]
use crate::path::{Finder, Settings};
use crate::render::order::{Kind, Object, Scene, ground_height};
use crate::strings::Language;
use crate::unit::Units;
use crate::zoo::{Entity, Tile, Zoo};
use held::Shift;
use undo::Action;
#[cfg(test)]
use undo::TankValue;
pub(crate) use undo::{Spot, Step, Undone};

mod amphibious;
mod areas;
mod decide;
mod edges;
mod edit;
mod exhibit;
mod filter;
mod held;
mod portal;
mod portal_anim;
mod save;
mod settle;
mod tank;
mod undo;
mod water;
mod wear;
pub(crate) use exhibit::Event;
pub(crate) use filter::{
    Wear, months_since as filter_months_since, placed as placed_filter,
    sell_rate as filter_sell_rate, wear as filter_wear,
};
pub(crate) use portal::Portal;
pub use wear::Decay;
pub(crate) use wear::piece_event;

/// How an entity takes its place on the map, from its type.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Shape {
    pub(crate) kind: Kind,
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
    pub(crate) tank: TankKind,
    /// `walkable` and `walkableByTall` (pathfinding 4.3): units walk through the object.
    pub walkable: (bool, bool),
    /// For an animal: its egg footprint (`cEggFootprintX/Y`) and its crate footprint
    /// (`cBoxFootprintX/Y`), in half tiles; `None` for the rest.
    pub forms: Option<[(i32, i32); 2]>,
    /// A scenery item whose type has `cRubbleable` 1 (the default): a rampaging animal walks
    /// through it and turns it to rubble (animals 11).
    pub rubbleable: bool,
}

/// What a type says about tanks (exhibits and money 3.9).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct TankKind {
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
    /// tank (tank fauna 7.3); `cHeight` is also what a fence-crushing animal measures.
    pub depth: i32,
    pub height: i32,
    /// `cIsJumpable`, `cIsClimbable` and `cIsElectrified` of a fence type, and its `cDecayedLife`
    /// (pathfinding 5.3).
    pub jumpable: bool,
    pub climbable: bool,
    pub electrified: bool,
    pub decayed: i32,
}

impl TankKind {
    /// A type that says nothing about tanks.
    pub(crate) const NONE: TankKind = TankKind {
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
        jumpable: false,
        climbable: false,
        electrified: false,
        decayed: 0,
    };
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
    /// The ids of the water effects the world manager made (simulation 7.2), which no save keeps.
    waves: std::collections::HashSet<u32>,
    /// The level and purity of each tank the last `world water` line told (not saved, not hashed).
    told_water: BTreeMap<u32, (u32, u32)>,
    /// The tanks whose water level or wall height moved since the last [`World::take_wet`]: the
    /// objects on their tiles are validated again (guest buildings 11.1.1; not saved).
    wet: Vec<u32>,
    /// The tanks whose water, base level, walls or ground changed (guest buildings 12.2 point 22).
    stirred: Vec<u32>,
    /// What each tank filter shows besides its record: switched off, or showing bubbles (not
    /// saved, not hashed).
    filter_view: BTreeMap<u32, filter::View>,
    /// The install language: the exhibit names of a loaded zoo are kept when its stored locale is
    /// this one, and a save writes it.
    language: Language,
    /// What the completed research programs did to the entity types of this game (research 6.4).
    pub(crate) research: crate::research::Overlay,
    /// The animals that came onto the map or left it, and the show-neighbour links made or dropped,
    /// that the show lists have not seen yet (shows 20; not saved, not hashed).
    show_moves: Vec<crate::show::Move>,
    /// A saved zoo is being read: a portal flag that changes keeps the open byte (shows 31.3 point 7).
    reading: bool,
    /// The sprite players of the portal walls, by the id of the piece (shows 32.6; not saved).
    portal_players: std::collections::HashMap<u32, portal_anim::Player>,
}

impl World {
    /// The world of a test zoo in the shipped language (see [`World::with_language`]).
    #[cfg(test)]
    pub(crate) fn new(zoo: Zoo, shape: impl Fn(&Entity) -> Option<Shape>) -> World {
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
            waves: Default::default(),
            told_water: BTreeMap::new(),
            wet: Vec::new(),
            stirred: Vec::new(),
            filter_view: BTreeMap::new(),
            words: language.exhibit_words.clone(),
            language,
            research: Default::default(),
            show_moves: Vec::new(),
            reading: true,
            portal_players: Default::default(),
        };
        world.mark_exterior();
        world.exhibits = Exhibits::new(&world);
        let all: Vec<u32> = world.exhibits.list().iter().map(|e| e.id).collect();
        world.resize_areas(&all);
        world.load_gates();
        world.load_tanks();
        world.retank(false);
        world.load_infos();
        world.name_loaded();
        world.relink();
        // The loaded animals are placed in file order and join the show lists with the first step
        // (shows 26 point 3); the first rating of their exhibits then finds them listed.
        world.show_moves.clear();
        for at in 0..world.zoo.entities.len() {
            world.filter_added(at);
            world.show_moved(at, true);
        }
        world.walk_loaded();
        world.units.sync(&world.zoo.entities);
        world.load_thoughts();
        world.dirty = false;
        world.reading = false;
        world.portals_at_rest();
        world.wet.clear();
        world.stirred.clear();
        world
    }

    /// Sets the settings of the path finder (the `[AI]` keys): the units and the gate choice
    /// search with them, the gate choice with the diagonal switch the last search left.
    #[cfg(test)]
    pub(crate) fn set_finder(&mut self, settings: Settings) {
        self.units.finder = Finder::new(settings);
    }

    /// A loaded zoo whose stored locale is not the install language keeps no entity name either:
    /// each entity is named like a new one (type text in the install language, numbered by the
    /// counters of a new world in file order), then the file's counters are put back
    /// (localisation 10, zoo format 2 rule 5). Needs the type names of `units.names`, so it runs
    /// once the interface gave them.
    pub(crate) fn name_loaded_entities(&mut self, cat: &Catalogue) {
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
    pub(crate) fn exhibits_mut(&mut self) -> &mut Exhibits {
        &mut self.exhibits
    }

    /// The tile across the edge of tile `t` in map direction `d` (0, 2, 4 or 6).
    pub(crate) fn neighbour(&self, t: usize, d: usize) -> Option<usize> {
        self.scene.neighbour(t, d)
    }

    /// Whether the edge of tile `t` in map direction `d` holds a fence of an exhibit, in its own
    /// slot or in the slot the neighbour has on the same line.
    pub(crate) fn closed(&self, t: usize, d: usize) -> bool {
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

    /// Whether nothing stands on tile `t` (see [`Scene::is_empty`]).
    pub(crate) fn is_empty(&self, t: usize) -> bool {
        self.scene.is_empty(t)
    }

    /// Whether tile `t` is inside the zoo wall.
    pub(crate) fn in_zoo(&self, t: usize) -> bool {
        self.in_zoo.get(t).copied().unwrap_or(false)
    }

    /// The entity of the path piece of tile `t`.
    pub(crate) fn path(&self, t: usize) -> Option<usize> {
        self.scene.path_of(t).map(|o| self.owner[o])
    }

    /// The entity that holds quadrant `q` (`qy * 2 + qx`, 0 to 3) of tile `t`.
    pub(crate) fn quadrant(&self, t: usize, q: usize) -> Option<usize> {
        self.scene.quadrant_object(t, q).map(|o| self.owner[o])
    }

    /// How many quadrants the entity at position `e` of `zoo.entities` holds.
    pub(crate) fn held(&self, e: usize) -> usize {
        (0..self.zoo.tiles.len())
            .flat_map(|t| (0..4).map(move |q| (t, q)))
            .filter(|&(t, q)| self.quadrant(t, q) == Some(e))
            .count()
    }

    /// The fence on the edge of tile `t` in map direction `d` (0, 2, 4 or 6).
    pub(crate) fn edge(&self, t: usize, d: usize) -> Option<usize> {
        self.scene.edge_object(t, d).map(|o| self.owner[o])
    }

    /// Whether the ground has a step on the edge of tile `t` in map direction `d` (edge state 2
    /// or 3).
    pub(crate) fn cliff(&self, t: usize, d: usize) -> bool {
        (self.zoo.tiles[t].edges >> (2 * (3 - d / 2))) & 3 >= 2
    }

    /// Whether the edge of tile `t` in map direction `d` holds a zoo wall piece in its own slot.
    pub(crate) fn wall(&self, t: usize, d: usize) -> bool {
        self.edge(t, d)
            .is_some_and(|e| self.shapes[e].is_some_and(|s| s.wall))
    }

    /// Whether the quarter of tile `t` at local (32.., 32..) is held by the zoo entrance.
    pub(crate) fn gate(&self, t: usize) -> bool {
        self.quadrant(t, 3)
            .is_some_and(|e| self.shapes[e].is_some_and(|s| s.gate))
    }

    /// Whether an object holding a quarter of tile `t` has a footprint other than exactly 1 by 1
    /// half tiles: the tile then keeps its shape when the ground is shaped.
    pub(crate) fn sized(&self, t: usize) -> bool {
        (0..4)
            .filter_map(|q| self.quadrant(t, q))
            .filter_map(|e| self.shapes[e])
            .any(|s| s.footprint != (1, 1))
    }

    /// Whether the object holding the quarter of tile `t` at local (32..64, 32..64) has a footprint
    /// above 2 half tiles in x or in y.
    pub(crate) fn big(&self, t: usize) -> bool {
        self.quadrant(t, 3)
            .and_then(|e| self.shapes[e])
            .is_some_and(|s| s.footprint.0 > 2 || s.footprint.1 > 2)
    }

    /// The tiles that a non-contiguous step on tile `t` takes along (terrain editing 5.5): the
    /// tiles under the object holding the quarter at local (0..32, 0..32) or, when that is empty,
    /// under each unit of the tile, when it is bigger than one tile; and, whatever the tile
    /// held, every tile of the tank it belongs to.
    pub(crate) fn carried(&self, t: usize) -> Vec<usize> {
        let mut tiles = match self.scene.quadrant_object(t, 0) {
            Some(o) => self.scene.large_tiles(o),
            None => (self.scene.units_on(t).iter())
                .flat_map(|&o| self.unit_tiles(o))
                .collect(),
        };
        if let Some(tank) = self.tank_at(t) {
            tiles.extend(self.tiles_of(tank));
        }
        tiles
    }

    /// The tiles under the unit object `o` (terrain editing 5.5): its egg, crate or plain footprint
    /// turned with its facing.
    fn unit_tiles(&self, o: usize) -> Vec<usize> {
        let at = self.owner[o];
        let (Some(c), Some((fx, fy))) = (&self.zoo.entities[at].common, self.unit_footprint(at))
        else {
            return self.scene.large_tiles(o);
        };
        let turned = if c.rotation % 4 == 0 {
            (fx, fy)
        } else {
            (fy, fx)
        };
        self.scene.tiles_under(c.x, c.y, turned)
    }

    /// The footprint in half tiles, along its own axes, of the unit at `at` of `zoo.entities`: an
    /// animal in its egg takes its type's egg footprint, else one in its crate the crate's, else the
    /// shape's (placing 5.2 item 4). `None` for an entity without a shape.
    pub(crate) fn unit_footprint(&self, at: usize) -> Option<(i32, i32)> {
        let shape = self.shape(at)?;
        let form = (self.zoo.entities[at].common.as_ref())
            .and_then(|c| self.units.get(c.id))
            .and_then(|u| match &u.part {
                crate::unit::Part::Animal(a) if a.egg || a.crated => Some(a.egg),
                _ => None,
            });
        Some(match (form, shape.forms) {
            (Some(egg), Some(forms)) => forms[usize::from(!egg)],
            _ => shape.footprint,
        })
    }

    /// The entities (positions in `zoo.entities`) of the units the map index holds on tile `t`.
    pub(crate) fn units_on(&self, t: usize) -> impl Iterator<Item = usize> + '_ {
        self.scene.units_on(t).iter().map(|&o| self.owner[o])
    }

    /// Puts the unit at position `at` of the entity list at a new point and facing, as the move of
    /// a step does; the map index follows and the picture is due.
    pub(crate) fn place_unit(&mut self, at: usize, point: (i32, i32, i32), rotation: u32) {
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

    /// The exhibit the entity at position `at` stands in, if it stands in one.
    pub(crate) fn exhibit_of(&self, at: usize) -> Option<u32> {
        let c = self.zoo.entities.get(at)?.common.as_ref()?;
        let t = self.tile_index(c.x.div_euclid(64), c.y.div_euclid(64))?;
        let x = self.exhibits.region(t)?;
        self.exhibits.get(x).map(|_| x)
    }

    /// Notes for the show lists that the animal at position `at` came onto the map (`on`) or is
    /// about to leave it (shows 20.1); any other entity is no news.
    pub(crate) fn show_moved(&mut self, at: usize, on: bool) {
        let e = &self.zoo.entities[at];
        let Some(id) = e
            .common
            .as_ref()
            .map(|c| c.id)
            .filter(|_| e.class == "animals")
        else {
            return;
        };
        let m = match on {
            true => crate::show::Move::On(id),
            false => crate::show::Move::Off {
                id,
                type_name: e.type_name.clone(),
                subtype: e.subtype.clone(),
                exhibit: self.exhibit_of(at),
            },
        };
        self.show_moves.push(m);
    }

    /// The moves the show lists have not seen yet.
    pub(crate) fn take_moves(&mut self) -> Vec<crate::show::Move> {
        std::mem::take(&mut self.show_moves)
    }

    /// Sets the escape check flag of the exhibit that tile `t` belongs to (habitat rating 5.8);
    /// the zoo grounds have none.
    pub(crate) fn escape_check(&mut self, t: usize) {
        let x = self.exhibits.region(t);
        if let Some(e) = x.and_then(|x| self.exhibits.get_mut(x)) {
            e.rating.escape = true;
        }
    }

    /// Marks the exhibit that tile `t` belongs to for a new rating (something on it changed);
    /// the zoo grounds have none.
    pub(crate) fn mark_rating(&mut self, t: usize) {
        if let Some(r) = self.exhibits.region(t) {
            self.exhibits.mark(r);
        }
    }

    /// Sets the units left of the food entity at position `at`: its exhibit is rated again and the
    /// picture is due (animals-needs 5.1).
    pub(crate) fn set_food(&mut self, at: usize, left: i32) {
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
    pub(crate) fn is_carried(&self, id: u32) -> bool {
        self.units.get(id).is_some_and(|u| u.carried)
    }

    /// Whether an animal, guest or staff member stands on tile `t`.
    pub(crate) fn unit_on(&self, t: usize) -> bool {
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
    pub(crate) fn shape(&self, e: usize) -> Option<Shape> {
        self.shapes.get(e).copied().flatten()
    }

    /// The edge slot a fence or tank wall holds: its tile and the map direction (0, 2, 4 or 6).
    /// Every question about where a piece stands goes through here and the slot lookup
    /// ([`World::edge`]), not through the piece's own position.
    pub(crate) fn slot(&self, e: usize) -> Option<(usize, usize)> {
        let o = self.owner.iter().position(|&owner| owner == e)?;
        let obj = &self.scene.objects[o];
        let t = self.scene.tile_at(obj.x, obj.y)?;
        let d = obj.facing as usize;
        (self.scene.edge_object(t, d) == Some(o)).then_some((t, d))
    }

    /// The own tile and the footprint tiles of the object at position `e` of the entity list
    /// (guest buildings 11.1.1); `None` for an entity the map index does not hold.
    pub(crate) fn footprint(&self, e: usize) -> Option<(Option<usize>, Vec<usize>)> {
        let o = self.owner.iter().position(|&owner| owner == e)?;
        let obj = &self.scene.objects[o];
        Some((
            self.scene.tile_at(obj.x, obj.y),
            self.scene.footprint_tiles(o),
        ))
    }

    /// The position of the entity with this id.
    pub(crate) fn find(&self, id: u32) -> Option<usize> {
        self.zoo
            .entities
            .iter()
            .position(|e| e.common.as_ref().is_some_and(|c| c.id == id))
    }

    /// Whether anything changed since the last call, and clears the mark: the picture is due.
    pub(crate) fn take_dirty(&mut self) -> bool {
        std::mem::take(&mut self.dirty)
    }

    fn record(&mut self, action: Action) {
        if std::mem::take(&mut self.fresh) {
            self.undo.clear();
        }
        self.undo.push(action);
    }

    /// The world was edited; `shift` tells how the entity list moved since the last rebuild.
    fn changed(&mut self, shift: Option<Shift>) {
        let held = self.held_units(shift);
        let paths = self.path_tiles();
        (self.scene, self.owner) = index(&self.zoo, &self.shapes);
        self.follow_paths(&paths);
        self.hold_again(held);
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
            if !matches!(shape.kind, Kind::Overlay | Kind::Flight)
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
                self.zoo.tiles[n].edges = edges::edge_states(&self.zoo, n);
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

#[cfg(test)]
mod tests;
