use super::*;

/// A folder below the system's temporary folder, removed on drop.
struct Temp(PathBuf);

impl Temp {
    fn new(name: &str) -> Temp {
        let dir = std::env::temp_dir().join(format!("rustyzoo-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        Temp(dir)
    }
}

impl Drop for Temp {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn install(tree: &Temp) -> PathBuf {
    let dir = tree.0.join("game");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("zoo.ini"), b"[resource]\npath=.\n").unwrap();
    dir
}

#[test]
fn the_launcher_shows_without_a_folder_or_an_answer() {
    let dir = Path::new("game");
    let both = [true, true];
    assert!(!needed(Some(dir), [Some(true), Some(false)], both));
    assert!(!needed(Some(dir), [Some(false), Some(false)], both));
    assert!(needed(None, [Some(true), Some(true)], both));
    assert!(needed(Some(dir), [None, Some(true)], both));
    assert!(needed(Some(dir), [Some(true), None], both));
    assert!(needed(None, [None, None], both));
}

#[test]
fn a_question_not_asked_neither_shows_nor_waits() {
    let dir = Path::new("game");
    let none = [None, None];
    assert!(!needed(Some(dir), none, [false, false]));
    assert!(needed(None, none, [false, false]));
    assert!(!needed(Some(dir), [Some(true), None], [true, false]));
    assert!(needed(Some(dir), [None, Some(true)], [true, false]));
    assert!(needed(Some(dir), [Some(true), None], [false, true]));
    let tree = Temp::new("launcher-asks");
    let game = install(&tree);
    let mut state = State::new(Some(game), none, [false, true]);
    assert_eq!(state.asked(), [1]);
    assert!(!state.playable());
    state.answers[1] = Some(false);
    assert!(state.playable());
    let state = State::new(state.folder, none, [false, false]);
    assert!(state.asked().is_empty() && state.playable());
    // No empty gap: the rest moves up, the first slot holds the one question.
    assert_eq!(height(2), H);
    assert_eq!(height(1), H - ROW);
    assert_eq!(height(0), H - 2 * ROW);
    assert_eq!(place(Hit::Play, 1)[3], place(Hit::Play, 2)[3] - ROW);
    assert_eq!(hit(30, 188, &[1]), Some(Hit::Answer(1, true)));
    assert_eq!(hit(30, 230, &[1]), None);
    assert_eq!(hit(30, 150, &[]), None);
    assert_eq!(hit(W - 50, 276 - 2 * ROW, &[]), None);
    assert_eq!(hit(50, 310 - 2 * ROW, &[]), Some(Hit::Quit));
}

#[test]
fn play_needs_a_folder_that_opens_and_both_answers() {
    let tree = Temp::new("launcher-play");
    let game = install(&tree);
    let mut state = State::new(None, [None, None], [true, true]);
    assert!(!state.playable());
    state.answers = [Some(true), Some(false)];
    assert!(!state.playable());
    state.set_folder(Some(tree.0.join("missing")));
    assert!(!state.playable());
    assert_eq!(status(&state), "This folder holds no Zoo Tycoon install");
    state.set_folder(Some(game.clone()));
    assert!(state.playable());
    assert_eq!(status(&state), "Zoo Tycoon found (English)");
    state.answers[1] = None;
    assert!(!state.playable());
    let state = State::new(Some(game), [Some(false), Some(false)], [true, true]);
    assert!(state.playable());
}

#[test]
fn play_keeps_the_folder_and_the_answers() {
    let tree = Temp::new("launcher-save");
    let game = install(&tree);
    let user = tree.0.join("user");
    assert_eq!(answers(Some(&user)), [None, None]);
    assert_eq!(answers(None), [None, None]);
    State::new(Some(game.clone()), [Some(true), Some(false)], [true, true])
        .save(&user)
        .unwrap();
    assert_eq!(answers(Some(&user)), [Some(true), Some(false)]);
    let ini = std::fs::read_to_string(user.join("zoo.ini")).unwrap();
    assert!(
        ini.contains("startStats=1") && ini.contains("crashReports=0"),
        "{ini}"
    );
    assert!(ini.contains(&format!("data={}", game.display())), "{ini}");
    // An unanswered question keeps the answer already there.
    State::new(None, [None, Some(true)], [true, true])
        .save(&user)
        .unwrap();
    assert_eq!(answers(Some(&user)), [Some(true), Some(true)]);
    // A question not asked is never written.
    let other = tree.0.join("other");
    State::new(None, [Some(true), Some(true)], [false, true])
        .save(&other)
        .unwrap();
    assert_eq!(answers(Some(&other)), [None, Some(true)]);
}

#[test]
fn clicks_land_on_their_places() {
    assert_eq!(hit(400, 97, &[0, 1]), Some(Hit::Browse));
    assert_eq!(hit(30, 188, &[0, 1]), Some(Hit::Answer(0, true)));
    assert_eq!(hit(100, 252, &[0, 1]), Some(Hit::Answer(1, false)));
    assert_eq!(hit(50, 310, &[0, 1]), Some(Hit::Quit));
    assert_eq!(hit(W - 30, 329, &[0, 1]), Some(Hit::Play));
    assert_eq!(hit(W - 50, 276, &[0, 1]), Some(Hit::Privacy));
    assert_eq!(hit(200, 200, &[0, 1]), None);
    assert_eq!(hit(-1, -1, &[0, 1]), None);
    assert_eq!(hit(i32::MAX, i32::MIN, &[0, 1]), None);
}

#[test]
fn the_update_page_has_three_choices_and_the_box() {
    for n in 0..=2 {
        let at = |hit| {
            let [l, t, r, b] = update_places(n).into_iter().find(|p| p.0 == hit).unwrap().1;
            ((l + r) / 2, (t + b) / 2)
        };
        for hit in [Hit::Skip, Hit::Later, Hit::Install, Hit::Updates] {
            let (x, y) = at(hit);
            assert_eq!(update_hit(x, y, n), Some(hit), "{hit:?} with {n}");
        }
        assert_eq!(update_hit(i32::MAX, i32::MIN, n), None);
        assert_eq!(update_hit(-1, -1, n), None);
        // The buttons keep the place of Quit and Play, and do not overlap.
        assert_eq!(update_places(n)[2].1, place(Hit::Play, n));
        assert!(update_places(n)[0].1[2] < update_places(n)[1].1[0]);
        assert!(update_places(n)[1].1[2] < update_places(n)[2].1[0]);
        assert!(update_places(n)[3].1[3] < place(Hit::Quit, n)[1]);
    }
    // On the launcher the box sits between Quit and Play, and moves up with them.
    assert_eq!(hit(200, 315, &[0, 1]), Some(Hit::Updates));
    assert_eq!(hit(200, 315 - ROW, &[1]), Some(Hit::Updates));
    assert_eq!(hit(200, 315 - 2 * ROW, &[]), Some(Hit::Updates));
}

#[test]
fn the_update_page_and_the_box_are_drawn() {
    let tree = Temp::new("launcher-update");
    let mut state = State::new(
        Some(install(&tree)),
        [Some(true), Some(true)],
        [false, false],
    );
    let mut fonts = Fonts::new();
    let mut draw = |state: &State| {
        let mut fb = Framebuffer::new(W as u32, height(0) as u32);
        paint(&mut fb, &mut fonts, state);
        fb.pixels
    };
    let plain = draw(&state);
    state.updates = Some(true);
    let boxed = draw(&state);
    assert_ne!(plain, boxed);
    state.updates = Some(false);
    assert_ne!(boxed, draw(&state));
    state.offer = Some(update::Offer {
        version: "9.9.9".into(),
        url: "https://example.org/a".into(),
        sha256: [0; 32],
    });
    let page = draw(&state);
    state.note = Some("Not updated: x. Nothing was changed.".into());
    assert_ne!(page, draw(&state));
    state.busy = true;
    assert_ne!(page, draw(&state));
}

#[test]
fn languages_are_named_and_long_folders_cut_at_the_start() {
    assert_eq!(language_name(1036), "French");
    assert_eq!(language_name(2057), "English");
    assert_eq!(language_name(0), "language 0");
    let font = Font::request("Arial", "9");
    assert_eq!(fit(&font, "C:\\Zoo", 330), b"C:\\Zoo");
    let long = format!("{}\\Zoo Tycoon", "x".repeat(500));
    let short = fit(&font, &long, 330);
    assert!(short.starts_with(b"...") && short.ends_with(b"\\Zoo Tycoon"));
    assert!(font.width(&short) <= 330);
    assert!(fit(&font, "abc", 0).is_empty());
}

/// The pixel at the middle of the dot of a radio button.
fn dot(fb: &Framebuffer, question: usize, yes: bool, n: usize) -> [u8; 3] {
    let [x, top, ..] = place(Hit::Answer(question, yes), n);
    let at = ((top + 6) * W + x + 6) as usize * 4;
    fb.pixels[at..at + 3].try_into().unwrap()
}

#[test]
fn a_reopened_launcher_shows_every_answer_already_set() {
    let tree = Temp::new("launcher-reopen-layout");
    let game = install(&tree);
    let user = tree.0.join("user");
    State::new(Some(game.clone()), [Some(true), Some(false)], [true, true])
        .save(&user)
        .unwrap();
    // What a start reads back from the kept settings.
    let mut state = State::new(Some(game), answers(Some(&user)), [true, true]);
    state.updates = Some(false);
    assert!(state.playable());
    assert_eq!(status(&state), "Zoo Tycoon found (English)");
    let mut fb = Framebuffer::new(W as u32, height(2) as u32);
    paint(&mut fb, &mut Fonts::new(), &state);
    let (on, off) = (LEAF[2], FIELD);
    assert_eq!(dot(&fb, 0, true, 2), on);
    assert_eq!(dot(&fb, 0, false, 2), off);
    assert_eq!(dot(&fb, 1, true, 2), off);
    assert_eq!(dot(&fb, 1, false, 2), on);
    let mut other = Framebuffer::new(W as u32, height(2) as u32);
    state.updates = Some(true);
    paint(&mut other, &mut Fonts::new(), &state);
    assert_ne!(fb.pixels, other.pixels);
}

#[test]
fn play_in_a_reopened_launcher_keeps_the_changes() {
    let tree = Temp::new("launcher-reopen-save");
    let game = install(&tree);
    let user = tree.0.join("user");
    let both = [true, true];
    State::new(Some(game.clone()), [Some(true), Some(true)], both)
        .save(&user)
        .unwrap();
    let mut state = State::new(Some(game.clone()), answers(Some(&user)), both);
    state.answers = [Some(false), Some(true)];
    state.updates = Some(false);
    let args = finish(Ok(true), true, state, Some(&user));
    assert_eq!(
        args,
        Some(vec!["--data".into(), game.clone().into_os_string()])
    );
    assert_eq!(answers(Some(&user)), [Some(false), Some(true)]);
    assert!(!update::enabled(Some(&user)));
    // The box back on is kept too.
    let mut state = State::new(Some(game), answers(Some(&user)), both);
    state.updates = Some(true);
    finish(Ok(true), true, state, Some(&user)).unwrap();
    assert!(update::enabled(Some(&user)));
}

#[test]
fn closing_a_reopened_launcher_without_play_changes_nothing() {
    let tree = Temp::new("launcher-reopen-quit");
    let game = install(&tree);
    let user = tree.0.join("user");
    let both = [true, true];
    State::new(Some(game.clone()), [Some(true), Some(true)], both)
        .save(&user)
        .unwrap();
    let before = std::fs::read_to_string(user.join("zoo.ini")).unwrap();
    let mut state = State::new(Some(tree.0.clone()), [Some(false), Some(false)], both);
    state.updates = Some(false);
    assert_eq!(finish(Ok(false), true, state, Some(&user)), None);
    assert_eq!(
        std::fs::read_to_string(user.join("zoo.ini")).unwrap(),
        before
    );
    assert_eq!(answers(Some(&user)), [Some(true), Some(true)]);
}
