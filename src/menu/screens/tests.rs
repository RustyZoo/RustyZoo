//! The scenario and freeform screens on made-up layouts, lists and texts.

use super::*;
use crate::app::Mouse;
use crate::lists::{freeform_list, scenario_list};
use crate::menu::{Request, Start};
use crate::money::StartCash;
use crate::sprite::tests::Install;
use crate::ui::draw::Strings as UiStrings;
use crate::ui::layout::{DISABLED, HIDDEN, Picture, Tree};

/// Made-up texts.
struct Texts;

impl UiStrings for Texts {
    fn string(&self, id: i32) -> String {
        match id {
            16001 => "Alpha",
            16002 => "Beta",
            16003 => "Gamma",
            27001 => "Map One",
            27002 => "Map Two",
            22100 => "#",
            11536 => "Easy",
            11537 => "Mid",
            11538 => "Hard",
            _ => "",
        }
        .into()
    }

    fn text_entry(&self, name: &[u8]) -> Vec<u8> {
        match name {
            b"STORY_A" => &b"Story A"[..],
            b"STORY_LOCK" => b"Locked story",
            b"DESC_1" => b"About map one",
            _ => b"",
        }
        .to_vec()
    }
}

fn tga() -> Vec<u8> {
    vec![
        0, 0, 2, 0, 0, 0, 0, 0, 0, 0, 0, 0, 3, 0, 2, 0, 24, 0x20, 1, 2, 3, 1, 2, 3, 1, 2, 3, 1, 2,
        3, 1, 2, 3, 1, 2, 3,
    ]
}

const SCENARIOS: &str = "[aa]\nname=16001\nscenario=s/aa.scn\nunlocks=aa\n\
    [ab]\nname=16002\nscenario=s/ab.scn\nunlocks=ab\nlocks=aa\n\
    [ac]\nname=16003\nscenario=s/missing.scn\nunlocks=ac\n";

/// A window of 120 x 140 pixels with the screens of the menu: both lists have rows of 16 pixels
/// (their small icon slot) at the left of the window, Play and the spinners are 10 x 10 buttons.
fn fixture(name: &str, scn: bool, free: bool) -> (Install, Vfs, Interface) {
    let root = "[LayoutInfo]\nid=1\nx=0\ny=0\ndx=whole\ndy=whole\n\
        [a]\ntype=ZTMapView\nid=7000\ndx=whole\ndy=whole\n\
        [b]\ntype=UILayout\nlayout=ui/scenario.lyt\n\
        [c]\ntype=UILayout\nlayout=ui/mapselec.lyt\n";
    let button = "type=UIButton\ndx=10\ndy=10\ncolorasicon=1\n";
    let scenario = format!(
        "[LayoutInfo]\nid=32\nlayer=3\nx=0\ny=0\ndx=whole\ndy=whole\nstate=1\n\
         [a]\ntype=UIImage\nid=50001\nx=70\ny=0\n\
         [b]\ntype=UIListBox\nid=50002\nx=0\ny=20\ndx=60\ndy=64\nminiconwidth=8\nminiconheight=16\n\
         handledoubleclicks=1\nstate=131072\nborder=0\n\
         [c]\ntype=UIText\nid=50004\nx=0\ny=90\ndx=60\ndy=20\n\
         [d]\nid=50008\nx=70\ny=20\n{button}\
         [e]\nid=50009\nx=70\ny=40\naction=2\ntarget=-1\n{button}"
    );
    let freeform = format!(
        "[LayoutInfo]\nid=39\nlayer=3\nx=0\ny=0\ndx=whole\ndy=whole\nstate=1\n\
         [a]\ntype=UIImage\nid=11501\nx=70\ny=0\n\
         [b]\ntype=UIListBox\nid=11504\nx=0\ny=20\ndx=60\ndy=64\nhandledoubleclicks=1\n\
         state=131072\nborder=0\n\
         [c]\ntype=UIText\nid=11507\nx=0\ny=90\ndx=60\ndy=20\n\
         [d]\ntype=UIEditableText\nid=11510\nx=0\ny=115\ndx=60\ndy=14\nnumeric=1\ncharlimit=8\n\
         [e]\nid=11511\nx=70\ny=60\n{button}\
         [f]\nid=11512\nx=70\ny=75\n{button}\
         [g]\ntype=UIText\nid=11531\nx=0\ny=130\ndx=60\ndy=10\n\
         [h]\nid=11513\nx=70\ny=20\n{button}\
         [i]\nid=11514\nx=70\ny=40\naction=2\ntarget=-1\n{button}"
    );
    let mut files: Vec<(&str, Vec<u8>)> = vec![
        ("ui/root.lyt", root.as_bytes().to_vec()),
        ("ui/scenario.lyt", scenario.into_bytes()),
        ("ui/mapselec.lyt", freeform.into_bytes()),
        ("ui/pic1.tga", tga()),
        ("ui/lock.tga", tga()),
    ];
    if scn {
        files.extend([
            ("scenar01.cfg", SCENARIOS.as_bytes().to_vec()),
            (
                "s/aa.scn",
                b"[desc]\npicture=ui/pic1.tga\nstory=STORY_A\nlockedpicture=ui/lock.tga\n\
                  lockedstory=STORY_LOCK\n[start]\nsavegame=maps/a.zoo\nsetcash=100000\naddcash=200.5\nreset=1\n"
                    .to_vec(),
            ),
            (
                "s/ab.scn",
                b"[desc]\npicture=ui/none.tga\nstory=\nlockedpicture=ui/lock.tga\n\
                  lockedstory=STORY_LOCK\n"
                    .to_vec(),
            ),
        ]);
    }
    if free {
        files.extend([
            (
                "freeform.cfg",
                b"[freeform]\nfreeform=f/m1.scn\nfreeform=f/m2.scn\n".to_vec(),
            ),
            (
                "f/m1.scn",
                b"[freeform]\nname=27001\ndescription=DESC_1\nicon=ui/pic1.tga\n\
                  [start]\nsavegame=maps/m1.zoo\nreset=1\nsetcash=1\n"
                    .to_vec(),
            ),
            ("f/m2.scn", b"[freeform]\nname=27002\n".to_vec()),
        ]);
    }
    let install = Install::new(&format!("screens-{name}"), &files);
    let vfs = Vfs::open(&install.0).unwrap();
    let mut tree = Tree::load(&vfs, "ui/root.lyt", 0);
    assert_eq!(tree.failure, None);
    tree.resolve(120, 140, |_| 12);
    (install, vfs, Interface::new(tree, Box::new(Texts)))
}

fn menu(ini: &str, install: &Install, vfs: &Vfs) -> Menu {
    let mut menu = Menu::new(
        Config::parse(ini.as_bytes()).unwrap(),
        &install.0,
        Some(install.0.join("user")),
        None,
        0,
    );
    menu.set_lists(scenario_list(vfs, 0), freeform_list(vfs));
    menu
}

fn written(install: &Install) -> Option<String> {
    std::fs::read_to_string(install.0.join("user/zoo.ini")).ok()
}

fn flag(ui: &Interface, id: i32, flag: u32) -> bool {
    ui.tree.elements[ui.tree.find(id).unwrap()].state & flag != 0
}

fn text(ui: &Interface, id: i32) -> String {
    String::from_utf8(ui.text(id).unwrap()).unwrap()
}

/// The pointer at the middle of row `row` of list `id`, and what the click did.
fn click_row(ui: &mut Interface, id: i32, row: i32, double: bool) -> Vec<Action> {
    let (x, y, _, _) = ui.tree.rect(ui.tree.find(id).unwrap());
    ui.mouse(Mouse::Move(x + 5, y + 16 * row + 4));
    ui.take_actions();
    let presses = match double {
        true => &[Mouse::Down, Mouse::Up, Mouse::Double, Mouse::Up][..],
        false => &[Mouse::Down, Mouse::Up],
    };
    for &mouse in presses {
        ui.mouse(mouse);
    }
    ui.take_actions()
}

#[test]
fn the_scenario_screen_lists_the_scenarios_with_their_state_and_describes_the_first() {
    let (install, vfs, mut ui) = fixture("scn", true, false);
    let mut menu = menu("", &install, &vfs);
    ui.show(32);
    let log = menu.pressed(&vfs, &mut ui, 7111);
    assert_eq!(
        log,
        [
            "scenarios 3",
            "scenario aa playable 16001",
            "scenario ab locked 16002",
            "scenario ac playable 16003",
            "scenario select aa playable picture ui/pic1.tga story shown play enabled",
        ]
    );
    // The names, the first selected, its picture of 3 x 2 and its story.
    let rows = match &ui.tree.elements[ui.tree.find(50002).unwrap()].kind {
        crate::ui::layout::Kind::ListBox(l) => l.rows.rows.clone(),
        _ => panic!("no list"),
    };
    assert_eq!(
        rows.iter().map(|r| r.text.clone()).collect::<Vec<_>>(),
        [b"Alpha".to_vec(), b"Beta".to_vec(), b"Gamma".to_vec()]
    );
    assert_eq!(ui.selected_row(50002), Some(0));
    assert_eq!(text(&ui, 50004), "Story A");
    assert!(!flag(&ui, 50004, HIDDEN) && !flag(&ui, 50001, HIDDEN));
    assert_eq!(ui.tree.rect(ui.tree.find(50001).unwrap()).2, 3);
    assert!(matches!(
        ui.tree.elements[ui.tree.find(50001).unwrap()].look.normal,
        Some(Picture::Image(_))
    ));
    assert!(!flag(&ui, 50008, DISABLED));
}

#[test]
fn a_locked_scenario_shows_its_locked_picture_and_story_and_play_stays_off() {
    let (install, vfs, mut ui) = fixture("locked", true, false);
    let mut menu = menu("", &install, &vfs);
    ui.show(32);
    menu.pressed(&vfs, &mut ui, 7111);
    let actions = click_row(&mut ui, 50002, 1, false);
    assert_eq!(actions, [Action::Row(50002, 1)]);
    let log = menu.action(&vfs, &mut ui, actions[0]);
    assert_eq!(
        log,
        ["scenario select ab locked picture ui/lock.tga story shown play disabled"]
    );
    assert_eq!(text(&ui, 50004), "Locked story");
    assert!(flag(&ui, 50008, DISABLED));
    // Play does nothing, and neither does a double click on the row.
    assert!(menu.pressed(&vfs, &mut ui, 50008).is_empty());
    let actions = click_row(&mut ui, 50002, 1, true);
    assert!(actions.contains(&Action::Double(50002, 1)));
    for action in actions {
        assert!(
            menu.action(&vfs, &mut ui, action)
                .iter()
                .all(|l| !l.starts_with("play"))
        );
    }
    // A win of the scenario it needs unlocks it.
    let mut won = self::menu("[scenario]\naa=1\n", &install, &vfs);
    let log = won.pressed(&vfs, &mut ui, 7111);
    assert_eq!(log[1], "scenario aa won 16001");
    assert_eq!(log[2], "scenario ab playable 16002");
    // Its picture is missing: the element is hidden, and its story is empty: hidden too.
    let actions = click_row(&mut ui, 50002, 1, false);
    let log = won.action(&vfs, &mut ui, actions[0]);
    assert_eq!(
        log,
        ["scenario select ab playable picture none story hidden play enabled"]
    );
    assert!(flag(&ui, 50001, HIDDEN) && flag(&ui, 50004, HIDDEN));
    assert!(!flag(&ui, 50008, DISABLED));
}

#[test]
fn play_and_a_double_click_ask_for_the_selected_scenario_and_the_selection_is_kept() {
    let (install, vfs, mut ui) = fixture("play", true, false);
    let mut menu = menu("", &install, &vfs);
    ui.show(32);
    menu.pressed(&vfs, &mut ui, 7111);
    assert_eq!(
        menu.pressed(&vfs, &mut ui, 50008),
        ["play scenario aa s/aa.scn"]
    );
    // It asks for the map its file names, and empties the last file when it starts.
    assert_eq!(
        menu.take_request(),
        Some(Request::Open(Start {
            path: "maps/a.zoo".into(),
            label: "scenario aa".into(),
            forget_last: true,
            freeform: false,
            cash: StartCash {
                set: Some(100_000.0),
                add: Some(200.5),
            },
            reset: true,
        }))
    );
    // The third entry's file is missing: no picture, no story, but it can be played (it fails
    // when it starts).
    let actions = click_row(&mut ui, 50002, 2, true);
    let mut lines = Vec::new();
    for action in actions {
        lines.extend(menu.action(&vfs, &mut ui, action));
    }
    assert!(
        lines.contains(&"play scenario ac s/missing.scn".to_string()),
        "{lines:?}"
    );
    assert_eq!(
        menu.take_request(),
        Some(Request::Message("s/missing.scn: no map to start".into()))
    );
    // Back leaves the selection, and the screen shown again describes it.
    assert_eq!(menu.pressed(&vfs, &mut ui, 50009), ["back"]);
    let log = menu.pressed(&vfs, &mut ui, 7111);
    assert_eq!(ui.selected_row(50002), Some(2));
    assert!(log.last().unwrap().starts_with("scenario select ac"));
}

#[test]
fn no_scenarios_leave_an_empty_list_and_play_off() {
    let (install, vfs, mut ui) = fixture("noscn", false, false);
    let mut menu = menu("", &install, &vfs);
    ui.show(32);
    assert_eq!(
        menu.pressed(&vfs, &mut ui, 7111),
        ["scenarios 0", "scenario none"]
    );
    assert_eq!(ui.selected_row(50002), None);
    assert!(flag(&ui, 50008, DISABLED) && flag(&ui, 50001, HIDDEN));
    assert!(menu.pressed(&vfs, &mut ui, 50008).is_empty());
    // A row that is not there is no selection.
    assert_eq!(
        menu.action(&vfs, &mut ui, Action::Row(50002, -1)),
        ["scenario none"]
    );
    assert_eq!(
        menu.action(&vfs, &mut ui, Action::Row(50002, 99)),
        ["scenario none"]
    );
}

#[test]
fn the_freeform_screen_lists_the_maps_and_shows_the_first_with_the_starting_cash() {
    let (install, vfs, mut ui) = fixture("free", false, true);
    let ini = "[UI]\nMSStartingCash=75000\nMSMinCash=10000\nMSMaxCash=500000\n";
    let mut menu = menu(ini, &install, &vfs);
    ui.show(39);
    let log = menu.pressed(&vfs, &mut ui, 7110);
    assert_eq!(
        log,
        [
            "freeform 2",
            "map f/m1.scn 27001",
            "map f/m2.scn 27002",
            "map select f/m1.scn picture ui/pic1.tga description shown",
            "cash 75000 middle up on down on",
        ]
    );
    assert_eq!(text(&ui, 11507), "About map one");
    assert_eq!(
        text(&ui, 11510),
        "#75,000",
        "the symbol of the string table"
    );
    assert_eq!(text(&ui, 11531), "Mid");
    assert_eq!(ui.selected_row(11504), Some(0));
    assert_eq!(written(&install), None, "showing the screen writes nothing");
    // The second map has neither picture nor description.
    let actions = click_row(&mut ui, 11504, 1, false);
    let log = menu.action(&vfs, &mut ui, actions[0]);
    assert_eq!(log, ["map select f/m2.scn picture none description hidden"]);
    assert!(flag(&ui, 11501, HIDDEN) && flag(&ui, 11507, HIDDEN));
    assert!(!flag(&ui, 11513, DISABLED));
    assert_eq!(
        menu.pressed(&vfs, &mut ui, 11513),
        ["play freeform f/m2.scn cash 75000"]
    );
    assert_eq!(
        menu.take_request(),
        Some(Request::Message("f/m2.scn: no map to start".into()))
    );
    // The first map names its own, and the cash goes with the request.
    ui.select_row(11504, Some(0));
    menu.action(&vfs, &mut ui, Action::Row(11504, 0));
    menu.pressed(&vfs, &mut ui, 11513);
    assert_eq!(
        menu.take_request(),
        Some(Request::Open(Start {
            path: "maps/m1.zoo".into(),
            label: "freeform cash 75000".into(),
            forget_last: true,
            freeform: true,
            // The cash of the screen, not the file's own `setcash`.
            cash: StartCash {
                set: Some(75_000.0),
                add: None,
            },
            reset: true,
        }))
    );
    ui.select_row(11504, Some(1));
    menu.action(&vfs, &mut ui, Action::Row(11504, 1));
    // A double click starts it too, and the selection comes back when the screen does.
    let mut lines = Vec::new();
    for action in click_row(&mut ui, 11504, 1, true) {
        lines.extend(menu.action(&vfs, &mut ui, action));
    }
    assert!(lines.contains(&"play freeform f/m2.scn cash 75000".to_string()));
    menu.pressed(&vfs, &mut ui, 7110);
    assert_eq!(ui.selected_row(11504), Some(1));
}

#[test]
fn the_spinners_step_to_the_limits_and_are_off_there_and_every_change_is_written() {
    let (install, vfs, mut ui) = fixture("spin", false, true);
    let ini =
        "[UI]\nMSStartingCash=20000\nMSMinCash=10000\nMSMaxCash=30000\nMSCashIncrement=5000\n";
    let mut menu = menu(ini, &install, &vfs);
    ui.show(39);
    menu.pressed(&vfs, &mut ui, 7110);
    let off = |ui: &Interface| (flag(ui, 11511, DISABLED), flag(ui, 11512, DISABLED));
    assert_eq!(off(&ui), (false, false));
    assert_eq!(
        menu.pressed(&vfs, &mut ui, 11512),
        ["cash 15000 hard up on down on"]
    );
    assert_eq!(
        menu.pressed(&vfs, &mut ui, 11512),
        ["cash 10000 hard up on down off"]
    );
    assert_eq!(off(&ui), (false, true));
    // Pressed again at the limit, nothing moves, and the other spinner brings the first back.
    assert_eq!(
        menu.pressed(&vfs, &mut ui, 11512),
        ["cash 10000 hard up on down off"]
    );
    assert_eq!(text(&ui, 11510), "#10,000");
    for _ in 0..5 {
        menu.pressed(&vfs, &mut ui, 11511);
    }
    assert_eq!(text(&ui, 11510), "#30,000");
    assert_eq!(text(&ui, 11531), "Mid");
    assert_eq!(off(&ui), (true, false));
    assert_eq!(written(&install).unwrap(), "[UI]\nMSStartingCash=30000\n");
    // Opened again, the screen starts from what was written (the menu keeps it too).
    ui.set_text(11510, b"");
    menu.pressed(&vfs, &mut ui, 7110);
    assert_eq!(text(&ui, 11510), "#30,000");
}

#[test]
fn an_amount_typed_in_the_field_is_read_when_the_edit_ends_and_limited() {
    let (install, vfs, mut ui) = fixture("typed", false, true);
    let ini = "[UI]\nMSStartingCash=75000\nMSMinCash=10000\nMSMaxCash=500000\n";
    let mut menu = menu(ini, &install, &vfs);
    ui.show(39);
    menu.pressed(&vfs, &mut ui, 7110);
    // Focus the field, type, and leave it by Enter.
    let (x, y, _, _) = ui.tree.rect(ui.tree.find(11510).unwrap());
    ui.mouse(Mouse::Move(x + 3, y + 3));
    ui.mouse(Mouse::Down);
    ui.mouse(Mouse::Up);
    for c in b"195000" {
        ui.char(*c);
    }
    assert!(ui.key(crate::app::Key::Enter, false));
    let mut log = Vec::new();
    for action in ui.take_actions() {
        log.extend(menu.action(&vfs, &mut ui, action));
    }
    assert_eq!(log, ["cash 195000 easy up on down on"]);
    assert_eq!(text(&ui, 11510), "#195,000");
    assert!(written(&install).unwrap().contains("MSStartingCash=195000"));
    // The symbol and the grouping are taken out; too little and too much are limited.
    for (typed, want) in [
        ("#12,500", "cash 12500 hard up on down on"),
        ("1", "cash 10000 hard up on down off"),
        ("99999999", "cash 500000 easy up off down on"),
        ("", "cash 10000 hard up on down off"),
        ("-5", "cash 10000 hard up on down off"),
    ] {
        ui.set_text(11510, typed.as_bytes());
        let log = menu.action(&vfs, &mut ui, Action::Edited(11510));
        assert_eq!(log, [want], "{typed:?}");
    }
    // Other fields and lists that end an edit are not the menu's business.
    assert!(menu.action(&vfs, &mut ui, Action::Edited(1)).is_empty());
    assert!(menu.action(&vfs, &mut ui, Action::Row(7, 3)).is_empty());
}

#[test]
fn without_maps_the_list_has_its_own_line_and_play_is_off() {
    let (install, vfs, mut ui) = fixture("nomaps", false, false);
    let mut menu = menu("", &install, &vfs);
    ui.show(39);
    let log = menu.pressed(&vfs, &mut ui, 7110);
    assert_eq!(log[0], "freeform 0");
    assert_eq!(log[1], "map none");
    assert_eq!(text_of_row(&ui, 11504, 0), "No maps found.");
    assert!(flag(&ui, 11513, DISABLED));
    assert!(menu.pressed(&vfs, &mut ui, 11513).is_empty());
    // The defaults: 50,000 is middle, with both spinners on.
    assert_eq!(log[2], "cash 50000 middle up on down on");
    // Back is noted.
    assert_eq!(menu.pressed(&vfs, &mut ui, 11514), ["back"]);
}

fn text_of_row(ui: &Interface, id: i32, row: usize) -> String {
    match &ui.tree.elements[ui.tree.find(id).unwrap()].kind {
        crate::ui::layout::Kind::ListBox(l) => String::from_utf8(l.rows.rows[row].text.clone()),
        _ => panic!("no list"),
    }
    .unwrap()
}

#[test]
fn a_tree_without_the_screens_is_left_alone() {
    let install = Install::new(
        "screens-empty",
        &[("ui/root.lyt", b"[LayoutInfo]\nid=1\n".to_vec())],
    );
    let vfs = Vfs::open(&install.0).unwrap();
    let tree = Tree::load(&vfs, "ui/root.lyt", 0);
    let mut ui = Interface::new(tree, Box::new(Texts));
    let mut menu = menu("", &install, &vfs);
    for id in [7110, 7111, 50008, 11513, 11511, 11512, 50009] {
        menu.pressed(&vfs, &mut ui, id);
    }
    menu.action(&vfs, &mut ui, Action::Edited(11510));
}
