//! What a building draws of its users, and its own picture (map drawing 9.5.1).

use super::*;
use crate::building::drawn::{Drawn, drawn_users};
use crate::building::top::{take_out, yank};
use crate::goal::{ENTER_BUILDING, EXIT_BUILDING, NONE, USE};
use crate::render::Hide;

const ONE: &str = "[slots]\nname=a\n[a]\nslotpos=0\nslotpos=0\ncapacity=";
const VISIBLE: &str = "[Characteristics/Integers]\ncHideUser=0\ncUserStaysOutside=0\n";
const SEATS: &str = "cHasDrawSlots=p\n[p]\np=0\np=0\np=0\np=0\np=0\np=0\np=0\np=0\np=0\n\
    order=n\norder=e\norder=s\norder=w\n[n]\no=0\no=1\no=2\n[e]\no=2\no=1\no=0\n\
    [s]\no=1\no=0\no=2\n[w]\no=0\n";

/// A world with the building types `types` (name, definition), one building of each at tile
/// (i, 1) with id `10 + i` and rotation `rotation`, and the guests 1 to 4.
fn village(types: &[(&str, String)], rotation: u32) -> (World, Catalogue) {
    let mut cat = Catalogue::default();
    for (name, text) in types {
        cat.push(named(name, text));
    }
    let mut w = world_with((12, 4), |zoo| {
        for (i, (name, _)) in types.iter().enumerate() {
            let at = centre((i as i32, 1));
            let id = 10 + i as u32;
            zoo.entities
                .push(entity("building", name, id, at, rotation));
        }
        for id in 1..=4 {
            zoo.entities
                .push(entity("guests", "man", id, centre((id as i32, 2)), 0));
        }
    });
    for e in w.zoo.entities.iter_mut().take(types.len()) {
        e.common.as_mut().unwrap().building = Some(record(0, START));
    }
    (w, cat)
}

fn set_task(w: &mut World, id: u32, task: u8) {
    w.units.get_mut(id).unwrap().goal.record.task = task;
}

fn users(w: &mut World, building: u32, list: &[(u32, usize)]) {
    let live = Live {
        users: list.iter().map(|&(u, s)| (u, s, 0)).collect(),
        ..Live::default()
    };
    w.units.buildings.insert(building, live);
    for &(unit, slot) in list {
        w.units
            .buildings
            .get_mut(&building)
            .unwrap()
            .tops
            .insert(slot, unit);
    }
}

fn draw(w: &World, cat: &Catalogue) -> Drawn {
    drawn_users(w, cat, Hide::default())
}

#[test]
fn the_top_user_of_a_slot_in_a_building_task_is_drawn_by_a_type_that_draws_its_users() {
    let base = "[Characteristics/Integers]\ncDrawUser=1\ncUserStaysOutside=1\n".to_string();
    let hidden = "[Characteristics/Integers]\ncHideUser=1\ncUserStaysOutside=0\n".to_string();
    let types = [
        ("base", base + ONE + "2\n"),
        (
            "visible",
            VISIBLE.to_string() + ONE + "2\n[Member]\nstructures\n",
        ),
        ("hidden", hidden + ONE + "2\n"),
        ("outside", ONE.to_string() + "2\n"),
    ];
    let (mut w, cat) = village(&types, 0);
    for e in w.zoo.entities.iter_mut().take(4) {
        e.common.as_mut().unwrap().building.as_mut().unwrap().users = vec![(1, 0), (2, 0)];
    }
    // Not in a building task: not drawn, and a record has no live list to take it out of.
    set_task(&mut w, 2, NONE);
    assert_eq!(draw(&w, &cat).pad, []);
    // Entering, leaving or using: the last user of the types that draw, not the others.
    for t in [ENTER_BUILDING, EXIT_BUILDING, USE] {
        set_task(&mut w, 2, t);
        assert_eq!(draw(&w, &cat).pad, [(10, 2), (11, 2)], "task {t}");
    }
    // Only the last user counts.
    set_task(&mut w, 2, NONE);
    set_task(&mut w, 1, USE);
    assert_eq!(draw(&w, &cat).pad, []);
    // A user that is not a unit is not drawn; a slot the type lacks draws nothing.
    set_task(&mut w, 2, USE);
    fn record(w: &mut World, i: usize) -> &mut crate::zoo::BuildingRecord {
        let c = w.zoo.entities[i].common.as_mut().unwrap();
        c.building.as_mut().unwrap()
    }
    record(&mut w, 0).users = vec![(99, 0)];
    record(&mut w, 1).users = vec![(2, 1)];
    assert_eq!(draw(&w, &cat).pad, []);
    // The live list replaces the record; its top user decides.
    users(&mut w, 10, &[(3, 0), (2, 0)]);
    assert_eq!(draw(&w, &cat).pad, [(10, 2)]);
    // The buttons that hide the buildings leave a structure out, users included.
    let hide = Hide {
        buildings: true,
        ..Hide::default()
    };
    users(&mut w, 11, &[(2, 0)]);
    assert_eq!(draw(&w, &cat).pad, [(10, 2), (11, 2)]);
    assert_eq!(drawn_users(&w, &cat, hide).pad, [(10, 2)]);
}

#[test]
fn a_building_with_a_user_in_another_task_takes_it_out_when_drawn_and_shows_it() {
    let types = [("visible", VISIBLE.to_string() + ONE + "3\n")];
    let (mut w, cat) = village(&types, 0);
    users(&mut w, 10, &[(1, 0), (2, 0)]);
    set_task(&mut w, 1, USE);
    set_task(&mut w, 2, NONE);
    w.units.get_mut(2).unwrap().hidden = true;
    let drawn = draw(&w, &cat);
    assert_eq!((drawn.pad, drawn.leave), (vec![], vec![(10, 2)]));
    // Taken out after the frame: off the list, the slot's top user chosen again, made visible.
    take_out(&mut w, 10, 2);
    let live = &w.units.buildings[&10];
    assert_eq!(
        (live.users.len(), live.top(0), live.users_now()),
        (1, Some(1), 1)
    );
    assert!(!w.units.get(2).unwrap().hidden);
    // The user left in the building task is drawn now.
    let drawn = draw(&w, &cat);
    assert_eq!((drawn.pad, drawn.leave), (vec![(10, 1)], vec![]));
}

#[test]
fn a_grandstand_draws_its_seated_users_by_the_order_list_of_its_rotation() {
    let text = format!("{VISIBLE}[slots]\nname=a\n[a]\ncapacity=3\n{SEATS}");
    for (rotation, expected) in [
        (0, [1, 2, 3]),
        (1, [1, 2, 3]),
        (2, [3, 2, 1]),
        (3, [1, 2, 3]),
        (4, [2, 1, 3]),
        (5, [1, 2, 3]),
        (6, [1, 0, 0]),
        (7, [1, 2, 3]),
        (8, [1, 2, 3]),
    ] {
        let (mut w, cat) = village(&[("stand", text.clone())], rotation);
        users(&mut w, 10, &[(1, 0), (2, 0), (3, 0)]);
        let live = w.units.buildings.get_mut(&10).unwrap();
        for (unit, place) in [(1, 0), (2, 1), (3, 2)] {
            live.seats.push(Seat {
                unit,
                slot: 0,
                place,
            });
        }
        // A user in another task is drawn all the same, and nothing is taken out.
        set_task(&mut w, 2, NONE);
        let drawn = draw(&w, &cat);
        let want: Vec<(u32, u32)> = match rotation % 8 {
            6 => vec![(10, 1)],
            _ => expected.iter().map(|&u| (10, u)).collect(),
        };
        assert_eq!(
            (drawn.pad, drawn.leave),
            (want, vec![]),
            "rotation {rotation}"
        );
    }
}

#[test]
fn a_building_with_hide_building_draws_no_picture_while_it_has_a_user() {
    let text = |flag: &str| format!("{VISIBLE}cHideBuilding={flag}\n{ONE}1\n");
    // Only a value of exactly 1 hides; missing, 0, negative and huge do not.
    for (flag, hides) in [
        ("1", true),
        ("0", false),
        ("-1", false),
        ("99999999999", false),
        ("x", false),
    ] {
        let (mut w, cat) = village(&[("bounce", text(flag))], 0);
        assert_eq!(draw(&w, &cat).bare, [], "{flag}: no user");
        users(&mut w, 10, &[(1, 0)]);
        set_task(&mut w, 1, USE);
        let drawn = draw(&w, &cat);
        assert_eq!(drawn.bare, if hides { vec![10] } else { vec![] }, "{flag}");
        // The user is drawn in its place whatever the building does.
        assert_eq!(drawn.pad, [(10, 1)], "{flag}");
        // The picture is back in the first frame after the last user left.
        assert_eq!(yank(&mut w, 10, 1, None), Some(0));
        assert_eq!(draw(&w, &cat).bare, [], "{flag}: left");
    }
    let (mut w, cat) = village(&[("plain", VISIBLE.to_string() + ONE + "1\n")], 0);
    users(&mut w, 10, &[(1, 0)]);
    assert_eq!(draw(&w, &cat).bare, []);
}

#[test]
fn a_top_user_that_left_stays_the_top_user_until_it_is_taken_out_of_a_slot_of_three() {
    let types = [("drift", VISIBLE.to_string() + ONE + "4\n")];
    let (mut w, cat) = village(&types, 0);
    users(&mut w, 10, &[(1, 0), (2, 0), (3, 0)]);
    (1..=3).for_each(|u| set_task(&mut w, u, USE));
    // The top user leaves: the first listed pair, in a building task, becomes the top user.
    assert_eq!(yank(&mut w, 10, 3, Some((3, USE))), Some(0));
    assert_eq!(w.units.buildings[&10].top(0), Some(1));
    // It leaves too, its own pair first in the list: it keeps itself, no longer a user.
    assert_eq!(yank(&mut w, 10, 1, Some((1, USE))), Some(0));
    let live = &w.units.buildings[&10];
    assert_eq!((live.users.len(), live.top(0)), (1, Some(1)));
    // Drawn over the building wherever it is, while its task is one of the three.
    assert_eq!(draw(&w, &cat).pad, [(10, 1)]);
    // In another task it is taken out at each drawn frame: only users now goes down, never below
    // the list length minus one, and the top user stays.
    set_task(&mut w, 1, NONE);
    assert_eq!(draw(&w, &cat).leave, [(10, 1)]);
    for _ in 0..2 {
        take_out(&mut w, 10, 1);
        let live = &w.units.buildings[&10];
        assert_eq!(
            (live.users.len(), live.users_now(), live.top(0)),
            (1, 0, Some(1))
        );
    }
    // A removal that finds the first listed user in no building task takes it out too.
    users(&mut w, 10, &[(2, 0), (3, 0), (4, 0)]);
    set_task(&mut w, 2, NONE);
    assert_eq!(yank(&mut w, 10, 4, Some((4, USE))), Some(0));
    assert_eq!(w.units.buildings[&10].users, [(3, 0, 0)]);
}
