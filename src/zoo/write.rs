//! The writer: the mirror of the reader (save games 6, 7). Every section is written in the
//! order of the format, from the model where the engine has one and from the bytes kept as read
//! elsewhere. The version written is 106, whatever the version read.

use super::{Behaviour, Common, Entity, MAX_VERSION, MIN_VERSION, Zoo};
use super::{fresh, unit, upgrade};
use crate::money::{CATEGORIES, Money, YEARS};
use crate::timing::Calendar;

pub(super) struct Out(pub(super) Vec<u8>);

impl Out {
    pub(super) fn u8(&mut self, n: u8) {
        self.0.push(n);
    }
    pub(super) fn u16(&mut self, n: u16) {
        self.0.extend(n.to_le_bytes());
    }
    pub(super) fn u32(&mut self, n: u32) {
        self.0.extend(n.to_le_bytes());
    }
    pub(super) fn i32(&mut self, n: i32) {
        self.u32(n as u32);
    }
    pub(super) fn bytes(&mut self, b: &[u8]) {
        self.0.extend(b);
    }
    /// A string of one byte per character, below 4096 bytes.
    pub(super) fn string(&mut self, s: &str) -> Result<(), String> {
        if s.chars().count() >= 4096 || s.chars().any(|c| c as u32 > 255) {
            return Err(format!("a string of {} characters", s.chars().count()));
        }
        self.u32(s.chars().count() as u32);
        self.0.extend(s.chars().map(|c| c as u8));
        Ok(())
    }
}

impl Zoo {
    /// The file of this zoo, with the cash and accounts of `money`, as version 106 (save games 6
    /// and 11.2): what the file read lacks of that version takes the values of the original. Fails
    /// for an entity the writer has no body for, for a zoo that was not read from a file, and for
    /// the files this writer cannot upgrade yet (a version below 70, shows below 103 that hold
    /// scripts, a habitat with a show below 106).
    pub fn write(&self, money: &Money) -> Result<Vec<u8>, String> {
        let v = self.version;
        if !(MIN_VERSION..=MAX_VERSION).contains(&v) {
            return Err(format!("version {v} cannot be written"));
        }
        if v < 70 {
            return Err(format!("version {v} cannot be upgraded yet (70 and later)"));
        }
        let kept = &self.kept;
        if kept.habitats.is_empty() || kept.finances.is_empty() {
            return Err("the zoo was not read from a file".into());
        }
        let mut o = Out(b"TZFB".to_vec());
        o.u32(MAX_VERSION);
        o.u32(self.locale);
        o.u32(self.expansion);
        o.u32(self.width);
        o.u32(self.height);
        o.u32(self.entrance.0);
        o.u32(self.entrance.1);
        o.bytes(&upgrade::habitats(v, &habitats(self)?)?);
        o.bytes(&upgrade::shows(v, kept)?);
        o.u32(self.id_counter);
        if self.tiles.len() as u64 != u64::from(self.width) * u64::from(self.height) {
            return Err("the tiles do not fill the map".into());
        }
        for t in &self.tiles {
            o.i32(t.elevation);
            o.u8(t.shape);
            o.u8(t.terrain);
            o.u8(t.edges);
            o.bytes(&[0; 3]);
        }
        o.u32(self.entities.len() as u32);
        let now = crate::timing::file_time(self.clocks.calendar);
        for (i, e) in self.entities.iter().enumerate() {
            entity(&mut o, v, now, e).map_err(|why| format!("entity {i}: {why}"))?;
        }

        let view = &self.view;
        for n in [view.zoom, view.rotation, view.scroll.0, view.scroll.1] {
            o.i32(n);
        }
        o.u32(view.brush);
        o.bytes(&kept.scenario);
        // Only the counters above 0, as the original does.
        let named = || self.names.iter().filter(|&(_, &n)| n > 0);
        o.u32(named().count() as u32);
        for (name, &n) in named() {
            o.string(name)?;
            o.u32(n);
        }
        o.bytes(&finances(self, money)?);
        o.u32(kept.thoughts.len() as u32);
        for n in kept.thoughts.iter().flatten() {
            o.u32(*n);
        }
        o.u32(self.clocks.ai_rem);
        o.u32(self.clocks.ai_ticks);
        let q = &self.clocks.queue;
        for list in [&q.first, &q.second] {
            o.u32(list.len() as u32);
            for e in list {
                o.u32(e.sender);
                o.u32(e.target);
                o.u32(e.data);
                o.u16(e.extra);
                o.u16(e.kind);
                o.u16(e.delay);
                o.u8(e.sender_channel);
                o.u8(e.channel);
            }
        }
        o.u8(q.next_is_first.into());
        o.bytes(&kept.research);
        // No cheat was used in a file of a version before 104.
        match v >= 104 {
            true => o.bytes(&kept.cheats),
            false => o.u32(0),
        }
        o.u32(self.clocks.world_rem);
        o.u32(self.clocks.world_secs);
        Ok(o.0)
    }
}

/// The habitat section as read, with the names, the statistics and the last service of each
/// habitat written over it.
fn habitats(zoo: &Zoo) -> Result<Vec<u8>, String> {
    let mut b = zoo.kept.habitats.clone();
    for (&at, info) in zoo.kept.stats_at.iter().zip(&zoo.habitat_info) {
        // The statistics, two dates, then the last service (from 83).
        let service = at + 36 + 16;
        let mut put = |at: usize, n: u32| {
            if let Some(to) = b.get_mut(at..at + 4) {
                to.copy_from_slice(&n.to_le_bytes());
            }
        };
        for (i, &n) in info.stats.iter().enumerate() {
            put(at + 4 * i, n);
        }
        if zoo.version >= 83 {
            put(service, info.serviced);
        }
    }
    // The names last, from the end, so that the places of the earlier ones stay right.
    for (&(from, to), info) in zoo.kept.names_at.iter().zip(&zoo.habitat_info).rev() {
        let mut name = Out(Vec::new());
        name.string(&info.name)?;
        b.splice(from..to, name.0);
    }
    Ok(b)
}

/// The game block as read, with the modelled values written over it, and the fields a version lacks
/// (31 rows of accounts, the date after the adult price, the word after the cash) added.
fn finances(zoo: &Zoo, money: &Money) -> Result<Vec<u8>, String> {
    let (kept, status) = (&zoo.kept, &zoo.clocks.status);
    let b = &kept.finances;
    let short = || "the game block is short".to_string();
    let mut o = Out(b.get(..45).ok_or_else(short)?.to_vec());
    for (i, t) in status.timers.iter().enumerate() {
        o.0[4 + 4 * i..8 + 4 * i].copy_from_slice(&t.to_le_bytes());
    }
    o.0[17..21].copy_from_slice(&status.rating.to_le_bytes());
    o.0[33..37].copy_from_slice(&status.given.to_le_bytes());
    o.0[37..41].copy_from_slice(&money.month.to_le_bytes());
    o.0[41..45].copy_from_slice(&money.year.to_le_bytes());
    o.u32(CATEGORIES as u32);
    o.u32(YEARS as u32);
    let floats = money.monthly.iter().flatten();
    let floats = floats.chain(money.yearly.iter().flatten());
    for f in floats.chain(&money.total).chain([&status.price]) {
        o.u32(f.to_bits());
    }
    // The date after the price (from 71) is 1 January 2000 when the file has none.
    match zoo.version >= 71 {
        true => o.bytes(b.get(kept.at[0] - 8..kept.at[0]).ok_or_else(short)?),
        false => o.bytes(&upgrade::NEVER.to_le_bytes()),
    }
    // The calendar: year, month, day of week (Sunday is 0), day, hour, minute, second, millisecond.
    let d = Calendar::at(zoo.clocks.calendar).date();
    let fields = [
        d.year,
        d.month,
        (d.weekday + 1) % 7,
        d.day,
        d.hour,
        d.minute,
        d.second,
        d.millisecond,
    ];
    for n in fields {
        o.u16(n as u16);
    }
    o.u32(money.cash.to_bits());
    match zoo.version >= 73 {
        true => o.bytes(b.get(kept.at[1] + 4..kept.at[1] + 8).ok_or_else(short)?),
        false => o.u32(0),
    }
    Ok(o.0)
}

/// Whether the body of this class starts with the food counter.
fn has_food(class: &str) -> bool {
    matches!(
        class.as_bytes(),
        b"objects"
            | b"food"
            | b"paths"
            | b"fences"
            | b"tankwall"
            | b"filter"
            | b"rubble"
            | b"building"
    )
}

fn entity(o: &mut Out, v: u32, now: u64, e: &Entity) -> Result<(), String> {
    o.string(&e.class)?;
    o.string(&e.type_name)?;
    o.string(&e.subtype)?;
    // A body the reader skipped (a transient entity, or one it could not decode) is written empty.
    let Some(c) = &e.common else {
        o.u32(0);
        return Ok(());
    };
    let body = body(v, now, e, c)?;
    o.u32(body.len() as u32);
    o.bytes(&body);
    Ok(())
}

/// Whether the class has a unit block (zoo format 6.14).
fn is_unit(class: &str) -> bool {
    matches!(
        class,
        "animals" | "guests" | "staff" | "keeper" | "maint" | "tour" | "helicopter"
    )
}

/// The body in the form of version 106. An entity read from a file of version `v` has its class
/// fields converted; one made in play has them built from the model (`fresh`), as is an animal's.
fn body(v: u32, now: u64, e: &Entity, c: &Common) -> Result<Vec<u8>, String> {
    let (b, s) = (&c.raw.body, &c.raw.spans);
    let read = !b.is_empty() && s.end <= b.len();
    // An animal and a staff member are written from their records, whether they were read or made
    // in play.
    let staff = matches!(
        &e.class[..],
        "staff" | "keeper" | "maint" | "tour" | "helicopter"
    );
    let (mut pre, mut post) = match read && e.class != "animals" && !staff {
        true => super::body::transcode(v, e.class.as_bytes(), b)?,
        false => fresh::parts(e, c, now)?,
    };
    // The guest's fields come from the model, the rest of the record as read.
    if let (true, Some(g)) = (read && e.class == "guests", c.guest.as_deref()) {
        pre = fresh::guest(Some(pre), g)?;
    }
    let mut o = Out(Vec::new());
    if has_food(&e.class) {
        let keeps = e.class == "food"
            || ["poo", "dinopoo"]
                .iter()
                .any(|n| e.subtype.eq_ignore_ascii_case(n));
        o.i32(if keeps { c.food } else { 0 });
    }
    o.bytes(&pre);
    // The fields the game changes while it runs are written from the model.
    match (&e.class[..], &c.fence, &c.filter) {
        ("fences" | "tankwall", Some(f), _) => post = fresh::fence(&e.class, f),
        ("fences" | "tankwall", None, _) => return Err("no fence figures".into()),
        ("filter", _, Some(f)) if read => post[..4].copy_from_slice(&f.health.to_le_bytes()),
        _ => {}
    }
    let unit = c.unit.as_deref().cloned().unwrap_or_default();
    if is_unit(&e.class) {
        // A unit made in play is on the ground.
        let mode = if read {
            c.movement_mode
        } else {
            c.movement_mode.max(2)
        };
        unit::write(&mut o, MAX_VERSION, e.class == "guests", mode, &unit)?;
    }
    for n in [c.x, c.y, c.z] {
        o.i32(n);
    }
    o.u32(c.rotation);
    o.u32(c.id);
    o.string(&c.name)?;
    // The visible flag, the stamp, the flag after it and the behaviour set; a unit has its own set.
    o.u8(if read { b[s.stamp - 1] } else { 1 });
    o.u32(c.stamp.unwrap_or(0));
    o.u8(if read { b[s.stamp + 4] } else { 0 });
    match (read, is_unit(&e.class)) {
        (true, false) => o.bytes(&b[s.stamp + 5..s.colors]),
        (_, unit_set) => {
            let fixed = Behaviour::default();
            let set = if unit_set { &unit.behaviour } else { &fixed };
            unit::write_behaviour(&mut o, set)?;
        }
    }
    o.u32(c.colors.len() as u32);
    o.bytes(&c.colors);
    o.bytes(&post);
    Ok(o.0)
}
