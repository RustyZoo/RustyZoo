use std::cell::RefCell;
use std::collections::HashMap;

use super::*;
use crate::sprite::tests::{Install, sized, sprite_file};
use crate::text::{fill_color, text_color};
use crate::ui::widgets::{Caret, Row};
use crate::vfs::Vfs;

/// A TGA image from rows of RGB pixels, top row first.
fn tga(rows: &[&[[u8; 3]]]) -> Vec<u8> {
    let (w, h) = (rows[0].len() as u16, rows.len() as u16);
    let mut out = vec![0, 0, 2, 0, 0, 0, 0, 0, 0, 0, 0, 0];
    out.extend(w.to_le_bytes());
    out.extend(h.to_le_bytes());
    out.extend([24, 0x20]);
    for pixel in rows.iter().flat_map(|r| r.iter()) {
        out.extend([pixel[2], pixel[1], pixel[0]]);
    }
    out
}

/// Members of an animation `ui/<name>/<name>.ani` with four sprite files, a 3 x 1 box around the
/// anchor, and each file one row of three pixels of palette colour 1 to 4.
fn animation(name: &str) -> Vec<(String, Vec<u8>)> {
    let ani = "[animation]\ndir0=a\ndir1=b\nanimation=s0\nanimation=s1\nanimation=s2\nanimation=s3\n\
               x0=-1\ny0=0\nx1=2\ny1=1\n";
    let mut files = vec![(format!("ui/{name}/{name}.ani"), ani.as_bytes().to_vec())];
    for v in 0..4u8 {
        let mut frame = Vec::new();
        for field in [1u16, 3, 0, 1, 7] {
            frame.extend(field.to_le_bytes());
        }
        frame.extend([1, 0, 3, v + 1, v + 1, v + 1]);
        files.push((format!("a/b/s{v}"), sprite_file(None, 1, &[sized(&frame)])));
    }
    let mut palette = 5u32.to_le_bytes().to_vec();
    for c in [
        [0, 0, 0],
        [255, 0, 0],
        [0, 255, 0],
        [0, 0, 255],
        [255, 255, 255],
    ] {
        palette.extend([c[0], c[1], c[2], 255]);
    }
    files.push(("animals/Lion/lion.pal".to_string(), palette));
    files
}

#[derive(Default)]
struct Texts(RefCell<HashMap<i32, String>>);

impl Strings for Texts {
    fn string(&self, id: i32) -> String {
        self.0.borrow().get(&id).cloned().unwrap_or_default()
    }

    fn text_entry(&self, _: &[u8]) -> Vec<u8> {
        vec![]
    }
}

const FULL: &str = "x=0\ny=0\ndx=whole\ndy=whole\n";

/// Loads a tree of `ui/root.lyt` and resolves it for 64 x 48.
fn tree(name: &str, root: &str, members: &[(String, Vec<u8>)]) -> Tree {
    let mut files: Vec<(&str, Vec<u8>)> = vec![("ui/root.lyt", root.as_bytes().to_vec())];
    files.extend(members.iter().map(|(n, b)| (n.as_str(), b.clone())));
    let install = Install::new(&format!("ui-draw-{name}"), &files);
    let mut tree = Tree::load(&Vfs::open(&install.0).unwrap(), "ui/root.lyt", 0);
    assert_eq!(tree.failure, None);
    assert_eq!(tree.warnings, Vec::<String>::new());
    tree.resolve(64, 48, |_| 12);
    tree
}

fn paint(tree: &mut Tree, painter: &mut Painter, texts: &Texts) -> Framebuffer {
    let mut frame = Framebuffer::new(64, 48);
    painter.draw(tree, &mut frame, texts, &mut |_, _| {});
    frame
}

fn at(frame: &Framebuffer, x: u32, y: u32) -> [u8; 3] {
    let i = ((y * frame.width + x) * 4) as usize;
    [frame.pixels[i], frame.pixels[i + 1], frame.pixels[i + 2]]
}

fn count(frame: &Framebuffer, color: [u8; 3]) -> usize {
    (0..frame.height)
        .flat_map(|y| (0..frame.width).map(move |x| (x, y)))
        .filter(|&(x, y)| at(frame, x, y) == color)
        .count()
}

fn rgb(c: [u8; 4]) -> [u8; 3] {
    [c[0], c[1], c[2]]
}

#[test]
fn a_text_widget_fills_its_canvas_and_a_transparent_one_keeps_only_the_glyphs() {
    let root = format!(
        "[LayoutInfo]\nid=1\n{FULL}\
         [a]\ntype=UIText\nid=2\nx=10\ny=5\ndx=40\ndy=20\nbackcolor=255\nbackcolor=0\nbackcolor=0\n\
         forecolor=0\nforecolor=255\nforecolor=0\ntransparent=0\ntextid=1\n\
         [b]\ntype=UIText\nid=3\nx=10\ny=30\ndx=40\ndy=16\nbackcolor=255\nbackcolor=0\nbackcolor=0\n\
         forecolor=0\nforecolor=255\nforecolor=0\ntransparent=1\ntextid=1\nshadowtext=0\n"
    );
    let mut tree = tree("text", &root, &[]);
    tree.show(1);
    tree.elements[1].state &= !HIDDEN;
    tree.elements[2].state &= !HIDDEN;
    let texts = Texts::default();
    texts.0.borrow_mut().insert(1, "HIH".into());
    let frame = paint(&mut tree, &mut Painter::default(), &texts);
    // The opaque canvas covers its rectangle and nothing else.
    assert_eq!(at(&frame, 10, 5), [255, 0, 0]);
    assert_eq!(at(&frame, 49, 24), [255, 0, 0]);
    assert_eq!(at(&frame, 9, 5), [0, 0, 0]);
    assert_eq!(at(&frame, 50, 24), [0, 0, 0]);
    let green = rgb(text_color([0, 255, 0]));
    assert!(count(&frame, green) > 10, "glyph pixels");
    // The transparent one shows no background, only the glyph pixels.
    assert_eq!(at(&frame, 10, 30), [0, 0, 0]);
    assert_eq!(at(&frame, 49, 45), [0, 0, 0]);
    let in_second: usize = (30..46)
        .flat_map(|y| (10..50).map(move |x| (x, y)))
        .filter(|&(x, y)| at(&frame, x, y) == green)
        .count();
    assert!(in_second > 10);
}

#[test]
fn a_tiled_strip_repeats_its_picture_and_cuts_the_last_copy() {
    let (r, g, b, y) = ([255, 0, 0], [0, 255, 0], [0, 0, 255], [255, 255, 0]);
    let image = tga(&[&[r, g, b, y], &[r, g, b, y]]);
    let root = format!(
        "[LayoutInfo]\nid=1\n{FULL}\
         [a]\ntype=UIImage\nid=2\nx=2\ny=3\ndx=10\ndy=2\nanimation=ui/t.tga\ndynamicwidth=1\ntransparent=0\n\
         [b]\ntype=UIImage\nid=3\nx=2\ny=10\ndx=10\ndy=2\nanimation=ui/t.tga\ndynamicwidth=1\n\
         [c]\ntype=UIImage\nid=4\nx=20\ny=3\ndx=4\ndy=6\nanimation=ui/t.tga\ndynamicheight=1\ntransparent=0\n"
    );
    let mut tree = tree("tile", &root, &[("ui/t.tga".to_string(), image)]);
    for e in &mut tree.elements {
        e.state &= !HIDDEN;
    }
    let frame = paint(&mut tree, &mut Painter::default(), &Texts::default());
    let row: Vec<[u8; 3]> = (0..13).map(|x| at(&frame, x, 3)).collect();
    let pattern = [r, g, b, y, r, g, b, y, r, g];
    assert_eq!(&row[2..12], &pattern);
    assert_eq!(row[12], [0, 0, 0], "cut at the width of the element");
    assert_eq!(at(&frame, 3, 4), g, "second row");
    // Transparent: the pixels equal to the top left one (red) are not copied.
    assert_eq!(at(&frame, 2, 10), [0, 0, 0]);
    assert_eq!(at(&frame, 3, 10), g);
    assert_eq!(at(&frame, 6, 10), [0, 0, 0]);
    // Tiled down: the picture is 2 high, the element 6, the picture 4 wide cut to 4.
    assert_eq!(at(&frame, 20, 3), r);
    assert_eq!(at(&frame, 23, 8), y);
}

#[test]
fn an_animation_is_centred_and_a_button_picks_the_set_of_its_state() {
    let root = format!(
        "[LayoutInfo]\nid=1\n{FULL}\
         [a]\ntype=UIButton\nid=2\nx=10\ny=5\nanimation=ui/b/b\n\
         [b]\ntype=UIImage\nid=3\nx=20\ny=5\nanimation=ui/b/b\n"
    );
    let mut tree = tree("buttons", &root, &animation("b"));
    for e in &mut tree.elements {
        e.state &= !HIDDEN;
    }
    // The box is 3 x 1: the element takes it, and the picture fills it.
    assert_eq!(tree.rect(1), (10, 5, 3, 1));
    let mut painter = Painter::default();
    let texts = Texts::default();
    let states = [
        (0, [255, 0, 0]),
        (HIGHLIGHTED, [0, 255, 0]),
        (SELECTED, [0, 0, 255]),
        (DISABLED, [255, 255, 255]),
        (SELECTED | HIGHLIGHTED, [0, 0, 255]),
        (TOGGLE | PRESSING, [0, 0, 255]),
        (TOGGLE | SELECTED | RELEASING | HIGHLIGHTED, [0, 255, 0]),
        (TOGGLE | SELECTED | RELEASING, [255, 0, 0]),
    ];
    for (state, color) in states {
        tree.elements[1].state = state;
        let frame = paint(&mut tree, &mut painter, &texts);
        assert_eq!(
            (0..3).map(|x| at(&frame, 10 + x, 5)).collect::<Vec<_>>(),
            [color; 3],
            "state {state}"
        );
        assert_eq!(at(&frame, 9, 5), [0, 0, 0]);
        assert_eq!(at(&frame, 13, 5), [0, 0, 0]);
        // An image-like element always shows set 0.
        assert_eq!(at(&frame, 21, 5), [255, 0, 0]);
    }
}

#[test]
fn a_buttons_icon_is_drawn_over_its_picture_or_under_it_with_iconfirst() {
    let button = |extra: &str| {
        format!(
            "[LayoutInfo]\nid=1\n{FULL}\
             [a]\ntype=UIButton\nid=2\nx=10\ny=5\nanimation=ui/b/b\n{extra}\n"
        )
    };
    for (extra, name, shows) in [
        ("", "over", [0, 0, 255]),
        ("iconfirst=1", "under", [255, 0, 0]),
    ] {
        let mut tree = tree(&format!("icon-{name}"), &button(extra), &animation("b"));
        for e in &mut tree.elements {
            e.state &= !HIDDEN;
        }
        // A 3 x 2 icon of blue, drawn at the button's corner, a row taller than the picture.
        let blue = tga(&[&[[0, 0, 255]; 3], &[[0, 0, 255]; 3]]);
        tree.pictures
            .images
            .push(crate::tga::Texture::parse(&blue).unwrap());
        let mut painter = Painter::default();
        let texts = Texts::default();
        let frame = paint(&mut tree, &mut painter, &texts);
        assert_eq!(at(&frame, 10, 5), [255, 0, 0], "no icon yet");
        assert_eq!(at(&frame, 10, 6), [0, 0, 0]);
        tree.elements[1].look.icon = Some(Picture::Image(tree.pictures.images.len() - 1));
        let frame = paint(&mut tree, &mut painter, &texts);
        assert_eq!(
            (0..3).map(|x| at(&frame, 10 + x, 5)).collect::<Vec<_>>(),
            [shows; 3],
            "{name}"
        );
        assert_eq!(
            at(&frame, 11, 6),
            [0, 0, 255],
            "the icon is taller than the picture"
        );
        assert_eq!(at(&frame, 13, 5), [0, 0, 0]);
    }
}

#[test]
fn a_button_text_takes_the_colour_of_its_state() {
    let root = format!(
        "[LayoutInfo]\nid=1\n{FULL}\
         [a]\ntype=UIButton\nid=2\nx=2\ny=2\ndx=50\ndy=fitfont\nforecolor=255\nforecolor=0\nforecolor=0\n\
         highlightcolor=0\nhighlightcolor=255\nhighlightcolor=0\nselectcolor=0\nselectcolor=0\nselectcolor=255\n\
         graycolor=255\ngraycolor=255\ngraycolor=255\ntextid=1\njustify=center\nshadowtext=0\n"
    );
    let mut tree = tree("button-text", &root, &[]);
    tree.elements[1].state &= !HIDDEN;
    tree.elements[0].state = 0;
    let mut painter = Painter::default();
    let texts = Texts::default();
    texts.0.borrow_mut().insert(1, "HIH".into());
    for (state, color) in [
        (0, [255, 0, 0]),
        (HIGHLIGHTED, [0, 255, 0]),
        (SELECTED, [0, 0, 255]),
        (DISABLED, [255, 255, 255]),
    ] {
        // The canvas is built again when the state changes, without the dirty flag.
        tree.elements[1].state = state;
        let frame = paint(&mut tree, &mut painter, &texts);
        assert!(count(&frame, rgb(text_color(color))) > 5, "state {state}");
        let others = [[255, 0, 0], [0, 255, 0], [0, 0, 255], [255, 255, 255]];
        for other in others.into_iter().filter(|c| *c != color) {
            assert_eq!(count(&frame, rgb(text_color(other))), 0, "state {state}");
        }
    }
}

#[test]
fn hidden_layouts_templates_and_layers_and_the_dirty_flag() {
    let (r, g) = ([255, 0, 0], [0, 255, 0]);
    let root = format!(
        "[LayoutInfo]\nid=1\n{FULL}\
         [a]\ntype=UIImage\nid=2\nx=0\ny=0\nanimation=ui/r.tga\ntransparent=0\nlayer=2\n\
         [b]\ntype=UIImage\nid=3\nx=0\ny=0\nanimation=ui/g.tga\ntransparent=0\nlayer=1\n\
         [c]\ntype=UIImage\nid=4\nx=4\ny=0\nanimation=ui/r.tga\ntransparent=0\nstate=256\n\
         [d]\ntype=UIImage\nid=5\nx=6\ny=0\nanimation=ui/r.tga\ntransparent=0\nstate=16\n\
         [e]\ntype=UILayout\nlayout=ui/sub.lyt\n\
         [f]\ntype=UIText\nid=7\nx=10\ny=10\ndx=20\ndy=14\nbackcolor=255\nbackcolor=255\nbackcolor=255\ntransparent=0\ntextid=1\n"
    );
    let sub = format!(
        "[LayoutInfo]\nid=6\n{FULL}[a]\ntype=UIImage\nid=8\nx=8\ny=0\nanimation=ui/r.tga\ntransparent=0\n"
    );
    let members = [
        ("ui/r.tga".to_string(), tga(&[&[r, r]])),
        ("ui/g.tga".to_string(), tga(&[&[g, g]])),
        ("ui/sub.lyt".to_string(), sub.into_bytes()),
    ];
    let mut tree = tree("order", &root, &members);
    for i in [1, 2, 3, 4, 7] {
        tree.elements[i].state &= !HIDDEN;
    }
    let mut painter = Painter::default();
    let texts = Texts::default();
    let frame = paint(&mut tree, &mut painter, &texts);
    // Layer 2 over layer 1.
    assert_eq!(at(&frame, 0, 0), r);
    // A template and a never-shown element are not drawn; a hidden layout hides its children.
    assert_eq!(at(&frame, 4, 0), [0, 0, 0]);
    assert_eq!(at(&frame, 6, 0), [0, 0, 0]);
    assert_eq!(at(&frame, 8, 0), [0, 0, 0]);
    // Showing the layout shows its children.
    assert!(tree.show(6));
    assert!(!tree.show(999));
    let frame = paint(&mut tree, &mut painter, &texts);
    assert_eq!(at(&frame, 8, 0), r);
    // The text canvas is kept until the element is dirty.
    texts.0.borrow_mut().insert(1, "W".into());
    let before = paint(&mut tree, &mut painter, &texts);
    assert_eq!(count(&before, [255, 255, 255]), 280, "no text yet");
    tree.elements[7].state |= DIRTY;
    let after = paint(&mut tree, &mut painter, &texts);
    assert!(count(&after, [255, 255, 255]) < 280);
    assert_eq!(tree.elements[7].state & DIRTY, 0, "drawing clears the flag");
}

#[test]
fn a_button_text_is_drawn_over_its_picture_without_a_background() {
    let blue = [0, 0, 255];
    let row = [blue; 60];
    let image = tga(&vec![&row[..]; 20]);
    let root = format!(
        "[LayoutInfo]\nid=1\n{FULL}\
         [a]\ntype=UIButton\nid=2\nx=2\ny=2\nnormal=ui/b.tga\ntransparent=0\ntextid=1\n\
         backcolor=255\nbackcolor=0\nbackcolor=0\nforecolor=0\nforecolor=255\nforecolor=0\njustify=center\n"
    );
    let mut tree = tree("over", &root, &[("ui/b.tga".to_string(), image)]);
    tree.elements[1].state &= !HIDDEN;
    let texts = Texts::default();
    texts.0.borrow_mut().insert(1, "HIH".into());
    let frame = paint(&mut tree, &mut Painter::default(), &texts);
    assert_eq!(count(&frame, [255, 0, 0]), 0, "no backcolor");
    assert_eq!(at(&frame, 2, 2), blue);
    assert!(count(&frame, rgb(text_color([0, 255, 0]))) > 5);
}

#[test]
fn the_map_view_calls_the_hook_with_its_rectangle_below_what_is_over_it() {
    let r = [255, 0, 0];
    let root = format!(
        "[LayoutInfo]\nid=1\n{FULL}[a]\ntype=ZTMapView\nid=7000\nx=4\ny=6\ndx=20\ndy=10\ntransparent=0\n\
         [b]\ntype=UIImage\nid=2\nx=10\ny=6\nanimation=ui/r.tga\ntransparent=0\nlayer=1\n"
    );
    let mut tree = tree("map", &root, &[("ui/r.tga".to_string(), tga(&[&[r, r]]))]);
    for e in &mut tree.elements {
        e.state &= !HIDDEN;
    }
    let mut frame = Framebuffer::new(64, 48);
    let mut seen = vec![];
    Painter::default().draw(&mut tree, &mut frame, &Texts::default(), &mut |f, rect| {
        seen.push(rect);
        f.pixels[..4].copy_from_slice(&[0, 9, 0, 255]);
        let at = (6 * 64 + 10) * 4;
        f.pixels[at..at + 4].copy_from_slice(&[0, 9, 0, 255]);
    });
    assert_eq!(seen, [(4, 6, 20, 10)]);
    assert_eq!(at(&frame, 0, 0), [0, 9, 0]);
    assert_eq!(at(&frame, 10, 6), r, "drawn over the map");
}

/// An animation `ui/<name>/<name>.ani` of one sprite file with two frames of palette colour 1 and
/// 2 (red, green), 125 ms each, over the box and palette of [`animation`].
pub(crate) fn two_frames(name: &str) -> Vec<(String, Vec<u8>)> {
    let mut files = animation(name);
    let frames: Vec<Vec<u8>> = [1u8, 2]
        .iter()
        .map(|&c| {
            let mut frame = Vec::new();
            for field in [1u16, 3, 0, 1, 7] {
                frame.extend(field.to_le_bytes());
            }
            frame.extend([1, 0, 3, c, c, c]);
            sized(&frame)
        })
        .collect();
    files[1].1 = sprite_file(None, 2, &frames);
    files
}

#[test]
fn an_animation_advances_with_real_time_and_the_sequence_starts_again() {
    let root = format!(
        "[LayoutInfo]\nid=1\n{FULL}\
         [a]\ntype=UIImage\nid=2\nx=10\ny=5\nanimation=ui/b/b\n\
         [b]\ntype=UIImage\nid=3\nx=20\ny=5\nanimation=ui/b/b\nupdateWhilePaused=0\n\
         [c]\ntype=UIImage\nid=4\nx=30\ny=5\nanimation=ui/b/b\nstate=1\n"
    );
    let mut tree = tree("anim", &root, &two_frames("b"));
    for e in &mut tree.elements {
        e.state &= !HIDDEN;
    }
    tree.elements[3].state |= HIDDEN;
    let (mut painter, texts) = (Painter::default(), Texts::default());
    let colors = |painter: &mut Painter, tree: &mut Tree| {
        let frame = paint(tree, painter, &texts);
        [at(&frame, 10, 5), at(&frame, 20, 5), at(&frame, 30, 5)]
    };
    let (red, green, black) = ([255, 0, 0], [0, 255, 0], [0, 0, 0]);
    assert_eq!(colors(&mut painter, &mut tree), [red, red, black]);
    // The first draw only notes the clock; 124 ms later is not a frame yet, 1 ms more is.
    assert!(!painter.animate(&tree, 1000, false));
    assert!(!painter.animate(&tree, 1124, false));
    assert_eq!(colors(&mut painter, &mut tree), [red, red, black]);
    assert!(painter.animate(&tree, 1125, false));
    assert_eq!(colors(&mut painter, &mut tree), [green, green, black]);
    // Paused: only the element that updates while paused moves, but both keep the time.
    assert!(painter.animate(&tree, 1250, true));
    assert_eq!(colors(&mut painter, &mut tree), [red, green, black]);
    // 249 ms: the first element is on its second frame with 124 ms to go, the second one reached
    // the end of the sequence, which drops the time left over, and starts again.
    assert!(painter.animate(&tree, 1499, false));
    assert_eq!(colors(&mut painter, &mut tree), [green, red, black]);
    // A stall, and no time at all.
    assert!(painter.animate(&tree, 101_499, false));
    assert!(!painter.animate(&tree, 101_499, false));
    // An element that is shown the first time starts on its first frame; hidden, its time goes on
    // and is credited whole when it is shown again.
    tree.elements[3].state &= !HIDDEN;
    painter.animate(&tree, 101_500, false);
    assert_eq!(colors(&mut painter, &mut tree)[2], red);
    tree.elements[3].state |= HIDDEN;
    painter.animate(&tree, 101_510, false);
    tree.elements[3].state &= !HIDDEN;
    assert!(painter.animate(&tree, 101_630, false));
    assert_eq!(colors(&mut painter, &mut tree)[2], green);
}

#[test]
fn a_still_picture_never_advances() {
    let root = format!(
        "[LayoutInfo]\nid=1\n{FULL}\
         [a]\ntype=UIButton\nid=2\nx=10\ny=5\nanimation=ui/b/b\n\
         [b]\ntype=UIImage\nid=3\nx=20\ny=5\nanimation=ui/t.tga\n"
    );
    let mut files = animation("b");
    files.push(("ui/t.tga".to_string(), tga(&[&[[9, 9, 9]]])));
    let mut tree = tree("still", &root, &files);
    for e in &mut tree.elements {
        e.state &= !HIDDEN;
    }
    let mut painter = Painter::default();
    // One frame per set, and an image: no frame to change to, whatever the time.
    assert!(!painter.animate(&tree, 0, false));
    assert!(!painter.animate(&tree, 10_000, false));
    assert!(!painter.animate(&Tree::default(), 20_000, false));
}

/// Shows every element and redraws the canvas of the given one.
fn show_all(tree: &mut Tree) {
    for e in &mut tree.elements {
        e.state &= !HIDDEN;
    }
}

#[test]
fn a_list_draws_its_rows_from_the_first_shown_with_the_selection_and_the_hover() {
    let root = format!(
        "[LayoutInfo]\nid=1\n{FULL}\
         [a]\ntype=UIListBox\nid=2\nx=4\ny=4\ndx=40\ndy=36\nbackcolor=0\nbackcolor=0\nbackcolor=255\n\
         selectbackcolor=255\nselectbackcolor=0\nselectbackcolor=0\n\
         highlightbordercolor=0\nhighlightbordercolor=255\nhighlightbordercolor=0\n"
    );
    let mut tree = tree("list", &root, &[]);
    show_all(&mut tree);
    let row = |text: &str| Row {
        text: text.as_bytes().to_vec(),
        ..Row::default()
    };
    let Kind::ListBox(list) = &mut tree.elements[1].kind else {
        panic!("not a list");
    };
    list.rows.rows = vec![row("a"), row("b"), row("c"), row("d")];
    list.rows.selected = Some(1);
    list.rows.hover = Some(2);
    let (mut painter, texts) = (Painter::default(), Texts::default());
    let frame = paint(&mut tree, &mut painter, &texts);
    // Rows of 12 pixels (the font's line): the selected one is filled, the hovered one framed.
    assert_eq!(at(&frame, 4, 4), [0, 0, 255], "the background");
    assert_eq!(
        at(&frame, 43, 16),
        [255, 0, 0],
        "the selected row's fill, its right end"
    );
    assert_eq!(at(&frame, 43, 27), [255, 0, 0], "to its last pixel row");
    assert_eq!(
        at(&frame, 4, 28),
        [0, 255, 0],
        "the hovered row's frame, its top left"
    );
    assert_eq!(at(&frame, 43, 39), [0, 255, 0], "its bottom right");
    assert_eq!(at(&frame, 43, 33), [0, 255, 0]);
    assert_eq!(at(&frame, 20, 34), [0, 0, 255], "inside the frame");
    // Scrolled by one row: the selected row is first.
    let Kind::ListBox(list) = &mut tree.elements[1].kind else {
        unreachable!();
    };
    list.rows.top = 1;
    tree.elements[1].state |= DIRTY;
    let frame = paint(&mut tree, &mut painter, &texts);
    assert_eq!(at(&frame, 43, 4), [255, 0, 0]);
    assert_eq!(
        at(&frame, 4, 16),
        [0, 255, 0],
        "the hovered row moved up too"
    );
}

#[test]
fn a_scroll_bar_draws_its_arrows_and_a_thumb_that_slides_with_the_position() {
    let (r, g, b) = ([255, 0, 0], [0, 255, 0], [0, 0, 255]);
    let root = format!(
        "[LayoutInfo]\nid=1\n{FULL}\
         [a]\ntype=UIScrollBar\nid=2\nx=4\ny=2\ndx=6\ndy=36\nupArrow=ui/u.tga\ndownArrow=ui/d.tga\n\
         thumb=ui/t.tga\nbackcolor=10\nbackcolor=10\nbackcolor=10\n"
    );
    let pictures = [
        ("ui/u.tga".to_string(), tga(&[&[r; 6][..]; 4])),
        ("ui/d.tga".to_string(), tga(&[&[g; 6][..]; 4])),
        ("ui/t.tga".to_string(), tga(&[&[b; 6][..]; 4])),
    ];
    let mut tree = tree("bar", &root, &pictures);
    show_all(&mut tree);
    let (mut painter, texts) = (Painter::default(), Texts::default());
    // The thumb at the top of its travel (36 less three pieces of 4: 24 pixels): just under the
    // up arrow.
    let frame = paint(&mut tree, &mut painter, &texts);
    assert_eq!((at(&frame, 4, 2), at(&frame, 9, 5)), (r, r));
    assert_eq!((at(&frame, 4, 6), at(&frame, 9, 9)), (b, b));
    assert_eq!(
        at(&frame, 4, 10),
        rgb(fill_color([10, 10, 10])),
        "the track"
    );
    assert_eq!((at(&frame, 4, 34), at(&frame, 9, 37)), (g, g));
    // At the end of the range the thumb is just over the down arrow.
    let Kind::ScrollBar(s) = &mut tree.elements[1].kind else {
        panic!("not a bar");
    };
    s.bar.set_range(0, 10, 3);
    s.bar.position = 10;
    tree.elements[1].state |= DIRTY;
    let frame = paint(&mut tree, &mut painter, &texts);
    assert_eq!(at(&frame, 4, 6), rgb(fill_color([10, 10, 10])));
    assert_eq!((at(&frame, 4, 30), at(&frame, 9, 33)), (b, b));
    assert_eq!(at(&frame, 4, 34), g);
}

#[test]
fn a_focused_field_shows_a_caret_or_its_selection_in_inverse_colours() {
    let root = format!(
        "[LayoutInfo]\nid=1\n{FULL}\
         [a]\ntype=UIEditableText\nid=2\nx=4\ny=4\ndx=40\ndy=14\nbackcolor=0\nbackcolor=0\nbackcolor=0\n\
         forecolor=200\nforecolor=200\nforecolor=200\ntransparent=0\n"
    );
    let mut tree = tree("field", &root, &[]);
    show_all(&mut tree);
    tree.elements[1].text = Some(b"HIH".to_vec());
    let (mut painter, texts) = (Painter::default(), Texts::default());
    let width = |painter: &mut Painter, n: usize| {
        let font = crate::text::Font::request("Arial", "8");
        let _ = painter;
        font.width(&b"HIH"[..n])
    };
    let white = rgb(text_color([200, 200, 200]));
    let fill = rgb(fill_color([200, 200, 200]));
    // Not focused: the text and no caret.
    let frame = paint(&mut tree, &mut painter, &texts);
    assert_eq!(at(&frame, 4, 4), [0, 0, 0]);
    assert!(count(&frame, white) > 10);
    assert_eq!(count(&frame, fill), 0);
    // Focused, nothing selected: a caret one pixel wide after the second letter, the height of
    // a line.
    tree.elements[1].state |= FOCUSED | DIRTY;
    let Kind::EditableText(f) = &mut tree.elements[1].kind else {
        panic!("not a field");
    };
    f.caret = Caret { at: 2, anchor: 2 };
    let frame = paint(&mut tree, &mut painter, &texts);
    let x = 4 + width(&mut painter, 2) as u32;
    assert_eq!((at(&frame, x, 4), at(&frame, x, 15)), (fill, fill));
    assert_eq!(at(&frame, x, 16), [0, 0, 0]);
    assert_eq!(count(&frame, fill), 12);
    // The middle letter selected: its rectangle in the text colour, the letter in the background.
    let Kind::EditableText(f) = &mut tree.elements[1].kind else {
        unreachable!();
    };
    f.caret = Caret { at: 2, anchor: 1 };
    tree.elements[1].state |= DIRTY;
    let frame = paint(&mut tree, &mut painter, &texts);
    let (x0, x1) = (width(&mut painter, 1) as u32, width(&mut painter, 2) as u32);
    assert_eq!(at(&frame, 4 + x0, 4), fill);
    assert_eq!(at(&frame, 4 + x1 - 1, 15), fill);
    assert_eq!(at(&frame, 4 + x1, 4), [0, 0, 0]);
}

#[test]
fn a_list_draws_whole_rows_only_inside_its_borders() {
    let root = format!(
        "[LayoutInfo]\nid=1\n{FULL}\
         [a]\ntype=UIListBox\nid=2\nx=4\ny=4\ndx=40\ndy=33\nborder=3\nbackcolor=0\nbackcolor=0\nbackcolor=255\n\
         selectbackcolor=255\nselectbackcolor=0\nselectbackcolor=0\n"
    );
    let mut tree = tree("listfit", &root, &[]);
    show_all(&mut tree);
    let Kind::ListBox(list) = &mut tree.elements[1].kind else {
        panic!("not a list");
    };
    list.rows.rows = (0..4)
        .map(|_| Row {
            text: b"a".to_vec(),
            ..Row::default()
        })
        .collect();
    // The third row would reach the bottom border (33 - 3 = 30 pixels hold two rows of 12).
    list.rows.selected = Some(2);
    let frame = paint(&mut tree, &mut Painter::default(), &Texts::default());
    assert_eq!(
        count(&frame, [255, 0, 0]),
        0,
        "the selected row is not drawn"
    );
    let Kind::ListBox(list) = &mut tree.elements[1].kind else {
        unreachable!();
    };
    list.rows.selected = Some(1);
    tree.elements[1].state |= DIRTY;
    let frame = paint(&mut tree, &mut Painter::default(), &Texts::default());
    // The second row: 3 + 12 to 3 + 24 pixels down, from the left of the list.
    assert_eq!(at(&frame, 43, 4 + 15), [255, 0, 0]);
    assert_eq!(at(&frame, 43, 4 + 27), [0, 0, 255], "under it, the border");
}

#[test]
fn a_list_with_a_small_icon_has_rows_as_tall_as_its_slot_and_draws_the_icon_in_it() {
    let root = format!(
        "[LayoutInfo]\nid=1\n{FULL}\
         [a]\ntype=UIListBox\nid=2\nx=4\ny=4\ndx=40\ndy=48\nbackcolor=0\nbackcolor=0\nbackcolor=255\n\
         selectbackcolor=255\nselectbackcolor=0\nselectbackcolor=0\nminiconwidth=6\nminiconheight=16\n\
         [b]\ntype=UIImage\nid=3\nx=60\ny=40\nanimation=ui/b/b\n"
    );
    let mut tree = tree("minicon", &root, &animation("b"));
    show_all(&mut tree);
    let icon = tree.elements[2].look.animation[0];
    assert!(matches!(icon, Some(Picture::Animation(_))));
    tree.elements[2].state |= HIDDEN;
    let row = |icon| Row {
        text: b"H".to_vec(),
        icon,
        ..Row::default()
    };
    let Kind::ListBox(list) = &mut tree.elements[1].kind else {
        panic!("not a list");
    };
    list.rows.rows = vec![row(icon), row(None), row(icon)];
    list.rows.selected = Some(1);
    let (mut painter, texts) = (Painter::default(), Texts::default());
    let frame = paint(&mut tree, &mut painter, &texts);
    let Kind::ListBox(list) = &tree.elements[1].kind else {
        unreachable!();
    };
    let heights = painter.row_heights(&tree.pictures, &tree.elements[1], list, &texts);
    assert_eq!((heights.rows.clone(), heights.inner), (vec![16; 3], 48));
    // Rows of 16 pixels, the second one selected: its fill is from y 20 to 35.
    assert_eq!(at(&frame, 43, 19), [0, 0, 255]);
    assert_eq!(at(&frame, 43, 20), [255, 0, 0]);
    assert_eq!(at(&frame, 43, 35), [255, 0, 0]);
    assert_eq!(at(&frame, 43, 36), [0, 0, 255]);
    // The icon of the box 3 x 1 is half its width from the left of the list, the middle of the
    // slot down, in each row that has one.
    for y in [12, 12 + 32] {
        assert_eq!(at(&frame, 4, y), [255, 0, 0], "row at {y}");
        assert_eq!(at(&frame, 6, y), [255, 0, 0]);
        assert_eq!(at(&frame, 7, y), [0, 0, 255], "past the icon");
        assert_eq!(at(&frame, 3, y), [0, 0, 0], "outside the list");
    }
}

#[test]
fn the_selected_row_of_a_transparent_list_is_filled_even_in_the_colour_of_the_background() {
    let root = format!(
        "[LayoutInfo]\nid=1\n{FULL}\
         [a]\ntype=UIListBox\nid=2\nx=4\ny=4\ndx=40\ndy=36\nbackcolor=0\nbackcolor=0\nbackcolor=255\n\
         selectbackcolor=0\nselectbackcolor=0\nselectbackcolor=255\ntransparent=1\n"
    );
    let mut tree = tree("transparent-select", &root, &[]);
    show_all(&mut tree);
    let Kind::ListBox(list) = &mut tree.elements[1].kind else {
        panic!("not a list");
    };
    list.rows.rows = vec![Row::default(), Row::default()];
    list.rows.selected = Some(0);
    let frame = paint(&mut tree, &mut Painter::default(), &Texts::default());
    // Under the list there is the black of the frame: the background is skipped, the fill not.
    assert_eq!(at(&frame, 43, 4), [0, 0, 255], "the selected row");
    assert_eq!(
        at(&frame, 43, 30),
        [0, 0, 0],
        "the others show what is behind"
    );
}

#[test]
fn a_row_is_as_tall_as_its_wrapped_text_or_its_icon_and_spacing_goes_between_rows() {
    let root = format!(
        "[LayoutInfo]\nid=1\n{FULL}\
         [a]\ntype=UIListBox\nid=2\nx=0\ny=0\ndx=30\ndy=60\nspacing=3\nborder=0\nminiconwidth=4\n"
    );
    let mut tree = tree("heights", &root, &[]);
    show_all(&mut tree);
    let mut painter = Painter::default();
    let texts = Texts::default();
    let line = painter.line_height(&tree.elements[1], &texts);
    let row = |text: &str| Row {
        text: text.as_bytes().to_vec(),
        ..Row::default()
    };
    let Kind::ListBox(list) = &mut tree.elements[1].kind else {
        panic!("not a list");
    };
    list.rows.rows = vec![row("a"), row("H H H H H H H H"), row("")];
    let Kind::ListBox(list) = &tree.elements[1].kind else {
        unreachable!();
    };
    let h = painter.row_heights(&tree.pictures, &tree.elements[1], list, &texts);
    assert_eq!(h.rows[0], line);
    assert!(
        h.rows[1] >= 2 * line && h.rows[1] % line == 0,
        "wraps: {h:?}"
    );
    assert_eq!((h.rows[2], h.spacing, h.inner), (line, 3, 60));
}
