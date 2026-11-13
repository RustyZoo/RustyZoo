use super::*;
use crate::Framebuffer;

/// The six sizes the shipped layouts use, with the pixel size, baseline and line height (text
/// 6.6 point 2), and the advances of a digit and of a space.
const SIZES: [(&str, u32, i32, i32, i32, i32); 6] = [
    ("8", 11, 11, 14, 6, 3),
    ("9", 12, 12, 15, 7, 3),
    ("10", 13, 13, 16, 7, 4),
    ("11", 15, 14, 18, 8, 4),
    ("12", 16, 15, 19, 9, 4),
    ("14", 19, 18, 22, 11, 5),
];

#[test]
fn line_height_baseline_and_advances_of_the_six_sizes() {
    for face in ["Arial", "Arial Bold"] {
        for (size, ppem, baseline, line, digit, space) in SIZES {
            let font = Font::request(face, size);
            let what = format!("{face} {size}");
            assert_eq!(font.ppem(), ppem, "{what}");
            assert_eq!(font.baseline(), baseline, "{what}");
            assert_eq!(font.line_height(), line, "{what}");
            assert_eq!(font.is_bold(), face.contains("Bold"));
            for d in b'0'..=b'9' {
                assert_eq!(font.advance(d), digit, "{what} digit {}", d as char);
            }
            assert_eq!(font.advance(b' '), space, "{what} space");
            assert_eq!(font.advance(b','), space, "{what} comma");
            assert_eq!(font.advance(b'.'), space, "{what} full stop");
        }
    }
}

/// Arial's rows (text 6.6 point 2) for the nine shipped fonts: bold, pixel size, caps, x-height,
/// descender, ascender, baseline.
const ROWS: [(bool, u32, i32, i32, i32, i32, i32); 9] = [
    (false, 11, 8, 6, 2, 8, 11),
    (false, 12, 9, 7, 2, 9, 12),
    (false, 13, 10, 7, 3, 10, 13),
    (true, 11, 8, 6, 2, 8, 11),
    (true, 12, 9, 7, 2, 9, 12),
    (true, 13, 10, 7, 3, 10, 13),
    (true, 15, 11, 8, 3, 11, 14),
    (true, 16, 12, 9, 3, 12, 15),
    (true, 19, 14, 11, 4, 14, 18),
];

#[test]
fn caps_x_height_descender_and_ascender_rows_are_arials() {
    for (bold, ppem, caps, x, desc, asc, _) in ROWS {
        let mut font = Font::new(bold, ppem, bold);
        let mut rows = |c: u8| {
            let g = font.glyph(c);
            (g.bottom, g.bottom + g.height as i32)
        };
        let what = format!("bold {bold} ppem {ppem}");
        assert_eq!(rows(b'H'), (0, caps), "{what} caps");
        assert_eq!(rows(b'E'), (0, caps), "{what} E");
        assert_eq!(rows(b'B'), (0, caps), "{what} B");
        assert_eq!(rows(b'0'), (0, caps), "{what} 0");
        assert_eq!(rows(b'O'), (0, caps), "{what} O");
        assert_eq!(rows(b'x'), (0, x), "{what} x-height");
        assert_eq!(rows(b'o'), (0, x), "{what} o");
        assert_eq!(rows(b'e'), (0, x), "{what} e");
        assert_eq!(rows(b'b').1, asc, "{what} ascender");
        assert_eq!(rows(b'd').1, asc, "{what} d");
        assert_eq!(rows(b'p').0, -desc, "{what} descender");
        assert_eq!(rows(b'g').0, -desc, "{what} g");
    }
}

#[test]
fn baseline_and_line_height_are_windows_at_shipped_sizes_only() {
    for (bold, ppem, .., baseline) in ROWS {
        let font = Font::new(bold, ppem, bold);
        let what = format!("bold {bold} ppem {ppem}");
        assert_eq!(font.baseline(), baseline, "{what}");
        assert_eq!(font.descent(), if ppem < 15 { 3 } else { 4 }, "{what}");
    }
    // Any other size keeps the rounding rule (text 4.1): 13 + 3 at ppem 14.
    let font = Font::new(false, 14, false);
    assert_eq!((font.baseline(), font.line_height()), (13, 16));
}

#[test]
fn drawn_letters_end_on_the_row_above_the_baseline() {
    // Text 6.6 point 2: letters end on the row `top + baseline - 1`, descenders go below it.
    let style = Style {
        color: [255, 255, 255],
        justify: Justify::Left,
        shadow: false,
    };
    for (bold, ppem, caps, _, desc, _, baseline) in ROWS {
        let mut font = Font::new(bold, ppem, bold);
        for (text, top, bottom) in [
            (b"H", baseline - caps, baseline),
            (b"p", 0, baseline + desc),
        ] {
            let mut c = canvas(40, 40, [0, 0, 0]);
            draw(&mut c, &mut font, Rect::inset(40, 40, 5, 5), text, &style);
            let ys: Vec<i32> = ink(&c, [0, 0, 0, 255]).iter().map(|p| p.1 - 5).collect();
            let what = format!("bold {bold} ppem {ppem} {}", text[0] as char);
            assert_eq!(*ys.iter().max().unwrap() + 1, bottom, "{what} bottom");
            if top > 0 {
                assert_eq!(*ys.iter().min().unwrap(), top, "{what} top");
            }
        }
    }
}

#[test]
fn a_line_high_rectangle_keeps_the_whole_descender() {
    // Text 6.6 point 4: the line ends `tmDescent` rows below the baseline, so a `fitfont`
    // element one line high draws the descender's last row.
    let style = Style {
        color: [255, 255, 255],
        justify: Justify::Left,
        shadow: false,
    };
    for (bold, ppem, _, _, desc, _, baseline) in ROWS {
        let mut font = Font::new(bold, ppem, bold);
        let rect = Rect {
            left: 0,
            top: 0,
            right: 40,
            bottom: font.line_height(),
        };
        let mut c = canvas(40, 40, [0, 0, 0]);
        draw(&mut c, &mut font, rect, b"p", &style);
        let bottom = ink(&c, [0, 0, 0, 255]).iter().map(|p| p.1).max().unwrap();
        assert_eq!(bottom + 1, baseline + desc, "bold {bold} ppem {ppem}");
    }
}

#[test]
fn width_is_the_sum_of_whole_advances() {
    let font = Font::request("Arial Bold", "11");
    let sum: i32 = b"Sample text 12".iter().map(|&b| font.advance(b)).sum();
    assert_eq!(font.width(b"Sample text 12"), sum);
    assert_eq!(font.width(b"1234567"), 7 * 8);
    assert_eq!(font.width(b""), 0);
    // No kerning: "AV" is the sum of "A" and "V".
    assert_eq!(font.width(b"AV"), font.advance(b'A') + font.advance(b'V'));
}

#[test]
fn wide_characters_are_not_clipped_at_the_largest_size() {
    // The em dash is one em wide: 267 pixels at 200 points.
    let font = Font::request("Arial", "200");
    assert_eq!(font.ppem(), 267);
    assert_eq!(font.width(&to_cp1252("\u{2014}")), 267);
}

#[test]
fn bold_is_the_bold_face_not_an_emboldened_regular() {
    let regular = Font::request("Arial", "11");
    let bold = Font::request("Arial Bold", "11");
    assert!(bold.width(b"Sample text 12") > regular.width(b"Sample text 12"));
}

#[test]
fn requests_read_the_strings_like_the_game() {
    // No digits, empty, or a negative number: the default 8 points (11 pixels).
    for size in ["", "x", "-5", "0"] {
        assert_eq!(Font::request("Arial", size).ppem(), 11, "{size:?}");
    }
    assert_eq!(Font::request("Arial", "9pt").ppem(), 12);
    // A huge size is cut, whatever its length.
    assert_eq!(Font::request("Arial", "100000").ppem(), 267);
    assert_eq!(Font::request("Arial", "99999999999999").ppem(), 267);
    // Bold by the name, by the weight after a comma, or not at all.
    assert!(Font::request("Arial Bold", "9").is_bold());
    assert!(Font::request("Arial,700", "9").is_bold());
    assert!(!Font::request("Arial,400", "9").is_bold());
    assert!(!Font::request("Courier New", "9").is_bold());
    assert!(!Font::request("", "9").is_bold());
}

#[test]
fn bold_words_of_every_language_make_the_bold_face() {
    for face in [
        "Arial Gras",
        "Arial Grassetto",
        "Arial Fett",
        "Arial Negrita",
        "Arial Negrito",
        "Arial,600",
        "Arial,900",
    ] {
        assert!(Font::request(face, "9").is_bold(), "{face:?}");
    }
    // The words are compared in the letter case written, and unknown faces stay regular.
    for face in [
        "Arial gras",
        "Arial bold",
        "Arial 700",
        "Arial,599",
        "Arial Gra",
    ] {
        assert!(!Font::request(face, "9").is_bold(), "{face:?}");
    }
}

#[test]
fn fonts_are_cached_per_face_and_size() {
    let mut fonts = Fonts::default();
    fonts.get("Arial", "9");
    fonts.get("Arial", "9");
    fonts.get("Arial Bold", "9");
    fonts.get("Arial Bold", "09 points");
    fonts.get("Arial", "10");
    assert_eq!(fonts.fonts.len(), 3);
}

#[test]
fn glyphs_are_cached_and_sit_on_the_baseline() {
    let mut font = Font::request("Arial", "9");
    let (w, h, bottom) = {
        let g = font.glyph(b'H');
        (g.width, g.height, g.bottom)
    };
    assert!(w > 0 && h > 0);
    assert_eq!(bottom, 0, "a capital stands on the baseline");
    assert_eq!(font.glyphs.len(), 1);
    font.glyph(b'H');
    assert_eq!(font.glyphs.len(), 1);
    assert_eq!(font.glyph(b' ').coverage.len(), 0, "a space has no ink");
    assert!(font.glyph(b'H').coverage.iter().any(|&c| c > 0));
}

#[test]
fn windows_1252_round_trips() {
    assert_eq!(to_cp1252("Zoo"), b"Zoo");
    assert_eq!(to_cp1252("€’é"), [0x80, 0x92, 0xe9]);
    assert_eq!(to_cp1252("日"), b"?");
    for b in 0..=255u8 {
        assert_eq!(to_cp1252(&from_cp1252(b).to_string()), [b]);
    }
}

fn lines(font: &Font, text: &str, width: i32) -> Vec<String> {
    layout(font, &to_cp1252(text), width)
        .iter()
        .map(|l| String::from_utf8(l.text.clone()).unwrap())
        .collect()
}

#[test]
fn a_message_wraps_at_the_space_before_the_word_that_does_not_fit() {
    // A message in 286 pixels of Arial Bold 11.
    let font = Font::request("Arial Bold", "11");
    let text = "Sample message with words enough to fill one whole line and then some more";
    let wrapped = layout(&font, text.as_bytes(), 286);
    assert_eq!(
        lines(&font, text, 286),
        [
            "Sample message with words enough to",
            "fill one whole line and then some more"
        ]
    );
    // The line widths are the sums of advances; the dropped space counts on neither line.
    assert_eq!(
        wrapped[0].width,
        font.width(b"Sample message with words enough to")
    );
    assert!(wrapped[0].width + font.width(b" fill") > 286);
    assert_eq!(
        measure(&font, text.as_bytes(), 286),
        (wrapped[0].width, 2 * 18)
    );
    // A wide enough rectangle takes it on one line.
    assert_eq!(lines(&font, text, 1000), [text]);
}

#[test]
fn line_breaks_end_lines() {
    let font = Font::request("Arial", "9");
    for text in ["a\nb", "a\r\nb", "a\rb"] {
        assert_eq!(lines(&font, text, 100), ["a", "b"], "{text:?}");
    }
    // Each break ends a line: a lone CR then LF are two breaks, a last break adds no line.
    assert_eq!(lines(&font, "a\n\nb", 100), ["a", "", "b"]);
    assert_eq!(lines(&font, "a\n", 100), ["a"]);
    assert_eq!(lines(&font, "\n", 100), [""]);
    assert!(lines(&font, "", 100).is_empty());
    assert_eq!(measure(&font, b"", 100), (0, 0));
}

#[test]
fn spaces_stay_inside_a_line_and_vanish_at_a_break() {
    let font = Font::request("Arial", "9");
    let ab = font.width(b"aa bb");
    assert_eq!(lines(&font, "aa   bb", 1000), ["aa   bb"]);
    assert_eq!(lines(&font, "  aa", 1000), ["  aa"]);
    assert_eq!(lines(&font, "aa   bb   ", 1000), ["aa   bb"]);
    assert_eq!(lines(&font, "aa   bb", ab), ["aa", "bb"]);
    assert_eq!(lines(&font, "&", 100), ["&"], "an ampersand is a character");
}

#[test]
fn a_word_wider_than_the_rectangle_is_cut_with_an_ellipsis() {
    let font = Font::request("Arial", "9");
    let dots = font.width(b"...");
    let word = b"Supercalifragilistic";
    for width in [60, 41, 40, 39] {
        let cut = layout(&font, word, width);
        assert_eq!(cut.len(), 1);
        let text = &cut[0].text;
        assert!(text.ends_with(b"..."));
        let kept = &text[..text.len() - 3];
        assert_eq!(cut[0].width, font.width(kept) + dots);
        assert!(cut[0].width <= width, "never wider than the rectangle");
        // The longest beginning: one more character would not fit.
        let next = font.advance(word[kept.len()]);
        assert!(cut[0].width + next > width, "width {width}");
    }
    // Other words go on the next line; the cut word is alone on its line.
    let two = lines(&font, "ok Supercalifragilistic fine", 60);
    assert_eq!(two.len(), 3);
    assert_eq!(two[0], "ok");
    assert!(two[1].ends_with("..."));
    assert_eq!(two[2], "fine");
    // Nothing fits: the dots remain (the drawing clips them).
    assert_eq!(lines(&font, "abc", 0), ["..."]);
    assert_eq!(lines(&font, "abc", -5), ["..."]);
}

#[test]
fn alignment_places_each_line_by_its_own_width() {
    let rect = Rect {
        left: 10,
        top: 0,
        right: 110,
        bottom: 40,
    };
    assert_eq!(Justify::Left.start(&rect, 31), 10);
    assert_eq!(Justify::Center.start(&rect, 31), 10 + (100 - 31) / 2);
    assert_eq!(Justify::Right.start(&rect, 31), 79);
    // Centring a line wider than the rectangle goes negative, truncating toward zero.
    assert_eq!(Justify::Center.start(&rect, 131), 10 - 15);
    assert_eq!(Justify::from_layout("center"), Justify::Center);
    assert_eq!(Justify::from_layout("right"), Justify::Right);
    for value in ["left", "Left", "top", "bottom", "-5", "", "Center"] {
        assert_eq!(Justify::from_layout(value), Justify::Left, "{value:?}");
    }
}

/// The columns of `canvas` that hold ink, given the canvas was filled with `bg`.
fn ink_columns(canvas: &Framebuffer, bg: [u8; 4]) -> (usize, usize) {
    let w = canvas.width as usize;
    let cols: Vec<usize> = canvas
        .pixels
        .as_chunks::<4>()
        .0
        .iter()
        .enumerate()
        .filter(|(_, p)| **p != bg)
        .map(|(i, _)| i % w)
        .collect();
    (*cols.iter().min().unwrap(), *cols.iter().max().unwrap())
}

fn canvas(width: u32, height: u32, bg: [u8; 3]) -> Framebuffer {
    let mut canvas = Framebuffer::new(width, height);
    canvas.fill(fill_color(bg));
    canvas
}

#[test]
fn lines_are_drawn_left_centred_and_right() {
    let white = [255, 255, 255];
    let bg = fill_color(white);
    let mut font = Font::request("Arial Bold", "11");
    let rect = Rect::inset(100, 40, 0, 0);
    let ends: Vec<(usize, usize)> = [Justify::Left, Justify::Center, Justify::Right]
        .into_iter()
        .map(|justify| {
            let mut c = canvas(100, 40, white);
            let style = Style {
                color: [0, 0, 0],
                justify,
                shadow: false,
            };
            draw(&mut c, &mut font, rect, b"HIH", &style);
            ink_columns(&c, bg)
        })
        .collect();
    let line = font.width(b"HIH") as usize;
    let h = font.glyph(b'H').left as usize;
    assert_eq!(ends[0].0, h);
    assert_eq!(ends[1].0, (100 - line) / 2 + h);
    assert_eq!(ends[2].0, 100 - line + h);
    // The text is clipped at the sides of the rectangle, not wrapped differently.
    assert!(ends[2].1 < 100);
}

fn ink(canvas: &Framebuffer, bg: [u8; 4]) -> Vec<(i32, i32)> {
    let w = canvas.width as usize;
    canvas
        .pixels
        .as_chunks::<4>()
        .0
        .iter()
        .enumerate()
        .filter(|(_, p)| **p != bg)
        .map(|(i, _)| ((i % w) as i32, (i / w) as i32))
        .collect()
}

#[test]
fn the_shadow_is_a_black_copy_left_and_down() {
    for (size, d) in [("11", 1), ("12", 1), ("14", 2)] {
        let mut font = Font::request("Arial", size);
        let white = [255, 255, 255];
        let bg = fill_color(white);
        let rect = Rect {
            left: 6,
            top: 2,
            right: 60,
            bottom: 40,
        };
        let red = [255, 0, 0];
        let style = |shadow| Style {
            color: red,
            justify: Justify::Left,
            shadow,
        };
        let mut plain = canvas(64, 44, white);
        draw(&mut plain, &mut font, rect, b"HE", &style(false));
        let mut shadowed = canvas(64, 44, white);
        draw(&mut shadowed, &mut font, rect, b"HE", &style(true));
        let text = ink(&plain, bg);
        let both = ink(&shadowed, bg);
        let at = |c: &Framebuffer, (x, y): (i32, i32)| {
            let i = (y as usize * 64 + x as usize) * 4;
            c.pixels[i..i + 4].to_vec()
        };
        // The shadow extends the ink by `d` to the left and below, and nothing else.
        let bounds = |marks: &[(i32, i32)]| {
            let xs = marks.iter().map(|p| p.0);
            let ys = marks.iter().map(|p| p.1);
            (
                xs.clone().min().unwrap(),
                xs.max().unwrap(),
                ys.clone().min().unwrap(),
                ys.max().unwrap(),
            )
        };
        let (x0, x1, y0, y1) = bounds(&text);
        assert_eq!(bounds(&both), (x0 - d, x1, y0, y1 + d), "{size}");
        if font.ppem() > 16 {
            continue; // smoothed edges mix with the shadow underneath
        }
        // Bilevel: the text is drawn over its shadow, unchanged; every other inked pixel is
        // black and is the text moved by (-d, +d).
        for &p in &text {
            assert_eq!(at(&shadowed, p), at(&plain, p), "{size} {p:?}");
        }
        let black = text_color([0, 0, 0]);
        let mut shadow_only = 0;
        for &p in both.iter().filter(|p| !text.contains(p)) {
            shadow_only += 1;
            assert_eq!(at(&shadowed, p), black, "{size} {p:?}");
            assert!(text.contains(&(p.0 + d, p.1 - d)), "{size} {p:?}");
        }
        assert!(shadow_only > 0);
    }
}

#[test]
fn drawing_is_clipped_to_the_rectangle() {
    let mut font = Font::request("Arial", "9");
    let white = [255, 255, 255];
    let bg = fill_color(white);
    let rect = Rect {
        left: 5,
        top: 5,
        right: 25,
        bottom: 5 + 14 + 6,
    };
    let style = Style {
        color: [0, 0, 0],
        justify: Justify::Left,
        shadow: true,
    };
    let mut c = canvas(40, 60, white);
    // Lines of 14 pixels; the second is cut at the bottom of the rectangle. The shadow is
    // clipped by its own rectangle, moved one pixel left and down.
    for shadow in [false, true] {
        let style = Style { shadow, ..style };
        c.fill(bg);
        draw(&mut c, &mut font, rect, b"HHH\nHHH\nHHH", &style);
        let marks = ink(&c, bg);
        assert!(!marks.is_empty());
        let (dx, dy) = (i32::from(shadow), i32::from(shadow));
        for (x, y) in marks {
            assert!(
                (5 - dx..25).contains(&x) && (5..25 + dy).contains(&y),
                "({x}, {y})"
            );
        }
    }
    // A rectangle outside the framebuffer, and an empty one, draw nothing and do not panic.
    let mut c = canvas(10, 10, white);
    for r in [
        (-50, -50, -5, -5),
        (20, 20, 90, 90),
        (5, 5, 5, 5),
        (9, 3, 2, 8),
    ] {
        let rect = Rect {
            left: r.0,
            top: r.1,
            right: r.2,
            bottom: r.3,
        };
        draw(&mut c, &mut font, rect, b"Hello\nworld", &style);
    }
    assert!(ink(&c, bg).is_empty());
}

#[test]
fn a_button_centres_one_line() {
    let font = Font::request("Arial Bold", "10"); // line height 16
    let r = |top, bottom| Rect {
        left: 3,
        top,
        right: 50,
        bottom,
    };
    assert_eq!(r(100, 131).centred_line(&font), r(107, 124));
    assert_eq!(r(100, 132).centred_line(&font), r(108, 124));
    // The rectangle is shorter than the line: it grows by the same on both sides.
    assert_eq!(r(100, 110).centred_line(&font), r(97, 113));
    assert_eq!(r(100, 109).centred_line(&font), r(97, 112));
    assert_eq!(r(100, 116).centred_line(&font), r(100, 116));
    assert_eq!(
        Rect::inset(108, 21, 2, 2),
        Rect {
            left: 2,
            top: 2,
            right: 106,
            bottom: 19
        }
    );
}

#[test]
fn a_nine_point_tooltip_is_small_only_with_the_bold_word() {
    // Only " Bold" makes the 9 point font small (text 5.5), whatever the language's bold word.
    let text = b"Sample tooltip text";
    let small = |face: &str| {
        let font = Font::request(face, "9");
        tooltip_size(&font, text, true, 200, 0, 0).0 - font.width(text)
    };
    assert_eq!(small("Arial Bold"), 2);
    assert_eq!(small("Arial Gras"), 0, "bold by the word, not small");
    assert_eq!(small("Arial,700"), 0, "bold by the weight, not small");
    assert_eq!(small("Arial"), 0);
}

#[test]
fn a_tooltip_is_sized_to_its_text() {
    let regular = Font::request("Arial", "8"); // small
    let big = Font::request("Arial", "10");
    let text = b"Sample tooltip text";
    let w = regular.width(text);
    assert_eq!(
        tooltip_size(&regular, text, false, 200, 2, 3),
        (w + 4, 14 + 6)
    );
    // Small font with an alignment other than left: 2 more pixels.
    assert_eq!(
        tooltip_size(&regular, text, true, 200, 2, 3),
        (w + 2 + 4, 14 + 6)
    );
    assert_eq!(
        tooltip_size(&big, text, true, 200, 0, 0),
        (big.width(text), 16)
    );
    // Too wide: three fifths of the width (at most the maximum), wrapped again.
    let long = b"Sample tooltip text that is long enough to be wrapped into several lines of text";
    let w = big.width(long);
    let (tw, th) = tooltip_size(&big, long, false, 200, 1, 1);
    let wrapped = measure(&big, long, (w * 3 / 5).min(200));
    assert!(w > 200);
    assert_eq!((tw, th), (wrapped.0 + 2, wrapped.1 + 2));
    assert!(wrapped.1 > 16 && wrapped.0 <= 200);
    // The whole text is one line, one line high, even with a line break in it.
    let two = b"one\ntwo";
    assert_eq!(
        tooltip_size(&big, two, false, 400, 0, 0),
        (big.width(two), 16)
    );
    // No maximum at all: every word is cut; nothing panics.
    for max in [0, -1] {
        tooltip_size(&big, long, true, max, 0, 0);
    }
}

#[test]
fn colours_reduce_by_the_shift_rule_for_text_and_by_the_game_rule_for_fills() {
    // (255, 186, 16): 5-6-5 codes (31, 46, 2) for text, (31, 45, 1) for a fill.
    assert_eq!(text_color([255, 186, 16]), [255, 186, 16, 255]);
    assert_eq!(fill_color([255, 186, 16]), [255, 182, 8, 255]);
    assert_eq!(text_color([0, 0, 0]), [0, 0, 0, 255]);
    assert_eq!(fill_color([255, 255, 255]), [255, 255, 255, 255]);
    assert_eq!(text_color([7, 3, 7]), [0, 0, 0, 255]);
}

/// The distinct colours of the pixels of `canvas` that differ from `bg`.
fn colours(canvas: &Framebuffer, bg: [u8; 4]) -> Vec<[u8; 4]> {
    let mut found: Vec<[u8; 4]> = canvas
        .pixels
        .as_chunks::<4>()
        .0
        .iter()
        .copied()
        .filter(|p| *p != bg)
        .collect();
    found.sort();
    found.dedup();
    found
}

#[test]
fn smoothed_pixels_fade_in_between_a_quarter_and_63_percent() {
    use super::draw::contrast;
    assert_eq!(
        (contrast(0), contrast(64), contrast(160), contrast(255)),
        (0, 0, 255, 255)
    );
    assert_eq!(contrast(112), 127);
    assert!((0..255).all(|c| contrast(c) <= contrast(c + 1)));
}

/// White `text` in Arial Bold 9 (12 pixels per em) on black; the rows of the canvas as
/// coverage levels: 2 for pure white, 1 for a grey, 0 for black.
fn message_rows(text: &[u8]) -> Vec<Vec<u8>> {
    let style = Style {
        color: [255, 255, 255],
        justify: Justify::Left,
        shadow: false,
    };
    let mut c = canvas(160, 20, [0, 0, 0]);
    let mut font = Font::request("Arial Bold", "9");
    draw(&mut c, &mut font, Rect::inset(160, 20, 2, 2), text, &style);
    let level = |p: &[u8]| match p[0] {
        255 => 2,
        0 => 0,
        _ => 1,
    };
    c.pixels
        .chunks(160 * 4)
        .map(|row| row.chunks(4).map(level).collect())
        .collect()
}

#[test]
fn a_message_is_grid_fitted_like_windows_draws_it() {
    // Text 6.5: the capture has 428 pure white pixels and about 90 grey ones, only on curves and
    // diagonals; stems are 2 whole pixels, caps 9 rows, x-height 7 (the descender of g has 3 rows here, 2 there).
    // The capture: 428 white and 90 grey of 518 inked pixels; 95% of the white ones is 78.5% of
    // the ink. The sample is another sentence of the same length.
    let rows = message_rows(b"The bag is packed up.");
    let count = |level| rows.iter().flatten().filter(|&&l| l == level).count();
    let ink = count(1) + count(2);
    assert!(count(2) * 1000 >= ink * 785, "{} white of {ink}", count(2));
    assert!(count(1) * 100 <= ink * 20, "{} grey of {ink}", count(1));
    // Rows with ink of one letter, and the widths of its whole-colour runs.
    let ink = |text: &[u8]| -> Vec<Vec<u8>> {
        message_rows(text)
            .into_iter()
            .filter(|r| r.iter().any(|&l| l > 0))
            .collect()
    };
    assert_eq!(ink(b"H").len(), 9, "caps");
    assert_eq!(ink(b"x").len(), 7, "x-height");
    let stem = ink(b"l");
    assert!(
        stem.iter()
            .all(|r| r.iter().filter(|&&l| l > 0).count() == 2)
    );
    assert!(stem.iter().flatten().all(|&l| l != 1), "no grey on a stem");
    // The T of the paused message: a 6 pixel bar over a 2 pixel stem in its middle, and a black
    // column between the bar and the h (the bar used to be 7 wide and touch it).
    let th = ink(b"Th");
    let at = th[0].iter().position(|&l| l == 2).unwrap();
    assert_eq!(&th[0][at..at + 8], [2, 2, 2, 2, 2, 2, 0, 2], "bar, gap, h");
    for row in &th[1..] {
        assert_eq!(&row[at..at + 7], [0, 0, 2, 2, 0, 0, 0], "stem");
    }
}

#[test]
fn glyphs_are_bilevel_at_the_middle_sizes_and_smoothed_elsewhere() {
    let style = Style {
        color: [255, 186, 16],
        justify: Justify::Left,
        shadow: false,
    };
    let bg = fill_color([68, 36, 20]);
    for (face, size, ppem, smooth) in [
        ("Arial", "6", 8, true),
        ("Arial", "7", 9, false),
        ("Arial", "12", 16, false),
        ("Arial", "13", 17, false),
        ("Arial", "14", 19, true),
        ("Arial", "8", 11, false),
        // Bold is bilevel at 9 and 10 pixels per em only.
        ("Arial Bold", "7", 9, false),
        ("Arial Bold", "8", 11, true),
        ("Arial Bold", "9", 12, true),
        ("Arial Bold", "11", 15, true),
    ] {
        let mut font = Font::request(face, size);
        assert_eq!(font.ppem(), ppem);
        let mut c = canvas(60, 30, [68, 36, 20]);
        draw(
            &mut c,
            &mut font,
            Rect::inset(60, 30, 0, 0),
            b"Sample 123",
            &style,
        );
        let found = colours(&c, bg);
        if smooth {
            assert!(found.len() > 3, "ppem {ppem}: {found:?}");
            // Fringes are mixtures of the text colour and the background.
            assert!(found.iter().any(|p| p[0] > 68 && p[0] < 255), "ppem {ppem}");
        } else {
            assert_eq!(found, [text_color(style.color)], "ppem {ppem}");
        }
    }
}

/// A picture of two shadowed lines for the hash pins below.
fn sample(face: &str, size: &str, justify: Justify) -> u64 {
    let mut font = Font::request(face, size);
    let style = Style {
        color: [255, 212, 60],
        justify,
        shadow: true,
    };
    let mut c = canvas(120, 70, [68, 36, 20]);
    let rect = Rect::inset(120, 70, 2, 2);
    draw(
        &mut c,
        &mut font,
        rect,
        b"Sample 12 - $74,878\nOther & Words",
        &style,
    );
    crate::app::frame_hash(&c)
}

#[test]
fn drawn_pictures_have_the_same_hash_on_every_system() {
    // Bold smoothed and heavier (ppem 15), regular smoothed (ppem 19). The values are pinned by CI on three systems.
    assert_eq!(
        format!("{:016x}", sample("Arial Bold", "11", Justify::Center)),
        "83f335b13cbfd6a1"
    );
    assert_eq!(
        format!("{:016x}", sample("Arial", "14", Justify::Right)),
        "5914885438a865dd"
    );
}

#[test]
fn a_visible_character_is_ascii_graphic_or_double_byte_by_code_page() {
    use super::visible;
    // Single-byte installs, and any language number that is none: ASCII 0x21 to 0x7E only.
    for language in [0, 1033, 1036, u32::MAX] {
        assert!(visible(language, b"Lion"), "{language}");
        assert!(visible(language, b"  7 "));
        assert!(visible(language, b"\x21") && visible(language, b"\x7e"));
        assert!(!visible(language, b""), "empty");
        assert!(
            !visible(language, b" \t\x7f\x20"),
            "space, controls, delete"
        );
        assert!(
            !visible(language, b"\xa1\xdf\x81\xe9\xff"),
            "bytes above 127"
        );
    }
    // Code page 932: lead bytes and the half-width range count.
    for byte in [0x81, 0x9f, 0xe0, 0xfc, 0xa1, 0xdf] {
        assert!(visible(1041, &[byte]), "{byte:#x}");
    }
    for byte in [0x80, 0xa0, 0xfd, 0xff] {
        assert!(!visible(1041, &[byte]), "{byte:#x}");
    }
    // The other double-byte code pages: lead bytes only.
    for language in [2052, 4100, 1042, 1028, 3076, 5124] {
        assert!(
            visible(language, &[0x81]) && visible(language, &[0xfe]),
            "{language}"
        );
        assert!(!visible(language, &[0x80]) && !visible(language, &[0xff]));
    }
}
