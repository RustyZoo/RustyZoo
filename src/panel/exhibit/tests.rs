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
            ..Default::default()
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
        kept: Default::default(),
        version: 0,
        locale: 0,
        expansion: 0,
        width: 6,
        height: 6,
        entrance: (5, 5),
        habitat_tiles: Vec::new(),
        habitat_gates: Vec::new(),
        habitat_info: Vec::new(),
        tanks: Vec::new(),
        id_counter: 100,
        tiles: vec![tile; 36],
        entities,
        view: View::default(),
        clocks: Default::default(),
        names: Default::default(),
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

const NONE: [&[i32]; 2] = [&[], &[]];

fn record(advice: &[(u32, bool)]) -> (u64, u64) {
    let mut r = (0, 0);
    for &(b, important) in advice {
        r.0 |= 1 << b;
        r.1 |= u64::from(important) << b;
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
        recommendations(r, false, false, false, NONE),
        [
            (9900 + 1 + 25, true, None),
            (9900 + 1 + 58, true, None),
            (9900 + 1 + 17, false, None),
            (9900 + 1 + 19, false, None)
        ]
    );
    assert_eq!(
        recommendations(r, true, false, false, NONE)[0],
        (39900 + 1 + 25, true, None)
    );
    let all = record(&(0..64).map(|b| (b, b % 2 == 0)).collect::<Vec<_>>());
    let lines = recommendations(all, false, false, false, NONE);
    assert_eq!(lines.len(), 64);
    assert_eq!(
        lines[0],
        (9901, true, None),
        "the first important line is bit 0's"
    );
    assert_eq!(lines[63], (9900 + 1 + 63, false, None));
}

#[test]
fn an_exhibit_with_nothing_to_say_is_well_suited() {
    assert_eq!(
        recommendations((0, 0), false, false, false, NONE),
        [(9900, false, None)]
    );
    assert_eq!(
        recommendations((0, 0), true, false, false, NONE),
        [(39900, false, None)]
    );
}

#[test]
fn an_animal_in_a_show_tank_has_its_own_line_and_the_salinity_bit_after_it() {
    let r = record(&[
        (bit::STINK, true),
        (SALINITY, true),
        (bit::FEW_ROCKS, false),
    ]);
    assert_eq!(
        recommendations(r, false, true, false, NONE),
        [(9991, false, None), (9912, true, None)]
    );
    assert_eq!(
        recommendations(r, true, true, false, NONE),
        [(39991, false, None), (39912, true, None)]
    );
    let plain = record(&[(bit::STINK, true)]);
    assert_eq!(
        recommendations(plain, false, true, false, NONE),
        [(9991, false, None)]
    );
    assert_eq!(
        recommendations((0, 0), false, true, false, NONE),
        [(9991, false, None)]
    );
}

#[test]
fn a_forced_refresh_is_at_once_and_the_next_call_refreshes_again() {
    let mut c = Clock::default();
    assert!(c.due(500, true, PERIOD), "the first call: due 0 has passed");
    assert!(!c.due(501, true, PERIOD));
    assert!(!c.due(PERIOD + 499, true, PERIOD));
    assert!(c.due(PERIOD + 500, true, PERIOD));
    c.force = true;
    assert!(c.due(PERIOD + 501, false, PERIOD), "forced, paused or not");
    assert!(
        c.due(PERIOD + 502, true, PERIOD),
        "the quirk: the due time is the period itself"
    );
    assert!(!c.due(PERIOD + 503, true, PERIOD));
}

#[test]
fn nothing_refreshes_by_the_clock_while_not_ticking_and_a_huge_time_does_not_overflow() {
    let mut c = Clock::default();
    assert!(!c.due(u64::MAX, false, PERIOD), "paused");
    assert!(c.due(u64::MAX, true, PERIOD));
    c.due(u64::MAX, true, PERIOD); // the due time saturates
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

#[test]
fn scenery_lines_follow_the_bits_of_their_importance_and_suppress_the_well_suited_line() {
    let r = record(&[(bit::FEW_ROCKS, false), (bit::NEEDS_SHELTER, true)]);
    assert_eq!(
        recommendations(r, true, false, false, [&[7], &[8, 9]]),
        [
            (39900 + 1 + 58, true, None),
            (39990, true, Some(7)),
            (39900 + 1 + 19, false, None),
            (39990, false, Some(8)),
            (39990, false, Some(9)),
        ]
    );
    assert_eq!(
        recommendations((0, 0), false, false, false, [&[], &[4]]),
        [(9990, false, Some(4))]
    );
    // A show tank has no scenery lines.
    assert_eq!(
        recommendations((0, 0), false, true, false, [&[7], &[4]]),
        [(9991, false, None)]
    );
}

#[test]
fn an_egg_lists_its_bits_and_the_egg_line_but_no_scenery() {
    let r = record(&[(bit::FEW_ROCKS, false), (bit::NEEDS_SHELTER, true)]);
    assert_eq!(
        recommendations(r, false, false, true, [&[7], &[8]]),
        [
            (9900 + 1 + 58, true, None),
            (9900 + 1 + 19, false, None),
            (9965, false, None)
        ]
    );
    // Nothing else to say: the egg line replaces "well suited", also for a female.
    assert_eq!(
        recommendations((0, 0), true, false, true, [&[7], &[4]]),
        [(9965, false, None)]
    );
}

#[test]
fn a_tank_shows_the_picture_of_its_water_and_of_its_kind() {
    let tank = |show: bool, purity: u32| Tank {
        show,
        height: 5,
        level: 5,
        salt: false,
        target: false,
        purity,
        filled: true,
        platform: (-1, -1),
    };
    let icons = |show: bool, murky: i32| {
        [100, 60, 59, 2, 1, 0, u32::MAX]
            .map(|p| tank_icon(&tank(show, p), murky))
            .map(|s| s.rsplit('/').next().unwrap().to_string())
            .to_vec()
    };
    assert_eq!(
        icons(false, 60),
        [
            "exbtank", "exbtank", "exbdirt1", "exbdirt1", "exbdirt2", "exbdirt2", "exbtank"
        ]
    );
    assert_eq!(icons(true, 60)[2..5], ["shwdirt1", "shwdirt1", "shwdirt2"]);
    // A limit of 0 or below keeps every purity above 1 clean; a huge one makes all but 0 and 1 light.
    assert_eq!(icons(false, -3)[3], "exbtank");
    assert_eq!(icons(false, i32::MAX)[0], "exbdirt1");
}

struct NoWords;

impl crate::ui::draw::Strings for NoWords {
    fn string(&self, _: i32) -> String {
        String::new()
    }

    fn text_entry(&self, _: &[u8]) -> Vec<u8> {
        vec![]
    }
}

#[test]
fn an_exhibit_that_becomes_current_shows_the_selected_tabs_layout() {
    use crate::sprite::tests::Install;
    use crate::ui::layout::Tree;
    let tab = |id: i32| format!("[t{id}]\ntype=UIButton\nid={id}\nx=0\ny=0\ndx=4\ndy=4\n");
    let sub = |id: i32| format!("[s{id}]\ntype=UILayout\nlayout=ui/t{id}.lyt\n");
    let panel = [4310, 4313, 4311, 4312, 4365].map(tab).concat()
        + &[700, 703, 701, 702, 704].map(sub).concat();
    let mut files = vec![
        ("ui/root.lyt", b"[LayoutInfo]\nid=1\nx=0\ny=0\ndx=whole\ndy=whole\n[b]\ntype=UILayout\nlayout=ui/p.lyt\n".to_vec()),
        ("ui/p.lyt", format!("[LayoutInfo]\nid=30\nx=0\ny=0\ndx=40\ndy=40\n{panel}").into_bytes()),
    ];
    let names = [700, 703, 701, 702, 704].map(|id| format!("ui/t{id}.lyt"));
    // A sub-layout's contents start hidden, as the shipped ones do: showing it shows them.
    let bodies = [700, 703, 701, 702, 704].map(|id| {
        format!("[LayoutInfo]\nid={id}\nx=0\ny=0\ndx=10\ndy=10\n[x]\ntype=UIText\nid={}\nx=0\ny=0\ndx=4\ndy=4\nstate=1\n", id + 9000)
    });
    files.extend(
        names
            .iter()
            .zip(&bodies)
            .map(|(n, b)| (n.as_str(), b.clone().into_bytes())),
    );
    let install = Install::new("exhibit-tab-layout", &files);
    let vfs = Vfs::open(&install.0).unwrap();
    let mut tree = Tree::load(&vfs, "ui/root.lyt", 0);
    tree.resolve(100, 80, |_| 12);
    let mut ui = Interface::new(tree, Box::new(NoWords));
    ui.show(LAYOUT);
    let (w, cat) = (world(), Catalogue::default());
    let id = w
        .exhibits()
        .list()
        .iter()
        .find(|e| e.id != GROUNDS)
        .unwrap()
        .id;
    let shown =
        |ui: &Interface| [700, 703, 701, 702, 704].map(|l| ui.shown(l) && ui.shown(l + 9000));
    let mut p = Panel::default();
    ui.set_button(STATUS_TAB, true, true);
    // The panel opens with no exhibit: the status layout is hidden.
    p.opened();
    p.refresh(&mut ui, &w, &cat, &vfs);
    assert!(!ui.shown(STATUS_LAYOUT));
    // A row click makes an exhibit current: the status layout shows, with no tab click.
    p.current = Some(id);
    p.refresh(&mut ui, &w, &cat, &vfs);
    assert_eq!(shown(&ui), [true, false, false, false, false]);
    // The donations tab, remembered while no exhibit is current, is the one shown next.
    ui.set_button(STATUS_TAB, false, true);
    ui.set_button(4313, true, true);
    p.refresh(&mut ui, &w, &cat, &vfs);
    assert_eq!(
        shown(&ui),
        [true, false, false, false, false],
        "the same exhibit: unchanged"
    );
    p.current = None;
    p.refresh(&mut ui, &w, &cat, &vfs);
    p.current = Some(id);
    p.refresh(&mut ui, &w, &cat, &vfs);
    assert_eq!(shown(&ui), [false, true, false, false, false]);
}

#[test]
fn the_thoughts_about_the_exhibit_are_listed_with_the_exhibits_name_for_the_percent_s() {
    use crate::sprite::tests::Install;
    use crate::ui::layout::Tree;
    struct Look;
    impl crate::ui::draw::Strings for Look {
        fn string(&self, _: i32) -> String {
            "Look %s".into()
        }
        fn text_entry(&self, _: &[u8]) -> Vec<u8> {
            vec![]
        }
    }
    let files = [(
        "ui/root.lyt",
        b"[LayoutInfo]\nid=1\nx=0\ny=0\ndx=whole\ndy=whole\n".to_vec(),
    )];
    let install = Install::new("exhibit-thoughts", &files);
    let vfs = Vfs::open(&install.0).unwrap();
    let mut tree = Tree::load(&vfs, "ui/root.lyt", 0);
    tree.resolve(100, 80, |_| 12);
    let mut ui = Interface::new(tree, Box::new(Look));
    let (mut w, cat) = (world(), Catalogue::default());
    let id = w
        .exhibits()
        .list()
        .iter()
        .find(|e| e.id != GROUNDS)
        .unwrap()
        .id;
    let name = w.exhibits().get(id).unwrap().name.clone();
    let animal = w
        .zoo
        .entities
        .iter()
        .find(|e| e.class == "animals")
        .unwrap();
    let animal = animal.common.as_ref().unwrap().id;
    let at = w.find(animal).unwrap();
    w.zoo.entities[at].common.as_mut().unwrap().name = "Zed".into();
    // The exhibit's, then an animal's thought (the object's name wins), and one elsewhere.
    w.units.think(5, 10096, None, Some((id, None)));
    w.units.think(5, 10098, Some(animal), Some((id, None)));
    w.units.think(5, 10100, None, None);
    let texts =
        |ui: &Interface, w: &World| thought_texts(ui, w, &cat, &w.units.thoughts.in_exhibit(id));
    assert_eq!(
        texts(&ui, &w),
        ["Look Zed".to_string(), format!("Look {name}")]
    );
    let wild = &w.units.thoughts.of(5)[0].clone();
    assert_eq!(
        thought_texts(&ui, &w, &cat, &[wild]),
        ["Look %s"],
        "no object, no exhibit"
    );
    // The panel tells the ids, newest first.
    let mut p = Panel {
        current: Some(id),
        ..Panel::default()
    };
    let told = p.refresh(&mut ui, &w, &cat, &vfs);
    assert_eq!(told[3], "animals 1 thoughts 2 [10098, 10096]");
    // Twenty-five thoughts: the twenty newest.
    for _ in 0..25 {
        w.units.think(5, 10096, None, Some((id, None)));
    }
    assert_eq!(texts(&ui, &w).len(), 20);
    assert!(p.refresh(&mut ui, &w, &cat, &vfs)[3].starts_with("animals 1 thoughts 20 [10096,"));
}
