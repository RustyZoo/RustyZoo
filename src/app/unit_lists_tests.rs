//! The animal, guest and staff list windows through the pass (information panels 20): what showing
//! one hides, the filter buttons, the refill, a row click, on the made-up map of the bulldozer
//! tests with a small layout for each window.

use super::bulldozer_tests::{Session, catalogue, ty, with_game_of};
use crate::panel::unitlist::{ANIMALS, GUESTS, STAFF};
use super::{Command, Mouse};
use crate::ui::layout::{HIDDEN, Kind};

/// The root of the bulldozer tests with the three windows, the panels they replace and the
/// plaques, each hidden at the start, and the follow buttons of the unit panels.
fn root() -> String {
    let mut root = super::bulldozer_tests::ROOT
        .replace("ui/animal.lyt", "ui/animal2.lyt")
        .replace("ui/guest.lyt", "ui/guest2.lyt")
        .replace("ui/staff.lyt", "ui/staff2.lyt");
    for name in ["16", "12", "31", "14", "803", "41", "27", "28"] {
        root += &format!("[w{name}]\ntype=UILayout\nlayout=ui/w{name}.lyt\nstate=1\n");
    }
    root
}

/// A window: its list, its title, and one sticky button and one count text per filter (`ids` is
/// the first button and the list, a filter's count text is the id after its button).
fn window(layout: i32, list: i32, title: i32, buttons: &[i32]) -> (String, Vec<u8>) {
    let mut text = format!(
        "[LayoutInfo]\nid={layout}\nlayer=7\nx=0\ny=0\ndx=60\ndy=80\n\
         [l]\ntype=UIListBox\nid={list}\nx=0\ny=0\ndx=60\ndy=36\n\
         [t]\ntype=UIText\nid={title}\nx=0\ny=36\ndx=10\ndy=4\n"
    );
    for (n, b) in buttons.iter().enumerate() {
        text += &format!(
            "[b{b}]\ntype=UIButton\nid={b}\nx={}\ny=40\ndx=4\ndy=4\nstate=4097\n\
             [c{b}]\ntype=UIText\nid={}\nx={}\ny=44\ndx=4\ndy=4\n",
            n * 5,
            b + 1,
            n * 5
        );
    }
    let radio = format!("[r]\ntype=UIRadioSet\nid={}\n", layout * 1000);
    let members: String = buttons.iter().map(|b| format!("button={b}\n")).collect();
    (
        format!("ui/w{layout}.lyt"),
        (text + &radio + &members).into_bytes(),
    )
}

fn empty(layout: i32) -> (String, Vec<u8>) {
    (
        format!("ui/w{layout}.lyt"),
        format!("[LayoutInfo]\nid={layout}\nlayer=7\nx=0\ny=0\ndx=10\ndy=10\n").into_bytes(),
    )
}

fn files() -> Vec<(String, Vec<u8>)> {
    let panel = |id: i32, follow: i32| {
        format!(
            "[LayoutInfo]\nid={id}\nlayer=7\nx=0\ny=0\ndx=20\ndy=10\n\
             [a]\ntype=UIButton\nid={follow}\nx=10\ndx=8\ndy=8\nstate=2049\ntransparent=0\n"
        )
        .into_bytes()
    };
    let mut files = vec![
        window(16, 3805, 3835, &[3815, 3817, 3819, 3821, 3823, 3838]),
        window(12, 3705, 3727, &[3715, 3717, 3719, 3721, 3723, 3725]),
        window(31, 4205, 4235, &[4215, 4217, 4219, 4221, 4223, 4225]),
        empty(14),
        empty(803),
        empty(41),
        empty(27),
        empty(28),
    ];
    files.push(("ui/animal2.lyt".into(), panel(6, 3114)));
    files.push(("ui/guest2.lyt".into(), panel(9, 3415)));
    files.push(("ui/staff2.lyt".into(), panel(17, 3515)));
    files
}

/// A game with the windows and a zebra "Bo" (a 1), "Al" (a 2), a guest, a keeper, a helicopter and
/// a building, in that order; the ids come back in that order.
fn with_lists(name: &str, body: impl FnOnce(&mut Session, [u32; 6])) {
    let mut cat = catalogue();
    cat.push(ty("helicopter", b"helicopter", b"", "", [1, 1, 1]));
    let files = files();
    let more: Vec<(&str, Vec<u8>)> = files.iter().map(|(n, b)| (n.as_str(), b.clone())).collect();
    with_game_of(name, "", (cat, &root(), &more), |s| {
        let at = |n: i32| (n * 64 + 32, 10 * 64 + 32);
        let kinds = [13, 13, 14, 12, 20, 2];
        let mut ids = [0; 6];
        for (n, ty) in kinds.into_iter().enumerate() {
            let (x, y) = at(8 + 3 * n as i32);
            ids[n] = s.add(ty, x, y, 0);
        }
        let game = s.app.game.as_mut().unwrap();
        for (id, name) in ids.iter().zip(["Bo", "Al", "Gus", "Kay", "Heli", "Base"]) {
            let at = game.world.find(*id).unwrap();
            game.world.zoo.entities[at].common.as_mut().unwrap().name = name.into();
        }
        s.pass(&[]);
        body(s, ids);
    });
}

fn shown(s: &mut Session, id: i32) -> bool {
    s.app.interface().unwrap().shown(id)
}

fn row_texts(s: &mut Session, list: i32) -> Vec<String> {
    let ui = s.app.interface().unwrap();
    (0..)
        .map_while(|r| ui.row_text(list, r))
        .map(|t| String::from_utf8_lossy(t).into())
        .collect()
}

fn hidden_rows(s: &mut Session, list: i32) -> Vec<bool> {
    let ui = s.app.interface().unwrap();
    match &ui.tree.elements[ui.tree.find(list).unwrap()].kind {
        Kind::ListBox(l) => (0..l.rows.rows.len()).map(|n| l.rows.hidden(n)).collect(),
        _ => unreachable!(),
    }
}

#[test]
fn showing_a_window_hides_the_status_and_show_panels_and_the_plaques_but_the_animal_list_keeps_them()
 {
    with_lists("lists-show", |s, _| {
        for window in [16, 12, 31] {
            for id in [14, 803, 41, 27, 28] {
                s.app.interface().unwrap().show(id);
            }
            s.app.interface().unwrap().show(window);
            s.pass(&[]);
            assert!(
                ![14, 803, 41].iter().any(|&id| shown(s, id)),
                "window {window}"
            );
            assert_eq!(shown(s, 27), window == 16, "window {window}");
            assert!(window == 16 || !shown(s, 28), "window {window}");
            s.app.interface().unwrap().set_hidden(window, true);
            s.pass(&[]);
        }
    });
}

#[test]
fn the_unit_panels_stand_above_a_shown_window_and_back_when_it_is_hidden() {
    with_lists("lists-above", |s, _| {
        let alt = |s: &mut Session| {
            let ui = s.app.interface().unwrap();
            [6, 9, 17].map(|id| ui.tree.elements[ui.tree.find(id).unwrap()].alt)
        };
        assert_eq!(alt(s), [false; 3]);
        s.app.interface().unwrap().show(12);
        s.pass(&[]);
        assert_eq!(alt(s), [true; 3]);
        s.app.interface().unwrap().set_hidden(12, true);
        s.pass(&[]);
        assert_eq!(alt(s), [false; 3]);
    });
}

#[test]
fn a_shown_window_lists_its_units_and_the_filter_buttons_hide_rows_and_keep_the_counts() {
    with_lists("lists-filter", |s, ids| {
        // Paused: the step does not move the needs.
        s.app.game.as_mut().unwrap().paused = true;
        s.app.interface().unwrap().show(16);
        s.pass(&[]);
        // Al before Bo: sorted by name.
        assert_eq!(row_texts(s, 3805), ["Al", "Bo"]);
        let game = s.app.game.as_mut().unwrap();
        match &mut game.world.units.get_mut(ids[0]).unwrap().part {
            crate::unit::Part::Animal(a) => a.hunger = 99,
            _ => unreachable!(),
        }
        // The hungry filter: 3821. The press runs the same pass; the list holds every row, hidden.
        assert!(s.app.interface().unwrap().press_button(3821));
        s.pass(&[]);
        assert_eq!(s.app.lists.windows[ANIMALS].filter, 3);
        assert_eq!(hidden_rows(s, 3805), [true, false]);
        let ui = s.app.interface().unwrap();
        assert_eq!(ui.text(3816).unwrap(), b"2", "all");
        assert_eq!(ui.text(3822).unwrap(), b"1", "hungry");
        // Another window starts on "all" and the filter of this one is kept when hidden and shown.
        s.app.interface().unwrap().set_hidden(16, true);
        s.pass(&[]);
        s.app.interface().unwrap().show(16);
        s.pass(&[]);
        assert_eq!(hidden_rows(s, 3805), [true, false]);
        assert_eq!(s.app.lists.windows[GUESTS].filter, 0);
    });
}

#[test]
fn a_unit_that_comes_or_goes_changes_the_list_at_once_and_a_rename_at_the_next_tick() {
    with_lists("lists-change", |s, ids| {
        s.app.interface().unwrap().show(12);
        s.pass(&[]);
        assert_eq!(row_texts(s, 3705), ["Gus"]);
        // A second guest arrives (a unit of the table), a rename follows within a tick.
        let (x, y) = (30 * 64 + 32, 10 * 64 + 32);
        let second = s.add(14, x, y, 0);
        let game = s.app.game.as_mut().unwrap();
        let at = game.world.find(second).unwrap();
        game.world.zoo.entities[at].common.as_mut().unwrap().name = "Ann".into();
        s.pass(&[]);
        assert_eq!(row_texts(s, 3705), ["Gus", "Ann"]);
        let game = s.app.game.as_mut().unwrap();
        let at = game.world.find(ids[2]).unwrap();
        game.world.zoo.entities[at].common.as_mut().unwrap().name = "Gil".into();
        for _ in 0..4 {
            s.pass(&[]);
        }
        assert_eq!(row_texts(s, 3705), ["Gil", "Ann"]);
    });
}

#[test]
fn a_row_click_selects_the_unit_centres_the_view_and_ends_the_follow() {
    with_lists("lists-click", |s, ids| {
        s.app.interface().unwrap().show(16);
        s.pass(&[]);
        let before = s.app.camera().unwrap().scroll;
        s.app.interface().unwrap().set_button(3114, true, true);
        s.app.interface().unwrap().set_button(1028, true, true);
        s.app.list_row(ANIMALS, 1);
        s.pass(&[]);
        let game = s.app.game.as_ref().unwrap();
        assert_eq!(game.selected, Some(ids[0]), "row 1 is Bo");
        assert_ne!(s.app.camera().unwrap().scroll, before, "the view moved");
        assert!(!s.app.interface().unwrap().is_down(3114), "no follow");
        assert!(
            !s.app.interface().unwrap().is_down(1028),
            "the bulldozer is off"
        );
        assert!(shown(s, 6), "the animal panel");
        // The row of the selected unit is the selected row.
        assert_eq!(s.app.interface().unwrap().selected_row(3805), Some(1));
        // A row that is not there changes nothing; a click on no row clears the selection.
        s.app.list_row(ANIMALS, 7);
        s.pass(&[]);
        assert_eq!(s.app.game.as_ref().unwrap().selected, Some(ids[0]));
        s.app.list_row(ANIMALS, -1);
        s.pass(&[]);
        assert_eq!(s.app.game.as_ref().unwrap().selected, None);
        assert!(!shown(s, 6));
        assert_eq!(s.app.interface().unwrap().selected_row(3805), None);
    });
}

#[test]
fn the_staff_list_row_of_the_helicopter_selects_its_base_and_a_followed_base_stays_put() {
    with_lists("lists-heli", |s, ids| {
        let game = s.app.game.as_mut().unwrap();
        match &mut game.world.units.get_mut(ids[4]).unwrap().part {
            crate::unit::Part::Staff(st) => st.heli.base = ids[5],
            _ => unreachable!(),
        }
        s.app.interface().unwrap().show(31);
        s.pass(&[]);
        // Kay, then the helicopter showing its base.
        assert_eq!(row_texts(s, 4205), ["Kay", "Base"]);
        let before = s.app.camera().unwrap().scroll;
        s.app.list_row(STAFF, 1);
        assert_eq!(
            s.app.game.as_ref().unwrap().selected,
            Some(ids[5]),
            "the base"
        );
        let moved = s.app.camera().unwrap().scroll;
        assert_ne!(moved, before);
        // The view follows the helicopter (its button is down) and the base is selected: the
        // click moves nothing and the follow stays.
        s.app.interface().unwrap().set_button(3515, true, true);
        s.app.game.as_mut().unwrap().staff.select(ids[4]);
        s.app.game.as_mut().unwrap().camera.scroll = (0, 0);
        s.app.list_row(STAFF, 1);
        assert_eq!(s.app.camera().unwrap().scroll, (0, 0));
        assert!(s.app.interface().unwrap().is_down(3515));
        // Kay's row centres the view again.
        s.app.list_row(STAFF, 0);
        assert_eq!(s.app.game.as_ref().unwrap().selected, Some(ids[3]));
        assert_ne!(s.app.camera().unwrap().scroll, (0, 0));
        assert!(!s.app.interface().unwrap().is_down(3515));
        // Without a base the helicopter's row selects nothing.
        let game = s.app.game.as_mut().unwrap();
        match &mut game.world.units.get_mut(ids[4]).unwrap().part {
            crate::unit::Part::Staff(st) => st.heli.base = 0,
            _ => unreachable!(),
        }
        s.app.list_row(STAFF, 1);
        assert_eq!(s.app.game.as_ref().unwrap().selected, Some(ids[3]));
    });
}

#[test]
fn the_selected_entity_selects_its_row_in_a_window_that_is_not_shown() {
    with_lists("lists-hidden-select", |s, ids| {
        s.app.interface().unwrap().show(12);
        s.pass(&[]);
        s.app.interface().unwrap().set_hidden(12, true);
        s.pass(&[]);
        s.app.game.as_mut().unwrap().selected = Some(ids[2]);
        s.pass(&[]);
        assert_eq!(s.app.interface().unwrap().selected_row(3705), Some(0));
        s.app.game.as_mut().unwrap().selected = None;
        s.pass(&[]);
        assert_eq!(s.app.interface().unwrap().selected_row(3705), None);
        assert!(!s.app.interface().unwrap().shown(12));
        let ui = s.app.interface().unwrap();
        assert_ne!(
            ui.tree.elements[ui.tree.find(12).unwrap()].state & HIDDEN,
            0
        );
    });
}

#[test]
fn a_mouse_click_on_a_row_runs_the_same_selection() {
    with_lists("lists-mouse", |s, ids| {
        s.app.interface().unwrap().show(16);
        s.pass(&[]);
        let before = s.app.camera().unwrap().scroll;
        // The second row (12 pixels each, from the top of the list) is Bo.
        let click = [
            Command::Mouse(Mouse::Move(5, 14)),
            Command::Mouse(Mouse::Down),
            Command::Mouse(Mouse::Up),
        ];
        s.pass(&click);
        assert_eq!(s.app.game.as_ref().unwrap().selected, Some(ids[0]));
        assert_ne!(s.app.camera().unwrap().scroll, before);
    });
}
