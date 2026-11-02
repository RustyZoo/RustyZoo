use super::*;
use crate::app::Mouse;
use crate::config::Config;
use crate::sprite::tests::Install;
use crate::ui::draw::Strings;
use crate::ui::interface::Action;
use crate::ui::layout::{DISABLED, Expansion, Kind, Tree};

/// A type of the class with a definition file of this text.
fn ty(class: &'static str, type_name: &str, subtype: &str, ini: &str) -> EntityType {
    EntityType {
        class,
        type_name: type_name.into(),
        subtype: subtype.into(),
        file: None,
        config: Some(std::sync::Arc::new(Config::parse(ini.as_bytes()).unwrap())),
        name_id: 0,
        footprint: [1, 1, 1],
        height: 0,
    }
}

fn catalogue(types: Vec<EntityType>) -> Catalogue {
    let mut cat = Catalogue::default();
    cat.types = types;
    cat
}

fn names(cat: &Catalogue, list: &[usize]) -> Vec<String> {
    list.iter()
        .map(|&i| String::from_utf8_lossy(&cat.types[i].subtype).into_owned())
        .collect()
}

/// A path type named `name` with these members, a cost and a name id.
fn path(name: &str, members: &str, cost: u32, id: i32) -> EntityType {
    let ini = format!("[Member]\n{members}\n[Characteristics/Integers]\ncPurchaseCost={cost}\n");
    EntityType {
        name_id: id,
        ..ty("paths", "paths", name, &ini)
    }
}

#[test]
fn a_list_holds_the_members_of_its_tag_sorted_by_cost() {
    let cat = catalogue(vec![
        path("c", "paths\nall", 30, 1),
        path("a", "foliage", 10, 2),
        path("b", "paths", 20, 3),
        ty(
            "paths",
            "paths",
            "none",
            "[Characteristics/Integers]\ncPurchaseCost=5\n",
        ),
        ty("paths", "paths", "nofile", ""),
    ]);
    assert_eq!(
        names(&cat, &items(&cat, &Availability::default(), "paths", 0)),
        ["b", "c"],
        "ascending cost"
    );
    assert_eq!(
        names(&cat, &items(&cat, &Availability::default(), "foliage", 0)),
        ["a"]
    );
    assert!(items(&cat, &Availability::default(), "rocks", 0).is_empty());
    assert!(items(&catalogue(vec![]), &Availability::default(), "paths", 0).is_empty());
}

/// A type with a definition file and no name.
fn typed(class: &'static str, sub: &str, ini: &str) -> EntityType {
    ty(class, "t", sub, ini)
}

#[test]
fn the_fence_list_is_sorted_filter_tank_walls_by_show_then_by_height_then_the_rest() {
    let member = "[Member]\nfence\n";
    let wall = |class, extra: &str, own: &str| {
        let ini = format!("{member}{extra}[f/Characteristics/Integers]\n{own}");
        typed(class, "f", &ini)
    };
    let cat = catalogue(vec![
        wall("fences", "", "cHeight=9\n"),                 // 0: no exhibit
        wall("fences", "[HabitatFence]\n", "cHeight=1\n"), // 1
        wall(
            "tankwall",
            "[HabitatFence]\n[TankFence]\n",
            "cIsShowFence=1\ncPurchaseCost=5\n",
        ), // 2
        wall("fences", "[HabitatFence]\n", "cHeight=3\n"), // 3
        wall(
            "tankwall",
            "[HabitatFence]\n[TankFence]\n",
            "cIsShowFence=0\ncPurchaseCost=50\n",
        ), // 4
        typed(
            "objects",
            "f",
            &format!("{member}[Characteristics/Integers]\ncPurchaseCost=1\n"),
        ), // 5 filter
        wall("fences", "[HabitatFence]\n", "cHeight=3\n"), // 6
        wall(
            "tankwall",
            "[HabitatFence]\n[TankFence]\n",
            "cPurchaseCost=20\n",
        ), // 7 show missing = 0
        wall(
            "tankwall",
            "[TankFence]\n",
            "cIsShowFence=1\ncPurchaseCost=1\n",
        ), // 8
    ]);
    // Filter, tanks without show (7 by cost 20, 4 by cost 50), tanks with show (8, 2), heights 3
    // (stable: 3 then 6), 1, then the fence of no exhibit.
    assert_eq!(
        items(&cat, &Availability::default(), "fence", 0),
        [5, 7, 4, 8, 2, 3, 6, 1, 0]
    );
}

#[test]
fn foliage_is_sorted_by_habitat_with_the_bad_ones_last_then_location_then_cost() {
    let plant = |habitat: Option<i32>, location: Option<i32>, cost: u32| {
        let h = habitat.map_or(String::new(), |h| format!("cHabitat={h}\n"));
        let l = location.map_or(String::new(), |l| format!("cLocation={l}\n"));
        let ini =
            format!("[Member]\nfoliage\n[Characteristics/Integers]\n{h}{l}cPurchaseCost={cost}\n");
        typed("objects", "x", &ini)
    };
    let cat = catalogue(vec![
        plant(None, None, 1),             // 0: 9410 by default, last
        plant(Some(9414), Some(1), 1),    // 1: 9414 after both 9410
        plant(Some(9500), Some(9700), 5), // 2
        plant(Some(9400), Some(9700), 9), // 3
        plant(Some(9500), Some(9600), 7), // 4
        plant(Some(9500), Some(9600), 6), // 5
        plant(Some(9410), Some(5), 0),    // 6: last, before 0 by location
    ]);
    assert_eq!(
        items(&cat, &Availability::default(), "foliage", 0),
        [3, 5, 4, 2, 6, 0, 1]
    );
}

#[test]
fn a_button_shows_the_icon_of_the_step_or_the_first() {
    let icons: Vec<String> = ["se", "sw", "nw", "ne"].map(String::from).to_vec();
    let shown: Vec<_> = (0..5)
        .map(|step| icon_at(&icons, step).unwrap().as_str())
        .collect();
    assert_eq!(shown, ["se", "sw", "nw", "ne", "se"]);
    let two = &icons[..2];
    assert_eq!(icon_at(two, 1).unwrap(), "sw");
    assert_eq!(icon_at(two, 2).unwrap(), "se", "not step % len");
    assert_eq!(icon_at(&[], 1), None);
}

#[test]
fn a_type_that_is_not_available_at_the_start_is_left_out() {
    let with = |value: &str| {
        let ini = format!("[Global]\nAvailableAtStartup={value}\n[Member]\npaths\n");
        ty("paths", "paths", value, &ini)
    };
    let cat = catalogue(vec![
        with("1"),
        with("0"),
        with("2"),
        with("-1"),
        with("0x1"),
        ty("paths", "paths", "missing", "[Member]\npaths\n"),
    ]);
    assert_eq!(
        names(&cat, &items(&cat, &Availability::default(), "paths", 0)),
        ["1", "0x1", "missing"]
    );
}

#[test]
fn an_expansion_type_shows_when_it_is_in_all_or_one_of_its_expansions_is_installed() {
    let with = |name: &str, members: &str| {
        ty(
            "paths",
            "paths",
            name,
            &format!("[Member]\npaths\n{members}\n"),
        )
    };
    let cat = catalogue(vec![
        with("base", ""),
        with("dino", "dinosaur"),
        with("aqua", "aqua"),
        with("both", "dinosaur\naqua"),
        with("all", "aqua\nall"),
    ]);
    let shown = |mask| names(&cat, &items(&cat, &Availability::default(), "paths", mask));
    assert_eq!(shown(0), ["base", "all"]);
    assert_eq!(shown(1), ["base", "dino", "both", "all"]);
    assert_eq!(shown(2), ["base", "aqua", "both", "all"]);
    assert_eq!(shown(3), ["base", "dino", "aqua", "both", "all"]);
    assert_eq!(shown(-1), shown(3), "a mask with more bits than expansions");
}

#[test]
fn the_fence_list_holds_fences_and_not_their_gates() {
    let fence = |class, sub: &str| ty(class, "wall", sub, "[Member]\nfence\n");
    let cat = catalogue(vec![
        fence("fences", "f"),
        fence("fences", "g"),
        fence("tankwall", "f"),
        fence("tankwall", "g"),
    ]);
    let list = items(&cat, &Availability::default(), "fence", 0);
    assert_eq!(list, [0, 2]);
}

#[test]
fn the_cost_of_a_fence_is_in_the_section_of_its_subtype() {
    let plain = ty(
        "objects",
        "other",
        "x",
        "[Characteristics/Integers]\ncPurchaseCost = 12.5\n",
    );
    assert_eq!(cost(&plain), 12.5);
    let ini = "[Characteristics/Integers]\ncPurchaseCost=1\n[f/Characteristics/Integers]\ncPurchaseCost=150\n";
    assert_eq!(cost(&ty("fences", "glass", "f", ini)), 150.0);
    assert_eq!(
        cost(&ty("fences", "glass", "g", ini)),
        1.0,
        "no section of its own"
    );
    assert_eq!(
        cost(&ty("objects", "other", "f", ini)),
        1.0,
        "scenery reads the plain one"
    );
    assert_eq!(cost(&ty("paths", "paths", "p", "")), 0.0);
    let junk = "[Characteristics/Integers]\ncPurchaseCost=abc\n";
    assert_eq!(cost(&ty("paths", "paths", "p", junk)), 0.0);
}

#[test]
fn the_icons_come_from_the_first_section_that_has_some() {
    let ini = "[Icon]\nIcon=a/b\nIcon=c/d\n[f/Icon]\nIcon=own/f\n[Global]\nDefaultSubtype=g\n[g/Icon]\nIcon=own/g\n";
    assert_eq!(icons(&ty("objects", "o", "f", ini)), ["a/b", "c/d"]);
    assert_eq!(icons(&ty("fences", "o", "f", ini)), ["own/f"]);
    assert_eq!(
        icons(&ty("fences", "o", "z", ini)),
        ["own/g"],
        "the default subtype's"
    );
    assert_eq!(icons(&ty("fences", "o", "z", "[Icon]\nIcon=x\n")), ["x"]);
    assert!(icons(&ty("fences", "o", "z", "[Icon]\n")).is_empty());
    assert!(icons(&ty("paths", "o", "z", "")).is_empty());
}

// ---- The panels

struct Texts;

impl Strings for Texts {
    fn string(&self, id: i32) -> String {
        match id {
            22100 => "$".into(),
            22900 => "All".into(),
            22901 => "Base".into(),
            22902 => "Dino".into(),
            22903 => "Aqua".into(),
            2024 => "Dinosaurs".into(),
            11 => "Fences".into(),
            12 => "Paths".into(),
            13 => "Foliage".into(),
            14 => "Buildings".into(),
            2025 => "Creatures".into(),
            3614 => "Cares".into(),
            3297 => "none".into(),
            3298 => "exhibit".into(),
            3299 => "show".into(),
            101..=199 => format!("Name{id}"),
            _ => String::new(),
        }
    }

    fn text_entry(&self, _: &[u8]) -> Vec<u8> {
        vec![]
    }
}

const ROOT: &str = "[LayoutInfo]\nid=1\nx=0\ny=0\ndx=whole\ndy=whole\n\
    [a]\ntype=ZTMapView\nid=7000\ndx=whole\ndy=whole\n\
    [b]\ntype=UILayout\nlayout=ui/c.lyt\n[c]\ntype=UILayout\nlayout=ui/o.lyt\n\
    [d]\ntype=UIButton\nid=1002\ndx=5\ndy=5\nstate=2048\naction=3\ntarget=8\n\
    [e]\ntype=UIButton\nid=1001\ndx=5\ndy=5\nstate=2048\naction=3\ntarget=4\n\
    [f]\ntype=UILayout\nlayout=ui/a.lyt\n[h]\ntype=UILayout\nlayout=ui/p.lyt\n\
    [g]\ntype=UILayout\nlayout=ui/x.lyt\n";

/// One panel: the layout `id`, title, expansion button, list (with its radio set, template and
/// bar from `list` + 1 to + 3), the tabs, the rotate buttons, the name and the cost.
fn panel(
    id: i32,
    title: i32,
    xpac: i32,
    list: i32,
    tabs: &[(i32, &str, i32)],
    more: &str,
) -> String {
    let mut text = format!(
        "[LayoutInfo]\nid={id}\nx=0\ny=0\ndx=100\ndy=100\nstate=1\n\
         [t]\ntype=UIText\nid={title}\ndx=50\ndy=10\n\
         [x]\ntype=UIButton\nid={xpac}\ndx=10\ndy=10\n\
         [r]\ntype=UIScrollingRegion\nid={list}\nx=0\ny=0\ndx=22\ndy=24\nradioset={}\n\
         template={}\nscrollbar={}\nspacing=2\nspacing=4\n\
         [s]\ntype=UIRadioSet\nid={}\n\
         [p]\ntype=UIButton\nid={}\ndx=10\ndy=6\ntransparent=0\nstate=256\nstate=4096\n\
         [u]\ntype=UIScrollBar\nid={}\ndx=3\ndy=10\n{more}",
        list + 1,
        list + 2,
        list + 3,
        list + 1,
        list + 2,
        list + 3
    );
    let ids: String = tabs.iter().map(|t| format!("button={}\n", t.0)).collect();
    text += &format!("[set]\ntype=UIRadioSet\nid=9{id}\n{ids}");
    for (n, (tab, tag, help)) in tabs.iter().enumerate() {
        let selected = if n == 0 { "state=8\n" } else { "" };
        text += &format!(
            "[tab{n}]\ntype=UIButton\nid={tab}\nhelpid={help}\ndx=5\ndy=5\nstate=4096\n\
             {selected}stringData={tag}\n"
        );
    }
    text
}

/// A TGA image of one colour.
fn tga(w: u16, h: u16) -> Vec<u8> {
    let mut out = vec![0, 0, 2, 0, 0, 0, 0, 0, 0, 0, 0, 0];
    out.extend(w.to_le_bytes());
    out.extend(h.to_le_bytes());
    out.extend([24, 0x20]);
    out.extend(vec![200; usize::from(w) * usize::from(h) * 3]);
    out
}

pub(crate) fn interface(name: &str) -> (Interface, Vfs, Install) {
    let construction = panel(
        8,
        3261,
        3204,
        3208,
        &[
            (3251, "fence", 11),
            (3056, "paths", 12),
            (3252, "foliage", 13),
            (3256, "rocks", 11),
        ],
        "[rl]\ntype=UIButton\nid=3258\ndx=5\ndy=5\nstate=1\n\
         [rr]\ntype=UIButton\nid=3259\ndx=5\ndy=5\nstate=1\n\
         [n]\ntype=UIText\nid=3263\ndx=50\ndy=10\n[k]\ntype=UIText\nid=3207\ndx=50\ndy=10\n\
         [e]\ntype=UIText\nid=3284\ndx=50\ndy=10\n\
         [fl]\ntype=UIText\nid=3296\ndx=50\ndy=10\n[fv]\ntype=UIText\nid=3299\ndx=50\ndy=10\n\
         [g]\ntype=UIButton\nid=3240\ndx=5\ndy=5\nstate=2048\n\
         [li]\ntype=UIImage\nid=3283\n[hi]\ntype=UIImage\nid=3281\n\
         [hb]\ntype=UIImage\nid=3280\n\
         [z]\ntype=UIButton\nid=3362\ndx=5\ndy=5\nstate=4096\n",
    );
    let objects = panel(
        4,
        3061,
        3005,
        3019,
        &[(3050, "structures", 14), (3053, "scenery", 14)],
        "[rl]\ntype=UIButton\nid=3054\ndx=5\ndy=5\nstate=1\n\
         [rr]\ntype=UIButton\nid=3055\ndx=5\ndy=5\nstate=1\n\
         [n]\ntype=UIText\nid=3058\ndx=50\ndy=10\n[k]\ntype=UIText\nid=3016\ndx=50\ndy=10\n",
    );
    let adopt = panel(
        3,
        2023,
        2005,
        2019,
        &[(2075, "animals", 14), (3255, "shelters", 14)],
        "[m]\ntype=UIButton\nid=2000\ndx=5\ndy=5\nstate=4096\nstate=8\n\
         [fe]\ntype=UIButton\nid=2001\ndx=5\ndy=5\nstate=4096\n\
         [sx]\ntype=UIRadioSet\nid=2040\nbutton=2000\nbutton=2001\n\
         [n]\ntype=UIText\nid=2043\ndx=50\ndy=10\n[k]\ntype=UIText\nid=2014\ndx=50\ndy=10\n\
         [e]\ntype=UIText\nid=2082\ndx=50\ndy=10\n[em]\ntype=UIText\nid=2009\ndx=50\ndy=10\n\
         [li]\ntype=UIImage\nid=2045\n[hi]\ntype=UIImage\nid=2047\n[hb]\ntype=UIImage\nid=2046\n",
    );
    let hire = panel(
        10,
        3697,
        3601,
        3602,
        &[],
        "[rl]\ntype=UIButton\nid=3676\ndx=5\ndy=5\nstate=1\n\
         [rr]\ntype=UIButton\nid=3677\ndx=5\ndy=5\nstate=1\n\
         [n]\ntype=UIText\nid=3620\ndx=50\ndy=10\n[k]\ntype=UIText\nid=3611\ndx=50\ndy=10\n\
         [d]\ntype=UIText\nid=3613\ndx=50\ndy=10\n[em]\ntype=UIText\nid=3609\ndx=50\ndy=10\n",
    );
    let install = Install::new(
        &format!("build-{name}"),
        &[
            ("ui/root.lyt", ROOT.as_bytes().to_vec()),
            ("ui/a.lyt", adopt.into_bytes()),
            ("ui/p.lyt", hire.into_bytes()),
            ("ui/c.lyt", construction.into_bytes()),
            ("ui/o.lyt", objects.into_bytes()),
            (
                "ui/x.lyt",
                b"[LayoutInfo]\nid=152\nx=0\ny=0\ndx=60\ndy=60\nstate=1\n\
                  [l]\ntype=UIListBox\nid=22900\nx=0\ny=0\ndx=50\ndy=50\n"
                    .to_vec(),
            ),
            (
                "ui/infoimg.cfg",
                b"[Images]\n; c\n9400 =  ui/h.tga \nlabel\n9600=ui/l.tga\n9601=ui/gone.tga\n"
                    .to_vec(),
            ),
            ("ui/h.tga", tga(4, 2)),
            ("ui/l.tga", tga(5, 3)),
        ],
    );
    let vfs = Vfs::open(&install.0).unwrap();
    let mut tree = Tree::load(&vfs, "ui/root.lyt", 0);
    assert_eq!(tree.failure, None);
    tree.resolve(200, 200, |_| 12);
    (Interface::new(tree, Box::new(Texts)), vfs, install)
}

fn scenery(
    class: &'static str,
    kind: &str,
    sub: &str,
    tag: &str,
    cost: u32,
    name: i32,
) -> EntityType {
    let ini = format!("[Member]\n{tag}\n[Characteristics/Integers]\ncPurchaseCost={cost}\n");
    EntityType {
        name_id: name,
        ..ty(class, kind, sub, &ini)
    }
}

fn types() -> Catalogue {
    catalogue(vec![
        scenery("paths", "paths", "dirt", "paths", 10, 101),
        scenery("paths", "paths", "sand", "paths", 20, 102),
        scenery("fences", "glass", "f", "fence", 150, 103),
        scenery("fences", "glass", "g", "fence", 150, 104),
        scenery("objects", "foliage", "tree", "foliage", 1234, 105),
        scenery("building", "building", "shop", "structures", 50, 106),
        scenery("building", "building", "stand", "structures", 60, 107),
    ])
}

fn state_of(ui: &Interface, id: i32) -> u32 {
    ui.tree.elements[ui.tree.find(id).unwrap()].state
}

fn text(ui: &Interface, id: i32) -> String {
    String::from_utf8_lossy(&ui.text(id).unwrap()).into_owned()
}

/// Presses the tab `id` as the mouse would: selected, the others of its panel not, then the
/// panel is told.
fn tab(
    ui: &mut Interface,
    build: &mut Build,
    (vfs, cat): (&Vfs, &Catalogue),
    id: i32,
    log: &mut Vec<String>,
) {
    let set = [3251, 3056, 3252, 3256, 3362, 3050, 3053];
    for other in set {
        ui.set_button(other, other == id, true);
    }
    build.selected(ui, vfs, cat, id, log);
}

#[test]
fn opening_the_construction_panel_fills_the_fences_and_chooses_the_first() {
    let (mut ui, vfs, _install) = interface("open");
    let cat = types();
    let (mut build, mut log) = (Build::new(3), vec![]);
    build.sync(&mut ui, &vfs, &cat, &mut log);
    assert!(log.is_empty(), "nothing is open: {log:?}");
    assert!(ui.show(8));
    build.sync(&mut ui, &vfs, &cat, &mut log);
    assert_eq!(
        log,
        [
            "tool panel construction",
            "tool item fence fences/glass/f name=Name103 cost=$150"
        ]
    );
    assert_eq!(
        (text(&ui, 3263), text(&ui, 3207)),
        ("Name103".into(), "$150".into())
    );
    assert_eq!(
        (text(&ui, 3261), text(&ui, 3204)),
        ("Fences".into(), "All".into())
    );
    assert_eq!(build.tool(), Some(2));
    // Fences do not rotate; the gate button shows on this tab, disabled without an exhibit.
    assert_eq!(state_of(&ui, 3258) & HIDDEN, HIDDEN);
    assert_eq!(state_of(&ui, 3240) & (HIDDEN | DISABLED), DISABLED);
    // The button is enabled while the zoo has an exhibit, and stays so when the tab is shown again.
    build.set_exhibits(&mut ui, 1, &mut log);
    assert_eq!(state_of(&ui, 3240) & (HIDDEN | DISABLED), 0);
    build.set_exhibits(&mut ui, 0, &mut log);
    assert_eq!(state_of(&ui, 3240) & (HIDDEN | DISABLED), DISABLED);
    // The same state again says nothing.
    log.clear();
    build.sync(&mut ui, &vfs, &cat, &mut log);
    assert!(log.is_empty());
}

#[test]
fn the_gate_button_makes_the_gate_the_tool_until_an_item_a_tab_escape_or_the_last_exhibit() {
    let (mut ui, vfs, _install) = interface("gate");
    let cat = types();
    let (mut build, mut log) = (Build::new(3), vec![]);
    ui.show(8);
    ui.set_button(1002, true, true);
    build.sync(&mut ui, &vfs, &cat, &mut log);
    build.set_exhibits(&mut ui, 1, &mut log);
    assert_eq!(build.tool(), Some(2));
    log.clear();
    // Pressing the button clears the item and the list selection.
    ui.set_button(3240, true, true);
    build.sync(&mut ui, &vfs, &cat, &mut log);
    assert_eq!(log, ["tool gate on"]);
    assert!(build.gating());
    assert_eq!(build.tool(), None);
    let first = 1_000_000 + 3208 * 1000;
    assert_eq!(state_of(&ui, first) & SELECTED, 0);
    // A second press ends it; so does Escape, with the button released.
    ui.set_button(3240, false, true);
    log.clear();
    build.sync(&mut ui, &vfs, &cat, &mut log);
    assert_eq!(
        (log.clone(), build.gating()),
        (vec!["tool gate off".into()], false)
    );
    ui.set_button(3240, true, true);
    build.sync(&mut ui, &vfs, &cat, &mut log);
    log.clear();
    assert!(build.cancel(&mut ui, &mut log));
    assert_eq!(log, ["tool gate off", "tool drop"]);
    assert_eq!(state_of(&ui, 3240) & SELECTED, 0);
    assert_eq!(state_of(&ui, 8) & HIDDEN, 0, "the panel stays");
    // Choosing an item ends it.
    ui.set_button(3240, true, true);
    build.sync(&mut ui, &vfs, &cat, &mut log);
    build.selected(&mut ui, &vfs, &cat, first, &mut log);
    assert!(!build.gating());
    assert_eq!(state_of(&ui, 3240) & SELECTED, 0);
    assert_eq!(build.tool(), Some(2));
    // The last exhibit going ends it and disables the button.
    ui.set_button(3240, true, true);
    build.sync(&mut ui, &vfs, &cat, &mut log);
    log.clear();
    build.set_exhibits(&mut ui, 0, &mut log);
    assert_eq!(log, ["tool gate off"]);
    assert_eq!(state_of(&ui, 3240) & (SELECTED | DISABLED), DISABLED);
    // Closing the panel ends it.
    build.set_exhibits(&mut ui, 1, &mut log);
    ui.set_button(3240, true, true);
    build.sync(&mut ui, &vfs, &cat, &mut log);
    ui.tree.hide(ui.tree.find(8).unwrap());
    log.clear();
    build.sync(&mut ui, &vfs, &cat, &mut log);
    assert!(!build.gating());
}

#[test]
fn a_tab_lists_its_tag_and_remembers_its_item() {
    let (mut ui, vfs, _install) = interface("tabs");
    let cat = types();
    let (mut build, mut log) = (Build::new(3), vec![]);
    ui.show(8);
    build.sync(&mut ui, &vfs, &cat, &mut log);
    log.clear();
    let env = (&vfs, &cat);
    tab(&mut ui, &mut build, env, 3056, &mut log);
    assert_eq!(
        log,
        ["tool item paths paths/paths/dirt name=Name101 cost=$10"],
        "the first item"
    );
    assert_eq!(text(&ui, 3261), "Paths");
    // Choose the second path by its button.
    log.clear();
    build.selected(&mut ui, &vfs, &cat, 1_000_000 + 3208 * 1000 + 1, &mut log);
    assert_eq!(
        log,
        ["tool item paths paths/paths/sand name=Name102 cost=$20"]
    );
    assert_eq!(build.tool(), Some(1));
    // Another tab, and back: the second path is chosen again, selected in the list.
    log.clear();
    tab(&mut ui, &mut build, env, 3252, &mut log);
    assert_eq!(
        log,
        ["tool item foliage objects/foliage/tree name=Name105 cost=$1,234"]
    );
    assert_eq!(text(&ui, 3207), "$1,234");
    assert_eq!(state_of(&ui, 3258) & HIDDEN, 0, "scenery rotates");
    log.clear();
    tab(&mut ui, &mut build, env, 3056, &mut log);
    assert_eq!(
        log,
        ["tool item paths paths/paths/sand name=Name102 cost=$20"]
    );
    assert_eq!(state_of(&ui, 3258) & HIDDEN, HIDDEN);
    let second = 1_000_000 + 3208 * 1000 + 1;
    assert_ne!(state_of(&ui, second) & SELECTED, 0);
    // A tab with nothing to list hides the list and drops the tool.
    log.clear();
    tab(&mut ui, &mut build, env, 3256, &mut log);
    assert!(log.is_empty());
    assert_eq!(
        (build.tool(), text(&ui, 3263), text(&ui, 3207)),
        (None, "".into(), "".into())
    );
    assert_eq!(state_of(&ui, 3208) & HIDDEN, HIDDEN);
    // The terrain tabs have no list of items either.
    tab(&mut ui, &mut build, env, 3362, &mut log);
    assert_eq!(build.tool(), None);
    // Back on a tab with items, the list shows again.
    tab(&mut ui, &mut build, env, 3251, &mut log);
    assert_eq!(state_of(&ui, 3208) & HIDDEN, 0);
    assert_eq!(build.tool(), Some(2));
}

#[test]
fn the_objects_panel_opens_through_the_radio_set_and_the_other_closes() {
    let (mut ui, vfs, _install) = interface("objects");
    let cat = types();
    let (mut build, mut log) = (Build::new(3), vec![]);
    ui.show(8);
    ui.set_button(1002, true, true);
    build.sync(&mut ui, &vfs, &cat, &mut log);
    log.clear();
    // The left bar hides one panel and shows the other.
    ui.tree.hide(ui.tree.find(8).unwrap());
    ui.show(4);
    build.sync(&mut ui, &vfs, &cat, &mut log);
    assert_eq!(
        log,
        [
            "tool panel none",
            "tool panel objects",
            "tool item structures building/building/shop name=Name106 cost=$50"
        ]
    );
    assert_eq!(state_of(&ui, 1002) & SELECTED, 0, "its button is released");
    assert_eq!(text(&ui, 3061), "Buildings");
    assert_eq!(state_of(&ui, 3054) & HIDDEN, 0, "buildings rotate");
    // The rotate buttons turn the panel's step, 0 to 3 either way, and refill the list.
    for (button, want) in [(3054, 3), (3054, 2), (3055, 3), (3055, 0), (3055, 1)] {
        log.clear();
        build.selected(&mut ui, &vfs, &cat, button, &mut log);
        assert_eq!(log[0], format!("tool rotate objects {want}"));
        assert_eq!(build.tool(), Some(5), "the item stays chosen");
    }
    // The other panel's step is its own.
    assert_eq!(build.step, [0, 1, 0, 0]);
}

#[test]
fn cancel_drops_the_tool_then_closes_the_panel() {
    let (mut ui, vfs, _install) = interface("cancel");
    let cat = types();
    let (mut build, mut log) = (Build::new(3), vec![]);
    assert!(!build.cancel(&mut ui, &mut log), "nothing is open");
    ui.show(8);
    ui.set_button(1002, true, true);
    build.sync(&mut ui, &vfs, &cat, &mut log);
    log.clear();
    assert!(build.cancel(&mut ui, &mut log));
    assert_eq!(
        (log.clone(), build.tool()),
        (vec!["tool drop".to_string()], None)
    );
    assert_eq!(state_of(&ui, 8) & HIDDEN, 0, "the panel stays");
    let first = 1_000_000 + 3208 * 1000;
    assert_eq!(state_of(&ui, first) & SELECTED, 0, "the selection is clear");
    log.clear();
    assert!(build.cancel(&mut ui, &mut log));
    assert_eq!(log, ["tool panel none"]);
    assert_eq!(state_of(&ui, 8) & HIDDEN, HIDDEN);
    assert_eq!(state_of(&ui, 1002) & SELECTED, 0);
    assert!(!build.cancel(&mut ui, &mut log));
    // Opening again chooses the remembered item again.
    log.clear();
    ui.show(8);
    build.sync(&mut ui, &vfs, &cat, &mut log);
    assert_eq!(build.tool(), Some(2));
}

#[test]
fn the_close_button_and_a_missing_layout_are_no_trouble() {
    let (mut ui, vfs, _install) = interface("close");
    let cat = types();
    let (mut build, mut log) = (Build::new(3), vec![]);
    ui.show(4);
    build.sync(&mut ui, &vfs, &cat, &mut log);
    ui.tree.hide(ui.tree.find(4).unwrap());
    build.sync(&mut ui, &vfs, &cat, &mut log);
    assert_eq!(log.last().unwrap(), "tool panel none");
    assert_eq!(build.tool(), None);
    // A catalogue without any type: the panel opens with an empty list.
    let (mut build, mut log) = (Build::new(0), vec![]);
    ui.show(8);
    build.sync(&mut ui, &vfs, &catalogue(vec![]), &mut log);
    assert_eq!(log, ["tool panel construction"]);
    assert_eq!(state_of(&ui, 3208) & HIDDEN, HIDDEN);
}

/// With the real files of the game, the lists are not empty and the first path is a path.
#[test]
#[ignore]
fn the_full_game_has_items_in_every_tab() {
    let dir = std::env::var("ZT_DATA_DIR").unwrap();
    let vfs = Vfs::open(std::path::Path::new(&dir)).unwrap();
    let cat = Catalogue::load(&vfs).unwrap();
    for tag in [
        "fence",
        "paths",
        "foliage",
        "rocks",
        "structures",
        "scenery",
    ] {
        let list = items(&cat, &Availability::default(), tag, 3);
        assert!(list.len() > 10, "{tag}: {}", list.len());
        assert!(
            list.iter().all(|&i| !icons(&cat.types[i]).is_empty()),
            "{tag} has icons"
        );
    }
    let paths = items(&cat, &Availability::default(), "paths", 3);
    assert_eq!(cat.types[paths[0]].class, "paths");
}

#[test]
fn the_era_and_the_fence_descriptor_follow_the_chosen_item() {
    let (mut ui, vfs, _install) = interface("details");
    let wall = |class, sub: &str, extra: &str, own: &str, id| EntityType {
        name_id: id,
        ..typed(
            class,
            sub,
            &format!("[Member]\nfence\n{extra}[f/Characteristics/Integers]\n{own}"),
        )
    };
    let cat = catalogue(vec![
        wall("fences", "f", "", "cHeight=1\n", 102),
        wall("fences", "f", "[HabitatFence]\n", "cHeight=1\n", 103),
        wall(
            "tankwall",
            "f",
            "[HabitatFence]\n[TankFence]\n",
            "cIsShowFence=1\ncEra=102\n",
            104,
        ),
        scenery("paths", "paths", "dirt", "paths", 10, 101),
    ]);
    let (mut build, mut log) = (Build::new(3), vec![]);
    ui.show(8);
    build.sync(&mut ui, &vfs, &cat, &mut log);
    // The show tank wall is first: its era and the show descriptor.
    assert_eq!(build.tool(), Some(2));
    let shown = |ui: &Interface, id| state_of(ui, id) & HIDDEN == 0;
    assert!(shown(&ui, 3284) && shown(&ui, 3296) && shown(&ui, 3299));
    assert_eq!(
        (text(&ui, 3284), text(&ui, 3299)),
        ("Name102".into(), "show".into())
    );
    // The next two: an exhibit fence and a fence of no exhibit, with no era.
    let item = |n| 1_000_000 + 3208 * 1000 + n;
    build.selected(&mut ui, &vfs, &cat, item(1), &mut log);
    assert_eq!(
        (text(&ui, 3299), shown(&ui, 3284)),
        ("exhibit".into(), false)
    );
    build.selected(&mut ui, &vfs, &cat, item(2), &mut log);
    assert_eq!((text(&ui, 3299), shown(&ui, 3299)), ("none".into(), true));
    // A path has neither.
    tab(&mut ui, &mut build, (&vfs, &cat), 3056, &mut log);
    assert!(!shown(&ui, 3284) && !shown(&ui, 3296) && !shown(&ui, 3299));
}

#[test]
fn a_tab_keeps_the_rows_it_showed_even_without_the_chosen_item() {
    let (mut ui, vfs, _install) = interface("scroll");
    let paths = (0..5).map(|n| scenery("paths", "paths", &format!("p{n}"), "paths", 10 + n, 101));
    let cat = catalogue(paths.collect());
    let (mut build, mut log) = (Build::new(3), vec![]);
    let env = (&vfs, &cat);
    ui.show(8);
    build.sync(&mut ui, &vfs, &cat, &mut log);
    tab(&mut ui, &mut build, env, 3056, &mut log);
    assert_eq!(ui.top(3208), 0);
    // The last item is at the third of three rows, two of which fit: the list scrolls to it.
    build.selected(&mut ui, &vfs, &cat, 1_000_000 + 3208 * 1000 + 4, &mut log);
    assert_eq!(ui.top(3208), 1);
    // The player scrolls up; another tab and back leave the rows as they were.
    ui.set_top(3208, 0);
    tab(&mut ui, &mut build, env, 3252, &mut log);
    tab(&mut ui, &mut build, env, 3056, &mut log);
    assert_eq!((ui.top(3208), build.tool()), (0, Some(4)));
    // So does closing the panel and opening it again.
    ui.set_top(3208, 1);
    assert!(build.cancel(&mut ui, &mut log) && build.cancel(&mut ui, &mut log));
    assert_eq!(build.tool(), None);
    ui.show(8);
    build.sync(&mut ui, &vfs, &cat, &mut log);
    assert_eq!((ui.top(3208), build.tool()), (1, Some(4)));
}

#[test]
fn a_panel_reopens_on_the_tab_it_was_closed_on_and_on_the_first_after_a_load() {
    let (mut ui, vfs, _install) = interface("reopen");
    let cat = types();
    let (mut build, mut log) = (Build::new(3), vec![]);
    ui.show(8);
    build.sync(&mut ui, &vfs, &cat, &mut log);
    tab(&mut ui, &mut build, (&vfs, &cat), 3056, &mut log);
    assert!(build.cancel(&mut ui, &mut log) && build.cancel(&mut ui, &mut log));
    ui.show(8);
    build.sync(&mut ui, &vfs, &cat, &mut log);
    assert_eq!(Build::tab(&ui, 0), Some(1), "the paths tab");
    assert_eq!(text(&ui, 3261), "Paths");
    assert!(build.cancel(&mut ui, &mut log) && build.cancel(&mut ui, &mut log));
    Build::first_tabs(&mut ui);
    let mut build = Build::new(3);
    ui.show(8);
    build.sync(&mut ui, &vfs, &cat, &mut log);
    assert_eq!(Build::tab(&ui, 0), Some(0));
    assert_eq!(text(&ui, 3261), "Fences");
}

#[test]
fn a_type_that_turns_by_itself_advances_the_pending_step_and_choosing_resets_it() {
    let turns = ty(
        "objects",
        "tree",
        "",
        "[Characteristics/Integers]\ncAutoRotate=1\n",
    );
    let fixed = ty(
        "objects",
        "bench",
        "",
        "[Characteristics/Integers]\ncAutoRotate=0\n",
    );
    let mut build = Build::new(3);
    build.step[0] = 2;
    build.turn = 2;
    assert!(!build.placed(&fixed));
    assert_eq!(build.step(), 2);
    // Three placements wrap round past 3; the panel's own step stays.
    for want in [3, 0, 1] {
        assert!(build.placed(&turns));
        assert_eq!(build.step(), want);
    }
    assert_eq!(build.step, [2, 0, 0, 0]);
}

#[test]
fn a_press_with_a_path_as_the_tool_starts_a_drag_whose_first_leg_the_first_move_chooses() {
    let mut cat = Catalogue::default();
    let path = |class| crate::entity::EntityType {
        class,
        type_name: b"dirt".to_vec(),
        subtype: Vec::new(),
        file: None,
        config: None,
        name_id: 0,
        footprint: [2, 2, 1],
        height: 0,
    };
    cat.push(path("paths"));
    cat.push(path("objects"));
    let mut build = Build::new(0);
    let tiles = |build: &mut Build, cell| -> Vec<(i64, i64)> {
        let points = build.run(cell, (0, 0), 0);
        points.iter().map(|&(x, y)| (x / 64, y / 64)).collect()
    };
    // Without the tool, or with a tool that is not a path, a press starts nothing.
    build.press(&cat, Some((5, 5)), (0, 0));
    assert!(!build.dragging());
    build.hold(1);
    build.press(&cat, Some((5, 5)), (0, 0));
    assert!(!build.dragging());
    assert!(build.run((7, 5), (0, 0), 0).is_empty());
    // A press off the map starts nothing either.
    build.hold(0);
    build.press(&cat, None, (0, 0));
    assert!(!build.dragging());
    build.press(&cat, Some((u32::MAX, 5)), (0, 0));
    assert!(!build.dragging());
    // The first tile entered decides the leg, and it holds until the mouse is back on the start.
    build.press(&cat, Some((5, 5)), (0, 0));
    assert!(build.dragging());
    assert_eq!(tiles(&mut build, (5, 5)), [(5, 5)]);
    assert_eq!(tiles(&mut build, (6, 5)), [(5, 5), (6, 5)]);
    assert_eq!(tiles(&mut build, (7, 6)), [(5, 5), (6, 5), (7, 5), (7, 6)]);
    assert_eq!(tiles(&mut build, (6, 7)), [(5, 5), (6, 5), (6, 6), (6, 7)]);
    assert_eq!(tiles(&mut build, (5, 5)), [(5, 5)]);
    assert_eq!(tiles(&mut build, (6, 7)), [(5, 5), (5, 6), (5, 7), (6, 7)]);
    // Releasing, or dropping the tool, ends it.
    build.end_drag();
    assert!(!build.dragging());
    assert!(build.run((6, 7), (0, 0), 0).is_empty());
}

#[test]
fn a_press_with_a_fence_as_the_tool_keeps_the_side_of_the_press_for_the_whole_run() {
    let mut cat = Catalogue::default();
    cat.push(crate::entity::EntityType {
        class: "fences",
        type_name: b"wire".to_vec(),
        subtype: b"f".to_vec(),
        file: None,
        config: None,
        name_id: 0,
        footprint: [1, 1, 1],
        height: 0,
    });
    let mut build = Build::new(0);
    build.hold(0);
    // Pressed at (10, 40) of tile (5, 5): side 6, the west side. The run goes along y first.
    build.press(&cat, Some((5, 5)), (5 * 64 + 10, 5 * 64 + 40));
    assert!(build.dragging());
    let (x, y) = (5 * 64, 5 * 64 + 32);
    assert_eq!(build.run((5, 5), (x + 20, y), 0), [(x, y)]);
    // Past the middle of the tile the run hooks round on the side the mouse is on.
    assert_eq!(
        build.run((5, 5), (x + 40, y), 0),
        [(x, y), (5 * 64 + 32, 5 * 64 + 63)]
    );
    // The mouse two tiles down and one tile right: the pieces of the first leg are side 6 and
    // the second leg lies on side 0 or 4 of the tiles it crosses.
    let run = build.run((6, 7), (6 * 64 + 32, 7 * 64 + 32), 0);
    assert_eq!(run[..3], [(x, y), (x, y + 64), (x, y + 128)]);
    assert!(run.len() > 3);
    // A press off the map, or at an absurd point, never panics.
    build.press(&cat, None, (i64::MIN, i64::MAX));
    assert!(!build.dragging());
    build.press(&cat, Some((5, 5)), (i64::MAX, i64::MIN));
    assert!(build.dragging());
}

#[test]
fn the_location_and_habitat_pictures_follow_the_chosen_item() {
    let (mut ui, vfs, _install) = interface("pictures");
    let item = |habitat: i32, location: i32| {
        let ini = format!(
            "[Member]\nfoliage\n[Characteristics/Integers]\ncHabitat={habitat}\ncLocation={location}\n"
        );
        EntityType {
            name_id: 105,
            ..ty("objects", "foliage", "t", &ini)
        }
    };
    let cat = catalogue(vec![
        item(9400, 9600),
        item(9410, 9600),
        item(9414, 9600),
        item(9400, 9999),
        item(9400, 9601),
        item(9400, 0),
        ty("objects", "foliage", "none", "[Member]\nfoliage\n"),
    ]);
    let (mut build, mut log) = (Build::new(3), vec![]);
    ui.show(8);
    build.sync(&mut ui, &vfs, &cat, &mut log);
    tab(&mut ui, &mut build, (&vfs, &cat), 3252, &mut log);
    // The list puts the types with a habitat first, by habitat and then location; the hidden
    // habitats 9410 and 9414 come last.
    let shown = |ui: &Interface, id| state_of(ui, id) & HIDDEN == 0;
    let rect = |ui: &Interface, id| ui.tree.rect(ui.tree.find(id).unwrap());
    let help = |ui: &Interface, id| ui.tree.elements[ui.tree.find(id).unwrap()].help_id.clone();
    let mut seen = vec![];
    for at in 0..build.list.len() {
        build.selected(
            &mut ui,
            &vfs,
            &cat,
            1_000_000 + 3208 * 1000 + at as i32,
            &mut log,
        );
        let ty = &cat.types[build.tool().unwrap()];
        seen.push((
            number(ty, "cLocation"),
            shown(&ui, 3283),
            shown(&ui, 3281),
            shown(&ui, 3280),
        ));
    }
    assert_eq!(
        seen,
        [
            (Some(0.0), false, true, true), // a value without a picture is blank
            (Some(9600.0), true, true, true),
            (Some(9601.0), false, true, true),
            (Some(9999.0), false, true, true),
            (None, false, false, false),
            (Some(9600.0), false, false, false),
            (Some(9600.0), false, false, false),
        ]
    );
    // The first item with both pictures: their size, and their values as tooltips.
    build.selected(&mut ui, &vfs, &cat, 1_000_000 + 3208 * 1000 + 1, &mut log);
    assert_eq!((rect(&ui, 3283).2, rect(&ui, 3283).3), (5, 3));
    assert_eq!((rect(&ui, 3281).2, rect(&ui, 3281).3), (4, 2));
    assert_eq!((help(&ui, 3283), help(&ui, 3281)), (vec![9600], vec![9400]));
}

#[test]
fn the_picture_table_reads_numbers_and_ignores_the_rest() {
    let (_ui, vfs, _install) = interface("table");
    let table = infoimg(&vfs);
    assert_eq!(table.get(&9400).map(String::as_str), Some("ui/h.tga"));
    assert_eq!(table.get(&9600).map(String::as_str), Some("ui/l.tga"));
    assert_eq!(table.len(), 3, "a bare label has no picture");
    let nothing = Install::new("table-none", &[("zoo.txt", vec![])]);
    assert!(infoimg(&Vfs::open(&nothing.0).unwrap()).is_empty());
}

#[test]
fn a_fence_run_is_computed_again_only_past_the_tile_the_height_step_or_17_world_units() {
    let mut cat = Catalogue::default();
    cat.push(crate::entity::EntityType {
        class: "fences",
        type_name: b"wire".to_vec(),
        subtype: b"f".to_vec(),
        file: None,
        config: None,
        name_id: 0,
        footprint: [1, 1, 1],
        height: 0,
    });
    let mut build = Build::new(0);
    build.hold(0);
    // Side 0 of tile (5, 5); the mouse two tiles along x, in the upper half: no hook piece.
    build.press(&cat, Some((5, 5)), (5 * 64 + 32, 5 * 64 + 1));
    let (x, y) = (7 * 64 + 40, 5 * 64 + 26);
    let first = build.run((7, 5), (x, y), 0);
    assert_eq!(first.len(), 3);
    // Eight units down in the same tile, now in the lower half: the old run stays.
    assert_eq!(build.run((7, 5), (x, y + 8), 0), first);
    // Small moves add up from the last computation: 17 units from it, the hook comes.
    assert_eq!(build.run((7, 5), (x, y + 16), 0), first);
    assert_eq!(build.run((7, 5), (x + 16, y + 16), 0), first);
    assert_eq!(build.run((7, 5), (x, y + 17), 0).len(), 4);
    // Another tile, or another height step, computes again.
    assert_eq!(build.run((7, 5), (x, y), 0).len(), 3);
    assert_eq!(build.run((7, 5), (x, y + 8), 16).len(), 4);
    assert_eq!(build.run((7, 5), (x + 17, y), 16).len(), 3);
    // A new press forgets the last computation.
    build.press(&cat, Some((5, 5)), (5 * 64 + 32, 5 * 64 + 1));
    assert_eq!(build.run((7, 5), (x, y + 8), 0).len(), 4);
}

#[test]
fn a_bulldozer_press_is_a_drag_once_it_leaves_its_tile_or_16_pixels() {
    let mut build = Build::new(0);
    // Without the tool a press holds nothing.
    build.sweep_press(Some((5, 5)), (100, 100));
    assert!(!build.sweeping());
    build.bulldozer = true;
    // Off the map: no press.
    build.sweep_press(None, (100, 100));
    assert!(!build.sweeping());
    build.sweep_press(Some((5, 5)), (100, 100));
    assert!(build.sweeping());
    // On the press tile within 16 pixels it is a click.
    assert_eq!(build.sweep_area(Some((5, 5)), (116, 100)), None);
    assert_eq!(build.sweep_area(Some((5, 5)), (110, 110)), None);
    assert_eq!(build.sweep_area(Some((5, 5)), (112, 112)), None);
    assert_eq!(build.sweep_area(Some((5, 5)), (116, 84)), None);
    assert_eq!(build.sweep_area(Some((5, 5)), (84, 100)), None);
    // More than 16 pixels away on one axis on the same tile, or on another tile: a drag.
    assert!(build.sweep_area(Some((5, 5)), (100, 117)).is_some());
    assert!(build.sweep_area(Some((5, 5)), (83, 100)).is_some());
    assert_eq!(
        build.sweep_area(Some((5, 5)), (117, 100)),
        Some(((5, 5), (5, 5)))
    );
    assert_eq!(
        build.sweep_area(Some((7, 4)), (101, 100)),
        Some(((5, 5), (7, 4)))
    );
    // Off the map the last tile stays; back on the press tile it is a click again.
    assert_eq!(build.sweep_area(None, (101, 100)), Some(((5, 5), (7, 4))));
    assert_eq!(build.sweep_area(Some((5, 5)), (101, 100)), None);
    // Extreme points do not overflow.
    build.sweep_area(Some((5, 5)), (i32::MAX, i32::MIN));
    build.end_sweep();
    assert_eq!(build.sweep_area(Some((5, 5)), (0, 0)), None);
}

/// An animal type of the Adopt panel: the keys are in the section of its default subtype `m`.
fn animal(name: &str, sub: &str, keys: &str, id: i32) -> EntityType {
    let ini = format!(
        "[Global]\nDefaultSubtype=m\n[Member]\nanimals\n[m/Characteristics/Integers]\n{keys}"
    );
    EntityType {
        name_id: id,
        ..ty("animals", name, sub, &ini)
    }
}

#[test]
fn the_adopt_panel_lists_each_species_once_as_the_sex_chosen_in_the_order_of_the_data() {
    let (mut ui, vfs, _install) = interface("adopt");
    let keys = |habitat: u32, family: u32, cost: u32| {
        format!(
            "cHabitat={habitat}\ncLocation=9600\ncFamily={family}\ncGenus=1\ncPurchaseCost={cost}\n"
        )
    };
    let cat = catalogue(vec![
        animal("zebra", "m", &keys(9400, 5204, 800), 101),
        animal("zebra", "f", &keys(9400, 5204, 800), 101),
        animal("lion", "m", &keys(9400, 5200, 700), 102),
        animal("lion", "f", &keys(9400, 5200, 700), 102),
        animal("camel", "m", &keys(9300, 5300, 90), 103),
        animal("zebra", "y", &keys(9400, 5204, 800), 101),
        scenery("objects", "shelter", "hut", "shelters", 50, 104),
    ]);
    let (mut build, mut log) = (Build::new(3), vec![]);
    build.set_month(10);
    assert!(build.october());
    assert!(ui.show(3));
    build.sync(&mut ui, &vfs, &cat, &mut log);
    // Males, by habitat then family: the camel, the lion, the zebra; the camel is chosen.
    assert_eq!(build.list, [4, 2, 0]);
    assert_eq!(log[0], "tool panel adopt");
    assert_eq!(
        log[1],
        "tool item animals animals/camel/m name=Name103 cost=$90"
    );
    assert_eq!(
        (text(&ui, 2023), text(&ui, 2005)),
        ("Creatures".into(), "All".into())
    );
    // The zebra's button, then the Female button: the same species, as a female.
    log.clear();
    build.selected(&mut ui, &vfs, &cat, 1_000_000 + 2019 * 1000 + 2, &mut log);
    assert_eq!(build.tool(), Some(0));
    ui.set_button(2000, false, true);
    ui.set_button(2001, true, true);
    build.selected(&mut ui, &vfs, &cat, 2001, &mut log);
    assert_eq!(build.list, [3, 1]);
    assert_eq!(build.tool(), Some(1), "the zebra stays");
    assert_eq!(text(&ui, 2043), "Name101");
    // A tab of other types lists them whatever their subtype.
    ui.set_button(2075, false, true);
    ui.set_button(3255, true, true);
    build.selected(&mut ui, &vfs, &cat, 3255, &mut log);
    assert_eq!(build.list, [6]);
    assert_eq!(state_of(&ui, 2009) & HIDDEN, HIDDEN);
}

/// A staff type of the Hire panel: the keys are in the section of its subtype.
fn hand(class: &'static str, name: &str, sub: &str, members: &str, id: i32) -> EntityType {
    let ini = format!(
        "[Member]\n{members}\n[{sub}/Characteristics/Integers]\ncPurchaseCost=800\n\
         cMonthlyCost=650.4\ncDutiesTextID=3614\n"
    );
    EntityType {
        name_id: id,
        ..ty(class, name, sub, &ini)
    }
}

#[test]
fn the_hire_panel_lists_the_staff_of_the_sex_drawn_by_kind_with_salary_and_duties() {
    let (mut ui, vfs, _install) = interface("hire");
    let cat = catalogue(vec![
        hand("keeper", "keeper", "m", "staff", 101),
        hand("keeper", "keeper", "f", "staff", 101),
        hand("maint", "maint", "m", "staff", 102),
        hand("maint", "maint", "f", "staff", 102),
        hand("keeper", "scient", "m", "staff\ndinosaur", 103),
        hand("keeper", "scient", "f", "staff\ndinosaur", 103),
        scenery("objects", "shelter", "hut", "shelters", 50, 104),
        hand("tour", "tour", "m", "staff", 105),
        hand("tour", "tour", "f", "staff", 105),
    ]);
    let (mut build, mut log) = (Build::new(0), vec![]);
    build.set_hire(true);
    assert!(ui.show(10));
    build.sync(&mut ui, &vfs, &cat, &mut log);
    // The men sorted by kind, the keeper class, the tour guide, then the maintenance worker; the
    // scientist needs the expansion.
    assert_eq!(build.list, [0, 7, 2]);
    assert_eq!(log[0], "tool panel hire");
    assert_eq!(
        log[1],
        "tool item staff keeper/keeper/m name=Name101 cost=$650"
    );
    assert_eq!(
        (
            text(&ui, 3697),
            text(&ui, 3620),
            text(&ui, 3611),
            text(&ui, 3613)
        ),
        ("".into(), "Name101".into(), "$650".into(), "Cares".into())
    );
    // Choosing the item made a pending staff member, and so does the new list's choice.
    assert_eq!(build.take_made(), [0]);
    assert_eq!(state_of(&ui, 3676) & HIDDEN, HIDDEN, "staff do not turn");
    assert_eq!(state_of(&ui, 3609) & HIDDEN, HIDDEN);
    // A hire draws a woman: the list is made again and the same kind of staff stays chosen.
    log.clear();
    build.set_hire(false);
    build.rehire(&mut ui, &vfs, &cat, &mut log);
    assert_eq!((build.list.clone(), build.tool()), (vec![1, 8, 3], Some(1)));
    assert_eq!(build.take_made(), [1]);
    // With the expansion the scientist is listed with the zookeeper, before the tour guide.
    assert_eq!(
        items_of(&cat, &Availability::default(), "staff", 1.into(), |t| t
            .subtype
            == b"m"),
        [0, 4, 7, 2]
    );
    // Nothing of the sex drawn: the empty text shows, the list is empty and the choice, the
    // details and the pending staff member stay (unit placing 4.3).
    let none = catalogue(vec![]);
    build.rehire(&mut ui, &vfs, &none, &mut log);
    assert!(build.list.is_empty());
    assert_eq!((build.tool(), state_of(&ui, 3609) & HIDDEN), (Some(1), 0));
    assert!(build.take_made().is_empty(), "nothing chosen, nothing made");
    assert_eq!(text(&ui, 3620), "Name101");
}

#[test]
fn the_hire_panel_ends_with_the_drt_base_that_shows_the_helicopter_details() {
    let (_ui, vfs, _install) = interface("hire-drt");
    let heli = || EntityType {
        name_id: 150,
        ..ty(
            "helicopter",
            "helicopter",
            "",
            "[Member]\nstaff\ndinosaur\n[Characteristics/Integers]\ncMonthlyCost=2000\n\
             cDutiesTextID=3614\n",
        )
    };
    let base = |ini: &str| EntityType {
        name_id: 8031,
        ..ty("building", "helibase", "", ini)
    };
    let cat = |base: EntityType| {
        catalogue(vec![
            base,
            heli(),
            hand("keeper", "keeper", "m", "staff", 101),
            hand("maint", "maint", "m", "staff", 102),
        ])
    };
    let dino = "[Member]\nstaff\ndinosaur\n[Characteristics/Integers]\ncPurchaseCost=2000\n";
    let (cat_on, cat_off) = (
        cat(base(dino)),
        cat(base(&dino.replace("dinosaur", "aqua"))),
    );
    let sync = |cat: &Catalogue, on: i32| {
        let (mut ui, vfs, _install) = interface("hire-drt");
        let (mut build, mut log) = (Build::new(on), vec![]);
        build.set_hire(true);
        assert!(ui.show(10));
        build.sync(&mut ui, &vfs, cat, &mut log);
        (build, ui)
    };
    // With the expansion: after the staff, choosing it shows the rotate buttons and the helicopter.
    let (mut build, mut ui) = sync(&cat_on, 1);
    assert_eq!(build.list, [2, 3, 0]);
    build.take_made();
    build.choose(&mut ui, &vfs, &cat_on, HIRE_PANEL, 2, &mut vec![]);
    assert_eq!(state_of(&ui, 3676) & HIDDEN, 0);
    assert_eq!(
        (text(&ui, 3620), text(&ui, 3611), text(&ui, 3613)),
        ("Name150".into(), "$2,000".into(), "Cares".into())
    );
    assert!(
        build.take_made().is_empty(),
        "a building is no pending staff member"
    );
    // Without it, or not a member of the expansion, or not available, the base is not listed.
    assert_eq!(sync(&cat_on, 0).0.list, [2, 3]);
    assert_eq!(sync(&cat_off, 1).0.list, [2, 3]);
    let (mut build, mut ui) = sync(&cat_on, 1);
    let mut log = vec![];
    assert!(build.set_available(&mut ui, &vfs, &cat_on, 8031, false, &mut log));
    assert_eq!(build.list, [2, 3]);
}

#[test]
fn a_flag_change_makes_the_open_list_again() {
    let (mut ui, vfs, _install) = interface("avail");
    let cat = types();
    let (mut build, mut log) = (Build::new(3), vec![]);
    ui.show(8);
    build.sync(&mut ui, &vfs, &cat, &mut log);
    assert_eq!(build.list, [2]);
    macro_rules! set {
        ($id:expr, $on:expr) => {
            build.set_available(&mut ui, &vfs, &cat, $id, $on, &mut log)
        };
    }
    assert!(!set!(0, false), "no type has the id 0");
    assert!(!set!(103, true), "already available");
    assert_eq!(build.list, [2]);
    assert!(set!(103, false));
    assert!(build.list.is_empty());
    assert!(!set!(104, true), "that subtype is not in the fences tab");
    assert!(set!(103, true));
    assert_eq!(build.list, [2]);
    // Closed, a change is kept for the next opening.
    build.cancel(&mut ui, &mut log);
    build.cancel(&mut ui, &mut log);
    assert!(set!(103, false));
    ui.show(8);
    build.sync(&mut ui, &vfs, &cat, &mut log);
    assert!(build.list.is_empty());
}

#[test]
fn a_new_games_tree_hides_its_unfinished_targets_before_the_ini_rewards_raise_theirs() {
    use crate::research::{Branch, Category, Program, Tree};
    let program = |target, cost| Program {
        name: 1,
        cost,
        progress: 0.0,
        target,
        effect: 0,
        vals: [0; 3],
        order: 0,
        icon: String::new(),
        ..Program::default()
    };
    let mut tree = Tree {
        branches: vec![Branch {
            name: 1,
            funding: vec![],
            level: 0,
            current: None,
            categories: vec![Category {
                name: 1,
                expansion: 0,
                tick: true,
                programs: vec![program(101, 5.0), program(102, 5.0), program(103, 0.0)],
                ..Default::default()
            }],
            ..Default::default()
        }],
        ..Tree::default()
    };
    let cat = types();
    let ini = Config::parse(b"[user]\nunlockCount=1\nunlockEntity0=102\n").unwrap();
    let mut build = Build::new(3);
    let flags = tree.apply(&cat, &mut Default::default()).0;
    build.start_research(&cat, &flags);
    build.after_triggers(&cat, &ini, &[]);
    let flags: Vec<bool> = (0..4).map(|i| build.avail.is(&cat, i)).collect();
    assert_eq!(flags, [false, true, true, true]);
}

#[test]
fn the_ini_rewards_apply_after_the_start_triggers_whatever_they_hide() {
    use crate::scenario::{Avail, Which};
    let cat = types();
    let ini = Config::parse(b"[user]\nunlockCount=1\nunlockEntity0=102\n").unwrap();
    let flags = |build: &Build| -> Vec<bool> { (0..7).map(|i| build.avail.is(&cat, i)).collect() };
    let hide = |which| Avail {
        which,
        on: false,
        tell: None,
    };
    // Hide all: the reward stays, and a trigger after hide-all can make another one available.
    let mut build = Build::new(3);
    let made = [
        hide(Which::All),
        Avail {
            which: Which::Name(105),
            on: true,
            tell: None,
        },
    ];
    assert!(build.after_triggers(&cat, &ini, &made));
    assert_eq!(
        flags(&build),
        [false, true, false, false, true, false, false]
    );
    // A block, a name id of 0 or a negative one that no type has; and nothing to do.
    let mut build = Build::new(3);
    let made = [
        hide(Which::Names(vec![101, 103])),
        hide(Which::Name(0)),
        hide(Which::Name(-4)),
    ];
    build.after_triggers(&cat, &Config::default(), &made);
    assert_eq!(flags(&build), [false, true, false, true, true, true, true]);
    assert!(!build.after_triggers(&cat, &Config::default(), &[]));
}

#[test]
fn an_unlock_trigger_makes_the_type_available_whatever_the_run_and_hide_all_comes_before() {
    use crate::scenario::{Avail, Which};
    let cat = types();
    let tell = |which, on| Avail {
        which,
        on,
        tell: None,
    };
    let mut build = Build::new(3);
    assert!(build.change(&cat, &[tell(Which::All, false)]));
    assert!(!build.avail.is(&cat, 1));
    assert!(build.change(&cat, &[tell(Which::Unlock(102), true)]));
    assert!(build.avail.is(&cat, 1) && !build.avail.is(&cat, 0));
    // Again, a bad id and a missing type change nothing.
    for id in [102, 0, -5, 9999] {
        assert!(
            !build.change(&cat, &[tell(Which::Unlock(id), true)]),
            "{id}"
        );
    }
}

// ---- The expansion selector

/// A path type of these expansion members (none: the base game's).
fn member(name: &str, members: &str) -> EntityType {
    path(name, &format!("paths\n{members}"), 10, 1)
}

#[test]
fn the_selected_expansion_filters_the_list() {
    let cat = catalogue(vec![
        member("none", ""),
        member("zoo", "zoo"),
        member("dino", "dinosaur"),
        member("aqua", "aqua"),
        member("both", "dinosaur\naqua"),
        member("all", "aqua\nall"),
    ]);
    let shown = |on, pick| {
        let filter = Filter { on, pick };
        names(
            &cat,
            &items(&cat, &Availability::default(), "paths", filter),
        )
    };
    // On one expansion: its members and the types of no expansion under the base game, whatever
    // is installed or off.
    assert_eq!(shown(7, Some(1)), ["none", "zoo", "all"]);
    assert_eq!(shown(0, Some(2)), ["dino", "both", "all"]);
    assert_eq!(shown(0, Some(3)), ["aqua", "both", "all"]);
    // On All: unless every expansion the type is a member of is off.
    assert_eq!(
        shown(7, None),
        ["none", "zoo", "dino", "aqua", "both", "all"]
    );
    assert_eq!(shown(4, None), ["none", "zoo", "all"]);
    assert_eq!(shown(1, None), ["none", "dino", "both", "all"]);
    assert_eq!(shown(0, None), ["none", "all"]);
}

/// The Dino-and-Aqua install of the fixture: rows 1 to 3 are the entries 0 to 2.
fn selector(stored: u32) -> Build {
    let entry = |id| Expansion {
        id,
        list: 22901 + id,
    };
    Build::new(3).with_selector(
        vec![entry(0), entry(1), entry(2), entry(7), entry(-1)],
        stored,
    )
}

#[test]
fn the_selector_fills_its_list_and_the_value_comes_from_a_file() {
    let (mut ui, vfs, _install) = interface("selector-rows");
    let (cat, mut log) = (types(), vec![]);
    let mut build = selector(2);
    build.sync(&mut ui, &vfs, &cat, &mut log);
    let Kind::ListBox(l) = &ui.tree.elements[ui.tree.find(22900).unwrap()].kind else {
        panic!("not a list");
    };
    let rows: Vec<_> = l.rows.rows.iter().map(|r| r.text.clone()).collect();
    let rows: Vec<_> = rows.iter().map(|t| String::from_utf8_lossy(t)).collect();
    assert_eq!(
        rows,
        ["All", "Base", "Dino", "Aqua"],
        "ids out of 0 to 2 have no row"
    );
    assert_eq!(l.rows.selected, Some(2));
    assert_eq!(build.value(), 2);
    // A value with no row is held as All: it is what a save writes.
    for stored in [0, 4, 9, u32::MAX] {
        assert_eq!(selector(stored).value(), 0, "{stored}");
    }
    let mut build = selector(2);
    build.set_scenario([false; 4], Some(-3));
    assert_eq!(build.value(), 0);
    build.set_scenario([false; 4], Some(i32::MAX));
    assert_eq!(build.value(), 0);
    build.set_scenario([false, false, true, false], None);
    assert_eq!(build.value(), 0, "no value leaves it");
    build.set_scenario([false; 4], Some(2));
    build.set_scenario([false, false, true, false], None);
    assert_eq!(build.value(), 2, "no value leaves it");
    build.set_scenario([false; 4], Some(3));
    assert_eq!(build.value(), 3);
    // Without rows (the base trial) nothing can be selected.
    assert_eq!(Build::new(0).with_selector(vec![], 3).value(), 0);
}

#[test]
fn choosing_a_row_filters_the_open_panel_and_forgets_what_the_tabs_chose() {
    let (mut ui, vfs, _install) = interface("selector-pick");
    let cat = catalogue(vec![
        scenery("paths", "paths", "dirt", "paths\nzoo", 10, 101),
        scenery("paths", "paths", "sand", "paths\ndinosaur", 20, 102),
        scenery("paths", "paths", "ice", "paths", 30, 103),
        scenery("building", "building", "shop", "structures\naqua", 50, 106),
    ]);
    let (mut build, mut log) = (selector(0), vec![]);
    assert!(ui.show(8));
    build.sync(&mut ui, &vfs, &cat, &mut log);
    tab(&mut ui, &mut build, (&vfs, &cat), 3056, &mut log);
    assert_eq!(build.list, [0, 1, 2]);
    // The Objects panel's tab, chosen before: it will reopen on its first tab.
    ui.set_button(3053, true, true);
    ui.set_button(3050, false, true);
    // The list opens from the button and closes on a choice.
    assert!(ui.show(152));
    log.clear();
    build.pick_row(&mut ui, &vfs, &cat, 3, &mut log);
    assert_eq!(state_of(&ui, 152) & HIDDEN, HIDDEN);
    assert_eq!(log[0], "tool expansion 3");
    assert!(build.list.is_empty(), "no path of Marine Mania");
    assert_eq!(text(&ui, 3204), "Aqua");
    assert_eq!(
        state_of(&ui, 3053) & SELECTED,
        0,
        "the closed panel: first tab"
    );
    assert_ne!(
        state_of(&ui, 3056) & SELECTED,
        0,
        "the open one keeps its tab"
    );
    log.clear();
    build.pick_row(&mut ui, &vfs, &cat, 2, &mut log);
    assert_eq!(build.list, [1]);
    assert_eq!((build.tool(), text(&ui, 3204)), (Some(1), "Dino".into()));
    build.pick_row(&mut ui, &vfs, &cat, 1, &mut log);
    assert_eq!(
        build.list,
        [0, 2],
        "the base game: and the types of no expansion"
    );
    build.pick_row(&mut ui, &vfs, &cat, 0, &mut log);
    assert_eq!(
        (build.list.clone(), text(&ui, 3204)),
        (vec![0, 1, 2], "All".into())
    );
    assert_eq!(
        build.tool(),
        Some(0),
        "the first item again, not the remembered one"
    );
    // None, a row out of the list and a huge one choose nothing and leave the list open.
    log.clear();
    for row in [-1, 4, 5, i32::MIN, i32::MAX] {
        assert!(ui.show(152));
        build.pick_row(&mut ui, &vfs, &cat, row, &mut log);
        assert_eq!(state_of(&ui, 152) & HIDDEN, 0, "{row}");
    }
    assert!(log.is_empty(), "{log:?}");
    assert_eq!(build.value(), 0);
    // A closed panel hides the list too.
    assert!(ui.show(152));
    Build::hide_panels(&mut ui);
    build.sync(&mut ui, &vfs, &cat, &mut log);
    assert_eq!(state_of(&ui, 152) & HIDDEN, HIDDEN);
}

/// Presses and releases the left button on the list row `n` of the selector (the rows are
/// scanned from the top) and gives the actions of the click.
fn click_row(ui: &mut Interface, n: usize) -> Vec<Action> {
    let y = (0..60)
        .find(|&y| {
            ui.mouse(Mouse::Move(5, y));
            ui.mouse(Mouse::Down);
            let found = ui.selected_row(22900) == Some(n);
            ui.mouse(Mouse::Up);
            ui.take_actions();
            found
        })
        .unwrap_or_else(|| panic!("no row {n}"));
    ui.mouse(Mouse::Move(5, y));
    ui.mouse(Mouse::Down);
    ui.mouse(Mouse::Up);
    ui.take_actions()
}

#[test]
fn a_press_outside_the_open_list_closes_it_without_a_choice() {
    let (mut ui, vfs, _install) = interface("selector-blur");
    let (cat, mut log) = (types(), vec![]);
    let mut build = selector(0);
    assert!(ui.show(8));
    build.sync(&mut ui, &vfs, &cat, &mut log);
    log.clear();
    for _ in 0..2 {
        assert!(ui.show(152));
        build.sync(&mut ui, &vfs, &cat, &mut log);
        assert!(ui.has_focus(22900) && build.selector_open(&ui));
        // A press on a row keeps the focus; one on the map view (a corner the list does not cover)
        // takes it, and the list closes without a choice.
        ui.mouse(Mouse::Move(5, 5));
        ui.mouse(Mouse::Down);
        ui.mouse(Mouse::Up);
        ui.take_actions();
        build.sync(&mut ui, &vfs, &cat, &mut log);
        assert_eq!(state_of(&ui, 152) & HIDDEN, 0);
        ui.mouse(Mouse::Move(150, 150));
        ui.mouse(Mouse::Down);
        ui.mouse(Mouse::Up);
        build.sync(&mut ui, &vfs, &cat, &mut log);
        assert_eq!(state_of(&ui, 152) & HIDDEN, HIDDEN);
        assert!(!ui.has_focus(22900) && !build.selector_open(&ui));
    }
    assert!(log.is_empty() && build.value() == 0, "{log:?}");
    // Escape does the same.
    assert!(ui.show(152));
    build.sync(&mut ui, &vfs, &cat, &mut log);
    build.close_selector(&mut ui);
    assert_eq!(state_of(&ui, 152) & HIDDEN, HIDDEN);
    assert!(!ui.has_focus(22900));
    build.sync(&mut ui, &vfs, &cat, &mut log);
    assert_eq!(state_of(&ui, 152) & HIDDEN, HIDDEN);
}

#[test]
fn a_disabled_row_of_the_selector_chooses_nothing() {
    let (mut ui, vfs, _install) = interface("selector-disabled");
    let (cat, mut log) = (types(), vec![]);
    let mut build = selector(0);
    build.set_scenario([false, false, true, true], Some(1));
    assert!(ui.show(8));
    build.sync(&mut ui, &vfs, &cat, &mut log);
    log.clear();
    assert!(ui.show(152));
    build.sync(&mut ui, &vfs, &cat, &mut log);
    let Kind::ListBox(l) = &ui.tree.elements[ui.tree.find(22900).unwrap()].kind else {
        panic!("not a list");
    };
    let off: Vec<bool> = l.rows.rows.iter().map(|r| r.disabled).collect();
    assert_eq!(off, [false, false, true, true]);
    for row in [2, 3] {
        let actions = click_row(&mut ui, row);
        assert_eq!(actions, [Action::Row(22900, -1)], "row {row}");
        for action in actions {
            if let Action::Row(_, r) = action {
                build.pick_row(&mut ui, &vfs, &cat, r, &mut log);
            }
        }
        build.sync(&mut ui, &vfs, &cat, &mut log);
        assert_eq!(state_of(&ui, 152) & HIDDEN, 0, "the list stays open");
        assert!(log.is_empty() && build.value() == 1, "{log:?}");
        assert_eq!(
            ui.selected_row(22900),
            Some(2 - 1),
            "the chosen row stays marked"
        );
    }
    // A new scenario enables them again.
    build.set_scenario([false; 4], None);
    build.sync(&mut ui, &vfs, &cat, &mut log);
    assert_eq!(click_row(&mut ui, 3), [Action::Row(22900, 3)]);
}

#[test]
fn choosing_the_selected_row_again_still_clears() {
    let (mut ui, vfs, _install) = interface("selector-again");
    let (cat, mut log) = (types(), vec![]);
    let mut build = selector(2);
    assert!(ui.show(8));
    build.sync(&mut ui, &vfs, &cat, &mut log);
    tab(&mut ui, &mut build, (&vfs, &cat), 3056, &mut log);
    build.chosen[1][1] = Some(1);
    build.step[0] = 3;
    build.step[1] = 2;
    // The Objects panel is not open: its tab goes back to the first.
    ui.set_button(3053, true, true);
    ui.set_button(3050, false, true);
    assert!(ui.show(152));
    log.clear();
    build.pick_row(&mut ui, &vfs, &cat, 2, &mut log);
    assert_eq!(build.value(), 2);
    assert_eq!(build.chosen, [[None; 4]; 4]);
    assert_eq!(build.step, [0; 4]);
    assert_eq!(state_of(&ui, 3053) & SELECTED, 0);
    assert_eq!(log[0], "tool expansion 2");
    assert_eq!(state_of(&ui, 152) & HIDDEN, HIDDEN);
}

#[test]
fn a_scenario_turns_expansions_off_for_the_all_list() {
    let (mut ui, vfs, _install) = interface("selector-off");
    let cat = catalogue(vec![
        scenery("paths", "paths", "dirt", "paths", 10, 101),
        scenery("paths", "paths", "sand", "paths\ndinosaur", 20, 102),
        scenery("paths", "paths", "ice", "paths\naqua", 30, 103),
    ]);
    let (mut build, mut log) = (selector(0), vec![]);
    build.set_scenario([false, false, true, false], None);
    assert!(ui.show(8));
    build.sync(&mut ui, &vfs, &cat, &mut log);
    tab(&mut ui, &mut build, (&vfs, &cat), 3056, &mut log);
    assert_eq!(build.list, [0, 2]);
    build.set_scenario([false, false, true, true], Some(1));
    assert_eq!(build.value(), 1);
    build.pick_row(&mut ui, &vfs, &cat, 0, &mut log);
    assert_eq!(
        (build.list.clone(), build.value()),
        (vec![0], 0),
        "All with both off"
    );
}

#[test]
fn the_animals_title_follows_the_selector() {
    let (mut ui, vfs, _install) = interface("selector-title");
    let cat = types();
    let (mut build, mut log) = (selector(0), vec![]);
    assert!(ui.show(3));
    build.sync(&mut ui, &vfs, &cat, &mut log);
    assert_eq!(text(&ui, 2023), "Creatures");
    for (row, want) in [
        (3, "Buildings"),
        (2, "Dinosaurs"),
        (1, "Buildings"),
        (0, "Creatures"),
    ] {
        build.pick_row(&mut ui, &vfs, &cat, row, &mut log);
        assert_eq!(text(&ui, 2023), want, "row {row}");
    }
}
