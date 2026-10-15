use super::*;
use crate::sprite::tests::Install;
use crate::ui::draw::Strings as UiStrings;
use crate::ui::layout::{DISABLED, Picture, Tree};
use crate::vfs::Vfs;

/// Made-up texts for the numbers the menu uses.
struct Texts;

impl UiStrings for Texts {
    fn string(&self, id: i32) -> String {
        match id {
            1526 => "Build %s".into(),
            7123 => "Go on with %s".into(),
            37123 => "Long: go on with %s".into(),
            7127 => "Nothing saved".into(),
            37127 => "Long: nothing saved".into(),
            _ => String::new(),
        }
    }

    fn text_entry(&self, _: &[u8]) -> Vec<u8> {
        vec![]
    }
}

/// A 1 x 1 picture.
fn tga() -> Vec<u8> {
    vec![
        0, 0, 2, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1, 0, 1, 0, 24, 0x20, 1, 2, 3,
    ]
}

const BUTTON: &str = "type=UIButton\ndx=10\ndy=10\ntextid=1\n";

/// A root with the map view and the main menu (layout 26, hidden) whose background lists four
/// pictures, with Continue, Zoo Items, Exit and the version label.
fn interface(install: &Install) -> Interface {
    let root = "[LayoutInfo]\nid=1\nx=0\ny=0\ndx=whole\ndy=whole\n\
        [a]\ntype=ZTMapView\nid=7000\ndx=whole\ndy=whole\n\
        [b]\ntype=UILayout\nlayout=ui/startup.lyt\n\
        [c]\ntype=UILayout\nlayout=ui/credits.lyt\n";
    let credits = "[LayoutInfo]\nid=40\nx=0\ny=0\ndx=whole\ndy=whole\nstate=1\n\
        [a]\ntype=UILayout\nlayout=ui/p250.lyt\n[b]\ntype=UILayout\nlayout=ui/p251.lyt\n\
        [x]\ntype=UIButton\nid=12111\naction=2\ntarget=-1\ndx=10\ndy=10\ncolorasicon=1\n";
    let menu = format!(
        "[LayoutInfo]\nid=26\nx=0\ny=0\ndx=whole\ndy=whole\nstate=1\n\
         [bg]\ntype=UIImage\nid=7100\nanimation=ui/p0.tga\nanimation=ui/p1.tga\n\
         animation=ui/p2.tga\nanimation=ui/p3.tga\n\
         [c]\nid=7113\nhelpid=7123\n{BUTTON}[d]\nid=7114\n{BUTTON}[e]\nid=7116\n{BUTTON}\
         [v]\ntype=UIText\nid=7119\ndx=100\ndy=10\n"
    );
    let mut files = vec![
        ("ui/root.lyt", root.as_bytes().to_vec()),
        ("ui/startup.lyt", menu.into_bytes()),
        ("ui/credits.lyt", credits.as_bytes().to_vec()),
        ("ui/p250.lyt", b"[LayoutInfo]\nid=250\n".to_vec()),
        ("ui/p251.lyt", b"[LayoutInfo]\nid=251\n".to_vec()),
    ];
    for name in ["ui/p0.tga", "ui/p1.tga", "ui/p2.tga", "ui/p3.tga"] {
        files.push((name, tga()));
    }
    let name = install
        .0
        .file_name()
        .unwrap()
        .to_string_lossy()
        .into_owned();
    let made = Install::new(&format!("files-{name}"), &files);
    let mut tree = Tree::load(&Vfs::open(&made.0).unwrap(), "ui/root.lyt", 0);
    assert_eq!(tree.failure, None);
    tree.resolve(64, 48, |_| 12);
    Interface::new(tree, Box::new(Texts))
}

fn files(install: &Install) -> Vfs {
    Vfs::open(&install.0).unwrap()
}

fn disabled(ui: &Interface, id: i32) -> bool {
    ui.tree.elements[ui.tree.find(id).unwrap()].state & DISABLED != 0
}

fn tip(ui: &Interface, id: i32) -> i32 {
    ui.tree.elements[ui.tree.find(id).unwrap()].help_id[0]
}

/// A menu over `ini` in an install folder that has the file `saved.zoo`, writing back to `user`.
fn menu(ini: &str, install: &Install, version: Option<&str>, flags: i32) -> Menu {
    Menu::new(
        Config::parse(ini.as_bytes()).unwrap(),
        &install.0,
        Some(install.0.join("user")),
        version.map(String::from),
        flags,
    )
}

fn install(name: &str) -> Install {
    let install = Install::new(name, &[]);
    std::fs::write(install.0.join("saved.zoo"), b"zoo").unwrap();
    std::fs::create_dir(install.0.join("dir.zoo")).unwrap();
    install
}

fn written(install: &Install) -> Option<String> {
    std::fs::read_to_string(install.0.join("user/zoo.ini")).ok()
}

#[test]
fn continue_is_enabled_for_a_file_that_exists_and_its_tooltip_names_it() {
    let install = install("menu-continue");
    let mut ui = interface(&install);
    let mut menu = menu(
        "[user]\nlastfile=Old\\Games\\saved.zoo\n",
        &install,
        None,
        0,
    );
    // The relative path is read below the install folder, with a Windows separator.
    std::fs::create_dir(install.0.join("Old")).unwrap();
    std::fs::create_dir(install.0.join("Old/Games")).unwrap();
    std::fs::copy(
        install.0.join("saved.zoo"),
        install.0.join("Old/Games/saved.zoo"),
    )
    .unwrap();
    let log = menu.show(&mut ui);
    assert_eq!(log, ["continue enabled saved.zoo"]);
    assert!(!disabled(&ui, 7113));
    assert_eq!(tip(&ui, 7113), 7123);
    assert_eq!(ui.string(7123), "Go on with saved.zoo");
    assert_eq!(written(&install), None, "nothing is written back");
    // Shown again, the name is put into the template again, not into the first result.
    menu.show(&mut ui);
    assert_eq!(ui.string(7123), "Go on with saved.zoo");
}

#[test]
fn the_last_file_is_found_whatever_the_letter_case() {
    let install = install("menu-case");
    std::fs::create_dir(install.0.join("Maps")).unwrap();
    std::fs::copy(
        install.0.join("saved.zoo"),
        install.0.join("Maps/Beach.zoo"),
    )
    .unwrap();
    for name in [
        "MAPS\\BEACH.ZOO",
        "maps/beach.zoo",
        "./Maps/beach.ZOO",
        "SAVED.zoo",
    ] {
        let mut ui = interface(&install);
        let mut menu = menu(&format!("[user]\nlastfile={name}\n"), &install, None, 0);
        let log = menu.show(&mut ui);
        assert!(log[0].starts_with("continue enabled "), "{name}: {log:?}");
        assert_eq!(written(&install), None, "{name}: nothing is forgotten");
    }
    // A name that is not there in any case, a directory, and a way out of the folder.
    for name in ["maps/other.zoo", "maps", "maps/beach.zoo/x", "../saved.zoo"] {
        let mut ui = interface(&install);
        let mut menu = menu(&format!("[user]\nlastfile={name}\n"), &install, None, 0);
        assert_eq!(menu.show(&mut ui), ["continue disabled"], "{name}");
    }
}

#[test]
fn long_help_takes_the_strings_30000_further() {
    let install = install("menu-long");
    let mut ui = interface(&install);
    let ini = "[UI]\nhelpType=1\n[user]\nlastfile=saved.zoo\n";
    menu(ini, &install, None, 0).show(&mut ui);
    assert_eq!(ui.string(37123), "Long: go on with saved.zoo");
    assert_eq!(
        ui.string(7123),
        "Go on with %s",
        "the short one is untouched"
    );
    let ini = "[UI]\nhelpType=0\n[user]\nlastfile=saved.zoo\n";
    menu(ini, &install, None, 0).show(&mut ui);
    assert_eq!(ui.string(7123), "Go on with saved.zoo");
}

#[test]
fn continue_is_grey_when_there_is_no_such_file_and_the_name_is_forgotten() {
    for (name, last, forgotten) in [
        ("gone", "gone.zoo", true),
        ("dir", "dir.zoo", true),
        ("blank", "   ", false),
        ("none", "", false),
    ] {
        let install = install(&format!("menu-grey-{name}"));
        let mut ui = interface(&install);
        let mut menu = menu(&format!("[user]\nlastfile={last}\n"), &install, None, 0);
        assert_eq!(menu.show(&mut ui), ["continue disabled"], "{name}");
        assert!(disabled(&ui, 7113), "{name}");
        assert_eq!(tip(&ui, 7113), 7127, "{name}");
        let want = forgotten.then_some("[user]\nlastfile=\n".to_string());
        assert_eq!(written(&install), want, "{name}");
    }
    // No ini key at all, and no folder for the settings.
    let install = install("menu-nokey");
    let mut ui = interface(&install);
    let mut menu = Menu::new(Config::default(), &install.0, None, None, 0);
    assert_eq!(menu.show(&mut ui), ["continue disabled"]);
    let mut gone = Menu::new(
        Config::parse(b"[user]\nlastfile=gone\n").unwrap(),
        &install.0,
        None,
        None,
        0,
    );
    gone.show(&mut ui);
}

#[test]
fn the_picture_follows_the_flags_zoo_items_is_off_and_the_version_is_shown() {
    let install = install("menu-picture");
    for flags in 0..4 {
        let mut ui = interface(&install);
        let all = ui.tree.elements[ui.tree.find(7100).unwrap()]
            .look
            .animation
            .clone();
        assert_eq!(all.len(), 4);
        let mut menu = menu("", &install, Some("1.2.3"), flags);
        let log = menu.show(&mut ui);
        let look = &ui.tree.elements[ui.tree.find(7100).unwrap()].look;
        assert_eq!(look.animation, [all[flags as usize]]);
        assert!(matches!(look.animation[0], Some(Picture::Image(_))));
        assert!(disabled(&ui, 7114));
        assert!(!disabled(&ui, 7116));
        assert_eq!(log, ["continue disabled", "version 1.2.3"]);
        let label = &ui.tree.elements[ui.tree.find(7119).unwrap()].look;
        assert_eq!(label.text_id, [1526]);
        assert_eq!(ui.string(1526), "Build 1.2.3");
    }
    // The flags beyond the two bits are not a picture; without a version the label is empty.
    let mut ui = interface(&install);
    let mut menu = menu("", &install, None, 7);
    assert_eq!(menu.show(&mut ui), ["continue disabled"]);
    assert!(
        ui.tree.elements[ui.tree.find(7119).unwrap()]
            .look
            .text_id
            .is_empty()
    );
    let look = &ui.tree.elements[ui.tree.find(7100).unwrap()].look;
    assert_eq!(look.animation.len(), 1);
    // The menu layout is on the screen now.
    let layout = ui.tree.find(26).unwrap();
    assert_eq!(
        ui.tree.elements[layout].state & crate::ui::layout::HIDDEN,
        0
    );
}

#[test]
fn exit_ends_the_program_and_other_buttons_do_not() {
    let install = install("menu-exit");
    let mut ui = interface(&install);
    let mut menu = menu("", &install, None, 0);
    assert_eq!(
        menu.selected(&files(&install), &mut ui, 7116),
        (vec!["exit".to_string()], true)
    );
    for id in [7112, 7113, 7114, 0, -1, i32::MAX] {
        assert_eq!(
            menu.selected(&files(&install), &mut ui, id),
            (vec![], false),
            "{id}"
        );
    }
}

#[test]
fn a_tree_without_the_menu_is_left_alone() {
    let install = install("menu-empty");
    let made = Install::new(
        "menu-empty-tree",
        &[("ui/root.lyt", b"[LayoutInfo]\nid=1\n".to_vec())],
    );
    let tree = Tree::load(&Vfs::open(&made.0).unwrap(), "ui/root.lyt", 0);
    let mut ui = Interface::new(tree, Box::new(Texts));
    let mut menu = menu("[user]\nlastfile=saved.zoo\n", &install, Some("9"), 3);
    assert_eq!(menu.show(&mut ui), ["continue enabled saved.zoo"]);
}

fn shown(ui: &Interface, id: i32) -> bool {
    ui.tree
        .find(id)
        .is_some_and(|i| ui.tree.elements[i].state & crate::ui::layout::HIDDEN == 0)
}

#[test]
fn the_credits_pages_replace_one_another_on_the_interface_clock() {
    let install = install("menu-credits");
    let mut ui = interface(&install);
    let mut menu = menu("", &install, None, 0);
    // Three pages of 1 s, the last one without a layout; the update is every 400 ms.
    menu.set_credits(vec![250, 251, 999], 1);
    menu.show(&mut ui);
    assert!(menu.update(&mut ui, 400).is_empty(), "nothing is shown yet");
    // The credits button shows the layout, and with it all its pages.
    ui.show(40);
    assert!(shown(&ui, 250) && shown(&ui, 251));
    let (log, exit) = menu.selected(&files(&install), &mut ui, 7115);
    assert_eq!(
        (log, exit),
        (vec!["credits page 1 layout 250".to_string()], false)
    );
    assert!(shown(&ui, 250) && !shown(&ui, 251), "the first page only");
    // The first update (800) only sets the deadline, 1800; 1200 and 1600 are before it, 2000 is after.
    for now in [800, 1200, 1600] {
        assert!(menu.update(&mut ui, now).is_empty());
    }
    assert_eq!(menu.update(&mut ui, 2000), ["credits page 2 layout 251"]);
    assert!(!shown(&ui, 250) && shown(&ui, 251));
    // The update that changed the page set the next deadline, 3000: an update at it changes.
    for now in [2400, 2800] {
        assert!(menu.update(&mut ui, now).is_empty());
    }
    assert_eq!(
        menu.update(&mut ui, 3000),
        ["credits page 3 layout 999 (no layout)"]
    );
    assert!(!shown(&ui, 250) && !shown(&ui, 251) && shown(&ui, 40));
    // After the last page, at its deadline (4000), the credits hide themselves.
    for now in [3400, 3800] {
        assert!(menu.update(&mut ui, now).is_empty());
    }
    assert_eq!(menu.update(&mut ui, 4000), ["credits end"]);
    assert!(!shown(&ui, 40));
    assert!(menu.update(&mut ui, 5200).is_empty());
}

#[test]
fn exit_closes_the_credits_at_once_and_they_start_again_from_the_first_page() {
    let install = install("menu-credits-exit");
    let mut ui = interface(&install);
    let mut menu = menu("", &install, None, 0);
    menu.set_credits(vec![250, 251], 8);
    ui.show(40);
    menu.selected(&files(&install), &mut ui, 7115);
    menu.update(&mut ui, 66);
    // The Exit button hides the layout; the next update sees it.
    let exit = ui.tree.find(12111).unwrap();
    let (x, y, _, _) = ui.tree.rect(exit);
    ui.mouse(crate::app::Mouse::Move(x + 1, y + 1));
    ui.mouse(crate::app::Mouse::Down);
    ui.mouse(crate::app::Mouse::Up);
    assert!(!shown(&ui, 40));
    assert_eq!(menu.update(&mut ui, 132), ["credits closed"]);
    assert!(menu.update(&mut ui, 198).is_empty());
    ui.show(40);
    assert_eq!(
        menu.selected(&files(&install), &mut ui, 7115).0,
        ["credits page 1 layout 250"]
    );
    // No pages at all: the button changes nothing here.
    let mut none = self::menu("", &install, None, 0);
    ui.show(40);
    assert!(none.selected(&files(&install), &mut ui, 7115).0.is_empty());
    assert!(none.update(&mut ui, 264).is_empty());
    // A delay of 0: the first update sets the deadline, the second changes the page.
    let mut quick = self::menu("", &install, None, 0);
    quick.set_credits(vec![250, 251], 0);
    quick.selected(&files(&install), &mut ui, 7115);
    assert!(quick.update(&mut ui, 1).is_empty());
    assert_eq!(quick.update(&mut ui, 1), ["credits page 2 layout 251"]);
    assert_eq!(quick.update(&mut ui, 1), ["credits end"]);
}

#[test]
fn the_page_list_comes_from_the_versioned_registry_files_in_descending_order() {
    let files: Vec<(&str, Vec<u8>)> = vec![
        (
            "ui/credits/credit01.cfg",
            b"[credits]\nversion=1\ndelay=5\n[a]\nlayout=1\n[b]\nlayout=2\n".to_vec(),
        ),
        (
            "ui/credits/credit03.cfg",
            b"[credits]\nversion = 2\n[p1]\nlayout = 30\n[p2]\nlayout=31\n[nolayout]\nx=1\n"
                .to_vec(),
        ),
        (
            "ui/credits/credit02.cfg",
            b"[credits]\nversion=0\ndelay=99\n[a]\nlayout=20\n".to_vec(),
        ),
        (
            "ui/credits/credits.cfg",
            b"[credits]\ndelay=77\n[a]\nlayout=70\n".to_vec(),
        ),
        (
            "ui/credits/credit00.cfg",
            b"[credits]\nversion=x\n[a]\nlayout=10\n".to_vec(),
        ),
        (
            "ui/credits/credit04.cfg",
            b"[credits]\nversion=1\ndelay=-3\n[a]\nlayout=40\n".to_vec(),
        ),
        ("ui/credits/credit05.cfg", vec![0xff; 20]),
    ];
    let install = Install::new("menu-credit-pages", &files);
    let vfs = Vfs::open(&install.0).unwrap();
    // Descending: 05 (damaged), 04 (a delay of -3), 03, 02 (version 0), 01 (the last delay, 5).
    assert_eq!(credit_pages(&vfs), (vec![40, 30, 31, 1, 2], 5));
    let empty = Install::new("menu-credit-none", &[]);
    assert_eq!(credit_pages(&Vfs::open(&empty.0).unwrap()), (vec![], 8));
}
