//! How an exhibit suits the species in it (habitat rating 3 to 9): the part scores of a species
//! record, the suitability an animal shows, the recommendation bits, the species list and the
//! timers that decide when they are computed again.
//!
//! Everything here is pure. The clock (the step in milliseconds) and the random draw are
//! parameters: the world manager calls [`State::update`] for every exhibit at each step with the
//! simulation's generator, and [`input`] reads an exhibit of a [`World`] into the plain
//! description the rules work on. Land exhibits and tanks are rated; the neighbours of a tank
//! (amphibious and show tanks) and the water type of a tank species are not.

use std::collections::{HashMap, HashSet};

use crate::build::number;
use crate::config::to_int;
use crate::entity::{Catalogue, EntityType};
use crate::world::World;

/// The terrain types a species rates.
const TERRAINS: usize = 18;
/// The habitat number of an object that gives none ("non-habitat").
const NO_HABITAT: i32 = 9410;
/// The species timer passes this to revise the species list, the rating timer that to rate again.
const SPECIES_AFTER: u32 = 7999;
const RATING_AFTER: u32 = 6999;

/// Recommendation bits (rule 5.6): the animal panel shows the text 9901 plus the bit number.
pub mod bit {
    pub const TOO_SHALLOW: u32 = 8;
    pub const TOO_DEEP: u32 = 9;
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
    pub attractiveness: i32,
    /// `cNameID`, `cGenus` and `cFamily`, and `cCompatibleAnimals`: pairs of such an id and a
    /// value.
    pub ids: [i32; 3],
    pub compatible: Vec<(i32, i32)>,
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
            attractiveness: int("cAttractiveness", 1),
            ids: [ty.name_id, int("cGenus", 0), int("cFamily", 0)],
            compatible: pairs("cCompatibleAnimals"),
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

/// A tank's water, for the depth and the water quality parts.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Water {
    pub level: i32,
    pub purity: i32,
    pub show: bool,
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
        sum
    }

    /// What an animal shows as its suitability: `ceil(max(0, rating))`.
    pub fn shown(&self) -> u32 {
        self.rating().max(0.0).ceil() as u32
    }

    /// Whether recommendation bit `b` is set.
    pub fn has(&self, b: u32) -> bool {
        self.advice >> b & 1 == 1
    }

    fn set(&mut self, b: u32, important: bool) {
        self.advice |= 1 << b;
        self.important |= u64::from(important) << b;
    }
}

/// Rates the exhibit for each species in it, in the order the species first appear (rules 5.1
/// to 5.6). The fields that depend on the animals' own state (happiness sums, hunger counts)
/// are not part of the rating and not kept.
pub fn rate(input: &Input) -> Vec<Record> {
    let site = &input.site;
    let mut records: Vec<Record> = Vec::new();
    // The first type of each record, and the record of each type.
    let mut first: Vec<usize> = Vec::new();
    let of: Vec<usize> = input
        .species
        .iter()
        .enumerate()
        .map(|(i, l)| {
            records
                .iter()
                .position(|r| r.key == l.key)
                .unwrap_or_else(|| {
                    records.push(Record {
                        key: l.key.clone(),
                        tank: site.water.is_some(),
                        ..Record::default()
                    });
                    first.push(i);
                    records.len() - 1
                })
        })
        .collect();
    for &a in &input.animals {
        if let Some(&r) = of.get(a) {
            records[r].animals += 1;
        }
    }
    // The other animals (5.5.7): the types of another name id, by name id, genus and family.
    for (rec, &i) in records.iter_mut().zip(&first) {
        let l = &input.species[i];
        let sum: i64 = (input.species.iter())
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
    // The tile pass (5.2).
    for (r, &i) in first.iter().enumerate() {
        let l = &input.species[i];
        for &(habitat, name) in &site.paths {
            sums[r] += l.value(habitat) + 25 * l.value(name);
        }
        for tile in &site.slots {
            let mut leafy = false;
            for it in tile.iter().filter_map(|&s| site.items.get(s)) {
                sums[r] += per_slot(l, it);
                if value(l, it) >= 0 {
                    rock_count[r] += u32::from(it.rock);
                    leafy |= it.foliage;
                }
            }
            foliage_tiles[r] += u32::from(leafy);
        }
    }
    // The claims of shelters and toys (5.4); each walk adds the building's value again (an
    // original bug, kept).
    let rooms: Vec<(i32, i32)> = site
        .listed
        .iter()
        .map(|&b| site.items[b].rooms.unwrap_or_default())
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
            for (k, &b) in site.listed.iter().enumerate() {
                let it = &site.items[b];
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
        let l = &input.species[i];
        let rec = &mut records[r];
        let unused = |full: &[i32], left: &[i32]| {
            site.listed
                .iter()
                .enumerate()
                .filter(|&(k, &b)| {
                    full[k] > 0 && left[k] == full[k] && value(l, &site.items[b]) >= 0
                })
                .count() as i32
        };
        let (tank, show) = (site.water.is_some(), site.water.is_some_and(|w| w.show));
        let u = unused(
            &rooms.iter().map(|p| p.0).collect::<Vec<_>>(),
            &shelter_room,
        );
        let t = if show {
            0
        } else {
            unused(&rooms.iter().map(|p| p.1).collect::<Vec<_>>(), &toy_room)
        };
        // Terrain (5.5.1) and its bits (5.6), land only.
        let mut sum = 0.0f32;
        for k in 0..TERRAINS {
            let (w, count) = (l.terrain[k], site.terrain[k]);
            let (wanted, share) = (w as f32, count as f32 * 100.0 / n);
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
            if !tank && k < TERRAINS - 1 && (more || less) {
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
        // Foliage, rocks and elevation (5.5.3, 5.5.4).
        let tree = foliage_tiles[r] as f32 * 100.0 / n;
        let rock = rock_count[r] as f32 * 100.0 / (4.0 * n);
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
            if water.purity < 50 {
                rec.set(bit::DIRTY_WATER, water.purity < 25);
            }
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
    /// The species list: `key/subtype` of each type on the tiles, as last revised.
    pub types: Vec<String>,
    /// The tile (index into the exhibit's tile list) of each species' ambient sound.
    pub sounds: Vec<(String, usize)>,
    /// The sum of `cAttractiveness` of the animals.
    pub attractiveness: i32,
    /// The entity id of the keeper assigned to the exhibit.
    pub keeper: Option<u32>,
    /// The time the exhibit was last serviced, in the game manager's units.
    pub serviced: u64,
}

impl State {
    /// An exhibit that was just made: dirty, both timers at 0.
    pub fn new() -> State {
        State {
            dirty: true,
            ..State::default()
        }
    }

    /// One step of the exhibit update (rule 4): `ms` is added to both timers; a timer past its
    /// limit marks the list or the records, then the list is revised and the records computed,
    /// each restarting its timer at `rand() % 200`. `rand` is the simulation's generator;
    /// `input` is called only when something is due.
    pub fn update(
        &mut self,
        ms: u32,
        rand: &mut dyn FnMut() -> u32,
        input: impl FnOnce() -> Input,
    ) {
        self.species_ms = self.species_ms.saturating_add(ms);
        self.rating_ms = self.rating_ms.saturating_add(ms);
        self.revise |= self.species_ms > SPECIES_AFTER;
        self.dirty |= self.rating_ms > RATING_AFTER;
        if !(self.revise || self.dirty) {
            return;
        }
        let input = input();
        if self.revise {
            self.species_ms = rand() % 200;
            self.revise_list(&input, rand);
        }
        if self.dirty {
            self.rating_ms = rand() % 200;
            self.compute(&input);
        }
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
        self.attractiveness = (input.animals.iter())
            .filter_map(|&a| input.species.get(a))
            .map(|l| l.attractiveness)
            .sum();
    }
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
    let bytes = |s: &str| s.chars().map(|c| c as u8).collect::<Vec<u8>>();
    let kind = |e: usize| {
        let e = &world.zoo.entities[e];
        catalogue.find(&e.class, &bytes(&e.type_name), &bytes(&e.subtype))
    };
    let mut site = Site {
        tiles: exhibit.order.len(),
        water: exhibit.tank.map(|t| Water {
            level: t.level as i32,
            purity: t.purity as i32,
            show: t.show,
        }),
        ..Site::default()
    };
    let mut items: HashMap<usize, usize> = HashMap::new();
    let mut inside = vec![false; world.zoo.tiles.len()];
    exhibit.order.iter().for_each(|&t| inside[t] = true);
    for &t in &exhibit.order {
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
    // The animals, tile by tile, and their types in the order they appear.
    let width = world.zoo.width as i64;
    let mut on: HashMap<usize, Vec<&EntityType>> = HashMap::new();
    for (e, entity) in world.zoo.entities.iter().enumerate() {
        let (Some(c), "animals") = (&entity.common, entity.class.as_str()) else {
            continue;
        };
        let (x, y) = (i64::from(c.x.div_euclid(64)), i64::from(c.y.div_euclid(64)));
        let t = (y >= 0 && x >= 0 && x < width).then(|| (y * width + x) as usize);
        if let Some((t, ty)) = t.filter(|&t| inside.get(t) == Some(&true)).zip(kind(e)) {
            on.entry(t).or_default().push(ty);
        }
    }
    let (mut species, mut animals): (Vec<Likes>, Vec<usize>) = (Vec::new(), Vec::new());
    for ty in exhibit
        .order
        .iter()
        .flat_map(|t| on.remove(t).unwrap_or_default())
    {
        let likes = Likes::of(ty);
        let at = (species.iter())
            .position(|s| (&s.key, &s.subtype) == (&likes.key, &likes.subtype))
            .unwrap_or_else(|| {
                species.push(likes);
                species.len() - 1
            });
        animals.push(at);
    }
    Input {
        site,
        species,
        animals,
    }
}
