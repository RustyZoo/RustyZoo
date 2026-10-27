//! Synthetic files built in code; no original data.

use super::write::Out;
use super::*;

/// A growing byte buffer with the field types of the format.
#[derive(Default)]
struct W(Vec<u8>);

impl W {
    fn u8(&mut self, n: u8) -> &mut Self {
        self.0.push(n);
        self
    }
    fn u16(&mut self, n: u16) -> &mut Self {
        self.0.extend(n.to_le_bytes());
        self
    }
    fn u32(&mut self, n: u32) -> &mut Self {
        self.0.extend(n.to_le_bytes());
        self
    }
    fn i32(&mut self, n: i32) -> &mut Self {
        self.u32(n as u32)
    }
    fn z(&mut self, n: usize) -> &mut Self {
        self.0.resize(self.0.len() + n, 0);
        self
    }
    fn str(&mut self, s: &str) -> &mut Self {
        self.u32(s.len() as u32);
        self.0.extend(s.as_bytes());
        self
    }
    fn bytes(&mut self, b: &[u8]) -> &mut Self {
        self.0.extend(b);
        self
    }
}

fn header(w: &mut W, v: u32, locale: u32) {
    w.bytes(b"TZFB").u32(v);
    if v >= 69 {
        w.u32(locale);
    }
    if v >= 82 {
        w.u32(2);
    }
}

/// One habitat with a name, optionally a tank and a show.
fn habitat(w: &mut W, v: u32, tank: bool, show: bool) {
    w.i32(13)
        .i32(45)
        .str("ex")
        .i32(-1)
        .i32(-1)
        .i32(6)
        .z(36 + 16);
    if v >= 83 {
        w.z(4);
    }
    if v >= 85 {
        w.u8(tank.into());
    }
    if v >= 90 {
        w.u8(show.into());
        if show {
            show_info(w, v);
        }
    }
    if tank && v >= 85 {
        w.u32(9).u32(6);
        if v >= 88 {
            w.z(1);
        }
        if v >= 100 {
            w.u32(1).u32(5).u32(77);
        }
    }
}

/// Show info with one element in every list.
fn show_info(w: &mut W, v: u32) {
    w.z(8).u32(1).z(4);
    if v <= 98 {
        w.u32(1).z(8);
    }
    w.u32(1).z(if v >= 99 { 44 } else { 6 });
    if v >= 97 {
        w.u32(1).z(20).z(6);
    }
    if v >= 105 {
        w.z(12);
    }
    if v >= 106 {
        w.z(24);
    }
    if v >= 97 {
        w.z(16 + 5 + 16).u32(1).z(14);
    }
    if v == 101 {
        w.z(1);
    }
}

/// The scripts (from 89) and counters (from 97), with one script of one item.
fn shows(w: &mut W, v: u32, scripts: u32) {
    if v >= 89 {
        w.u32(scripts);
        for _ in 0..scripts {
            w.z(10).u32(1).z(12);
            w.str("a").str("").str("bc").str("");
            w.z(8 + 1 + 16);
            if v >= 103 {
                w.z(8).str("icon").str("grey");
            }
        }
    }
    if v >= 97 {
        w.z(4);
    }
}

/// The sections after the entities, with something in every list.
fn tail(w: &mut W, v: u32, scripts: u32) {
    w.i32(-2).i32(3).i32(1216).i32(-16).u32(5); // view
    if v >= 51 {
        w.u32(4 + 5 + 4 + 8).str("scn/x").u32(8).z(8);
    } else {
        w.u32(5).str("x"); // before 51 the block is the name alone
    }
    w.u32(2).str("a").u32(1).str("b").u32(2); // name counters
    // The zoo status: timers, month flag, rating and three more numbers, the donation counter.
    w.z(4)
        .u32(11)
        .u32(12)
        .u32(13)
        .z(1)
        .u32(14)
        .z(12)
        .u32(2.5f32.to_bits());
    w.u32(5).u32(3); // month and year of the accounts
    w.u32(2).u32(3); // two categories, three columns
    let float = |w: &mut W, f: f32| {
        w.u32(f.to_bits());
    };
    for r in 0..2 {
        (0..12).for_each(|c| float(w, 100.0 * r as f32 + c as f32));
    }
    for r in 0..2 {
        (0..3).for_each(|c| float(w, 1000.0 + 10.0 * r as f32 + c as f32));
    }
    (0..2).for_each(|r| float(w, 77.0 + r as f32));
    if v >= 39 {
        w.u32(22.5f32.to_bits()); // the adult admission price
    }
    if v >= 71 {
        w.z(8);
    }
    // The calendar: Friday 14 March 2003, 06:30:15.250.
    for n in [2003, 3, 5, 14, 6, 30, 15, 250] {
        w.u16(n);
    }
    w.u32(1234.5f32.to_bits()); // cash
    if v >= 73 {
        w.u32(123_456); // the play time
    }
    w.u32(2).z(40); // two thoughts, all 0
    w.u32(7).u32(8); // the AI clock: milliseconds left over, ticks
    if v >= 97 {
        // One event in the first buffer, none in the second, and the first is the next one.
        w.u32(1).u32(1).u32(2).u32(3);
        w.0.extend([4, 0, 0x11, 0x27, 5, 0, 0x55, 0x4b]);
        w.u32(0);
        w.0.push(1);
    }
    if v >= 40 {
        w.z(4);
        w.i32(0).z(8).i32(1).z(4 + 1).i32(2).z(8);
        w.i32(-1);
    }
    if v >= 59 {
        w.u32(1).z(4);
    }
    if v >= 60 {
        w.z(4);
    }
    if (89..=96).contains(&v) {
        shows(w, v, scripts);
    }
    if v >= 104 {
        w.u32(1).str("cheat");
    }
    if v >= 58 {
        w.u32(9).u32(10); // the world clock: milliseconds left over, seconds
        w.z(if v <= 66 { 4 } else { 0 });
    }
}

/// The common part of every body.
fn base(w: &mut W, v: u32, rot: u32, id: u32, name: &str) {
    w.i32(100).i32(-5).i32(32).u32(rot).u32(id).str(name);
    if v <= 62 {
        w.str("old");
    }
    w.z(5);
    if v >= 58 {
        w.z(1);
    }
    if v >= 63 {
        behaviour(w, 0);
    }
    if v >= 67 {
        w.u32(2).u8(3).u8(9);
    }
}

/// A behaviour set with `children` nested sets, each level with its own strings.
fn behaviour(w: &mut W, children: usize) {
    for level in 0..=children {
        w.str("bWalk").z(1).u32(1).str("x").str("");
        w.str("call").u32(0).z(3);
        w.u8((level < children).into());
    }
    for _ in 0..=children {
        w.u32(1).str("y").z(4);
    }
}

fn scenery(v: u32, id: u32, name: &str) -> Vec<u8> {
    let mut w = W::default();
    w.z(4);
    base(&mut w, v, 2, id, name);
    w.0
}

/// An animal-like body: `before` zero bytes up to the movement block, then the rest.
fn unit(v: u32, before: usize, id: u32) -> Vec<u8> {
    let mut w = W::default();
    w.z(before);
    if v >= 84 {
        w.z(4);
        if v >= 93 {
            w.i32(-1);
        }
    }
    let after = (v >= 85) as usize * 4 + (v >= 98) as usize * 2;
    w.z(after);
    base(&mut w, v, 6, id, "unit");
    w.0
}

/// The file: sections up to the entities, each entity as `(class, type, subtype, body)`.
pub(crate) fn file(
    v: u32,
    width: u32,
    height: u32,
    habitats: &[(bool, bool)],
    ents: &[(&str, &str, &str, Vec<u8>)],
) -> Vec<u8> {
    file_with(v, width, height, habitats, ents, 1)
}

/// [`file`] with `scripts` show scripts.
fn file_with(
    v: u32,
    width: u32,
    height: u32,
    habitats: &[(bool, bool)],
    ents: &[(&str, &str, &str, Vec<u8>)],
    scripts: u32,
) -> Vec<u8> {
    let mut w = W::default();
    header(&mut w, v, 1033);
    w.u32(width)
        .u32(height)
        .u32(11)
        .u32(42)
        .u32(habitats.len() as u32);
    for &(tank, show) in habitats {
        habitat(&mut w, v, tank, show);
    }
    if v >= 64 {
        w.u32(1);
    }
    if v >= 97 {
        shows(&mut w, v, scripts);
    }
    w.u32(7777);
    for i in 0..width * height {
        w.i32(i as i32 - 3)
            .u8(i as u8 & 0x55)
            .u8(i as u8 % 18)
            .u32(i & 0xff);
    }
    w.u32(ents.len() as u32);
    for (class, ty, sub, body) in ents {
        w.str(class).str(ty).str(sub);
        if v >= 70 {
            w.u32(body.len() as u32);
        }
        w.bytes(body);
    }
    tail(&mut w, v, scripts);
    w.0
}

#[test]
fn keeps_the_cash_and_the_accounts() {
    for v in [39, 71, 106] {
        let (_, money) = Zoo::parse_money(&file(v, 2, 2, &[], &[])).unwrap();
        assert_eq!((money.cash, money.month, money.year), (1234.5, 5, 3), "{v}");
        // The two rows that the file has are kept, the other categories are 0.
        assert_eq!(money.monthly[1][4], 104.0);
        assert_eq!(money.yearly[1][2], 1012.0);
        assert_eq!(money.yearly[1][3], 0.0, "a column the file does not have");
        assert_eq!(
            (money.total[0], money.total[1], money.total[2]),
            (77.0, 78.0, 0.0)
        );
        assert_eq!(money.monthly[2], [0.0; 12]);
    }
}

#[test]
fn reads_the_clocks_the_file_stores() {
    for v in VERSIONS {
        let zoo = parse(&file(v, 2, 2, &[], &[]));
        let world = if v >= 58 { (9, 10) } else { (0, 0) };
        let clocks = (zoo.clocks.ai_rem, zoo.clocks.ai_ticks);
        assert_eq!(clocks, (7, 8), "version {v}");
        assert_eq!(
            (zoo.clocks.world_rem, zoo.clocks.world_secs),
            world,
            "version {v}"
        );
        // Both event buffers and which one is next are stored from version 97.
        let queue = &zoo.clocks.queue;
        assert_eq!(queue.first.len(), usize::from(v >= 97), "version {v}");
        assert!(queue.second.is_empty());
        assert_eq!(queue.next_is_first, v >= 97, "version {v}");
        if let Some(e) = queue.first.first() {
            assert_eq!(
                (e.sender, e.target, e.data, e.extra, e.kind),
                (1, 2, 3, 4, 0x2711)
            );
            assert_eq!((e.delay, e.sender_channel, e.channel), (5, 0x55, 0x4b));
        }
    }
}

/// The file with its game date (the 2003 date of `file`) replaced by `date`.
fn with_date(bytes: &[u8], date: [u16; 8]) -> Vec<u8> {
    let old: Vec<u8> = [2003u16, 3, 5, 14, 6, 30, 15, 250]
        .iter()
        .flat_map(|n| n.to_le_bytes())
        .collect();
    let new: Vec<u8> = date.iter().flat_map(|n| n.to_le_bytes()).collect();
    let at: Vec<usize> = bytes
        .windows(old.len())
        .enumerate()
        .filter(|(_, w)| *w == old.as_slice())
        .map(|(i, _)| i)
        .collect();
    assert_eq!(at.len(), 1);
    let mut out = bytes.to_vec();
    out[at[0]..at[0] + new.len()].copy_from_slice(&new);
    out
}

#[test]
fn a_game_date_before_2001_is_kept_as_stored() {
    // Thursday 15 June 2000, 06:30:15.250 (the weekday is stored, and not read).
    let stored = [2000, 6, 3, 15, 6, 30, 15, 250];
    for v in VERSIONS {
        let zoo = parse(&with_date(&file(v, 2, 2, &[], &[]), stored));
        assert!(zoo.clocks.calendar < 0, "version {v}");
        let d = crate::timing::Calendar::at(zoo.clocks.calendar).date();
        assert_eq!(
            [
                d.year,
                d.month,
                d.day,
                d.hour,
                d.minute,
                d.second,
                d.millisecond
            ],
            [2000u32, 6, 15, 6, 30, 15, 250],
            "version {v}"
        );
    }
}

#[test]
fn a_refused_game_date_is_the_start_of_the_calendar() {
    for date in [
        [2003, 13, 5, 14, 6, 30, 15, 250],
        [2003, 0, 5, 14, 6, 30, 15, 250],
        [2003, 2, 5, 29, 6, 30, 15, 250],
        [2001, 2, 5, 29, 6, 30, 15, 250],
        [2003, 3, 5, 14, 24, 30, 15, 250],
        [1600, 12, 5, 31, 6, 30, 15, 250],
    ] {
        let zoo = parse(&with_date(&file(VERSIONS[0], 2, 2, &[], &[]), date));
        assert_eq!(zoo.clocks.calendar, 0, "{date:?}");
    }
}

#[test]
fn the_play_time_is_read_from_version_73_and_always_written() {
    for v in [71, 73, 106] {
        let (zoo, money) = Zoo::parse_money(&file(v, 2, 2, &[], &[])).unwrap();
        let time = if v >= 73 { 123_456 } else { 0 };
        assert_eq!(zoo.clocks.play_time, time, "version {v}");
        for set in [0, 7, u32::MAX] {
            let mut zoo = zoo.clone();
            zoo.clocks.play_time = set;
            let back = parse(&zoo.write(&money).unwrap());
            assert_eq!(back.clocks.play_time, set, "{v}");
        }
    }
}

#[test]
fn reads_the_game_date_the_file_stores() {
    // 802 days from 1 January 2001 to 14 March 2003.
    let ms = 802 * 86_400_000 + (6 * 3600 + 30 * 60 + 15) * 1000 + 250;
    for v in VERSIONS {
        assert_eq!(
            parse(&file(v, 2, 2, &[], &[])).clocks.calendar,
            ms,
            "version {v}"
        );
    }
}

#[test]
fn reads_what_the_zoo_status_stores() {
    for v in VERSIONS {
        let status = parse(&file(v, 2, 2, &[], &[])).clocks.status;
        assert_eq!(
            (status.timers, status.rating, status.given),
            ([11, 12, 13], 14, 2.5)
        );
        // A file without the price field has the price of old files.
        assert_eq!(
            status.price,
            if v >= 39 { 22.5 } else { 49.0 },
            "version {v}"
        );
    }
}

fn parse(bytes: &[u8]) -> Zoo {
    Zoo::parse(bytes).unwrap_or_else(|e| panic!("{e}"))
}

fn refusal(bytes: &[u8]) -> String {
    Zoo::parse(bytes).unwrap_err().to_string()
}

fn fence(v: u32, id: u32) -> Vec<u8> {
    let mut b = scenery(v, id, "Fence");
    if v >= 55 {
        b.extend([0; 16]);
    }
    b
}

/// Every version that has a distinct layout somewhere.
const VERSIONS: [u32; 28] = [
    38, 40, 45, 48, 55, 58, 60, 63, 66, 67, 69, 70, 71, 75, 82, 83, 85, 89, 90, 96, 97, 98, 99,
    101, 103, 104, 105, 106,
];

#[test]
fn refuses_what_the_original_refuses() {
    assert_eq!(refusal(b""), "not a saved game");
    assert_eq!(refusal(b"TZF"), "not a saved game");
    assert_eq!(refusal(b"tzfb\x46\0\0\0"), "not a saved game");
    assert_eq!(refusal(b"TZFB"), "corrupted: header: truncated");
    assert_eq!(refusal(b"TZFB\x25\0\0\0"), "unsupported old version 37");
    assert_eq!(
        refusal(b"TZFB\x21\0\0\0 and then some"),
        "unsupported old version 33"
    );
    assert_eq!(refusal(b"TZFB\x6b\0\0\0"), "made by a newer version (107)");
    assert_eq!(
        refusal(b"TZFB\xff\xff\xff\xff"),
        "made by a newer version (4294967295)"
    );
    // The version rules come first; a valid version with nothing after it is corrupted.
    assert!(refusal(b"TZFB\x46\0\0\0").starts_with("corrupted:"));
    assert!(refusal(b"TZFB\x26\0\0\0").starts_with("corrupted:"));
}

#[test]
fn reads_the_header_by_version() {
    let z = parse(&file(68, 3, 2, &[], &[]));
    assert_eq!((z.version, z.locale, z.expansion), (68, 1033, 0));
    let z = parse(&file(69, 3, 2, &[], &[]));
    assert_eq!((z.version, z.locale, z.expansion), (69, 1033, 0));
    let z = parse(&file(82, 3, 2, &[], &[]));
    assert_eq!((z.version, z.locale, z.expansion), (82, 1033, 2));
    // A stored locale of 0 reads as 1033.
    let mut bytes = file(106, 3, 2, &[], &[]);
    bytes[8..12].copy_from_slice(&0u32.to_le_bytes());
    assert_eq!(parse(&bytes).locale, 1033);
    bytes[8..12].copy_from_slice(&1031u32.to_le_bytes());
    assert_eq!(parse(&bytes).locale, 1031);
}

#[test]
fn reads_map_size_entrance_counter_and_tiles() {
    let z = parse(&file(70, 4, 3, &[], &[]));
    assert_eq!((z.width, z.height, z.entrance), (4, 3, (11, 42)));
    assert_eq!((z.habitat_tiles.len(), z.id_counter), (0, 7777));
    assert_eq!(z.tiles.len(), 12);
    // Tile 6 is column 2 of row 1: elevation 3, shape 4, terrain 6, edges 6.
    let t = z.tile(2, 1).unwrap();
    assert_eq!((t.elevation, t.shape, t.terrain, t.edges), (3, 4, 6, 6));
    assert_eq!(z.tile(3, 2).unwrap().elevation, 8);
    let want = View {
        zoom: -2,
        rotation: 3,
        scroll: (1216, -16),
        brush: 5,
    };
    assert_eq!(z.view, want);
    assert!(z.tile(4, 0).is_none() && z.tile(0, 3).is_none());
    // A map without tiles is fine.
    assert!(parse(&file(70, 0, 5, &[], &[])).tiles.is_empty());
}

#[test]
fn skips_habitats_and_shows_of_every_version() {
    let habitats = [(false, false), (true, false), (true, true), (false, true)];
    for v in VERSIONS {
        let z = parse(&file(v, 2, 2, &habitats, &[]));
        assert_eq!(
            (z.habitat_tiles.len(), z.id_counter, z.tiles.len()),
            (4, 7777, 4),
            "version {v}"
        );
        assert_eq!(z.habitat_gates, vec![((-1, -1), 6); 4], "version {v}");
        // The tank fields: height 9 and level 6; salt water of purity 100 before version 100.
        let tanks: Vec<_> = z.tanks.iter().map(|t| (t.height, t.level)).collect();
        assert_eq!(tanks, if v >= 85 { vec![(9, 6); 2] } else { vec![] });
        if v >= 85 {
            let t = z.tanks[0];
            assert_eq!((t.tile, t.gate), ((13, 45), (-1, -1)), "version {v}");
            let (salt, purity) = if v >= 100 { (true, 77) } else { (true, 100) };
            assert_eq!((t.salt, t.purity), (salt, purity), "version {v}");
        }
    }
}

#[test]
fn skips_the_sections_after_the_entities_of_every_version() {
    for v in VERSIONS {
        let bytes = file(v, 2, 2, &[], &[("fences", "zoowall", "f", fence(v, 5))]);
        assert_eq!(parse(&bytes).entities.len(), 1, "version {v}");
        // Bytes after the last section are ignored.
        let mut more = bytes.clone();
        more.extend([1, 2, 3]);
        assert_eq!(parse(&more), parse(&bytes), "version {v}");
        more.truncate(bytes.len() - 1);
        assert!(refusal(&more).starts_with("corrupted:"), "version {v}");
    }
}

/// The offset of the first byte of `needle` in the sections after the entities.
fn in_tail(bytes: &[u8], v: u32, needle: &[u8]) -> usize {
    let start = bytes.len() - tail_len(v);
    start
        + bytes[start..]
            .windows(needle.len())
            .position(|w| w == needle)
            .unwrap()
}

#[test]
fn the_finance_table_size_does_not_overflow() {
    let good = file(106, 1, 1, &[], &[]);
    let mut counts = W::default();
    counts.u32(2).u32(3);
    let at = in_tail(&good, 106, &counts.0);
    for (months, columns) in [
        (u32::MAX, u32::MAX),
        (u32::MAX, 3),
        (2, u32::MAX),
        (1 << 30, 1 << 30),
    ] {
        let mut bad = good.clone();
        bad[at..at + 4].copy_from_slice(&months.to_le_bytes());
        bad[at + 4..at + 8].copy_from_slice(&columns.to_le_bytes());
        assert_eq!(
            refusal(&bad),
            "corrupted: later sections: finance tables pass the end of the file"
        );
    }
}

#[test]
fn the_scenario_block_must_match_its_content() {
    for v in [51, 70, 106] {
        let good = file(v, 1, 1, &[], &[]);
        // The size, the name (5 bytes) and the goal count (8 bytes) of the block.
        let mut block = W::default();
        block.u32(4 + 5 + 4 + 8).str("scn/x").u32(8);
        let at = in_tail(&good, v, &block.0);
        let count = at + 4 + 4 + 5;
        // A count that is not a multiple of 4, with the size matching it.
        let mut bad = good.clone();
        bad[at..at + 4].copy_from_slice(&(4u32 + 5 + 4 + 6).to_le_bytes());
        bad[count..count + 4].copy_from_slice(&6u32.to_le_bytes());
        bad.drain(count + 4 + 6..count + 4 + 8);
        assert_eq!(
            refusal(&bad),
            "corrupted: later sections: scenario goal states are not a multiple of 4 bytes"
        );
        // A byte left in the block after the goal states.
        let mut bad = good.clone();
        bad[at..at + 4].copy_from_slice(&(4u32 + 5 + 4 + 8 + 1).to_le_bytes());
        bad.insert(count + 4 + 8, 0);
        assert_eq!(
            refusal(&bad),
            "corrupted: later sections: scenario block size does not match its content"
        );
        // A size smaller than the content.
        let mut bad = good.clone();
        bad[at..at + 4].copy_from_slice(&(4u32 + 5 + 4 + 4).to_le_bytes());
        assert!(refusal(&bad).starts_with("corrupted: later sections:"));
        // A name that passes the end of the file.
        let mut bad = good.clone();
        bad[at + 4..at + 8].copy_from_slice(&u32::MAX.to_le_bytes());
        assert!(refusal(&bad).starts_with("corrupted: later sections:"));
    }
}

#[test]
fn reads_the_common_part_of_fixed_entities() {
    for v in [70, 106] {
        let mut building = W::default();
        building.z(4);
        base(&mut building, v, 4, 9, "House");
        building.z(16).u32(1).z(8).z(4 + 16 + 8);
        building.z(4); // from 68
        if v >= 74 {
            building.z(4).u32(1).z(4); // from 74, from 75
        }
        let mut ambient = W::default();
        ambient.z(8);
        base(&mut ambient, v, 0, 10, "Bird");
        let mut filter = scenery(v, 11, "Filter");
        filter.extend(vec![7; if v >= 95 { 4 + 12 + 16 + 4 } else { 4 }]);
        let mut rubble = scenery(v, 12, "Rubble");
        rubble.extend([1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0]);
        let mut tankwall = fence(v, 13);
        if v >= 101 {
            tankwall.extend([0; 2]);
        }
        let ents = [
            ("objects", "tree", "oak", scenery(v, 6, "Tree")),
            ("food", "food", "meat", scenery(v, 7, "Meat")),
            ("paths", "paths", "path", scenery(v, 8, "Path")),
            ("building", "building", "shop", building.0),
            ("ambient", "ambient", "bird", ambient.0),
            ("filter", "filter", "f", filter),
            ("rubble", "rubble", "r", rubble),
            ("fences", "zoowall", "f", fence(v, 14)),
            ("tankwall", "glass", "g", tankwall),
        ];
        let z = parse(&file(v, 1, 1, &[], &ents));
        assert_eq!(z.entities.len(), ents.len());
        for (e, (class, ty, sub, body)) in z.entities.iter().zip(&ents) {
            assert_eq!((&*e.class, &*e.type_name, &*e.subtype), (*class, *ty, *sub));
            assert_eq!(e.body_size, Some(body.len() as u32));
            let c = e
                .common
                .as_ref()
                .unwrap_or_else(|| panic!("{class} at {v}"));
            assert_eq!((c.x, c.y, c.z), (100, -5, 32));
            assert_eq!(c.colors, [3, 9], "{class} at {v}");
        }
        let c = z.entities[3].common.as_ref().unwrap();
        assert_eq!((c.rotation, c.id, &*c.name), (4, 9, "House"));
        // The words after the health are upkeep and dates, never timers (zoo format 6.8).
        let f = z.entities[5].common.as_ref().unwrap().filter.unwrap();
        assert_eq!((f.health, f.timers), (0x0707_0707, None), "filter at {v}");
    }
}

#[test]
fn keeps_the_food_units_of_scenery() {
    // Kept as stored (signed) for the class `food` and the dung subtypes; zeroed for the rest.
    for v in [48, 106] {
        for (class, sub, units, kept) in [
            ("food", "meat", 1000i32, 1000),
            ("food", "meat", -7, -7),
            ("objects", "poo", 2, 2),
            ("objects", "dinopoo", 1, 1),
            ("objects", "tree", 5, 0),
            ("paths", "path", 1000, 0),
        ] {
            let mut body = scenery(v, 7, "Meat");
            body[..4].copy_from_slice(&units.to_le_bytes());
            let z = parse(&file(v, 1, 1, &[], &[(class, "t", sub, body)]));
            let c = z.entities[0].common.as_ref().unwrap();
            assert_eq!((c.food, c.id), (kept, 7), "{class} {sub} at {v}");
        }
    }
    // Everything that is not scenery holds none.
    let z = parse(&file(
        106,
        1,
        1,
        &[],
        &[("ambient", "ambient", "bird", {
            let mut w = W::default();
            w.z(8);
            base(&mut w, 106, 0, 10, "Bird");
            w.0
        })],
    ));
    assert_eq!(z.entities[0].common.as_ref().unwrap().food, 0);
}

#[test]
fn keeps_the_condition_of_fences_from_version_55() {
    for v in [48, 55, 106] {
        let mut body = scenery(v, 3, "Fence");
        if v >= 55 {
            body.extend([7, 0, 0, 0, 0xff, 0xff, 0xff, 0xff, 5, 0, 0, 0, 6, 0, 0, 0]);
        }
        let z = parse(&file(v, 1, 1, &[], &[("fences", "zoowall", "f", body)]));
        let fence = z.entities[0].common.as_ref().unwrap().fence;
        match v {
            55.. => {
                let fence = fence.unwrap();
                assert_eq!((fence.strength, fence.life), (7, -1));
                assert_eq!(fence.timers, [5, 6]);
            }
            _ => assert_eq!(fence, None),
        }
    }
}

#[test]
fn reads_units_of_version_106() {
    // Hand-counted bytes before the movement block: the fields of the class, the unit block.
    let animal = unit(106, 387, 21);
    // The five status values lead the guest's own fields.
    let mut guest = W::default();
    for v in [36, 52, 60, 70, 80] {
        guest.i32(v);
    }
    // Then the sick flag, the increment factors, female and child, the ten countdowns of the block, the 20 bytes of the lists and the chase
    // countdown, which comes after them.
    guest.0.push(1);
    guest.z(20);
    guest.i32(1);
    guest.i32(0);
    for c in [1, 2, 3, 4, 5, 6, 8, 9, 10, 11] {
        guest.i32(c);
    }
    guest.z(20);
    guest.i32(7);
    // Then two flags, the saw-animal countdown, two flags, the arrival date and the favourite.
    guest.z(2);
    guest.i32(33);
    guest.z(2);
    guest.i32(7);
    guest.i32(2);
    guest.i32(5003);
    guest
        .0
        .extend(unit(106, 310 - 20 - 29 - 40 - 20 - 4 - 20, 22));
    let guest = guest.0;
    // Own fields of each staff class, the common staff block (16 bytes), the unit block (161).
    let staff = |own: usize, id| unit(106, own + 16 + 161, id);
    let ents = [
        ("animals", "polarbr", "m", animal),
        ("guests", "man", "", guest),
        ("keeper", "keeper", "f", staff(21, 23)),
        ("maint", "maint", "m", staff(18, 24)),
        ("tour", "tour", "m", staff(13, 25)),
        ("helicopter", "heli", "m", staff(12, 26)),
        ("staff", "vet", "m", staff(0, 27)),
    ];
    let z = parse(&file(106, 1, 1, &[], &ents));
    for (e, id) in z.entities.iter().zip(21..) {
        let c = e.common.as_ref().unwrap_or_else(|| panic!("{}", e.class));
        assert_eq!((c.id, c.rotation, &*c.name), (id, 6, "unit"), "{}", e.class);
    }
    let guest = z.entities[1].common.as_ref().unwrap();
    let record = guest.guest.as_deref().unwrap();
    assert_eq!(record.values, [36, 52, 60, 70, 80]);
    assert_eq!(record.countdowns, [1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11]);
    assert_eq!(record.favourite, 5003);
    assert_eq!((record.arrived, record.sick), (7 | 2 << 32, true));
    assert_eq!(
        (record.saw_animal, record.female, record.child),
        (33, true, false)
    );
    assert_eq!(z.entities[0].common.as_ref().unwrap().guest, None);
}

#[test]
fn reads_units_of_version_60_without_sizes() {
    let v = 60;
    // The animal fields and the unit block add up to 326 bytes, then comes the common part.
    let mut animal = W::default();
    animal.z(326);
    base(&mut animal, v, 3, 31, "Lion");
    let mut building = W::default();
    building.z(4);
    base(&mut building, v, 2, 32, "Shop");
    building.z(16 + 4 + 4 + 16 + 8 + 4); // the fields; the colour list is a count from 48
    let ents = [
        ("fences", "zoowall", "f", fence(v, 30)),
        ("animals", "lion", "m", animal.0),
        ("building", "building", "shop", building.0),
        ("objects", "tree", "oak", scenery(v, 33, "Tree")),
    ];
    let z = parse(&file(v, 1, 1, &[], &ents));
    let ids: Vec<u32> = z
        .entities
        .iter()
        .map(|e| e.common.as_ref().unwrap().id)
        .collect();
    assert_eq!(ids, [30, 31, 32, 33]);
    assert!(z.entities.iter().all(|e| e.body_size.is_none()));
}

#[test]
fn unknown_classes_are_skipped_by_size_or_refused() {
    let odd = vec![1, 2, 3, 4, 5];
    let ents = [
        ("objects", "tree", "oak", scenery(70, 1, "Tree")),
        ("mystery", "x", "y", odd.clone()),
        ("objects", "tree", "oak", scenery(70, 2, "Tree")),
    ];
    let z = parse(&file(70, 1, 1, &[], &ents));
    assert_eq!(z.entities.len(), 3);
    assert_eq!(z.entities[1].common, None);
    assert_eq!(z.entities[1].body_size, Some(5));
    assert_eq!(z.entities[2].common.as_ref().unwrap().id, 2);
    // Without sizes the next record cannot be found.
    let ents = [("mystery", "x", "y", odd)];
    assert_eq!(
        refusal(&file(69, 1, 1, &[], &ents)),
        "corrupted: entities: entity 0: unknown class in a file without body sizes"
    );
}

#[test]
fn a_type_without_definition_refuses_a_file_without_body_sizes() {
    let known = |e: &Entity| e.type_name == "tree";
    for (v, refused) in [
        (45, true),
        (55, true),
        (69, true),
        (70, false),
        (106, false),
    ] {
        let ents = [
            ("objects", "tree", "oak", scenery(v, 1, "Tree")),
            ("objects", "lost", "oak", scenery(v, 2, "Lost")),
        ];
        let z = parse(&file(v, 1, 1, &[], &ents));
        // The class is known and the body decodes; only the type is missing.
        assert_eq!(z.check_types(known).is_err(), refused, "version {v}");
        assert!(z.check_types(|_| true).is_ok(), "version {v}");
        // The entity of the missing type can be dropped, with its record.
        if !refused {
            let (mut z, money) = Zoo::parse_money(&file(v, 1, 1, &[], &ents)).unwrap();
            z.drop_unknown(known);
            assert_eq!(z.entities.len(), 1, "version {v}");
            assert_eq!(
                Zoo::parse(&z.write(&money).unwrap())
                    .unwrap()
                    .entities
                    .len(),
                1
            );
        }
    }
    let ents = [("objects", "lost", "oak", scenery(55, 1, "Lost"))];
    assert_eq!(
        parse(&file(55, 1, 1, &[], &ents)).check_types(known),
        Err(ZooError::Corrupted(
            "entities: entity 0: no definition of its type in a file without body sizes".into()
        ))
    );
}

#[test]
fn bodies_of_size_zero_and_of_the_wrong_size_are_skipped() {
    let mut long = scenery(106, 3, "Long");
    long.push(0);
    let short = scenery(106, 4, "Short")[..20].to_vec();
    let ents = [
        ("objects", "other", "tankripl", vec![]),
        ("objects", "tree", "oak", long),
        ("objects", "tree", "oak", short),
        ("objects", "tree", "oak", scenery(106, 5, "Ok")),
    ];
    let z = parse(&file(106, 1, 1, &[], &ents));
    let found: Vec<Option<u32>> = z
        .entities
        .iter()
        .map(|e| e.common.as_ref().map(|c| c.id))
        .collect();
    assert_eq!(found, [None, None, None, Some(5)]);
}

#[test]
fn reads_nested_behaviour_sets() {
    for children in [0, 1, 5, 200] {
        let mut body = W::default();
        body.z(4).i32(1).i32(2).i32(3).u32(0).u32(77).str("n");
        body.z(5 + 1);
        behaviour(&mut body, children);
        body.u32(0);
        let ents = [("objects", "tree", "oak", body.0)];
        let z = parse(&file(106, 1, 1, &[], &ents));
        assert_eq!(
            z.entities[0].common.as_ref().unwrap().id,
            77,
            "{children} children"
        );
    }
}

#[test]
fn strings_of_4096_bytes_or_more_are_refused() {
    let mut body = W::default();
    body.z(4).z(12).u32(0).u32(1).str(&"n".repeat(4095));
    body.z(6);
    behaviour(&mut body, 0);
    body.u32(0);
    let z = parse(&file(
        106,
        1,
        1,
        &[],
        &[("objects", "t", "s", body.0.clone())],
    ));
    assert_eq!(z.entities[0].common.as_ref().unwrap().name.len(), 4095);
    // The same with the name one byte longer, and the length field patched to match.
    let mut longer = file(106, 1, 1, &[], &[("objects", "t", "s", body.0)]);
    let at = longer
        .windows(4095)
        .position(|w| w.iter().all(|&b| b == b'n'))
        .unwrap();
    longer[at - 4..at].copy_from_slice(&4096u32.to_le_bytes());
    longer.insert(at, b'n');
    assert!(refusal(&longer).starts_with("corrupted:"));
}

#[test]
fn the_awards_section_is_the_held_list_written_largest_first() {
    let (zoo, money) = Zoo::parse_money(&file(106, 2, 2, &[], &[])).unwrap();
    assert_eq!(zoo.awards, [0], "the file's one award");
    assert!(
        parse(&file(58, 2, 2, &[], &[])).awards.is_empty(),
        "no section before 59"
    );
    let mut zoo = zoo;
    zoo.awards = vec![8, 1];
    let out = zoo.write(&money).unwrap();
    assert_eq!(parse(&out).awards, [8, 1]);
    // The ids are written as given; a load keeps each once, largest first.
    zoo.awards = vec![1, 8, 1, -3];
    let back = parse(&zoo.write(&money).unwrap());
    assert_eq!(back.awards, [8, 1, -3]);
    assert_eq!(
        back.kept.research,
        parse(&file(106, 2, 2, &[], &[])).kept.research
    );
}

#[test]
fn research_and_movement_with_unknown_types_are_refused() {
    let mut bytes = file(106, 1, 1, &[], &[]);
    // The type of the last research record, counted back from the end: the terminator, the
    // record (type, id, value), then awards (8), marketing (4), cheats (13) and the tail (8).
    let two = bytes.len() - (8 + 4 + 13 + 8) - 4 - 12;
    assert_eq!(&bytes[two..two + 4], 2i32.to_le_bytes());
    bytes[two] = 3;
    assert_eq!(
        refusal(&bytes),
        "corrupted: later sections: research record of an unknown type"
    );

    // A movement type of 0 or above 3 breaks the body, which is then skipped by its size.
    let at = 387 + 4;
    let kind = |kind: i32| {
        let mut body = unit(106, 387, 1);
        body[at..at + 4].copy_from_slice(&kind.to_le_bytes());
        let z = parse(&file(106, 1, 1, &[], &[("animals", "a", "m", body)]));
        z.entities[0].common.is_some()
    };
    for bad in [0, 4, i32::MAX] {
        assert!(!kind(bad), "type {bad}");
    }
    // Types 1 and 2 have no data, and neither has any negative type.
    for good in [1, 2, -1, -2, i32::MIN] {
        assert!(kind(good), "type {good}");
    }
}

#[test]
fn flags_that_are_not_exactly_1_read_nothing_more() {
    // The escape flag of an animal.
    let mut animal = W::default();
    animal.z(326);
    base(&mut animal, 60, 3, 31, "Lion");
    for flag in [2, u8::MAX] {
        let mut animal = animal.0.clone();
        animal[179] = flag;
        let z = parse(&file(60, 1, 1, &[], &[("animals", "lion", "m", animal)]));
        assert_eq!(z.entities[0].common.as_ref().unwrap().id, 31);
    }
    // The escape flag and the two optional flags of the unit block.
    for at in [179, 278, 279] {
        let flagged = |flag| {
            let mut body = unit(106, 387, 21);
            body[at] = flag;
            let z = parse(&file(106, 1, 1, &[], &[("animals", "a", "m", body)]));
            z.entities[0].common.is_some()
        };
        assert!(flagged(2), "byte {at}");
        assert!(flagged(0), "byte {at}");
    }
}

#[test]
fn the_rotation_is_taken_modulo_8() {
    let mut body = W::default();
    body.z(4);
    base(&mut body, 106, 13, 5, "r");
    let z = parse(&file(106, 1, 1, &[], &[("objects", "t", "s", body.0)]));
    assert_eq!(z.entities[0].common.as_ref().unwrap().rotation, 5);
}

#[test]
fn a_file_of_version_38_with_the_old_cash_layout_is_corrupted() {
    // The leftover files of version 38 have no cash field after the calendar: four bytes short.
    let bytes = file(38, 2, 2, &[], &[("fences", "zoowall", "f", fence(38, 5))]);
    assert!(Zoo::parse(&bytes).is_ok());
    // Without the cash and with no thoughts, the file ends with a count and the events (8):
    // the loader reads the count as the cash and then runs past the end.
    let cash = bytes.len() - 52 - 4;
    let old = [&bytes[..cash], &[0; 12]].concat();
    assert!(refusal(&old).starts_with("corrupted: later sections:"));
}

#[test]
fn huge_counts_do_not_allocate_or_loop() {
    // A map of 4 billion by 4 billion tiles.
    let mut w = W::default();
    header(&mut w, 106, 1033);
    w.u32(u32::MAX).u32(u32::MAX).z(8).u32(0).z(4);
    shows(&mut w, 106, 1);
    w.z(4);
    assert_eq!(
        refusal(&w.0),
        "corrupted: tiles: the map is bigger than the file"
    );
    // The habitat count, the script count, the entity count and the research list.
    let good = file(106, 1, 1, &[], &[]);
    let counts = [
        (16 + 16, "habitats"),
        (16 + 16 + 4 + 4, "shows"),
        (good.len() - tail_len(106) - 4, "entities"),
    ];
    for (at, section) in counts {
        let mut bad = good.clone();
        bad[at..at + 4].copy_from_slice(&u32::MAX.to_le_bytes());
        assert_eq!(
            refusal(&bad),
            format!("corrupted: {section}: count passes the end of the file")
        );
    }
}

fn tail_len(v: u32) -> usize {
    let mut w = W::default();
    tail(&mut w, v, 1);
    w.0.len()
}

/// A tiny xorshift generator, so the fuzz loop is the same on every run.
fn next(state: &mut u64) -> u64 {
    *state ^= *state << 13;
    *state ^= *state >> 7;
    *state ^= *state << 17;
    *state
}

fn rich(v: u32) -> Vec<u8> {
    rich_with(v, true)
}

/// [`rich`] without a show, which the writer cannot upgrade below 106.
fn lean(v: u32) -> Vec<u8> {
    rich_with(v, false)
}

fn rich_with(v: u32, show: bool) -> Vec<u8> {
    let ents = [
        ("fences", "zoowall", "f", fence(v, 1)),
        (
            "animals",
            "lion",
            "m",
            unit(v, if v >= 70 { 387 } else { 326 }, 2),
        ),
        (
            "objects",
            "other",
            "tankripl",
            if v >= 70 { vec![] } else { scenery(v, 3, "R") },
        ),
        ("mystery", "x", "y", vec![7; 9]),
    ];
    let ents: Vec<_> = if v >= 70 {
        ents.to_vec()
    } else {
        ents[..1].to_vec()
    };
    let habitats = [(true, show), (false, false)];
    file_with(v, 3, 3, &habitats, &ents, show.into())
}

#[test]
fn hostile_input_never_panics() {
    let mut state = 0x9e3779b97f4a7c15;
    for v in [38, 45, 60, 70, 84, 97, 101, 106] {
        let good = rich(v);
        let _ = Zoo::parse(&good);
        for len in 0..good.len() {
            let _ = Zoo::parse(&good[..len]);
        }
        for at in 0..good.len() {
            for value in [0x00, 0x01, 0x7f, 0x80, 0xff] {
                let mut bad = good.clone();
                bad[at] = value;
                let _ = Zoo::parse(&bad);
            }
        }
        // Two adjacent extreme values, to reach products of counts.
        for at in 0..good.len().saturating_sub(8) {
            let mut bad = good.clone();
            bad[at..at + 8].fill(0xff);
            let _ = Zoo::parse(&bad);
        }
        // Extreme counts and sizes at every position, aligned or not.
        for at in 0..good.len().saturating_sub(4) {
            for value in [u32::MAX, i32::MAX as u32, 4096, 4095, 1 << 24, 0x1000_0000] {
                let mut bad = good.clone();
                bad[at..at + 4].copy_from_slice(&value.to_le_bytes());
                let _ = Zoo::parse(&bad);
            }
        }
        for _ in 0..300 {
            let mut bad = good.clone();
            for _ in 0..1 + next(&mut state) % 6 {
                let at = next(&mut state) as usize % bad.len();
                bad[at] = next(&mut state) as u8;
            }
            let _ = Zoo::parse(&bad);
        }
    }
    for len in [0, 1, 7, 8, 12, 100] {
        let mut junk: Vec<u8> = b"TZFB\x6a\0\0\0".to_vec();
        junk.extend((0..len).map(|_| next(&mut state) as u8));
        let _ = Zoo::parse(&junk);
    }
}

#[test]
fn the_rich_files_parse() {
    for v in [38, 45, 60, 70, 84, 97, 101, 106] {
        let z = parse(&rich(v));
        assert_eq!((z.width, z.habitat_tiles.len()), (3, 2), "version {v}");
        assert!(z.entities[0].common.is_some());
    }
}

/// The facts below were measured on Zoo Tycoon: Complete Collection.
#[test]
#[ignore = "needs the game: set ZT_DATA_DIR to the install folder"]
fn full_game_facts() {
    let dir = std::env::var_os("ZT_DATA_DIR").expect("ZT_DATA_DIR is not set");
    let vfs = crate::vfs::Vfs::open(std::path::Path::new(&dir)).unwrap();
    let load = |member: &str| Zoo::parse(&vfs.read(member).unwrap());

    let map = load("maps/default.zoo").unwrap();
    assert_eq!(
        (map.version, map.locale, map.width, map.height),
        (70, 1033, 75, 75)
    );
    assert_eq!(
        (map.entrance, map.habitat_tiles.len(), map.id_counter),
        ((11, 42), 0, 14907)
    );
    assert_eq!((map.tiles.len(), map.entities.len()), (5625, 392));
    let first = map.tiles[0];
    assert_eq!(
        (first.elevation, first.shape, first.terrain, first.edges),
        (0, 0, 0, 0)
    );
    let fence = &map.entities[0];
    assert_eq!(
        (&*fence.class, &*fence.type_name, &*fence.subtype),
        ("fences", "zoowall", "f")
    );
    assert_eq!(fence.body_size, Some(98));
    let c = fence.common.as_ref().unwrap();
    assert_eq!((c.x, c.y, c.z, c.rotation, c.id), (767, 2848, 0, 2, 2));

    // The saved game exists in the English install only.
    if crate::pe::installed_language(&dir) == 1036 {
        eprintln!("skipped: saved games/test.zoo is not in the French install");
    } else {
        let save = load("saved games/test.zoo").unwrap();
        assert_eq!(
            (save.version, save.expansion, save.width, save.height),
            (106, 0, 125, 125)
        );
        assert_eq!(
            (save.entrance, save.habitat_tiles.len(), save.id_counter),
            ((12, 73), 6, 166520)
        );
        assert_eq!((save.tiles.len(), save.entities.len()), (15625, 1683));
        let first = save.tiles[0];
        assert_eq!((first.elevation, first.shape, first.terrain), (-2, 0, 10));
        let path = &save.entities[0];
        assert_eq!(
            (&*path.class, &*path.type_name, &*path.subtype),
            ("paths", "paths", "path")
        );
        assert_eq!(path.body_size, Some(84));
        let c = path.common.as_ref().unwrap();
        assert_eq!((c.x, c.y, c.z, c.rotation, c.id), (800, 4704, 0, 4, 37091));
    }

    // Every .zoo file: 96 parse (95 in the French install), 3 are older than version 38, 6 of version 38 are corrupted.
    // Two of the 96 (the leftovers of the scenario archive, versions 45 and 55) hold entities
    // of a type without definition, which the original cannot load before version 70.
    let catalogue = crate::entity::Catalogue::load(&vfs).unwrap();
    let bytes = |s: &str| s.chars().map(|c| c as u8).collect::<Vec<u8>>();
    let mut untyped = Vec::new();
    let (mut ok, mut old, mut corrupted, mut transient, mut skipped) = (0, 0, 0, 0, 0);
    for member in vfs.list(None).iter().filter(|m| m.ends_with(".zoo")) {
        match load(member) {
            Ok(zoo) => {
                let known = |e: &Entity| {
                    let (t, s) = (bytes(&e.type_name), bytes(&e.subtype));
                    catalogue.find(&e.class, &t, &s).is_some()
                };
                if zoo.check_types(known).is_err() {
                    untyped.push((member.clone(), zoo.version));
                }
                ok += 1;
                transient += zoo
                    .entities
                    .iter()
                    .filter(|e| e.body_size == Some(0))
                    .count();
                skipped += zoo.entities.iter().filter(|e| e.common.is_none()).count();
            }
            Err(ZooError::TooOld(_)) => old += 1,
            Err(ZooError::Corrupted(_)) => corrupted += 1,
            Err(e) => panic!("{member}: {e}"),
        }
    }
    // The English install also holds the saved game.
    let (parsed, french) = (96, crate::pe::installed_language(&dir) == 1036);
    assert_eq!((ok, old, corrupted), (parsed - usize::from(french), 3, 6));
    untyped.sort();
    assert_eq!(
        untyped,
        [
            ("scenario/scn08/scn08.zoo".to_string(), 55),
            ("scenario/scn15/scn15.zoo".to_string(), 45)
        ]
    );
    // The only bodies not decoded are the 939 of size 0 (730 without the saved game).
    let empty = if french { 730 } else { 939 };
    assert_eq!((transient, skipped), (empty, empty));
}

#[test]
fn reads_the_values_counters_and_flags_of_an_animal_record() {
    let mut w = W::default();
    w.i32(38).i32(7).i32(250).i32(15);
    w.u8(1).u8(0).u8(0).u8(1).u8(0).z(25);
    w.str("Female").str("x");
    (10..21).for_each(|n| _ = w.u32(n));
    w.z(4).u32(77).u32(33).u8(1).z(2).u32(55).z(36).z(8);
    w.z(12).u8(1).u8(1).u32(88).u32(0);
    w.z(9).u8(1).u8(1).z(1).u8(1).u32(5).u32(12).z(1);
    w.u8(0).z(4).u32(20).u32(99).z(8);
    let before = 387 + "Female".len() + "x".len() - w.0.len();
    let mut body = w.0;
    body.extend(unit(106, before, 21));
    let z = parse(&file(106, 1, 1, &[], &[("animals", "a", "f", body)]));
    let c = z.entities[0].common.as_ref().unwrap();
    assert_eq!(c.id, 21);
    let r = c.animal.as_ref().unwrap();
    assert_eq!(
        (r.happiness, r.hunger, r.health, r.energy),
        (38, 7, 250, 15)
    );
    assert_eq!((r.dung, r.female), (33, true));
    // Hunger, breath, health, energy, escape, other, keeper, captivity, habitat, bored, social,
    // reproduction, message, shelter, water, murky water.
    let counters = [
        10, 99, 11, 12, 55, 13, 14, 15, 16, 17, 18, 19, 77, -1, 88, -1,
    ];
    let got = r.counters.map(|c| c.unwrap_or(-1));
    assert_eq!(got, counters);
    assert_eq!(
        [r.at_minimum_health, r.very_unhappy, r.keeper_visited],
        [true; 3]
    );
    assert_eq!(
        [r.mourning, r.dying, r.crated, r.egg],
        [true, true, true, false]
    );
    assert_eq!((r.thirsty, r.rampaging, r.rampage), (true, true, 12));
    assert_eq!(r.breath, Some(20));
    // Other classes keep no record.
    let z = parse(&file(
        70,
        1,
        1,
        &[],
        &[("objects", "tree", "oak", scenery(70, 1, "T"))],
    ));
    assert_eq!(z.entities[0].common.as_ref().unwrap().animal, None);
}

#[test]
fn reads_the_figures_of_a_building() {
    let bits = |v: f32| v.to_bits();
    // Users now, guests since construction, income since construction, the price, the users
    // (one pair), the fill, income last and this month, guests last and this month, the date.
    for v in [66, 67, 68, 70, 74, 106] {
        let mut b = W::default();
        b.z(4);
        base(&mut b, v, 4, 9, "House");
        b.u32(2).u32(40).u32(bits(182.5)).u32(bits(3.5));
        b.u32(1).u32(77).u32(2);
        b.u32(14).u32(bits(105.0)).u32(bits(77.25)).u32(15).u32(12);
        b.u32(0x1234_5678).u32(0x01d0_0000);
        if (48..=66).contains(&v) {
            b.u32(0);
        } else if v < 48 {
            b.z(2);
        }
        if v >= 68 {
            b.u32(bits(100.0));
        }
        if v >= 74 {
            b.z(4);
        }
        if v >= 75 {
            b.u32(0);
        }
        // The stamp of the entity base follows the food word, the position, the rotation, the id,
        // the name and the visible flag.
        b.0[34..38].copy_from_slice(&42u32.to_le_bytes());
        let z = parse(&file(
            v,
            1,
            1,
            &[],
            &[("building", "building", "shop", b.0)],
        ));
        let c = z.entities[0].common.as_ref().unwrap();
        assert_eq!(c.stamp, Some(42), "version {v}");
        let r = c.building.as_ref().unwrap();
        assert_eq!(f32::from_bits(r.price), 3.5, "version {v}");
        assert_eq!(r.users, [(77, 2)], "version {v}");
        assert_eq!(r.guests, [12, 15, 40], "version {v}");
        let money = r.income.map(f32::from_bits);
        assert_eq!(money, [77.25, 105.0, 182.5], "version {v}");
        assert_eq!(r.fill, 14, "version {v}");
        assert_eq!(r.built, 0x01d0_0000_1234_5678, "version {v}");
        let upkeep = r.upkeep.map(f32::from_bits);
        assert_eq!(upkeep, (v >= 68).then_some(100.0), "version {v}");
    }
}

/// Reads `bytes` and checks that they end where the last section does.
fn parse_to_the_end(bytes: &[u8]) -> Zoo {
    let v = u32::from_le_bytes(bytes[4..8].try_into().unwrap());
    let mut r = Reader::new(&bytes[8..], v);
    let zoo = read(&mut r, &mut "", &mut Money::default()).unwrap_or_else(|e| panic!("{}", e.0));
    assert!(r.data.is_empty(), "{} bytes left", r.data.len());
    zoo
}

/// Reads `bytes`, writes them back and reads that.
fn again(bytes: &[u8]) -> (Zoo, Vec<u8>, Zoo) {
    let (zoo, money) = Zoo::parse_money(bytes).unwrap();
    let out = zoo.write(&money).unwrap();
    let back = parse_to_the_end(&out);
    assert_eq!(money, Zoo::parse_money(&out).unwrap().1);
    (zoo, out, back)
}

/// What a zoo read in `from` equals once written as 106: the version, the sizes and the bytes of the records.
fn upgraded(mut zoo: Zoo, back: &Zoo) -> Zoo {
    zoo.version = MAX_VERSION;
    for (e, b) in zoo.entities.iter_mut().zip(&back.entities) {
        e.body_size = b.body_size;
        // The bytes of an animal are those of its record, not of the file.
        if let (Some(a), Some(b)) = (&mut e.common, &b.common) {
            a.raw = b.raw.clone();
        }
    }
    zoo
}

const WRITTEN: [u32; 10] = [70, 71, 82, 84, 97, 101, 104, 105, 106, 83];

#[test]
fn a_file_is_written_as_version_106_and_reads_the_same() {
    for v in WRITTEN {
        let (zoo, out, back) = again(&lean(v));
        assert_eq!(back.version, 106, "version {v}");
        let (mut a, mut b) = (upgraded(zoo, &back), back.clone());
        (a.kept, b.kept) = (Kept::default(), Kept::default());
        let (a, b) = (format!("{a:?}"), format!("{b:?}"));
        let at = a
            .chars()
            .zip(b.chars())
            .position(|(x, y)| x != y)
            .unwrap_or(0);
        assert!(
            a == b,
            "version {v}: {} / {}",
            &a[at.saturating_sub(100)..at + 100],
            &b[at.saturating_sub(100)..at + 100]
        );
        // A second write changes nothing.
        let (again, money) = Zoo::parse_money(&out).unwrap();
        assert!(again.write(&money).unwrap() == out, "version {v}");
    }
}

#[test]
fn the_fields_of_the_model_are_written() {
    for v in [70, 106] {
        let (mut zoo, money) = Zoo::parse_money(&lean(v)).unwrap();
        zoo.id_counter = 77;
        zoo.tiles[4].elevation = -3;
        zoo.names.insert("lion".into(), 5);
        zoo.view.zoom = 2;
        zoo.clocks.ai_ticks = 9;
        zoo.clocks.world_secs = 11;
        zoo.clocks.status.rating = 42;
        zoo.clocks.status.price = 31.5;
        zoo.clocks.calendar = 123_456_789;
        let c = zoo.entities[0].common.as_mut().unwrap();
        (c.x, c.y, c.z, c.rotation, c.id) = (1, 2, 3, 6, 99);
        c.name = "A longer name".into();
        c.stamp = Some(55);
        c.colors = vec![1, 2, 3];
        let mut money = money;
        money.set_cash(1234.5);
        money.monthly[1][4] = 7.0;
        money.monthly[30][11] = 8.0;
        money.total[1] = 8.0;
        let back = parse_to_the_end(&zoo.write(&money).unwrap());
        let money_back = Zoo::parse_money(&zoo.write(&money).unwrap()).unwrap().1;
        assert_eq!(back, upgraded(zoo, &back), "version {v}");
        assert_eq!(money_back, money, "version {v}");
        assert_eq!(
            back.entities[0].common.as_ref().unwrap().name,
            "A longer name"
        );
    }
}

#[test]
fn the_writer_puts_names_thoughts_and_habitat_figures_over_the_file() {
    for v in [70, 82, 83, 106] {
        let bytes = file(v, 3, 3, &[(false, false), (true, false)], &[]);
        let (mut zoo, money) = Zoo::parse_money(&bytes).unwrap();
        let out = zoo.write(&money).unwrap();
        assert_eq!(parse_to_the_end(&out).version, 106, "version {v}");
        assert_eq!(zoo.thoughts(), [[0; 5]; 2]);
        // A counter at 0 is not written; the thoughts and the figures are the live ones.
        zoo.names.insert("z".into(), 0);
        zoo.set_thoughts(vec![[1, 2, 3, 4, 5], [6, 7, 8, u32::MAX, u32::MAX]]);
        zoo.habitat_info[1].stats = [1, 2, 3, 4, 5, 6, 7, 8, 9];
        zoo.habitat_info[1].serviced = 55;
        // A name of another length moves the figures after it.
        zoo.habitat_info[1].name = "Lions 12".into();
        let (back, _) = Zoo::parse_money(&zoo.write(&money).unwrap()).unwrap();
        assert_eq!(back.habitat_info[1].name, "Lions 12", "version {v}");
        assert_eq!(back.names.len(), 2, "version {v}");
        assert_eq!(back.thoughts(), zoo.thoughts(), "version {v}");
        assert_eq!(back.habitat_info[0], zoo.habitat_info[0], "version {v}");
        assert_eq!(back.habitat_info[1].stats, [1, 2, 3, 4, 5, 6, 7, 8, 9]);
        assert_eq!(back.habitat_info[1].serviced, if v >= 83 { 55 } else { 0 });
        assert_eq!(back.tanks, zoo.tanks, "version {v}");
        assert_eq!(back.id_counter, zoo.id_counter, "version {v}");
        assert_eq!(back.clocks, zoo.clocks, "version {v}");
    }
}

#[test]
fn the_game_block_of_an_old_file_gets_the_rows_and_words_it_lacked() {
    for v in [70, 71, 73, 106] {
        let back = again(&lean(v)).2;
        let (f, at) = (&back.kept.finances, back.kept.at);
        // 31 rows and 20 columns, whatever the file had.
        assert_eq!(
            f[45..53],
            [31u32.to_le_bytes(), 20u32.to_le_bytes()].concat()
        );
        // The date after the price is 1 January 2000 when the file has none (before 71).
        let date = u64::from_le_bytes(f[at - 8..at].try_into().unwrap());
        assert_eq!(
            date,
            if v < 71 { 125_911_584_000_000_000 } else { 0 },
            "version {v}"
        );
        assert_eq!(f.len(), at + 24, "version {v}");
    }
}

#[test]
fn an_entity_without_a_body_is_not_written_unless_the_model_builds_it() {
    // A class the writer has no fields for.
    let (mut zoo, money) = Zoo::parse_money(&lean(106)).unwrap();
    let mut mystery = zoo.entities[0].clone();
    mystery.class = "mystery".into();
    mystery.common.as_mut().unwrap().raw = Raw::default();
    zoo.entities.push(mystery);
    let at = zoo.entities.len() - 1;
    let why = zoo.write(&money).unwrap_err();
    assert!(
        why.starts_with(&format!("entity {at}: mystery/zoowall")),
        "{why}"
    );
    let (mut zoo, money) = Zoo::parse_money(&lean(106)).unwrap();
    zoo.entities[0].common.as_mut().unwrap().name = "x".repeat(4096);
    assert!(zoo.write(&money).is_err());
    zoo.version = 107;
    assert!(zoo.write(&money).is_err());
    let blank = Zoo {
        kept: Kept::default(),
        ..Zoo::parse(&lean(106)).unwrap()
    };
    assert!(blank.write(&money).is_err());
}

/// An entity made in play: no body was read for it.
fn made(class: &str, subtype: &str, common: Common) -> Entity {
    Entity {
        class: class.into(),
        type_name: "t".into(),
        subtype: subtype.into(),
        body_size: None,
        common: Some(common),
    }
}

fn new_animal() -> AnimalRecord {
    let mut counters = [None; 16];
    for (i, at) in [0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 14]
        .iter()
        .enumerate()
    {
        counters[*at] = Some(i as i32 + 1);
    }
    AnimalRecord {
        happiness: 71,
        hunger: 12,
        health: 88,
        energy: 33,
        dung: 5,
        female: true,
        counters,
        breath: Some(44),
        second: "x".into(),
        at_minimum_health: true,
        sleeping: true,
        eating: true,
        very_unhappy: true,
        keeper_visited: true,
        mourning: true,
        dying: true,
        crated: true,
        egg: true,
        thirsty: true,
        drinking: true,
        rampaging: true,
        rampage: 9,
        dates: [0x01d0_0000_1234_5678, 0x01d0_0000_8765_4321],
        escape: Some(Escape {
            tiles: [[1, 2], [-3, 4]],
            words: [5, 6, 7],
            last: 8,
        }),
        life: Some(4319),
        baby: 999,
        interval: 1600,
        mate_delay: Some(2),
        boxed: 1,
        home: [30, 40],
        spare: (1..=74).collect(),
    }
}

fn new_guest() -> GuestRecord {
    GuestRecord {
        values: [1, -2, 3, 4, 5],
        countdowns: [10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20],
        arrived: 0x01d0_0000_1234_5678,
        favourite: 7,
        sick: true,
        saw_animal: 9,
        female: true,
        child: true,
        items: [0, 6491, 0, 6319, 6310],
        leave: true,
        afraid: true,
        gift: true,
        wandered: true,
        stand: true,
        in_group: true,
        listening: true,
        guide: 77,
    }
}

#[test]
fn an_entity_made_in_play_is_written_and_read_back() {
    let fence = Fence {
        strength: 30,
        life: -20,
        timers: [1, 2],
        portal: false,
    };
    let record = BuildingRecord {
        guests: [1, 2, 3],
        income: [4, 5, 6],
        upkeep: Some(7),
        price: 8,
        users: vec![(9, 1)],
        users_now: 2,
        fill: 10,
        built: 11,
    };
    let at = Common {
        x: 96,
        y: 160,
        z: 16,
        rotation: 3,
        name: "New".into(),
        colors: vec![5, 6],
        ..Common::default()
    };
    let with = |f: &dyn Fn(&mut Common)| {
        let mut c = at.clone();
        f(&mut c);
        c
    };
    let made = [
        made("objects", "bush", at.clone()),
        made("objects", "poo", with(&|c| c.food = 4)),
        made("objects", "bush", with(&|c| c.food = 4)),
        made("food", "meat", with(&|c| c.food = -3)),
        made("paths", "path", at.clone()),
        made("ambient", "bird", at.clone()),
        made("rubble", "r", at.clone()),
        made("fences", "f", with(&|c| c.fence = Some(fence))),
        made(
            "tankwall",
            "f",
            with(&|c| {
                c.fence = Some(Fence {
                    portal: true,
                    ..fence
                })
            }),
        ),
        made(
            "filter",
            "f",
            with(&|c| {
                c.filter = Some(Filter {
                    health: 55,
                    timers: None,
                })
            }),
        ),
        made(
            "building",
            "shop",
            with(&|c| c.building = Some(record.clone())),
        ),
        made(
            "animals",
            "m",
            with(&|c| c.animal = Some(Box::new(new_animal()))),
        ),
        made(
            "guests",
            "",
            with(&|c| c.guest = Some(Box::new(new_guest()))),
        ),
        made("keeper", "k", at.clone()),
        made("maint", "k", at.clone()),
        made("tour", "k", at.clone()),
        made("helicopter", "k", at.clone()),
        made("staff", "k", at.clone()),
        // A new unit without a record is written with the values of a new one.
        made("animals", "m", at.clone()),
        made("guests", "", at.clone()),
    ];
    for v in [70, 106] {
        let (mut zoo, money) = Zoo::parse_money(&lean(v)).unwrap();
        let before = zoo.entities.len();
        for (i, e) in made.iter().enumerate() {
            let mut e = e.clone();
            e.common.as_mut().unwrap().id = 100 + i as u32;
            zoo.entities.push(e);
        }
        let back = parse_to_the_end(&zoo.write(&money).unwrap());
        assert_eq!(back.entities.len(), before + made.len());
        for (i, (e, b)) in made.iter().zip(&back.entities[before..]).enumerate() {
            let what = format!("version {v}, {}/{} ({i})", e.class, e.subtype);
            assert_eq!((&b.class, &b.subtype), (&e.class, &e.subtype), "{what}");
            let (c, b) = (e.common.as_ref().unwrap(), b.common.as_ref().expect(&what));
            let place = |c: &Common| {
                (
                    c.x,
                    c.y,
                    c.z,
                    c.rotation,
                    c.id,
                    c.name.clone(),
                    c.colors.clone(),
                )
            };
            assert_eq!(
                place(b),
                (96, 160, 16, 3, 100 + i as u32, "New".into(), vec![5, 6]),
                "{what}"
            );
            let keeps = e.class == "food" || e.subtype == "poo";
            assert_eq!(b.food, if keeps { c.food } else { 0 }, "{what}");
            assert_eq!((b.fence, b.filter), (c.fence, c.filter), "{what}");
            assert_eq!(b.building, c.building, "{what}");
            assert_eq!(b.animal.is_some(), e.class == "animals", "{what}");
            assert_eq!(b.guest.is_some(), e.class == "guests", "{what}");
            if c.animal.is_some() {
                assert_eq!(b.animal, c.animal, "{what}");
            }
            if c.guest.is_some() {
                assert_eq!(b.guest, c.guest, "{what}");
            }
            // A unit is on the ground with the blocks of a new one.
            if let Some(u) = &b.unit {
                assert_eq!(
                    (b.movement_mode, u.as_ref()),
                    (
                        2,
                        &fresh::unit_new(
                            (e.class == "guests").then_some(fresh::DEFAULT_NON_PATH_COST)
                        )
                    ),
                    "{what}"
                );
            }
        }
    }
}

#[test]
fn a_new_unit_is_written_with_the_values_of_a_unit_made_in_play() {
    let (mut zoo, money) = Zoo::parse_money(&lean(106)).unwrap();
    zoo.entities.push(made("animals", "m", Common::default()));
    zoo.entities.push(made("guests", "", Common::default()));
    let back = parse_to_the_end(&zoo.write(&money).unwrap());
    let n = back.entities.len();
    let unit = |i: usize| {
        back.entities[n - 2 + i]
            .common
            .clone()
            .unwrap()
            .unit
            .unwrap()
    };
    let (a, g) = (unit(0), unit(1));
    // No goal, no target, no movement yet; the counters a new unit has (save games 11.3).
    assert_eq!(
        (a.target.active, a.target.active_sub, a.target.target, a.p3),
        (-1, -1, [-1; 3], [-1; 3])
    );
    assert_eq!((a.move_kind, a.w7, a.slot, a.need), (-1, 4, 255, 255));
    assert_eq!((a.w4, a.short), (1, 100));
    // A guest's word 230 is the type's cost (7 when none is given), not 0.
    assert_eq!(g.w4, 7);
    assert_eq!(g.target.active, -1);
}

#[test]
fn a_new_unit_has_no_sub_target() {
    let (mut zoo, money) = Zoo::parse_money(&lean(106)).unwrap();
    zoo.entities.push(made("animals", "m", Common::default()));
    let back = parse_to_the_end(&zoo.write(&money).unwrap());
    let unit = back
        .entities
        .last()
        .unwrap()
        .common
        .clone()
        .unwrap()
        .unit
        .unwrap();
    assert_eq!(unit.target.sub_target, [-1; 3]);
}

#[test]
fn a_new_unit_has_the_last_position_and_call_flag_of_a_unit_made_in_play() {
    let (mut zoo, money) = Zoo::parse_money(&lean(106)).unwrap();
    zoo.entities.push(made("animals", "m", Common::default()));
    let back = parse_to_the_end(&zoo.write(&money).unwrap());
    let unit = back
        .entities
        .last()
        .unwrap()
        .common
        .clone()
        .unwrap()
        .unit
        .unwrap();
    assert_eq!((unit.p1, unit.pending_flag), ([-1; 3], 1));
}

#[test]
fn a_new_unit_is_alive_and_a_new_building_is_not_for_free() {
    let (mut zoo, money) = Zoo::parse_money(&lean(106)).unwrap();
    zoo.clocks.calendar = 86_400_000 * 40;
    let at = Common::default();
    zoo.entities.push(made("animals", "m", at.clone()));
    zoo.entities.push(made("guests", "", at.clone()));
    zoo.entities.push(made("building", "shop", at));
    let back = parse_to_the_end(&zoo.write(&money).unwrap());
    let c = |i: usize| {
        back.entities[back.entities.len() - 3 + i]
            .common
            .clone()
            .unwrap()
    };
    let a = c(0).animal.unwrap();
    assert_eq!(
        (a.happiness, a.health, a.energy, a.dying),
        (60, 500, 0, false)
    );
    // The arrival and construction dates are the game date.
    let now = crate::timing::file_time(zoo.clocks.calendar);
    assert_eq!(c(1).guest.unwrap().arrived, now);
    assert_eq!(c(2).building.unwrap().built, now);
}

/// A zero body of class `class` in version `v`, whose class fields before the unit block or after
/// the base are `own` bytes.
fn zero_body(v: u32, class: &str, own: usize) -> Vec<u8> {
    let mut w = W::default();
    let unit = is_unit_class(class);
    match class {
        "ambient" => w.z(own),
        _ if !unit => w.z(4),
        _ => &mut w,
    };
    if unit {
        w.z(own);
        let mut o = Out(Vec::new());
        unit::write(&mut o, v, class == "guests", 2, &UnitBlock::default()).unwrap();
        w.bytes(&o.0);
    }
    base(&mut w, v, 6, 1, "x");
    if !unit && class != "ambient" {
        w.z(own);
    }
    w.0
}

fn is_unit_class(class: &str) -> bool {
    !matches!(class, "building" | "filter" | "rubble" | "ambient")
}

/// An animal record of `own` zero bytes in version `v`, read and written as 106.
fn animal_through(v: u32, own: usize) -> Vec<u8> {
    let body = vec![0; own];
    let mut r = Reader::new(&body, v);
    let a = animal::read(&mut r).unwrap_or_else(|e| panic!("{}", e.0));
    assert!(r.data.is_empty(), "version {v}");
    let mut o = Out(Vec::new());
    animal::write(&mut o, &a).unwrap();
    o.0
}

#[test]
fn the_class_fields_of_every_version_become_those_of_106() {
    // The length of the class fields in version `v` and in 106, from zoo format 6.7 to 6.13.
    for v in 70..=106u32 {
        let from = |x: bool| usize::from(x);
        let cases: [(&str, usize, usize); 5] = [
            (
                "animals",
                195 + from(v >= 76)
                    + 8 * from(v >= 77)
                    + from(v >= 78)
                    + 5 * from(v >= 79)
                    + 8 * from(v >= 87)
                    + 8 * from(v >= 101),
                // The sex string is written as "Female" for the empty one read.
                232,
            ),
            ("guests", 151, 151),
            (
                "building",
                4 * 5 + 4 + 16 + 8 + 4 + 4 * from(v >= 74) + 4 * from(v >= 75),
                60,
            ),
            ("filter", 4 + 32 * from(v >= 95), 36),
            ("rubble", 8, 8),
        ];
        for (class, own, to) in cases {
            if class == "animals" {
                assert_eq!(animal_through(v, own).len(), to, "animals in version {v}");
                continue;
            }
            let body = zero_body(v, class, own);
            let (pre, post) = body::transcode(v, class.as_bytes(), &body)
                .unwrap_or_else(|e| panic!("{class} in version {v}: {e}"));
            let got = if is_unit_class(class) {
                pre.len()
            } else {
                post.len()
            };
            assert_eq!(got, to, "{class} in version {v}");
            assert_eq!(pre.len() + post.len(), got, "{class} in version {v}");
        }
        let body = zero_body(v, "ambient", 8);
        assert_eq!(body::transcode(v, b"ambient", &body).unwrap().0.len(), 8);
    }
}

#[test]
fn the_staff_record_of_every_version_is_written_as_that_of_106() {
    // The own fields and the common staff block (zero counts) in version `v`, and the written length.
    for v in 70..=106u32 {
        let from = |x: bool| usize::from(x);
        let common = 12 + 4 * from(v >= 75);
        let cases = [
            ("keeper", 21 + 4 * from(v <= 74), 37),
            ("maint", 4 + 14 * from(v >= 92), 34 + 4 * from(v < 92)),
            ("tour", 13, 29),
            ("helicopter", 12 * from(v >= 75), 28),
            ("staff", 0, 16),
        ];
        for (class, own, to) in cases {
            let body = vec![0; own + common];
            let mut r = Reader::new(&body, v);
            let record = staff::read(&mut r, class.as_bytes())
                .unwrap_or_else(|e| panic!("{class} in version {v}: {}", e.0));
            assert!(r.data.is_empty(), "{class} in version {v}");
            let mut o = Out(Vec::new());
            staff::write(&mut o, class.as_bytes(), &record);
            assert_eq!(o.0.len(), to, "{class} in version {v}");
        }
    }
}

#[test]
fn the_fields_a_record_lacked_take_the_values_of_the_original() {
    // An animal before 101 has no home exhibit: none is written; before 87 the breath is 0.
    let out = animal_through(70, 195);
    assert_eq!(out[out.len() - 8..], [0xff; 8]);
    assert_eq!(out[out.len() - 16..out.len() - 8], [0; 8]);
    // A filter before 95 has no upkeep: three zero words, two "never" dates and a 1 follow its health.
    let mut body = zero_body(70, "filter", 4);
    body[4 + 12..4 + 16].copy_from_slice(&[7, 0, 0, 0]);
    let (_, post) = body::transcode(70, b"filter", &body).unwrap();
    assert_eq!(post[..4], [0; 4]);
    let never = 125_911_584_000_000_000u64.to_le_bytes();
    assert_eq!(post[16..24], never);
    assert_eq!(post[32..], [1, 0, 0, 0]);
    // A guest before 94 drops its two strings for a word and an empty string.
    let mut own = vec![0; 97];
    own.extend([2, 0, 0, 0, b'a', b'b', 0, 0, 0, 0]);
    own.extend([0; 46]);
    let mut body = zero_body(93, "guests", 0);
    body.splice(0..0, own);
    let (pre, _) = body::transcode(93, b"guests", &body).unwrap();
    assert_eq!((pre.len(), pre[97..105].to_vec()), (151, vec![0; 8]));
}

#[test]
fn habitats_and_shows_of_an_old_file_are_written_as_106() {
    for v in [70, 83, 85, 88, 89, 93, 97, 99, 100, 103, 105] {
        let (zoo, money) = Zoo::parse_money(&lean(v)).unwrap();
        let back = parse_to_the_end(&zoo.write(&money).unwrap());
        assert_eq!(back.tanks, zoo.tanks, "version {v}");
        assert_eq!(back.habitat_info, zoo.habitat_info, "version {v}");
        if v < 97 {
            // No script, and the counters of a fresh map.
            assert_eq!(
                back.kept.shows,
                [0, 0, 0, 0, 0xd0, 7, 0xe8, 3],
                "version {v}"
            );
        }
    }
    // A show is written from 106 only, scripts from 103.
    for v in [90, 98, 101, 105] {
        let (zoo, money) = Zoo::parse_money(&rich(v)).unwrap();
        let why = zoo.write(&money).unwrap_err();
        assert!(why.contains("show"), "version {v}: {why}");
    }
    for v in [89, 96, 97, 102] {
        let (zoo, money) = Zoo::parse_money(&file_with(v, 3, 3, &[], &[], 1)).unwrap();
        let why = zoo.write(&money).unwrap_err();
        assert!(why.starts_with("shows with scripts"), "version {v}: {why}");
    }
    for (v, file) in [(103, file_with(103, 3, 3, &[], &[], 1)), (106, rich(106))] {
        let (zoo, money) = Zoo::parse_money(&file).unwrap();
        let out = zoo.write(&money).unwrap();
        assert_eq!(
            parse_to_the_end(&out).kept.shows,
            zoo.kept.shows,
            "version {v}"
        );
    }
}

#[test]
fn a_file_before_version_70_is_not_written_yet() {
    let (zoo, money) = Zoo::parse_money(&lean(60)).unwrap();
    assert!(zoo.write(&money).unwrap_err().contains("70"));
}

/// A unit block of the model, with every kind of field set.
fn block() -> UnitBlock {
    let level = |name: &str, child| Level {
        name: name.into(),
        list_a: vec!["a".into(), "b".into()],
        call: "bCall".into(),
        args: vec!["x".into()],
        flags: [1, 2, 3],
        child,
        list_b: vec!["t".into()],
        tail: [4, 5, 6, 7],
        ..Level::default()
    };
    UnitBlock {
        b1: 3,
        w1: 9,
        p1: [32 + 64 * 3, 32 + 64 * 5, 0],
        building: [4, 8],
        p2: [64 * 2 + 7, 64 * 4 + 9, 0],
        p3: [32 + 64 * 9, 32 + 64 * 8, 0],
        target: Target {
            goal: 7,
            sub_goal: -3,
            words: [1, u32::MAX],
            target: [64 * 6, 64, 0],
            sub_target: [-1, -1, -1],
            bytes: [1, 2],
            extra: [5, 6],
            active: 7,
            active_sub: 0,
        },
        pending: "bEat".into(),
        pending_flag: 1,
        records: vec![[1, 2, 3], [4, 5, 6]],
        limited: 10,
        b11: 1,
        w9: 12,
        move_kind: 3,
        move_data: (1..=33).collect(),
        text: "far".into(),
        flags: [(1, 513), (0, 0)],
        behaviour: Behaviour {
            levels: vec![level("bWalk", 1), level("bHappy", 1), level("bSit", 0)],
        },
        ..UnitBlock::default()
    }
}

/// The unit block as written in version `v`, read back in version `v`.
fn through(v: u32, guest: bool, u: &UnitBlock) -> UnitBlock {
    let mut o = Out(Vec::new());
    unit::write(&mut o, v, guest, 2, u).unwrap();
    let mut r = Reader::new(&o.0, v);
    let (back, mode) = unit::read(&mut r, guest).unwrap_or_else(|e| panic!("{}", e.0));
    assert_eq!((mode, r.data.len()), (2, 0), "version {v}");
    back
}

fn read_unit(bytes: &[u8], v: u32) -> (UnitBlock, u32) {
    unit::read(&mut Reader::new(bytes, v), false).unwrap_or_else(|e| panic!("{}", e.0))
}

#[test]
fn the_unit_block_is_written_and_read_back_in_every_version() {
    for v in [45, 53, 57, 62, 70, 74, 80, 84, 85, 90, 93, 97, 98, 101, 106] {
        let mut u = block();
        // What a version lacks takes the default: the model of an old file cannot hold more.
        let old = through(v, false, &u);
        assert_eq!(through(v, false, &old), old, "version {v}");
        assert_eq!(old.p1, u.p1, "version {v}");
        assert_eq!(old.p2, u.p2, "version {v}");
        assert_eq!(old.target.target, u.target.target, "version {v}");
        assert_eq!(old.p3, u.p3, "version {v}");
        assert_eq!(old.b1, 3);
        assert_eq!(old.pending, "bEat");
        assert_eq!(old.records.len(), 2);
        assert_eq!(old.limited, if v >= 46 { 10 } else { 0 });
        if v >= 106 {
            u.behaviour = old.behaviour.clone();
            assert_eq!(old, u);
        }
    }
}

#[test]
fn a_field_a_version_lacks_takes_the_value_the_original_writes() {
    let u = UnitBlock {
        short: 5,
        b5: 4,
        w9: 9,
        text: "x".into(),
        move_kind: 3,
        move_data: vec![1; 33],
        limited: 4,
        target: Target {
            goal: 12,
            sub_goal: 4,
            extra: [1, 2],
            active: 99,
            active_sub: 98,
            ..Target::default()
        },
        records: vec![[1, 2, 3]],
        ..UnitBlock::default()
    };
    let old = through(70, false, &u);
    assert_eq!((old.short, old.b5, old.w9), (100, 0, 0));
    assert_eq!((old.move_kind, old.move_data.len(), &*old.text), (2, 0, ""));
    // Before 72 the active goal and sub-goal are the wanted ones.
    assert_eq!((old.target.active, old.target.active_sub), (12, 4));
    let newer = through(72, false, &u);
    assert_eq!((newer.target.active, newer.target.active_sub), (99, 98));
    assert_eq!(through(52, false, &u).target.extra, [0, 0]);
    assert_eq!(old.records, [[1, 2, 0]]);
    assert_eq!(through(83, false, &u).records, [[1, 2, 0]]);
    // Reading version 84 gives the mode and the type 2 only when the file lacks them.
    let mut o = Out(Vec::new());
    unit::write(&mut o, 83, false, 3, &u).unwrap();
    let (back, mode) = read_unit(&o.0, 83);
    assert_eq!((mode, back.move_kind), (2, 2));
}

#[test]
fn old_tiles_become_world_positions() {
    let u = UnitBlock {
        p1: [32 + 64 * 3, 32 + 64 * 5, 0],
        p2: [64 * 2 + 7, 64 * 4 + 9, 0],
        target: Target {
            target: [64 * 6, 64 * 7, 0],
            sub_target: [-1; 3],
            ..Target::default()
        },
        p3: [32 + 64 * 9, 32 + 64 * 8, 0],
        ..UnitBlock::default()
    };
    let mut o = Out(Vec::new());
    unit::write(&mut o, 70, false, 2, &u).unwrap();
    let int = |o: &Out, at: usize| i32::from_le_bytes(o.0[at..at + 4].try_into().unwrap());
    // 6 bytes, then the first tile; a tile and two words; then the pair of tiles.
    assert_eq!((int(&o, 6), int(&o, 10)), (3, 5));
    assert_eq!(
        [int(&o, 30), int(&o, 34), int(&o, 38), int(&o, 42)],
        [2, 4, 7, 9]
    );
    // After 12 more bytes of the base block and 16 of the goals: the two target tiles.
    assert_eq!(
        [int(&o, 80), int(&o, 84), int(&o, 88), int(&o, 92)],
        [6, 7, -1, -1]
    );
    let (back, _) = read_unit(&o.0, 70);
    assert_eq!((back.p1, back.p2), (u.p1, u.p2));
    assert_eq!(
        (back.target.target, back.target.sub_target),
        (u.target.target, [-1; 3])
    );
    assert_eq!(back.p3, u.p3);
    // The tile (-1, -1) is the position (-1, -1, -1), and the pair of tiles (-1, -1) is (-65, -65, 0).
    let none = UnitBlock {
        p1: [-1; 3],
        p2: [-65, -65, 0],
        ..UnitBlock::default()
    };
    let mut o = Out(Vec::new());
    unit::write(&mut o, 70, false, 2, &none).unwrap();
    assert_eq!([int(&o, 6), int(&o, 10)], [-1, -1]);
    assert_eq!(
        [int(&o, 30), int(&o, 34), int(&o, 38), int(&o, 42)],
        [-1; 4]
    );
    let (back, _) = read_unit(&o.0, 70);
    assert_eq!((back.p1, back.p2), (none.p1, none.p2));
    // And the hostile extremes wrap instead of panicking.
    let huge = UnitBlock {
        p1: [i32::MAX, i32::MIN, 0],
        p2: [i32::MIN, i32::MAX, 0],
        ..UnitBlock::default()
    };
    through(70, false, &huge);
}

#[test]
fn the_destination_before_75_is_the_tile_centre() {
    let at = |p: [i32; 3]| {
        through(
            70,
            false,
            &UnitBlock {
                p3: p,
                ..UnitBlock::default()
            },
        )
        .p3
    };
    assert_eq!(at([64 * 27 + 32, 64 * 30 + 32, 0]), [1760, 1952, 0]);
    // No (-1, -1) case.
    assert_eq!(at([-32, -32, 0]), [-32, -32, 0]);
    assert_eq!(at([-1, -1, -1]), [-32, -32, 0]);
}

#[test]
fn old_tiles_carry_the_ground_height() {
    use super::{Ground, Tile};
    let tile = |elevation, shape| Tile {
        elevation,
        shape,
        terrain: 0,
        edges: 0,
    };
    // Row 0: base -3 flat, base 2 sloping; row 1 the same.
    let tiles = vec![tile(-3, 0), tile(2, 0x50), tile(-3, 0), tile(2, 0x50)];
    let ground = std::rc::Rc::new(Ground {
        width: 2,
        height: 2,
        tiles,
    });
    let u = UnitBlock {
        p1: [32, 32, 0],
        p2: [64 + 8, 5, 0],
        p3: [32 + 64, 32, 0],
        target: Target {
            target: [0, 0, 0],
            sub_target: [64 * 5, 0, 0],
            ..Target::default()
        },
        ..UnitBlock::default()
    };
    let mut o = Out(Vec::new());
    unit::write(&mut o, 70, false, 2, &u).unwrap();
    let mut r = Reader::new(&o.0, 70);
    r.ground = Some(ground);
    let (back, _) = unit::read(&mut r, false).unwrap_or_else(|e| panic!("{}", e.0));
    assert_eq!(back.p1, [32, 32, -48]);
    // The point (8, 5) of a sloping tile: 16 * 2 + 8 / 4.
    assert_eq!(back.p2, [72, 5, 34]);
    assert_eq!(back.p3, [96, 32, 40]);
    // The target takes the base height only; a tile outside the map is (-1, -1, -1).
    assert_eq!(back.target.target, [0, 0, -48]);
    assert_eq!(back.target.sub_target, [-1; 3]);
}

#[test]
fn a_unit_block_value_above_10_is_reset() {
    let mut o = Out(Vec::new());
    let u = UnitBlock {
        limited: 11,
        ..UnitBlock::default()
    };
    unit::write(&mut o, 106, false, 2, &u).unwrap();
    let (back, _) = read_unit(&o.0, 106);
    assert_eq!(back.limited, 0);
    for n in [0, 10] {
        let u = UnitBlock {
            limited: n,
            ..u.clone()
        };
        assert_eq!(through(106, false, &u).limited, n);
    }
}

#[test]
fn the_flags_of_version_97_are_written_only_when_exactly_1() {
    let flags = [(1, 513), (2, 0)];
    let u = UnitBlock {
        flags,
        ..UnitBlock::default()
    };
    assert_eq!(through(106, false, &u).flags, flags);
    // Guests have none, and neither has a version before 97.
    assert_eq!(through(106, true, &u).flags, [(0, 0); 2]);
    assert_eq!(through(96, false, &u).flags, [(0, 0); 2]);
    // A value that follows a flag other than 1 is not written.
    let lost = UnitBlock {
        flags: [(2, 77), (0, 0)],
        ..u
    };
    assert_eq!(through(106, false, &lost).flags, [(2, 0), (0, 0)]);
}

#[test]
fn a_movement_of_type_3_keeps_its_data_and_the_mode_is_the_type_before_93() {
    let u = block();
    let back = through(106, false, &u);
    assert_eq!((back.move_kind, back.move_data), (3, u.move_data.clone()));
    let back = through(95, false, &u);
    assert_eq!(back.move_data.len(), 13);
    // Before 93 the type written is the mode, so a mode of 3 gets data and another mode drops it.
    let mut o = Out(Vec::new());
    unit::write(&mut o, 92, false, 1, &u).unwrap();
    let (back, mode) = read_unit(&o.0, 92);
    assert_eq!((mode, back.move_kind, back.move_data.len()), (1, 1, 0));
    for kind in [-1, i32::MIN, 1, 2] {
        let u = UnitBlock {
            move_kind: kind,
            ..u.clone()
        };
        assert_eq!(through(106, false, &u).move_kind, kind);
    }
}

#[test]
fn a_string_of_4096_characters_in_a_unit_block_is_not_written() {
    let mut o = Out(Vec::new());
    let u = UnitBlock {
        pending: "x".repeat(4096),
        ..UnitBlock::default()
    };
    assert!(unit::write(&mut o, 106, false, 2, &u).is_err());
    let mut b = block().behaviour;
    assert!(unit::write_behaviour(&mut Out(Vec::new()), &b).is_ok());
    b.levels[1].args.push("y".repeat(4096));
    assert!(unit::write_behaviour(&mut Out(Vec::new()), &b).is_err());
}

#[test]
fn the_unit_blocks_of_a_zoo_are_read_and_written() {
    for v in [101, 106] {
        let (mut zoo, money) = Zoo::parse_money(&lean(v)).unwrap();
        let c = zoo.entities[1].common.as_mut().unwrap();
        let mut u = through(v, false, &block());
        u.behaviour = block().behaviour;
        c.unit = Some(Box::new(u.clone()));
        c.movement_mode = 3;
        let out = zoo.write(&money).unwrap();
        let back = Zoo::parse(&out).unwrap();
        let b = back.entities[1].common.as_ref().unwrap();
        assert_eq!(b.unit.as_deref(), Some(&u), "version {v}");
        assert_eq!(b.movement_mode, 3, "version {v}");
        // The entity base after the set is intact.
        assert_eq!(b.name, "unit");
        assert_eq!(b.colors, [3, 9]);
        assert_eq!(b.rotation, 6);
    }
    // A unit read from a file keeps the set of its base, and the other entities have none.
    let zoo = Zoo::parse(&rich(106)).unwrap();
    let u = zoo.entities[1]
        .common
        .as_ref()
        .unwrap()
        .unit
        .as_ref()
        .unwrap();
    assert_eq!(u.behaviour.levels[0].name, "bWalk");
    assert_eq!(u.behaviour.levels[0].list_a, ["x"]);
    assert_eq!(u.behaviour.levels[0].list_b, ["y"]);
    assert!(zoo.entities[0].common.as_ref().unwrap().unit.is_none());
    assert_eq!(u.short, 0);
}

/// Every shipped file that loads is written as version 106, which reads to the last byte and does
/// not change on a second write; a file that was 106 comes back byte for byte but for the name
/// counters at 0, which the original does not write (save games 7).
#[test]
#[ignore = "needs the game: set ZT_DATA_DIR to the install folder"]
fn full_game_round_trip() {
    let dir = std::env::var_os("ZT_DATA_DIR").expect("ZT_DATA_DIR is not set");
    let vfs = crate::vfs::Vfs::open(std::path::Path::new(&dir)).unwrap();
    let (mut files, mut same, mut refused) = (0, 0, 0);
    for member in vfs.list(None).iter().filter(|m| m.ends_with(".zoo")) {
        let bytes = vfs.read(member).unwrap();
        let Ok((zoo, money)) = Zoo::parse_money(&bytes) else {
            continue;
        };
        files += 1;
        let out = match zoo.write(&money) {
            Ok(out) => out,
            Err(why) if why.contains("show") || zoo.version < 70 => {
                refused += 1;
                continue;
            }
            Err(why) => panic!("{member}: {why}"),
        };
        let back = parse_to_the_end(&out);
        assert_eq!(back.entities.len(), zoo.entities.len(), "{member}");
        let (again, money) = Zoo::parse_money(&out).unwrap();
        assert!(
            again.write(&money).unwrap() == out,
            "{member}: second write"
        );
        if zoo.version == 106 {
            let zeros = zoo.names.values().filter(|&&n| n == 0).count();
            let at = out.iter().zip(&bytes).position(|(a, b)| a != b);
            assert!(
                out == bytes || zeros > 0,
                "{member}: first difference at {at:?}"
            );
            assert_eq!(back.names.len() + zeros, zoo.names.len(), "{member}");
            same += 1;
        }
    }
    // The 96 files that parse (95 in the French install, which lacks the saved game).
    assert!(files >= 95, "{files} files");
    assert!(
        same >= 10 && refused < 10,
        "{same} identical, {refused} refused"
    );
}

/// A guest record of version 106 whose lists hold five item slots and marks in the fields the
/// model does not hold (the increment factors, the second date), and the guest's class fields'
/// length. The tail starts 42 bytes before its end.
fn marked_guest() -> (Vec<u8>, usize) {
    let mut body = zero_body(106, "guests", 151);
    let items: Vec<u8> = [0u32, 6491, 0, 6319, 6310]
        .iter()
        .flat_map(|n| n.to_le_bytes())
        .collect();
    body.splice(89..93, [5, 0, 0, 0].into_iter().chain(items));
    let tail = 171 - 42;
    body[21] = 7;
    body[tail + 10] = 9;
    body[tail + 34] = 3;
    for (at, v) in [(4, 1), (5, 1), (11, 1), (24, 1), (25, 1), (36, 1), (37, 1)] {
        body[tail + at] = v;
    }
    body[tail + 38] = 77;
    (body, tail)
}

#[test]
fn a_guest_record_reads_and_writes_its_items_flags_and_guide() {
    let (body, tail) = marked_guest();
    let bytes = file(106, 1, 1, &[], &[("guests", "man", "", body)]);
    let (mut zoo, money) = Zoo::parse_money(&bytes).unwrap();
    let g = *zoo.entities[0]
        .common
        .as_ref()
        .unwrap()
        .guest
        .as_deref()
        .unwrap();
    assert_eq!(g.items, [0, 6491, 0, 6319, 6310]);
    let flags = [
        g.leave,
        g.afraid,
        g.gift,
        g.wandered,
        g.stand,
        g.in_group,
        g.listening,
    ];
    assert_eq!((flags, g.guide), ([true; 7], 77));
    // What was read is written back as it was.
    let same = parse_to_the_end(&zoo.write(&money).unwrap());
    let kept = |z: &Zoo| z.entities[0].common.as_ref().unwrap().raw.body.clone();
    assert!(kept(&same) == kept(&zoo));
    // The model's fields are written over the record, the rest of it stays.
    let c = zoo.entities[0].common.as_mut().unwrap();
    c.guest = Some(Box::new(GuestRecord {
        items: [5003, 0, 0, 0, u32::MAX],
        leave: false,
        wandered: false,
        guide: 0,
        values: [1, -2, 3, 4, 5],
        ..g
    }));
    let back = parse_to_the_end(&zoo.write(&money).unwrap());
    let c = back.entities[0].common.as_ref().unwrap();
    let r = *c.guest.as_deref().unwrap();
    assert_eq!(r.items, [5003, 0, 0, 0, u32::MAX]);
    assert_eq!(
        (r.leave, r.wandered, r.guide, r.values),
        (false, false, 0, [1, -2, 3, 4, 5])
    );
    assert_eq!(
        (r.afraid, r.gift, r.stand, r.listening),
        (true, true, true, true)
    );
    let b = &c.raw.body;
    assert_eq!((b[21], b[tail + 10], b[tail + 34]), (7, 9, 3));
}

#[test]
fn a_new_guest_record_has_five_empty_slots_and_factors_of_one() {
    let b = fresh::guest(None, &fresh::guest_new(50, 5)).unwrap();
    assert_eq!(b.len(), 171);
    assert_eq!(b[21..41], [1, 0, 0, 0].repeat(5));
    assert_eq!(b[89..93], 5u32.to_le_bytes());
    assert_eq!(b[93..113], [0; 20]);
    assert_eq!(b[0..4], 50u32.to_le_bytes());
    // The same record patched again does not change.
    assert!(fresh::guest(Some(b.clone()), &fresh::guest_new(50, 5)).unwrap() == b);
}

#[test]
fn the_guest_record_of_every_version_takes_the_model_without_changing_its_length() {
    for v in 70..=106u32 {
        let body = zero_body(v, "guests", 151);
        let (pre, _) = body::transcode(v, b"guests", &body).unwrap();
        let out = fresh::guest(Some(pre.clone()), &fresh::guest_new(1, 1)).unwrap();
        // Five slots instead of none.
        assert_eq!(out.len(), pre.len() + 20, "version {v}");
    }
}

#[test]
fn a_short_or_wild_guest_record_is_refused_and_never_panics() {
    let g = fresh::guest_new(1, 1);
    for n in [0, 1, 41, 42, 88, 89, 92, 93, 100, 134] {
        assert!(fresh::guest(Some(vec![0; n]), &g).is_err(), "{n} bytes");
    }
    // A count of items that passes the end.
    let mut wild = vec![0; 200];
    wild[89..93].copy_from_slice(&u32::MAX.to_le_bytes());
    assert!(fresh::guest(Some(wild.clone()), &g).is_err());
    // More than five items: the first five are the slots, the rest is dropped.
    wild[89..93].copy_from_slice(&7u32.to_le_bytes());
    let out = fresh::guest(Some(wild), &g).unwrap();
    assert_eq!(out.len(), 200 - 28 + 20);
    // The smallest record there is.
    assert!(fresh::guest(Some(vec![0; 135]), &g).is_ok());
}

/// An animal record written as 106 and read back.
fn animal_back(a: &AnimalRecord) -> AnimalRecord {
    let mut o = Out(Vec::new());
    animal::write(&mut o, a).unwrap();
    let mut r = Reader::new(&o.0, 106);
    let back = animal::read(&mut r).unwrap_or_else(|e| panic!("{}", e.0));
    assert!(r.data.is_empty());
    back
}

#[test]
fn a_new_animal_is_written_with_the_fields_of_a_new_one() {
    let new = AnimalRecord {
        life: Some(4319),
        baby: 999,
        interval: 1600,
        mate_delay: Some(2),
        boxed: 1,
        ..animal_new(75, 120, 7)
    };
    let back = animal_back(&new);
    assert_eq!(
        (back.happiness, back.hunger, back.health, back.energy),
        (75, 0, 120, 0)
    );
    assert_eq!(
        (back.dates, back.home, back.escape),
        ([7, 7], [-1, -1], None)
    );
    assert_eq!(back.second, "\n");
    assert_eq!(
        (
            back.life,
            back.baby,
            back.interval,
            back.mate_delay,
            back.boxed
        ),
        (Some(4319), 999, 1600, Some(2), 1)
    );
    // The keeper-visit level and the five rate multipliers are 1.
    let ones: Vec<usize> = (0..74).filter(|&i| back.spare[i] != 0).collect();
    assert_eq!((back.spare.len(), ones), (74, vec![1, 6, 10, 14, 18, 22]));
    assert!(back.spare.iter().filter(|&&b| b > 1).count() == 0);
}

#[test]
fn an_animal_record_of_any_size_is_written_whole() {
    let full = new_animal();
    assert_eq!(animal_back(&full), full);
    // The extremes, a spare list that is short or too long: the segments it lacks are a new animal's,
    // the extra ones are not written.
    let extreme = AnimalRecord {
        happiness: i32::MIN,
        hunger: i32::MAX,
        rampage: -1,
        counters: [Some(i32::MIN); 16],
        breath: Some(i32::MAX),
        spare: vec![9; 3],
        ..full.clone()
    };
    let back = animal_back(&extreme);
    assert_eq!(
        (back.happiness, back.hunger, back.rampage),
        (i32::MIN, i32::MAX, -1)
    );
    assert_eq!(back.breath, Some(i32::MAX));
    assert_eq!(back.spare[..3], [9, 1, 0]);
    assert_eq!(back.spare.len(), 74);
    let long = AnimalRecord {
        spare: vec![1; 500],
        ..full
    };
    assert_eq!(animal_back(&long).spare, vec![1; 74]);
    // An escape record is kept only with the flag 1.
    let none = AnimalRecord {
        escape: None,
        ..new_animal()
    };
    assert_eq!(animal_back(&none), none);
}

/// A staff body of version 106 up to the end of the staff block: the own fields of `class`, then
/// the work and chase counters, two assigned tiles and the captured animal.
fn staff_106(class: &str) -> Vec<u8> {
    let mut w = W::default();
    match class {
        "keeper" => {
            w.u32(1).u32(2).u32(3).u32(4).u32(5).u8(6);
        }
        "maint" => {
            w.u32(9).u32(2).u32(3).u16(2).u32(4).bytes(&[1, 0, 1, 0]);
        }
        "tour" => {
            w.u32(7).i32(-1).u32(2).u32(40).u32(41).u8(1);
        }
        "helicopter" => {
            w.u32(8).u32(9).u32(10);
        }
        _ => {}
    }
    w.i32(-7)
        .i32(30)
        .u32(2)
        .i32(3)
        .i32(4)
        .i32(-1)
        .i32(-1)
        .u32(55);
    w.0
}

fn staff_read(v: u32, class: &str, body: &[u8]) -> Result<StaffRecord, String> {
    let mut r = Reader::new(body, v);
    let record = staff::read(&mut r, class.as_bytes()).map_err(|e| e.0)?;
    assert!(r.data.is_empty() || v < 106, "{class}: unread bytes");
    Ok(record)
}

#[test]
fn a_staff_record_of_version_106_is_read_and_written_back_byte_for_byte() {
    for class in ["keeper", "maint", "tour", "helicopter", "staff"] {
        let body = staff_106(class);
        let record = staff_read(106, class, &body).unwrap();
        assert_eq!((record.work, record.chase, record.captured), (-7, 30, 55));
        assert_eq!(record.tiles, [[3, 4], [-1, -1]], "{class}");
        let mut o = Out(Vec::new());
        staff::write(&mut o, class.as_bytes(), &record);
        assert_eq!(o.0, body, "{class}");
    }
    let tour = staff_read(106, "tour", &staff_106("tour")).unwrap();
    assert_eq!(
        (tour.crowd, &tour.members[..], tour.leg),
        (Some(-1), &[40, 41][..], true)
    );
    let maint = staff_read(106, "maint", &staff_106("maint")).unwrap();
    assert_eq!(maint.duties, [true, false, true, false]);
}

#[test]
fn a_worker_writes_the_count_of_switches_on_from_its_flags() {
    let mut record = staff_read(106, "maint", &staff_106("maint")).unwrap();
    let count = |record: &StaffRecord| {
        let mut o = Out(Vec::new());
        staff::write(&mut o, b"maint", record);
        o.0[12..14].to_vec()
    };
    assert_eq!(count(&record), 2u16.to_le_bytes());
    record.duties = vec![true, true, true, false];
    assert_eq!(count(&record), 3u16.to_le_bytes());
    record.duties = vec![false; 4];
    assert_eq!(count(&record), 0u16.to_le_bytes());
    record.duties = Vec::new();
    assert_eq!(count(&record), 0u16.to_le_bytes());
}

#[test]
fn the_staff_fields_a_version_lacks_take_the_values_of_the_original() {
    let write = |class: &str, record: &StaffRecord| {
        let mut o = Out(Vec::new());
        staff::write(&mut o, class.as_bytes(), record);
        o.0
    };
    // A worker before 92: the two words 2 and 2, the number of duty kinds, all four on; before 75
    // no captured animal.
    let mut w = W::default();
    w.u32(5).i32(8).i32(9).u32(0);
    let record = staff_read(70, "maint", &w.0).unwrap();
    assert_eq!((record.work, record.chase, record.captured), (8, 9, 0));
    let mut expect = W::default();
    expect.u32(5).u32(2).u32(2).u16(4).u32(4).bytes(&[1; 4]);
    expect.i32(8).i32(9).u32(0).u32(0);
    assert_eq!(write("maint", &record), expect.0);
    // A guide before 54 has no crowd countdown (the load keeps its draw), from 54 to 64 a word that
    // 106 drops, before 65 no group and before 66 no leg flag.
    let mut w = W::default();
    w.u32(1).i32(8).i32(9).u32(0);
    let record = staff_read(53, "tour", &w.0).unwrap();
    assert_eq!(
        (record.crowd, record.members.len(), record.leg),
        (None, 0, false)
    );
    let mut w = W::default();
    w.u32(1).i32(6).u32(77).i32(8).i32(9).u32(0);
    let record = staff_read(60, "tour", &w.0).unwrap();
    assert_eq!((record.crowd, record.work, record.chase), (Some(6), 8, 9));
    // A keeper of 52 to 74 has one more word; a helicopter before 75 has no own fields.
    let mut w = W::default();
    w.z(21 + 4).i32(8).i32(9).u32(0);
    assert_eq!(staff_read(70, "keeper", &w.0).unwrap().work, 8);
    let mut w = W::default();
    w.i32(8).i32(9).u32(0);
    let record = staff_read(70, "helicopter", &w.0).unwrap();
    assert_eq!(write("helicopter", &record).len(), 12 + 16);
}

#[test]
fn a_staff_record_with_a_count_past_the_end_or_cut_short_is_an_error() {
    let full = staff_106("tour");
    for cut in 0..full.len() {
        assert!(
            staff_read(106, "tour", &full[..cut]).is_err(),
            "cut at {cut}"
        );
    }
    for (class, at) in [("tour", 8), ("maint", 14), ("staff", 8)] {
        for huge in [u32::MAX, 1 << 30, 1000] {
            let mut body = staff_106(class);
            body[at..at + 4].copy_from_slice(&huge.to_le_bytes());
            assert!(staff_read(106, class, &body).is_err(), "{class} {huge}");
        }
    }
    // Empty lists are fine.
    let mut w = W::default();
    w.u32(0).i32(0).u32(0).u8(0).i32(0).i32(0).u32(0).u32(0);
    assert!(staff_read(106, "tour", &w.0).unwrap().members.is_empty());
}

#[test]
fn a_guide_written_with_a_zero_countdown_reads_it_back() {
    let mut tour = staff_read(106, "tour", &staff_106("tour")).unwrap();
    tour.crowd = Some(0);
    let mut o = Out(Vec::new());
    staff::write(&mut o, b"tour", &tour);
    assert_eq!(staff_read(106, "tour", &o.0).unwrap().crowd, Some(0));
}
