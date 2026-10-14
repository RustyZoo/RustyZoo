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
//! research, awards, marketing, cheats and the tail) are not kept: they are read and dropped,
//! only to refuse the files the original loader refuses. Bytes after the last section are
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
mod tests;

use std::error::Error;
use std::fmt;

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
}

/// The four values a fence or tank wall record holds after its common part.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Fence {
    pub strength: u32,
    /// Below 1 the fence is broken; at or below the `cDecayedLife` of its type, decayed.
    pub life: u32,
    /// Two decay timers.
    pub timers: [u32; 2],
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
#[derive(Debug, Clone, PartialEq, Eq)]
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
    pub habitat_count: usize,
    /// The highest entity id handed out so far.
    pub id_counter: u32,
    /// `width * height` tiles, row by row: the tile in column x and row y is `y * width + x`.
    pub tiles: Vec<Tile>,
    pub entities: Vec<Entity>,
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

    /// Reads a file, refusing it the way the original does.
    pub fn parse(bytes: &[u8]) -> Result<Zoo, ZooError> {
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
        read(&mut r, &mut section)
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

    fn u32(&mut self) -> R<u32> {
        Ok(u32::from_le_bytes(self.take(4)?.try_into().unwrap()))
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
fn read(r: &mut Reader, section: &mut &'static str) -> R<Zoo> {
    let v = r.v;
    let locale = if v >= 69 { r.u32()? } else { 0 };
    let locale = if locale == 0 { 1033 } else { locale };
    let expansion = if v >= 82 { r.u32()? } else { 0 };

    *section = "habitats";
    let (width, height) = (r.u32()?, r.u32()?);
    let entrance = (r.u32()?, r.u32()?);
    let habitat_count = r.count(HABITAT_MIN)?;
    for _ in 0..habitat_count {
        habitat(r)?;
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
    after_entities(r)?;
    Ok(Zoo {
        version: v,
        locale,
        expansion,
        width,
        height,
        entrance,
        habitat_count,
        id_counter,
        tiles,
        entities,
    })
}

/// The smallest habitat record: two tiles, a string, 4 + 36 bytes and two dates.
const HABITAT_MIN: usize = 76;

fn habitat(r: &mut Reader) -> R<()> {
    let v = r.v;
    r.skip(8)?;
    r.string()?;
    r.skip(8 + 4 + 36 + 16)?;
    if v >= 83 {
        r.skip(4)?;
    }
    let is_tank = v >= 85 && r.u8()? != 0;
    let has_show = v >= 90 && r.u8()? != 0;
    if has_show {
        show_info(r)?;
    }
    if is_tank {
        r.skip(8)?;
        if v >= 88 {
            r.skip(1)?;
        }
        if v >= 100 {
            r.skip(12)?;
        }
    }
    Ok(())
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
    Ok(Entity {
        class: latin1(class),
        type_name,
        subtype,
        body_size,
        common,
    })
}

/// Reads and drops the sections after the entities, as far as the original loader checks them.
fn after_entities(r: &mut Reader) -> R<()> {
    let v = r.v;
    r.skip(20)?; // view

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
    for _ in 0..r.count(8)? {
        r.string()?;
        r.skip(4)?;
    }

    // Game and finances.
    r.skip(4 + 12 + 1 + 16 + 4 + 8)?;
    let (months, columns) = (u64::from(r.u32()?), u64::from(r.u32()?));
    let tables = ((12 + columns + 1) * 4)
        .checked_mul(months)
        .filter(|&n| n <= r.data.len() as u64);
    r.skip(tables.ok_or_else(|| Bad("finance tables pass the end of the file".into()))? as usize)?;
    r.skip(if v >= 39 { 4 } else { 0 } + if v >= 71 { 8 } else { 0 })?;
    r.skip(16 + 4)?; // calendar, cash
    if v >= 73 {
        r.skip(4)?;
    }

    r.skip_list(20)?; // thoughts
    r.skip(8)?; // events
    if v >= 97 {
        r.skip_list(20)?;
        r.skip_list(20)?;
        r.skip(1)?;
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
        r.skip(if v <= 66 { 12 } else { 8 })?;
    }
    Ok(())
}
