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
//! name). Of the sections after the entities it models the view, the clocks with the event queue, the
//! name counters and, through [`Zoo::parse_money`], the cash and the accounts. Everything else is read
//! only to refuse the files the original loader refuses, and its bytes are kept for [`Zoo::write`]
//! (the habitat and show records, the body of every entity, the scenario, research, marketing
//! and cheats; the thoughts are kept as five words each, the statistics of the habitats are
//! patched in by [`Zoo::write`]). The awards are read into [`Zoo::awards`]. Bytes after the last
//! section are ignored, as the original does.
//!
//! From version 70 every entity record states the size of its body, so a record whose class or
//! layout is not understood is skipped (its [`Entity::common`] is `None`). Before version 70 the
//! body must be parsed to find the next record, so any unknown class is an error there.
//!
//! The parser knows nothing of the entity definitions. The original also refuses a file of a
//! version below 70 whose entity has a type it has no definition for, even if its class is
//! known; [`Zoo::check_types`] does the same once the caller has the definitions. From version 70
//! such an entity is skipped, and [`Zoo::checked`] drops it ([`Zoo::drop_unknown`]) after the load.
//!
//! [`Zoo::write`] mirrors the reader and always writes version 106 (from 70 on). What the engine
//! does not model is kept as read ([`Kept`], and the body of each entity in [`Common`]), converted to
//! that version and written back; what it models is written from the model. An entity made in play
//! has no body: its class fields are built from the model.
//!
//! Everything read from a file is untrusted. Every count and size is checked against the bytes
//! left before anything is allocated or looped over, so the time and memory used are linear in
//! the size of the input, which is limited to [`MAX_SIZE`].

mod animal;
mod body;
pub(crate) mod fresh;
mod staff;
#[cfg(test)]
pub(crate) mod tests;
mod unit;
mod upgrade;
pub(crate) use upgrade::NEVER;
mod write;
pub(crate) use animal::animal_new;
pub use animal::{AnimalRecord, Escape};
pub use staff::StaffRecord;
pub use unit::{Behaviour, Level, Target, UnitBlock};

use std::collections::BTreeMap;
use std::error::Error;
use std::fmt;
use std::rc::Rc;

use crate::entity::Catalogue;
use crate::money::{CATEGORIES, MONTHS, Money, YEARS};
use crate::render::order::ground_height;

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
#[derive(Debug, Clone, Default, PartialEq, Eq)]
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
    /// The AI clock stamp of the entity base (zoo format 6.4): the tick a guest took its building
    /// slot, among others. A stored 0 is the current tick, so the loader keeps it as `None`.
    pub stamp: Option<u32>,
    /// The condition of a fence or tank wall of a version from 55; `None` for everything else.
    pub fence: Option<Fence>,
    /// The movement mode of a unit (3: inside a tank); 0 for everything else.
    pub movement_mode: u32,
    /// The food units left of a scenery entity, a signed counter (zoo format 6.6): kept as stored
    /// for the class `food` and the subtypes `poo` and `dinopoo`, 0 for everything else.
    pub food: i32,
    /// What an animal record stores of the animal's live state; `None` for everything else.
    pub animal: Option<Box<AnimalRecord>>,
    /// What a guest record stores of the guest; `None` for everything else.
    pub guest: Option<Box<GuestRecord>>,
    /// What a staff record stores of the staff member; `None` for everything else.
    pub staff: Option<Box<StaffRecord>>,
    /// The goal, target, movement, pending call and behaviour set of a unit; `None` for everything
    /// else.
    pub unit: Option<Box<UnitBlock>>,
    /// The health and timers of a tank filter; `None` for everything else.
    pub filter: Option<Filter>,
    /// The figures a guest building record stores; `None` for everything else.
    pub building: Option<BuildingRecord>,
    /// The body as read, which the writer patches with the fields above.
    pub raw: Raw,
}

/// Where parts of the entity base lie inside a body, so a writer can keep what it does not model:
/// the stamp, the colour count and the end of the colours. All 0 for an entity that was not read
/// from a file.
#[derive(Debug, Clone, Copy, Default)]
struct Spans {
    stamp: usize,
    colors: usize,
    end: usize,
}

/// The body of an entity record as read. It never makes two entities differ: the fields of [`Common`]
/// are the state, the bytes are what is kept of the rest.
#[derive(Debug, Clone, Default)]
pub struct Raw {
    body: Vec<u8>,
    spans: Spans,
}

impl Raw {
    /// The raw record of an entity read from a file, for tests of a loaded unit.
    #[cfg(test)]
    pub fn read() -> Raw {
        Raw {
            body: vec![0],
            ..Raw::default()
        }
    }

    /// Whether the entity was made in play: no body was read for it.
    pub fn is_new(&self) -> bool {
        self.body.is_empty()
    }
}

impl PartialEq for Raw {
    fn eq(&self, _: &Raw) -> bool {
        true
    }
}

impl Eq for Raw {}

/// The bytes of the sections the engine does not model, as read (save games 7): written back
/// unchanged, so they never make two zoos differ. The offsets tell where the modelled values lie in
/// `finances`.
#[derive(Debug, Clone, Default)]
pub struct Kept {
    /// From the habitat count to the end of the habitat records.
    habitats: Vec<u8>,
    /// The shows that precede the id counter (from version 97).
    shows: Vec<u8>,
    /// The shows after the marketing (versions 89 to 96).
    late_shows: Vec<u8>,
    /// The cheats (from version 104).
    cheats: Vec<u8>,
    /// The scenario block: its size and content.
    scenario: Vec<u8>,
    /// The game block, from the zoo status to the cash and the word after it.
    finances: Vec<u8>,
    /// Where the statistics of each habitat start in `habitats`.
    stats_at: Vec<usize>,
    /// Where the name of each habitat is in `habitats`, its length word included.
    names_at: Vec<(usize, usize)>,
    /// The thoughts as stored: text id, thinker, object, tile x and y (save games 9).
    thoughts: Vec<[u32; 5]>,
    /// The research records (from version 40).
    research: Vec<u8>,
    /// The marketing word (from version 60).
    marketing: Vec<u8>,
}

impl PartialEq for Kept {
    fn eq(&self, _: &Kept) -> bool {
        true
    }
}

impl Eq for Kept {}

/// What a guest building record stores of its figures (zoo format 6.9, guest buildings 4): the
/// guests and the income this month, last month and since construction, the upkeep paid since
/// construction (from version 68), the price, the users as (unit id, slot) pairs in entering
/// order, the count of users (the stored one, or the number of pairs if that is more), the trash
/// can's fill and the construction date (a file time).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BuildingRecord {
    pub guests: [u32; 3],
    /// The money figures as the bits of a single precision number.
    pub income: [u32; 3],
    pub upkeep: Option<u32>,
    /// The index of the building's current upgrade (from version 74, else 0).
    pub upgrade: u32,
    /// The price as the bits of a single precision number.
    pub price: u32,
    pub users: Vec<(u32, u32)>,
    pub users_now: u32,
    pub fill: u32,
    pub built: u64,
}

/// The milliseconds between two checks of a filter's period (tank staff 7.3 point 6).
pub const FILTER_CHECK_MS: i32 = 3000;

/// What a tank filter keeps (tank staff 7): its health, the milliseconds left before it loses a
/// point and before it filters again, the upkeep it paid in the last 30-day period, the current one
/// and since it was built (whole money), its build and last service dates (file times), the months
/// count of its periods (starts at 1) and the milliseconds before the next check of the period. A
/// loaded filter has no timers, since a record does not save them (zoo format 6.8): the first
/// update starts them from the type; the period check restarts at 3,000.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Filter {
    pub health: u32,
    pub timers: Option<[i32; 2]>,
    pub upkeep: [u32; 3],
    pub built: u64,
    pub serviced: u64,
    pub months: i32,
    pub check: i32,
}

/// What a guest record stores: the five status values (happiness, hunger, thirst, energy,
/// bathroom), the eleven check countdowns in the order of the guest's periods, the arrival date (a
/// file time), the name id of the favourite species, the sick flag, the saw-animal countdown,
/// female and child, the item slots, the other flags and the guide id (zoo format 6.12).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GuestRecord {
    pub values: [i32; 5],
    pub countdowns: [i32; 11],
    pub arrived: u64,
    pub favourite: i32,
    pub sick: bool,
    /// The saw-animal countdown, female and child.
    pub saw_animal: i32,
    pub female: bool,
    pub child: bool,
    /// The item slots, an item name id each, 0 for an empty slot (guests 9.2).
    pub items: [u32; 5],
    pub leave: bool,
    /// Seen a predator; nothing in the engine raises it yet.
    pub afraid: bool,
    pub gift: bool,
    pub wandered: bool,
    /// Eating standing.
    pub stand: bool,
    pub in_group: bool,
    pub listening: bool,
    /// The id of the tour guide followed, 0 for none.
    pub guide: u32,
}

/// The four values a fence or tank wall record holds after its common part.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Fence {
    pub strength: u32,
    /// A signed number, as the original tests it: below 1 (a stored value above `i32::MAX` too) the
    /// fence is broken; at or below the `cDecayedLife` of its type, decayed.
    pub life: i32,
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
    /// What the file holds that is not modelled.
    pub kept: Kept,
    /// The awards the zoo holds: no duplicates, largest id first (`awards.md` 4).
    pub awards: Vec<i32>,
}

/// What a habitat record stores besides its place (zoo format 4.1): the name, the nine
/// statistics as stored (habitat rating 13.3), the creation date (a file time) and the guests'
/// viewing time (the second date, habitat rating 13.2, a duration in file time units).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HabitatInfo {
    pub name: String,
    pub stats: [u32; 9],
    pub created: u64,
    pub viewing: u64,
    /// The AI tick a keeper last serviced the exhibit (`unknown_ec`, from version 83; 0 for never).
    pub serviced: u32,
}

/// What the zoo status keeps in a saved file (zoo status 16): its three timers in milliseconds, the
/// rating, the donation counter, the adult admission price and the date of the last escape (a file
/// time). The price of a file without the field is 49, the date 1 January 2000.
#[derive(Debug, Clone, PartialEq)]
pub struct StatusRecord {
    /// The rating, message and new guest timers.
    pub timers: [u32; 3],
    pub rating: u32,
    pub given: f32,
    pub price: f32,
    pub escape: u64,
}

impl Default for StatusRecord {
    fn default() -> Self {
        StatusRecord {
            timers: [0; 3],
            rating: 0,
            given: 0.0,
            price: 0.0,
            escape: upgrade::NEVER,
        }
    }
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
    /// The play time: milliseconds of unpaused running time (read from version 73, else counted from 0; always written).
    pub play_time: u32,
    /// The two event buffers (from version 97).
    pub queue: Queue,
    /// The game date, in game milliseconds from the calendar's start: negative before 2001, 0 when
    /// absent or refused.
    pub calendar: i64,
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
    /// The name of the scenario file the zoo holds (save games 7), if it holds one. Every goal
    /// of a file before version 51 restarts at 0 ([`Zoo::scenario_states`] is empty).
    pub fn scenario_file(&self) -> Option<String> {
        let block = self.kept.scenario.get(4..).filter(|b| !b.is_empty())?;
        if self.version < 51 {
            // Before 51 the block is the name alone, with no length word.
            return Some(latin1(block));
        }
        let len = u32::from_le_bytes(block.get(..4)?.try_into().ok()?) as usize;
        Some(latin1(block.get(4..)?.get(..len)?))
    }

    /// The goal states of the scenario block, one word per goal as stored (zoo format 7.2); none
    /// when the zoo holds no scenario or its version is before 51.
    pub fn scenario_states(&self) -> Vec<u32> {
        let words = || {
            let block = self.kept.scenario.get(4..).filter(|_| self.version >= 51)?;
            let name = 4 + u32::from_le_bytes(block.get(..4)?.try_into().ok()?) as usize;
            block.get(name + 4..)
        };
        let (words, _) = words().unwrap_or_default().as_chunks::<4>();
        words.iter().map(|w| u32::from_le_bytes(*w)).collect()
    }

    /// Replaces the scenario block written: the scenario `file` and its goal states.
    pub fn set_scenario(&mut self, file: &str, states: &[u32]) {
        let name: Vec<u8> = file.chars().map(|c| c as u8).collect();
        let count = 4 * states.len() as u32;
        let size = 8 + name.len() as u32 + count;
        let mut block = size.to_le_bytes().to_vec();
        block.extend((name.len() as u32).to_le_bytes());
        block.extend(name);
        block.extend(count.to_le_bytes());
        block.extend(states.iter().flat_map(|s| s.to_le_bytes()));
        self.kept.scenario = block;
    }

    /// The research section as read, from its leading word to its end record (empty before
    /// version 40).
    pub fn research(&self) -> &[u8] {
        &self.kept.research
    }

    /// Replaces the research section written (research 7.5).
    pub fn set_research(&mut self, section: Vec<u8>) {
        self.kept.research = section;
    }

    /// The thoughts as stored, newest first.
    pub fn thoughts(&self) -> &[[u32; 5]] {
        &self.kept.thoughts
    }

    /// Replaces the thoughts a save writes.
    pub fn set_thoughts(&mut self, thoughts: Vec<[u32; 5]>) {
        self.kept.thoughts = thoughts;
    }

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

    /// Drops the entities whose type `known` does not find, as the original's loader does when it
    /// skips their records: it makes no unit for them and so never saves them (zoo format 6.2).
    /// [`Zoo::checked`] calls it after the load; the engine drops them again when it writes.
    pub fn drop_unknown(&mut self, known: impl Fn(&Entity) -> bool) {
        self.entities.retain(known);
    }

    /// Applies [`Zoo::check_types`] with the types of `catalogue`, then drops the entities of an
    /// unknown type, so the world built from the zoo has no unit or id for them.
    pub fn checked(mut self, catalogue: &Catalogue) -> Result<Zoo, ZooError> {
        self.check_types(knows(catalogue))?;
        self.drop_unknown(knows(catalogue));
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
        let mut r = Reader::new(&bytes[8..], version);
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

/// Whether the types of `catalogue` have a definition for an entity.
pub fn knows(catalogue: &Catalogue) -> impl Fn(&Entity) -> bool + '_ {
    let bytes = |s: &str| s.chars().map(|c| c as u8).collect::<Vec<u8>>();
    move |e| {
        catalogue
            .find(&e.class, &bytes(&e.type_name), &bytes(&e.subtype))
            .is_some()
    }
}

/// A bounds-checked reader over untrusted bytes; `v` is the version of the file.
struct Reader<'a> {
    data: &'a [u8],
    v: u32,
    /// The length `data` had at the start of the body being read, and where its entity base lies.
    total: usize,
    spans: Spans,
    /// The behaviour set of the entity base just read.
    behaviour: Option<Behaviour>,
    /// The tile grid, once it is read: the old tiles of a unit become positions with its heights.
    ground: Option<Rc<Ground>>,
    /// The copy being made of what is read, in the form of version 106: fields the version lacks
    /// are put into it, fields it no longer has are left out. A copy is made when it is `want`ed,
    /// while `copy` is set.
    tee: Vec<u8>,
    want: bool,
    copy: bool,
    /// The copy of the fields before the unit block or the entity base, and after them.
    pre: Vec<u8>,
    post: Vec<u8>,
}

/// The map of a file, for the heights of the positions an old file stores as tiles.
struct Ground {
    width: u32,
    height: u32,
    tiles: Vec<Tile>,
}

impl Ground {
    /// The tile at the tile coordinates, if it is on the map.
    fn tile(&self, x: i32, y: i32) -> Option<&Tile> {
        let (x, y) = (u32::try_from(x).ok()?, u32::try_from(y).ok()?);
        if x >= self.width || y >= self.height {
            return None;
        }
        self.tiles
            .get(y as usize * self.width as usize + x as usize)
    }

    /// The height of the ground at a world point, 0 outside the map.
    fn at(&self, x: i32, y: i32) -> i32 {
        self.tile(x.div_euclid(64), y.div_euclid(64))
            .map_or(0, |t| ground_height(t, x.rem_euclid(64), y.rem_euclid(64)))
    }
}

impl<'a> Reader<'a> {
    fn new(data: &'a [u8], v: u32) -> Reader<'a> {
        Reader {
            data,
            v,
            total: data.len(),
            spans: Spans::default(),
            behaviour: None,
            ground: None,
            tee: Vec::new(),
            want: false,
            copy: false,
            pre: Vec::new(),
            post: Vec::new(),
        }
    }

    /// Reads without copying what `f` reads.
    fn quiet<T>(&mut self, f: impl FnOnce(&mut Self) -> R<T>) -> R<T> {
        let was = std::mem::replace(&mut self.copy, false);
        let out = f(self);
        self.copy = was;
        out
    }

    /// Puts bytes into the copy only (a field the version lacks).
    fn put(&mut self, bytes: &[u8]) {
        if self.copy {
            self.tee.extend(bytes);
        }
    }

    /// Starts copying, if a copy is wanted.
    fn copy_on(&mut self) {
        self.copy = self.want;
    }

    /// Ends the part of the copy before the unit block or the entity base.
    fn cut(&mut self) {
        self.pre = std::mem::take(&mut self.tee);
        self.copy = false;
    }

    /// The offset in the body being read.
    fn pos(&self) -> usize {
        self.total - self.data.len()
    }

    fn take(&mut self, len: usize) -> R<&'a [u8]> {
        if len > self.data.len() {
            return bad("truncated");
        }
        let (head, tail) = self.data.split_at(len);
        self.data = tail;
        if self.copy {
            self.tee.extend(head);
        }
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

    fn u64(&mut self) -> R<u64> {
        Ok(u64::from_le_bytes(self.take(8)?.try_into().unwrap()))
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
    let from = r.data;
    let habitat_count = r.count(HABITAT_MIN)?;
    let mut tanks = Vec::new();
    let mut habitat_tiles = Vec::new();
    let mut habitat_gates = Vec::new();
    let mut habitat_info = Vec::new();
    let mut stats_at = Vec::new();
    let mut names_at = Vec::new();
    for _ in 0..habitat_count {
        let (tile, gate, tank, info, at, name) = habitat(r, from.len())?;
        stats_at.push(at);
        names_at.push(name);
        habitat_tiles.push(tile);
        habitat_gates.push(gate);
        habitat_info.push(info);
        tanks.extend(tank);
    }
    if v >= 64 {
        r.skip(4)?;
    }
    let habitats = taken(from, r);
    let mut early_shows = Vec::new();
    if v >= 97 {
        *section = "shows";
        let from = r.data;
        shows(r)?;
        early_shows = taken(from, r);
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

    r.ground = Some(Rc::new(Ground {
        width,
        height,
        // A hostile elevation must not overflow the height.
        tiles: tiles
            .iter()
            .map(|t| Tile {
                elevation: t.elevation.clamp(-(1 << 24), 1 << 24),
                ..*t
            })
            .collect(),
    }));

    *section = "entities";
    let min_record = if v >= 70 { 16 } else { 12 };
    let count = r.count(min_record)?;
    let mut entities = Vec::with_capacity(count);
    for i in 0..count {
        let entity = entity(r).map_err(|Bad(why)| Bad(format!("entity {i}: {why}")))?;
        entities.push(entity);
    }

    *section = "later sections";
    let mut awards = Vec::new();
    let (view, clocks, names, mut kept) = after_entities(r, money, &mut awards)?;
    (kept.habitats, kept.shows) = (habitats, early_shows);
    kept.stats_at = stats_at;
    kept.names_at = names_at;
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
        kept,
        awards,
    })
}

/// The bytes `r` has read since `from` was its data.
fn taken(from: &[u8], r: &Reader) -> Vec<u8> {
    from[..from.len() - r.data.len()].to_vec()
}

/// The smallest habitat record: two tiles, a string, 4 + 36 bytes and two dates.
const HABITAT_MIN: usize = 76;

type Habitat = (
    (i32, i32),
    ((i32, i32), i32),
    Option<Tank>,
    HabitatInfo,
    usize,
    (usize, usize),
);

/// Reads a habitat record; `size` is the length of the bytes the section started with, and the last
/// values are where the record's statistics and its name start and end in them.
fn habitat(r: &mut Reader, size: usize) -> R<Habitat> {
    let v = r.v;
    let tile = (r.i32()?, r.i32()?);
    let name_at = size - r.data.len();
    let name = latin1(r.string()?);
    let name_to = size - r.data.len();
    let gate = (r.i32()?, r.i32()?);
    let side = r.i32()?;
    let at = size - r.data.len();
    let mut stats = [0; 9];
    for s in &mut stats {
        *s = r.u32()?;
    }
    let created = u64::from(r.u32()?) | u64::from(r.u32()?) << 32;
    let viewing = u64::from(r.u32()?) | u64::from(r.u32()?) << 32;
    let serviced = if v >= 83 { r.u32()? } else { 0 };
    let info = HabitatInfo {
        name,
        stats,
        created,
        viewing,
        serviced,
    };
    let is_tank = v >= 85 && r.u8()? != 0;
    let has_show = v >= 90 && r.u8()? != 0;
    if has_show {
        show_info(r)?;
    }
    if !is_tank {
        return Ok((tile, (gate, side), None, info, at, (name_at, name_to)));
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
        at,
        (name_at, name_to),
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
        let data = r.take(size as usize)?;
        let mut body = Reader::new(data, r.v);
        body.ground = r.ground.clone();
        // A body that does not decode to exactly its size is skipped, like an unknown class.
        let common = match body::decode(&mut body, class) {
            Ok(Some(mut common)) if body.data.is_empty() && size > 0 => {
                common.raw = Raw {
                    body: data.to_vec(),
                    spans: body.spans,
                };
                Some(common)
            }
            _ => None,
        };
        (Some(size), common)
    } else {
        let (from, v) = (r.data, r.v);
        let mut body = Reader::new(from, v);
        body.ground = r.ground.clone();
        match body::decode(&mut body, class)? {
            Some(mut common) => {
                let data = &from[..from.len() - body.data.len()];
                common.raw = Raw {
                    body: data.to_vec(),
                    spans: body.spans,
                };
                r.data = body.data;
                (None, Some(common))
            }
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
fn after_entities(
    r: &mut Reader,
    money: &mut Money,
    awards: &mut Vec<i32>,
) -> R<(View, Clocks, BTreeMap<String, u32>, Kept)> {
    let mut clocks = Clocks::default();
    let mut kept = Kept::default();
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
    let from = r.data;
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
    kept.scenario = taken(from, r);

    // Name counters: type name and number.
    let mut names = BTreeMap::new();
    for _ in 0..r.count(8)? {
        let name = latin1(r.string()?);
        names.insert(name, r.u32()?);
    }

    // Game and finances: the zoo status' timers, rating and donation counter first.
    let from = r.data;
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
    clocks.status.escape = if v >= 71 { r.u64()? } else { upgrade::NEVER };
    // The calendar: year, month, day of week, day, hour, minute, second, millisecond.
    let [year, month, _, day, hour, minute, second, millisecond] = [(); 8].map(|_| r.u16());
    clocks.calendar = crate::timing::Date {
        year: u32::from(year?),
        month: u32::from(month?),
        day: u32::from(day?),
        weekday: 0,
        hour: u32::from(hour?),
        minute: u32::from(minute?),
        second: u32::from(second?),
        millisecond: u32::from(millisecond?),
    }
    .game_ms();
    money.set_cash(r.f32()?);
    if v >= 73 {
        clocks.play_time = r.u32()?;
    }
    kept.finances = taken(from, r);

    kept.thoughts = (0..r.count(20)?)
        .map(|_| Ok([r.u32()?, r.u32()?, r.u32()?, r.u32()?, r.u32()?]))
        .collect::<R<_>>()?;
    // The AI clock: the milliseconds left over, then the whole AI ticks (7.6).
    (clocks.ai_rem, clocks.ai_ticks) = (r.u32()?, r.u32()?);
    if v >= 97 {
        clocks.queue.first = r.events()?;
        clocks.queue.second = r.events()?;
        clocks.queue.next_is_first = r.u8()? != 0;
    }
    let from = r.data;
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
    kept.research = taken(from, r);
    if v >= 59 {
        for _ in 0..r.count(4)? {
            crate::award::add(awards, r.i32()?);
        }
    }
    let from = r.data;
    if v >= 60 {
        r.skip(4)?; // marketing
    }
    kept.marketing = taken(from, r);
    if (89..=96).contains(&v) {
        let from = r.data;
        shows(r)?;
        kept.late_shows = taken(from, r);
    }
    if v >= 104 {
        let from = r.data;
        r.skip_strings()?; // cheats
        kept.cheats = taken(from, r);
    }
    if v >= 58 {
        // The world clock: the milliseconds left over, then the whole seconds (7.11).
        (clocks.world_rem, clocks.world_secs) = (r.u32()?, r.u32()?);
        r.skip(if v <= 66 { 4 } else { 0 })?;
    }
    Ok((view, clocks, names, kept))
}
