//! The staff record (zoo format 6.13): read whole into [`StaffRecord`] and written from it, so a
//! saved staff member holds the state the engine models and keeps what it does not. A field a
//! version lacks takes the value the original writes for it (save games 11.2); a new member's
//! record is [`StaffRecord::default`] (save games 11.3).

use super::write::Out;
use super::{R, Reader};

/// What the record of a keeper, maintenance worker, tour guide, helicopter or other staff member
/// stores: the common staff block and the own fields of the class (zoo format 6.13). The layout of a
/// tour guide in version 106 was read from the writer, not seen in a file (save games 11.1).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct StaffRecord {
    /// The work and chase counters (staff 4.2).
    pub work: i32,
    pub chase: i32,
    /// The tiles inside the gates of the assigned exhibits, oldest first (staff 6).
    pub tiles: Vec<[i32; 2]>,
    /// A guide's crowd countdown, from version 54; `None` before: it keeps the draw.
    pub crowd: Option<i32>,
    /// A guide's group and its leg flag.
    pub members: Vec<u32>,
    pub leg: bool,
    /// One flag per duty kind of a maintenance worker, true for switched on; the switches on are
    /// counted from them when the record is written.
    pub duties: Vec<bool>,
    /// The captured animal, from version 75.
    pub captured: u32,
    /// The own fields before the modelled ones, in the form of version 106; empty or too short, the
    /// rest takes the value of a new member.
    pub own: Vec<u8>,
}

/// The duty kinds a worker's record holds when the file has none.
const KINDS: usize = 4;

/// The own fields of a new member: a worker's two words are 2 and 2 (its u16, the number of
/// switches on, is written from the flags).
fn own_new(class: &[u8]) -> Vec<u8> {
    match class {
        b"keeper" => vec![0; 21],
        b"maint" => [&[0; 4][..], &[2, 0, 0, 0, 2, 0, 0, 0]].concat(),
        b"tour" => vec![0; 4],
        b"helicopter" => vec![0; 12],
        _ => Vec::new(),
    }
}

/// Reads the record of version `r.v`.
pub(super) fn read(r: &mut Reader, class: &[u8]) -> R<StaffRecord> {
    let v = r.v;
    let mut s = StaffRecord {
        duties: vec![true; KINDS],
        own: own_new(class),
        ..StaffRecord::default()
    };
    match class {
        b"keeper" => {
            s.own = r.take(21)?.to_vec();
            if (52..=74).contains(&v) {
                r.skip(4)?;
            }
        }
        b"maint" => {
            s.own = r.take(4)?.to_vec();
            if v >= 92 {
                s.own.extend(r.take(8)?);
                r.skip(2)?;
                let n = r.count(1)?;
                s.duties = r.take(n)?.iter().map(|&b| b != 0).collect();
            } else {
                s.own.extend(&own_new(class)[4..]);
            }
        }
        b"tour" => {
            s.own = r.take(4)?.to_vec();
            if v >= 54 {
                s.crowd = Some(r.i32()?);
            }
            if (54..=64).contains(&v) {
                r.skip(4)?;
            }
            if v >= 65 {
                for _ in 0..r.count(4)? {
                    s.members.push(r.u32()?);
                }
            }
            s.leg = v >= 66 && r.u8()? != 0;
        }
        b"helicopter" if v >= 75 => s.own = r.take(12)?.to_vec(),
        _ => {}
    }
    s.work = r.i32()?;
    s.chase = r.i32()?;
    for _ in 0..r.count(8)? {
        s.tiles.push([r.i32()?, r.i32()?]);
    }
    if v >= 75 {
        s.captured = r.u32()?;
    }
    Ok(s)
}

/// Writes the record in the form of version 106.
pub(super) fn write(o: &mut Out, class: &[u8], s: &StaffRecord) {
    let new = own_new(class);
    // The own fields, then for a worker, a guide the fields the engine models.
    let own = |from: usize, to: usize, o: &mut Out| {
        o.bytes(s.own.get(from..to).unwrap_or(&new[from..to]));
    };
    match class {
        b"keeper" => own(0, 21, o),
        b"maint" => {
            own(0, 12, o);
            o.u16(s.duties.iter().filter(|&&d| d).count() as u16);
            o.u32(s.duties.len() as u32);
            s.duties.iter().for_each(|&d| o.u8(d.into()));
        }
        b"tour" => {
            own(0, 4, o);
            o.i32(s.crowd.unwrap_or(0));
            o.u32(s.members.len() as u32);
            s.members.iter().for_each(|&m| o.u32(m));
            o.u8(s.leg.into());
        }
        b"helicopter" => own(0, 12, o),
        _ => {}
    }
    o.i32(s.work);
    o.i32(s.chase);
    o.u32(s.tiles.len() as u32);
    s.tiles.iter().flatten().for_each(|&n| o.i32(n));
    o.u32(s.captured);
}
