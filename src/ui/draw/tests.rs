use std::cell::RefCell;
use std::collections::HashMap;

use super::*;
use crate::sprite::tests::{Install, sized, sprite_file};
use crate::text::text_color;
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
