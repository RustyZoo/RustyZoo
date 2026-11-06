//! The show panel's hooks in the game (information panels 19.3): what opens the window, what
//! hides it, and the drop-downs that close when they lose the keyboard focus. The layouts are
//! made-up: the window with the elements the panel writes, the two drop-downs, and one empty
//! layout for each panel that hides it.

use super::bulldozer_tests::{ROOT, Session, with_game_of};
use crate::panel::show::{self, FINANCES_TAB, LAYOUT, SCRIPT_TAB};
use crate::panel::{animal, exhibit, guest, research, staff};
use crate::show::tests::cycle::{staged, tank_mut};

/// The menu panels that hide the window (building, buying, hiring, research, options, developer),
/// the zoo status panels, and the exhibit, animal, guest and staff panels.
const PANELS: [i32; 7] = [8, 3, 4, 10, research::LAYOUT, 5, 34];
const STATUS: [i32; 2] = [14, 803];
const UNITS: [i32; 4] = [
    exhibit::LAYOUT,
    animal::LAYOUT,
    guest::LAYOUT,
    staff::LAYOUT,
];

/// The window, the drop-downs and the panels that hide it, as a root layout and its files.
fn layouts() -> (String, Vec<(String, Vec<u8>)>) {
    let one = |kind: &str, id: i32| format!("[e{id}]\ntype={kind}\nid={id}\ndx=60\ndy=20\n");
    let texts = [11102, 11120, 11173].into_iter().chain(11140..11156);
    let buttons = [11105, 11106, 11123, 11108, 11113, 11114, 11117, 11118];
    let body: String = texts.map(|i| one("UIText", i)).collect::<String>()
        + &buttons.map(|i| one("UIButton", i)).concat()
        + &[11111, 11115].map(|i| one("UIListBox", i)).concat()
        + &[SCRIPT_TAB, FINANCES_TAB]
            .map(|i| one("UIButton", i))
            .concat()
        + &one("UIEditableText", 11104);
    let window =
        |id: i32, body: &str| format!("[LayoutInfo]\nid={id}\nlayer=7\ndx=60\ndy=20\n{body}");
    let mut files = vec![
        ("ui/show.lyt".to_string(), window(LAYOUT, &body)),
        (
            "ui/species.lyt".to_string(),
            window(show::SPECIES_LAYOUT, &one("UIListBox", show::SPECIES_LIST)),
        ),
        (
            "ui/frequency.lyt".to_string(),
            window(
                show::FREQUENCY_LAYOUT,
                &one("UIListBox", show::FREQUENCY_LIST),
            ),
        ),
    ];
    // The panels of the building, buying, hiring, research, options and developer menus, and the
    // zoo status panels; the unit panels are those of the root.
    for id in PANELS.into_iter().chain(STATUS) {
        files.push((format!("ui/p{id}.lyt"), window(id, "")));
    }
    let mut root = ROOT.to_string();
    for (name, _) in &files {
        root += &format!("[{name}]\ntype=UILayout\nlayout={name}\nstate=1\n");
    }
    (
        root,
        files
            .into_iter()
            .map(|(n, t)| (n, t.into_bytes()))
            .collect(),
    )
}

/// A game on the staged world of the show tests, whose show tank is returned, with its window
/// hidden and the panel unopened.
fn with_show(name: &str, body: impl FnOnce(&mut Session, u32)) {
    let (mut world, cat) = staged(vec![]);
    let x = tank_mut(&mut world).id;
    let (root, files) = layouts();
    let files: Vec<(&str, Vec<u8>)> = files.iter().map(|(n, b)| (n.as_str(), b.clone())).collect();
    with_game_of(name, "", (cat, &root, &files), |s| {
        s.app.game.as_mut().unwrap().world = world;
        // The first look settles the panel's own state.
        s.app.show_pass(0, false);
        body(s, x);
    });
}

fn shown(s: &mut Session, id: i32) -> bool {
    s.app.interface().unwrap().shown(id)
}

/// Opens the window by the exhibit panel's way.
fn open(s: &mut Session, x: u32) {
    s.app.game.as_mut().unwrap().panel.current = Some(x);
    s.app.show_exhibit_set();
    s.app.show_pass(0, false);
    assert!(shown(s, LAYOUT));
}

#[test]
fn a_show_tank_on_the_exhibit_panel_opens_the_window_on_the_script_and_another_exhibit_hides_it() {
    with_show("show-exhibit", |s, x| {
        assert!(!shown(s, LAYOUT));
        open(s, x);
        let ui = s.app.interface().unwrap();
        assert!(ui.is_down(SCRIPT_TAB) && !ui.is_down(FINANCES_TAB));
        assert_eq!(s.app.game.as_ref().unwrap().show.exhibit, Some(x));
        // The exhibit without a show info: the window closes and forgets the tank.
        let other = s.app.game.as_ref().unwrap().world.exhibits().list();
        let other = other.iter().map(|e| e.id).find(|&id| id != x).unwrap();
        s.app.game.as_mut().unwrap().panel.current = Some(other);
        s.app.show_exhibit_set();
        assert!(!shown(s, LAYOUT));
        assert_eq!(s.app.game.as_ref().unwrap().show.exhibit, None);
    });
}

#[test]
fn a_grandstand_facing_the_show_tank_opens_the_finances_and_deselecting_it_closes_the_window() {
    with_show("show-stand", |s, x| {
        // The grandstand turned away opens nothing.
        s.app.show_stand(8);
        assert!(!shown(s, LAYOUT));
        let layout = crate::panel::building::PLAIN;
        s.app.game.as_mut().unwrap().building.select(7, layout);
        s.app.show_stand(7);
        let ui = s.app.interface().unwrap();
        assert!(ui.shown(LAYOUT) && ui.is_down(FINANCES_TAB) && !ui.is_down(SCRIPT_TAB));
        let game = s.app.game.as_ref().unwrap();
        assert_eq!((game.show.exhibit, game.show.stand), (Some(x), Some(7)));
        // Still selected: the window stays.
        s.app.show_pass(0, false);
        assert!(shown(s, LAYOUT));
        s.app.game.as_mut().unwrap().building.deselect();
        s.app.show_pass(0, false);
        assert!(!shown(s, LAYOUT));
        assert_eq!(s.app.game.as_ref().unwrap().show.stand, None);
    });
}

#[test]
fn opening_the_window_hides_the_menu_and_status_panels() {
    with_show("show-open-hides", |s, x| {
        for id in PANELS.into_iter().chain(STATUS) {
            s.app.interface().unwrap().show(id);
        }
        open(s, x);
        for id in PANELS.into_iter().chain(STATUS) {
            assert!(!shown(s, id), "{id}");
        }
    });
}

#[test]
fn a_panel_shown_after_the_window_hides_it_and_one_that_was_shown_before_does_not() {
    with_show("show-panels", |s, x| {
        // The unit panels are not hidden by opening the window: one shown before it stays.
        s.app.interface().unwrap().show(UNITS[1]);
        open(s, x);
        s.app.show_pass(0, false);
        assert!(shown(s, LAYOUT) && shown(s, UNITS[1]));
        for id in PANELS.into_iter().chain(UNITS) {
            s.app.interface().unwrap().set_hidden(id, true);
            open(s, x);
            s.app.interface().unwrap().show(id);
            s.app.show_pass(0, false);
            assert!(!shown(s, LAYOUT), "{id}");
        }
    });
}

#[test]
fn a_drop_down_without_the_keyboard_focus_is_hidden_and_the_button_gives_it_the_focus() {
    with_show("show-focus", |s, x| {
        open(s, x);
        for (layout, list, button) in [
            (
                show::SPECIES_LAYOUT,
                show::SPECIES_LIST,
                show::SPECIES_BUTTON,
            ),
            (
                show::FREQUENCY_LAYOUT,
                show::FREQUENCY_LIST,
                show::FREQUENCY_BUTTON,
            ),
        ] {
            s.app.interface().unwrap().show(layout);
            s.app.show_pass(0, false);
            assert!(!shown(s, layout), "{layout}");
            // The button focuses the list, which keeps its drop-down open.
            s.app.interface().unwrap().show(layout);
            s.app.show_pressed(button);
            s.app.show_pass(0, false);
            assert!(shown(s, layout) && s.app.interface().unwrap().has_focus(list));
            s.app.interface().unwrap().unfocus(list);
            s.app.show_pass(0, false);
            assert!(!shown(s, layout), "{layout}");
        }
    });
}

#[test]
fn a_show_info_that_is_gone_closes_the_window() {
    with_show("show-gone", |s, x| {
        open(s, x);
        s.app.game.as_mut().unwrap().world.zoo.shows.infos = vec![None];
        s.app.show_pass(0, false);
        assert!(!shown(s, LAYOUT));
        assert_eq!(s.app.game.as_ref().unwrap().show.exhibit, None);
    });
}
