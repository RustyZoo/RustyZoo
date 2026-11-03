//! The numbers of `shows.cfg`, which show a grandstand belongs to, and the emptying routines.

mod audience;
mod call;
mod cycle;
mod performer;
mod relay;
mod validate;

use super::*;
use crate::building::Live;
use crate::config::Config;
use crate::entity::EntityType;
use crate::unit::testing::{centre, entity, world_with};
use crate::zoo::{HabitatInfo, Show, ShowInfo};
use std::sync::Arc;

/// A grandstand: 12 places in two columns, a slot of 12, users that sit and use the exit.
fn stand() -> String {
    let mut text = "[Characteristics/Floats]\ncUpkeep=15\n[Characteristics/Integers]\ncCommerce=1\ncCapacity=12\ncUserStaysOutside=0\n\
        cUserUsesExit=1\ncHoldsOntoUser=1\n[Satisfies]\nbuilding\ngrandstand\n[slots]\nname=slot\n\
        [slot]\nslotpos=0\nslotpos=0\nentrpos=0\nentrpos=144\nexitpos=0\nexitpos=144\ncapacity=12\n\
        cHasDrawSlots=drawslots\n[drawslots]\n"
        .to_string();
    for place in 0..12 {
        let x = if place % 2 == 0 { -13 } else { 13 };
        text += &format!(
            "p={x}\np={}\np={}\n",
            -102 + 32 * (place / 2),
            50 - 9 * (place / 2)
        );
    }
    text += "order=ordern\n[ordern]\n";
    (0..12).for_each(|o| text += &format!("o={o}\n"));
    text
}

fn kind(class: &'static str, name: &str, text: &str) -> EntityType {
    EntityType {
        class,
        type_name: name.as_bytes().to_vec(),
        subtype: Vec::new(),
        file: None,
        config: Some(Arc::new(Config::parse(text.as_bytes()).unwrap())),
        name_id: 0,
        footprint: [2, 2, 1],
        height: 0,
    }
}

fn types() -> Catalogue {
    let mut cat = Catalogue::default();
    cat.push(kind("building", "stand", &stand()));
    cat.push(kind(
        "fences",
        "glass",
        "[Characteristics/Integers]\ncHeight=3\ncSeeThrough=1\n",
    ));
    cat.push(kind(
        "fences",
        "iron",
        "[Characteristics/Integers]\ncHeight=3\n",
    ));
    cat
}

fn info(id: u16) -> ShowInfo {
    ShowInfo {
        frequency: 90,
        index: 0,
        schedule: Vec::new(),
        records: Vec::new(),
        events: Vec::new(),
        next: 0,
        id,
        upkeep: [0; 3],
        receipts: [0; 3],
        attendance: [0; 3],
        show: Show::default(),
    }
}

/// A map `width` tiles wide and 5 high whose columns from 5 on are an exhibit with a show of id
/// 1001, behind a fence of type `wall`. The stands are given as (id, tile, rotation); guests 1 to 3
/// stand at the top left.
fn world(width: i32, wall: &str, stands: &[(u32, (i32, i32), u32)]) -> (World, Catalogue) {
    world_in(width, wall, stands, true)
}

/// [`world`] with the fence entities read before the stands (`walls_first`) or after them.
fn world_in(
    width: i32,
    wall: &str,
    stands: &[(u32, (i32, i32), u32)],
    walls_first: bool,
) -> (World, Catalogue) {
    let cat = types();
    let w = world_with((width, 5), |z| {
        let walls = |z: &mut crate::zoo::Zoo| {
            for y in 0..5 {
                let at = (5 * 64, y * 64 + 32);
                z.entities
                    .push(entity("fences", wall, 500 + y as u32, at, 6));
            }
        };
        if walls_first {
            walls(z);
        }
        for &(id, tile, rotation) in stands {
            z.entities
                .push(entity("building", "stand", id, centre(tile), rotation));
        }
        if !walls_first {
            walls(z);
        }
        for id in 1..=3 {
            z.entities
                .push(entity("guests", "man", id, centre((id as i32, 0)), 0));
        }
        z.habitat_tiles = vec![(5, 0)];
        z.habitat_gates = vec![((5, 0), 0)];
        z.habitat_info = vec![HabitatInfo {
            name: String::new(),
            stats: [0; 9],
            created: 0,
            viewing: 0,
            serviced: 0,
        }];
        z.shows.infos = vec![Some(info(1001))];
    });
    let mut w = w;
    let stand = cat.types.iter().find(|t| t.type_name == b"stand").unwrap();
    for &(id, ..) in stands {
        crate::building::built(&mut w, stand, id, 0);
    }
    (w, cat)
}

fn show_of(w: &World, id: u32) -> u16 {
    w.units.buildings[&id].show_id
}

#[test]
fn shows_cfg_gives_the_thresholds_and_a_key_that_is_missing_keeps_the_built_in_number() {
    assert_eq!(Params::default().trick, [0, 3, 6]);
    let read = |text: &str| Params::read(&Config::parse(text.as_bytes()).unwrap());
    assert_eq!(read(""), Params::default());
    let full = read(
        "[trickSatisfactionThresholds]\ngreatTrick=8\ngoodTrick=4\nbadTrick=0\n\
         [show]\nminIdealLength=5\nmaxIdealLength=7\n\
         [showSatisfactionThresholds]\ngreatShow=100\ngoodShow=66\nbadShow=33\n",
    );
    assert_eq!(
        (full.trick, full.ideal, full.show),
        ([0, 4, 8], [5, 7], [33, 66, 100])
    );
    // One key alone, a negative number, a huge one and a word (which reads 0): nothing panics.
    let one = read(
        "[show]\nmaxIdealLength=-3\n[showSatisfactionThresholds]\ngoodShow=99999999999\nbadShow=x\n",
    );
    assert_eq!(one.ideal, [6, -3]);
    assert_eq!(one.show[0], 0);
    assert_eq!(read("[show]\nminIdealLength=0\n").ideal, [0, 6]);
}

#[test]
fn a_grandstand_four_tiles_from_a_show_tank_with_a_clear_view_takes_its_show_id() {
    let (mut w, cat) = world(9, "glass", &[(7, (1, 2), 2)]);
    assert_eq!(show_of(&w, 7), 0);
    associate(&mut w, &cat);
    assert_eq!(show_of(&w, 7), 1001);
}

#[test]
fn a_grandstand_turned_away_or_at_an_odd_angle_belongs_to_no_show() {
    for (tile, rotation) in [(1, 0), (1, 4), (1, 6), (3, 7), (1, 1)] {
        let (mut w, cat) = world(9, "glass", &[(7, (tile, 2), rotation)]);
        associate(&mut w, &cat);
        assert_eq!(show_of(&w, 7), 0, "{tile} {rotation}");
    }
    // Facing right from the first column the tile four on is still outside the exhibit.
    let (mut w, cat) = world(9, "glass", &[(7, (0, 2), 2)]);
    associate(&mut w, &cat);
    assert_eq!(show_of(&w, 7), 0);
}

#[test]
fn a_wall_that_blocks_the_view_between_three_and_five_tiles_in_front_leaves_no_show() {
    let (mut w, cat) = world(9, "iron", &[(7, (1, 2), 2)]);
    associate(&mut w, &cat);
    assert_eq!(show_of(&w, 7), 0);
}

#[test]
fn the_tile_five_in_front_off_the_map_does_not_face() {
    // Four tiles in front of (1, 2) is in the exhibit in both maps; five in front is on the
    // map of 7 columns and off the map of 6.
    for (width, show) in [(7, 1001), (6, 0)] {
        let (mut w, cat) = world(width, "glass", &[(7, (1, 2), 2)]);
        associate(&mut w, &cat);
        assert_eq!(show_of(&w, 7), show, "{width}");
    }
}

#[test]
fn a_grandstand_keeps_the_show_it_had_when_it_no_longer_faces_a_tank() {
    let (mut w, cat) = world(9, "glass", &[(7, (1, 2), 2)]);
    associate(&mut w, &cat);
    let at = w.find(7).unwrap();
    w.zoo.entities[at].common.as_mut().unwrap().rotation = 6;
    associate(&mut w, &cat);
    assert_eq!(show_of(&w, 7), 1001);
}

#[test]
fn placing_a_building_that_is_not_a_grandstand_leaves_the_show_ids() {
    // Stand 7 faces the tank but has no id yet: a bench set down runs no pass, a grandstand does.
    let (mut w, mut cat) = world(9, "glass", &[(7, (1, 2), 2), (8, (1, 0), 4)]);
    cat.push(kind("building", "bench", "[Satisfies]\nbuilding\n"));
    w.zoo
        .entities
        .push(entity("building", "bench", 9, centre((0, 4)), 0));
    added(&mut w, &cat, 9);
    added(&mut w, &cat, 404);
    assert_eq!(show_of(&w, 7), 0);
    added(&mut w, &cat, 8);
    assert_eq!(show_of(&w, 7), 1001);
}

#[test]
fn a_grandstand_read_before_the_wall_that_blocks_its_view_keeps_the_show_it_got() {
    // Stand 7 is the first entity: the pass run as it is read sees no wall yet. Once the zoo
    // is read the full pass finds none, but the id stays.
    let (mut w, cat) = world_in(9, "iron", &[(7, (1, 2), 2)], false);
    loaded(&mut w, &cat, 0);
    assert_eq!(show_of(&w, 7), 1001);
    associate(&mut w, &cat);
    assert_eq!(show_of(&w, 7), 1001);
    // The wall read first blocks it.
    let (mut w, cat) = world(9, "iron", &[(7, (1, 2), 2)]);
    loaded(&mut w, &cat, 5);
    assert_eq!(show_of(&w, 7), 0);
}

#[test]
fn a_zoo_load_gives_a_grandstand_the_show_it_faced_when_it_was_read() {
    // The wall comes after the stand in the file: the pass at the stand finds the show; the
    // pass after the load finds none and leaves the id.
    let (mut w, cat) = world_in(9, "iron", &[(7, (1, 2), 2)], false);
    let mut sim = crate::sim::Sim::new(b"");
    sim.load(&mut w, &cat);
    assert_eq!(show_of(&w, 7), 1001);
    // With the wall read first the stand never faces the show.
    let (mut w, cat) = world(9, "iron", &[(7, (1, 2), 2)]);
    sim.load(&mut w, &cat);
    assert_eq!(show_of(&w, 7), 0);
}

#[test]
fn reading_an_entity_that_is_not_a_grandstand_runs_no_pass() {
    let (mut w, cat) = world_in(9, "glass", &[(7, (1, 2), 2)], false);
    loaded(&mut w, &cat, 1);
    assert_eq!(show_of(&w, 7), 0);
    loaded(&mut w, &cat, 0);
    assert_eq!(show_of(&w, 7), 1001);
}

#[test]
fn a_show_info_without_a_show_id_is_not_a_show() {
    let (mut w, cat) = world(9, "glass", &[(7, (1, 2), 2)]);
    w.zoo.shows.infos = vec![Some(info(0))];
    associate(&mut w, &cat);
    assert_eq!(show_of(&w, 7), 0);
    w.zoo.shows.infos = vec![None];
    associate(&mut w, &cat);
    assert_eq!(show_of(&w, 7), 0);
}

/// Stands 7 (facing the exhibit) and 8 (turned away), each with guests as users: 7 holds 1 and 2,
/// 8 holds 3; all three flags are up.
fn crowded() -> (World, Catalogue) {
    let (mut w, cat) = world(9, "glass", &[(7, (1, 2), 2), (8, (1, 0), 4)]);
    associate(&mut w, &cat);
    for (id, users) in [(7, vec![1, 2]), (8, vec![3])] {
        let live = w.units.buildings.get_mut(&id).unwrap();
        live.users = users.into_iter().map(|u| (u, 0, 0)).collect();
        live.fill = 1;
    }
    w.units.buildings.get_mut(&8).unwrap().show_id = 1001;
    (w, cat)
}

#[test]
fn emptying_a_show_removes_every_user_of_the_grandstands_that_face_its_tank() {
    let (mut w, cat) = crowded();
    empty_show_grandstands(&mut w, &cat, 1001);
    let (a, b) = (&w.units.buildings[&7], &w.units.buildings[&8]);
    assert!(a.users.is_empty() && a.fill == 0);
    // The grandstand turned away keeps its guest and its flag, whatever show id it holds.
    assert_eq!((b.users.len(), b.fill), (1, 1));
    // The guests were put at the exit of the slot, outside.
    let at = w.find(1).unwrap();
    let c = w.zoo.entities[at].common.as_ref().unwrap();
    assert_eq!((c.x, c.y), (240, 160));
}

/// The targets of the abort events posted so far, in posting order.
fn aborts(w: &World) -> Vec<u32> {
    (w.units.posts.iter())
        .filter(|e| e.kind == 0x2722)
        .map(|e| e.target)
        .collect()
}

#[test]
fn two_users_of_a_building_get_one_abort_event_each_in_entering_order() {
    let (mut w, cat) = crowded();
    // Entering order, not id order.
    w.units.buildings.get_mut(&7).unwrap().users = vec![(2, 0, 0), (1, 0, 0)];
    w.units.posts.clear();
    crate::building::leave::push_out(&mut w, &cat, 7);
    assert_eq!(aborts(&w), [2, 1]);
}

#[test]
fn a_user_listed_by_the_live_list_and_the_record_gets_one_abort_event() {
    let (mut w, cat) = crowded();
    let at = w.find(7).unwrap();
    let mut b = crate::zoo::fresh::building_new(5.0, 0);
    b.users = vec![(1, 0)];
    w.zoo.entities[at].common.as_mut().unwrap().building = Some(b);
    w.units.posts.clear();
    crate::building::leave::push_out(&mut w, &cat, 7);
    assert_eq!(aborts(&w), [1, 2]);
}

#[test]
fn emptying_a_show_that_has_no_info_or_no_tank_does_nothing() {
    let (mut w, cat) = crowded();
    empty_show_grandstands(&mut w, &cat, 1002);
    empty_show_grandstands(&mut w, &cat, 0);
    assert_eq!(w.units.buildings[&7].users.len(), 2);
    w.zoo.habitat_tiles = vec![(0, 0)];
    empty_show_grandstands(&mut w, &cat, 1001);
    assert_eq!(w.units.buildings[&7].users.len(), 2);
}

#[test]
fn emptying_a_grandstand_lowers_its_flag_and_empties_its_show() {
    let (mut w, cat) = crowded();
    empty_grandstands(&mut w, &cat, &mut Events::default(), 7);
    assert!(w.units.buildings[&7].users.is_empty() && w.units.buildings[&7].fill == 0);
    // A grandstand that holds the show but faces away loses its flag and keeps its users; the
    // grandstands that face the tank are emptied all the same.
    let (mut w, cat) = crowded();
    empty_grandstands(&mut w, &cat, &mut Events::default(), 8);
    let (a, b) = (&w.units.buildings[&7], &w.units.buildings[&8]);
    assert_eq!((a.users.len(), b.users.len(), b.fill), (0, 1, 0));
}

#[test]
fn emptying_a_grandstand_with_no_show_leaves_its_users() {
    let (mut w, cat) = crowded();
    w.units.buildings.get_mut(&7).unwrap().show_id = 0;
    empty_grandstands(&mut w, &cat, &mut Events::default(), 7);
    let live = &w.units.buildings[&7];
    assert_eq!((live.users.len(), live.fill), (2, 1));
    // A building that is no grandstand, and an id that is nothing.
    empty_grandstands(&mut w, &cat, &mut Events::default(), 1);
    empty_grandstands(&mut w, &cat, &mut Events::default(), 99);
}

#[test]
fn a_grandstand_of_a_show_whose_info_is_gone_is_not_emptied() {
    let (mut w, cat) = crowded();
    w.zoo.shows.infos = vec![None];
    empty_grandstands(&mut w, &cat, &mut Events::default(), 7);
    let live = &w.units.buildings[&7];
    assert_eq!((live.users.len(), live.fill), (2, 1));
}

#[test]
fn a_guest_leaving_a_grandstand_of_a_show_books_no_concessions() {
    let (mut w, cat) = crowded();
    let live = |w: &mut World, id: u32, show: u16| {
        w.units.buildings.insert(
            id,
            Live {
                show_id: show,
                ..Live::default()
            },
        );
    };
    for (show, booked) in [(1001, 0), (0, 1)] {
        live(&mut w, 7, show);
        w.units.buildings.get_mut(&7).unwrap().users = vec![(1, 0, 0)];
        w.units.buildings.get_mut(&7).unwrap().price = 5.0;
        w.units.concessions.clear();
        crate::building::leave::push_out(&mut w, &cat, 7);
        assert_eq!(w.units.concessions.len(), booked, "show {show}");
        // The building's own figures count the income either way.
        assert_eq!(w.units.buildings[&7].income[0], 5.0, "show {show}");
    }
}

#[test]
fn a_loaded_zoo_gives_its_grandstands_their_show_and_their_flag_before_the_first_step() {
    let (mut w, cat) = world(9, "glass", &[(7, (1, 2), 2), (8, (1, 0), 4)]);
    w.units.buildings.clear();
    w.zoo.version = 106;
    for id in [7, 8] {
        let mut b = crate::zoo::fresh::building_new(5.0, 0);
        b.fill = 1;
        b.users = (id == 7).then_some((1, 0)).into_iter().collect();
        b.users_now = b.users.len() as u32;
        let at = w.find(id).unwrap();
        w.zoo.entities[at].common.as_mut().unwrap().building = Some(b);
    }
    crate::sim::Sim::new(b"").load(&mut w, &cat);
    assert_eq!((show_of(&w, 7), show_of(&w, 8)), (1001, 0));
    assert_eq!(
        (w.units.buildings[&7].fill, w.units.buildings[&8].fill),
        (1, 1)
    );
    // The guest the record lists sits on a draw place.
    assert_eq!(w.units.buildings[&7].seats.len(), 1);
}

#[test]
fn a_saved_grandstand_stores_its_live_show_soon_flag_as_the_fill_word() {
    let (mut w, cat) = world(9, "glass", &[(7, (1, 2), 2)]);
    let at = w.find(7).unwrap();
    w.zoo.entities[at].common.as_mut().unwrap().building =
        Some(crate::zoo::fresh::building_new(5.0, 0));
    let fill = |w: &World| {
        let mut zoo = w.saved(|_| true);
        w.finish_save(&mut zoo, &cat);
        zoo.entities[at]
            .common
            .as_ref()
            .unwrap()
            .building
            .as_ref()
            .unwrap()
            .fill
    };
    assert_eq!(fill(&w), 0);
    w.units.buildings.get_mut(&7).unwrap().fill = 1;
    assert_eq!(fill(&w), 1);
}
