use super::*;
use crate::sprite::tests::Install;
use crate::vfs::Vfs;

/// A true colour TGA image of the given size.
fn tga(w: u16, h: u16) -> Vec<u8> {
    let mut out = vec![0, 0, 2, 0, 0, 0, 0, 0, 0, 0, 0, 0];
    out.extend(w.to_le_bytes());
    out.extend(h.to_le_bytes());
    out.extend([24, 0x20]);
    out.extend(vec![7; usize::from(w) * usize::from(h) * 3]);
    out
}

/// The tree of `ui/root.lyt` with the given layout texts and pictures (name, width, height),
/// resolved for `sizes`; calls `check` on the tree at each size. The font height is 12, or
/// 10 + the face number for an element with font keys.
fn resolved(
    name: &str,
    layouts: &[(&str, &str)],
    pictures: &[(&str, u16, u16)],
    animations: &[(&str, i32, i32)],
    sizes: &[(i32, i32)],
    mut check: impl FnMut((i32, i32), &mut Tree),
) {
    let mut files: Vec<(&str, Vec<u8>)> = layouts
        .iter()
        .map(|(n, text)| (*n, text.as_bytes().to_vec()))
        .collect();
    let names: Vec<String> = pictures.iter().map(|p| format!("ui/{}.tga", p.0)).collect();
    files.extend(
        names
            .iter()
            .zip(pictures)
            .map(|(n, p)| (n.as_str(), tga(p.1, p.2))),
    );
    // An animation of the given box, `ui/<name>/<name>.ani`, over one tiny sprite file.
    let anis: Vec<(String, Vec<u8>)> = animations
        .iter()
        .map(|(n, w, h)| {
            let text =
                format!("[animation]\ndir0=a\ndir1=b\nanimation=N\nx0=0\ny0=0\nx1={w}\ny1={h}\n");
            (format!("ui/{n}/{n}.ani"), text.into_bytes())
        })
        .collect();
    files.extend(anis.iter().map(|(n, b)| (n.as_str(), b.clone())));
    let mut frame = Vec::new();
    for field in [1u16, 3, 0, 0, 7] {
        frame.extend(field.to_le_bytes());
    }
    frame.extend([1, 0, 3, 1, 2, 3]);
    let sprite = crate::sprite::tests::sized(&frame);
    files.push((
        "a/b/N",
        crate::sprite::tests::sprite_file(None, 1, &[sprite]),
    ));
    let install = Install::new(&format!("ui-resolve-{name}"), &files);
    let mut tree = Tree::load(&Vfs::open(&install.0).unwrap(), "ui/root.lyt", 0);
    assert_eq!(tree.failure, None);
    for &(w, h) in sizes {
        tree.resolve(w, h, |font| font.map_or(12, |(face, _)| 10 + face));
        check((w, h), &mut tree);
    }
}

fn rect(tree: &Tree, id: i32) -> (i32, i32, i32, i32) {
    tree.rect(tree.find(id).unwrap())
}

const FULL: &str = "x=0\ny=0\ndx=whole\ndy=whole\n";

#[test]
fn the_main_menu_at_two_window_sizes() {
    let root =
        format!("[LayoutInfo]\nid=1\n{FULL}[Startup]\ntype=UILayout\nlayout=ui/startup.lyt\n");
    let startup = format!(
        "[LayoutInfo]\nid=26\nlayer=2\n{FULL}\
         [a]\ntype=UIImage\nid=7100\nx=center\ny=center\ndx=800\ndy=600\n\
         [b]\ntype=UIButton\nid=7111\nanchor=7100\nx=center\ny=260\ndx=350\ndy=fitfont\nborder=2\n\
         [c]\ntype=UIText\nid=7119\nanchor=7100\nx=right\ny=bottom\ndx=320\ndy=fitfont\nborder=2\n"
    );
    resolved(
        "menu",
        &[("ui/root.lyt", &root), ("ui/startup.lyt", &startup)],
        &[],
        &[],
        &[(1024, 768), (800, 600)],
        |size, tree| {
            let (bg, button, version) = match size {
                (1024, _) => ((112, 84), (337, 344), (592, 84 + 584)),
                _ => ((0, 0), (225, 260), (480, 584)),
            };
            assert_eq!(rect(tree, 26), (0, 0, size.0, size.1));
            assert_eq!(rect(tree, 7100), (bg.0, bg.1, 800, 600));
            assert_eq!(rect(tree, 7111), (button.0, button.1, 350, 16), "{size:?}");
            assert_eq!(
                rect(tree, 7119),
                (version.0, version.1, 320, 16),
                "{size:?}"
            );
        },
    );
}

#[test]
fn a_strip_between_two_anchors_and_a_panel_beside_the_bar() {
    let root = format!(
        "[LayoutInfo]\nid=1\n{FULL}[a]\ntype=UILayout\nlayout=ui/main.lyt\n[b]\ntype=UILayout\nlayout=ui/buya.lyt\n"
    );
    let main = format!(
        "[LayoutInfo]\nid=2\nlayer=2\n{FULL}\
         [a]\ntype=UIImage\nid=1035\nx=left\ny=bottom\nanimation=ui/b3/b3\nlayer=1\n\
         [b]\ntype=UIImage\nid=1037\nx=center\ny=bottom\nanimation=ui/b4/b4\nlayer=2\n\
         [c]\ntype=UIImage\nid=1036\nanchor1=1035\nanchor2=1037\nx=left\ny=bottom\ndx=whole\n\
         dynamicwidth=1\nanimation=ui/f3/f3\n\
         [d]\ntype=UIImage\nid=1032\nx=0\ny=0\nanimation=ui/bar.tga\n\
         [e]\ntype=UIImage\nid=1033\nx=0\ny=-248\ndy=100\ndx=10\n"
    );
    let panel = "[LayoutInfo]\nid=3\nlayer=1\nanchor=1032\nx=17\ny=6\ndx=179\ndy=440\n\
                 [a]\ntype=UIImage\nid=2003\nx=0\ny=0\nanimation=ui/back.tga\n";
    resolved(
        "strip",
        &[
            ("ui/root.lyt", &root),
            ("ui/main.lyt", &main),
            ("ui/buya.lyt", panel),
        ],
        &[("bar", 33, 700), ("back", 171, 439)],
        &[("b3", 304, 114), ("b4", 330, 35), ("f3", 8, 35)],
        &[(1024, 768), (800, 600)],
        |size, tree| {
            let (b3, b4, filler, y) = match size {
                (1024, _) => ((0, 654), (347, 733), (304, 733, 43), 520),
                _ => ((0, 486), (235, 565), (235, 565, 69), 352),
            };
            assert_eq!(rect(tree, 1035), (b3.0, b3.1, 304, 114));
            assert_eq!(rect(tree, 1037), (b4.0, b4.1, 330, 35));
            assert_eq!(
                rect(tree, 1036),
                (filler.0, filler.1, filler.2, 35),
                "{size:?}"
            );
            // The panel starts beside the bar's corner and its picture follows it.
            assert_eq!(rect(tree, 3), (17, 6, 179, 440));
            assert_eq!(rect(tree, 2003), (17, 6, 171, 439));
            // A negative position counts from the bottom of the layout.
            assert_eq!(rect(tree, 1033), (0, y, 10, 100));
        },
    );
}

#[test]
fn words_percentages_alternatives_and_wrong_axes() {
    let root = format!(
        "[LayoutInfo]\nid=1\n{FULL}\
         [a]\ntype=UIImage\nid=2\ndx=third\ndy=quarter\nx=50\nxispercent=1\ny=200\nyispercent=1\n\
         [b]\ntype=UIImage\nid=3\ndx=half\ndy=10\nx=0\naltx=right\nalty=center\n\
         [c]\ntype=UIImage\nid=4\ndx=2000\ndy=10\nx=right\ny=top\n\
         [d]\ntype=UIImage\nid=5\ndx=10\ndy=10\nx=top\ny=left\n\
         [e]\ntype=UIImage\nid=6\ndx=10\ndy=10\nanchor=555\nx=3\ny=4\n\
         [f]\ntype=UIButton\nid=7\ndy=fitfont\nfont=3\nfontsize=4\nborder=1\nyborder=3\ndx=fitfont\n"
    );
    resolved(
        "words",
        &[("ui/root.lyt", &root)],
        &[],
        &[],
        &[(1024, 768)],
        |_, tree| {
            // 341 wide and 192 high; 50 percent of the free width, 200 clamped to 100 percent.
            assert_eq!(rect(tree, 2), (341, 576, 341, 192));
            assert_eq!(rect(tree, 3).0, 0);
            let b = tree.find(3).unwrap();
            let p = tree.elements[b].placed.unwrap();
            assert_eq!((p.alt_x, p.alt_y), (1024 - 512, (768 - 10) / 2));
            // `right` of a box wider than the layout is negative and keeps its place.
            assert_eq!(rect(tree, 4), (-976, 0, 2000, 10));
            // The words of the wrong axis put the element far away.
            assert_eq!(rect(tree, 5), (-30003 + 1024, -30001 + 768, 10, 10));
            // An anchor id that names nothing is no anchor.
            assert_eq!(rect(tree, 6), (3, 4, 10, 10));
            // fitfont: the font height of the keys, plus twice the vertical border; dx fitfont
            // is not handled and is far outside.
            let f = rect(tree, 7);
            assert_eq!(f.3, 13 + 2 * 3);
            assert_eq!(f.2, -31005);
        },
    );
}

#[test]
fn the_alternative_position_is_used_when_switched() {
    let root = format!(
        "[LayoutInfo]\nid=1\n{FULL}[a]\ntype=UIImage\nid=2\ndx=100\ndy=10\nx=0\ny=0\naltx=right\nalty=bottom\n"
    );
    resolved(
        "alt",
        &[("ui/root.lyt", &root)],
        &[],
        &[],
        &[(800, 600)],
        |_, tree| {
            let i = tree.find(2).unwrap();
            assert_eq!(tree.screen(i), (0, 0));
            tree.elements[i].alt = true;
            assert_eq!(tree.screen(i), (700, 590));
        },
    );
}

#[test]
fn anchors_resolve_in_later_passes_and_a_cycle_is_dropped() {
    // Section `a` is anchored to `z`, which is created later, so it waits for the next pass.
    let root = format!(
        "[LayoutInfo]\nid=1\n{FULL}\
         [a]\ntype=UIImage\nid=2\nanchor=3\nx=1\ny=1\ndx=5\ndy=5\n\
         [z]\ntype=UIImage\nid=3\nx=100\ny=50\ndx=5\ndy=5\n\
         [p]\ntype=UIImage\nid=4\nanchor=5\nx=2\ny=2\ndx=5\ndy=5\n\
         [q]\ntype=UIImage\nid=5\nanchor=4\nx=7\ny=7\ndx=5\ndy=5\n\
         [r]\ntype=UIImage\nid=6\nanchor=5\nx=1\ny=1\ndx=5\ndy=5\n"
    );
    resolved(
        "cycle",
        &[("ui/root.lyt", &root)],
        &[],
        &[],
        &[(800, 600)],
        |_, tree| {
            assert_eq!(rect(tree, 2), (101, 51, 5, 5));
            // The cycle and what waits for it are placed as if they had no anchor.
            assert_eq!(rect(tree, 4), (2, 2, 5, 5));
            assert_eq!(rect(tree, 5), (7, 7, 5, 5));
            assert_eq!(rect(tree, 6), (1, 1, 5, 5));
            assert_eq!(tree.warnings.len(), 1, "{:?}", tree.warnings);
            assert!(tree.warnings[0].starts_with("3 elements never resolve"));
        },
    );
}

#[test]
fn layers_sort_stably_and_set_the_draw_order() {
    let root = format!(
        "[LayoutInfo]\nid=1\n{FULL}\
         [a]\ntype=UIImage\nid=2\nlayer=2\n[b]\ntype=UIImage\nid=3\nlayer=1\n\
         [c]\ntype=UILayout\nlayout=ui/sub.lyt\n[d]\ntype=UIImage\nid=5\nlayer=1\n[e]\ntype=UIImage\nid=6\nlayer=-1\n"
    );
    let sub = format!(
        "[LayoutInfo]\nid=4\nlayer=1\n{FULL}[a]\ntype=UIImage\nid=7\nlayer=3\n[b]\ntype=UIImage\nid=8\n"
    );
    resolved(
        "layers",
        &[("ui/root.lyt", &root), ("ui/sub.lyt", &sub)],
        &[],
        &[],
        &[(800, 600), (640, 480)],
        |_, tree| {
            let ids: Vec<i32> = tree
                .draw_order()
                .iter()
                .map(|&i| tree.elements[i].id)
                .collect();
            assert_eq!(ids, [1, 6, 3, 4, 8, 7, 5, 2]);
        },
    );
}

#[test]
fn hostile_sizes_do_not_overflow() {
    let root = format!(
        "[LayoutInfo]\nid=1\n{FULL}[a]\ntype=UIImage\nid=2\ndx=2147483647\ndy=-2147483648\n\
         x=center\ny=right\naltx=center\nxispercent=1\n[b]\ntype=UIImage\nid=3\nx=100\nxispercent=1\ndx=-2147483648\n"
    );
    resolved(
        "big",
        &[("ui/root.lyt", &root)],
        &[],
        &[],
        &[(0, 0), (1, 1), (16384, 16384)],
        |_, tree| {
            let _ = rect(tree, 2);
            let _ = rect(tree, 3);
        },
    );
}

#[test]
fn word_codes_are_numbers_and_images_set_a_size_of_zero_after_the_positions() {
    let root = format!(
        "[LayoutInfo]\nid=1\n{FULL}\
         [a]\ntype=UIImage\nid=2\nx=-30002\ndx=10\ndy=10\n\
         [b]\ntype=UIImage\nid=3\nx=top\nxispercent=1\ndx=10\ndy=10\n\
         [c]\ntype=UIImage\nid=4\ndx=-31002\ndy=-31004\n\
         [d]\ntype=UIStatusImage\nid=5\nimage=ui/s1.tga\nimage=ui/s2.tga\nx=center\ny=bottom\n\
         [e]\ntype=UIImage\nid=6\nnormal=ui/s1.tga\nx=right\ny=-5\n\
         [f]\ntype=UILayout\nlayout=ui/sub.lyt\n"
    );
    let sub = "[LayoutInfo]\nid=7\ndy=fitfont\nborder=3\n";
    resolved(
        "codes",
        &[("ui/root.lyt", &root), ("ui/sub.lyt", sub)],
        &[("s1", 45, 10), ("s2", 30, 14)],
        &[],
        &[(1024, 768)],
        |_, tree| {
            assert_eq!(
                rect(tree, 2),
                (1014, 0, 10, 10),
                "a number equal to a code is the word"
            );
            assert_eq!(
                rect(tree, 3).0,
                0,
                "a word of the other axis is 0 as a percentage"
            );
            assert_eq!((rect(tree, 4).2, rect(tree, 4).3), (512, 192));
            // The position is made with the size 0, then the size is that of the largest image.
            assert_eq!(rect(tree, 5), (512, 768, 45, 14));
            assert_eq!(rect(tree, 6), (1024, 763, 45, 10));
            // A layout has no font height, only its borders.
            assert_eq!(rect(tree, 7).3, 6);
        },
    );
}
