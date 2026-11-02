//! The sections kept as read, written in the form of version 106 (save games 6 and 11.2): the
//! fields a version lacks take the value the original writes for them.

use super::write::Out;
use super::{Bad, HABITAT_MIN, R, Reader, ShowInfo, Shows, bad};

/// The date the original puts in a date field that has none: 1 January 2000.
pub(crate) const NEVER: u64 = 125_911_584_000_000_000;

/// The habitat records of a file of version `v`. A habitat that holds a show is refused below 106,
/// where the show's record differs.
pub(super) fn habitats(v: u32, kept: &[u8]) -> Result<Vec<u8>, String> {
    let mut r = Reader::new(kept, v);
    (r.want, r.copy) = (true, true);
    habitat_records(&mut r).map_err(|Bad(why)| format!("habitats: {why}"))?;
    Ok(r.tee)
}

fn habitat_records(r: &mut Reader) -> R<()> {
    let v = r.v;
    for _ in 0..r.count(HABITAT_MIN)? {
        // The tile, the name, the gate, its side, the statistics and the two dates.
        r.skip(8)?;
        r.string()?;
        r.skip(8 + 4 + 36 + 16)?;
        match v >= 83 {
            true => r.skip(4)?,
            false => r.put(&[0; 4]),
        }
        let tank = match v >= 85 {
            true => r.u8()? != 0,
            false => {
                r.put(&[0]);
                false
            }
        };
        if v < 90 {
            r.put(&[0]);
        } else if r.u8()? != 0 {
            if v < 106 {
                return bad("a show cannot be written yet");
            }
            ShowInfo::read(r)?;
        }
        if tank {
            r.skip(8)?;
            match v >= 88 {
                true => r.skip(1)?,
                false => r.put(&[0]),
            }
            match v >= 100 {
                true => r.skip(12)?,
                false => (0..3).for_each(|_| r.put(&100u32.to_le_bytes())),
            }
        }
    }
    r.skip(4)
}

/// The shows that precede the id counter: the scripts and the two counters, which a fresh map holds
/// when the file has none. Scripts are refused below 103, where their items differ.
pub(super) fn shows(v: u32, shows: &Shows) -> Result<Vec<u8>, String> {
    if !shows.scripts.is_empty() && v < 103 {
        return Err(format!(
            "shows with scripts of version {v} cannot be written yet"
        ));
    }
    let mut o = Out(Vec::new());
    shows.write_section(&mut o)?;
    Ok(o.0)
}
