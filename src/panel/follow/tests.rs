use super::*;
use crate::config::Config;
use crate::entity::Catalogue;
use crate::sprite::tests::Install;
use crate::ui::draw::Strings;
use crate::ui::layout::Tree;

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

/// What the follow button `id` shows: whether it has an animated icon, and its sprite file.
pub(crate) fn shown(ui: &Interface, id: i32) -> (bool, Option<usize>) {
    let look = &ui.tree.elements[ui.tree.find(id).unwrap()].look;
    (
        matches!(look.icon, Some(Picture::Animation(_))),
        look.icon_file,
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
fn the_button_shows_the_idle_animation_seen_from_the_south_east() {
    let (mut ui, vfs) = button("follow-icon");
    let cat = catalogue("keeper", "keeper");
    set_follow_icon(&mut ui, &vfs, 3114, cat.types.first());
    let (animated, file) = shown(&ui, 3114);
    assert!(animated);
    // The view of facing 4 at rotation 0, as the set maps it.
    let Some(Picture::Animation(a)) = ui.tree.elements[ui.tree.find(3114).unwrap()].look.icon
    else {
        panic!("no animation");
    };
    assert_eq!(
        file,
        Some(ui.tree.pictures.animations[a].view_of(4, 0).file)
    );
}

#[test]
fn a_type_without_a_picture_clears_the_button() {
    let (mut ui, vfs) = button("follow-icon-none");
    let cat = catalogue("keeper", "keeper");
    set_follow_icon(&mut ui, &vfs, 3114, cat.types.first());
    assert!(shown(&ui, 3114).0);
    // No type clears it; a missing button is no harm.
    set_follow_icon(&mut ui, &vfs, 3114, None);
    assert_eq!(shown(&ui, 3114), (false, None));
    set_follow_icon(&mut ui, &vfs, 1, cat.types.first());
}
