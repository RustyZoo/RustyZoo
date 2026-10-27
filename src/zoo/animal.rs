//! The animal record (zoo format 6.11): read whole into [`AnimalRecord`] and written from it, so a
//! saved animal holds the state the engine models and keeps what it does not. A field a version
//! lacks takes the value the original writes for it (save games 11.2); a new animal's record is
//! [`animal_new`] (save games 11.3).

use super::write::Out;
use super::{R, Reader};

/// The values, counters and flags an animal record stores (animals 3.5). A counter the record
/// does not hold (by the version) is `None`: it keeps what creation gave it.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct AnimalRecord {
    pub happiness: i32,
    pub hunger: i32,
    pub health: i32,
    pub energy: i32,
    pub dung: i32,
    pub female: bool,
    /// The second string: one line feed for a new animal, the file's text for a read one.
    pub second: String,
    /// The counters of the checks in the order of the animal module's indices.
    pub counters: [Option<i32>; 16],
    /// The breath, as saved (tank fauna 10.1): only from version 87, `None` before.
    pub breath: Option<i32>,
    pub at_minimum_health: bool,
    pub sleeping: bool,
    pub eating: bool,
    pub very_unhappy: bool,
    pub keeper_visited: bool,
    pub mourning: bool,
    pub dying: bool,
    pub crated: bool,
    pub egg: bool,
    pub thirsty: bool,
    pub drinking: bool,
    /// The rampage flag and counter (animals 11).
    pub rampaging: bool,
    pub rampage: i32,
    /// The counters the record stores that are not drawn (save games 11.3): life, grow-up, birth
    /// interval, mate delay and crate. A version that lacks one reads it as 0, and the life and
    /// the mate delay as `None` (the animal keeps its creation values).
    pub life: Option<i32>,
    pub baby: i32,
    pub interval: i32,
    pub mate_delay: Option<i32>,
    pub boxed: i32,
    /// The two dates (zoo format 6.11), as file times.
    pub dates: [u64; 2],
    /// Where the animal escaped from, when the escape flag is 1.
    pub escape: Option<Escape>,
    /// The home exhibit's gate tile, (-1, -1) for none.
    pub home: [i32; 2],
    /// The fields the engine does not model, in file order and in the form of version 106 (94
    /// bytes): a segment the vector is too short for takes the value of a new animal.
    pub spare: Vec<u8>,
}

/// The escape record of an animal that escaped (zoo format 6.11).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Escape {
    pub tiles: [[i32; 2]; 2],
    pub words: [u32; 3],
    /// The word of version 81, 0 before.
    pub last: u32,
}

/// A new animal (save games 11.3): the type's initial happiness and full health, not hungry, no
/// energy, no counter running, the one line feed, both dates `now`, no home exhibit. The caller
/// gives the counters that are not drawn their creation values.
pub(crate) fn animal_new(happiness: i32, health: i32, now: u64) -> AnimalRecord {
    AnimalRecord {
        happiness,
        health,
        second: "\n".into(),
        dates: [now; 2],
        home: [-1; 2],
        ..AnimalRecord::default()
    }
}

/// Reads the record of version `r.v`.
pub(super) fn read(r: &mut Reader) -> R<AnimalRecord> {
    let v = r.v;
    let mut a = animal_new(r.i32()?, 0, 0);
    let mut spare = Vec::new();
    (a.hunger, a.health, a.energy) = (r.i32()?, r.i32()?, r.i32()?);
    // Health at its minimum, sleeping, eating, very unhappy and one more.
    let flags = r.take(5)?;
    (a.at_minimum_health, a.sleeping, a.eating, a.very_unhappy) =
        (flags[0] != 0, flags[1] != 0, flags[2] != 0, flags[3] != 0);
    spare.push(flags[4]);
    // The keeper-visit level and a byte, then the five rate multipliers.
    spare.extend(r.take(5 + 20)?);
    a.female = !r.string()?.starts_with(b"M");
    a.second = r.text()?;
    // Hunger, health, energy, other, keeper, captivity, habitat, bored, social, reproduction,
    // and one more.
    for at in [0, 2, 3, 5, 6, 7, 8, 9, 10, 11] {
        a.counters[at] = Some(r.i32()?);
    }
    spare.extend(r.take(4)?);
    if v <= 53 {
        r.skip(4)?;
    }
    spare.extend(r.take(4)?);
    a.counters[12] = Some(r.i32()?);
    a.dung = r.i32()?;
    a.keeper_visited = r.u8()? != 0;
    spare.extend(r.take(2)?);
    a.counters[4] = Some(r.i32()?);
    spare.extend(r.take(6)?);
    (a.baby, a.interval) = (r.i32()?, r.i32()?);
    spare.extend(r.take(4)?);
    a.dates = [r.u64()?, r.u64()?];
    spare.extend(r.take(2)?);
    // Two words (from 41), three words (from 43) and the dying flag.
    match v >= 41 {
        true => spare.extend(r.take(8)?),
        false => spare.extend([0; 8]),
    }
    match v >= 43 {
        true => {
            (a.mate_delay, a.life) = (Some(r.i32()?), Some(r.i32()?));
            spare.extend(r.take(4)?);
            a.dying = r.u8()? != 0;
        }
        false => spare.extend([0; 4]),
    }
    a.mourning = v >= 44 && r.u8()? != 0;
    if v >= 46 {
        a.counters[14] = Some(r.i32()?);
    }
    // The escape flag counts only when it is exactly 1.
    if v >= 47 && r.u32()? == 1 {
        let tile = |r: &mut Reader| -> R<[i32; 2]> { Ok([r.i32()?, r.i32()?]) };
        let tiles = [tile(r)?, tile(r)?];
        let words = [r.u32()?, r.u32()?, r.u32()?];
        let last = if v >= 81 { r.u32()? } else { 0 };
        a.escape = Some(Escape { tiles, words, last });
    }
    // The crate counter, a word and a byte, then the crated flag.
    match v >= 52 {
        true => {
            a.boxed = r.i32()?;
            spare.extend(r.take(5)?);
            a.crated = r.u8()? != 0;
        }
        false => spare.extend([0; 5]),
    }
    a.thirsty = v >= 54 && r.u8()? != 0;
    a.drinking = (v == 57 || v >= 61) && r.u8()? != 0;
    a.rampaging = v >= 76 && r.u8()? != 0;
    match v >= 77 {
        true => {
            spare.extend(r.take(4)?);
            a.rampage = r.i32()?;
        }
        false => spare.extend([0; 4]),
    }
    match v >= 78 {
        true => spare.extend(r.take(1)?),
        false => spare.push(0),
    }
    match v >= 79 {
        true => {
            a.egg = r.u8()? != 0;
            spare.extend(r.take(4)?);
        }
        false => spare.extend([0; 4]),
    }
    if v >= 87 {
        a.breath = Some(r.i32()?);
        a.counters[1] = Some(r.i32()?);
    }
    // No home before 101: the reader works it out again.
    if v >= 101 {
        a.home = [r.i32()?, r.i32()?];
    }
    a.spare = spare;
    Ok(a)
}

/// The next segment of the fields the engine does not model.
struct Spare<'a>(&'a [u8], usize);

impl Spare<'_> {
    /// Writes the next `default.len()` bytes, or `default` when the record holds no more.
    fn put(&mut self, o: &mut Out, default: &[u8]) {
        let end = self.1 + default.len();
        o.bytes(self.0.get(self.1..end).unwrap_or(default));
        self.1 = end;
    }
}

/// Writes the record in the form of version 106.
pub(super) fn write(o: &mut Out, a: &AnimalRecord) -> Result<(), String> {
    let mut s = Spare(&a.spare, 0);
    let c = |i: usize| a.counters[i].unwrap_or(0);
    for n in [a.happiness, a.hunger, a.health, a.energy] {
        o.i32(n);
    }
    let flag = |b: bool| u8::from(b);
    o.bytes(&[
        flag(a.at_minimum_health),
        flag(a.sleeping),
        flag(a.eating),
        flag(a.very_unhappy),
    ]);
    s.put(o, &[0]);
    // The keeper-visit level 1, a byte and the five rate multipliers 1.
    let mut new = [0; 25];
    for at in [0, 5, 9, 13, 17, 21] {
        new[at] = 1;
    }
    s.put(o, &new);
    o.string(if a.female { "Female" } else { "Male" })?;
    o.string(&a.second)?;
    for i in [0, 2, 3, 5, 6, 7, 8, 9, 10, 11] {
        o.i32(c(i));
    }
    s.put(o, &[0; 4]);
    s.put(o, &[0; 4]);
    o.i32(c(12));
    o.i32(a.dung);
    o.u8(flag(a.keeper_visited));
    s.put(o, &[0; 2]);
    o.i32(c(4));
    s.put(o, &[0; 6]);
    o.i32(a.baby);
    o.i32(a.interval);
    s.put(o, &[0; 4]);
    for d in a.dates {
        o.bytes(&d.to_le_bytes());
    }
    s.put(o, &[0; 2 + 8]);
    o.i32(a.mate_delay.unwrap_or(0));
    o.i32(a.life.unwrap_or(0));
    s.put(o, &[0; 4]);
    o.bytes(&[flag(a.dying), flag(a.mourning)]);
    o.i32(c(14));
    match &a.escape {
        Some(e) => {
            o.u32(1);
            e.tiles.iter().flatten().for_each(|&n| o.i32(n));
            e.words.iter().for_each(|&n| o.u32(n));
            o.u32(e.last);
        }
        None => o.u32(0),
    }
    // The crate counter, a word and the tranquilised flag; then the crated one.
    o.i32(a.boxed);
    s.put(o, &[0; 5]);
    o.bytes(&[flag(a.crated), flag(a.thirsty), flag(a.drinking)]);
    o.u8(flag(a.rampaging));
    s.put(o, &[0; 4]);
    o.i32(a.rampage);
    s.put(o, &[0]);
    o.u8(flag(a.egg));
    s.put(o, &[0; 4]);
    o.i32(a.breath.unwrap_or(0));
    o.i32(c(1));
    a.home.iter().for_each(|&n| o.i32(n));
    Ok(())
}
