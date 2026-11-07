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
mod habitat;
mod read;
use habitat::{HABITAT_MIN, habitat};
pub(crate) use read::knows;
use read::{Bad, Marks, R, Reader, bad, latin1, read};
pub(crate) mod fresh;
#[cfg(test)]
mod parity;
mod show;
mod staff;
#[cfg(test)]
pub(crate) mod tests;
mod unit;
mod upgrade;
pub(crate) use upgrade::NEVER;
mod write;
pub(crate) use animal::animal_new;
pub(crate) use animal::{AnimalRecord, Escape};
#[cfg(test)]
pub(crate) use show::Show;
pub(crate) use show::{Item, ItemState, Script, ShowInfo, Shows, TypeRecord};
pub(crate) use staff::StaffRecord;
#[cfg(test)]
pub(crate) use unit::Level;
pub(crate) use unit::{Behaviour, Target, UnitBlock};

use std::collections::BTreeMap;
use std::error::Error;
use std::fmt;
use std::rc::Rc;

use crate::entity::Catalogue;
use crate::money::{CATEGORIES, MONTHS, Money, YEARS};
use crate::render::order::ground_height;

/// The oldest version the original accepts.
pub(crate) const MIN_VERSION: u32 = 38;

/// The newest version the original accepts (and the only one it writes).
pub(crate) const MAX_VERSION: u32 = 106;

/// The biggest file [`Zoo::parse`] reads: 32 MiB. The biggest file of the game is below 0.7 MiB;
/// the original has no limit, this one bounds the memory a hostile file can make the parser take.
pub(crate) const MAX_SIZE: usize = 32 * 1024 * 1024;

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

/// What a rubble piece stores besides its scenery fields (cheats 4.6).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct Rubble {
    /// The ids of all the pieces made by the same call.
    pub group: Vec<u32>,
    /// The name id of the type that turned to rubble.
    pub name_id: i32,
}

/// What an ambient creature stores before its entity base (ambients 8): the speed in movement
/// units per second and the accumulator of the remainder of its move.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct Flight {
    pub speed: u32,
    pub acc: u32,
}

impl Flight {
    /// The two fields as the record holds them.
    pub(crate) fn bytes(self) -> [u8; 8] {
        let mut bytes = [0; 8];
        bytes[..4].copy_from_slice(&self.speed.to_le_bytes());
        bytes[4..].copy_from_slice(&self.acc.to_le_bytes());
        bytes
    }
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
    /// slot, among others. A stored 0 is the AI clock held before the file's own is read (0 in a fresh
    /// process): the parser keeps it as `None` and the sim's load stamps it (from version 35).
    pub stamp: Option<u32>,
    /// The condition of a fence or tank wall of a version from 55; `None` for everything else.
    pub(crate) fence: Option<Fence>,
    /// The movement mode of a unit (3: inside a tank); 0 for everything else.
    pub movement_mode: u32,
    /// The food units left of a scenery entity, a signed counter (zoo format 6.6): kept as stored
    /// for the class `food` and the subtypes `poo` and `dinopoo`, 0 for everything else.
    pub food: i32,
    /// What an animal record stores of the animal's live state; `None` for everything else.
    pub(crate) animal: Option<Box<AnimalRecord>>,
    /// What a guest record stores of the guest; `None` for everything else.
    pub(crate) guest: Option<Box<GuestRecord>>,
    /// What a staff record stores of the staff member; `None` for everything else.
    pub(crate) staff: Option<Box<StaffRecord>>,
    /// The goal, target, movement, pending call and behaviour set of a unit; `None` for everything
    /// else.
    pub(crate) unit: Option<Box<UnitBlock>>,
    /// The health and timers of a tank filter; `None` for everything else.
    pub(crate) filter: Option<Filter>,
    /// The figures a guest building record stores; `None` for everything else.
    pub(crate) building: Option<BuildingRecord>,
    /// The group and the type name id of a rubble piece; `None` for everything else.
    pub(crate) rubble: Option<Rubble>,
    /// The speed and the accumulator of an ambient creature (ambients 8); `None` for everything
    /// else.
    pub(crate) flight: Option<Flight>,
    /// The body as read, which the writer patches with the fields above.
    pub(crate) raw: Raw,
}

/// Where parts of the entity base lie inside a body, so a writer can keep what it does not model:
/// the stamp, the colour count and the end of the colours. All 0 for an entity that was not read
/// from a file.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
struct Spans {
    stamp: usize,
    colors: usize,
    end: usize,
}

/// The body of an entity record as read: the bytes are what is kept of the rest of the record, and
/// they count in the equality of two entities.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct Raw {
    body: Vec<u8>,
    spans: Spans,
}

impl Raw {
    /// The raw record of an entity read from a file, for tests of a loaded unit.
    #[cfg(test)]
    pub(crate) fn read() -> Raw {
        Raw {
            body: vec![0],
            ..Raw::default()
        }
    }

    /// Whether the entity was made in play: no body was read for it.
    pub(crate) fn is_new(&self) -> bool {
        self.body.is_empty()
    }
}

/// The bytes of the sections the engine does not model, as read (save games 7): written back
/// unchanged. The offsets tell where the modelled values lie in `finances`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct Kept {
    /// From the habitat count to the end of the habitat records.
    habitats: Vec<u8>,
    /// Where the has-show byte of each habitat record starts and where its show info ends in
    /// `habitats` (from version 90).
    show_at: Vec<Option<(usize, usize)>>,
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

/// What a guest building record stores of its figures (zoo format 6.9, guest buildings 4): the
/// guests and the income this month, last month and since construction, the upkeep paid since
/// construction (from version 68), the price, the users as (unit id, slot) pairs in entering
/// order, the count of users (the stored one, or the number of pairs if that is more), the trash
/// can's fill, the construction date (a file time) and the members (unit ids, from version 75).
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct BuildingRecord {
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
    pub members: Vec<u32>,
}

/// The milliseconds between two checks of a filter's period (tank staff 7.3 point 6).
pub(crate) const FILTER_CHECK_MS: i32 = 3000;

/// What a tank filter keeps (tank staff 7): its health, the milliseconds left before it loses a
/// point and before it filters again, the upkeep it paid in the last 30-day period, the current one
/// and since it was built (whole money), its build and last service dates (file times), the months
/// count of its periods (starts at 1) and the milliseconds before the next check of the period. A
/// loaded filter has no timers, since a record does not save them (zoo format 6.8): the first
/// update starts them from the type; the period check restarts at 3,000.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) struct Filter {
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
pub(crate) struct GuestRecord {
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
    /// The return word, as stored (zoo format 6.12): the return building's name id in the low 19
    /// bits, the recovering flag in bit 19. Versions before 94 have none (0).
    pub ret: u32,
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
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) struct Fence {
    pub strength: u32,
    /// A signed number, as the original tests it: below 1 (a stored value above `i32::MAX` too) the
    /// fence is broken; at or below the `cDecayedLife` of its type, decayed.
    pub life: i32,
    /// Two decay timers.
    pub timers: [u32; 2],
    /// A tank wall whose first trailing byte is not 0 is a portal.
    pub portal: bool,
    /// The second trailing byte of a tank wall: a portal is open while an animal crosses it
    /// (shows 21.5).
    pub open: bool,
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
#[cfg_attr(test, derive(Default))]
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
    pub(crate) habitat_info: Vec<HabitatInfo>,
    /// The tank exhibits, in file order.
    pub tanks: Vec<Tank>,
    /// The show scripts, the id counters and the show info of each habitat (shows 3, 11).
    pub(crate) shows: Shows,
    /// The highest entity id handed out so far.
    pub id_counter: u32,
    /// `width * height` tiles, row by row: the tile in column x and row y is `y * width + x`.
    pub tiles: Vec<Tile>,
    pub entities: Vec<Entity>,
    /// The camera the file stores.
    pub(crate) view: View,
    /// The AI clock and the world clock the file stores.
    pub(crate) clocks: Clocks,
    /// How many units of each type name were named so far (zoo format 7.3), for the number in a
    /// default name.
    pub names: BTreeMap<String, u32>,
    /// What the file holds that is not modelled.
    pub(crate) kept: Kept,
    /// The awards the zoo holds: no duplicates, largest id first (`awards.md` 4).
    pub awards: Vec<i32>,
}

/// What a habitat record stores besides its place (zoo format 4.1): the name, the nine
/// statistics as stored (habitat rating 13.3), the creation date (a file time) and the guests'
/// viewing time (the second date, habitat rating 13.2, a duration in file time units).
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct HabitatInfo {
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
pub(crate) struct StatusRecord {
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
pub(crate) struct Clocks {
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
pub(crate) struct Event {
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
pub(crate) struct Queue {
    pub first: Vec<Event>,
    pub second: Vec<Event>,
    pub next_is_first: bool,
}

/// The view state of a file (zoo format 7.1), as stored: nothing is checked.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct View {
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
    pub(crate) fn scenario_file(&self) -> Option<String> {
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
    pub(crate) fn scenario_states(&self) -> Vec<u32> {
        let words = || {
            let block = self.kept.scenario.get(4..).filter(|_| self.version >= 51)?;
            let name = 4 + u32::from_le_bytes(block.get(..4)?.try_into().ok()?) as usize;
            block.get(name + 4..)
        };
        let (words, _) = words().unwrap_or_default().as_chunks::<4>();
        words.iter().map(|w| u32::from_le_bytes(*w)).collect()
    }

    /// Replaces the scenario block written: the scenario `file` and its goal states.
    pub(crate) fn set_scenario(&mut self, file: &str, states: &[u32]) {
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
    pub(crate) fn research(&self) -> &[u8] {
        &self.kept.research
    }

    /// Replaces the research section written (research 7.5).
    pub(crate) fn set_research(&mut self, section: Vec<u8>) {
        self.kept.research = section;
    }

    /// The thoughts as stored, newest first.
    pub(crate) fn thoughts(&self) -> &[[u32; 5]] {
        &self.kept.thoughts
    }

    /// Replaces the thoughts a save writes.
    pub(crate) fn set_thoughts(&mut self, thoughts: Vec<[u32; 5]>) {
        self.kept.thoughts = thoughts;
    }

    /// The tile in column `x` and row `y`, if it is on the map.
    pub(crate) fn tile(&self, x: u32, y: u32) -> Option<&Tile> {
        if x >= self.width || y >= self.height {
            return None;
        }
        self.tiles
            .get(y as usize * self.width as usize + x as usize)
    }

    /// Refuses a file of a version below 70 that holds an entity whose type `known` does not
    /// find, as the original does: there is no body size to skip it by, so the whole load
    /// fails. From version 70 such an entity is skipped, so the file passes.
    pub(crate) fn check_types(&self, known: impl Fn(&Entity) -> bool) -> Result<(), ZooError> {
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
    pub(crate) fn drop_unknown(&mut self, known: impl Fn(&Entity) -> bool) {
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
        Zoo::parse_marked(bytes, false).map(|(zoo, money, _)| (zoo, money))
    }

    /// Where each section of a file ends (a name and the offset in the file, an entity as
    /// `entity/CLASS`), for the parity report of the full-game tests.
    #[cfg(test)]
    pub(crate) fn sections(bytes: &[u8]) -> Result<Marks, ZooError> {
        Zoo::parse_marked(bytes, true).map(|(_, _, marks)| marks)
    }

    fn parse_marked(bytes: &[u8], marked: bool) -> Result<(Zoo, Money, Marks), ZooError> {
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
        r.marks = marked.then(Vec::new);
        let mut section = "header";
        let mut money = Money::default();
        read(&mut r, &mut section, &mut money)
            .map(|zoo| (zoo, money, r.marks.take().unwrap_or_default()))
            .map_err(|Bad(why)| ZooError::Corrupted(format!("{section}: {why}")))
    }
}
