//! The file the original writes when a shipped file is opened with Continue and saved before the
//! first step (save games 15), built from the bytes of the source: the engine's writer is not
//! used, so a bug of the writer is not in both files of the comparison. The source's sections are
//! copied, and the changes of the original applied to them: the records of a type the game does
//! not have are dropped, the name counters above 0 are kept, the events of the after-load steps
//! are posted, the life counters are cut and lengthened, and a show info of version 105 gets its
//! sums. The bytes that no engine can reproduce, or that the engine has to take from its running
//! game, are listed in [`Built::masked`] and left out of the comparison.

use super::super::*;
use crate::building;
use crate::exhibit::{EXTERIOR, GROUNDS};
use crate::goal::{ENTER_BUILDING, EXIT_BUILDING, USE};
use crate::world::World;
use std::ops::Range;

/// What the original changes in a file, per file: how many times the longevity research lengthens
/// the animals (save games 15.5).
#[derive(Debug, Default, Clone, Copy)]
pub(super) struct Changes {
    pub lengthened: u32,
}

/// What the longevity research (aquatic species, way 2) adds to a life, in per cent.
const LONGEVITY: i64 = 25;

pub(super) struct Built {
    pub bytes: Vec<u8>,
    /// The ranges of `bytes` that a comparison leaves out: the data word of a posted event (an
    /// address in the original, 15.4 point 2) and the position of a user of a draw place (seated
    /// again at random, 15.6).
    pub masked: Vec<Range<usize>>,
    /// The records dropped (15.2) and the kinds of the events posted at load (15.4), each with its
    /// count.
    pub dropped: usize,
    pub events: std::collections::BTreeMap<u16, usize>,
}

/// What the build needs besides the source.
pub(super) struct Inputs<'a> {
    pub vfs: &'a crate::vfs::Vfs,
    pub cat: &'a Catalogue,
    pub source: &'a [u8],
    /// The engine's save of the file, for the sections 15.10 leaves to it and the names of a file
    /// in another language than the install's.
    pub engine: &'a [u8],
    pub language: u32,
    pub changes: Changes,
}

fn u32_at(b: &[u8], at: usize) -> u32 {
    u32::from_le_bytes(b[at..at + 4].try_into().unwrap())
}

/// The sections of a file with their byte ranges.
pub(super) fn ranges(bytes: &[u8]) -> Result<Vec<(String, Range<usize>)>, String> {
    let mut at = 0;
    let mut out = Vec::new();
    for (name, end) in Zoo::sections(bytes).map_err(|e| e.to_string())? {
        out.push((name, at..end));
        at = end;
    }
    out.push(("tail".into(), at..bytes.len()));
    Ok(out)
}

/// One entity record of the source: its three names and where its body lies.
struct Record {
    start: usize,
    kind: (String, String, String),
    size_at: usize,
    body: Range<usize>,
}

/// The records of the entity part (the count, then the records), as framed by save games 6.2.
fn records(part: &[u8]) -> Result<Vec<Record>, String> {
    let short = || "entity records: truncated".to_string();
    let text = |at: usize| -> Result<(String, usize), String> {
        let len = u32_at(part.get(at..at + 4).ok_or_else(short)?, 0) as usize;
        let s = part.get(at + 4..at + 4 + len).ok_or_else(short)?;
        Ok((s.iter().map(|&b| b as char).collect(), at + 4 + len))
    };
    let mut out = Vec::new();
    let mut at = 4;
    for _ in 0..u32_at(part, 0) {
        let start = at;
        let (class, next) = text(at)?;
        let (ty, next) = text(next)?;
        let (sub, size_at) = text(next)?;
        let size = u32_at(part.get(size_at..size_at + 4).ok_or_else(short)?, 0) as usize;
        let body = size_at + 4..size_at + 4 + size;
        if body.end > part.len() {
            return Err(short());
        }
        at = body.end;
        out.push(Record {
            start,
            kind: (class, ty, sub),
            size_at,
            body,
        });
    }
    Ok(out)
}

/// The offset in `body` of the life counter of an animal record: the only place where a changed
/// word changes the life the record reads and nothing else.
fn life_offset(body: &[u8], v: u32) -> Result<usize, String> {
    let mut r = Reader::new(body, v);
    let base = animal::read(&mut r).map_err(|_| "an animal record that does not read")?;
    let (used, life) = (body.len() - r.data.len(), base.life.ok_or("no life")?);
    let found: Vec<usize> = (0..used.saturating_sub(3))
        .filter(|&p| body[p..p + 4] == life.to_le_bytes())
        .filter(|&p| {
            let mut other = body.to_vec();
            other[p..p + 4].copy_from_slice(&(life ^ 0x5555).to_le_bytes());
            let mut r = Reader::new(&other, v);
            let mut moved = base.clone();
            moved.life = Some(life ^ 0x5555);
            animal::read(&mut r).is_ok_and(|a| a == moved)
        })
        .collect();
    match found[..] {
        [p] => Ok(p),
        _ => Err(format!("the life counter has {} places", found.len())),
    }
}

/// The offset in `body` of the entity base's position (the 12 bytes before the rotation and the
/// id), found by the three words and the id the reader gave.
fn base_offset(body: &[u8], c: &Common) -> Result<usize, String> {
    let key: Vec<u8> = [c.x, c.y, c.z]
        .iter()
        .flat_map(|n| n.to_le_bytes())
        .collect();
    let found: Vec<usize> = (0..body.len().saturating_sub(23))
        .filter(|&p| body[p..p + 12] == key[..] && body[p + 16..p + 20] == c.id.to_le_bytes())
        .collect();
    match found[..] {
        [p] => Ok(p),
        _ => Err(format!(
            "the base of entity {} has {} places",
            c.id,
            found.len()
        )),
    }
}

/// Whether the file's world has an animal in habitat `x` or in one of its amphibious neighbours.
fn empty(world: &World, x: u32) -> bool {
    let near = (world.exhibits().get(x)).map_or(&[][..], |e| &e.amphibious[..]);
    let width = i64::from(world.zoo.width);
    let region = |c: &Common| {
        let (tx, ty) = (i64::from(c.x.div_euclid(64)), i64::from(c.y.div_euclid(64)));
        ((0..width).contains(&tx) && ty >= 0)
            .then(|| world.exhibits().region((ty * width + tx) as usize))
            .flatten()
    };
    let animals = world.zoo.entities.iter().filter(|e| e.class == "animals");
    let regions: Vec<Option<u32>> = animals
        .filter_map(|e| e.common.as_ref())
        .map(region)
        .collect();
    !near.iter().chain([&x]).any(|&n| regions.contains(&Some(n)))
}

/// The events the after-load steps of the entities post, in entity list order (save games 15.4
/// points 1 and 1b): the sender and the kind of each.
fn posted(world: &World, cat: &Catalogue) -> Vec<(u32, u16)> {
    let number = |e: &Entity, key: &str| {
        crate::place::bulldozer::type_of(cat, e).and_then(|t| crate::build::number(t, key))
    };
    let mut out = Vec::new();
    for e in &world.zoo.entities {
        let Some(c) = e.common.as_ref() else { continue };
        // A trash can of more than 12 pieces (guest buildings 4.1).
        if e.class == "building" && c.building.as_ref().is_some_and(|b| b.fill > 12) {
            out.push((c.id, 10030));
        }
        // A fence piece or a tank wall (placing 7.4): broken again, or decayed.
        if let Some(f) = c.fence {
            let ty = crate::place::bulldozer::type_of(cat, e);
            let limit = number(e, "cDecayedLife").unwrap_or(0.0) as i64;
            if f.life < 1 {
                if ty.is_some_and(crate::world::deteriorates) {
                    out.push((c.id, 10034));
                }
            } else if i64::from(f.life) <= limit {
                let electric = number(e, "cIsElectrified").unwrap_or(0.0) != 0.0;
                out.push((c.id, if electric { 10034 } else { 10033 }));
            }
        }
        // A filter (tank staff 7.5): broken, or decayed.
        if let Some(f) = c.filter {
            let limit = number(e, "cDecayedHealth").map_or(5, |v| v as i64);
            if f.health == 0 {
                out.push((c.id, 10036));
            } else if i64::from(f.health) <= limit {
                out.push((c.id, 10035));
            }
        }
        // Dung or food on the grounds or in an empty habitat (staff 8.2 point 3).
        let leaves = |key| number(e, key).is_some_and(|v| v != 0.0);
        if leaves("cStink") || leaves("cFood") {
            let tile = world.tile_index(c.x.div_euclid(64), c.y.div_euclid(64));
            let lonely = match tile.and_then(|t| world.exhibits().region(t)) {
                Some(GROUNDS | EXTERIOR) => true,
                None => false,
                Some(x) => empty(world, x),
            };
            if lonely {
                out.push((c.id, 10032));
            }
        }
    }
    out
}

/// The users of a slot with draw places, which the building's after-load step seats again at
/// random (shows 4.3): the units its record lists that are guests or animals on their way in, in
/// or out.
fn seated(world: &World, cat: &Catalogue) -> Vec<u32> {
    let mut out = Vec::new();
    for e in &world.zoo.entities {
        let Some(record) = e.common.as_ref().and_then(|c| c.building.as_ref()) else {
            continue;
        };
        let Some(kind) = building::kind_of(cat, e, world.units.prices.default) else {
            continue;
        };
        if kind.hide || kind.outside {
            continue;
        }
        for &(unit, slot) in &record.users {
            let draws = kind
                .slots
                .get(slot as usize)
                .is_some_and(|s| s.places.is_some());
            let there = world
                .find(unit)
                .and_then(|at| world.zoo.entities[at].common.as_ref());
            let goal = there.and_then(|c| Some(c.unit.as_ref()?.target.goal));
            let on = [ENTER_BUILDING, EXIT_BUILDING, USE].map(i32::from);
            if draws && goal.is_some_and(|g| on.contains(&g)) {
                out.push(unit);
            }
        }
    }
    out
}

/// The life the original writes for an animal record (15.5): cut to the type's `cTimeDeath - 1`,
/// then lengthened once for each completion of the longevity research of the Marine Mania species.
fn life(cat: &Catalogue, e: &Entity, stored: i32, lengthened: u32) -> Option<i32> {
    let ty = crate::place::bulldozer::type_of(cat, e)?;
    let death = crate::build::number(ty, "cTimeDeath")?;
    let aquatic = crate::build::plain(ty, "cExpansionID") == Some(2.0);
    let mut life = i64::from(stored.min(death as i32 - 1));
    for _ in 0..lengthened * u32::from(aquatic) {
        if life > 0 {
            life += life * LONGEVITY / 100;
        }
    }
    Some(life as i32)
}

/// A habitat record of the source: where its name lies, its kind, and for a show info of
/// version 105 where its upkeep ends and the sums the original inserts there (15.9).
struct Habitat {
    name: Range<usize>,
    tank: bool,
    show: bool,
    sums: Option<(usize, [u32; 6])>,
}

fn habitats(habitats: &[u8], v: u32) -> Result<Vec<Habitat>, String> {
    let mut r = Reader::new(habitats, v);
    let bad = |_| "habitat records that do not read".to_string();
    let mut out = Vec::new();
    // The map size and the entrance come first.
    r.skip(16).map_err(bad)?;
    for _ in 0..r.count(HABITAT_MIN).map_err(bad)? {
        r.skip(8).map_err(bad)?;
        let from = r.pos();
        r.string().map_err(bad)?;
        let name = from..r.pos();
        r.skip(8 + 4 + 36 + 16).map_err(bad)?;
        r.skip(4).map_err(bad)?;
        let tank = r.u8().map_err(bad)? != 0;
        let mut habitat = Habitat {
            name,
            tank,
            show: false,
            sums: None,
        };
        if r.u8().map_err(bad)? != 0 {
            habitat.show = true;
            let start = r.pos();
            let mut whole = Reader::new(&habitats[start..], v);
            ShowInfo::read(&mut whole).map_err(bad)?;
            if v == 105 {
                // The info up to its upkeep, then the sums of its type records.
                let mut i = Reader::new(&habitats[start..], v);
                i.skip(8).map_err(bad)?;
                let schedule = i.count(4).map_err(bad)?;
                i.skip(4 * schedule).map_err(bad)?;
                let records = (0..i.count(44).map_err(bad)?)
                    .map(|_| TypeRecord::read(&mut i))
                    .collect::<R<Vec<_>>>()
                    .map_err(bad)?;
                i.events().map_err(bad)?;
                i.skip(4 + 2 + 12).map_err(bad)?;
                let receipts = |n: usize| {
                    let sum = records
                        .iter()
                        .fold(0.0f32, |a, t| a + f32::from_bits(t.receipts[n]));
                    sum.to_bits()
                };
                let visits = |n: usize| {
                    records
                        .iter()
                        .fold(0u32, |a, t| a.wrapping_add(t.attendance[n]))
                };
                let six = [
                    receipts(0),
                    receipts(1),
                    receipts(2),
                    visits(0),
                    visits(1),
                    visits(2),
                ];
                habitat.sums = Some((start + i.pos(), six));
            }
            r.skip(whole.pos()).map_err(bad)?;
        }
        if tank {
            r.skip(8 + 1 + 12).map_err(bad)?;
        }
        out.push(habitat);
    }
    Ok(out)
}

/// The names the original gives when a file's language is not the install's (localisation 10),
/// in the install's language: the entities, numbered by the counters of a new world in file order
/// (the type's text, a space and the number for the numbered classes, the text alone for the
/// others), and the habitats, by their word and their place in the file.
struct Naming<'a> {
    cat: &'a Catalogue,
    tables: crate::strings::StringTables,
    language: u32,
    counters: std::collections::BTreeMap<String, u32>,
    /// The numbers the engine's file gives its habitats, in the order of the records.
    numbers: Vec<u32>,
}

impl Naming<'_> {
    fn entity(&mut self, e: &Entity) -> Option<Vec<u8>> {
        let ty = crate::place::bulldozer::type_of(self.cat, e)?;
        let text = match ty.class {
            "guests" => self.tables.string(9507),
            _ => self.tables.string(ty.name_id as u32),
        };
        let bare = matches!(
            ty.class,
            "objects" | "fences" | "paths" | "tankwall" | "rubble" | "ambient" | "food"
        );
        let subtyped = matches!(ty.class, "building" | "filter");
        let counted = crate::place::unit::is_unit(ty)
            || subtyped
            || matches!(ty.class, "helicopter" | "guests");
        let numbered =
            crate::build::plain(ty, "cUseNumbersInName").unwrap_or(f32::from(u8::from(counted)));
        let name = if bare || numbered != 1.0 {
            text.to_string()
        } else if text.is_empty() {
            String::new()
        } else {
            let key = match ty.class {
                "guests" => b"guests".to_vec(),
                _ if subtyped => ty.subtype.clone(),
                _ => ty.type_name.clone(),
            };
            let counter = self
                .counters
                .entry(String::from_utf8_lossy(&key).into_owned())
                .or_insert(0);
            *counter += 1;
            format!("{text} {counter}")
        };
        Some(crate::strings::encode_text(self.language, &name))
    }

    /// The name of habitat `n` of the file: its word, and a number. The counters go to the exhibits
    /// in the order the load makes them, which is not the order of the records (open point of
    /// save games 15.1 point 4): the engine's number is taken, and the numbers must be 1 to the
    /// count, each once.
    fn habitat(&self, h: &Habitat, n: usize) -> Result<Vec<u8>, String> {
        let word = 6208 + u32::from(h.tank) * (1 + u32::from(h.show));
        let name = format!("{} {}", self.tables.string(word), self.numbers[n]);
        Ok(crate::strings::encode_text(self.language, &name))
    }
}

/// The habitat records, named again when the file's language is not the install's (naming), with
/// the sums a show info of version 105 gets.
fn habitat_section(bytes: &[u8], v: u32, naming: Option<&Naming>) -> Result<Vec<u8>, String> {
    let mut splices: Vec<(Range<usize>, Vec<u8>)> = Vec::new();
    for (n, h) in habitats(bytes, v)?.iter().enumerate() {
        if let Some(naming) = naming {
            let name = naming.habitat(h, n)?;
            let mut field = (name.len() as u32).to_le_bytes().to_vec();
            field.extend(name);
            splices.push((h.name.clone(), field));
        }
        if let Some((end, six)) = h.sums {
            splices.push((end..end, six.iter().flat_map(|n| n.to_le_bytes()).collect()));
        }
    }
    let (mut out, mut at) = (Vec::new(), 0);
    for (range, with) in splices {
        out.extend(&bytes[at..range.start]);
        out.extend(with);
        at = range.end;
    }
    out.extend(&bytes[at..]);
    Ok(out)
}

/// The name counters above 0 only, in the order of the file (7).
fn counter_section(bytes: &[u8]) -> Vec<u8> {
    let (mut at, mut kept) = (4, Vec::new());
    for _ in 0..u32_at(bytes, 0) {
        let end = at + 4 + u32_at(bytes, at) as usize + 4;
        if u32_at(bytes, end - 4) > 0 {
            kept.push(&bytes[at..end]);
        }
        at = end;
    }
    let mut out = (kept.len() as u32).to_le_bytes().to_vec();
    kept.iter().for_each(|k| out.extend(*k));
    out
}

/// The thoughts, the AI clock and the event buffers with the events posted at load added to the
/// next buffer; the offsets (in the section) of the data words of the added events.
fn event_section(bytes: &[u8], events: &[(u32, u16)]) -> Result<(Vec<u8>, Vec<usize>), String> {
    let first = 4 + 20 * u32_at(bytes, 0) as usize + 8;
    let second = first + 4 + 20 * u32_at(bytes, first) as usize;
    let end = second + 4 + 20 * u32_at(bytes, second) as usize;
    if end + 1 != bytes.len() {
        return Err("the events section does not frame".into());
    }
    let (count_at, buffer_end) = if bytes[end] != 0 {
        (first, second)
    } else {
        (second, end)
    };
    let mut section = bytes.to_vec();
    let count = u32_at(bytes, count_at) + events.len() as u32;
    section[count_at..count_at + 4].copy_from_slice(&count.to_le_bytes());
    let mut added = Vec::new();
    for &(sender, kind) in events {
        added.extend(sender.to_le_bytes());
        added.extend([0u8; 8]); // the target, and the data word left out
        added.extend(0u16.to_le_bytes());
        added.extend(kind.to_le_bytes());
        added.extend(1u16.to_le_bytes());
        added.extend([0x45, 0x4d]);
    }
    section.splice(buffer_end..buffer_end, added);
    let data = (0..events.len()).map(|k| buffer_end + 20 * k + 8).collect();
    Ok((section, data))
}

/// The entity part (the count and the records) as the original writes it: the records of a type
/// the game does not have dropped, the names made again, the life counters cut and lengthened.
/// Returns the part, the records dropped, and the ranges of the positions of the users of a draw
/// place in it.
struct EntityPart {
    bytes: Vec<u8>,
    dropped: usize,
    masked: Vec<Range<usize>>,
}

fn entity_part(
    i: &Inputs,
    zoo: &Zoo,
    world: &World,
    naming: &mut Option<Naming>,
    part: &[u8],
) -> Result<EntityPart, String> {
    let v = u32_at(i.source, 4);
    let records = records(part)?;
    let jeep = |r: &Record| r.kind == ("objects".into(), "other".into(), "jeep1".into());
    let kept: Vec<&Record> = records.iter().filter(|r| !jeep(r)).collect();
    if kept.len() != zoo.entities.len() {
        return Err(format!(
            "{} records, {} entities",
            kept.len(),
            zoo.entities.len()
        ));
    }
    let mut out = (kept.len() as u32).to_le_bytes().to_vec();
    let (users, mut masked) = (seated(world, i.cat), Vec::new());
    for (rec, e) in kept.iter().zip(&zoo.entities) {
        let mut body = part[rec.body.clone()].to_vec();
        let common = e.common.as_ref();
        if let (Some(naming), Some(c)) = (&mut *naming, common) {
            // The name of the base: after the position, the rotation and the id.
            let at = base_offset(&body, c)? + 20;
            let was = u32_at(&body, at) as usize;
            if let Some(name) = naming.entity(e) {
                let mut field = (name.len() as u32).to_le_bytes().to_vec();
                field.extend(name);
                body.splice(at..at + 4 + was, field);
            }
        }
        if let Some(a) = common.and_then(|c| c.animal.as_ref())
            && let Some(stored) = a.life
            && let Some(want) = life(i.cat, e, stored, i.changes.lengthened)
        {
            let at = life_offset(&body, v)?;
            body[at..at + 4].copy_from_slice(&want.to_le_bytes());
        }
        out.extend(&part[rec.start..rec.size_at]);
        out.extend((body.len() as u32).to_le_bytes());
        let body_at = out.len();
        out.extend(&body);
        // A user of a draw place: its position is random again.
        if let Some(c) = common
            && users.contains(&c.id)
            && c.guest.is_some()
        {
            let at = body_at + base_offset(&body, c)?;
            masked.push(at..at + 12);
        }
    }
    Ok(EntityPart {
        bytes: out,
        dropped: records.len() - kept.len(),
        masked,
    })
}

pub(super) fn build(i: &Inputs) -> Result<Built, String> {
    let src = i.source;
    let v = u32_at(src, 4);
    let (zoo, _) = Zoo::parse_money_in(src, i.cat).map_err(|e| e.to_string())?;
    let renderer = crate::render::MapRenderer::new(i.vfs, i.cat);
    let lang = crate::strings::Language::of(i.vfs.root());
    let mut world = World::with_language(zoo.clone(), |e| renderer.shape(e), lang);
    world.set_water(crate::place::water(&crate::terrain::types(i.vfs)));
    let mut naming = None;
    if zoo.locale != i.language {
        let tables = crate::strings::StringTables::open(i.vfs.root()).map_err(|e| e.to_string())?;
        let (engine, _) = Zoo::parse_money_in(i.engine, i.cat).map_err(|e| e.to_string())?;
        let numbers: Vec<u32> = (engine.habitat_info.iter())
            .map(|h| {
                h.name
                    .rsplit(' ')
                    .next()
                    .and_then(|n| n.parse().ok())
                    .unwrap_or(0)
            })
            .collect();
        let mut sorted = numbers.clone();
        sorted.sort_unstable();
        if sorted != (1..=numbers.len() as u32).collect::<Vec<_>>() {
            return Err(format!("the engine numbers its habitats {numbers:?}"));
        }
        naming = Some(Naming {
            cat: i.cat,
            tables,
            language: i.language,
            counters: Default::default(),
            numbers,
        });
    }
    let (sections, engine_sections) = (ranges(src)?, ranges(i.engine)?);
    let from_engine = |name: &str| {
        let s = engine_sections.iter().find(|s| s.0 == name);
        s.map(|s| i.engine[s.1.clone()].to_vec())
            .ok_or(format!("no {name} in the engine's file"))
    };
    let at = |name: &str| sections.iter().find(|s| s.0 == name).map(|s| s.1.clone());
    let entities = at("tiles").ok_or("no tiles")?.end;
    let entities = entities..at("entities end, view").ok_or("no end of entities")?.start;
    let mut out: Vec<u8> = Vec::new();
    let mut built = Built {
        bytes: Vec::new(),
        masked: Vec::new(),
        dropped: 0,
        events: Default::default(),
    };
    for (name, r) in &sections {
        let bytes = &src[r.clone()];
        match name.as_str() {
            n if n.starts_with("entity/") => {}
            "tiles" => {
                out.extend(bytes);
                let part = entity_part(i, &zoo, &world, &mut naming, &src[entities.clone()])?;
                built.dropped = part.dropped;
                built.masked.extend(
                    part.masked
                        .iter()
                        .map(|m| out.len() + m.start..out.len() + m.end),
                );
                out.extend(part.bytes);
            }
            "habitats" => out.extend(habitat_section(bytes, v, naming.as_ref())?),
            "header" => {
                out.extend(bytes);
                out[4..8].copy_from_slice(&106u32.to_le_bytes());
                // A save writes the install language (localisation 10).
                out[8..12].copy_from_slice(&i.language.to_le_bytes());
            }
            "name counters" => out.extend(counter_section(bytes)),
            "thoughts, ai clock, events" => {
                let events = posted(&world, i.cat);
                for &(_, kind) in &events {
                    *built.events.entry(kind).or_default() += 1;
                }
                let (section, data) = event_section(bytes, &events)?;
                built
                    .masked
                    .extend(data.iter().map(|d| out.len() + d..out.len() + d + 4));
                out.extend(section);
            }
            // The original rewrites both from its running game (15.10): the engine's, whose sizes
            // the comparison checks.
            "research" | "scenario" => out.extend(from_engine(name)?),
            _ => out.extend(bytes),
        }
    }
    built.bytes = out;
    Ok(built)
}
