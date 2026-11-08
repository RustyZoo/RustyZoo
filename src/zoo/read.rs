//! The reader of a file: a bounds-checked cursor over untrusted bytes and the sections read with it.

use super::*;

/// The end of each section of a file read: a name and an offset.
pub(super) type Marks = Vec<(String, usize)>;

/// Why a read failed, in a few words.
pub(super) struct Bad(pub(super) String);

pub(super) type R<T> = Result<T, Bad>;

pub(super) fn bad<T>(why: &str) -> R<T> {
    Err(Bad(why.into()))
}

/// Whether the types of `catalogue` have a definition for an entity.
pub(crate) fn knows(catalogue: &Catalogue) -> impl Fn(&Entity) -> bool + '_ {
    let bytes = |s: &str| s.chars().map(|c| c as u8).collect::<Vec<u8>>();
    move |e| {
        catalogue
            .find(&e.class, &bytes(&e.type_name), &bytes(&e.subtype))
            .is_some()
    }
}

/// A bounds-checked reader over untrusted bytes; `v` is the version of the file.
pub(super) struct Reader<'a> {
    pub(super) data: &'a [u8],
    pub(super) v: u32,
    /// The length `data` had at the start of the body being read, and where its entity base lies.
    pub(super) total: usize,
    pub(super) spans: Spans,
    /// The behaviour set of the entity base just read.
    pub(super) behaviour: Option<Behaviour>,
    /// The tile grid, once it is read: the old tiles of a unit become positions with its heights.
    pub(super) ground: Option<Rc<Ground>>,
    /// The copy being made of what is read, in the form of version 106: fields the version lacks
    /// are put into it, fields it no longer has are left out. A copy is made when it is `want`ed,
    /// while `copy` is set.
    pub(super) tee: Vec<u8>,
    pub(super) want: bool,
    pub(super) copy: bool,
    /// The copy of the fields before the unit block or the entity base, and after them.
    pub(super) pre: Vec<u8>,
    pub(super) post: Vec<u8>,
    /// The end of each section read, when asked for ([`Zoo::sections`]).
    pub(super) marks: Option<Marks>,
}

/// The map of a file, for the heights of the positions an old file stores as tiles.
pub(super) struct Ground {
    pub(super) width: u32,
    pub(super) height: u32,
    pub(super) tiles: Vec<Tile>,
}

impl Ground {
    /// The ground of a map. A hostile elevation must not overflow the height, so it is clamped.
    pub(super) fn of(width: u32, height: u32, tiles: &[Tile]) -> Ground {
        Ground {
            width,
            height,
            tiles: tiles
                .iter()
                .map(|t| Tile {
                    elevation: t.elevation.clamp(-(1 << 24), 1 << 24),
                    ..*t
                })
                .collect(),
        }
    }

    /// The tile at the tile coordinates, if it is on the map.
    pub(super) fn tile(&self, x: i32, y: i32) -> Option<&Tile> {
        let (x, y) = (u32::try_from(x).ok()?, u32::try_from(y).ok()?);
        if x >= self.width || y >= self.height {
            return None;
        }
        self.tiles
            .get(y as usize * self.width as usize + x as usize)
    }

    /// The height of the ground at a world point, 0 outside the map.
    pub(super) fn at(&self, x: i32, y: i32) -> i32 {
        self.tile(x.div_euclid(64), y.div_euclid(64))
            .map_or(0, |t| ground_height(t, x.rem_euclid(64), y.rem_euclid(64)))
    }
}

impl<'a> Reader<'a> {
    pub(super) fn new(data: &'a [u8], v: u32) -> Reader<'a> {
        Reader {
            data,
            v,
            total: data.len(),
            spans: Spans::default(),
            behaviour: None,
            ground: None,
            tee: Vec::new(),
            want: false,
            copy: false,
            pre: Vec::new(),
            post: Vec::new(),
            marks: None,
        }
    }

    /// Notes that section `name` ends here.
    pub(super) fn mark(&mut self, name: &str) {
        let at = 8 + self.total - self.data.len();
        if let Some(marks) = &mut self.marks {
            marks.push((name.into(), at));
        }
    }

    /// Reads without copying what `f` reads.
    pub(super) fn quiet<T>(&mut self, f: impl FnOnce(&mut Self) -> R<T>) -> R<T> {
        let was = std::mem::replace(&mut self.copy, false);
        let out = f(self);
        self.copy = was;
        out
    }

    /// Puts bytes into the copy only (a field the version lacks).
    pub(super) fn put(&mut self, bytes: &[u8]) {
        if self.copy {
            self.tee.extend(bytes);
        }
    }

    /// Starts copying, if a copy is wanted.
    pub(super) fn copy_on(&mut self) {
        self.copy = self.want;
    }

    /// Ends the part of the copy before the unit block or the entity base.
    pub(super) fn cut(&mut self) {
        self.pre = std::mem::take(&mut self.tee);
        self.copy = false;
    }

    /// The offset in the body being read.
    pub(super) fn pos(&self) -> usize {
        self.total - self.data.len()
    }

    pub(super) fn take(&mut self, len: usize) -> R<&'a [u8]> {
        if len > self.data.len() {
            return bad("truncated");
        }
        let (head, tail) = self.data.split_at(len);
        self.data = tail;
        if self.copy {
            self.tee.extend(head);
        }
        Ok(head)
    }

    pub(super) fn skip(&mut self, len: usize) -> R<()> {
        self.take(len).map(|_| ())
    }

    pub(super) fn u8(&mut self) -> R<u8> {
        Ok(self.take(1)?[0])
    }

    pub(super) fn u16(&mut self) -> R<u16> {
        Ok(u16::from_le_bytes(self.take(2)?.try_into().unwrap()))
    }

    /// A count and that many event records.
    pub(super) fn events(&mut self) -> R<Vec<Event>> {
        (0..self.count(20)?)
            .map(|_| {
                Ok(Event {
                    sender: self.u32()?,
                    target: self.u32()?,
                    data: self.u32()?,
                    extra: self.u16()?,
                    kind: self.u16()?,
                    delay: self.u16()?,
                    sender_channel: self.u8()?,
                    channel: self.u8()?,
                })
            })
            .collect()
    }

    pub(super) fn u64(&mut self) -> R<u64> {
        Ok(u64::from_le_bytes(self.take(8)?.try_into().unwrap()))
    }

    pub(super) fn u32(&mut self) -> R<u32> {
        Ok(u32::from_le_bytes(self.take(4)?.try_into().unwrap()))
    }

    pub(super) fn f32(&mut self) -> R<f32> {
        self.u32().map(f32::from_bits)
    }

    pub(super) fn i32(&mut self) -> R<i32> {
        self.u32().map(|n| n as i32)
    }

    /// A string: `u32` length below 4096, then the bytes.
    pub(super) fn string(&mut self) -> R<&'a [u8]> {
        let len = self.u32()?;
        if len >= 4096 {
            return bad("string of 4096 bytes or more");
        }
        self.take(len as usize)
    }

    /// A string, one character per byte.
    pub(super) fn text(&mut self) -> R<String> {
        Ok(latin1(self.string()?))
    }

    /// A `u32` count of elements of at least `min_size` bytes each, checked against the bytes
    /// left, so a loop of that many steps is bounded by the input.
    pub(super) fn count(&mut self, min_size: usize) -> R<usize> {
        let n = self.u32()? as usize;
        if n > self.data.len() / min_size {
            return bad("count passes the end of the file");
        }
        Ok(n)
    }

    /// A count and that many elements of `size` bytes.
    pub(super) fn skip_list(&mut self, size: usize) -> R<()> {
        let n = self.count(size)?;
        self.skip(n * size)
    }

    /// A count and that many strings.
    pub(super) fn skip_strings(&mut self) -> R<()> {
        for _ in 0..self.count(4)? {
            self.string()?;
        }
        Ok(())
    }
}

pub(super) fn latin1(bytes: &[u8]) -> String {
    bytes.iter().map(|&b| b as char).collect()
}

/// Reads the file after the magic and the version, noting in `section` where it is.
pub(super) fn read(r: &mut Reader, section: &mut &'static str, money: &mut Money) -> R<Zoo> {
    let v = r.v;
    let locale = if v >= 69 { r.u32()? } else { 0 };
    let locale = if locale == 0 { 1033 } else { locale };
    let expansion = if v >= 82 { r.u32()? } else { 0 };
    r.mark("header");

    *section = "habitats";
    let (width, height) = (r.u32()?, r.u32()?);
    let entrance = (r.u32()?, r.u32()?);
    let from = r.data;
    let habitat_count = r.count(HABITAT_MIN)?;
    let mut tanks = Vec::new();
    let mut habitat_tiles = Vec::new();
    let mut habitat_gates = Vec::new();
    let mut habitat_info = Vec::new();
    let mut stats_at = Vec::new();
    let mut names_at = Vec::new();
    let mut shows = Shows::default();
    let mut show_at = Vec::new();
    for _ in 0..habitat_count {
        let (tile, gate, tank, info, at, name, show) = habitat(r, from.len())?;
        stats_at.push(at);
        names_at.push(name);
        shows.infos.push(show.0);
        show_at.push(show.1);
        habitat_tiles.push(tile);
        habitat_gates.push(gate);
        habitat_info.push(info);
        tanks.extend(tank);
    }
    if v >= 64 {
        r.skip(4)?;
    }
    let habitats = taken(from, r);
    r.mark("habitats");
    if v >= 97 {
        *section = "shows";
        shows.read_section(r)?;
        r.mark("shows");
    }

    *section = "id counter";
    let id_counter = r.u32()?;
    r.mark("id counter");

    *section = "tiles";
    let n = u64::from(width) * u64::from(height);
    if n > r.data.len() as u64 / 10 {
        return bad("the map is bigger than the file");
    }
    let mut tiles = Vec::with_capacity(n as usize);
    for _ in 0..n {
        let rec = r.take(10)?;
        tiles.push(Tile {
            elevation: i32::from_le_bytes(rec[..4].try_into().unwrap()),
            shape: rec[4],
            terrain: rec[5],
            edges: rec[6],
        });
    }

    r.mark("tiles");
    r.ground = Some(Rc::new(Ground::of(width, height, &tiles)));

    *section = "entities";
    let min_record = if v >= 70 { 16 } else { 12 };
    let count = r.count(min_record)?;
    let mut entities = Vec::with_capacity(count);
    for i in 0..count {
        let entity = entity(r).map_err(|Bad(why)| Bad(format!("entity {i}: {why}")))?;
        if r.marks.is_some() {
            r.mark(&format!("entity/{}", entity.class));
        }
        entities.push(entity);
    }

    *section = "later sections";
    let mut awards = Vec::new();
    let (view, clocks, names, mut kept) = after_entities(r, money, &mut awards, &mut shows)?;
    kept.habitats = habitats;
    kept.show_at = show_at;
    kept.stats_at = stats_at;
    kept.names_at = names_at;
    Ok(Zoo {
        version: v,
        locale,
        expansion,
        width,
        height,
        entrance,
        habitat_tiles,
        habitat_gates,
        habitat_info,
        tanks,
        shows,
        id_counter,
        tiles,
        entities,
        view,
        clocks,
        names,
        kept,
        awards,
    })
}

/// The bytes `r` has read since `from` was its data.
pub(super) fn taken(from: &[u8], r: &Reader) -> Vec<u8> {
    from[..from.len() - r.data.len()].to_vec()
}

pub(super) fn entity(r: &mut Reader) -> R<Entity> {
    let class = r.string()?;
    let type_name = r.text()?;
    let subtype = r.text()?;
    let (body_size, common) = if r.v >= 70 {
        let size = r.u32()?;
        let data = r.take(size as usize)?;
        // A record of a parent class is read with the layout of its type's class, which
        // `Zoo::checked` knows: the bytes wait for it (zoo format 6.15 points 1 and 2). `objects` and
        // `fences` are read as stored first, which is right for nearly every record.
        let deferred = || {
            Some(Common {
                raw: Raw {
                    body: data.to_vec(),
                    spans: Spans::default(),
                },
                ..Common::default()
            })
        };
        let common = match class {
            _ if size == 0 => None,
            b"staff" | b"unit" => deferred(),
            b"objects" | b"fences" => sized(data, r.v, r.ground.clone(), class).or_else(deferred),
            _ => sized(data, r.v, r.ground.clone(), class),
        };
        (Some(size), common)
    } else {
        let (from, v) = (r.data, r.v);
        let mut body = Reader::new(from, v);
        body.ground = r.ground.clone();
        match body::decode(&mut body, class)? {
            Some(mut common) => {
                let data = &from[..from.len() - body.data.len()];
                common.raw = Raw {
                    body: data.to_vec(),
                    spans: body.spans,
                };
                r.data = body.data;
                (None, Some(common))
            }
            None => return bad("unknown class in a file without body sizes"),
        }
    };
    let mut common: Option<Common> = common;
    if class != b"food"
        && !["poo", "dinopoo"]
            .iter()
            .any(|s| subtype.eq_ignore_ascii_case(s))
    {
        common.iter_mut().for_each(|c| c.food = 0);
    }
    Ok(Entity {
        class: latin1(class),
        type_name,
        subtype,
        body_size,
        common,
    })
}

/// Decodes a body that has its stored size. A body that does not decode to exactly its size is
/// skipped, like an unknown class.
fn sized(data: &[u8], v: u32, ground: Option<Rc<Ground>>, class: &[u8]) -> Option<Common> {
    let mut body = Reader::new(data, v);
    body.ground = ground;
    match body::decode(&mut body, class) {
        Ok(Some(mut common)) if body.data.is_empty() && !data.is_empty() => {
            common.raw = Raw {
                body: data.to_vec(),
                spans: body.spans,
            };
            Some(common)
        }
        _ => None,
    }
}

/// Reads a record of a parent class again with the layout of the class of its type
/// (zoo format 6.2 point 2), once `Zoo::checked` has found that type.
pub(super) fn reread(zoo: &mut Zoo, at: usize, class: &'static str) {
    let ground = Rc::new(Ground::of(zoo.width, zoo.height, &zoo.tiles));
    let e = &mut zoo.entities[at];
    let data = e.common.take().map(|c| c.raw.body).unwrap_or_default();
    e.class = class.into();
    e.common = sized(&data, zoo.version, Some(ground), class.as_bytes());
}

/// Reads and drops the sections after the entities, as far as the original loader checks them.
pub(super) fn after_entities(
    r: &mut Reader,
    money: &mut Money,
    awards: &mut Vec<i32>,
    shows: &mut Shows,
) -> R<(View, Clocks, BTreeMap<String, u32>, Kept)> {
    let mut clocks = Clocks::default();
    let mut kept = Kept::default();
    let v = r.v;
    let view = View {
        zoom: r.i32()?,
        rotation: r.i32()?,
        scroll: (r.i32()?, r.i32()?),
        brush: r.u32()?,
    };
    r.mark("entities end, view");

    // Scenario: a size, then (from 51, unless the size is 0) a name and the goal states, one
    // `u32` each. The original skips the block by its size when it starts a scenario and reads
    // its content otherwise, and the two agree only when the size is exactly that of the
    // content, so any other size is refused. Before 51 the block is the name alone.
    let from = r.data;
    let size = u64::from(r.u32()?);
    if size > 0 && v >= 51 {
        let name = u64::from(r.u32()?);
        r.skip(name as usize)?;
        let goals = u64::from(r.u32()?);
        if goals % 4 != 0 {
            return bad("scenario goal states are not a multiple of 4 bytes");
        }
        r.skip(goals as usize)?;
        if size != 8 + name + goals {
            return bad("scenario block size does not match its content");
        }
    } else {
        r.skip(size as usize)?;
    }
    kept.scenario = taken(from, r);
    r.mark("scenario");

    // Name counters: type name and number.
    let mut names = BTreeMap::new();
    for _ in 0..r.count(8)? {
        let name = latin1(r.string()?);
        names.insert(name, r.u32()?);
    }
    r.mark("name counters");

    // Game and finances: the zoo status' timers, rating and donation counter first.
    let from = r.data;
    r.skip(4)?;
    clocks.status.timers = [r.u32()?, r.u32()?, r.u32()?];
    r.skip(1)?;
    clocks.status.rating = r.u32()?;
    r.skip(12)?;
    clocks.status.given = r.f32()?;
    (money.month, money.year) = (r.u32()?.clamp(1, 12), r.u32()?.min(19));
    let (rows, columns) = (u64::from(r.u32()?), u64::from(r.u32()?));
    let tables = ((12 + columns + 1) * 4)
        .checked_mul(rows)
        .filter(|&n| n <= r.data.len() as u64);
    tables.ok_or_else(|| Bad("finance tables pass the end of the file".into()))?;
    // The rows past the 31 and the columns past the 20 that are kept are read and dropped.
    let (rows, columns) = (rows as usize, columns as usize);
    for m in 0..rows {
        for column in 0..MONTHS {
            let value = r.f32()?;
            if m < CATEGORIES {
                money.monthly[m][column] = value;
            }
        }
    }
    for y in 0..rows {
        for column in 0..columns {
            let value = r.f32()?;
            if y < CATEGORIES && column < YEARS {
                money.yearly[y][column] = value;
            }
        }
    }
    for c in 0..rows {
        let value = r.f32()?;
        if c < CATEGORIES {
            money.total[c] = value;
        }
    }
    clocks.status.price = if v >= 39 { r.f32()? } else { 49.0 };
    clocks.status.escape = if v >= 71 { r.u64()? } else { upgrade::NEVER };
    // The calendar: year, month, day of week, day, hour, minute, second, millisecond.
    let [year, month, _, day, hour, minute, second, millisecond] = [(); 8].map(|_| r.u16());
    clocks.calendar = crate::timing::Date {
        year: u32::from(year?),
        month: u32::from(month?),
        day: u32::from(day?),
        weekday: 0,
        hour: u32::from(hour?),
        minute: u32::from(minute?),
        second: u32::from(second?),
        millisecond: u32::from(millisecond?),
    }
    .game_ms();
    money.set_cash(r.f32()?);
    if v >= 73 {
        clocks.play_time = r.u32()?;
    }
    kept.finances = taken(from, r);
    r.mark("finances");

    kept.thoughts = (0..r.count(20)?)
        .map(|_| Ok([r.u32()?, r.u32()?, r.u32()?, r.u32()?, r.u32()?]))
        .collect::<R<_>>()?;
    // The AI clock: the milliseconds left over, then the whole AI ticks (7.6).
    (clocks.ai_rem, clocks.ai_ticks) = (r.u32()?, r.u32()?);
    if v >= 97 {
        clocks.queue.first = r.events()?;
        clocks.queue.second = r.events()?;
        clocks.queue.next_is_first = r.u8()? != 0;
    }
    r.mark("thoughts, ai clock, events");
    let from = r.data;
    if v >= 40 {
        r.skip(4)?;
        loop {
            let kind = r.i32()?;
            if kind < 0 {
                break;
            }
            r.skip(4)?;
            match kind {
                0 | 2 => r.skip(4)?,
                1 => r.skip(1)?,
                _ => return bad("research record of an unknown type"),
            }
        }
    }
    kept.research = taken(from, r);
    r.mark("research");
    if v >= 59 {
        for _ in 0..r.count(4)? {
            crate::award::add(awards, r.i32()?);
        }
    }
    r.mark("awards");
    let from = r.data;
    if v >= 60 {
        r.skip(4)?; // marketing
    }
    kept.marketing = taken(from, r);
    if (89..=96).contains(&v) {
        shows.read_section(r)?;
    }
    r.mark("marketing, shows");
    if v >= 104 {
        let from = r.data;
        r.skip_strings()?; // cheats
        kept.cheats = taken(from, r);
    }
    r.mark("cheats");
    if v >= 58 {
        // The world clock: the milliseconds left over, then the whole seconds (7.11).
        (clocks.world_rem, clocks.world_secs) = (r.u32()?, r.u32()?);
        r.skip(if v <= 66 { 4 } else { 0 })?;
    }
    r.mark("world clock");
    Ok((view, clocks, names, kept))
}
