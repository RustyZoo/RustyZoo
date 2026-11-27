//! The launcher: a small RustyZoo window shown by a start without arguments when no game folder
//! is found or a start question (statistics, crash reports) has no answer yet. It shows the game
//! folder with a Browse button and the questions this build would ask (none without a server or
//! with reports turned off); Play keeps the folder and the answers in the user settings and starts
//! the game, Quit or closing the window ends the run. It draws with the embedded fonts and loads
//! nothing from the game.

use std::error::Error;
use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use rustyzoo::text::{Font, Justify, Rect, Style, draw, to_cp1252};
use rustyzoo::{Framebuffer, crash, install, stats, strings, userdir};
use sdl3::dialog::{DialogError, show_open_folder_dialog};
use sdl3::event::{Event, WindowEvent};
use sdl3::mouse::MouseButton;
use sdl3::pixels::PixelFormat;
use sdl3::render::ScaleMode;

/// The size of the drawing, shown twice as large; the height is for two questions.
const W: i32 = 480;
const H: i32 = 340;
const SCALE: u32 = 2;
/// The height of one question.
const ROW: i32 = 64;

type Rgb = [u8; 3];
const WOOD: [Rgb; 3] = [[122, 84, 46], [168, 124, 74], [70, 46, 24]];
const SAND: [Rgb; 3] = [[226, 208, 160], [246, 234, 198], [170, 148, 98]];
const LEAF: [Rgb; 3] = [[70, 132, 52], [122, 182, 88], [36, 82, 28]];
const GREY: [Rgb; 3] = [[150, 150, 130], [180, 180, 160], [110, 110, 95]];
const GRAIN: Rgb = [112, 76, 41];
const FIELD: Rgb = [250, 244, 224];
const INK: Rgb = [52, 36, 20];
const NOTE: Rgb = [96, 74, 46];
const CREAM: Rgb = [255, 248, 220];
const LINK: Rgb = [40, 80, 150];
const RED: Rgb = [170, 40, 30];
const WHITE: Rgb = [255, 255, 255];

/// The settings keys of the two questions, in the order they are shown.
const KEYS: [&str; 2] = [stats::KEY, crash::KEY];
const QUESTIONS: [(&str, &str); 2] = [
    (
        "Send anonymous start statistics?",
        "Version, build, system and architecture only. No identifier.",
    ),
    (
        "Send a report when the game crashes?",
        "The error, the version, your system. Never your files or saved games.",
    ),
];

/// What a click lands on.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Hit {
    Browse,
    /// The answer to the question in a slot (in `PLACES`), or to the question 0 or 1 (in a hit).
    Answer(usize, bool),
    Privacy,
    Quit,
    Play,
}

/// Where each clickable thing is: left, top, right, bottom, inclusive.
const PLACES: [(Hit, [i32; 4]); 8] = [
    (Hit::Browse, [380, 86, 456, 108]),
    (Hit::Answer(0, true), [24, 182, 74, 194]),
    (Hit::Answer(0, false), [84, 182, 134, 194]),
    (Hit::Answer(1, true), [24, 246, 74, 258]),
    (Hit::Answer(1, false), [84, 246, 134, 258]),
    (Hit::Privacy, [W - 100, 268, W - 25, 285]),
    (Hit::Quit, [24, 302, 124, 328]),
    (Hit::Play, [W - 145, 300, W - 25, 330]),
];

/// The height of the drawing with `n` questions.
fn height(n: usize) -> i32 {
    H - ROW * (2 - n.min(2) as i32)
}

/// Where `hit` is with `n` questions: the things under them move up when fewer are asked.
fn place(hit: Hit, n: usize) -> [i32; 4] {
    let [l, t, r, b] = PLACES
        .iter()
        .find(|(h, _)| *h == hit)
        .map_or([0; 4], |p| p.1);
    let up = if matches!(hit, Hit::Privacy | Hit::Quit | Hit::Play) {
        H - height(n)
    } else {
        0
    };
    [l, t - up, r, b - up]
}

/// What a click at (`x`, `y`) lands on when the questions `asked` (their numbers, in order) are shown.
fn hit(x: i32, y: i32, asked: &[usize]) -> Option<Hit> {
    PLACES.iter().find_map(|&(h, _)| {
        let [l, t, r, b] = place(h, asked.len());
        let inside = (l..=r).contains(&x) && (t..=b).contains(&y);
        match h {
            Hit::Answer(slot, yes) => asked
                .get(slot)
                .filter(|_| inside)
                .map(|&q| Hit::Answer(q, yes)),
            Hit::Privacy if asked.is_empty() => None,
            _ => inside.then_some(h),
        }
    })
}

/// What the launcher shows and the player chose.
#[derive(Debug)]
struct State {
    folder: Option<PathBuf>,
    /// The language number of the install in `folder`, when the engine can open it.
    language: Option<u32>,
    answers: [Option<bool>; 2],
    /// The questions this build asks.
    asks: [bool; 2],
}

impl State {
    fn new(folder: Option<PathBuf>, answers: [Option<bool>; 2], asks: [bool; 2]) -> State {
        let mut state = State {
            folder: None,
            language: None,
            answers,
            asks,
        };
        state.set_folder(folder);
        state
    }

    fn set_folder(&mut self, folder: Option<PathBuf>) {
        self.language = folder
            .as_deref()
            .filter(|dir| install::usable(dir))
            .map(|dir| strings::Language::of(dir).number);
        self.folder = folder;
    }

    /// The numbers of the questions shown, in order.
    fn asked(&self) -> Vec<usize> {
        (0..2).filter(|&q| self.asks[q]).collect()
    }

    /// Play needs a folder the engine opens and an answer to each question asked.
    fn playable(&self) -> bool {
        self.language.is_some() && !unanswered(self.answers, self.asks)
    }

    /// Keeps the folder and the answers in the settings folder `user`.
    fn save(&self, user: &Path) -> Result<(), String> {
        if let Some(dir) = &self.folder {
            install::keep(user, dir)?;
        }
        for ((key, answer), asked) in KEYS.iter().zip(self.answers).zip(self.asks) {
            if let (true, Some(answer)) = (asked, answer) {
                crash::keep(user, key, answer)?;
            }
        }
        Ok(())
    }
}

/// Whether a question asked has no answer.
fn unanswered(answers: [Option<bool>; 2], asks: [bool; 2]) -> bool {
    answers
        .iter()
        .zip(asks)
        .any(|(a, asked)| asked && a.is_none())
}

/// Whether a start shows the launcher: no folder found or a question asked without an answer.
fn needed(folder: Option<&Path>, answers: [Option<bool>; 2], asks: [bool; 2]) -> bool {
    folder.is_none() || unanswered(answers, asks)
}

/// The answers kept in the settings folder `user`.
fn answers(user: Option<&Path>) -> [Option<bool>; 2] {
    KEYS.map(|key| user.and_then(|dir| crash::consent(dir, key)))
}

/// The arguments a start without arguments goes on with: none when the launcher is not needed
/// or cannot be shown (a warning), `--data <folder>` after Play; `None` after Quit.
pub fn start() -> Option<Vec<OsString>> {
    if std::env::var_os("ZT_DATA_DIR").is_some_and(|dir| !dir.is_empty()) {
        return Some(Vec::new());
    }
    let user = userdir::dir();
    let asks = [stats::asks(), crash::asks()];
    let mut state = State::new(install::find(), answers(user.as_deref()), asks);
    if !needed(state.folder.as_deref(), state.answers, asks) {
        return Some(Vec::new());
    }
    match show(&mut state) {
        Ok(true) => {}
        Ok(false) => return None,
        Err(e) => {
            eprintln!("rustyzoo: warning: no launcher: {e}");
            return Some(Vec::new());
        }
    }
    let saved = user.ok_or_else(|| "the system gives no user folder".to_string());
    if let Err(e) = saved.and_then(|user| state.save(&user)) {
        eprintln!("rustyzoo: warning: the launcher choices are not kept: {e}");
    }
    let folder = state
        .folder
        .map(PathBuf::into_os_string)
        .unwrap_or_default();
    Some(vec!["--data".into(), folder])
}

/// Shows the window until Play (`true`) or Quit (`false`).
fn show(state: &mut State) -> Result<bool, Box<dyn Error>> {
    let asked = state.asked();
    let h = height(asked.len());
    let sdl = sdl3::init()?;
    let video = sdl.video()?;
    let mut window = video
        .window("RustyZoo", W as u32 * SCALE, h as u32 * SCALE)
        .build()?;
    crate::window::set_icon(&mut window);
    let mut canvas = sdl3::render::create_renderer(window, None)?;
    let textures = canvas.texture_creator();
    let mut texture = textures.create_texture_streaming(PixelFormat::RGBA32, W as u32, h as u32)?;
    texture.set_scale_mode(ScaleMode::Nearest);
    let mut pump = sdl.event_pump()?;
    let mut fonts = Fonts::new();
    let mut picture = Framebuffer::new(W as u32, h as u32);
    let picked: Arc<Mutex<Option<Option<PathBuf>>>> = Arc::new(Mutex::new(None));
    let mut browsing = false;
    loop {
        if let Some(folder) = picked.lock().ok().and_then(|mut slot| slot.take()) {
            browsing = false;
            if folder.is_some() {
                state.set_folder(folder);
            }
        }
        paint(&mut picture, &mut fonts, state);
        texture.update(None, &picture.pixels, W as usize * 4)?;
        canvas.copy(&texture, None, None)?;
        canvas.present();
        let Some(event) = pump.wait_event_timeout_ms(100) else {
            continue;
        };
        let (x, y) = match event {
            Event::Quit { .. }
            | Event::Window {
                win_event: WindowEvent::CloseRequested,
                ..
            } => return Ok(false),
            Event::MouseButtonDown {
                mouse_btn: MouseButton::Left,
                x,
                y,
                ..
            } => (x, y),
            _ => continue,
        };
        let (w, wh) = canvas.window().size();
        let at = |v: f32, size: u32, logical: i32| (v * logical as f32 / size.max(1) as f32) as i32;
        match hit(at(x, w, W), at(y, wh, h), &asked) {
            Some(Hit::Browse) if !browsing => {
                browsing = true;
                let slot = Arc::clone(&picked);
                show_open_folder_dialog(
                    state.folder.as_deref(),
                    false,
                    Some(canvas.window()),
                    Box::new(move |result, _| {
                        let folder = match result {
                            Ok(dirs) => dirs.into_iter().next(),
                            Err(DialogError::Canceled) => None,
                            Err(e) => {
                                eprintln!("rustyzoo: warning: no folder picker: {e}");
                                None
                            }
                        };
                        if let Ok(mut slot) = slot.lock() {
                            *slot = Some(folder);
                        }
                    }),
                );
            }
            Some(Hit::Answer(question, yes)) => state.answers[question] = Some(yes),
            Some(Hit::Privacy) => {
                if let Err(e) = sdl3::url::open_url(crash::POLICY) {
                    eprintln!("rustyzoo: warning: the privacy policy does not open: {e}");
                }
            }
            Some(Hit::Quit) => return Ok(false),
            Some(Hit::Play) if state.playable() => return Ok(true),
            _ => {}
        }
    }
}

/// The fonts of the launcher, Liberation Sans at pixel sizes 22, 16, 13, 12 and 11.
struct Fonts {
    title: Font,
    play: Font,
    bold: Font,
    body: Font,
    note: Font,
}

impl Fonts {
    fn new() -> Fonts {
        Fonts {
            title: Font::request("Arial Bold", "16"),
            play: Font::request("Arial Bold", "12"),
            bold: Font::request("Arial Bold", "10"),
            body: Font::request("Arial", "9"),
            note: Font::request("Arial", "8"),
        }
    }
}

/// The name of the language number `lcid`, by its primary language.
fn language_name(lcid: u32) -> String {
    let name = match lcid & 0x3ff {
        0x04 => "Chinese",
        0x06 => "Danish",
        0x07 => "German",
        0x09 => "English",
        0x0a => "Spanish",
        0x0c => "French",
        0x10 => "Italian",
        0x11 => "Japanese",
        0x12 => "Korean",
        0x13 => "Dutch",
        0x14 => "Norwegian",
        0x15 => "Polish",
        0x16 => "Portuguese",
        0x1d => "Swedish",
        _ => return format!("language {lcid}"),
    };
    name.into()
}

/// The line under the folder: whether it holds the game.
fn status(state: &State) -> String {
    match (state.language, &state.folder) {
        (Some(lcid), _) => format!("Zoo Tycoon found ({})", language_name(lcid)),
        (None, Some(_)) => "This folder holds no Zoo Tycoon install".into(),
        (None, None) => "Zoo Tycoon not found: choose its folder".into(),
    }
}

/// `text` cut at the start to `width` pixels of `font`, with "..." in front when cut.
fn fit(font: &Font, text: &str, width: i32) -> Vec<u8> {
    let bytes = to_cp1252(text);
    if font.width(&bytes) <= width {
        return bytes;
    }
    (1..bytes.len())
        .map(|cut| [b"...".as_slice(), &bytes[cut..]].concat())
        .find(|short| font.width(short) <= width)
        .unwrap_or_default()
}

fn paint(fb: &mut Framebuffer, fonts: &mut Fonts, state: &State) {
    let asked = state.asked();
    let n = asked.len();
    let h = height(n);
    bevel(fb, [0, 0, W - 1, h - 1], WOOD, 3);
    for y in (6..h - 6).step_by(7) {
        line(fb, (4, y), (W - 5, y + 2), GRAIN);
    }
    bevel(fb, [10, 8, W - 11, 50], LEAF, 2);
    icon(fb, 18, 13);
    let title = (&mut fonts.title, 58, W, 29);
    write(fb, title, b"RustyZoo", CREAM, Justify::Left);

    bevel(fb, [10, 58, W - 11, h - 50], SAND, 2);
    write(
        fb,
        (&mut fonts.bold, 24, W, 74),
        b"Game folder",
        INK,
        Justify::Left,
    );
    bevel(fb, [24, 86, 370, 108], [FIELD, SAND[2], SAND[1]], 2);
    let folder = state.folder.as_ref().map(|d| d.display().to_string());
    let shown = fit(&fonts.body, folder.as_deref().unwrap_or(""), 330);
    write(
        fb,
        (&mut fonts.body, 32, 370, 97),
        &shown,
        INK,
        Justify::Left,
    );
    button(
        fb,
        &mut fonts.bold,
        place(Hit::Browse, n),
        b"Browse...",
        SAND,
        INK,
    );
    let colour = if state.language.is_some() {
        LEAF[0]
    } else {
        RED
    };
    disc(fb, 24, 114, 13, colour, colour);
    if state.language.is_some() {
        line(fb, (27, 120), (29, 123), WHITE);
        line(fb, (29, 123), (33, 116), WHITE);
        line(fb, (28, 120), (30, 123), WHITE);
        line(fb, (30, 123), (34, 116), WHITE);
    } else {
        line(fb, (27, 117), (33, 123), WHITE);
        line(fb, (33, 117), (27, 123), WHITE);
    }
    let ink = if state.language.is_some() {
        LEAF[2]
    } else {
        RED
    };
    let line_text = to_cp1252(&status(state));
    write(
        fb,
        (&mut fonts.body, 42, W, 120),
        &line_text,
        ink,
        Justify::Left,
    );
    if n > 0 {
        line(fb, (24, 138), (W - 25, 138), SAND[2]);
        line(fb, (24, 139), (W - 25, 139), SAND[1]);
    }

    for (slot, &q) in asked.iter().enumerate() {
        let (question, note) = QUESTIONS[q];
        let y = 154 + ROW * slot as i32;
        write(
            fb,
            (&mut fonts.bold, 24, W, y),
            question.as_bytes(),
            INK,
            Justify::Left,
        );
        write(
            fb,
            (&mut fonts.note, 24, W, y + 17),
            note.as_bytes(),
            NOTE,
            Justify::Left,
        );
        for yes in [true, false] {
            let [x, top, ..] = place(Hit::Answer(slot, yes), n);
            radio(fb, x, top, state.answers[q] == Some(yes));
            let label: &[u8] = if yes { b"Yes" } else { b"No" };
            write(
                fb,
                (&mut fonts.body, x + 18, W, top + 6),
                label,
                INK,
                Justify::Left,
            );
        }
    }
    if n > 0 {
        let privacy = b"Privacy policy";
        let top = place(Hit::Privacy, n)[1];
        write(
            fb,
            (&mut fonts.note, 0, W - 25, top + 8),
            privacy,
            LINK,
            Justify::Right,
        );
        let left = W - 25 - fonts.note.width(privacy);
        line(fb, (left, top + 15), (W - 25, top + 15), LINK);
    }

    button(fb, &mut fonts.bold, place(Hit::Quit, n), b"Quit", SAND, INK);
    let (colours, ink) = if state.playable() {
        (LEAF, CREAM)
    } else {
        (GREY, [210, 210, 195])
    };
    button(
        fb,
        &mut fonts.play,
        place(Hit::Play, n),
        b"Play",
        colours,
        ink,
    );
}

fn set(fb: &mut Framebuffer, x: i32, y: i32, [r, g, b]: Rgb) {
    let at = (y * W + x) as usize * 4;
    if (0..W).contains(&x) && y >= 0 && at + 4 <= fb.pixels.len() {
        fb.pixels[at..at + 4].copy_from_slice(&[r, g, b, 255]);
    }
}

fn line(fb: &mut Framebuffer, (x0, y0): (i32, i32), (x1, y1): (i32, i32), colour: Rgb) {
    let steps = (x1 - x0).abs().max((y1 - y0).abs()).max(1);
    for i in 0..=steps {
        let at = |a: i32, b: i32| a + ((b - a) * i * 2 + steps) / (steps * 2);
        set(fb, at(x0, x1), at(y0, y1), colour);
    }
}

/// A filled rectangle with `width` pixels of light top and left edges and dark bottom and right
/// ones; `[face, light, dark]`.
fn bevel(fb: &mut Framebuffer, [l, t, r, b]: [i32; 4], [face, light, dark]: [Rgb; 3], width: i32) {
    for y in t..=b {
        for x in l..=r {
            set(fb, x, y, face);
        }
    }
    for i in 0..width {
        line(fb, (l + i, t + i), (r - i, t + i), light);
        line(fb, (l + i, t + i), (l + i, b - i), light);
        line(fb, (l + i, b - i), (r - i, b - i), dark);
        line(fb, (r - i, t + i), (r - i, b - i), dark);
    }
}

fn button(
    fb: &mut Framebuffer,
    font: &mut Font,
    [l, t, r, b]: [i32; 4],
    text: &[u8],
    colours: [Rgb; 3],
    ink: Rgb,
) {
    let [l0, t0, r0, b0] = [l - 1, t - 1, r + 1, b + 1];
    for (from, to) in [
        ((l0, t0), (r0, t0)),
        ((l0, b0), (r0, b0)),
        ((l0, t0), (l0, b0)),
        ((r0, t0), (r0, b0)),
    ] {
        line(fb, from, to, WOOD[2]);
    }
    bevel(fb, [l, t, r, b], colours, 2);
    write(
        fb,
        (font, l, r + 1, (t + b) / 2),
        text,
        ink,
        Justify::Center,
    );
}

/// A disc of diameter `d` in the square at `(x, y)`, its edge in `edge`.
fn disc(fb: &mut Framebuffer, x: i32, y: i32, d: i32, fill: Rgb, edge: Rgb) {
    let r2 = d * d; // radius squared, in halves of a pixel squared times 4
    for py in 0..d {
        for px in 0..d {
            let (dx, dy) = (2 * px + 1 - d, 2 * py + 1 - d);
            let dist = dx * dx + dy * dy;
            if dist <= r2 {
                let rim = dist > (d - 2) * (d - 2);
                set(fb, x + px, y + py, if rim { edge } else { fill });
            }
        }
    }
}

fn radio(fb: &mut Framebuffer, x: i32, y: i32, on: bool) {
    disc(fb, x, y, 13, FIELD, SAND[2]);
    // The upper half of the rim is darker, as if sunk.
    for py in 0..6 {
        for px in 0..13 {
            let (dx, dy) = (2 * px - 12, 2 * py - 12);
            let dist = dx * dx + dy * dy;
            if dist <= 169 && dist > 121 {
                set(fb, x + px, y + py, WOOD[2]);
            }
        }
    }
    if on {
        disc(fb, x + 3, y + 3, 7, LEAF[2], LEAF[2]);
    }
}

/// The application icon with its top left corner at `(x, y)`, its transparent pixels left out.
fn icon(fb: &mut Framebuffer, x: i32, y: i32) {
    let Ok((side, rgba)) = crate::window::icon_pixels() else {
        return;
    };
    for (n, p) in rgba.as_chunks::<4>().0.iter().enumerate() {
        let (px, py) = ((n as u32 % side) as i32, (n as u32 / side) as i32);
        if p[3] >= 128 {
            set(fb, x + px, y + py, [p[0], p[1], p[2]]);
        }
    }
}

/// One line of `text` with `font`, between `left` and `right`, centred on the row `mid`.
fn write(
    fb: &mut Framebuffer,
    (font, left, right, mid): (&mut Font, i32, i32, i32),
    text: &[u8],
    color: Rgb,
    justify: Justify,
) {
    let top = mid - font.line_height() / 2;
    let rect = Rect {
        left,
        top,
        right,
        bottom: top + font.line_height(),
    };
    let style = Style {
        color,
        justify,
        shadow: false,
    };
    draw(fb, font, rect, text, &style);
}

#[cfg(test)]
mod tests {
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
        assert_eq!(hit(30, 188, &[]), None);
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
}
