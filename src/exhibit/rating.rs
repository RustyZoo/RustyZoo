//! How an exhibit suits the species in it (habitat rating 3 to 9): the part scores of a species
//! record, the suitability an animal shows, the recommendation bits, the species list and the
//! timers that decide when they are computed again.
//!
//! Everything here is pure. The clock (the step in milliseconds) and the random draw are
//! parameters: the world manager calls [`State::update`] for every exhibit at each step with the
//! simulation's generator, and [`input`] reads an exhibit of a [`World`] into the plain
//! description the rules work on. Land exhibits and tanks are rated with their amphibious
//! neighbours (5.3, 9.2) and a tank with its show neighbours (5.1 point 4, 9.2).

use std::collections::{HashMap, HashSet};

use crate::build::number;
use crate::config::to_int;
use crate::entity::{Catalogue, EntityType};
use crate::exhibit::{EXTERIOR, GROUNDS, Tank};
use crate::unit::{Kind, Part};
use crate::world::World;

/// The terrain types a species rates; fresh and salt water among them.
const TERRAINS: usize = 18;
const FRESH: usize = 9;
const SALT: usize = 10;
/// The habitat number of an object that gives none ("non-habitat").
const NO_HABITAT: i32 = 9410;
/// The species timer passes this to revise the species list, the rating timer that to rate again.
const SPECIES_AFTER: u32 = 7999;
const RATING_AFTER: u32 = 6999;

/// Recommendation bits (rule 5.6): the animal panel shows the text 9901 plus the bit number.
pub mod bit {
    pub const TOO_SHALLOW: u32 = 8;
    pub const TOO_DEEP: u32 = 9;
    /// The tank's water is not the species' (salinity).
    pub const SALINITY: u32 = 11;
    pub const FEW_TREES: u32 = 12;
    pub const MANY_TREES: u32 = 13;
    pub const TOO_FLAT: u32 = 17;
    pub const TOO_STEEP: u32 = 18;
    pub const FEW_ROCKS: u32 = 19;
    pub const MANY_ROCKS: u32 = 20;
    /// Bit of "not enough" of terrain `k`; "too much" is the next one.
    pub fn terrain(k: usize) -> u32 {
        21 + 2 * k as u32
    }
    pub const DIRTY_WATER: u32 = 55;
    pub const UNUSED_SHELTER: u32 = 57;
    pub const NEEDS_SHELTER: u32 = 58;
    pub const UNUSED_TOYS: u32 = 59;
    pub const NEEDS_TOYS: u32 = 60;
    pub const STINK: u32 = 61;
    /// An amphibious animal in a tank with no land beside it.
    pub const NO_LAND: u32 = 63;
}

/// What the rating reads of an animal type's file.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Likes {
    /// The species key: the type name. Records are kept per key; the list per key and subtype.
    pub key: String,
    pub subtype: String,
    /// The type has an ambient sound (`cAmbients`).
    pub ambient: bool,
    /// `cCompatibleTerrain`: the wanted percentage of each terrain type, 0 when absent.
    pub terrain: [i32; TERRAINS],
    /// `cSuitableObjects`: pairs of an id (habitat number or name id) and a value.
    pub objects: Vec<(i32, i32)>,
    pub trees: i32,
    pub rocks: i32,
    pub elevation: i32,
    pub shelter: bool,
    pub toys: bool,
    /// `cDepthMin`, `cDepthMax`.
    pub depth: (i32, i32),
    /// `cUnderwater` 1 and `cOnlyUnderwater` 0 (tank fauna 2).
    pub amphibious: bool,
    pub attractiveness: i32,
    /// `cNameID`, `cGenus` and `cFamily`, and `cCompatibleAnimals`: pairs of such an id and a
    /// value.
    pub ids: [i32; 3],
    pub compatible: Vec<(i32, i32)>,
    /// `cBashStrength` above 0 (habitat rating 12).
    pub bash: bool,
    /// Found on a show neighbour of an amphibious neighbour: a record, never in the surrounding
    /// list (5.1 point 4, 9.2).
    pub via: bool,
}

/// The value of an id in a list of pairs: the last pair that names it, 0 when none does.
fn lookup(pairs: &[(i32, i32)], id: i32) -> i64 {
    (pairs.iter().rev())
        .find(|p| p.0 == id)
        .map_or(0, |p| i64::from(p.1))
}

impl Likes {
    fn value(&self, id: i32) -> i64 {
        lookup(&self.objects, id)
    }

    /// Whether salt water suits the species (`isRightSalinity`, tank fauna 11): it wants salt
    /// water (terrain 10) at least as much as fresh water (terrain 9).
    pub fn salt(&self) -> bool {
        self.terrain[SALT] >= self.terrain[FRESH]
    }

    /// The likes of an animal type. An integer is looked up in the sections of the subtype, of
    /// the default subtype, then in the plain ones (entity types 6.2); the lists are plain.
    pub fn of(ty: &EntityType) -> Likes {
        let Some(config) = ty.config.as_deref() else {
            return Likes::default();
        };
        let text = |b: &[u8]| String::from_utf8_lossy(b).into_owned();
        let default = config
            .section("Global")
            .and_then(|g| g.get("DefaultSubtype"))
            .unwrap_or_default();
        let int = |key: &str, fallback: i32| {
            [&ty.subtype[..], default, b""]
                .into_iter()
                .find_map(|prefix| {
                    let name = if prefix.is_empty() {
                        b"Characteristics/Integers".to_vec()
                    } else {
                        [prefix, b"/Characteristics/Integers"].concat()
                    };
                    config.section(name)?.get(key).and_then(to_int)
                })
                .unwrap_or(fallback)
        };
        let pairs = |list: &str| -> Vec<(i32, i32)> {
            let values = config.section(list).map_or(&[][..], |s| s.get_all("v"));
            let numbers: Vec<i32> = values.iter().map(|v| to_int(v).unwrap_or(0)).collect();
            numbers
                .as_chunks::<2>()
                .0
                .iter()
                .map(|&[k, v]| (k, v))
                .collect()
        };
        let mut terrain = [0; TERRAINS];
        for (k, w) in pairs("cCompatibleTerrain") {
            if let Some(slot) = usize::try_from(k).ok().and_then(|k| terrain.get_mut(k)) {
                *slot = w;
            }
        }
        let ambient = [&ty.subtype[..], default, b""].into_iter().any(|prefix| {
            let name = [
                prefix,
                if prefix.is_empty() { b"" } else { b"/" },
                b"Characteristics/Strings",
            ]
            .concat();
            config
                .section(name)
                .and_then(|s| s.get("cAmbients"))
                .is_some_and(|v| !v.is_empty())
        });
        Likes {
            key: text(&ty.type_name),
            subtype: text(&ty.subtype),
            ambient,
            terrain,
            objects: pairs("cSuitableObjects"),
            trees: int("cTreePref", 0),
            rocks: int("cRockPref", 0),
            elevation: int("cElevationPref", 0),
            shelter: int("cNeedShelter", 0) == 1,
            toys: int("cNeedToys", 0) == 1,
            depth: (int("cDepthMin", 0), int("cDepthMax", 0)),
            amphibious: int("cUnderwater", 0) == 1 && int("cOnlyUnderwater", 0) == 0,
            attractiveness: int("cAttractiveness", 1),
            ids: [ty.name_id, int("cGenus", 0), int("cFamily", 0)],
            compatible: pairs("cCompatibleAnimals"),
            bash: int("cBashStrength", 0) > 0,
            via: false,
        }
    }
}

/// An object (or a building) that holds slots in the exhibit.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Item {
    pub habitat: i32,
    pub name: i32,
    /// Its type as `class/type/subtype`; empty for a type that is not found.
    pub kind: String,
    /// The slots it covers in all (`S`).
    pub slots: i32,
    pub foliage: bool,
    pub rock: bool,
    /// `cStink` above 0.
    pub stink: bool,
    /// The room of a shelter and of a toy, for a building that is one or the other.
    pub rooms: Option<(i32, i32)>,
}

impl Item {
    /// Scenery in the sense of habitat rating 14: an object, building, food or filter type, but
    /// not one of the two dung types.
    fn scenery(&self) -> bool {
        let class = self.kind.split('/').next();
        matches!(class, Some("objects" | "building" | "food" | "filter"))
            && !matches!(
                self.kind.as_str(),
                "objects/other/poo" | "objects/other/dinopoo"
            )
    }
}

/// A tank's water, for the depth, water quality and salinity parts.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Water {
    pub level: i32,
    pub purity: i32,
    pub show: bool,
    pub salt: bool,
    pub filled: bool,
}

/// The tiles of an exhibit as the rating reads them (rule 5.2).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Site {
    /// `N`: the tiles of the exhibit.
    pub tiles: usize,
    /// The tiles of each terrain type that hold no path.
    pub terrain: [u32; TERRAINS],
    /// The habitat number and name id of each path tile.
    pub paths: Vec<(i32, i32)>,
    pub items: Vec<Item>,
    /// Per tile, the item of each slot it holds (one entry per held slot).
    pub slots: Vec<Vec<usize>>,
    /// The buildings that are shelters or toys, once each, in the order the tiles find them.
    pub listed: Vec<usize>,
    /// `H`: the summed hilliness.
    pub hills: i32,
    /// The tiles that hold something that stinks.
    pub stinky: u32,
    pub water: Option<Water>,
}

/// The scenery of a site that the species dislikes (habitat rating 14), as name ids of its types
/// in tile order, each type once: with `important` the path type first (all paths make one entry)
/// and the objects worth less than -5 to the species, else those worth -5 to -1.
pub fn disliked(site: &Site, likes: &Likes, important: bool) -> Vec<i32> {
    let mut names: Vec<i32> = site
        .paths
        .first()
        .filter(|_| important)
        .map(|p| p.1)
        .into_iter()
        .collect();
    let mut seen = HashSet::new();
    for it in site
        .slots
        .iter()
        .flatten()
        .filter_map(|&s| site.items.get(s))
    {
        let v = likes.value(it.habitat) + likes.value(it.name);
        let hit = if important { v < -5 } else { -6 < v && v < 0 };
        if hit && it.scenery() && seen.insert(&it.kind) {
            names.push(it.name);
        }
    }
    names
}

/// An exhibit as the rules read it: its tiles, the animal types on them in list order and the
/// type of each animal (an index into `species`) in the exhibit's animal order.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Input {
    pub site: Site,
    pub species: Vec<Likes>,
    pub animals: Vec<usize>,
    /// The first keeper-class staff member that holds the exhibit in his list (habitat rating 11.2).
    pub keeper: Option<u32>,
    /// The exhibit has an amphibious neighbour.
    pub linked: bool,
    /// The animal types of its amphibious neighbours, each once (5.1 point 4, 9.2), and those
    /// neighbours' tiles, a tank's with its water (5.3).
    pub near: Vec<Likes>,
    pub neighbours: Vec<Site>,
    /// The exhibit has a show neighbour, and the animal types of its show neighbours, each once
    /// (5.1 point 4, 9.2).
    pub show_linked: bool,
    pub show: Vec<Likes>,
}

/// The record of one species in an exhibit (rule 3): its part scores and the recommendation
/// bits, `important` marking the bits that are important.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Record {
    pub key: String,
    pub animals: u32,
    pub terrain: f32,
    pub objects: f32,
    pub foliage: f32,
    pub rocks: f32,
    pub elevation: f32,
    pub shelters: f32,
    pub toys: f32,
    pub depth: f32,
    pub water: f32,
    pub salinity: f32,
    pub tank: bool,
    pub show: bool,
    /// The exhibit has a show neighbour (6).
    pub show_linked: bool,
    pub amphibious: bool,
    /// What the pair rates (9.2, 9.4), the surrounding species list: an exhibit rates its own
    /// species, a land exhibit also the amphibious types of its tank, though both keep a record of
    /// every species of their neighbours.
    pub rated: bool,
    /// What the other species of the exhibit mean to this one (5.5.7); not part of the rating.
    pub other: f32,
    pub advice: u64,
    pub important: u64,
}

impl Record {
    /// The rating (rule 6), the terms added in the order written, in single precision.
    pub fn rating(&self) -> f32 {
        let rest = [
            self.foliage,
            self.rocks,
            self.elevation,
            self.shelters,
            self.toys,
        ];
        let mut sum = if self.tank {
            let sum = 0.15f32 * self.depth + 0.15f32 * self.water;
            let sum = sum + 0.1f32 * self.salinity;
            sum + 0.1f32 * self.objects
        } else {
            0.3f32 * self.terrain + 0.2f32 * self.objects
        };
        for part in rest {
            sum += 0.1f32 * part;
        }
        // A show tank loses 50 only without a show neighbour, any other tank always (6).
        if self.tank && self.amphibious && !(self.show && self.show_linked) {
            sum -= 50.0;
        }
        sum
    }

    /// `getSpeciesRating` (rule 6): the plain sum of the parts, the elevation left out in a tank.
    pub fn sum(&self) -> f32 {
        let (o, f, r) = (self.objects, self.foliage, self.rocks);
        let parts: &[f32] = if self.tank {
            &[
                self.depth,
                self.water,
                self.salinity,
                o,
                f,
                r,
                self.shelters,
                self.toys,
            ]
        } else {
            &[
                self.terrain,
                o,
                f,
                r,
                self.elevation,
                self.shelters,
                self.toys,
            ]
        };
        parts.iter().fold(0.0, |s, p| s + p)
    }

    /// What an animal shows as its suitability: `ceil(max(0, rating))`.
    pub fn shown(&self) -> u32 {
        shown(self.rating())
    }

    /// Whether recommendation bit `b` is set.
    pub fn has(&self, b: u32) -> bool {
        self.advice >> b & 1 == 1
    }

    fn set(&mut self, b: u32, important: bool) {
        self.advice |= 1 << b;
        self.important |= u64::from(important) << b;
    }

    fn clear(&mut self, b: u32) {
        self.advice &= !(1 << b);
        self.important &= !(1 << b);
    }
}

/// What an animal shows of a rating: `ceil(max(0, rating))`.
pub fn shown(rating: f32) -> u32 {
    rating.max(0.0).ceil() as u32
}

/// The rating an animal of species `key` sees in exhibit `x` (rule 6), `None` when the exhibit
/// has no record of it. An amphibious species in a tank that is not a show tank takes the best
/// of the ratings of the exhibit's amphibious `neighbours`, from -1000, when it has any.
/// `records` gives the records of an exhibit.
pub fn rating(
    x: u32,
    neighbours: &[u32],
    key: &str,
    records: &mut dyn FnMut(u32) -> Vec<Record>,
) -> Option<f32> {
    let find = |list: Vec<Record>| list.into_iter().find(|r| r.key == key);
    let own = find(records(x))?;
    if !(own.tank && !own.show && own.amphibious && !neighbours.is_empty()) {
        return Some(own.rating());
    }
    let best = (neighbours.iter()).filter_map(|&n| find(records(n)).map(|r| r.rating()));
    Some(best.fold(-1000.0, f32::max))
}

/// The draws of the simulation's generator.
pub type Rand<'a> = &'a mut dyn FnMut() -> u32;

/// Computes exhibit `x` again when it is dirty, as reading a part of it does (rule 4): the species
/// list is revised first when it must be (5.1 point 1), which draws for a type that came with an
/// ambient sound; the reachability test (5.8) draws last, all from `rand`.
pub fn refresh(world: &mut World, cat: &Catalogue, x: u32, rand: Rand) {
    if world.exhibits().get(x).is_some_and(|e| e.rating.dirty) {
        let input = input(world, cat, x);
        if let Some(e) = world.exhibits_mut().get_mut(x) {
            // The species list is revised first when it must be (5.1 point 1).
            if e.rating.revise {
                e.rating.revise_list(&input, rand);
            }
            e.rating.compute(&input);
        }
        finish(world, cat, x, rand);
    }
}

/// The end of the recomputation of exhibit `x`: a land exhibit has read the species lists of its
/// amphibious neighbours and a tank those of its show neighbours, which computes those that are
/// dirty first (9.2), then the reachability test runs (5.8).
pub fn finish(world: &mut World, cat: &Catalogue, x: u32, rand: Rand) {
    // The exhibit's animal list is emptied and refilled at the recomputation (5.1): its tiles in
    // the order of the last fill, then each tile's unit list.
    let list: Vec<u32> = (world.tiles_of(x).into_iter())
        .flat_map(|t| world.units_on(t))
        .filter_map(|i| Some(&world.zoo.entities[i]).filter(|e| e.class == "animals"))
        .filter_map(|e| e.common.as_ref().map(|c| c.id))
        .collect();
    let Some(e) = world.exhibits_mut().get_mut(x) else {
        return;
    };
    e.rating.animals = list;
    let Some(e) = world.exhibits().get(x) else {
        return;
    };
    // The lists read are those of the amphibious neighbours of a land exhibit and the show
    // neighbours of a tank.
    let read = if e.tank.is_none() {
        &e.amphibious
    } else {
        &e.show_near
    };
    for n in read.clone() {
        refresh(world, cat, n, rand);
    }
    reach(world, cat, x, rand);
}

thread_local! {
    /// The reachability tests since the last one that went on (5.8): one counter for the program,
    /// never saved in a zoo file and not reset by a load or a new game. The simulation runs on one
    /// thread; a test thread starts at 0.
    static TESTS: std::cell::Cell<u32> = const { std::cell::Cell::new(0) };
}

/// The reachability tests since the last one that went on (5.8).
pub fn tests() -> u32 {
    TESTS.get()
}

/// The reachability test of exhibit `x` (5.8): the program's counter goes up; with the escape
/// check flag set or the counter at 30, the counter and the flag are cleared, a target is drawn
/// among the candidate tiles of 10.2 that belong to no amphibious neighbour, and the last animal
/// of each type searches its way there with its test flag up (no diagonals). The types that get
/// there make the escape table, which the escape walk reads (animals 11.3).
pub fn reach(world: &mut World, cat: &Catalogue, x: u32, rand: Rand) {
    let tests = TESTS.get().saturating_add(1);
    TESTS.set(tests);
    let due = tests >= 30;
    let Some(e) = world
        .exhibits_mut()
        .get_mut(x)
        .filter(|e| e.rating.escape || due)
    else {
        return;
    };
    e.rating.escape = false;
    e.rating.escapes.clear();
    let (near, order) = (e.amphibious.clone(), e.order.clone());
    TESTS.set(0);
    let plan = crate::exhibit::viewing::Plan::of(world, x);
    let tiles: Vec<usize> = crate::exhibit::viewing::candidates(&plan)
        .into_iter()
        .filter(|&t| {
            world
                .exhibits()
                .region(t)
                .is_none_or(|r| !near.contains(&r))
        })
        .collect();
    if tiles.is_empty() {
        return;
    }
    let target = tiles[rand() as usize % tiles.len()];
    let water = world.tank_at(target).and_then(|t| world.tank(t));
    let level = world.zoo.tiles[target].elevation + water.map_or(0, |t| t.steps().1);
    let mut last: Vec<(&EntityType, usize)> = Vec::new();
    for (at, ty) in animals_at(world, cat, &order) {
        match last.iter_mut().find(|l| std::ptr::eq(l.0, ty)) {
            Some(l) => l.1 = at,
            None => last.push((ty, at)),
        }
    }
    let mut escapes = Vec::new();
    for (ty, at) in last {
        let c = world.zoo.entities[at].common.as_ref();
        let here = c.and_then(|c| world.tile_index(c.x.div_euclid(64), c.y.div_euclid(64)));
        let unit = c.and_then(|c| world.units.get(c.id));
        let (Some(here), Some(unit)) = (here, unit) else {
            continue;
        };
        let mut m = crate::unit::movement::mover(unit, world, at, here);
        (m.test, m.levels.1) = (true, level);
        let mut finder = std::mem::take(&mut world.units.finder);
        let reached = finder.test(world, &m, target);
        world.units.finder = finder;
        if reached {
            let text = |b: &[u8]| String::from_utf8_lossy(b).into_owned();
            escapes.push((
                format!("{}/{}", text(&ty.type_name), text(&ty.subtype)),
                target,
            ));
        }
    }
    if let Some(e) = world.exhibits_mut().get_mut(x) {
        e.rating.escapes = escapes;
    }
}

/// The sums of each species (`getSpeciesRating`) of each exhibit read by a pair (9.4), with the
/// species [`react`] compares there: `None` all of them, `Some(a)` those whose amphibious flag is `a`.
pub type Sums = Vec<(u32, Vec<(String, f32)>, Option<bool>)>;

/// The sum of each species in each exhibit of `tiles`, before a terrain change or an entity
/// placed or removed there (9.4): every exhibit is marked and its species list marked for
/// revision first, then the sums of each are read in turn through a recomputation (the list
/// revised first, the reachability test last, drawing from `rand`). With `grounds`, an entity's
/// change, the zoo grounds under `tiles` are marked and flagged too, and listed with no sums:
/// [`react`] marks them again and compares nothing.
pub fn before(
    world: &mut World,
    cat: &Catalogue,
    tiles: &[usize],
    grounds: bool,
    rand: Rand,
) -> Sums {
    let mut ids: Vec<u32> = Vec::new();
    for &t in tiles {
        let id = world
            .exhibits()
            .region(t)
            .filter(|&r| r != EXTERIOR && (grounds || r != GROUNDS));
        if let Some(id) = id.filter(|id| !ids.contains(id)) {
            ids.push(id);
        }
    }
    for &x in &ids {
        touch(world, x);
    }
    (ids.into_iter())
        .map(|x| (x, sums(world, cat, x, None, rand), None))
        .collect()
}

/// The sums before an entity is placed or removed at `tile` (9.4): its exhibit, or the zoo
/// grounds, as [`before`]. When that exhibit is a tank, its amphibious species are compared only
/// when it has no amphibious neighbour and `near` (an animal or a scenery piece, not a staff
/// member); with `near`, each of its neighbours is then read in turn (already marked with the
/// tank, not flagged), to read and compare its amphibious species only. The sums are one table
/// that each read empties first: every exhibit is compared against the last one read.
pub fn entity(
    world: &mut World,
    cat: &Catalogue,
    tile: Option<usize>,
    near: bool,
    rand: Rand,
) -> Sums {
    let mut all = before(world, cat, tile.as_slice(), true, rand);
    let tank = (all.first().and_then(|b| world.exhibits().get(b.0)))
        .filter(|e| e.tank.is_some())
        .map(|e| e.amphibious.clone());
    let Some(neighbours) = tank else { return all };
    if !neighbours.is_empty() || !near {
        all[0].2 = Some(false);
    }
    for x in neighbours.into_iter().filter(|_| near) {
        all.push((x, sums(world, cat, x, Some(true), rand), Some(true)));
    }
    let last = all.last().map(|b| b.1.clone()).unwrap_or_default();
    for b in &mut all {
        b.1.clone_from(&last);
    }
    all
}

/// After an entity is placed or removed at `tile`, the exhibit of its tile is looked up again
/// (9.4): a fence piece that closes, opens or splits an exhibit leaves the tile in another one, and
/// everything the after step reads comes from that one, as [`entity`] reads it from the exhibit
/// of the before: its tank test, its amphibious neighbours when `near`, its rates flag. Only the
/// table of sums read before stays (a species it lacks is an old sum of 0).
pub fn again(world: &World, before: &mut Sums, tile: Option<usize>, near: bool) {
    let x = tile.and_then(|t| world.exhibits().region(t));
    let (Some(x), Some(last)) = (x.filter(|&x| x != EXTERIOR), before.last()) else {
        return;
    };
    let table = last.1.clone();
    let tank = world.exhibits().get(x).filter(|e| e.tank.is_some());
    let neighbours = tank.map(|e| e.amphibious.clone());
    let rates = neighbours.as_ref().filter(|n| !n.is_empty() || !near);
    let mut all = vec![(x, table.clone(), rates.map(|_| false))];
    for n in neighbours.into_iter().flatten().filter(|_| near) {
        all.push((n, table.clone(), Some(true)));
    }
    *before = all;
}

/// The sum of each species of exhibit `x` that `rates` names (as [`Sums`]), read through a
/// recomputation.
fn sums(
    world: &mut World,
    cat: &Catalogue,
    x: u32,
    rates: Option<bool>,
    rand: Rand,
) -> Vec<(String, f32)> {
    refresh(world, cat, x, rand);
    (world.exhibits().get(x).into_iter())
        .flat_map(|e| &e.rating.records)
        .filter(|r| r.rated && rates.is_none_or(|a| r.amphibious == a))
        .map(|r| (r.key.clone(), r.sum()))
        .collect()
}

/// Marks exhibit `x`, or the zoo grounds, dirty and its species list for revision.
fn touch(world: &mut World, x: u32) {
    let exhibits = world.exhibits_mut();
    exhibits.mark(x);
    let state = match x {
        GROUNDS => Some(&mut exhibits.grounds),
        _ => exhibits.get_mut(x).map(|e| &mut e.rating),
    };
    if let Some(s) = state {
        (s.dirty, s.revise) = (true, true);
    }
}

/// Runs `edit`, a terrain change, then lets the animals of the exhibits of the tiles it changed
/// react (9.4): their sums before are taken from the tiles as they were.
pub fn terrain<T>(
    world: &mut World,
    cat: &Catalogue,
    rand: Rand,
    edit: impl FnOnce(&mut World) -> T,
) -> T {
    let old = world.zoo.tiles.clone();
    let done = edit(world);
    let changed: Vec<usize> = (0..old.len().min(world.zoo.tiles.len()))
        .filter(|&t| old[t] != world.zoo.tiles[t])
        .collect();
    if !changed.is_empty() {
        let new = std::mem::replace(&mut world.zoo.tiles, old);
        let sums = before(world, cat, &changed, false, rand);
        world.zoo.tiles = new;
        react(world, cat, sums, None, rand);
    }
    done
}

/// A new wall height of the filled tank `id` with a gate (habitat rating 4): the animals react
/// as to a terrain change at the gate's tile, with the water at the full level of the old height
/// (`from`) for the sums before and at that of the new height (`to`) for the sums after. The
/// level is then put back and the tank marked again.
pub fn depth(world: &mut World, cat: &Catalogue, rand: Rand, id: u32, from: u32, to: u32) {
    let Some((tile, level)) =
        (world.exhibits().get(id)).and_then(|e| Some((e.order.first().copied()?, e.tank?.level)))
    else {
        return;
    };
    let set = |world: &mut World, level| {
        if let Some(t) = (world.exhibits_mut().get_mut(id)).and_then(|e| e.tank.as_mut()) {
            t.level = level;
        }
    };
    set(world, from);
    let sums = before(world, cat, &[tile], false, rand);
    set(world, to);
    react(world, cat, sums, None, rand);
    set(world, level);
    world.exhibits_mut().mark(id);
}

/// What changed besides the ground, for the species whose sum stays the same (animals 10).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Change {
    /// An object placed (`sign` 1) or removed (-1), with its habitat number and name id.
    Object { habitat: i32, name: i32, sign: i64 },
    /// An animal placed (`sign` 1) or removed (-1), with its name id, genus and family: the
    /// compatible animals of the species decide, the newcomer's own species included.
    Animal { ids: [i32; 3], sign: i64 },
}

/// After the change (9.4, animals 10): each exhibit of `before` is marked, its species list is
/// marked for revision and the exhibit is rated again (the list is revised first, 5.1); the zoo
/// grounds are only marked and flagged. The animals (not eggs) of a species whose sum went up
/// queue a "likes" reaction, those of one whose sum went down a "dislikes" one; in a land exhibit
/// the animals of an amphibious species in its amphibious neighbours whose home it is react too.
/// Each exhibit compares the species its entry names. An old sum of 0 counts as unchanged; an
/// unchanged sum lets the `change`, an object placed or removed or an animal placed, decide by its
/// value to the species (with none, as for a staff member or a fence, an unchanged sum gives no
/// reaction). Logs `likes ID` or `dislikes ID`.
/// Once all exhibits are done, the approving sound plays if an animal liked the change and the
/// disapproving one if an animal disliked it (a species of eggs only reaches none).
pub fn react(world: &mut World, cat: &Catalogue, before: Sums, change: Option<Change>, rand: Rand) {
    let (mut liked_any, mut disliked_any) = (false, false);
    for (x, old, rates) in before {
        touch(world, x);
        if x == GROUNDS {
            continue;
        }
        let likes = input(world, cat, x);
        refresh(world, cat, x, rand);
        let Some(e) = world.exhibits().get(x) else {
            continue;
        };
        let near = match e.tank {
            Some(_) => Vec::new(),
            None => e.amphibious.clone(),
        };
        let mut moods: Vec<(String, bool, bool)> = Vec::new();
        for r in
            (e.rating.records.iter()).filter(|r| r.rated && rates.is_none_or(|a| r.amphibious == a))
        {
            let was = (old.iter()).find(|o| o.0 == r.key).map_or(0.0, |o| o.1);
            let l = (likes.species.iter().chain(&likes.near).chain(&likes.show))
                .find(|l| l.key == r.key);
            let by = match (was != 0.0 && was != r.sum(), change, l) {
                (true, ..) => r.sum() - was,
                (
                    false,
                    Some(Change::Object {
                        habitat,
                        name,
                        sign,
                    }),
                    Some(l),
                ) => (sign * (l.value(habitat) + l.value(name))) as f32,
                (false, Some(Change::Animal { ids, sign }), Some(l)) => {
                    (sign * ids.iter().map(|&id| lookup(&l.compatible, id)).sum::<i64>()) as f32
                }
                _ => 0.0,
            };
            if by != 0.0 {
                moods.push((r.key.clone(), by > 0.0, r.amphibious));
            }
        }
        for (key, liked, amphibious) in moods {
            // A neighbour's animals are reached when this land exhibit is their home.
            let l =
                (likes.species.iter().chain(&likes.near).chain(&likes.show)).find(|l| l.key == key);
            let home = |p: u32| l.and_then(|l| crate::animal::home::home(world, cat, p, l));
            let mut places = vec![x];
            places.extend(near.iter().filter(|&&p| amphibious && home(p) == Some(x)));
            places.iter().for_each(|&p| refresh(world, cat, p, rand));
            let ids: Vec<u32> = (places.into_iter())
                .flat_map(|p| crate::animal::exhibit_animals(world, p))
                .map(|at| &world.zoo.entities[at])
                .filter(|e| e.type_name == key)
                .filter_map(|e| e.common.as_ref().map(|c| c.id))
                .collect();
            for id in ids {
                if let Some(Part::Animal(a)) = world.units.get(id).map(|u| &u.part)
                    && !a.egg
                {
                    world.units.react(id, liked);
                    liked_any |= liked;
                    disliked_any |= !liked;
                }
            }
        }
    }
    if liked_any {
        world.units.cheer("smile");
    }
    if disliked_any {
        world.units.cheer("frown");
    }
}

/// The surrounding species list (9.2): the species list, then in a land exhibit the amphibious
/// types of its amphibious neighbours, then the types of its show neighbours. What a show
/// neighbour of an amphibious neighbour holds is in the records only.
fn surrounding(input: &Input) -> Vec<&Likes> {
    let land = input.site.water.is_none();
    let mut list: Vec<&Likes> = input.species.iter().collect();
    let amphibious = input.near.iter().filter(|l| land && l.amphibious && !l.via);
    for l in amphibious.chain(&input.show) {
        if !(list.iter()).any(|o| (&o.key, &o.subtype) == (&l.key, &l.subtype)) {
            list.push(l);
        }
    }
    list
}

/// Rates the exhibit for each species in it, in the order the species first appear, then for
/// those of its amphibious neighbours (rules 5.1 to 5.6). The fields that depend on the animals'
/// own state (happiness sums, hunger counts) are not part of the rating and not kept.
pub fn rate(input: &Input) -> Vec<Record> {
    let site = &input.site;
    let (tank, show) = (site.water.is_some(), site.water.is_some_and(|w| w.show));
    let all: Vec<&Likes> = (input.species.iter())
        .chain(&input.near)
        .chain(&input.show)
        .collect();
    let mut records: Vec<Record> = Vec::new();
    // The first type of each record, and the record of each type.
    let mut first: Vec<usize> = Vec::new();
    let mut of: Vec<usize> = Vec::new();
    let own = input.species.len();
    for (i, l) in all.iter().enumerate() {
        // What the pair rates: the exhibit's own species, a land exhibit the amphibious types of
        // its tank, a tank the types of its show neighbours (9.2).
        let rated = i < own || i >= own + input.near.len() || (l.amphibious && !tank && !l.via);
        let at = records.iter().position(|r| r.key == l.key);
        if let Some(r) = at {
            records[r].rated |= rated;
        } else {
            records.push(Record {
                key: l.key.clone(),
                tank,
                show,
                show_linked: input.show_linked,
                amphibious: l.amphibious,
                rated,
                ..Record::default()
            });
            first.push(i);
        }
        of.push(at.unwrap_or(records.len() - 1));
    }
    let of = &of[..input.species.len()];
    for &a in &input.animals {
        if let Some(&r) = of.get(a) {
            records[r].animals += 1;
        }
    }
    let around = surrounding(input);
    // The other animals (5.5.7): the types of another name id, by name id, genus and family.
    for (rec, &i) in records.iter_mut().zip(&first) {
        let l = all[i];
        let sum: i64 = (around.iter())
            .filter(|o| o.ids[0] != l.ids[0])
            .flat_map(|o| o.ids)
            .map(|id| lookup(&l.compatible, id))
            .sum();
        rec.other = sum as f32;
    }
    let n = site.tiles.max(1) as f32;
    let value = |l: &Likes, it: &Item| l.value(it.habitat) + l.value(it.name);
    let per_slot = |l: &Likes, it: &Item| 100 * value(l, it) / i64::from(it.slots.max(1));
    let (mut sums, mut rock_count, mut foliage_tiles) = (
        vec![0i64; records.len()],
        vec![0u32; records.len()],
        vec![0u32; records.len()],
    );
    // The tile pass (5.2): the paths, then the scenery.
    for (r, &i) in first.iter().enumerate() {
        for &(habitat, name) in &site.paths {
            sums[r] += all[i].value(habitat) + 25 * all[i].value(name);
        }
    }
    // The scenery of site `s` for the species of record `r`: each slot's share of the value,
    // the rocks and the tiles with foliage, those of a negative value only when `any`.
    let mut scenery = |r: usize, l: &Likes, s: &Site, any: bool| {
        for tile in &s.slots {
            let mut leafy = false;
            for it in tile.iter().filter_map(|&k| s.items.get(k)) {
                sums[r] += per_slot(l, it);
                if any || value(l, it) >= 0 {
                    rock_count[r] += u32::from(it.rock);
                    leafy |= it.foliage;
                }
            }
            foliage_tiles[r] += u32::from(leafy);
        }
    };
    for (r, &i) in first.iter().enumerate() {
        scenery(r, all[i], site, false);
    }
    // The neighbours (5.3): of a tank its tiles, the filled ones by water, and its murk, and in a
    // land exhibit its scenery for each amphibious species, whatever its value.
    let (mut fresh, mut salt, mut size, mut murk) = (0i64, 0i64, 0i64, 0f32);
    let count = |n: usize| n.min(u32::MAX as usize) as i64;
    for s in &input.neighbours {
        let Some(w) = s.water else { continue };
        let tiles = count(s.tiles);
        size += tiles;
        match (w.filled, w.salt) {
            (true, true) => salt += tiles,
            (true, false) => fresh += tiles,
            _ => {}
        }
        murk += (100 - i64::from(w.purity)).saturating_mul(tiles) as f32 * 0.01f32;
        for (r, &i) in first.iter().enumerate() {
            if !tank && all[i].amphibious {
                scenery(r, all[i], s, true);
            }
        }
    }
    // The claims of shelters and toys (5.4), the neighbours' joining the exhibit's (5.3); each
    // walk adds the building's value again (an original bug, kept).
    let listed: Vec<&Item> = (std::iter::once(site).chain(&input.neighbours))
        .flat_map(|s| s.listed.iter().map(|&b| &s.items[b]))
        .collect();
    let rooms: Vec<(i32, i32)> = (listed.iter())
        .map(|it| it.rooms.unwrap_or_default())
        .collect();
    let (mut shelter_room, mut toy_room): (Vec<i32>, Vec<i32>) = rooms.iter().copied().unzip();
    let (mut shelterless, mut toyless) = (vec![0i32; records.len()], vec![0i32; records.len()]);
    for &a in &input.animals {
        let Some(&r) = of.get(a) else { continue };
        let l = &input.species[a];
        for toys in [false, true] {
            let (need, room) = if toys {
                (l.toys, &mut toy_room)
            } else {
                (l.shelter, &mut shelter_room)
            };
            let mut claimed = false;
            for (k, &it) in listed.iter().enumerate() {
                sums[r] += per_slot(l, it);
                if need && value(l, it) >= 0 && room[k] != 0 {
                    room[k] -= 1;
                    claimed = true;
                    break;
                }
            }
            if need && !claimed {
                [&mut shelterless, &mut toyless][usize::from(toys)][r] += 1;
            }
        }
    }
    let total_shelterless: i32 = shelterless.iter().sum();
    let denominator = 100.0f32 * (0.025f32 * n).max(1.0);
    for (r, &i) in first.iter().enumerate() {
        let l = all[i];
        let rec = &mut records[r];
        let unused = |full: &[i32], left: &[i32]| {
            (listed.iter().enumerate())
                .filter(|&(k, &it)| full[k] > 0 && left[k] == full[k] && value(l, it) >= 0)
                .count() as i32
        };
        let u = unused(
            &rooms.iter().map(|p| p.0).collect::<Vec<_>>(),
            &shelter_room,
        );
        let t = if show {
            0
        } else {
            unused(&rooms.iter().map(|p| p.1).collect::<Vec<_>>(), &toy_room)
        };
        // The neighbour water an amphibious species counts (5.5.1): the tiles that would bring
        // the share of fresh or salt water to the one it wants, as far as the neighbours have.
        let mut extra = [0i64; TERRAINS];
        for (k, available) in [(FRESH, fresh), (SALT, salt)] {
            let w = i64::from(l.terrain[k]);
            if l.amphibious && (1..=99).contains(&w) {
                let x = (w * count(site.tiles) - 100 * i64::from(site.terrain[k])) / (100 - w);
                extra[k] = x.min(available).max(0);
            }
        }
        let d = (count(site.tiles) + extra[FRESH] + extra[SALT]).max(1) as f32;
        // Terrain (5.5.1) and its bits (5.6), land only; terrain 17's pair is bits 55 and 56.
        let mut sum = 0.0f32;
        for (k, more_water) in extra.into_iter().enumerate() {
            let (w, count) = (l.terrain[k], site.terrain[k]);
            let (wanted, share) = (w as f32, (count as i64 + more_water) as f32 * 100.0 / d);
            sum += if w < 0 {
                if count > 0 { wanted } else { 0.0 }
            } else {
                wanted.min(share)
            };
            let (more, less) = if w < 0 {
                (count > 0, false)
            } else {
                (wanted < share - 3.0, share + 3.0 < wanted)
            };
            if !tank && (more || less) {
                rec.set(
                    bit::terrain(k) + u32::from(more),
                    w >= 0 && (share - wanted).abs() > 6.0,
                );
            }
        }
        rec.terrain = if tank { 100.0 } else { sum.min(100.0) };
        // Objects (5.5.2).
        let mut objects = (sums[r] as f32 / denominator).min(100.0);
        if objects < 0.0 {
            objects *= 50.0;
        }
        rec.objects = objects;
        // Foliage, rocks and elevation (5.5.3, 5.5.4): an amphibious species counts all the
        // neighbours' water, not its extras.
        let water = if l.amphibious { fresh + salt } else { 0 };
        let d = (count(site.tiles) + water).max(1) as f32;
        let tree = foliage_tiles[r] as f32 * 100.0 / d;
        let rock = rock_count[r] as f32 * 100.0 / (4.0 * d);
        let hill = site.hills as f32 * 50.0 / n;
        for (m, p, weight, bits) in [
            (tree, l.trees as f32, 8.0, (bit::FEW_TREES, bit::MANY_TREES)),
            (rock, l.rocks as f32, 6.0, (bit::FEW_ROCKS, bit::MANY_ROCKS)),
        ] {
            let few = m < p - 4.0 || (m == 0.0 && p > 0.0);
            if few || m > p + 4.0 {
                rec.set(if few { bits.0 } else { bits.1 }, (m - p).abs() > 8.0);
            }
            let score = 100.0 - weight * (m - p).abs().clamp(0.0, 50.0);
            if bits.0 == bit::FEW_TREES {
                rec.foliage = score
            } else {
                rec.rocks = score
            }
        }
        let p = l.elevation as f32;
        if hill < p - 10.0 || hill > p + 10.0 {
            rec.set(
                if hill < p {
                    bit::TOO_FLAT
                } else {
                    bit::TOO_STEEP
                },
                (hill - p).abs() > 20.0,
            );
        }
        rec.elevation = 100.0 - (hill - p).abs().clamp(0.0, 50.0);
        // An amphibious species in a tank with no land beside it (5.5.4, 5.6).
        if tank && l.amphibious && !input.linked {
            for b in [
                bit::FEW_TREES,
                bit::MANY_TREES,
                bit::TOO_FLAT,
                bit::TOO_STEEP,
                bit::FEW_ROCKS,
                bit::MANY_ROCKS,
            ] {
                rec.clear(b);
            }
            rec.elevation = 100.0;
            rec.set(bit::NO_LAND, true);
        }
        // Shelters (5.5.5) and toys (5.5.6).
        let (s, w) = (shelterless[r], toyless[r]);
        let penalty = match (s, u) {
            (1..=3, _) => 0,
            (4..=6, _) => 15 * s,
            (7.., _) => 100 * total_shelterless - 600,
            (_, 3..) => (50 * u + 100) * (u - 3).max(1),
            _ => 0,
        };
        rec.shelters = (100 - penalty).min(100) as f32;
        for (flag, on) in [(bit::NEEDS_SHELTER, s > 0), (bit::UNUSED_SHELTER, u > 0)] {
            if on {
                rec.set(flag, u > 3 || s > 7);
            }
        }
        let penalty = match (w, t) {
            (4.., _) => 15 * w,
            (1..=3, _) => 0,
            (_, 3..) => (50 * t + 100) * (t - 2),
            _ => 0,
        };
        rec.toys = (100 - penalty).min(100) as f32;
        for (flag, on) in [(bit::NEEDS_TOYS, w > 0), (bit::UNUSED_TOYS, t > 1)] {
            if on {
                rec.set(flag, t > 2);
            }
        }
        // Depth, water quality and salinity (5.5.8).
        (rec.depth, rec.water, rec.salinity) = (100.0, 100.0, 100.0);
        if let Some(water) = site.water {
            let (a, b) = (i64::from(l.depth.0), i64::from(l.depth.1));
            let level = i64::from(water.level);
            let (d, flag) = match (a != b, level < a, level > b) {
                (true, true, _) => (a - level, bit::TOO_SHALLOW),
                (true, _, true) => (level - b, bit::TOO_DEEP),
                _ => (0, 0),
            };
            let p = 20 * d.clamp(0, 30);
            if d > 0 {
                rec.set(flag, p >= 50);
            }
            rec.depth = (100 - p).min(100) as f32;
            rec.water = water.purity as f32;
            if water.salt != l.salt() {
                rec.salinity = -400.0;
                rec.set(bit::SALINITY, true);
            }
        } else if l.amphibious && size > 0 {
            rec.water = 100.0 - murk / size as f32 * 100.0;
        }
        // The water quality bit is written again after the terrain pairs.
        rec.clear(bit::DIRTY_WATER);
        if rec.water < 50.0 {
            rec.set(bit::DIRTY_WATER, rec.water < 25.0);
        }
        // Stink (5.2).
        let stink = site.stinky as f32 / n;
        if stink > 0.05 {
            rec.set(bit::STINK, stink > 0.1);
        }
    }
    records
}

/// What an exhibit keeps around its rating: the timers, the records, the species list and the
/// sounds of its species, the attractiveness, the keeper and the service.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct State {
    /// The records must be computed again; the species list must be revised.
    pub dirty: bool,
    pub revise: bool,
    /// Milliseconds since the species list was revised and since the records were computed.
    pub species_ms: u32,
    pub rating_ms: u32,
    pub records: Vec<Record>,
    /// The exhibit's animal list (5.1): the ids of its animals as the last recomputation left
    /// them, or in name order once the exhibit panel sorted it. Not saved.
    pub animals: Vec<u32>,
    /// The species list: `key/subtype` of each type on the tiles, as last revised.
    pub types: Vec<String>,
    /// The tile (index into the exhibit's tile list) of each species' ambient sound.
    pub sounds: Vec<(String, usize)>,
    /// The sum of `cAttractiveness` of the animals.
    pub attractiveness: i32,
    /// The entity id of the keeper assigned to the exhibit.
    pub keeper: Option<u32>,
    /// The AI clock, in ticks, when a keeper last finished his duties in the exhibit (staff 7.2);
    /// 0 for never.
    pub serviced: u64,
    /// The keepers on their way to the exhibit (staff 2).
    pub coming: u32,
    /// The escape check flag (5.8): the next reachability test runs.
    pub escape: bool,
    /// The species and the animals the last recomputation counted (5.1 point 5).
    pub counts: (usize, usize),
    /// The animal count fell to 0 at a recomputation: the step posts the call for the leftovers.
    pub emptied: bool,
    /// The surrounding species list holds a species with `cBashStrength` above 0 (12).
    pub bash: bool,
    /// The escape table the last reachability test wrote, for the escapes (M7): the animal
    /// types (`key/subtype`) that reach the target, and the target tile.
    pub escapes: Vec<(String, usize)>,
}

impl State {
    /// An exhibit that was just made: dirty, its escape check flag set, both timers at 0.
    pub fn new() -> State {
        State {
            dirty: true,
            escape: true,
            ..State::default()
        }
    }

    /// One step of the exhibit update (rule 4): `ms` is added to both timers; a timer past its
    /// limit marks the list or the records, then the list is revised and the records computed,
    /// each restarting its timer at `rand() % 200`. `rand` is the simulation's generator;
    /// `input` is called only when something is due. Whether the records were computed.
    pub fn update(
        &mut self,
        ms: u32,
        rand: &mut dyn FnMut() -> u32,
        input: impl FnOnce() -> Input,
    ) -> bool {
        self.species_ms = self.species_ms.saturating_add(ms);
        self.rating_ms = self.rating_ms.saturating_add(ms);
        self.revise |= self.species_ms > SPECIES_AFTER;
        self.dirty |= self.rating_ms > RATING_AFTER;
        if !(self.revise || self.dirty) {
            return false;
        }
        let input = input();
        if self.revise {
            self.species_ms = rand() % 200;
            self.revise_list(&input, rand);
        }
        let dirty = self.dirty;
        if dirty {
            self.rating_ms = rand() % 200;
            self.compute(&input);
        }
        dirty
    }

    /// Revises the species list (rule 9.1): a type that left loses its sound, a type that came
    /// and has an ambient sound gets one at a tile picked with `rand() % tiles`.
    pub fn revise_list(&mut self, input: &Input, rand: &mut dyn FnMut() -> u32) {
        self.revise = false;
        let now: Vec<String> = input
            .species
            .iter()
            .map(|l| format!("{}/{}", l.key, l.subtype))
            .collect();
        self.sounds.retain(|(t, _)| now.contains(t));
        for (l, t) in input.species.iter().zip(&now) {
            if l.ambient && input.site.tiles > 0 && !self.types.contains(t) {
                self.sounds
                    .push((t.clone(), rand() as usize % input.site.tiles));
            }
        }
        self.types = now;
    }

    /// Computes the records and the attractiveness again; reading a part of a dirty exhibit
    /// does this first, without touching the timers.
    pub fn compute(&mut self, input: &Input) {
        self.dirty = false;
        self.records = rate(input);
        // A change of the species or the animals counted sets the escape check flag (5.1).
        let counts = (self.records.len(), input.animals.len());
        self.escape |= counts != self.counts;
        self.emptied |= counts.1 == 0 && self.counts.1 != 0;
        self.counts = counts;
        self.bash = surrounding(input).iter().any(|l| l.bash);
        self.keeper = input.keeper;
        self.attractiveness = (input.animals.iter())
            .filter_map(|&a| input.species.get(a))
            .map(|l| l.attractiveness)
            .sum();
    }
}

/// Whether the pair of an entity of type `ty` reads the neighbours of a tank ([`entity`]): an
/// animal or a scenery piece (an object, a fence or tank wall piece, a path, a building), not a
/// staff member or a guest.
pub fn reads_neighbours(ty: &EntityType) -> bool {
    use crate::render::order::Kind;
    ty.class == "animals" || Kind::of_class(ty.class) != Kind::Unit
}

/// The habitat number and name id of an object type (habitat rating 2).
pub fn object(ty: &EntityType) -> (i32, i32) {
    let it = item(Some(ty), 1);
    (it.habitat, it.name)
}

/// The item an object type makes; a type that is not found makes the default one.
fn item(ty: Option<&EntityType>, slots: i32) -> Item {
    let int = |key: &str| ty.and_then(|ty| number(ty, key)).map_or(0, |v| v as i32);
    let (shelter, toy) = (int("cShelter") == 1, int("cToySatisfaction").max(0));
    Item {
        habitat: ty
            .and_then(|ty| number(ty, "cHabitat"))
            .map_or(NO_HABITAT, |v| v as i32),
        name: ty.map_or(0, |ty| ty.name_id),
        kind: ty.map_or_else(String::new, |ty| {
            let text = |b: &[u8]| String::from_utf8_lossy(b).into_owned();
            format!("{}/{}/{}", ty.class, text(&ty.type_name), text(&ty.subtype))
        }),
        slots: slots.max(1),
        foliage: int("cFoliage") == 1,
        rock: int("cRock") == 1,
        stink: int("cStink") > 0,
        rooms: (shelter || toy > 0).then(|| (if shelter { int("cCapacity") } else { 0 }, toy)),
    }
}

/// Reads exhibit `id` of a world into the description the rules work on (rule 5.1 to 5.4).
pub fn input(world: &World, catalogue: &Catalogue, id: u32) -> Input {
    let Some(exhibit) = world.exhibits().get(id) else {
        return Input::default();
    };
    let (species, animals) = animals_on(world, catalogue, &exhibit.order);
    let keeper = (world.units.iter())
        .filter(|u| u.kind == Kind::Keeper)
        .find(|u| matches!(&u.part, Part::Staff(s) if s.assigned.contains(&id)))
        .map(|u| u.id);
    // The types and the tiles of the amphibious neighbours (5.1 point 4, 5.3).
    let (mut near, mut neighbours): (Vec<Likes>, Vec<Site>) = (Vec::new(), Vec::new());
    for n in (exhibit.amphibious.iter()).filter_map(|&n| world.exhibits().get(n)) {
        // Its species, then those of its show neighbours.
        let beside = (n.show_near.iter()).filter_map(|&s| world.exhibits().get(s));
        let listed = [(n, false)].into_iter().chain(beside.map(|s| (s, true)));
        for (from, via) in listed {
            for l in animals_on(world, catalogue, &from.order).0 {
                let seen = (near.iter_mut()).find(|o| (&o.key, &o.subtype) == (&l.key, &l.subtype));
                match seen {
                    // Met again on the tiles of a neighbour: no longer only a show neighbour's.
                    Some(o) => o.via &= via,
                    None => near.push(Likes { via, ..l }),
                }
            }
        }
        neighbours.push(site(world, catalogue, &n.order, n.tank.map(water)));
    }
    let mut show: Vec<Likes> = Vec::new();
    for s in (exhibit.show_near.iter()).filter_map(|&s| world.exhibits().get(s)) {
        for l in animals_on(world, catalogue, &s.order).0 {
            if !show
                .iter()
                .any(|o| (&o.key, &o.subtype) == (&l.key, &l.subtype))
            {
                show.push(l);
            }
        }
    }
    Input {
        site: site(world, catalogue, &exhibit.order, exhibit.tank.map(water)),
        species,
        animals,
        keeper,
        linked: !exhibit.amphibious.is_empty(),
        near,
        neighbours,
        show_linked: !exhibit.show_near.is_empty(),
        show,
    }
}

/// The tiles `order` of an exhibit as the tile pass reads them (5.2).
fn site(world: &World, catalogue: &Catalogue, order: &[usize], water: Option<Water>) -> Site {
    let bytes = |s: &str| s.chars().map(|c| c as u8).collect::<Vec<u8>>();
    let kind = |e: usize| {
        let e = &world.zoo.entities[e];
        catalogue.find(&e.class, &bytes(&e.type_name), &bytes(&e.subtype))
    };
    let mut site = Site {
        tiles: order.len(),
        water,
        ..Site::default()
    };
    let mut items: HashMap<usize, usize> = HashMap::new();
    let mut inside = vec![false; world.zoo.tiles.len()];
    order.iter().for_each(|&t| inside[t] = true);
    for &t in order {
        let tile = &world.zoo.tiles[t];
        match world.path(t) {
            Some(p) => {
                let it = item(kind(p), 1);
                site.paths.push((it.habitat, it.name));
            }
            None => site.terrain[usize::from(tile.terrain) % TERRAINS] += 1,
        }
        let mut held = Vec::new();
        let mut stinks = false;
        for q in 0..4 {
            let Some(e) = world.quadrant(t, q) else {
                continue;
            };
            let at = match items.get(&e) {
                Some(&at) => at,
                None => {
                    let slots = world
                        .shape(e)
                        .map_or(1, |s| s.footprint.0.saturating_mul(s.footprint.1));
                    let item = item(kind(e), slots);
                    site.items.push(item);
                    items.insert(e, site.items.len() - 1);
                    site.items.len() - 1
                }
            };
            let item = &site.items[at];
            stinks |= item.stink;
            if item.rooms.is_some() && !site.listed.contains(&at) {
                site.listed.push(at);
            }
            held.push(at);
        }
        site.slots.push(held);
        site.stinky += u32::from(stinks);
        // Hilliness (5.2): the corner spread, and the cliffs on a side that stays in the exhibit.
        let corners = (0..4).map(|c| i32::from(tile.shape >> (2 * c) & 3));
        site.hills += corners.clone().max().unwrap_or(0) - corners.min().unwrap_or(0);
        site.hills += [0, 2, 4, 6]
            .into_iter()
            .filter(|&d| world.cliff(t, d) && world.neighbour(t, d).is_some_and(|n| inside[n]))
            .count() as i32;
    }
    site
}

fn water(t: Tank) -> Water {
    Water {
        level: t.level as i32,
        purity: t.purity as i32,
        show: t.show,
        salt: t.salt,
        filled: t.filled,
    }
}

/// The animal types on the tiles `order`, each once in the order they first appear tile by
/// tile, and the type of each animal.
fn animals_on(world: &World, catalogue: &Catalogue, order: &[usize]) -> (Vec<Likes>, Vec<usize>) {
    let (mut species, mut animals): (Vec<Likes>, Vec<usize>) = (Vec::new(), Vec::new());
    for (_, ty) in animals_at(world, catalogue, order) {
        let mut likes = Likes::of(ty);
        // The bears' cheat gives the grizzly a bash strength (cheats 4.1).
        likes.bash |= world.units.cheat.bears_bash && likes.ids[0] == crate::cheat::GRIZZLY;
        let at = (species.iter())
            .position(|s| (&s.key, &s.subtype) == (&likes.key, &likes.subtype))
            .unwrap_or_else(|| {
                species.push(likes);
                species.len() - 1
            });
        animals.push(at);
    }
    (species, animals)
}

/// The animals whose own tile is one of `order` (5.1 point 3), tile by tile in that order: the
/// place of each in the entity list and its type.
pub(crate) fn animals_at<'c>(
    world: &World,
    catalogue: &'c Catalogue,
    order: &[usize],
) -> Vec<(usize, &'c EntityType)> {
    let bytes = |s: &str| s.chars().map(|c| c as u8).collect::<Vec<u8>>();
    let mut inside = vec![false; world.zoo.tiles.len()];
    order.iter().for_each(|&t| inside[t] = true);
    let width = world.zoo.width as i64;
    let mut on: HashMap<usize, Vec<(usize, &EntityType)>> = HashMap::new();
    for (at, entity) in world.zoo.entities.iter().enumerate() {
        let (Some(c), "animals") = (&entity.common, entity.class.as_str()) else {
            continue;
        };
        // A carried animal has left the tiles' lists, so it counts in no exhibit (8.2).
        if world.is_carried(c.id) {
            continue;
        }
        let (x, y) = (i64::from(c.x.div_euclid(64)), i64::from(c.y.div_euclid(64)));
        let t = (y >= 0 && x >= 0 && x < width).then(|| (y * width + x) as usize);
        let ty = catalogue.find(
            &entity.class,
            &bytes(&entity.type_name),
            &bytes(&entity.subtype),
        );
        if let Some((t, ty)) = t.filter(|&t| inside.get(t) == Some(&true)).zip(ty) {
            on.entry(t).or_default().push((at, ty));
        }
    }
    (order.iter())
        .flat_map(|t| on.remove(t).unwrap_or_default())
        .collect()
}
