//! Synthetic files built in code; no original data.

use super::*;

/// A growing byte buffer with the field types of the format.
#[derive(Default)]
struct W(Vec<u8>);

impl W {
    fn u8(&mut self, n: u8) -> &mut Self {
        self.0.push(n);
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
    w.i32(13).i32(45).str("ex").i32(-1).i32(-1).z(4 + 36 + 16);
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
fn shows(w: &mut W, v: u32) {
    if v >= 89 {
        w.u32(1).z(10).u32(1).z(12);
        w.str("a").str("").str("bc").str("");
        w.z(8 + 1 + 16);
        if v >= 103 {
            w.z(8).str("icon").str("grey");
        }
    }
    if v >= 97 {
        w.z(4);
    }
}

/// The sections after the entities, with something in every list.
fn tail(w: &mut W, v: u32) {
    w.i32(-2).i32(3).i32(1216).i32(-16).u32(5); // view
    if v >= 51 {
        w.u32(4 + 5 + 4 + 8).str("scn/x").u32(8).z(8);
    } else {
        w.u32(5).str("x"); // before 51 the block is the name alone
    }
    w.u32(2).str("a").u32(1).str("b").u32(2); // name counters
    w.z(4 + 12 + 1 + 16 + 4 + 8);
    w.u32(2).u32(3).z(2 * (12 + 3 + 1) * 4); // two months, three columns
    if v >= 39 {
        w.z(4);
    }
    if v >= 71 {
        w.z(8);
    }
    w.z(16 + 4);
    if v >= 73 {
        w.z(4);
    }
    w.u32(2).z(40); // thoughts
    w.z(8);
    if v >= 97 {
        w.u32(1).z(20).u32(0).z(1);
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
        shows(w, v);
    }
    if v >= 104 {
        w.u32(1).str("cheat");
    }
    if v >= 58 {
        w.z(if v <= 66 { 12 } else { 8 });
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
        shows(&mut w, v);
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
    tail(&mut w, v);
    w.0
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
    assert_eq!((z.habitat_count, z.id_counter), (0, 7777));
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
            (z.habitat_count, z.id_counter, z.tiles.len()),
            (4, 7777, 4),
            "version {v}"
        );
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
        filter.extend(vec![0; if v >= 95 { 4 + 12 + 16 + 4 } else { 4 }]);
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
    }
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
                assert_eq!((fence.strength, fence.life as i32), (7, -1));
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
    let guest = unit(106, 310, 22);
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
    shows(&mut w, 106);
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
    tail(&mut w, v);
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
    file(v, 3, 3, &[(true, true), (false, false)], &ents)
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
        assert_eq!((z.width, z.habitat_count), (3, 2), "version {v}");
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
        (map.entrance, map.habitat_count, map.id_counter),
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

    let save = load("saved games/test.zoo").unwrap();
    assert_eq!(
        (save.version, save.expansion, save.width, save.height),
        (106, 0, 125, 125)
    );
    assert_eq!(
        (save.entrance, save.habitat_count, save.id_counter),
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

    // Every .zoo file: 96 parse, 3 are older than version 38, 6 of version 38 are corrupted.
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
    assert_eq!((ok, old, corrupted), (96, 3, 6));
    untyped.sort();
    assert_eq!(
        untyped,
        [
            ("scenario/scn08/scn08.zoo".to_string(), 55),
            ("scenario/scn15/scn15.zoo".to_string(), 45)
        ]
    );
    // The only bodies not decoded are the 939 of size 0.
    assert_eq!((transient, skipped), (939, 939));
}
