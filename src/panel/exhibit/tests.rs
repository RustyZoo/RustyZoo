use super::*;
use crate::config::Config;
use crate::exhibit::rating::bit;
use crate::render::order::Kind;
use crate::timing::Date;
use crate::world::{Shape, TankKind};
use crate::zoo::{Common, Entity, Tile, View, Zoo};

fn entity(class: &str, tx: i32, ty: i32, id: u32) -> Entity {
    Entity {
        class: class.to_string(),
        type_name: String::new(),
        subtype: "f".to_string(),
        body_size: None,
        common: Some(Common {
            x: tx * 64 + 32,
            y: ty * 64 + 32,
            z: 0,
            rotation: 0,
            id,
            name: format!("e{id}"),
            colors: Vec::new(),
            fence: None,
            movement_mode: 0,
        }),
    }
}

fn shape(e: &Entity) -> Option<Shape> {
    (e.class == "fences").then_some(Shape {
        kind: Kind::Edge,
        footprint: (1, 1),
        fence: true,
        wall: false,
        gate: false,
        gated: true,
        walkable: (false, false),
        tank: TankKind::NONE,
    })
}

/// A 6 by 6 map with a square exhibit of the tiles 1..=2 and one animal in it, one on the
/// grounds, one off the map and a tree.
fn world() -> World {
    let tile = Tile {
        elevation: 0,
        shape: 0,
        terrain: 0,
        edges: 0,
    };
    let mut entities = Vec::new();
    let mut id = 1;
    let mut add = |class: &str, x: i32, y: i32| {
        entities.push(entity(class, x, y, id));
        id += 1;
    };
    // The pieces round the square, each on the edge of its own tile (rotation 0 is side 0).
    for x in 1..=2 {
        add("fences", x, 1);
    }
    add("animals", 1, 1);
    add("animals", 4, 4);
    add("animals", -3, 1);
    add("objects", 2, 2);
    let mut zoo = Zoo {
        version: 0,
        locale: 0,
        expansion: 0,
        width: 6,
        height: 6,
        entrance: (5, 5),
        habitat_tiles: Vec::new(),
        habitat_gates: Vec::new(),
        tanks: Vec::new(),
        id_counter: 100,
        tiles: vec![tile; 36],
        entities,
        view: View::default(),
        clocks: Default::default(),
    };
    // Close the square with pieces on every side of its tiles.
    for (x, y, d) in [
        (1, 1, 6),
        (1, 2, 6),
        (1, 2, 4),
        (2, 2, 4),
        (2, 1, 2),
        (2, 2, 2),
    ] {
        let mut e = entity("fences", x, y, 50 + zoo.entities.len() as u32);
        e.common.as_mut().unwrap().rotation = d;
        zoo.entities.push(e);
    }
    World::new(zoo, shape)
}

fn record(advice: &[(u32, bool)]) -> Record {
    let mut r = Record::default();
    for &(b, important) in advice {
        r.advice |= 1 << b;
        r.important |= u64::from(important) << b;
    }
    r
}

#[test]
fn popularity_picks_one_of_eleven_pictures_by_halves() {
    let pictures: Vec<usize> = [0, 9, 10, 19, 20, 49, 50, 99, 100].map(stars).to_vec();
    assert_eq!(pictures, [0, 0, 1, 1, 2, 4, 5, 9, 10]);
    assert_eq!(stars(u32::MAX), 10, "huge");
}

#[test]
fn the_keeper_kind_is_the_scientists_or_the_marine_specialists_else_the_zookeepers() {
    let kinds: Vec<usize> = [9500, 9551, 9552, 0, -1, 9553, i32::MAX]
        .map(keeper_kind)
        .to_vec();
    assert_eq!(kinds, [0, 1, 2, 0, 0, 0, 0]);
}

#[test]
fn the_important_lines_come_first_and_a_female_reads_the_other_range() {
    let r = record(&[
        (bit::FEW_ROCKS, false),
        (bit::NEEDS_SHELTER, true),
        (bit::TOO_FLAT, false),
        (bit::terrain(2), true),
    ]);
    assert_eq!(
        recommendations(Some(&r), false),
        [
            (9900 + 1 + 25, true),
            (9900 + 1 + 58, true),
            (9900 + 1 + 17, false),
            (9900 + 1 + 19, false)
        ]
    );
    assert_eq!(recommendations(Some(&r), true)[0], (39900 + 1 + 25, true));
    let all = record(&(0..64).map(|b| (b, b % 2 == 0)).collect::<Vec<_>>());
    let lines = recommendations(Some(&all), false);
    assert_eq!(lines.len(), 64);
    assert_eq!(
        lines[0],
        (9901, true),
        "the first important line is bit 0's"
    );
    assert_eq!(lines[63], (9900 + 1 + 63, false));
}

#[test]
fn an_exhibit_with_nothing_to_say_is_well_suited() {
    assert_eq!(
        recommendations(Some(&Record::default()), false),
        [(9900, false)]
    );
    assert_eq!(recommendations(None, true), [(39900, false)]);
}

#[test]
fn a_forced_refresh_is_at_once_and_the_next_call_refreshes_again() {
    let mut c = Clock::default();
    assert!(c.due(500, true), "the first call: due 0 has passed");
    assert!(!c.due(501, true));
    assert!(!c.due(PERIOD + 499, true));
    assert!(c.due(PERIOD + 500, true));
    c.force = true;
    assert!(c.due(PERIOD + 501, false), "forced, paused or not");
    assert!(
        c.due(PERIOD + 502, true),
        "the quirk: the due time is the period itself"
    );
    assert!(!c.due(PERIOD + 503, true));
}

#[test]
fn nothing_refreshes_by_the_clock_while_not_ticking_and_a_huge_time_does_not_overflow() {
    let mut c = Clock::default();
    assert!(!c.due(u64::MAX, false), "paused");
    assert!(c.due(u64::MAX, true));
    c.due(u64::MAX, true); // the due time saturates
}

#[test]
fn the_animals_of_an_exhibit_are_those_on_its_tiles() {
    let w = world();
    let id = w
        .exhibits()
        .list()
        .iter()
        .find(|e| e.id != GROUNDS)
        .unwrap()
        .id;
    assert_eq!(w.exhibits().list().len(), 1);
    let names: Vec<_> = animals_in(&w, id)
        .into_iter()
        .map(|at| w.zoo.entities[at].common.as_ref().unwrap().name.clone())
        .collect();
    assert_eq!(
        names,
        ["e3"],
        "not the one on the grounds, off the map or the tree"
    );
    assert!(animals_in(&w, 99).is_empty());
    let at = |x: usize| {
        w.zoo
            .entities
            .iter()
            .position(|e| e.common.as_ref().is_some_and(|c| c.name == format!("e{x}")))
            .unwrap()
    };
    assert_eq!(animals_exhibit(&w, at(3)), Some(id));
    assert_eq!(animals_exhibit(&w, at(4)), None, "the grounds");
    assert_eq!(animals_exhibit(&w, at(5)), None, "off the map");
    assert_eq!(animals_exhibit(&w, 999), None, "no such entity");
}

#[test]
fn an_exhibit_records_the_date_it_was_made() {
    let mut w = world();
    let id = w.exhibits().list()[0].id;
    assert_eq!(w.exhibits().get(id).unwrap().stats.created, None);
    let date = Date {
        year: 2001,
        month: 3,
        day: 9,
        weekday: 4,
        hour: 0,
        minute: 0,
        second: 0,
        millisecond: 0,
    };
    w.set_created(id, date);
    assert_eq!(w.exhibits().get(id).unwrap().stats.created, Some(date));
    w.set_created(99, date); // no such exhibit
}

#[test]
fn an_animals_list_icon_is_the_first_of_its_subtype_default_or_plain_section() {
    let ty = |ini: &str, subtype: &[u8]| EntityType {
        class: "animals",
        type_name: b"lion".to_vec(),
        subtype: subtype.to_vec(),
        file: None,
        config: Some(std::sync::Arc::new(Config::parse(ini.as_bytes()).unwrap())),
        name_id: 0,
        footprint: [1, 1, 1],
        height: 0,
    };
    let ini = "[Global]\nDefaultSubtype=m\n[m/Icon]\nIcon=a/m1\nIcon=a/m2\n[f/Icon]\nIcon=a/f\n[Icon]\nIcon=a/plain\n";
    assert_eq!(list_icon(&ty(ini, b"f")).as_deref(), Some("a/f"));
    assert_eq!(
        list_icon(&ty(ini, b"c")).as_deref(),
        Some("a/m1"),
        "the default subtype"
    );
    assert_eq!(
        list_icon(&ty("[Icon]\nIcon=a/plain\n", b"c")).as_deref(),
        Some("a/plain")
    );
    assert_eq!(list_icon(&ty("", b"c")), None);
    let mut none = ty("", b"c");
    none.config = None;
    assert_eq!(list_icon(&none), None);
}
