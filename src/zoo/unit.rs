//! The unit blocks of animals, guests and staff: the goal and target, the movement, the pending
//! behaviour call and the behaviour set (zoo format 6.10 and 6.14). Fields are named after their
//! offset in the original. A field a version lacks takes the value the original writes for it
//! (save games 11.2), and positions are held as world positions, the form of version 106.

use super::write::Out;
use super::{R, Reader, bad};

/// A behaviour set: the outermost level first, each level nested in the one before.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Behaviour {
    pub levels: Vec<Level>,
}

/// One level of a behaviour set.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Level {
    /// The current behaviour (`bWalk` ...).
    pub name: String,
    pub u58: u8,
    pub list_a: Vec<String>,
    pub u34: String,
    /// The current call: name, arguments, three flags.
    pub call: String,
    pub args: Vec<String>,
    pub flags: [u8; 3],
    /// Not 0 when the next level is nested in this one.
    pub child: u8,
    pub list_b: Vec<String>,
    pub tail: [u8; 4],
}

impl Default for Level {
    fn default() -> Level {
        Level {
            name: String::new(),
            u58: 1,
            list_a: Vec::new(),
            u34: String::new(),
            call: String::new(),
            args: Vec::new(),
            flags: [0; 3],
            child: 0,
            list_b: Vec::new(),
            tail: [0; 4],
        }
    }
}

impl Default for Behaviour {
    fn default() -> Behaviour {
        Behaviour {
            levels: vec![Level::default()],
        }
    }
}

/// The goal and target block.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Target {
    /// The goal and sub-goal type wanted.
    pub goal: i32,
    pub sub_goal: i32,
    pub words: [u32; 2],
    pub target: [i32; 3],
    pub sub_target: [i32; 3],
    pub bytes: [u8; 2],
    pub extra: [u32; 2],
    /// The type number of the active goal and sub-goal, -1 for none.
    pub active: i32,
    pub active_sub: i32,
}

/// What the zoo unit block and the base unit block store, apart from the movement mode, which is
/// `Common::movement_mode`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnitBlock {
    /// Until version 57.
    pub old_flag: u8,
    pub u24c: u8,
    pub u22c: u32,
    pub u25c: u8,
    pub u238: [i32; 3],
    /// A tile, (-1, -1) for none.
    pub u244: [i32; 2],
    pub u214: u32,
    pub u218: u32,
    pub u220: [i32; 3],
    pub u24d: u8,
    pub u24e: u8,
    pub u230: u32,
    /// The two optional flags of version 97 (not for guests) and the value that follows a flag of 1.
    pub flags: [(u8, u16); 2],
    pub u24f: u8,
    pub u250: u8,
    pub u20d: u8,
    pub u20c: u8,
    pub u1f8: u32,
    pub u20e: u8,
    pub u20f: u8,
    pub u200: u32,
    pub target: Target,
    pub u1d8: u32,
    pub u1dc: [i32; 3],
    pub u1e8: u32,
    /// The pending behaviour call: name and flag.
    pub pending: String,
    pub pending_flag: u8,
    pub records: Vec<[u32; 3]>,
    pub u204: u8,
    pub u211: u8,
    pub u208: u32,
    /// The type of the movement in progress (negative: none) and its data.
    pub move_kind: i32,
    pub move_data: Vec<u8>,
    pub u1ec: String,
    pub u1fc: u16,
    pub behaviour: Behaviour,
}

impl Default for UnitBlock {
    /// The values the original writes for a field the loaded version lacks.
    fn default() -> UnitBlock {
        UnitBlock {
            old_flag: 0,
            u24c: 0,
            u22c: 0,
            u25c: 0,
            u238: [0; 3],
            u244: [-1; 2],
            u214: 0,
            u218: 0,
            u220: [0; 3],
            u24d: 0,
            u24e: 0,
            u230: 0,
            flags: [(0, 0); 2],
            u24f: 0,
            u250: 0,
            u20d: 0,
            u20c: 0,
            u1f8: 0,
            u20e: 0,
            u20f: 0,
            u200: 0,
            target: Target::default(),
            u1d8: 0,
            u1dc: [0; 3],
            u1e8: 0,
            pending: String::new(),
            pending_flag: 0,
            records: Vec::new(),
            u204: 0,
            u211: 0,
            u208: 0,
            move_kind: 2,
            move_data: Vec::new(),
            u1ec: String::new(),
            u1fc: 100,
            behaviour: Behaviour::default(),
        }
    }
}

/// The movement mode of a unit whose version has no movement block: on the ground.
const GROUND: i32 = 2;

/// A tile from a version before the world positions: its corner, (-1, -1) for none.
fn corner(t: [i32; 2]) -> [i32; 3] {
    if t == [-1, -1] {
        [-1; 3]
    } else {
        [t[0].wrapping_mul(64), t[1].wrapping_mul(64), 0]
    }
}

/// The tile of a position, the inverse of `corner` and of the centre.
fn tile(p: [i32; 3]) -> [i32; 2] {
    [p[0].div_euclid(64), p[1].div_euclid(64)]
}

fn read_tile(r: &mut Reader) -> R<[i32; 2]> {
    Ok([r.i32()?, r.i32()?])
}

fn read_pos(r: &mut Reader) -> R<[i32; 3]> {
    Ok([r.i32()?, r.i32()?, r.i32()?])
}

fn strings(r: &mut Reader) -> R<Vec<String>> {
    (0..r.count(4)?).map(|_| r.text()).collect()
}

/// A behaviour set (zoo format 6.10). Nested sets are read in a loop: all the leading parts, then
/// all the trailing parts, innermost first.
pub(super) fn behaviour_set(r: &mut Reader) -> R<Behaviour> {
    let mut levels = Vec::new();
    loop {
        let mut l = Level {
            name: r.text()?,
            u58: r.u8()?,
            list_a: strings(r)?,
            u34: r.text()?,
            call: r.text()?,
            args: strings(r)?,
            ..Level::default()
        };
        l.flags.copy_from_slice(r.take(3)?);
        l.child = r.u8()?;
        let more = l.child != 0;
        levels.push(l);
        if !more {
            break;
        }
    }
    for l in levels.iter_mut().rev() {
        l.list_b = strings(r)?;
        l.tail.copy_from_slice(r.take(4)?);
    }
    Ok(Behaviour { levels })
}

/// Reads the zoo unit block and the base unit block (up to the entity base); returns them with the
/// movement mode.
pub(super) fn read(r: &mut Reader, guest: bool) -> R<(UnitBlock, u32)> {
    let v = r.v;
    let mut u = UnitBlock::default();
    let mut mode = GROUND;
    if v <= 57 {
        u.old_flag = r.u8()?;
    }
    u.u24c = r.u8()?;
    u.u22c = r.u32()?;
    u.u25c = r.u8()?;
    u.u238 = if v <= 90 {
        let t = read_tile(r)?;
        if t == [-1, -1] {
            [-1; 3]
        } else {
            [
                t[0].wrapping_mul(64).wrapping_add(32),
                t[1].wrapping_mul(64).wrapping_add(32),
                0,
            ]
        }
    } else {
        read_pos(r)?
    };
    u.u244 = read_tile(r)?;
    u.u214 = r.u32()?;
    u.u218 = r.u32()?;
    u.u220 = if v <= 85 {
        let (t, inner) = (read_tile(r)?, read_tile(r)?);
        [
            t[0].wrapping_mul(64).wrapping_add(inner[0]),
            t[1].wrapping_mul(64).wrapping_add(inner[1]),
            0,
        ]
    } else {
        read_pos(r)?
    };
    u.u24d = r.u8()?;
    u.u24e = r.u8()?;
    u.u230 = r.u32()?;
    if v >= 97 && !guest {
        // Each flag is followed by a value only when it is exactly 1.
        for f in &mut u.flags {
            f.0 = r.u8()?;
            if f.0 == 1 {
                f.1 = r.u16()?;
            }
        }
    }
    if v >= 98 {
        u.u24f = r.u8()?;
        u.u250 = r.u8()?;
    }
    u.u20d = r.u8()?;
    u.u20c = r.u8()?;
    u.u1f8 = r.u32()?;
    u.u20e = r.u8()?;
    u.u20f = r.u8()?;
    u.u200 = r.u32()?;
    let t = &mut u.target;
    t.goal = r.i32()?;
    t.sub_goal = r.i32()?;
    t.words = [r.u32()?, r.u32()?];
    if v <= 85 {
        t.target = corner(read_tile(r)?);
        t.sub_target = corner(read_tile(r)?);
    } else {
        t.target = read_pos(r)?;
        t.sub_target = read_pos(r)?;
    }
    t.bytes = [r.u8()?, r.u8()?];
    if v >= 53 {
        t.extra = [r.u32()?, r.u32()?];
    }
    // Before 72 the active goal is the one built for the wanted task.
    (t.active, t.active_sub) = (t.goal, 0);
    if v >= 72 {
        (t.active, t.active_sub) = (r.i32()?, r.i32()?);
    }
    u.u1d8 = r.u32()?;
    u.u1dc = if v <= 74 {
        corner(read_tile(r)?)
    } else {
        read_pos(r)?
    };
    u.u1e8 = r.u32()?;
    u.pending = r.text()?;
    u.pending_flag = r.u8()?;
    let size = if v <= 83 { 8 } else { 12 };
    for _ in 0..r.count(size)? {
        let a = [r.u32()?, r.u32()?, if v >= 84 { r.u32()? } else { 0 }];
        u.records.push(a);
    }
    if v >= 46 {
        // A value above 10 is reset.
        u.u204 = Some(r.u8()?).filter(|&n| n <= 10).unwrap_or(0);
    }
    if v >= 62 {
        u.u211 = r.u8()?;
    }
    if v >= 84 {
        u.u208 = r.u32()?;
        let first = r.i32()?;
        mode = first;
        u.move_kind = if v >= 93 { r.i32()? } else { first };
        match u.move_kind {
            k if k < 0 => {}
            1 | 2 => {}
            3 => u.move_data = r.take(if v >= 96 { 1 + 32 } else { 1 + 12 })?.to_vec(),
            _ => return bad("movement of an unknown type"),
        }
    }
    if v >= 85 {
        u.u1ec = r.text()?;
    }
    if v >= 98 {
        u.u1fc = r.u16()?;
    }
    Ok((u, mode as u32))
}

fn put_tile(o: &mut Out, t: [i32; 2]) {
    t.iter().for_each(|&n| o.i32(n));
}

fn put_pos(o: &mut Out, p: [i32; 3]) {
    p.iter().for_each(|&n| o.i32(n));
}

fn put_strings(o: &mut Out, list: &[String]) -> Result<(), String> {
    o.u32(list.len() as u32);
    list.iter().try_for_each(|s| o.string(s))
}

/// A behaviour set, the mirror of `behaviour_set`.
pub(super) fn write_behaviour(o: &mut Out, b: &Behaviour) -> Result<(), String> {
    let last = b.levels.len().saturating_sub(1);
    for (i, l) in b.levels.iter().enumerate() {
        o.string(&l.name)?;
        o.u8(l.u58);
        put_strings(o, &l.list_a)?;
        o.string(&l.u34)?;
        o.string(&l.call)?;
        put_strings(o, &l.args)?;
        o.bytes(&l.flags);
        // The last level has no child, whatever it holds.
        o.u8(if i == last { 0 } else { l.child.max(1) });
    }
    for l in b.levels.iter().rev() {
        put_strings(o, &l.list_b)?;
        o.bytes(&l.tail);
    }
    Ok(())
}

/// The mirror of `read`, in version `v`: the fields the version lacks are not written, and the
/// positions go back to the tiles of the versions that stored tiles.
pub(super) fn write(
    o: &mut Out,
    v: u32,
    guest: bool,
    mode: u32,
    u: &UnitBlock,
) -> Result<(), String> {
    if v <= 57 {
        o.u8(u.old_flag);
    }
    o.u8(u.u24c);
    o.u32(u.u22c);
    o.u8(u.u25c);
    if v <= 90 {
        put_tile(o, tile(u.u238));
    } else {
        put_pos(o, u.u238);
    }
    put_tile(o, u.u244);
    o.u32(u.u214);
    o.u32(u.u218);
    if v <= 85 {
        // The position (-65, -65) is the pair of tiles (-1, -1).
        let (t, inner) = if u.u220[..2] == [-65, -65] {
            ([-1; 2], [-1; 2])
        } else {
            let p = u.u220;
            (tile(p), [p[0].rem_euclid(64), p[1].rem_euclid(64)])
        };
        put_tile(o, t);
        put_tile(o, inner);
    } else {
        put_pos(o, u.u220);
    }
    o.u8(u.u24d);
    o.u8(u.u24e);
    o.u32(u.u230);
    if v >= 97 && !guest {
        for &(flag, n) in &u.flags {
            o.u8(flag);
            if flag == 1 {
                o.u16(n);
            }
        }
    }
    if v >= 98 {
        o.u8(u.u24f);
        o.u8(u.u250);
    }
    o.u8(u.u20d);
    o.u8(u.u20c);
    o.u32(u.u1f8);
    o.u8(u.u20e);
    o.u8(u.u20f);
    o.u32(u.u200);
    let t = &u.target;
    o.i32(t.goal);
    o.i32(t.sub_goal);
    t.words.iter().for_each(|&n| o.u32(n));
    for p in [t.target, t.sub_target] {
        if v <= 85 {
            put_tile(o, tile(p));
        } else {
            put_pos(o, p);
        }
    }
    t.bytes.iter().for_each(|&n| o.u8(n));
    if v >= 53 {
        t.extra.iter().for_each(|&n| o.u32(n));
    }
    if v >= 72 {
        o.i32(t.active);
        o.i32(t.active_sub);
    }
    o.u32(u.u1d8);
    if v <= 74 {
        put_tile(o, tile(u.u1dc));
    } else {
        put_pos(o, u.u1dc);
    }
    o.u32(u.u1e8);
    o.string(&u.pending)?;
    o.u8(u.pending_flag);
    o.u32(u.records.len() as u32);
    for rec in &u.records {
        rec[..if v <= 83 { 2 } else { 3 }]
            .iter()
            .for_each(|&n| o.u32(n));
    }
    if v >= 46 {
        o.u8(u.u204);
    }
    if v >= 62 {
        o.u8(u.u211);
    }
    if v >= 84 {
        o.u32(u.u208);
        o.u32(mode);
        // Before 93 the mode is also the type.
        let kind = if v >= 93 { u.move_kind } else { mode as i32 };
        if v >= 93 {
            o.i32(kind);
        }
        if kind == 3 {
            let mut data = u.move_data.clone();
            data.resize(if v >= 96 { 1 + 32 } else { 1 + 12 }, 0);
            o.bytes(&data);
        }
    }
    if v >= 85 {
        o.string(&u.u1ec)?;
    }
    if v >= 98 {
        o.u16(u.u1fc);
    }
    Ok(())
}
