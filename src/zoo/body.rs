//! The bodies of entity records, by class. Only the common part is kept; every other field is
//! read and dropped to reach the next record (or the end of the body).

use super::{AnimalRecord, Common, Fence, GuestRecord, R, Reader, bad};

/// Reads one body. `None` is a class this reader does not know.
pub(super) fn decode(r: &mut Reader, class: &[u8]) -> R<Option<Common>> {
    let v = r.v;
    Ok(Some(match class {
        // Scenery, then (after the common part) the fields of the class.
        b"objects" | b"food" | b"paths" => scenery(r)?,
        b"fences" | b"tankwall" => {
            let mut common = scenery(r)?;
            if v >= 55 {
                common.fence = Some(Fence {
                    strength: r.u32()?,
                    life: r.u32()?,
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
            r.skip(8)?;
            base(r)?
        }
        b"filter" => {
            let common = scenery(r)?;
            r.skip(4)?;
            if v >= 95 {
                r.skip(12 + 16 + 4)?;
            }
            common
        }
        b"rubble" => {
            let common = scenery(r)?;
            r.skip_list(4)?;
            r.skip(4)?;
            common
        }
        b"building" => {
            let common = scenery(r)?;
            building(r)?;
            common
        }
        b"animals" => {
            let record = animal(r)?;
            Common {
                animal: Some(Box::new(record)),
                ..zoo_unit(r, false)?
            }
        }
        b"guests" => {
            let record = guest(r)?;
            Common {
                guest: Some(Box::new(record)),
                ..zoo_unit(r, true)?
            }
        }
        b"staff" | b"keeper" | b"maint" | b"tour" | b"helicopter" => {
            staff(r, class)?;
            zoo_unit(r, false)?
        }
        _ => return Ok(None),
    }))
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
    r.skip(1 + 4)?;
    if v >= 58 {
        r.skip(1)?;
    }
    if v >= 63 {
        behaviour_set(r)?;
    }
    let mut colors = Vec::new();
    if v >= 67 {
        let n = r.count(1)?;
        colors = r.take(n)?.to_vec();
    }
    Ok(Common {
        x,
        y,
        z,
        rotation,
        id,
        name,
        colors,
        fence: None,
        movement_mode: 0,
        food: 0,
        animal: None,
        guest: None,
    })
}

/// A behaviour set holds at most one nested set, which holds one more, and so on. The leading
/// fields of each level come first and its trailing fields after the nested set, so the levels
/// are read in a loop: all the leading parts, then all the trailing parts, innermost first.
fn behaviour_set(r: &mut Reader) -> R<()> {
    let mut levels = 1;
    loop {
        r.string()?;
        r.skip(1)?;
        r.skip_strings()?;
        r.string()?;
        r.string()?; // the current call: name, arguments, three flags
        r.skip_strings()?;
        r.skip(3)?;
        if r.u8()? == 0 {
            break;
        }
        levels += 1;
    }
    for _ in 0..levels {
        r.skip_strings()?;
        r.skip(4)?;
    }
    Ok(())
}

fn building(r: &mut Reader) -> R<()> {
    let v = r.v;
    r.skip(16)?;
    r.skip_list(8)?;
    r.skip(4 + 16 + 8)?;
    match v {
        45..=47 => r.skip(2)?,
        48..=66 => r.skip_list(1)?,
        _ => {}
    }
    if v >= 68 {
        r.skip(4)?;
    }
    if v >= 74 {
        r.skip(4)?;
    }
    if v >= 75 {
        r.skip_list(4)?;
    }
    Ok(())
}

fn animal(r: &mut Reader) -> R<AnimalRecord> {
    let v = r.v;
    let (happiness, hunger, health, energy) = (r.i32()?, r.i32()?, r.i32()?, r.i32()?);
    let flags = r.take(5)?;
    let (at_minimum_health, very_unhappy) = (flags[0] != 0, flags[3] != 0);
    r.skip(5 + 20)?;
    let female = !r.string()?.starts_with(b"M");
    r.string()?;
    // Hunger, health, energy, other, keeper, captivity, habitat, bored, social, reproduction,
    // and one more.
    let mut c = [None; 16];
    for at in [0, 2, 3, 5, 6, 7, 8, 9, 10, 11] {
        c[at] = Some(r.i32()?);
    }
    r.skip(4)?;
    if v <= 53 {
        r.skip(4)?;
    }
    r.skip(4)?;
    c[12] = Some(r.i32()?);
    let dung = r.i32()?;
    let keeper_visited = r.u8()? != 0;
    r.skip(2)?;
    c[4] = Some(r.i32()?);
    r.skip(6 + 12 + 16 + 2)?;
    if v >= 41 {
        r.skip(8)?;
    }
    let mut dying = false;
    if v >= 43 {
        r.skip(12)?;
        dying = r.u8()? != 0;
    }
    let mourning = v >= 44 && r.u8()? != 0;
    if v >= 46 {
        c[14] = Some(r.i32()?);
    }
    // The escape flag counts only when it is exactly 1.
    if v >= 47 && r.u32()? == 1 {
        r.skip(8 + 8 + 12)?;
        if v >= 81 {
            r.skip(4)?;
        }
    }
    let mut crated = false;
    if v >= 52 {
        r.skip(9)?;
        crated = r.u8()? != 0;
    }
    let thirsty = v >= 54 && r.u8()? != 0;
    if v == 57 || v >= 61 {
        r.skip(1)?;
    }
    let rampaging = v >= 76 && r.u8()? != 0;
    let mut rampage = 0;
    if v >= 77 {
        r.skip(4)?;
        rampage = r.i32()?;
    }
    if v >= 78 {
        r.skip(1)?;
    }
    let mut egg = false;
    if v >= 79 {
        egg = r.u8()? != 0;
        r.skip(4)?;
    }
    if v >= 87 {
        r.skip(4)?;
        c[1] = Some(r.i32()?);
    }
    if v >= 101 {
        r.skip(8)?;
    }
    Ok(AnimalRecord {
        happiness,
        hunger,
        health,
        energy,
        dung,
        female,
        counters: c,
        at_minimum_health,
        very_unhappy,
        keeper_visited,
        mourning,
        dying,
        crated,
        egg,
        thirsty,
        rampaging,
        rampage,
    })
}

/// The guest fields: the five status values come first, the eleven check countdowns are spread
/// over the record (the chase one after the lists, the others in a block), the rest is skipped.
fn guest(r: &mut Reader) -> R<GuestRecord> {
    let v = r.v;
    let values = [r.i32()?, r.i32()?, r.i32()?, r.i32()?, r.i32()?];
    r.skip(1 + 20)?;
    if v <= 66 {
        r.skip(4)?;
    }
    r.skip(8)?;
    if v <= 64 {
        r.skip(6)?;
    }
    // Hunger, thirst, bathroom, leave, gift, energy, then trash, like, viewing, surroundings.
    let mut block = [0; 10];
    for c in &mut block {
        *c = r.i32()?;
    }
    r.skip_list(4)?;
    r.skip_list(4)?;
    if v <= 93 {
        r.string()?;
        r.string()?;
    } else {
        r.skip(4)?;
        r.string()?;
    }
    r.skip_list(8)?;
    let chase = r.i32()?;
    r.skip(6 + 2 + 12 + 2 + 8)?;
    if v >= 42 {
        r.skip(2)?;
    }
    if v >= 54 {
        r.skip(1)?;
    }
    if v >= 56 {
        r.skip(1)?;
    }
    if v >= 65 {
        r.skip(4)?;
    }
    let [a, b, c, d, e, f, g, h, i, j] = block;
    Ok(GuestRecord {
        values,
        countdowns: [a, b, c, d, e, f, chase, g, h, i, j],
    })
}

fn staff(r: &mut Reader, class: &[u8]) -> R<()> {
    let v = r.v;
    match class {
        b"keeper" => {
            r.skip(4 + 16 + 1)?;
            if (52..=74).contains(&v) {
                r.skip(4)?;
            }
        }
        b"maint" => {
            r.skip(4)?;
            if v >= 92 {
                r.skip(10)?;
                r.skip_list(1)?;
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
        _ => {}
    }
    // The common staff block.
    r.skip(8)?;
    r.skip_list(8)?;
    if v >= 75 {
        r.skip(4)?;
    }
    Ok(())
}

/// The unit block of animals, guests and staff, ending with the common part.
fn zoo_unit(r: &mut Reader, guest: bool) -> R<Common> {
    let v = r.v;
    if v <= 57 {
        r.skip(1)?;
    }
    r.skip(1 + 5)?;
    r.skip(if v <= 90 { 8 } else { 12 })?;
    r.skip(8 + 8)?;
    r.skip(if v <= 85 { 16 } else { 12 })?;
    r.skip(6)?;
    if v >= 97 && !guest {
        // Two flags, each followed by a value only when it is exactly 1.
        for _ in 0..2 {
            if r.u8()? == 1 {
                r.skip(2)?;
            }
        }
    }
    if v >= 98 {
        r.skip(2)?;
    }
    base_unit(r)
}

fn base_unit(r: &mut Reader) -> R<Common> {
    let v = r.v;
    r.skip(2 + 10)?;
    // The target block.
    r.skip(16)?;
    r.skip(if v <= 85 { 16 } else { 24 })?;
    r.skip(2)?;
    if v >= 53 {
        r.skip(8)?;
    }
    if v >= 72 {
        r.skip(8)?;
    }
    r.skip(4)?;
    r.skip(if v <= 74 { 8 } else { 12 })?;
    r.skip(4)?;
    r.string()?;
    r.skip(1)?;
    r.skip_list(if v <= 83 { 8 } else { 12 })?;
    if v >= 46 {
        r.skip(1)?;
    }
    if v >= 62 {
        r.skip(1)?;
    }
    let mut mode = 0;
    if v >= 84 {
        r.skip(4)?;
        mode = movement(r)?;
    }
    if v >= 85 {
        r.string()?;
    }
    if v >= 98 {
        r.skip(2)?;
    }
    Ok(Common {
        movement_mode: mode as u32,
        ..base(r)?
    })
}

/// Reads the movement block; returns the movement mode.
fn movement(r: &mut Reader) -> R<i32> {
    let first = r.i32()?;
    let kind = if r.v >= 93 { r.i32()? } else { first };
    match kind {
        _ if kind < 0 => {} // no movement
        1 | 2 => {}
        3 => r.skip(if r.v >= 96 { 1 + 32 } else { 1 + 12 })?,
        _ => return bad("movement of an unknown type"),
    }
    Ok(first)
}
