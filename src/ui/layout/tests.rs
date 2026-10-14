use super::*;
use crate::sprite::tests::Install;

/// A 2 x 1 true colour TGA image.
fn tga() -> Vec<u8> {
    let mut out = vec![0, 0, 2, 0, 0, 0, 0, 0, 0, 0, 0, 0, 2, 0, 1, 0, 24, 0x20];
    out.extend([1, 2, 3, 4, 5, 6]);
    out
}

fn load(name: &str, members: &[(&str, &str)], expansions: i32) -> Tree {
    let mut files: Vec<(&str, Vec<u8>)> = members
        .iter()
        .map(|(n, text)| (*n, text.as_bytes().to_vec()))
        .collect();
    files.push(("ui/pic.tga", tga()));
    let install = Install::new(&format!("ui-{name}"), &files);
    Tree::load(&Vfs::open(&install.0).unwrap(), "ui/root.lyt", expansions)
}

fn ids(tree: &Tree, of: usize) -> Vec<i32> {
    tree.elements[of]
        .children
        .iter()
        .map(|&c| tree.elements[c].id)
        .collect()
}

#[test]
fn sections_come_in_ascending_byte_order_and_sublayouts_keep_their_place() {
    let tree = load(
        "order",
        &[
            (
                "ui/root.lyt",
                "[LayoutInfo]\nid=1\n[b]\ntype=UIImage\nid=2\n[B]\ntype=UIImage\nid=3\n\
                 [a]\ntype=UILayout\nlayout=ui/sub.lyt\nid=99\nstate=2\n[c]\ntype=UIText\nid=4\n",
            ),
            (
                "ui/sub.lyt",
                "[LayoutInfo]\nid=10\nstate=2\n[x]\ntype=UIImage\nid=11\n",
            ),
        ],
        0,
    );
    assert_eq!(tree.failure, None);
    // B (0x42) sorts before a, b, c; the sub-layout is the element of `a`.
    assert_eq!(ids(&tree, 0), [3, 10, 2, 4]);
    // The keys of the UILayout section are ignored: id 99 and state 2 do not count.
    let sub = tree.find(10).unwrap();
    assert_eq!(tree.find(99), None);
    assert_eq!(
        tree.elements[sub].state, 3,
        "hidden and disabled from the layout's own file"
    );
    assert_eq!(ids(&tree, sub), [11]);
    assert_eq!(tree.elements[0].state, 0, "the root is shown");
    assert_eq!(tree.counts()["UIImage"], 3);
}

#[test]
fn flags_defaults_and_words() {
    let tree = load(
        "keys",
        &[(
            "ui/root.lyt",
            "[LayoutInfo]\nid=1\ndx=whole\n\
             [a]\ntype=UIButton\nid=5\nstate=2048\nstate=8\nx=centre\ny=-248\ndx=fitfont\ndy=350\n\
             altx=Right\nalty=bottom\nanchor=7\nanchor1=8\nborder=2\nyborder=5\nnormal=ui/pic.tga\n\
             forecolor=1\nforecolor=2\nforecolor=300\nbackcolor=1\ntarget=-1\nalias=-1\n\
             helpid=7\nhelpid=8\nrepeating=0\ntimed=1\n\
             [b]\ntype=UIText\nid=6\nscrollbarPos=BOTTOM\n\
             [c]\ntype=UIEditableText\nid=7\n\
             [d]\ntype=UIListBox\nid=9\ncolumn=5\ncolumn=70\nhighlightcolor=1\nhighlightcolor=2\nhighlightcolor=3\n\
             [e]\ntype=UIImage\nid=10\nx=+5\ndx=0x\n",
        )],
        0,
    );
    assert_eq!(tree.failure, None);
    assert_eq!(tree.elements[0].dx, Dim::Whole);
    let a = &tree.elements[tree.find(5).unwrap()];
    assert_eq!(a.state, 1 | 2048 | 8 | 1024);
    assert_eq!((a.x, a.y), (Pos::Center, Pos::Num(-248)));
    assert_eq!((a.dx, a.dy), (Dim::FitFont, Dim::Num(350)));
    assert_eq!(
        (a.alt_x, a.alt_y),
        (Pos::Num(0), Pos::Bottom),
        "capital R is ignored"
    );
    assert_eq!(a.anchor, 8, "anchor1 wins");
    assert_eq!((a.x_border, a.y_border), (2, 5));
    assert_eq!(a.look.fore_color, [1, 2, 255]);
    assert_eq!(
        a.look.back_color, [0; 3],
        "fewer than three values: the default of a button"
    );
    assert_eq!((a.target, a.alias), (1, 1), "-1 is the layout");
    assert_eq!(a.help_id, [7, 8]);
    assert!(a.transparent && a.look.shadow_text && a.look.normal.is_some());
    assert_eq!(a.timer_duration, 3000);
    assert!(matches!(&a.kind, Kind::Button(b) if b.repeating && b.grayed == a.look.normal));
    let b = &tree.elements[tree.find(6).unwrap()];
    assert!(!b.transparent && b.look.shadow_text && b.look.back_color == [255; 3]);
    assert!(matches!(&b.kind, Kind::Text(t) if t.wrappable && t.scroll_bar_pos == Side::Bottom));
    let c = &tree.elements[tree.find(7).unwrap()];
    assert!(c.transparent && !c.look.shadow_text);
    let d = &tree.elements[tree.find(9).unwrap()];
    assert!(matches!(&d.kind, Kind::ListBox(l)
        if l.columns == [5, 70] && l.select_color == [1, 2, 3] && l.highlight_selection));
    let e = &tree.elements[tree.find(10).unwrap()];
    assert_eq!(
        (e.x, e.dx),
        (Pos::Num(0), Dim::Num(0)),
        "ignored values leave the defaults"
    );
}

#[test]
fn expansion_masks_skip_sections_and_sublayouts() {
    let files = [
        (
            "ui/root.lyt",
            "[LayoutInfo]\nid=1\n[a]\ntype=UIImage\nid=2\nexpansion=1\n\
             [b]\ntype=UIImage\nid=3\nexpansion=2\n[c]\ntype=UILayout\nlayout=ui/sub.lyt\nexpansion=3\n\
             [d]\ntype=UIImage\nid=4\n[e]\ntype=UIImage\nid=5\nexpansion=0\n",
        ),
        ("ui/sub.lyt", "[LayoutInfo]\nid=9\n"),
    ];
    for (mask, expected) in [
        (0, vec![4]),
        (1, vec![2, 9, 4]),
        (2, vec![3, 9, 4]),
        (3, vec![2, 3, 9, 4]),
    ] {
        let tree = load(&format!("mask{mask}"), &files, mask);
        assert_eq!(ids(&tree, 0), expected, "mask {mask}");
    }
}

#[test]
fn a_section_without_a_known_type_stops_the_load_and_drops_the_interface() {
    let files = [
        (
            "ui/root.lyt",
            "[LayoutInfo]\nid=1\n[a]\ntype=UIImage\nid=2\n[b]\ntype=UILayout\nlayout=ui/sub.lyt\n\
             [c]\ntype=UIImage\nid=3\n",
        ),
        (
            "ui/sub.lyt",
            "[LayoutInfo]\nid=5\n[a]\ntype=UIImage\nid=6\n[b]\ntype=Nonsense\n[c]\ntype=UIImage\nid=7\n",
        ),
    ];
    let tree = load("stop", &files, 0);
    let failure = tree.failure.clone().unwrap();
    assert_eq!(
        (failure.file.as_str(), failure.section.as_str()),
        ("ui/sub.lyt", "b")
    );
    assert_eq!(failure.to_string(), "ui/sub.lyt [b]: unknown type Nonsense");
    assert!(tree.elements.is_empty() && tree.find(2).is_none());

    let tree = load("notype", &[("ui/root.lyt", "[LayoutInfo]\n[a]\nid=2\n")], 0);
    assert_eq!(tree.failure.unwrap().why, "no type");
    // A layout that names itself ends at the depth limit.
    let tree = load(
        "cycle",
        &[("ui/root.lyt", "[a]\ntype=UILayout\nlayout=ui/root.lyt\n")],
        0,
    );
    assert_eq!(tree.failure.unwrap().why, "layouts nested too deeply");
    // A UILayout without `layout` fails the load.
    let tree = load("nolayout", &[("ui/root.lyt", "[a]\ntype=UILayout\n")], 0);
    assert_eq!(
        tree.failure.unwrap().to_string(),
        "ui/root.lyt [a]: no layout"
    );
}

#[test]
fn a_file_that_cannot_be_read_is_an_empty_layout() {
    let tree = load(
        "nofile",
        &[(
            "ui/root.lyt",
            "[LayoutInfo]\nid=1\n[a]\ntype=UILayout\nlayout=ui/gone.lyt\n[b]\ntype=UIImage\nid=2\n",
        )],
        0,
    );
    assert_eq!(tree.failure, None);
    assert_eq!(tree.elements.len(), 3);
    assert_eq!(tree.elements[1].kind, Kind::Layout);
    assert_eq!(tree.warnings.len(), 1, "{:?}", tree.warnings);
}

#[test]
fn the_root_and_the_layout_info_section() {
    // The root starts shown, and `state=1` keeps it hidden; the name of [LayoutInfo] ignores case.
    let tree = load(
        "root",
        &[(
            "ui/root.lyt",
            "[layoutinfo]\nid=1\nstate=1\n[a]\ntype=UIMessageQueue\nid=5\n",
        )],
        0,
    );
    assert_eq!(tree.failure, None);
    assert_eq!((tree.elements[0].id, tree.elements[0].state), (1, 1));
    assert_eq!(tree.elements[1].kind.name(), "UIMessageQueue");
    let tree = load("root2", &[("ui/root.lyt", "[LayoutInfo]\nid=1\n")], 0);
    assert_eq!(tree.elements[0].state, 0);
}

#[test]
fn the_first_element_keeps_an_id() {
    let tree = load(
        "ids",
        &[(
            "ui/root.lyt",
            "[LayoutInfo]\nid=1\n[a]\ntype=UIImage\nid=2\n[b]\ntype=UIText\nid=2\n[c]\ntype=UIText\nid=1\n",
        )],
        0,
    );
    assert_eq!(tree.find(2), Some(1));
    assert_eq!(tree.find(1), Some(0));
    assert_eq!(tree.duplicates, [2, 3]);
}

#[test]
fn pictures_load_once_and_a_missing_one_is_a_warning() {
    let tree = load(
        "pictures",
        &[(
            "ui/root.lyt",
            "[LayoutInfo]\n[a]\ntype=UIImage\nanimation=ui/PIC.TGA\n[b]\ntype=UIImage\nanimation=ui/pic.tga\n\
             animation=ui/gone.tga\nnormal=ui/pic.tga\n[c]\ntype=UIImage\nanimation=ui/gone\n[d]\ntype=UIImage\nnormal=ui/pic.tga\n",
        )],
        0,
    );
    assert_eq!(tree.failure, None);
    assert_eq!(tree.pictures.images.len(), 1);
    let looks: Vec<_> = tree
        .elements
        .iter()
        .map(|e| (e.look.animation.clone(), e.look.normal))
        .collect();
    let image = Some(Picture::Image(0));
    assert_eq!(looks[1], (vec![image], None));
    assert_eq!(
        looks[2],
        (vec![image, None], None),
        "normal is read only without animation"
    );
    assert_eq!(looks[3], (vec![None], None));
    assert_eq!(looks[4], (vec![], image));
    assert_eq!(tree.warnings.len(), 2, "{:?}", tree.warnings);
}

#[test]
fn position_and_size_values() {
    assert_eq!(pos(b"-5"), Some(Pos::Num(-5)));
    assert_eq!(pos(b"center"), Some(Pos::Center));
    assert_eq!(pos(b"cool"), Some(Pos::Center));
    assert_eq!(
        (pos(b"Center"), pos(b"+5"), pos(b"x"), pos(b"")),
        (None, None, None, None)
    );
    assert_eq!(dim(b"whole"), Some(Dim::Whole));
    assert_eq!(dim(b"quarter"), Some(Dim::Quarter));
    assert_eq!(dim(b"third"), Some(Dim::Third));
    assert_eq!(dim(b"left"), None);
}

/// The numbers of the specification for the full game with both expansions.
#[test]
#[ignore = "needs the game: set ZT_DATA_DIR to the install folder"]
fn full_game_tree() {
    let dir = std::env::var_os("ZT_DATA_DIR").expect("ZT_DATA_DIR is not set");
    let vfs = Vfs::open(std::path::Path::new(&dir)).unwrap();
    assert_eq!(installed_expansions(&vfs), 3);
    let tree = Tree::load(&vfs, "ui/gamescrn.lyt", 3);
    assert_eq!(tree.failure, None);
    assert_eq!(tree.elements.len(), 1138);
    assert_eq!(tree.counts()["UILayout"], 102);
    assert_eq!(tree.duplicates, [] as [usize; 0]);
}
