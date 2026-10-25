//! The bodies of entity records, by class. Only the common part is decoded; every other field is
//! read and dropped to reach the next record (or the end of the body). The reader keeps the bytes.

use super::unit::{behaviour_set, read};
use super::upgrade::NEVER;
use super::{BuildingRecord, Common, Fence, Filter, GuestRecord, R, Reader, Spans};

/// Reads one body. `None` is a class this reader does not know.
pub(super) fn decode(r: &mut Reader, class: &[u8]) -> R<Option<Common>> {
    let v = r.v;
    let common = match class {
        // Scenery, then (after the common part) the fields of the class.
        b"objects" | b"food" | b"paths" => scenery(r)?,
        b"fences" | b"tankwall" => {
            let mut common = scenery(r)?;
            if v >= 55 {
                common.fence = Some(Fence {
                    strength: r.u32()?,
                    life: r.u32()? as i32,
                    timers: [r.u32()?, r.u32()?],
                    portal: false,
                });
            }
            if class == b"tankwall" && v >= 101 {
                let portal = r.u8()? != 0;
                r.skip(1)?;
                if let Some(fence) = &mut common.fence {
                    fence.portal = portal;
                }
            }
            common
        }
        b"ambient" => {
            r.copy_on();
            r.skip(8)?;
            r.cut();
            base(r)?
        }
        b"filter" => {
            let mut common = scenery(r)?;
            r.copy_on();
            let health = r.u32()?;
            // From 95 the upkeep of last month, this month and since built, two dates and one word
            // follow; the timers are not saved (zoo format 6.8).
            if v >= 95 {
                r.skip(3 * 4 + 2 * 8 + 4)?;
            } else {
                r.put(&[0; 12]);
                r.put(&NEVER.to_le_bytes());
                r.put(&NEVER.to_le_bytes());
                r.put(&1u32.to_le_bytes());
            }
            common.filter = Some(Filter {
                health,
                timers: None,
            });
            common
        }
        b"rubble" => {
            let common = scenery(r)?;
            r.copy_on();
            r.skip_list(4)?;
            r.skip(4)?;
            common
        }
        b"building" => {
            let common = scenery(r)?;
            r.copy_on();
            Common {
                building: Some(building(r)?),
                ..common
            }
        }
        b"animals" => {
            let record = super::animal::read(r)?;
            Common {
                animal: Some(Box::new(record)),
                ..zoo_unit(r, false)?
            }
        }
        b"guests" => {
            r.copy_on();
            let record = guest(r)?;
            Common {
                guest: Some(Box::new(record)),
                ..zoo_unit(r, true)?
            }
        }
        b"staff" | b"keeper" | b"maint" | b"tour" | b"helicopter" => {
            r.copy_on();
            staff(r, class)?;
            zoo_unit(r, false)?
        }
        _ => return Ok(None),
    };
    if r.copy {
        r.post = std::mem::take(&mut r.tee);
        r.copy = false;
    }
    Ok(Some(common))
}

/// The food counter, then the common part.
fn scenery(r: &mut Reader) -> R<Common> {
    let food = r.i32()?;
    Ok(Common { food, ..base(r)? })
}

/// The common part of every body.
fn base(r: &mut Reader) -> R<Common> {
    let v = r.v;
    let (x, y, z) = (r.i32()?, r.i32()?, r.i32()?);
    let rotation = r.u32()? % 8;
    let id = r.u32()?;
    let name = r.text()?;
    if v <= 62 {
        r.string()?;
    }
    r.skip(1)?;
    let stamp_at = r.pos();
    let stamp = Some(r.u32()?).filter(|&s| s != 0);
    if v >= 58 {
        r.skip(1)?;
    }
    r.behaviour = None;
    if v >= 63 {
        r.behaviour = Some(behaviour_set(r)?);
    }
    let mut colors = Vec::new();
    let colors_at = r.pos();
    if v >= 67 {
        let n = r.count(1)?;
        colors = r.take(n)?.to_vec();
    }
    r.spans = Spans {
        stamp: stamp_at,
        colors: colors_at,
        end: r.pos(),
    };
    Ok(Common {
        x,
        y,
        z,
        rotation,
        id,
        name,
        colors,
        stamp,
        fence: None,
        ..Default::default()
    })
}

/// The figures, the price and the users of a guest building (zoo format 6.9, guest buildings 4);
/// the fields the engine has no use for are read past.
fn building(r: &mut Reader) -> R<BuildingRecord> {
    let v = r.v;
    let stored = r.u32()?;
    let guests_since = r.u32()?;
    let income_since = r.u32()?;
    let price = r.u32()?;
    let mut users = Vec::new();
    for _ in 0..r.count(8)? {
        users.push((r.u32()?, r.u32()?));
    }
    // The count is not brought back to the list (guest buildings 4.2).
    let users_now = stored.max(users.len() as u32);
    let fill = r.u32()?;
    let income_last = r.u32()?;
    let income_this = r.u32()?;
    let guests_last = r.u32()?;
    let guests_this = r.u32()?;
    let built = r.u64()?;
    match v {
        45..=47 => r.skip(2)?,
        48..=66 => r.skip_list(1)?,
        _ => {}
    }
    let mut upkeep = None;
    if v >= 68 {
        upkeep = Some(r.u32()?);
    }
    if v >= 74 {
        r.skip(4)?;
    } else {
        r.put(&[0; 4]);
    }
    if v >= 75 {
        r.skip_list(4)?;
    } else {
        r.put(&[0; 4]);
    }
    Ok(BuildingRecord {
        guests: [guests_this, guests_last, guests_since],
        income: [income_this, income_last, income_since],
        upkeep,
        price,
        users,
        users_now,
        fill,
        built,
    })
}

/// The guest fields: the five status values come first, the eleven check countdowns are spread
/// over the record (the chase one after the lists, the others in a block), the rest is skipped.
fn guest(r: &mut Reader) -> R<GuestRecord> {
    let v = r.v;
    let values = [r.i32()?, r.i32()?, r.i32()?, r.i32()?, r.i32()?];
    // The sick flag (the byte at 0x336 of the guest) and the five increment factors.
    let sick = r.u8()? != 0;
    r.skip(20)?;
    if v <= 66 {
        r.skip(4)?;
    }
    let (female, child) = (r.u32()? != 0, r.u32()? != 0);
    if v <= 64 {
        r.skip(6)?;
    }
    // Hunger, thirst, bathroom, leave, gift, energy, then trash, like, viewing, surroundings.
    let mut block = [0; 10];
    for c in &mut block {
        *c = r.i32()?;
    }
    // The item slots (at most five kept), then the list of seen things.
    let mut items = [0; 5];
    for i in 0..r.count(4)? {
        let item = r.u32()?;
        if let Some(slot) = items.get_mut(i) {
            *slot = item;
        }
    }
    r.skip_list(4)?;
    if v <= 93 {
        r.quiet(|r| {
            r.string()?;
            r.string().map(|_| ())
        })?;
        r.put(&[0; 8]);
    } else {
        r.skip(4)?;
        r.string()?;
    }
    r.skip_list(8)?;
    let chase = r.i32()?;
    // The leave and afraid flags, the saw-animal countdown, an unknown flag and the wants-a-gift
    // flag, the arrival date, the favourite species, the wandered and eat-standing flags, a date,
    // an unknown word, and the in-group, listening and guide fields of the versions that have them.
    let (leave, afraid) = (r.u8()? != 0, r.u8()? != 0);
    let saw_animal = r.i32()?;
    let gift = r.take(2)?[1] != 0;
    let arrived = u64::from(r.u32()?) | u64::from(r.u32()?) << 32;
    let favourite = r.i32()?;
    let (wandered, stand) = (r.u8()? != 0, r.u8()? != 0);
    r.skip(8)?;
    if v >= 42 {
        r.skip(2)?;
    }
    let in_group = v >= 54 && r.u8()? != 0;
    let listening = v >= 56 && r.u8()? != 0;
    let guide = if v >= 65 { r.u32()? } else { 0 };
    let [a, b, c, d, e, f, g, h, i, j] = block;
    Ok(GuestRecord {
        values,
        countdowns: [a, b, c, d, e, f, chase, g, h, i, j],
        arrived,
        favourite,
        sick,
        saw_animal,
        female,
        child,
        items,
        leave,
        afraid,
        gift,
        wandered,
        stand,
        in_group,
        listening,
        guide,
    })
}

fn staff(r: &mut Reader, class: &[u8]) -> R<()> {
    let v = r.v;
    match class {
        b"keeper" => {
            r.skip(4 + 16 + 1)?;
            if (52..=74).contains(&v) {
                r.quiet(|r| r.skip(4))?;
            }
        }
        b"maint" => {
            r.skip(4)?;
            if v >= 92 {
                r.skip(10)?;
                r.skip_list(1)?;
            } else {
                r.put(&[0; 14]);
            }
        }
        b"tour" => {
            r.skip(4)?;
            if v >= 54 {
                r.skip(4)?;
            }
            if (54..=64).contains(&v) {
                r.skip(4)?;
            }
            if v >= 65 {
                r.skip_list(4)?;
            }
            if v >= 66 {
                r.skip(1)?;
            }
        }
        b"helicopter" if v >= 75 => r.skip(12)?,
        b"helicopter" => r.put(&[0; 12]),
        _ => {}
    }
    // The common staff block.
    r.skip(8)?;
    r.skip_list(8)?;
    if v >= 75 {
        r.skip(4)?;
    } else {
        r.put(&[0; 4]);
    }
    Ok(())
}

/// The unit block of animals, guests and staff, ending with the common part.
fn zoo_unit(r: &mut Reader, guest: bool) -> R<Common> {
    r.cut();
    let (mut unit, mode) = read(r, guest)?;
    let common = base(r)?;
    if let Some(set) = r.behaviour.take() {
        unit.behaviour = set;
    }
    Ok(Common {
        movement_mode: mode,
        unit: Some(Box::new(unit)),
        ..common
    })
}

/// The class fields of a body read in version `v`, in the form of version 106: the ones before the
/// unit block or the entity base, and the ones after them. The food counter, the fence figures,
/// the unit block and the entity base are not among them.
pub(super) fn transcode(v: u32, class: &[u8], body: &[u8]) -> Result<(Vec<u8>, Vec<u8>), String> {
    let mut r = Reader::new(body, v);
    r.want = true;
    match decode(&mut r, class) {
        Ok(Some(_)) if r.data.is_empty() => Ok((r.pre, r.post)),
        _ => Err("the body cannot be read again".into()),
    }
}
