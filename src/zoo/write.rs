//! The writer: the mirror of the reader (save games 6, 7). Every section is written in the
//! order of the format, from the model where the engine has one and from the bytes kept as read
//! elsewhere. The version written is the version read (the upgrade to 106 is not done yet).

use super::unit;
use super::{Common, Entity, MAX_VERSION, MIN_VERSION, Zoo};
use crate::money::{CATEGORIES, MONTHS, Money, YEARS};
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
    /// The file of this zoo, with the cash and accounts of `money`. Fails for an entity the writer
    /// has no body for (one made by the game, not read from a file) and for a zoo that was not read
    /// from a file.
    pub fn write(&self, money: &Money) -> Result<Vec<u8>, String> {
        let v = self.version;
        if !(MIN_VERSION..=MAX_VERSION).contains(&v) {
            return Err(format!("version {v} cannot be written"));
        }
        let kept = &self.kept;
        if kept.habitats.is_empty() || kept.finances.is_empty() {
            return Err("the zoo was not read from a file".into());
        }
        let mut o = Out(b"TZFB".to_vec());
        o.u32(v);
        if v >= 69 {
            o.u32(self.locale);
        }
        if v >= 82 {
            o.u32(self.expansion);
        }
        o.u32(self.width);
        o.u32(self.height);
        o.u32(self.entrance.0);
        o.u32(self.entrance.1);
        o.bytes(&habitats(self));
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
        for (i, e) in self.entities.iter().enumerate() {
            entity(&mut o, v, e).map_err(|why| format!("entity {i}: {why}"))?;
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
        if v >= 97 {
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
        }
        o.bytes(&kept.research);
        if v >= 58 {
            o.u32(self.clocks.world_rem);
            o.u32(self.clocks.world_secs);
            o.bytes(&kept.tail);
        }
        Ok(o.0)
    }
}

/// The habitat section as read, with the statistics and the last service of each habitat written
/// over it.
fn habitats(zoo: &Zoo) -> Vec<u8> {
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
    b
}

/// The game block as read, with the modelled values written over it.
fn finances(zoo: &Zoo, money: &Money) -> Result<Vec<u8>, String> {
    let (kept, status) = (&zoo.kept, &zoo.clocks.status);
    let mut b = kept.finances.clone();
    let word = |b: &[u8], at: usize| {
        b.get(at..at + 4)
            .map(|w| u32::from_le_bytes(w.try_into().unwrap()))
    };
    let short = || "the game block is short".to_string();
    let (rows, columns) = (
        word(&b, 45).ok_or_else(short)?,
        word(&b, 49).ok_or_else(short)?,
    );
    let (rows, columns) = (rows as usize, columns as usize);
    let mut put = |at: usize, bytes: &[u8]| {
        let to = b.get_mut(at..at + bytes.len()).ok_or_else(short)?;
        to.copy_from_slice(bytes);
        Ok::<(), String>(())
    };
    for (i, t) in status.timers.iter().enumerate() {
        put(4 + 4 * i, &t.to_le_bytes())?;
    }
    put(17, &status.rating.to_le_bytes())?;
    put(33, &status.given.to_le_bytes())?;
    put(37, &money.month.to_le_bytes())?;
    put(41, &money.year.to_le_bytes())?;
    let mut at = kept.at[0];
    for m in 0..rows {
        for column in 0..MONTHS {
            if m < CATEGORIES {
                put(at, &money.monthly[m][column].to_le_bytes())?;
            }
            at += 4;
        }
    }
    for y in 0..rows {
        for column in 0..columns {
            if y < CATEGORIES && column < YEARS {
                put(at, &money.yearly[y][column].to_le_bytes())?;
            }
            at += 4;
        }
    }
    for c in 0..rows.min(CATEGORIES) {
        put(at + 4 * c, &money.total[c].to_le_bytes())?;
    }
    if zoo.version >= 39 {
        put(kept.at[1], &status.price.to_le_bytes())?;
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
    for (i, n) in fields.iter().enumerate() {
        put(kept.at[2] + 2 * i, &(*n as u16).to_le_bytes())?;
    }
    put(kept.at[3], &money.cash.to_le_bytes())?;
    Ok(b)
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

fn entity(o: &mut Out, v: u32, e: &Entity) -> Result<(), String> {
    o.string(&e.class)?;
    o.string(&e.type_name)?;
    o.string(&e.subtype)?;
    // A body the reader skipped (a transient entity, or one it could not decode) is written empty.
    let Some(c) = &e.common else {
        if v < 70 {
            return Err("no body in a file without body sizes".into());
        }
        o.u32(0);
        return Ok(());
    };
    let body = body(v, e, c)?;
    if v >= 70 {
        o.u32(body.len() as u32);
    }
    o.bytes(&body);
    Ok(())
}

/// The body: the one read, with the entity base written from the fields of `c`.
fn body(v: u32, e: &Entity, c: &Common) -> Result<Vec<u8>, String> {
    let (b, s) = (&c.raw.body, &c.raw.spans);
    if b.is_empty() || s.end > b.len() {
        return Err(format!(
            "{}/{}: no body to write for an entity that was not read from a file",
            e.class, e.type_name
        ));
    }
    // The unit block replaces the bytes read, up to the entity base.
    let mut o = Out(b[..c.unit.as_ref().map_or(s.base, |_| s.unit)].to_vec());
    if let Some(u) = &c.unit {
        unit::write(&mut o, v, e.class == "guests", c.movement_mode, u)?;
    }
    if has_food(&e.class) && s.base >= 4 {
        let keeps = e.class == "food"
            || ["poo", "dinopoo"]
                .iter()
                .any(|n| e.subtype.eq_ignore_ascii_case(n));
        if keeps {
            o.0[..4].copy_from_slice(&c.food.to_le_bytes());
        }
    }
    for n in [c.x, c.y, c.z] {
        o.i32(n);
    }
    o.u32(c.rotation);
    o.u32(c.id);
    o.string(&c.name)?;
    o.bytes(&b[s.name..s.stamp]);
    o.u32(c.stamp.unwrap_or(0));
    match &c.unit {
        // The visible flag of version 58, then the behaviour set of the unit.
        Some(u) if v >= 63 => {
            o.bytes(&b[s.stamp + 4..s.stamp + 5]);
            unit::write_behaviour(&mut o, &u.behaviour)?;
        }
        _ => o.bytes(&b[s.stamp + 4..s.colors]),
    }
    if v >= 67 {
        o.u32(c.colors.len() as u32);
        o.bytes(&c.colors);
    }
    o.bytes(&b[s.end..]);
    Ok(o.0)
}
