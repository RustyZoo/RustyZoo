use super::*;
use crate::config::Config;
use crate::entity::Catalogue;
use crate::sprite::tests::Install;
use crate::ui::draw::Strings;
use crate::ui::layout::Tree;
use crate::unit::Kind;

pub(crate) struct Texts;

impl Strings for Texts {
    fn string(&self, _: i32) -> String {
        String::new()
    }

    fn text_entry(&self, _: &[u8]) -> Vec<u8> {
        vec![]
    }
}

/// The members of a synthetic animation `ui/z/z.ani`: the first of its four sprite files has two
/// frames, the others one.
pub(crate) fn members() -> Vec<(String, Vec<u8>)> {
    crate::ui::draw::tests::two_frames("z")
}

/// A catalogue with the type `class`/`kind` whose `idle` animation is the one of [`members`].
pub(crate) fn catalogue(class: &'static str, kind: &str) -> Catalogue {
    let mut cat = Catalogue::default();
    cat.push(EntityType {
        class,
        type_name: kind.as_bytes().to_vec(),
        subtype: Vec::new(),
        file: None,
        config: Some(std::sync::Arc::new(
            Config::parse(b"[Animations]\nidle = ui/z/z.ani\n").unwrap(),
        )),
        name_id: 0,
        footprint: [1, 1, 1],
        height: 0,
    });
    cat
}

/// What the follow button `id` shows: whether it has an animated icon, and the sprite file, the
/// frame and the reflection.
pub(crate) fn shown(ui: &Interface, id: i32) -> (bool, (usize, usize, bool)) {
    let look = &ui.tree.elements[ui.tree.find(id).unwrap()].look;
    (
        matches!(look.icon, Some(Picture::Animation(_))),
        look.icon_frame,
    )
}

fn button(name: &str) -> (Interface, Vfs) {
    let layout = "[LayoutInfo]\nid=6\nx=0\ny=0\ndx=40\ndy=40\n\
                  [u]\ntype=UIButton\nid=3114\nx=0\ny=0\ndx=4\ndy=4\n";
    let root = "[LayoutInfo]\nid=1\nx=0\ny=0\ndx=whole\ndy=whole\n\
                [b]\ntype=UILayout\nlayout=ui/p.lyt\n";
    let mut files = vec![
        ("ui/root.lyt", root.as_bytes().to_vec()),
        ("ui/p.lyt", layout.as_bytes().to_vec()),
    ];
    let members = members();
    files.extend(members.iter().map(|(n, b)| (n.as_str(), b.clone())));
    let install = Install::new(name, &files);
    let vfs = Vfs::open(&install.0).unwrap();
    let mut tree = Tree::load(&vfs, "ui/root.lyt", 0);
    tree.resolve(100, 80, |_| 12);
    (Interface::new(tree, Box::new(Texts)), vfs)
}

#[test]
fn the_button_shows_the_frame_and_the_view_of_the_unit() {
    let (mut ui, vfs) = button("follow-icon");
    let cat = catalogue("keeper", "keeper");
    let ty = cat.types.first();
    let mut unit = Unit::new(1, Kind::Keeper);
    // The animation the type does not have falls back to its idle one; the frame is the unit's.
    unit.animation.name = "walk".into();
    unit.animation.frame = 1;
    set_follow_icon(&mut ui, &vfs, 3114, ty, &unit, (0, 0));
    let (animated, (file, frame, _)) = shown(&ui, 3114);
    assert!(animated);
    assert_eq!((file < 4, frame), (true, 1));
    // Another facing and another turn of the map pick another sprite file.
    let views: Vec<_> = [(0, 0), (2, 0), (0, 1), (7, 3)]
        .map(|view| {
            set_follow_icon(&mut ui, &vfs, 3114, ty, &unit, view);
            shown(&ui, 3114).1.0
        })
        .into();
    assert!(views.iter().any(|&f| f != views[0]), "{views:?}");
    // A facing past the eighth is taken round.
    set_follow_icon(&mut ui, &vfs, 3114, ty, &unit, (8, 0));
    let wrapped = shown(&ui, 3114);
    set_follow_icon(&mut ui, &vfs, 3114, ty, &unit, (0, 0));
    assert_eq!(wrapped, shown(&ui, 3114));
}

#[test]
fn a_unit_without_a_picture_clears_the_button() {
    let (mut ui, vfs) = button("follow-icon-none");
    let unit = Unit::new(1, Kind::Keeper);
    let cat = catalogue("keeper", "keeper");
    set_follow_icon(&mut ui, &vfs, 3114, cat.types.first(), &unit, (0, 0));
    assert!(shown(&ui, 3114).0);
    // No type, a missing button and a huge frame are no harm.
    set_follow_icon(&mut ui, &vfs, 3114, None, &unit, (0, 0));
    assert_eq!(shown(&ui, 3114), (false, (0, 0, false)));
    let mut far = Unit::new(2, Kind::Keeper);
    far.animation.frame = u32::MAX;
    set_follow_icon(
        &mut ui,
        &vfs,
        3114,
        cat.types.first(),
        &far,
        (u32::MAX, u8::MAX),
    );
    set_follow_icon(&mut ui, &vfs, 1, cat.types.first(), &far, (0, 0));
}
