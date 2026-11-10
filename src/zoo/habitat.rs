//! The habitat record of a zoo file: the gate and side, the statistics, and the
//! tank and show a habitat may carry.

use super::{HabitatInfo, R, Reader, ShowInfo, Tank, cp1252};

/// The smallest habitat record: two tiles, a string, 4 + 36 bytes and two dates.
pub(super) const HABITAT_MIN: usize = 76;

pub(super) type Habitat = (
    (i32, i32),
    ((i32, i32), i32),
    Option<Tank>,
    HabitatInfo,
    usize,
    (usize, usize),
    (Option<ShowInfo>, Option<(usize, usize)>),
);

/// Reads a habitat record; `size` is the length of the bytes the section started with, and the last
/// values are where the record's statistics and its name start and end in them.
pub(super) fn habitat(r: &mut Reader, size: usize) -> R<Habitat> {
    let v = r.v;
    let tile = (r.i32()?, r.i32()?);
    let name_at = size - r.data.len();
    let name = cp1252(r.string()?);
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
    let show_from = size - r.data.len();
    let has_show = v >= 90 && r.u8()? != 0;
    let show = has_show.then(|| ShowInfo::read(r)).transpose()?;
    let show_at = (v >= 90).then_some((show_from, size - r.data.len()));
    let show = (show, show_at);
    if !is_tank {
        return Ok((tile, (gate, side), None, info, at, (name_at, name_to), show));
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
        show,
    ))
}
