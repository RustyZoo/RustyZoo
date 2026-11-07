//! The shows of a file (zoo format 4.2 and 4.3, shows 3 and 11): the scripts and the two id
//! counters of the shows section, and the show info inside the record of a tank. Both are read
//! into fields and written back from them as version 106.

use super::write::Out;
use super::{Event, R, Reader};

/// The first script id and the first show id of a fresh map.
const FIRST_SCRIPT: u16 = 2000;
const FIRST_SHOW: u16 = 1000;

/// What a file stores of the shows.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Shows {
    pub scripts: Vec<Script>,
    /// The next script id and the next show id (from version 97; a fresh map holds 2000 and 1000).
    pub next_script: u16,
    pub next_show: u16,
    /// The show info of each habitat record, in file order: `None` for a habitat that has none. Shorter
    /// than the records for a zoo made in memory.
    pub infos: Vec<Option<ShowInfo>>,
}

impl Default for Shows {
    fn default() -> Shows {
        Shows {
            scripts: Vec::new(),
            next_script: FIRST_SCRIPT,
            next_show: FIRST_SHOW,
            infos: Vec::new(),
        }
    }
}

impl Shows {
    /// The show info of habitat record `n`.
    pub(crate) fn info(&self, n: usize) -> Option<&ShowInfo> {
        self.infos.get(n)?.as_ref()
    }

    /// The scripts section (from version 89) and the counters (from 97), which replace the ones held.
    pub(super) fn read_section(&mut self, r: &mut Reader) -> R<()> {
        let v = r.v;
        if v >= 89 {
            self.scripts = (0..r.count(2 + 4 + 4 + 4)?)
                .map(|_| Script::read(r))
                .collect::<R<_>>()?;
        }
        if v >= 97 {
            (self.next_script, self.next_show) = (r.u16()?, r.u16()?);
        }
        Ok(())
    }

    /// The section as version 106 stores it.
    pub(super) fn write_section(&self, o: &mut Out) -> Result<(), String> {
        o.u32(self.scripts.len() as u32);
        for s in &self.scripts {
            s.write(o)?;
        }
        o.u16(self.next_script);
        o.u16(self.next_show);
        Ok(())
    }
}

#[cfg(test)]
impl Shows {
    /// The section and each show info written as version 106 and read back, as a save and a load
    /// do (the unit lists are not stored).
    pub(crate) fn saved_and_read(&self) -> Shows {
        let mut o = Out(Vec::new());
        self.write_section(&mut o).unwrap();
        let infos: Vec<_> = self.infos.iter().flatten().collect();
        infos.iter().for_each(|i| i.write(&mut o));
        let mut r = Reader::new(&o.0, 106);
        let mut back = Shows::default();
        back.read_section(&mut r)
            .unwrap_or_else(|e| panic!("{}", e.0));
        back.infos = (infos.iter())
            .map(|_| Some(ShowInfo::read(&mut r).unwrap_or_else(|e| panic!("{}", e.0))))
            .collect();
        assert!(r.data.is_empty());
        back
    }
}

/// A numbered list of tricks for one species (zoo format 4.2).
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Script {
    pub id: u16,
    /// 0xffffffff when fresh.
    pub marker: u32,
    /// The script type.
    pub kind: u32,
    pub items: Vec<Item>,
}

/// One trick of a script. The satisfaction is the item's possible gain, `current` the one it
/// gives now (shows 8.1).
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Item {
    /// Available by default, visible.
    pub available: u8,
    pub visible: u8,
    pub id: u16,
    /// The type id of the animal the item was made for (the word the item's maker sets).
    pub kind: u32,
    /// The trick type: 0 behaviour, 1 toy, 3 food, which scores nothing (shows 8.1, 19.1).
    pub marker: u32,
    /// The trick name, the animation, the keeper's pre-trick and post-trick.
    pub names: [String; 4],
    pub building: u32,
    pub complexity: u32,
    pub returns: u8,
    pub satisfaction: i32,
    pub delta: i32,
    pub current: i32,
    pub depth: i32,
    /// The help ids and the names of the normal and the greyed icon (from version 103).
    pub help: [u32; 2],
    pub icons: [String; 2],
}

impl Script {
    fn read(r: &mut Reader) -> R<Script> {
        let (id, marker, kind) = (r.u16()?, r.u32()?, r.u32()?);
        let items = (0..r.count(53)?).map(|_| Item::read(r)).collect::<R<_>>()?;
        Ok(Script {
            id,
            marker,
            kind,
            items,
        })
    }

    fn write(&self, o: &mut Out) -> Result<(), String> {
        o.u16(self.id);
        o.u32(self.marker);
        o.u32(self.kind);
        o.u32(self.items.len() as u32);
        self.items.iter().try_for_each(|i| i.write(o))
    }
}

impl Item {
    fn read(r: &mut Reader) -> R<Item> {
        let (available, visible, id) = (r.u8()?, r.u8()?, r.u16()?);
        let (kind, marker) = (r.u32()?, r.u32()?);
        let names = [r.text()?, r.text()?, r.text()?, r.text()?];
        let (building, complexity, returns) = (r.u32()?, r.u32()?, r.u8()?);
        let [satisfaction, delta, current, depth] = [r.i32()?, r.i32()?, r.i32()?, r.i32()?];
        let (mut help, mut icons) = ([0; 2], [String::new(), String::new()]);
        if r.v >= 103 {
            help = [r.u32()?, r.u32()?];
            icons = [r.text()?, r.text()?];
        }
        Ok(Item {
            available,
            visible,
            id,
            kind,
            marker,
            names,
            building,
            complexity,
            returns,
            satisfaction,
            delta,
            current,
            depth,
            help,
            icons,
        })
    }

    fn write(&self, o: &mut Out) -> Result<(), String> {
        o.u8(self.available);
        o.u8(self.visible);
        o.u16(self.id);
        o.u32(self.kind);
        o.u32(self.marker);
        for s in &self.names {
            o.string(s)?;
        }
        o.u32(self.building);
        o.u32(self.complexity);
        o.u8(self.returns);
        for n in [self.satisfaction, self.delta, self.current, self.depth] {
            o.i32(n);
        }
        o.u32(self.help[0]);
        o.u32(self.help[1]);
        o.string(&self.icons[0])?;
        o.string(&self.icons[1])
    }
}

/// The show info of a tank (shows 3.2). The money figures are the bits of single precision
/// numbers; each trio is this month, last month and the total.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ShowInfo {
    /// AI ticks between two shows, -1 for never.
    pub frequency: i32,
    /// The index of the scheduled species in `schedule`.
    pub index: u32,
    /// The species type ids that have a script.
    pub schedule: Vec<u32>,
    pub records: Vec<TypeRecord>,
    /// The info's own event list (channel 0x53).
    pub events: Vec<Event>,
    /// The next show time in AI ticks, -1 for never.
    pub next: i32,
    pub id: u16,
    pub upkeep: [u32; 3],
    pub receipts: [u32; 3],
    pub attendance: [u32; 3],
    pub show: Show,
}

/// The record of one entity type in a show info: a species, or the marine specialist.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct TypeRecord {
    pub key: u32,
    pub script: u16,
    /// The script the show editor chose, 0xffff for none (also in a file before version 99).
    pub pending: u16,
    /// The admission as the bits of a single precision number.
    pub admission: u32,
    pub receipts: [u32; 3],
    pub attendance: [u32; 3],
    /// The creation date (a file time); 0 in a file before version 99.
    pub date: u64,
    /// The ids of the units that take part, in joining order (shows 3.2, 20). Not saved: the units
    /// join again when they are put on the map.
    pub units: Vec<u32>,
    /// The editor may still make the default script for this record (information panels 19.9.2
    /// point 7): up at a creation and a load, not saved; only the editor clears it.
    pub default_flag: bool,
}

/// The running part of a show info (shows 3.3).
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Show {
    pub script: u16,
    pub id: u16,
    pub species: u32,
    pub keeper: u32,
    pub word: u32,
    pub state_word: u16,
    pub ready: bool,
    pub running: bool,
    pub stopped: bool,
    pub start: u32,
    pub done: u32,
    pub possible: u32,
    pub achieved: u32,
    pub items: Vec<ItemState>,
}

impl Default for Show {
    /// A new show: no script, everything 0, stopped.
    fn default() -> Show {
        Show {
            script: 0,
            id: 0,
            species: 0,
            keeper: 0,
            word: 0,
            state_word: 0,
            ready: false,
            running: false,
            stopped: true,
            start: 0,
            done: 0,
            possible: 0,
            achieved: 0,
            items: Vec::new(),
        }
    }
}

/// The state of one animal in a show (shows 3.4).
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ItemState {
    /// The show's current script when the state is made.
    pub script: u16,
    pub animal: u32,
    /// -1 before the first item.
    pub item: i16,
    pub finished: bool,
    pub aborted: bool,
    pub cue_sent: bool,
    pub ready: bool,
    pub started: bool,
    pub done: bool,
}

impl ShowInfo {
    /// A new show info (shows 31.1 point 2): the show of 3.3 new, no type record, no schedule, every
    /// figure 0, and the frequency and the next show time given.
    pub(crate) fn new(id: u16, frequency: i32, next: i32) -> ShowInfo {
        ShowInfo {
            frequency,
            index: 0,
            schedule: Vec::new(),
            records: Vec::new(),
            events: Vec::new(),
            next,
            id,
            upkeep: [0; 3],
            receipts: [0; 3],
            attendance: [0; 3],
            show: Show::default(),
        }
    }

    /// The record of type `t`, for a reader that is no lookup of the original (see `lookup`).
    pub(crate) fn record(&self, t: u32) -> Option<&TypeRecord> {
        self.records.iter().find(|r| r.key == t)
    }

    /// The record of type `t`, made empty when there is none (shows 3.2, 32.7): no script, no
    /// pending one, no figure, dated `date`, put in key order.
    pub(crate) fn lookup(&mut self, t: u32, date: u64) -> &mut TypeRecord {
        let at = match self.records.binary_search_by_key(&t, |r| r.key) {
            Ok(at) => at,
            Err(at) => {
                (self.records).insert(at, TypeRecord::new(t, u16::MAX, date));
                at
            }
        };
        &mut self.records[at]
    }

    /// A seat of price `r` is sold on file time `date` (shows 7.3 point 3): the receipts of the
    /// type record of the show's species (made when missing) and then of the info, this month and
    /// in all, and the attendance in the same four places.
    pub(crate) fn book(&mut self, r: f32, date: u64) {
        let species = self.show.species;
        let sold = |receipts: &mut [u32; 3], attendance: &mut [u32; 3]| {
            for n in [0, 2] {
                receipts[n] = (f32::from_bits(receipts[n]) + r).to_bits();
                attendance[n] = attendance[n].wrapping_add(1);
            }
        };
        let record = self.lookup(species, date);
        sold(&mut record.receipts, &mut record.attendance);
        sold(&mut self.receipts, &mut self.attendance);
    }

    /// The month change (shows 9.3): receipts and attendance of this month become last month's,
    /// for the info and each type record; the upkeep takes the new rate `upkeep` and the total
    /// adds it. The cumulative receipts and attendance stay.
    pub(crate) fn new_month(&mut self, upkeep: f32) {
        let shift = |f: &mut [u32; 3]| (f[1], f[0]) = (f[0], 0);
        for t in &mut self.records {
            shift(&mut t.receipts);
            shift(&mut t.attendance);
        }
        shift(&mut self.receipts);
        shift(&mut self.attendance);
        let [this, last, total] = &mut self.upkeep;
        (*last, *this) = (*this, upkeep.to_bits());
        *total = (f32::from_bits(*total) + upkeep).to_bits();
    }

    /// The units of type `t` that take part (shows 3.2), read without a record made (tests).
    #[cfg(test)]
    pub(crate) fn units(&self, t: u32) -> &[u32] {
        self.record(t).map_or(&[], |r| &r.units)
    }

    /// The species whose turn it is.
    pub(crate) fn scheduled(&self) -> Option<u32> {
        self.schedule.get(self.index as usize).copied()
    }

    /// The script id of the scheduled species, 0 for none (shows 5.2).
    pub(crate) fn scheduled_script(&self) -> u16 {
        (self.scheduled()).map_or(0, |t| self.record(t).map_or(0, |r| r.script))
    }

    /// Running the scheduled script (shows 3.3).
    pub(crate) fn is_started(&self) -> bool {
        self.show.running && self.show.script == self.scheduled_script()
    }

    /// Reads a show info of the version being read; the fields it lacks stay as a fresh info has
    /// them.
    pub(super) fn read(r: &mut Reader) -> R<ShowInfo> {
        let v = r.v;
        let (frequency, index) = (r.i32()?, r.u32()?);
        let schedule = (0..r.count(4)?).map(|_| r.u32()).collect::<R<Vec<_>>>()?;
        if v <= 98 {
            r.skip_list(8)?;
        }
        let records = (0..r.count(if v >= 99 { 44 } else { 6 })?)
            .map(|_| TypeRecord::read(r))
            .collect::<R<_>>()?;
        let mut info = ShowInfo {
            frequency,
            index,
            schedule,
            records,
            events: Vec::new(),
            next: 0,
            id: 0,
            upkeep: [0; 3],
            receipts: [0; 3],
            attendance: [0; 3],
            show: Show::default(),
        };
        if v >= 97 {
            info.events = r.events()?;
            (info.next, info.id) = (r.i32()?, r.u16()?);
        }
        if v >= 105 {
            info.upkeep = [r.u32()?, r.u32()?, r.u32()?];
        }
        if v >= 106 {
            info.receipts = [r.u32()?, r.u32()?, r.u32()?];
            info.attendance = [r.u32()?, r.u32()?, r.u32()?];
        } else {
            // Not stored before 106: the sums of the type records (shows 11), receipts in single
            // precision.
            for n in 0..3 {
                let sum =
                    (info.records.iter()).fold(0.0f32, |a, t| a + f32::from_bits(t.receipts[n]));
                info.receipts[n] = sum.to_bits();
                info.attendance[n] =
                    (info.records.iter()).fold(0u32, |a, t| a.wrapping_add(t.attendance[n]));
            }
        }
        if v >= 97 {
            info.show = Show::read(r)?;
        }
        if v == 101 {
            r.skip(1)?;
        }
        Ok(info)
    }

    /// The info as version 106 stores it.
    pub(super) fn write(&self, o: &mut Out) {
        o.i32(self.frequency);
        o.u32(self.index);
        o.u32(self.schedule.len() as u32);
        self.schedule.iter().for_each(|&n| o.u32(n));
        o.u32(self.records.len() as u32);
        // Saved in increasing type id (shows 32.7 point 4).
        let mut records: Vec<_> = self.records.iter().collect();
        records.sort_by_key(|t| t.key);
        records.into_iter().for_each(|t| t.write(o));
        o.u32(self.events.len() as u32);
        for e in &self.events {
            o.event(e);
        }
        o.i32(self.next);
        o.u16(self.id);
        for n in [self.upkeep, self.receipts, self.attendance].concat() {
            o.u32(n);
        }
        self.show.write(o);
    }
}

impl TypeRecord {
    /// A record with no script and no pending one, no figures and no unit.
    pub(crate) fn new(key: u32, script: u16, date: u64) -> TypeRecord {
        TypeRecord {
            key,
            script,
            pending: u16::MAX,
            admission: 0,
            receipts: [0; 3],
            attendance: [0; 3],
            date,
            units: Vec::new(),
            default_flag: true,
        }
    }

    fn read(r: &mut Reader) -> R<TypeRecord> {
        let (key, script) = (r.u32()?, r.u16()?);
        let mut record = TypeRecord::new(key, script, 0);
        if r.v >= 99 {
            record.pending = r.u16()?;
            record.admission = r.u32()?;
            record.receipts = [r.u32()?, r.u32()?, r.u32()?];
            record.attendance = [r.u32()?, r.u32()?, r.u32()?];
            record.date = r.u64()?;
        }
        Ok(record)
    }

    fn write(&self, o: &mut Out) {
        o.u32(self.key);
        o.u16(self.script);
        o.u16(self.pending);
        o.u32(self.admission);
        for n in self.receipts.into_iter().chain(self.attendance) {
            o.u32(n);
        }
        o.bytes(&self.date.to_le_bytes());
    }
}

impl Show {
    fn read(r: &mut Reader) -> R<Show> {
        let (script, id) = (r.u16()?, r.u16()?);
        let (species, keeper, word) = (r.u32()?, r.u32()?, r.u32()?);
        let state_word = r.u16()?;
        let (ready, running, stopped) = (r.u8()? != 0, r.u8()? != 0, r.u8()? != 0);
        let [start, done, possible, achieved] = [r.u32()?, r.u32()?, r.u32()?, r.u32()?];
        let items = (0..r.count(14)?)
            .map(|_| {
                Ok(ItemState {
                    script: r.u16()?,
                    animal: r.u32()?,
                    item: r.u16()? as i16,
                    finished: r.u8()? != 0,
                    aborted: r.u8()? != 0,
                    cue_sent: r.u8()? != 0,
                    ready: r.u8()? != 0,
                    started: r.u8()? != 0,
                    done: r.u8()? != 0,
                })
            })
            .collect::<R<_>>()?;
        Ok(Show {
            script,
            id,
            species,
            keeper,
            word,
            state_word,
            ready,
            running,
            stopped,
            start,
            done,
            possible,
            achieved,
            items,
        })
    }

    fn write(&self, o: &mut Out) {
        o.u16(self.script);
        o.u16(self.id);
        o.u32(self.species);
        o.u32(self.keeper);
        o.u32(self.word);
        o.u16(self.state_word);
        for b in [self.ready, self.running, self.stopped] {
            o.u8(b.into());
        }
        for n in [self.start, self.done, self.possible, self.achieved] {
            o.u32(n);
        }
        o.u32(self.items.len() as u32);
        // The item states of a map by animal id come in increasing id (shows 32.7 point 4).
        let mut items: Vec<_> = self.items.iter().collect();
        items.sort_by_key(|s| s.animal);
        for s in items {
            o.u16(s.script);
            o.u32(s.animal);
            o.u16(s.item as u16);
            for b in [
                s.finished, s.aborted, s.cue_sent, s.ready, s.started, s.done,
            ] {
                o.u8(b.into());
            }
        }
    }
}
