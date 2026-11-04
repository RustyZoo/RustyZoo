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

mod habitat;
mod likes;
mod read;
pub(crate) use habitat::rate;
pub(crate) use likes::Likes;
use likes::lookup;
pub use read::input;
pub(crate) use read::{animals_at, object, reads_neighbours};

/// The terrain types a species rates; fresh and salt water among them.
const TERRAINS: usize = 18;
const FRESH: usize = 9;
const SALT: usize = 10;
/// The habitat number of an object that gives none ("non-habitat").
const NO_HABITAT: i32 = 9410;
/// The species timer passes this to revise the species list, the rating timer that to rate again.
/// The food kinds an exhibit sums the food of (5.1 point 2).
pub(crate) const FOODS: usize = 16;
const SPECIES_AFTER: u32 = 7999;
const RATING_AFTER: u32 = 6999;

/// Recommendation bits (rule 5.6): the animal panel shows the text 9901 plus the bit number.
pub mod bit {
    pub(crate) const TOO_SHALLOW: u32 = 8;
    pub(crate) const TOO_DEEP: u32 = 9;
    /// The tank's water is not the species' (salinity).
    pub(crate) const SALINITY: u32 = 11;
    pub(crate) const FEW_TREES: u32 = 12;
    pub(crate) const MANY_TREES: u32 = 13;
    pub(crate) const TOO_FLAT: u32 = 17;
    pub(crate) const TOO_STEEP: u32 = 18;
    pub(crate) const FEW_ROCKS: u32 = 19;
    pub(crate) const MANY_ROCKS: u32 = 20;
    /// Bit of "not enough" of terrain `k`; "too much" is the next one.
    pub(crate) fn terrain(k: usize) -> u32 {
        21 + 2 * k as u32
    }
    pub(crate) const DIRTY_WATER: u32 = 55;
    pub(crate) const UNUSED_SHELTER: u32 = 57;
    pub(crate) const NEEDS_SHELTER: u32 = 58;
    pub(crate) const UNUSED_TOYS: u32 = 59;
    pub(crate) const NEEDS_TOYS: u32 = 60;
    pub(crate) const STINK: u32 = 61;
    /// An amphibious animal in a tank with no land beside it.
    pub(crate) const NO_LAND: u32 = 63;
}

/// An object (or a building) that holds slots in the exhibit.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct Item {
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
pub(crate) struct Water {
    pub level: i32,
    pub purity: i32,
    pub show: bool,
    pub salt: bool,
    pub filled: bool,
}

/// The tiles of an exhibit as the rating reads them (rule 5.2).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct Site {
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
pub(crate) fn disliked(site: &Site, likes: &Likes, important: bool) -> Vec<i32> {
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
    pub(crate) site: Site,
    pub(crate) species: Vec<Likes>,
    pub animals: Vec<usize>,
    /// The sum of their happiness (5.1 point 3).
    pub happy: i32,
    /// The sick and the very unhappy animals among them, and the food amounts on the tiles by
    /// food kind (5.1 point 2).
    pub sick: u32,
    pub unhappy: u32,
    pub food: [i32; FOODS],
    /// The first keeper-class staff member that holds the exhibit in his list (habitat rating 11.2).
    pub keeper: Option<u32>,
    /// The exhibit has an amphibious neighbour.
    pub linked: bool,
    /// The animal types of its amphibious neighbours, each once (5.1 point 4, 9.2), and those
    /// neighbours' tiles, a tank's with its water (5.3).
    pub(crate) near: Vec<Likes>,
    pub(crate) neighbours: Vec<Site>,
    /// The exhibit has a show neighbour, and the animal types of its show neighbours, each once
    /// (5.1 point 4, 9.2).
    pub show_linked: bool,
    pub(crate) show: Vec<Likes>,
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
    pub(crate) fn sum(&self) -> f32 {
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
pub(crate) fn shown(rating: f32) -> u32 {
    rating.max(0.0).ceil() as u32
}

/// The rating an animal of species `key` sees in exhibit `x` (rule 6), `None` when the exhibit
/// has no record of it. An amphibious species in a tank that is not a show tank takes the best
/// of the ratings of the exhibit's amphibious `neighbours`, from -1000, when it has any.
/// `records` gives the records of an exhibit.
pub(crate) fn rating(
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
pub(crate) type Rand<'a> = &'a mut dyn FnMut() -> u32;

/// Computes exhibit `x` again when it is dirty, as reading a part of it does (rule 4): the species
/// list is revised first when it must be (5.1 point 1), which draws for a type that came with an
/// ambient sound; the reachability test (5.8) draws last, all from `rand`.
pub(crate) fn refresh(world: &mut World, cat: &Catalogue, x: u32, rand: Rand) {
    if world.exhibits().get(x).is_some_and(|e| e.rating.dirty) {
        let input = input(world, cat, x);
        let mut trio = false;
        if let Some(e) = world.exhibits_mut().get_mut(x) {
            // The species list is revised first when it must be (5.1 point 1).
            if e.rating.revise {
                e.rating.revise_list(&input, rand);
                trio = e.rating.trio;
            }
            e.rating.compute(&input);
        }
        world.units.cheat.lions_tigers_bears |= trio;
        finish(world, cat, x, rand);
    }
}

/// The end of the recomputation of exhibit `x`: a land exhibit has read the species lists of its
/// amphibious neighbours and a tank those of its show neighbours, which computes those that are
/// dirty first (9.2), then the reachability test runs (5.8).
pub(crate) fn finish(world: &mut World, cat: &Catalogue, x: u32, rand: Rand) {
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
    e.rating.animals = list.clone();
    if std::mem::take(&mut e.rating.first) {
        crate::show::counted(world, cat, x, &list);
    }
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
pub(crate) fn tests() -> u32 {
    TESTS.get()
}

/// The reachability test of exhibit `x` (5.8): the program's counter goes up; with the escape
/// check flag set or the counter at 30, the counter and the flag are cleared, a target is drawn
/// among the candidate tiles of 10.2 that belong to no amphibious neighbour, and the last animal
/// of each type searches its way there with its test flag up (no diagonals). The types that get
/// there make the escape table, which the escape walk reads (animals 11.3).
pub(crate) fn reach(world: &mut World, cat: &Catalogue, x: u32, rand: Rand) {
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
pub(crate) type Sums = Vec<(u32, Vec<(String, f32)>, Option<bool>)>;

/// The sum of each species in each exhibit of `tiles`, before a terrain change or an entity
/// placed or removed there (9.4): every exhibit is marked and its species list marked for
/// revision first, then the sums of each are read in turn through a recomputation (the list
/// revised first, the reachability test last, drawing from `rand`). With `grounds`, an entity's
/// change, the zoo grounds under `tiles` are marked and flagged too, and listed with no sums:
/// [`react`] marks them again and compares nothing.
pub(crate) fn before(
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
pub(crate) fn entity(
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
pub(crate) fn again(world: &World, before: &mut Sums, tile: Option<usize>, near: bool) {
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
pub(crate) fn terrain<T>(
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
pub(crate) fn depth(world: &mut World, cat: &Catalogue, rand: Rand, id: u32, from: u32, to: u32) {
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

/// What an exhibit keeps around its rating: the timers, the records, the species list and the
/// sounds of its species, the attractiveness, the keeper and the service.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct State {
    /// The records must be computed again; the species list must be revised.
    pub dirty: bool,
    pub revise: bool,
    /// The next recomputation is the exhibit's first, or the first since a tank merge: the
    /// performing animals it counts join the shows (shows 20.1). Not saved.
    pub first: bool,
    /// Milliseconds since the species list was revised and since the records were computed.
    pub species_ms: u32,
    pub rating_ms: u32,
    pub records: Vec<Record>,
    /// The exhibit's animal list (5.1): the ids of its animals as the last recomputation left
    /// them, or in name order once the exhibit panel sorted it. Not saved.
    pub animals: Vec<u32>,
    /// The species list: `key/subtype` of each type on the tiles, as last revised.
    pub types: Vec<String>,
    /// The last revision found a lion, a tiger and a grizzly bear (9.1 point 4).
    pub trio: bool,
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
    /// The exhibit's average happiness h as the last recomputation left it (5.1 point 6): the
    /// guests' viewing reads this, not the animals' happiness now. Not saved.
    pub happiness: i32,
    /// The sick and the very unhappy animals and the food by kind as the last recomputation left
    /// them (5.1 point 2): the health, other-animals and hunger checks read these. Not saved.
    pub sick: u32,
    pub unhappy: u32,
    pub food: [i32; FOODS],
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
            first: true,
            ..State::default()
        }
    }

    /// One step of the exhibit update (rule 4): `ms` is added to both timers; a timer past its
    /// limit marks the list or the records, then the list is revised and the records computed,
    /// each restarting its timer at `rand() % 200`. `rand` is the simulation's generator;
    /// `input` is called only when something is due. Whether the records were computed.
    pub(crate) fn update(
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
    pub(crate) fn revise_list(&mut self, input: &Input, rand: &mut dyn FnMut() -> u32) {
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
        let has = |id| input.species.iter().any(|l| l.ids[0] == id);
        self.trio = [
            crate::cheat::LION,
            crate::cheat::TIGER,
            crate::cheat::GRIZZLY,
        ]
        .into_iter()
        .all(has);
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
        self.happiness = input.happy.checked_div(counts.1 as i32).unwrap_or(0);
        (self.sick, self.unhappy, self.food) = (input.sick, input.unhappy, input.food);
        self.bash = surrounding(input).iter().any(|l| l.bash);
        self.keeper = input.keeper;
        self.attractiveness = (input.animals.iter())
            .filter_map(|&a| input.species.get(a))
            .map(|l| l.attractiveness)
            .sum();
    }
}
