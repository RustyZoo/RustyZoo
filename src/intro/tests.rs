use super::*;
use crate::movie::tests::{avi, key_frame};
use crate::sprite::tests::Install;

/// Three frames at 29.97 per second (the second and third repeat the first): 100 ms.
fn three() -> Movie {
    let key = key_frame(16, 16);
    let data = avi(
        b"IV50",
        1,
        &[(b"00dc", &key), (b"00dc", b""), (b"00dc", b"")],
    );
    Movie::parse(data).unwrap()
}

#[test]
fn a_frame_is_given_once_when_it_falls_due_and_late_ones_are_skipped() {
    let movie = three();
    let mut player = Player::new(&movie);
    let (n, picture) = player.at(0).unwrap();
    assert_eq!((n, picture.width, picture.height), (0, 16, 16));
    assert!(player.at(10).is_none() && player.at(33).is_none());
    assert_eq!(player.at(34).unwrap().0, 1);
    assert!(player.at(34).is_none(), "once");
    // 5 seconds late: the last frame, and the one before it is not shown.
    assert_eq!(player.at(5000).unwrap().0, 2);
    assert!(player.at(6000).is_none());
    assert!(!player.done(99) && player.done(100) && player.done(u64::MAX));
}

#[test]
fn the_wait_is_until_the_next_frame_or_the_end() {
    let movie = three();
    let mut player = Player::new(&movie);
    assert_eq!(player.wait(0), 0, "the first frame is due");
    player.at(0);
    assert_eq!(player.wait(0), 34);
    assert_eq!(player.wait(30), 4);
    assert_eq!(player.wait(500), 0);
    player.at(5000);
    assert_eq!(player.wait(80), 20, "to the end of the last frame");
    assert_eq!(player.wait(u64::MAX), 0);
}

#[test]
fn a_movie_that_cannot_be_decoded_ends_where_it_fails() {
    let key = key_frame(16, 16);
    let data = avi(b"IV50", 1, &[(b"00dc", &key), (b"00dc", b"garbage!")]);
    let movie = Movie::parse(data).unwrap();
    let mut player = Player::new(&movie);
    assert!(player.at(0).is_some());
    assert!(player.at(34).is_none());
    assert!(player.done(0), "over at once");
    assert!(player.at(1000).is_none());
    // No frame at all.
    let empty = Movie::parse(avi(b"IV50", 1, &[])).unwrap();
    let mut player = Player::new(&empty);
    assert!(player.at(0).is_none() && player.at(u64::MAX).is_none());
    assert!(player.done(0));
}

#[test]
fn the_picture_is_stretched_over_the_whole_window() {
    // 2 x 2 pixels, each different, over 6 x 4 (each pixel becomes 3 x 2) and over 1 x 1.
    let mut picture = Framebuffer::new(2, 2);
    for (i, px) in picture.pixels.chunks_mut(4).enumerate() {
        px.copy_from_slice(&[i as u8 + 1, 0, 0, 255]);
    }
    let mut window = Framebuffer::new(6, 4);
    window.fill([9, 9, 9, 9]);
    compose(&mut window, &picture);
    let at = |x: usize, y: usize| window.pixels[(y * 6 + x) * 4];
    let rows: Vec<Vec<u8>> = (0..4).map(|y| (0..6).map(|x| at(x, y)).collect()).collect();
    assert_eq!(
        rows,
        [
            [1, 1, 1, 2, 2, 2],
            [1, 1, 1, 2, 2, 2],
            [3, 3, 3, 4, 4, 4],
            [3, 3, 3, 4, 4, 4]
        ]
    );
    assert!(window.pixels.chunks(4).all(|p| p[3] == 255));
    let mut small = Framebuffer::new(1, 1);
    compose(&mut small, &picture);
    assert_eq!(small.pixels, [1, 0, 0, 255]);
    // A window of no size, or a picture of none, is not an error.
    compose(&mut Framebuffer::new(0, 0), &picture);
    let mut black = Framebuffer::new(2, 2);
    compose(&mut black, &Framebuffer::new(0, 0));
    assert_eq!(black.pixels, [0, 0, 0, 255].repeat(4));
}

fn vfs(name: &str, files: &[&str]) -> (Install, Vfs) {
    let key = key_frame(16, 16);
    let movie = avi(b"IV50", 1, &[(b"00dc", &key)]);
    let members: Vec<(&str, Vec<u8>)> = files
        .iter()
        .map(|f| {
            (
                *f,
                if f.starts_with("bad") {
                    b"x".to_vec()
                } else {
                    movie.clone()
                },
            )
        })
        .collect();
    let install = Install::new(&format!("intro-{name}"), &members);
    let vfs = Vfs::open(&install.0).unwrap();
    (install, vfs)
}

fn ini(text: &str) -> Config {
    Config::parse(text.as_bytes()).unwrap()
}

#[test]
fn the_settings_choose_the_movies_and_their_volumes() {
    let (_keep, vfs) = vfs("list", &["mslogo.avi", "bflogo.avi"]);
    let names = |text: &str| -> Vec<(&str, i32)> {
        list(&vfs, &ini(text))
            .0
            .iter()
            .map(|m| (m.file, m.volume))
            .collect()
    };
    assert_eq!(names(""), []);
    assert_eq!(names("[UI]\nplayMovie=0\nplaySecondMovie=1\n"), []);
    assert_eq!(names("[UI]\nplayMovie=1\n"), [("MSLOGO.AVI", 0)]);
    assert_eq!(
        names(
            "[UI]\nplayMovie=1\nplaySecondMovie=1\nmovievolume1=-1000\nmovievolume2=-2147483648\n"
        ),
        [("MSLOGO.AVI", -1000), ("BFLOGO.AVI", i32::MIN)]
    );
    // The second movie is only after the first, and any number but 0 turns one on.
    assert_eq!(
        names("[UI]\nplayMovie=-1\nplaySecondMovie=99999\n").len(),
        2
    );
    assert_eq!(names("[UI]\nplayMovie=x\nplaySecondMovie=1\n"), []);
}

#[test]
fn a_missing_or_damaged_movie_is_skipped_with_a_reason() {
    let (_keep, vfs) = vfs("gone", &["bflogo.avi"]);
    let (movies, skipped) = list(&vfs, &ini("[UI]\nplayMovie=1\nplaySecondMovie=1\n"));
    assert_eq!(movies.len(), 1);
    assert_eq!(movies[0].file, "BFLOGO.AVI");
    assert_eq!(skipped.len(), 1);
    assert!(
        skipped[0].starts_with("MSLOGO.AVI skipped: "),
        "{skipped:?}"
    );
    let damaged = Install::new("intro-damaged", &[("MSLOGO.AVI", b"not a movie".to_vec())]);
    let vfs = Vfs::open(&damaged.0).unwrap();
    let (movies, skipped) = list(&vfs, &ini("[UI]\nplayMovie=1\n"));
    assert!(movies.is_empty() && skipped.len() == 1);
}

#[test]
fn a_volume_is_the_attenuation_a_media_player_reads() {
    assert_eq!(attenuation(-1000), 1000);
    assert_eq!(attenuation(0), 0);
    assert_eq!(attenuation(-10_000), 10_000);
    // Outside -10000 to 0 the player refuses the value and the movie plays at full level.
    for refused in [1, 500, -10_001, i32::MIN, i32::MAX] {
        assert_eq!(attenuation(refused), 0, "{refused}");
    }
}
