use super::*;

fn ini(text: &str) -> Config {
    Config::parse(text.as_bytes()).unwrap()
}

/// A screen of 40 x 20 pixels without a picture (no library in the folder), a bar from x 10 to 20
/// and y 10 to 12 with the shadow two to the left and two down.
fn screen(extra: &str, size: (u32, u32)) -> Startup {
    let bar = "[UI]\nprogressLeft=10\nprogressTop=10\nprogressRight=20\nprogressBottom=12\n";
    Startup::new(
        Path::new("/nowhere"),
        &ini(&format!("{bar}{extra}")),
        0,
        size,
    )
}

fn pixel(s: &Startup, x: usize, y: usize) -> [u8; 3] {
    let at = (y * s.frame().width as usize + x) * 4;
    s.frame().pixels[at..at + 3].try_into().unwrap()
}

#[test]
fn the_picture_is_chosen_by_the_expansion_flags() {
    let numbers: Vec<u32> = (0..5).map(picture_number).collect();
    assert_eq!(numbers, [502, 503, 504, 505, 502]);
}

#[test]
fn a_bitmap_resource_becomes_a_picture_with_5_6_5_colours() {
    // 2 x 2, 24 bits, bottom row first, rows padded to 8 bytes (blue, green, red).
    let mut dib = 40u32.to_le_bytes().to_vec();
    dib.extend(2i32.to_le_bytes());
    dib.extend(2i32.to_le_bytes());
    dib.extend([1, 0, 24, 0]);
    dib.extend([0; 24]);
    dib.extend([0, 0, 255, 0, 255, 0, 0, 0]);
    dib.extend([50, 100, 200, 255, 255, 255, 0, 0]);
    let picture = decode(&dib).unwrap();
    assert_eq!((picture.width, picture.height), (2, 2));
    assert_eq!(picture.pixels[..4], [197, 97, 49, 255], "top left");
    assert_eq!(picture.pixels[4..8], [255, 255, 255, 255]);
    assert_eq!(picture.pixels[8..12], [255, 0, 0, 255], "bottom left");
    for bad in [&dib[..3], &dib[..50], &[][..], &[0; 40][..]] {
        assert!(decode(bad).is_none(), "{} bytes", bad.len());
    }
    let mut four_bits = dib.clone();
    four_bits[14] = 4;
    assert!(decode(&four_bits).is_none());
}

#[test]
fn the_bar_is_drawn_one_step_behind_with_its_shadow_first() {
    let mut s = screen("progresscalls=10\n", (40, 20));
    let bar = fill_color([255, 184, 43]);
    let (bar, shadow) = ([bar[0], bar[1], bar[2]], {
        let c = fill_color([32, 32, 32]);
        [c[0], c[1], c[2]]
    });
    let black = [0, 0, 0];
    // One pixel per step: the first step draws a bar of the length before it, 0, the second one
    // of length 1.
    assert!(s.step());
    assert_eq!(pixel(&s, 10, 10), black);
    assert!(s.step());
    assert_eq!(pixel(&s, 10, 10), bar);
    assert_eq!(pixel(&s, 11, 10), black);
    assert_eq!(pixel(&s, 8, 12), shadow, "two to the left, two down");
    assert_eq!(pixel(&s, 9, 12), black);
    assert!(s.step() && s.step());
    assert_eq!((pixel(&s, 12, 11), pixel(&s, 13, 11)), (bar, black));
    // The shadow is under the bar where they meet.
    assert_eq!(pixel(&s, 10, 11), bar);
    assert_eq!(pixel(&s, 10, 12), shadow);
    // Every step flips once the length is not 0; the length stops at the width, which the bar
    // reaches one step after it.
    let flips = (0..20).filter(|_| s.step()).count();
    assert_eq!((flips, s.steps()), (20, 24));
    assert_eq!(pixel(&s, 19, 10), bar);
    assert_eq!(pixel(&s, 20, 10), black);
}

#[test]
fn the_length_may_pass_the_width_by_less_than_an_increment() {
    // 10 pixels in 17 steps of 0.588 in single precision end at 10.588, which rounds to 11.
    let mut s = screen("progresscalls=17\n", (40, 20));
    (0..30).for_each(|_| _ = s.step());
    assert_eq!(pixel(&s, 20, 10), [255, 182, 41], "the eleventh pixel");
    assert_eq!(pixel(&s, 21, 10), [0, 0, 0]);
}

#[test]
fn a_fractional_step_rounds_the_length() {
    // 10 pixels in 4 steps: 2.5, 5, 7.5 and 10 give 3, 5, 8 and 10, drawn at the next step.
    let mut s = screen("progresscalls=4\n", (40, 20));
    let lengths: Vec<usize> = (0..5)
        .map(|_| {
            s.step();
            (0..40).filter(|&x| pixel(&s, x, 10) != [0, 0, 0]).count()
        })
        .collect();
    assert_eq!(lengths, [0, 3, 5, 8, 10], "one step behind");
}

#[test]
fn a_bar_with_odd_settings_does_not_panic() {
    for extra in [
        "progresscalls=0\n",
        "progresscalls=-7\n",
        "progresscalls=2147483647\n",
        "progressLeft=2147483647\nprogressRight=-2147483648\n",
        "progressTop=-2147483648\nprogressBottom=2147483647\nprogressShadowXOffset=2147483647\n",
        "progressRed=999\nprogressGreen=-5\nprogressBlue=x\n",
    ] {
        let mut s = screen(extra, (40, 20));
        (0..50).for_each(|_| _ = s.step());
        assert_eq!(s.steps(), 50, "{extra}");
    }
    let mut empty = screen("", (0, 0));
    (0..2000).for_each(|_| _ = empty.step());
    // The defaults: 340 pixels in 957 steps, the rectangle 230, 480 to 570, 495.
    let mut s = Startup::new(Path::new("/nowhere"), &ini(""), 0, (800, 600));
    let flips = (0..957).filter(|_| s.step()).count();
    assert_eq!(
        flips, 956,
        "every step but the first, whose length rounds to 0"
    );
    assert_eq!(pixel(&s, 230, 480), [255, 182, 41]);
}

#[test]
fn without_a_picture_the_log_says_so() {
    let s = screen("", (40, 20));
    assert_eq!(s.log_line(), "no picture");
}

#[test]
fn the_product_version_is_read_after_its_key() {
    let mut data: Vec<u16> = "\u{1}\u{2}junk ProductVersion\0".encode_utf16().collect();
    data.extend([0, 0]);
    data.extend("12.04.00.0009\0".encode_utf16());
    data.extend("more".encode_utf16());
    let bytes: Vec<u8> = data.iter().flat_map(|u| u.to_le_bytes()).collect();
    assert_eq!(version_in(&bytes).as_deref(), Some("12.04.00.0009"));
    for bad in [&[][..], &bytes[..10], &bytes[..bytes.len() - 70]] {
        assert_eq!(version_in(bad), None);
    }
    let empty: Vec<u8> = "ProductVersion\0\0\0"
        .encode_utf16()
        .flat_map(u16::to_le_bytes)
        .collect();
    assert_eq!(version_in(&empty), None);
}
