//! The habitat record of a zoo file: the gate and side, the statistics, and the
//! tank and show a habitat may carry, read and written (zoo format 4.1).

use super::{HabitatInfo, R, Reader, ShowInfo, Tank};

/// The smallest habitat record: two tiles, a string, 4 + 36 bytes and two dates.
pub(super) const HABITAT_MIN: usize = 76;

pub(super) type Habitat = (
    (i32, i32),
    ((i32, i32), i32),
    Option<Tank>,
    HabitatInfo,
    Option<ShowInfo>,
);

/// Reads a habitat record.
pub(super) fn habitat(r: &mut Reader) -> R<Habitat> {
    let v = r.v;
    let tile = (r.i32()?, r.i32()?);
    let name = r.text()?;
    let gate = (r.i32()?, r.i32()?);
    let side = r.i32()?;
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
    let show = has_show.then(|| ShowInfo::read(r)).transpose()?;
    if !is_tank {
        return Ok((tile, (gate, side), None, info, show));
    }
    let (height, level) = (r.u32()?, r.u32()?);
    let filled = v < 88 || r.u8()? != 0;
    let (mut salt, mut target, mut purity) = (100, 100, 100);
    if v >= 100 {
        salt = r.u32()?;
        target = r.u32()?;
        purity = r.u32()?;
    }
    let tank = Tank {
        tile,
        gate,
        height,
        level,
        salt: salt != 0,
        purity,
        filled,
        target: target != 0,
    };
    Ok((tile, (gate, side), Some(tank), info, show))
}
