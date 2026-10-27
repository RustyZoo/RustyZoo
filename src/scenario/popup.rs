//! The popups of a scenario file (scenarios 8.1) and the blocks of interface elements (7.1).
//!
//! Both are read when the goal is made, from the file the goal belongs to, so a goal of an extra
//! file reads its own sections.

use crate::config::{Config, Section, to_int};

/// The element that shows the last popup again, when the file names none.
const HINT_BUTTON: i32 = 45406;

/// A popup: the section `[p]` over `[default]` of the same file.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Popup {
    /// The dialog layout; none (0 or less) shows nothing.
    pub layout: i32,
    /// The pictures of the background, the icon and the two images, by name; empty for none.
    pub pictures: [String; 4],
    /// The string ids of the two texts, and the names of the texts that win when not empty.
    pub short: [i32; 2],
    pub long: [String; 2],
    pub ok: bool,
    pub cancel: bool,
    /// Stops the game until answered.
    pub modal: bool,
    pub hint: Hint,
}

/// The line that stays on the screen after a popup.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Hint {
    /// The layout; 0 or less for none.
    pub layout: i32,
    /// A string id, or -1 for the text of the first goal in progress; the name of a text wins.
    pub short: i32,
    pub long: String,
    /// The element that shows the last popup again.
    pub button: i32,
}

fn text(s: Option<&[u8]>) -> String {
    String::from_utf8_lossy(s.unwrap_or_default()).into_owned()
}

impl Popup {
    /// The popup `p` of `cfg`; none for an id of 0 or less.
    pub fn read(cfg: &Config, p: i32) -> Option<Popup> {
        if p <= 0 {
            return None;
        }
        let (own, base) = (cfg.section(p.to_string()), cfg.section("default"));
        let get = |key: &str| {
            own.and_then(|s: &Section| s.get(key))
                .or_else(|| base?.get(key))
        };
        let int = |key: &str, or: i32| get(key).and_then(to_int).unwrap_or(or);
        let pause = int("pause", 0);
        Some(Popup {
            layout: int("layout", 0),
            pictures: ["background", "icon", "image1", "image2"].map(|k| text(get(k))),
            short: [int("shortText1", 0), int("shortText2", 0)],
            long: [text(get("longText1")), text(get("longText2"))],
            ok: int("ok", 1) != 0,
            cancel: int("cancel", 0) != 0,
            modal: int("modal", pause) != 0,
            hint: Hint {
                layout: int("persistentTextLayout", 0),
                short: int("persistentShortText", 0),
                long: text(get("persistentLongText")),
                button: int("persistentButton", HINT_BUTTON),
            },
        })
    }
}

/// What a trigger does to one element of a block.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Op {
    Disable,
    Enable,
    Hide,
    Show,
}

/// The `id` keys of the section `[block]`.
pub fn ids(cfg: &Config, block: i32) -> Vec<i32> {
    let s = cfg.section(block.to_string());
    let all = s.map_or(&[][..], |s| s.get_all("id"));
    all.iter().filter_map(|v| to_int(v)).collect()
}

/// The entries `<id>=<letter>` of the section `[block]`: the first letter of the value, `d`, `e`,
/// `h` or `s`, lower case only (the example file writes the words), says what to do.
pub fn entries(cfg: &Config, block: i32) -> Vec<(i32, Op)> {
    let Some(s) = cfg.section(block.to_string()) else {
        return Vec::new();
    };
    let entry = |key: &[u8]| {
        let op = match s.get(key)?.first()? {
            b'd' => Op::Disable,
            b'e' => Op::Enable,
            b'h' => Op::Hide,
            b's' => Op::Show,
            _ => return None,
        };
        Some((to_int(key)?, op))
    };
    s.keys().filter_map(entry).collect()
}
