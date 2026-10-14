use super::*;

/// The six sizes the shipped layouts use, with the pixel size, ascent and line height, and the
/// advances of a digit and of a space.
const SIZES: [(&str, u32, i32, i32, i32, i32); 6] = [
    ("8", 11, 10, 12, 6, 3),
    ("9", 12, 11, 14, 7, 3),
    ("10", 13, 12, 15, 7, 4),
    ("11", 15, 14, 17, 8, 4),
    ("12", 16, 14, 17, 9, 4),
    ("14", 19, 17, 21, 11, 5),
];

#[test]
fn line_height_baseline_and_advances_of_the_six_sizes() {
    for face in ["Arial", "Arial Bold"] {
        for (size, ppem, ascent, line, digit, space) in SIZES {
            let font = Font::request(face, size);
            let what = format!("{face} {size}");
            assert_eq!(font.ppem(), ppem, "{what}");
            assert_eq!(font.ascent(), ascent, "{what}");
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
