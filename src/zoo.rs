//! Maps and saved games of the original game (`.zoo` files).
//!
//! A map is a saved game without guests. The file is a plain sequence of sections with no table
//! of contents: header, habitats (which also hold the map size and the entrance), shows,
//! entity id counter, tile grid, entities, and then sections that are not needed to draw a map.
//! All integers are little-endian; a string is a `u32` length below 4096 and that many bytes.
//!
//! [`Zoo::parse`] keeps everything up to and including the entities: the header, the map size
//! and entrance, the number of exhibits, the id counter, the tiles, and for every entity its
//! class, type and subtype names and the part every body ends with (position, rotation, id and
//! name). The sections after the entities (view, scenario, counters, finances, thoughts, events,
//! research, awards, marketing, cheats and the tail) are not kept, except the view, the clocks with
//! the event queue and, through [`Zoo::parse_money`], the cash and the accounts: they are read and dropped, only to refuse
//! the files the original loader refuses. Bytes after the last section are
//! ignored, as the original does. The habitat and show records are skipped the same way.
//!
//! From version 70 every entity record states the size of its body, so a record whose class or
//! layout is not understood is skipped (its [`Entity::common`] is `None`). Before version 70 the
//! body must be parsed to find the next record, so any unknown class is an error there.
//!
//! The parser knows nothing of the entity definitions. The original also refuses a file of a
//! version below 70 whose entity has a type it has no definition for, even if its class is
//! known; [`Zoo::check_types`] does the same once the caller has the definitions.
//!
//! Everything read from a file is untrusted. Every count and size is checked against the bytes
//! left before anything is allocated or looped over, so the time and memory used are linear in
//! the size of the input, which is limited to [`MAX_SIZE`].

mod body;
#[cfg(test)]
pub(crate) mod tests;

use std::collections::BTreeMap;
use std::error::Error;
use std::fmt;

use crate::entity::Catalogue;
use crate::money::{CATEGORIES, MONTHS, Money, YEARS};

/// The oldest version the original accepts.
pub const MIN_VERSION: u32 = 38;

/// The newest version the original accepts (and the only one it writes).
pub const MAX_VERSION: u32 = 106;

/// The biggest file [`Zoo::parse`] reads: 32 MiB. The biggest file of the game is below 0.7 MiB;
/// the original has no limit, this one bounds the memory a hostile file can make the parser take.
pub const MAX_SIZE: usize = 32 * 1024 * 1024;

/// Why a file was refused. The messages are single lines.
#[derive(Debug, PartialEq, Eq)]
pub enum ZooError {
    /// The file does not start with `TZFB`.
    NotASavedGame,
    /// The version is below [`MIN_VERSION`].
    TooOld(u32),
    /// The version is above [`MAX_VERSION`].
    TooNew(u32),
    /// A read failed or a rule of the format was broken; the text says where.
    Corrupted(String),
}

impl fmt::Display for ZooError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ZooError::NotASavedGame => f.write_str("not a saved game"),
            ZooError::TooOld(v) => write!(f, "unsupported old version {v}"),
            ZooError::TooNew(v) => write!(f, "made by a newer version ({v})"),
            ZooError::Corrupted(why) => write!(f, "corrupted: {why}"),
        }
    }
}

impl Error for ZooError {}

/// One tile of the grid.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Tile {
    /// Height of the corner at the tile origin, in elevation steps (16 world units each).
    pub elevation: i32,
    /// The heights of the four corners above the lowest one, 2 bits each.
    pub shape: u8,
    /// The `type` of a terrain definition (0 is grass).
    pub terrain: u8,
    /// Two bits for each side of the tile; 0 and 1 mean the ground meets the neighbour.
    pub edges: u8,
}

/// The part every entity body ends with.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Common {
    /// World position; a tile is 64 by 64 units and a world z unit is 1/16 of an elevation step.
    pub x: i32,
    pub y: i32,
    pub z: i32,
    /// Direction, 0 to 7. The file may hold any value; it is stored modulo 8, as the original's
    /// users read it.
    pub rotation: u32,
    pub id: u32,
    /// The name the file stores for the entity; each byte is one character (Latin-1).
    pub name: String,
    /// The colour choices of the entity, one byte per replaceable colour group of its type, from
    /// version 67 (empty before, and when the entity stores none).
    pub colors: Vec<u8>,
    /// The condition of a fence or tank wall of a version from 55; `None` for everything else.
    pub fence: Option<Fence>,
    /// The movement mode of a unit (3: inside a tank); 0 for everything else.
    pub movement_mode: u32,
    /// The food units left of a scenery entity, a signed counter (zoo format 6.6): kept as stored
    /// for the class `food` and the subtypes `poo` and `dinopoo`, 0 for everything else.
    pub food: i32,
    /// What an animal record stores of the animal's live state; `None` for everything else.
    pub animal: Option<Box<AnimalRecord>>,
}

/// The values, counters and flags an animal record stores (animals 3.5). A counter the record
/// does not hold (by the version) is `None`: it keeps what creation gave it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AnimalRecord {
    pub happiness: i32,
    pub hunger: i32,
    pub health: i32,
    pub energy: i32,
    pub dung: i32,
    pub female: bool,
    /// The counters of the checks in the order of the animal module's indices.
    pub counters: [Option<i32>; 16],
    pub at_minimum_health: bool,
    pub very_unhappy: bool,
    pub keeper_visited: bool,
    pub mourning: bool,
    pub dying: bool,
    pub crated: bool,
    pub egg: bool,
    pub thirsty: bool,
    /// The rampage flag and counter (animals 11).
    pub rampaging: bool,
    pub rampage: i32,
}

/// The four values a fence or tank wall record holds after its common part.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Fence {
    pub strength: u32,
    /// Below 1 the fence is broken; at or below the `cDecayedLife` of its type, decayed.
    pub life: u32,
    /// Two decay timers.
    pub timers: [u32; 2],
    /// A tank wall whose first trailing byte is not 0 is a portal.
    pub portal: bool,
}

/// What the exhibit record keeps of a tank exhibit.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Tank {
    /// The first tile of the exhibit's tile list.
    pub tile: (i32, i32),
    /// The second tile (the gate), or (-1, -1).
    pub gate: (i32, i32),
    /// The tank height and the water level, in elevation steps above the tank floor.
    pub height: u32,
    pub level: u32,
    /// Not 0 in the file: salt water. Maps before version 100 hold salt water of purity 100.
    pub salt: bool,
    pub purity: u32,
}

/// One entity record.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entity {
    pub class: String,
    pub type_name: String,
    pub subtype: String,
    /// The stored size of the body; `None` before version 70, which has none.
    pub body_size: Option<u32>,
    /// `None` when the body was skipped: size 0 (a transient entity), a class this reader does
    /// not know, or a body that did not decode to exactly its stored size. The original never
    /// compares a body with its size for a class it knows (it goes on from where its decoding
    /// stopped); such a record is kept without fields here, which the format allows.
    pub common: Option<Common>,
}

/// A loaded map or saved game, up to the entities.
#[derive(Debug, Clone, PartialEq)]
pub struct Zoo {
    pub version: u32,
    /// The Windows language identifier the file was written with (a stored 0 reads as 1033).
    pub locale: u32,
    /// The expansion selection; 0 before version 82.
    pub expansion: u32,
    pub width: u32,
    pub height: u32,
    /// The tile of the zoo entrance.
    pub entrance: (u32, u32),
    /// The first tile of every habitat (exhibit or tank), in file order; the rest of the record
    /// is in the fields below, tank fields aside.
    pub habitat_tiles: Vec<(i32, i32)>,
    /// The gate of every habitat as its record names it: the tile, (-1, -1) for none, and the
    /// side (map direction 0, 2, 4 or 6, -1 for none); in file order like `habitat_tiles`.
    pub habitat_gates: Vec<((i32, i32), i32)>,
    /// The name, statistics and creation date every habitat stores, in file order.
    pub habitat_info: Vec<HabitatInfo>,
    /// The tank exhibits, in file order.
    pub tanks: Vec<Tank>,
    /// The highest entity id handed out so far.
    pub id_counter: u32,
    /// `width * height` tiles, row by row: the tile in column x and row y is `y * width + x`.
    pub tiles: Vec<Tile>,
    pub entities: Vec<Entity>,
    /// The camera the file stores.
    pub view: View,
    /// The AI clock and the world clock the file stores.
    pub clocks: Clocks,
    /// How many units of each type name were named so far (zoo format 7.3), for the number in a
    /// default name.
    pub names: BTreeMap<String, u32>,
}

/// What a habitat record stores besides its place (zoo format 4.1): the name, the nine
/// statistics as stored (habitat rating 13.3) and the creation date (a file time).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HabitatInfo {
    pub name: String,
    pub stats: [u32; 9],
    pub created: u64,
}

/// What the zoo status keeps in a saved file (zoo status 16): its three timers in milliseconds, the
/// rating, the donation counter and the adult admission price. The price of a file without the
/// field is 49.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct StatusRecord {
    /// The rating, message and new guest timers.
    pub timers: [u32; 3],
    pub rating: u32,
    pub given: f32,
    pub price: f32,
}

/// The clocks and the event queue a map stores (simulation 4.3, 8.3): a new game on it starts from
/// them. All 0 and empty in a file too old to store them, and in a map made by hand.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Clocks {
    /// The zoo status' timers and counters.
    pub status: StatusRecord,
    /// The AI clock: the milliseconds left over and the whole AI ticks.
    pub ai_rem: u32,
    pub ai_ticks: u32,
    /// The world clock: the milliseconds left over and the whole seconds.
    pub world_rem: u32,
    pub world_secs: u32,
    /// The two event buffers (from version 97).
    pub queue: Queue,
}

/// One record of the event queue (simulation 5.1).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Event {
    pub sender: u32,
    /// The entity it is for, 0 for everyone.
    pub target: u32,
    pub data: u32,
    pub extra: u16,
    /// The event type, 0x2711 and up.
    pub kind: u16,
    /// Steps before delivery; 0 and 1 both mean the next step.
    pub delay: u16,
    pub sender_channel: u8,
    pub channel: u8,
}

/// The two event buffers a file stores, in file order, and which one is the "next" buffer.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Queue {
    pub first: Vec<Event>,
    pub second: Vec<Event>,
    pub next_is_first: bool,
}

/// The view state of a file (zoo format 7.1), as stored: nothing is checked.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct View {
    pub zoom: i32,
    pub rotation: i32,
    /// The scroll position `(x, y)`.
    pub scroll: (i32, i32),
    /// The terraform brush size; only the low byte is used.
    pub brush: u32,
}

impl Zoo {
    /// The tile in column `x` and row `y`, if it is on the map.
    pub fn tile(&self, x: u32, y: u32) -> Option<&Tile> {
        if x >= self.width || y >= self.height {
            return None;
        }
        self.tiles
            .get(y as usize * self.width as usize + x as usize)
    }

    /// Refuses a file of a version below 70 that holds an entity whose type `known` does not
    /// find, as the original does: there is no body size to skip it by, so the whole load
    /// fails. From version 70 such an entity is skipped, so the file passes.
    pub fn check_types(&self, known: impl Fn(&Entity) -> bool) -> Result<(), ZooError> {
        if self.version >= 70 {
            return Ok(());
        }
        match self.entities.iter().position(|e| !known(e)) {
            Some(i) => Err(ZooError::Corrupted(format!(
                "entities: entity {i}: no definition of its type in a file without body sizes"
            ))),
            None => Ok(()),
        }
    }

    /// Applies [`Zoo::check_types`] with the types of `catalogue`.
    pub fn checked(self, catalogue: &Catalogue) -> Result<Zoo, ZooError> {
        let bytes = |s: &str| s.chars().map(|c| c as u8).collect::<Vec<u8>>();
        self.check_types(|e| {
            catalogue
                .find(&e.class, &bytes(&e.type_name), &bytes(&e.subtype))
                .is_some()
        })?;
        Ok(self)
    }

    /// Reads a file, refusing it the way the original does.
    pub fn parse(bytes: &[u8]) -> Result<Zoo, ZooError> {
        Zoo::parse_money(bytes).map(|(zoo, _)| zoo)
    }

    /// Reads a file as [`Zoo::parse`] does and keeps the cash and the accounts it stores.
    pub fn parse_money(bytes: &[u8]) -> Result<(Zoo, Money), ZooError> {
        if bytes.get(..4) != Some(b"TZFB") {
            return Err(ZooError::NotASavedGame);
        }
        let version = bytes
            .get(4..8)
            .map(|v| u32::from_le_bytes(v.try_into().unwrap()))
            .ok_or_else(|| ZooError::Corrupted("header: truncated".into()))?;
        if version < MIN_VERSION {
            return Err(ZooError::TooOld(version));
        }
        if version > MAX_VERSION {
            return Err(ZooError::TooNew(version));
        }
        if bytes.len() > MAX_SIZE {
            return Err(ZooError::Corrupted("file too big".into()));
        }
        let mut r = Reader {
            data: &bytes[8..],
            v: version,
        };
        let mut section = "header";
        let mut money = Money::default();
        read(&mut r, &mut section, &mut money)
            .map(|zoo| (zoo, money))
            .map_err(|Bad(why)| ZooError::Corrupted(format!("{section}: {why}")))
    }
}

/// Why a read failed, in a few words.
struct Bad(String);

type R<T> = Result<T, Bad>;

fn bad<T>(why: &str) -> R<T> {
    Err(Bad(why.into()))
}

/// A bounds-checked reader over untrusted bytes; `v` is the version of the file.
struct Reader<'a> {
    data: &'a [u8],
    v: u32,
}

impl<'a> Reader<'a> {
    fn take(&mut self, len: usize) -> R<&'a [u8]> {
        if len > self.data.len() {
            return bad("truncated");
        }
        let (head, tail) = self.data.split_at(len);
        self.data = tail;
        Ok(head)
    }

    fn skip(&mut self, len: usize) -> R<()> {
        self.take(len).map(|_| ())
    }

    fn u8(&mut self) -> R<u8> {
        Ok(self.take(1)?[0])
    }

    fn u16(&mut self) -> R<u16> {
        Ok(u16::from_le_bytes(self.take(2)?.try_into().unwrap()))
    }

    /// A count and that many event records.
    fn events(&mut self) -> R<Vec<Event>> {
        (0..self.count(20)?)
            .map(|_| {
                Ok(Event {
                    sender: self.u32()?,
                    target: self.u32()?,
                    data: self.u32()?,
                    extra: self.u16()?,
                    kind: self.u16()?,
                    delay: self.u16()?,
                    sender_channel: self.u8()?,
                    channel: self.u8()?,
                })
            })
            .collect()
    }

    fn u32(&mut self) -> R<u32> {
        Ok(u32::from_le_bytes(self.take(4)?.try_into().unwrap()))
    }

    fn f32(&mut self) -> R<f32> {
        self.u32().map(f32::from_bits)
    }

    fn i32(&mut self) -> R<i32> {
        self.u32().map(|n| n as i32)
    }

    /// A string: `u32` length below 4096, then the bytes.
    fn string(&mut self) -> R<&'a [u8]> {
        let len = self.u32()?;
        if len >= 4096 {
            return bad("string of 4096 bytes or more");
        }
        self.take(len as usize)
    }

    /// A string, one character per byte.
    fn text(&mut self) -> R<String> {
        Ok(latin1(self.string()?))
    }

    /// A `u32` count of elements of at least `min_size` bytes each, checked against the bytes
    /// left, so a loop of that many steps is bounded by the input.
    fn count(&mut self, min_size: usize) -> R<usize> {
        let n = self.u32()? as usize;
        if n > self.data.len() / min_size {
            return bad("count passes the end of the file");
        }
        Ok(n)
    }

    /// A count and that many elements of `size` bytes.
    fn skip_list(&mut self, size: usize) -> R<()> {
        let n = self.count(size)?;
        self.skip(n * size)
    }

    /// A count and that many strings.
    fn skip_strings(&mut self) -> R<()> {
        for _ in 0..self.count(4)? {
            self.string()?;
        }
        Ok(())
    }
}

fn latin1(bytes: &[u8]) -> String {
    bytes.iter().map(|&b| b as char).collect()
}

/// Reads the file after the magic and the version, noting in `section` where it is.
fn read(r: &mut Reader, section: &mut &'static str, money: &mut Money) -> R<Zoo> {
    let v = r.v;
    let locale = if v >= 69 { r.u32()? } else { 0 };
    let locale = if locale == 0 { 1033 } else { locale };
    let expansion = if v >= 82 { r.u32()? } else { 0 };

    *section = "habitats";
    let (width, height) = (r.u32()?, r.u32()?);
    let entrance = (r.u32()?, r.u32()?);
    let habitat_count = r.count(HABITAT_MIN)?;
    let mut tanks = Vec::new();
    let mut habitat_tiles = Vec::new();
    let mut habitat_gates = Vec::new();
    let mut habitat_info = Vec::new();
    for _ in 0..habitat_count {
        let (tile, gate, tank, info) = habitat(r)?;
        habitat_tiles.push(tile);
        habitat_gates.push(gate);
        habitat_info.push(info);
        tanks.extend(tank);
    }
    if v >= 64 {
        r.skip(4)?;
    }
    if v >= 97 {
        *section = "shows";
        shows(r)?;
    }

    *section = "id counter";
    let id_counter = r.u32()?;

    *section = "tiles";
    let n = u64::from(width) * u64::from(height);
    if n > r.data.len() as u64 / 10 {
        return bad("the map is bigger than the file");
    }
    let mut tiles = Vec::with_capacity(n as usize);
    for _ in 0..n {
        let rec = r.take(10)?;
        tiles.push(Tile {
            elevation: i32::from_le_bytes(rec[..4].try_into().unwrap()),
            shape: rec[4],
            terrain: rec[5],
            edges: rec[6],
        });
    }

    *section = "entities";
    let min_record = if v >= 70 { 16 } else { 12 };
    let count = r.count(min_record)?;
    let mut entities = Vec::with_capacity(count);
    for i in 0..count {
        let entity = entity(r).map_err(|Bad(why)| Bad(format!("entity {i}: {why}")))?;
        entities.push(entity);
    }

    *section = "later sections";
    let (view, clocks, names) = after_entities(r, money)?;
    Ok(Zoo {
        version: v,
        locale,
        expansion,
        width,
        height,
        entrance,
        habitat_tiles,
        habitat_gates,
        habitat_info,
        tanks,
        id_counter,
        tiles,
        entities,
        view,
        clocks,
        names,
    })
}

/// The smallest habitat record: two tiles, a string, 4 + 36 bytes and two dates.
const HABITAT_MIN: usize = 76;

type Habitat = ((i32, i32), ((i32, i32), i32), Option<Tank>, HabitatInfo);

fn habitat(r: &mut Reader) -> R<Habitat> {
    let v = r.v;
    let tile = (r.i32()?, r.i32()?);
    let name = latin1(r.string()?);
    let gate = (r.i32()?, r.i32()?);
    let side = r.i32()?;
    let mut stats = [0; 9];
    for s in &mut stats {
        *s = r.u32()?;
    }
    let created = u64::from(r.u32()?) | u64::from(r.u32()?) << 32;
    r.skip(8)?;
    let info = HabitatInfo {
        name,
        stats,
        created,
    };
    if v >= 83 {
        r.skip(4)?;
    }
    let is_tank = v >= 85 && r.u8()? != 0;
    let has_show = v >= 90 && r.u8()? != 0;
    if has_show {
        show_info(r)?;
    }
    if !is_tank {
        return Ok((tile, (gate, side), None, info));
    }
    let (height, level) = (r.u32()?, r.u32()?);
    if v >= 88 {
        r.skip(1)?;
    }
    let (mut salt, mut purity) = (100, 100);
    if v >= 100 {
        salt = r.u32()?;
        r.skip(4)?;
        purity = r.u32()?;
    }
    Ok((
        tile,
        (gate, side),
        Some(Tank {
            tile,
            gate,
            height,
            level,
            salt: salt != 0,
            purity,
        }),
        info,
    ))
}

fn show_info(r: &mut Reader) -> R<()> {
    let v = r.v;
    r.skip(8)?;
    r.skip_list(4)?;
    if v <= 98 {
        r.skip_list(8)?;
    }
    r.skip_list(if v >= 99 { 6 + 2 + 28 + 8 } else { 6 })?;
    if v >= 97 {
        r.skip_list(20)?;
        r.skip(6)?;
    }
    if v >= 105 {
        r.skip(12)?;
    }
    if v >= 106 {
        r.skip(24)?;
    }
    if v >= 97 {
        // A show: ids and values, its state, and a list of script states.
        r.skip(16 + 5 + 16)?;
        r.skip_list(14)?;
    }
    if v == 101 {
        r.skip(1)?;
    }
    Ok(())
}

/// The show scripts (from version 89) and the two id counters (from 97).
fn shows(r: &mut Reader) -> R<()> {
    let v = r.v;
    if v >= 89 {
        // A script: id, two values and a list of items of at least 53 bytes.
        for _ in 0..r.count(2 + 4 + 4 + 4)? {
            r.skip(10)?;
            for _ in 0..r.count(53)? {
                r.skip(12)?;
                for _ in 0..4 {
                    r.string()?;
                }
                r.skip(8 + 1 + 16)?;
                if v >= 103 {
                    r.skip(8)?;
                    r.string()?;
                    r.string()?;
                }
            }
        }
    }
    if v >= 97 {
        r.skip(4)?;
    }
    Ok(())
}

fn entity(r: &mut Reader) -> R<Entity> {
    let class = r.string()?;
    let type_name = r.text()?;
    let subtype = r.text()?;
    let (body_size, common) = if r.v >= 70 {
        let size = r.u32()?;
        let mut body = Reader {
            data: r.take(size as usize)?,
            v: r.v,
        };
        // A body that does not decode to exactly its size is skipped, like an unknown class.
        let common = match body::decode(&mut body, class) {
            Ok(Some(common)) if body.data.is_empty() && size > 0 => Some(common),
            _ => None,
        };
        (Some(size), common)
    } else {
        match body::decode(r, class)? {
            Some(common) => (None, Some(common)),
            None => return bad("unknown class in a file without body sizes"),
        }
    };
    let mut common: Option<Common> = common;
    if class != b"food"
        && !["poo", "dinopoo"]
            .iter()
            .any(|s| subtype.eq_ignore_ascii_case(s))
    {
        common.iter_mut().for_each(|c| c.food = 0);
    }
    Ok(Entity {
        class: latin1(class),
        type_name,
        subtype,
        body_size,
        common,
    })
}

/// Reads and drops the sections after the entities, as far as the original loader checks them.
fn after_entities(r: &mut Reader, money: &mut Money) -> R<(View, Clocks, BTreeMap<String, u32>)> {
    let mut clocks = Clocks::default();
    let v = r.v;
    let view = View {
        zoom: r.i32()?,
        rotation: r.i32()?,
        scroll: (r.i32()?, r.i32()?),
        brush: r.u32()?,
    };

    // Scenario: a size, then (from 51, unless the size is 0) a name and the goal states, one
    // `u32` each. The original skips the block by its size when it starts a scenario and reads
    // its content otherwise, and the two agree only when the size is exactly that of the
    // content, so any other size is refused. Before 51 the block is the name alone.
    let size = u64::from(r.u32()?);
    if size > 0 && v >= 51 {
        let name = u64::from(r.u32()?);
        r.skip(name as usize)?;
        let goals = u64::from(r.u32()?);
        if goals % 4 != 0 {
            return bad("scenario goal states are not a multiple of 4 bytes");
        }
        r.skip(goals as usize)?;
        if size != 8 + name + goals {
            return bad("scenario block size does not match its content");
        }
    } else {
        r.skip(size as usize)?;
    }

    // Name counters: type name and number.
    let mut names = BTreeMap::new();
    for _ in 0..r.count(8)? {
        let name = latin1(r.string()?);
        names.insert(name, r.u32()?);
    }

    // Game and finances: the zoo status' timers, rating and donation counter first.
    r.skip(4)?;
    clocks.status.timers = [r.u32()?, r.u32()?, r.u32()?];
    r.skip(1)?;
    clocks.status.rating = r.u32()?;
    r.skip(12)?;
    clocks.status.given = r.f32()?;
    (money.month, money.year) = (r.u32()?.clamp(1, 12), r.u32()?.min(19));
    let (rows, columns) = (u64::from(r.u32()?), u64::from(r.u32()?));
    let tables = ((12 + columns + 1) * 4)
        .checked_mul(rows)
        .filter(|&n| n <= r.data.len() as u64);
    tables.ok_or_else(|| Bad("finance tables pass the end of the file".into()))?;
    // The rows past the 31 and the columns past the 20 that are kept are read and dropped.
    let (rows, columns) = (rows as usize, columns as usize);
    for m in 0..rows {
        for column in 0..MONTHS {
            let value = r.f32()?;
            if m < CATEGORIES {
                money.monthly[m][column] = value;
            }
        }
    }
    for y in 0..rows {
        for column in 0..columns {
            let value = r.f32()?;
            if y < CATEGORIES && column < YEARS {
                money.yearly[y][column] = value;
            }
        }
    }
    for c in 0..rows {
        let value = r.f32()?;
        if c < CATEGORIES {
            money.total[c] = value;
        }
    }
    clocks.status.price = if v >= 39 { r.f32()? } else { 49.0 };
    r.skip(if v >= 71 { 8 } else { 0 })?;
    r.skip(16)?; // calendar
    money.set_cash(r.f32()?);
    if v >= 73 {
        r.skip(4)?;
    }

    r.skip_list(20)?; // thoughts
    // The AI clock: the milliseconds left over, then the whole AI ticks (7.6).
    (clocks.ai_rem, clocks.ai_ticks) = (r.u32()?, r.u32()?);
    if v >= 97 {
        clocks.queue.first = r.events()?;
        clocks.queue.second = r.events()?;
        clocks.queue.next_is_first = r.u8()? != 0;
    }
    if v >= 40 {
        r.skip(4)?;
        loop {
            let kind = r.i32()?;
            if kind < 0 {
                break;
            }
            r.skip(4)?;
            match kind {
                0 | 2 => r.skip(4)?,
                1 => r.skip(1)?,
                _ => return bad("research record of an unknown type"),
            }
        }
    }
    if v >= 59 {
        r.skip_list(4)?; // awards
    }
    if v >= 60 {
        r.skip(4)?; // marketing
    }
    if (89..=96).contains(&v) {
        shows(r)?;
    }
    if v >= 104 {
        r.skip_strings()?; // cheats
    }
    if v >= 58 {
        // The world clock: the milliseconds left over, then the whole seconds (7.11).
        (clocks.world_rem, clocks.world_secs) = (r.u32()?, r.u32()?);
        r.skip(if v <= 66 { 4 } else { 0 })?;
    }
    Ok((view, clocks, names))
}
