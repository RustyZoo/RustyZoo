use super::*;
use crate::Framebuffer;
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
    let mut members = crate::ui::draw::tests::two_frames("z");
    // The egg and the crate animations (`y`, `x`) share the sprite files. Every view has the two
    // frames, so that the one the button picks plays.
    for name in ["y", "x"] {
        for file in crate::ui::draw::tests::two_frames(name) {
            if !members.iter().any(|m| m.0 == file.0) {
                members.push(file);
            }
        }
    }
    let two = members.iter().find(|m| m.0 == "a/b/s0").unwrap().1.clone();
    for m in &mut members {
        if m.0.starts_with("a/b/s") {
            m.1 = two.clone();
        }
    }
    members
}

/// A catalogue with the type `class`/`kind` whose `idle` animation is the one of [`members`].
pub(crate) fn catalogue(class: &'static str, kind: &str) -> Catalogue {
    animals_catalogue(class, kind, "idle = ui/z/z.ani\n")
}

/// As [`catalogue`], with these lines for the animation section.
pub(crate) fn animals_catalogue(class: &'static str, kind: &str, animations: &str) -> Catalogue {
    let mut cat = Catalogue::default();
    cat.push(EntityType {
        class,
        type_name: kind.as_bytes().to_vec(),
        subtype: Vec::new(),
        file: None,
        config: Some(std::sync::Arc::new(
            Config::parse(format!("[Animations]\n{animations}").as_bytes()).unwrap(),
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
    set_follow_icon(&mut ui, &vfs, 3114, cat.types.first(), "idle");
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
    set_follow_icon(&mut ui, &vfs, 3114, cat.types.first(), "idle");
    assert!(shown(&ui, 3114).0);
    // No type clears it; a missing button is no harm.
    set_follow_icon(&mut ui, &vfs, 3114, None, "idle");
    assert_eq!(shown(&ui, 3114), (false, None));
    set_follow_icon(&mut ui, &vfs, 1, cat.types.first(), "idle");
}

/// The picture the follow button `id` shows, by the index of its animation.
fn picture(ui: &Interface, id: i32) -> Option<Picture> {
    ui.tree.elements[ui.tree.find(id).unwrap()].look.icon
}

#[test]
fn an_egg_and_a_crated_animal_take_their_own_animation() {
    let (mut ui, vfs) = button("follow-icon-egg");
    let all = "idle = ui/z/z.ani\negg_idle = ui/y/y.ani\nbox_idle = ui/x/x.ani\n";
    let cat = animals_catalogue("animals", "beast", all);
    let ty = cat.types.first();
    let mut show = |egg, crated| {
        set_follow_icon(&mut ui, &vfs, 3114, ty, animal_animation(egg, crated));
        picture(&ui, 3114)
    };
    let (idle, egg, boxed) = (show(false, false), show(true, false), show(false, true));
    assert!(idle.is_some() && egg.is_some() && boxed.is_some());
    assert!(idle != egg && idle != boxed && egg != boxed);
    // An egg wins over the crate; a type without the animation shows its idle one.
    assert_eq!(show(true, true), egg);
    let cat = catalogue("animals", "plain");
    set_follow_icon(&mut ui, &vfs, 3114, cat.types.first(), "egg_idle");
    assert_eq!(picture(&ui, 3114), idle);
}

#[test]
fn setting_the_picture_again_restarts_its_animation() {
    let (mut ui, vfs) = button("follow-icon-clock");
    let cat = catalogue("keeper", "keeper");
    let ty = cat.types.first();
    for e in &mut ui.tree.elements {
        e.state &= !crate::ui::layout::HIDDEN;
    }
    let green = |ui: &mut Interface| {
        let mut frame = Framebuffer::new(100, 80);
        ui.paint(&mut frame, &mut |_, _| {});
        frame.pixels.chunks(4).any(|p| p[..3] == [0, 255, 0])
    };
    set_follow_icon(&mut ui, &vfs, 3114, ty, "idle");
    ui.animate(1000, false);
    assert!(!green(&mut ui));
    ui.animate(1125, false);
    assert!(green(&mut ui));
    set_follow_icon(&mut ui, &vfs, 3114, ty, "idle");
    ui.animate(1130, false);
    assert!(!green(&mut ui));
    ui.animate(1255, false);
    assert!(green(&mut ui));
}
