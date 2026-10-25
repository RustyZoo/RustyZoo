use super::*;
use crate::config::Config;
use crate::render::order::Kind as Class;
use crate::unit::Staff;
use crate::world::{Shape, TankKind};
use crate::zoo::{Clocks, Common, Entity, Tile, View, Zoo};
use std::sync::Arc;

fn entity(class: &str, type_name: &str, (tx, ty): (i32, i32), rotation: u32, id: u32) -> Entity {
    Entity {
        class: class.to_string(),
        type_name: type_name.to_string(),
        subtype: "f".to_string(),
        body_size: None,
        common: Some(Common {
            x: tx * 64 + 32,
            y: ty * 64 + 32,
            z: 0,
            rotation,
            id,
            name: String::new(),
            colors: Vec::new(),
            fence: None,
            ..Default::default()
        }),
    }
}

fn shape(e: &Entity) -> Option<Shape> {
    let fence = e.class == "fences";
    let unit = matches!(e.class.as_str(), "animals" | "keeper" | "maint" | "tour");
    (fence || unit).then_some(Shape {
        kind: if fence { Class::Edge } else { Class::Unit },
        footprint: (1, 1),
        fence,
        wall: false,
        gate: false,
        gated: fence,
        walkable: (false, false),
        tank: TankKind::NONE,
    })
}

fn ty(class: &'static str, name: &str, id: i32, keys: &str) -> EntityType {
    let config = format!("[f/Characteristics/Integers]\n{keys}\n");
    EntityType {
        class,
        type_name: name.as_bytes().to_vec(),
        subtype: b"f".to_vec(),
        file: None,
        config: Some(Arc::new(Config::parse(config.as_bytes()).unwrap())),
        name_id: id,
        footprint: [1, 1, 1],
        height: 0,
    }
}

fn catalogue() -> Catalogue {
    let mut cat = Catalogue::default();
    for t in [
        ty("animals", "zebra", 1, ""),
        ty("animals", "dino", 2, "cKeeper=9551"),
        ty("keeper", "keeper", 9500, "cDirt=6062\ncDirt=6093"),
        ty("objects", "other", 6062, ""),
        ty("animals", "frog", 3, "cUnderwater=1"),
        ty("keeper", "scient", 9551, ""),
        ty("keeper", "trainer", 9552, "cUnderwater=1"),
        ty("maint", "maint", 9501, ""),
        ty("tour", "tour", 9502, ""),
    ] {
        cat.push(t);
    }
    cat
}

/// An 8 by 8 map with four exhibits: a 2 by 2 of a zebra at (1, 1), one tile at (4, 1), an empty
/// 2 by 2 at (1, 4) and a 2 by 2 of a dinosaur at (4, 4); the staff of ids 1 to 5 stand on the
/// grounds: zookeeper, scientist, marine specialist, worker, guide.
fn world() -> (World, [u32; 4]) {
    let mut entities = Vec::new();
    let mut id = 100;
    for (x0, y0, n) in [(1, 1, 2), (4, 1, 1), (1, 4, 2), (4, 4, 2)] {
        for i in 0..n {
            for (at, d) in [
                ((x0 + i, y0), 0),
                ((x0 + i, y0 + n - 1), 4),
                ((x0, y0 + i), 6),
                ((x0 + n - 1, y0 + i), 2),
            ] {
                id += 1;
                entities.push(entity("fences", "f", at, d, id));
            }
        }
    }
    entities.push(entity("animals", "zebra", (1, 1), 0, 50));
    entities.push(entity("animals", "dino", (4, 4), 0, 51));
    for (n, (class, name)) in [
        ("keeper", "keeper"),
        ("keeper", "scient"),
        ("keeper", "trainer"),
        ("maint", "maint"),
        ("tour", "tour"),
    ]
    .into_iter()
    .enumerate()
    {
        entities.push(entity(class, name, (7, n as i32), 0, 1 + n as u32));
    }
    let zoo = Zoo {
        kept: Default::default(),
        version: 0,
        locale: 0,
        expansion: 0,
        width: 8,
        height: 8,
        entrance: (7, 7),
        habitat_tiles: Vec::new(),
        habitat_gates: Vec::new(),
        habitat_info: Vec::new(),
        tanks: Vec::new(),
        id_counter: 10_000,
        tiles: vec![
            Tile {
                elevation: 0,
                shape: 0,
                terrain: 0,
                edges: 0
            };
            64
        ],
        entities,
        view: View::default(),
        clocks: Clocks::default(),
        names: Default::default(),
    };
    let w = World::new(zoo, shape);
    let at = |x: usize, y: usize| w.exhibits().region(y * 8 + x).unwrap();
    let ids = [at(1, 1), at(4, 1), at(1, 4), at(4, 4)];
    (w, ids)
}

const KEEPER: u32 = 1;
const SCIENTIST: u32 = 2;
const MARINE: u32 = 3;
const WORKER: u32 = 4;
const GUIDE: u32 = 5;

fn code(w: &World, cat: &Catalogue, who: u32, exhibit: u32) -> u8 {
    let at = w.find(who).unwrap();
    block_service(
        w,
        cat,
        exhibit,
        type_of(cat, &w.zoo.entities[at]).unwrap(),
        (true, true),
    )
}

fn list(w: &World, who: u32) -> Vec<u32> {
    match &w.units.get(who).unwrap().part {
        Part::Staff(Staff { assigned, .. }) => assigned.clone(),
        _ => panic!("not staff"),
    }
}

#[test]
fn a_keeper_may_serve_an_exhibit_with_animals_of_his_kind_that_has_two_tiles() {
    let (w, [zebra, small, empty, dino]) = world();
    let cat = catalogue();
    assert_eq!(w.exhibits().get(small).unwrap().tiles, 1);
    assert_eq!(code(&w, &cat, KEEPER, zebra), 0);
    assert_eq!(code(&w, &cat, SCIENTIST, dino), 0);
    // Too small first, whoever the keeper; then nothing for his kind, an empty exhibit included.
    assert_eq!(code(&w, &cat, KEEPER, small), 1);
    assert_eq!(code(&w, &cat, KEEPER, empty), 2);
    assert_eq!(code(&w, &cat, KEEPER, dino), 2);
    assert_eq!(code(&w, &cat, SCIENTIST, zebra), 2);
    // A land exhibit is no work for a marine specialist.
    assert_eq!(code(&w, &cat, MARINE, zebra), 2);
    // Anyone who is not a keeper passes the test at once.
    for who in [WORKER, GUIDE] {
        assert_eq!([small, empty].map(|x| code(&w, &cat, who, x)), [0, 0]);
    }
    assert_eq!(code(&w, &cat, KEEPER, 999), 0, "no such exhibit");
}

#[test]
fn a_click_appends_the_exhibit_once_and_a_refusal_changes_nothing_and_says_why() {
    let (mut w, [zebra, small, empty, dino]) = world();
    let cat = catalogue();
    assert_eq!(assign(&mut w, &cat, KEEPER, zebra), Outcome::Assigned);
    assert_eq!(assign(&mut w, &cat, GUIDE, small), Outcome::Assigned);
    assert_eq!(assign(&mut w, &cat, GUIDE, zebra), Outcome::Assigned);
    assert_eq!(list(&w, GUIDE), [small, zebra], "the oldest first");
    assert_eq!(assign(&mut w, &cat, GUIDE, small), Outcome::Assigned);
    assert_eq!(list(&w, GUIDE), [small, zebra], "no duplicate, no move");
    let refused = |w: &mut World, who, x| assign(w, &cat, who, x);
    assert_eq!(
        refused(&mut w, KEEPER, small),
        Outcome::Refused(Some(10323))
    );
    // Zookeeper: 10320 for an exhibit with species, 10329 for an empty one.
    assert_eq!(refused(&mut w, KEEPER, dino), Outcome::Refused(Some(10320)));
    assert_eq!(
        refused(&mut w, KEEPER, empty),
        Outcome::Refused(Some(10329))
    );
    assert_eq!(
        refused(&mut w, SCIENTIST, zebra),
        Outcome::Refused(Some(10324))
    );
    assert_eq!(
        refused(&mut w, SCIENTIST, empty),
        Outcome::Refused(Some(10329))
    );
    // The marine keeper: 10328 for any land exhibit (only an empty tank gets 10329).
    assert_eq!(
        refused(&mut w, MARINE, zebra),
        Outcome::Refused(Some(10328))
    );
    assert_eq!(
        refused(&mut w, MARINE, empty),
        Outcome::Refused(Some(10328))
    );
    assert_eq!(list(&w, KEEPER), [zebra]);
    assert_eq!(list(&w, SCIENTIST), Vec::<u32>::new());
    // The worker never takes a list; no unit, no assignment.
    assert_eq!(refused(&mut w, 999, zebra), Outcome::Refused(None));
}

#[test]
fn removing_one_or_all_and_a_vanished_exhibit_empty_the_lists() {
    let (mut w, [zebra, _, _, dino]) = world();
    let cat = catalogue();
    assign(&mut w, &cat, KEEPER, zebra);
    assign(&mut w, &cat, GUIDE, zebra);
    assign(&mut w, &cat, GUIDE, dino);
    // Row 1 of the guide's list is the dinosaur exhibit; a row past the end does nothing.
    unassign(&mut w, GUIDE, Some(9));
    unassign(&mut w, GUIDE, Some(1));
    assert_eq!(list(&w, GUIDE), [zebra]);
    unassign(&mut w, KEEPER, None);
    assert_eq!(list(&w, KEEPER), Vec::<u32>::new());
    clear_habitat(&mut w, zebra);
    assert_eq!(list(&w, GUIDE), Vec::<u32>::new());
    // An id that is no unit or no staff changes nothing.
    unassign(&mut w, 999, None);
    unassign(&mut w, 50, None);
    clear_habitat(&mut w, 999);
}

#[test]
fn the_keeper_assigned_flag_is_read_from_the_lists_of_the_keeper_class_only() {
    let (mut w, [zebra, _, _, dino]) = world();
    let cat = catalogue();
    let flag = |w: &World, x| crate::exhibit::rating::input(w, &cat, x).keeper;
    assign(&mut w, &cat, GUIDE, zebra);
    assign(&mut w, &cat, KEEPER, zebra);
    assign(&mut w, &cat, GUIDE, dino);
    assert_eq!((flag(&w, zebra), flag(&w, dino)), (Some(KEEPER), None));
    unassign(&mut w, KEEPER, None);
    assert_eq!(flag(&w, zebra), None);
}

fn fixed() -> Shape {
    Shape {
        kind: Class::Fixed,
        footprint: (1, 1),
        fence: false,
        wall: false,
        gate: false,
        gated: false,
        walkable: (false, false),
        tank: TankKind::NONE,
    }
}

#[test]
fn his_dung_in_a_centre_slot_and_an_amphibious_neighbour_are_reasons_to_go() {
    let (mut w, [_, small, empty, _]) = world();
    let cat = catalogue();
    let code = |w: &World, flags| {
        let at = w.find(KEEPER).unwrap();
        let ty = type_of(&cat, &w.zoo.entities[at]).unwrap();
        block_service(w, &cat, empty, ty, flags)
    };
    assert_eq!(code(&w, (true, true)), 2);
    // Dung off the centre of a tile is not in the centre slot; at the centre it is.
    let mut pile = entity("objects", "other", (1, 4), 0, 300);
    pile.common.as_mut().unwrap().x -= 24;
    let off = w.add(pile, Some(fixed())).unwrap();
    assert_eq!(code(&w, (true, true)), 2, "not the centre slot");
    let centre = w.add(entity("objects", "other", (2, 4), 0, 301), Some(fixed()));
    assert_eq!(code(&w, (true, true)), 0);
    let scientist = type_of(&cat, &w.zoo.entities[w.find(SCIENTIST).unwrap()]).unwrap();
    assert_eq!(
        block_service(&w, &cat, empty, scientist, (true, true)),
        2,
        "not his dung"
    );
    // An amphibious animal in a neighbour counts whatever `kindOnly` says, with `checkKind` only.
    w.remove(off);
    w.remove(centre.unwrap());
    w.add(entity("animals", "zebra", (4, 1), 0, 302), None);
    // Edits make the links again, so the link is set after them.
    w.exhibits_mut().get_mut(empty).unwrap().amphibious = vec![small];
    assert_eq!(code(&w, (true, true)), 2, "a zebra is no amphibious animal");
    w.add(entity("animals", "frog", (4, 1), 0, 303), None);
    w.exhibits_mut().get_mut(empty).unwrap().amphibious = vec![small];
    assert_eq!(
        [(true, true), (false, true), (true, false)].map(|f| code(&w, f)),
        [0, 0, 2]
    );
}

#[test]
fn the_purity_of_a_tank_counts_without_kind_only() {
    let mut w = crate::unit::testing::tank_world(2, 1, &[]);
    let cat = catalogue();
    let mut marine = entity("keeper", "trainer", (0, 0), 0, 9);
    marine.subtype = "f".into();
    let marine = w.add(marine, None).unwrap();
    let tank = w.tank_at(2 * 12 + 2).unwrap();
    let code = |w: &World, flags| {
        let ty = type_of(&cat, &w.zoo.entities[w.find(marine).unwrap()]).unwrap();
        block_service(w, &cat, tank, ty, flags)
    };
    // Clean water: nothing to do. Murky water: a reason, but not for the assign mode.
    assert_eq!(code(&w, (false, true)), 2);
    w.exhibits_mut()
        .get_mut(tank)
        .unwrap()
        .tank
        .as_mut()
        .unwrap()
        .purity = 99;
    assert_eq!(
        [(false, true), (true, true), (false, false)].map(|f| code(&w, f)),
        [0, 2, 2]
    );
}

#[test]
fn a_status_tick_drops_the_assigned_exhibits_that_are_too_small() {
    let (mut w, [zebra, small, ..]) = world();
    let cat = catalogue();
    for x in [small, zebra] {
        let Some(Part::Staff(s)) = w.units.get_mut(KEEPER).map(|u| &mut u.part) else {
            panic!("no keeper");
        };
        s.assigned.push(x);
    }
    let mut sim = crate::sim::Sim::new(b"");
    sim.set_seed(1);
    sim.start_game(64, Clocks::default());
    sim.step_in(&mut w, &cat, 1100, |_, _, _| {});
    assert_eq!(list(&w, KEEPER), [zebra]);
}
