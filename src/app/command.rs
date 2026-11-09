//! What the shell tells the game: the keys, the mouse and the commands of a pass, and the words
//! a script gives them.

use std::fmt;
use std::str::FromStr;

/// A key the interface reads, by the name a script gives it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Key {
    Enter,
    Escape,
    Tab,
    Backspace,
    Delete,
    Left,
    Right,
    Up,
    Down,
    Home,
    End,
    PageUp,
    PageDown,
    Y,
    N,
    S,
    L,
    B,
    F,
    V,
    P,
}

const KEY_NAMES: [(&str, Key); 21] = [
    ("enter", Key::Enter),
    ("escape", Key::Escape),
    ("tab", Key::Tab),
    ("backspace", Key::Backspace),
    ("delete", Key::Delete),
    ("left", Key::Left),
    ("right", Key::Right),
    ("up", Key::Up),
    ("down", Key::Down),
    ("home", Key::Home),
    ("end", Key::End),
    ("pageup", Key::PageUp),
    ("pagedown", Key::PageDown),
    ("y", Key::Y),
    ("n", Key::N),
    ("s", Key::S),
    ("l", Key::L),
    ("b", Key::B),
    ("f", Key::F),
    ("v", Key::V),
    ("p", Key::P),
];

impl Key {
    pub(crate) fn name(self) -> &'static str {
        KEY_NAMES.iter().find(|k| k.1 == self).map_or("", |k| k.0)
    }
}

/// What the mouse does over the window, in window coordinates.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mouse {
    /// The pointer moved here.
    Move(i32, i32),
    /// The left button went down or up at the pointer.
    Down,
    Up,
    /// The left button went down for the second time of a double click, which the system
    /// reports instead of a second `Down`.
    Double,
    /// The wheel turned by this many notches (positive: away from the player).
    Wheel(i32),
    /// The right button went up at the pointer.
    Right,
}

/// What the player does (input 4). The text form is the script format of a headless run:
/// `scroll DX DY`, `pointer X Y`, `pointer none`, `rotate cw`, `rotate ccw`, `zoom in`,
/// `zoom out`, `grid` (toggle the tile grid), `devmode` (Ctrl+D), `shift on` and `shift off` (a Shift key goes down or up), `fullscreen on`, `fullscreen off`, `pause`, `space`, `quit` (end the run), `close` (the
/// window is closed) for the map view, `skip` (any key
/// or button press, which stops an intro movie), and for the
/// interface `mouse X Y`, `down`, `up`, `double` (the second press of a double click), `right` (the right button went up), `wheel N`, `key NAME [ctrl] [shift]` (NAME: enter, escape,
/// tab, backspace, delete, left, right, up, down, home, end, pageup, pagedown, y, n, s, l, b, f, v, p; Ctrl+S saves, Ctrl+L loads, Ctrl+B, Ctrl+F and Ctrl+V press the buttons that hide buildings, foliage and guests and Ctrl+P the snapshot button) and
/// `char C` (a byte, as a number from 0 to 255 or as one character that is not a digit).
/// `press ID` presses the button with that id as a click would, when it is shown and enabled and no
/// question is open (a script does not need the pixel of an element).
/// `save FILE` writes the game to a file and `load FILE` opens a saved game as Continue does (the
/// file name runs to the end of the line; the name is kept for the rest of the run), and
/// `scenario FILE` starts a scenario from the menu (a listed one or not).
/// A window reads `fullscreen` in a script as a request to switch, and makes the command itself
/// when the window has switched, so a log replays it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Command {
    /// One key-down message of a scroll key: the direction on each axis, -1, 0 or 1.
    Scroll(i32, i32),
    /// The pointer is at this position in the window, or has left it (or the window is not
    /// active): the edge scroll of the interface pass reads it.
    Pointer(Option<(i32, i32)>),
    /// A quarter turn, clockwise or not.
    Rotate(bool),
    /// Zoom in or out.
    Zoom(bool),
    /// Ctrl+G: switch the tile grid on or off.
    Grid,
    /// Ctrl+D: switch the developer mode on or off, if the ini allows it (main loop 5.2).
    DevMode,
    /// The window is full screen (true) or windowed (false) now.
    Fullscreen(bool),
    /// The Pause key: toggles the pause while the map view is shown, whatever has the focus.
    Pause,
    /// The space bar: presses the pause button, so it toggles the pause only while the map view
    /// is shown and no element has the keyboard focus (a field types the space).
    Space,
    /// A Shift key went down or up: `true` while one is held. The snapshot button reads it
    /// (map drawing 15.2).
    Shift(bool),
    /// End the run at once.
    Quit,
    /// The player closed the window: the program ends at once from the menu, and asks first in a
    /// game (main menu 9.3).
    Close,
    Mouse(Mouse),
    /// A key of the interface, with Ctrl and Shift held or not.
    Key(Key, bool, bool),
    /// A character typed, as a Windows-1252 byte.
    Char(u8),
    /// A key or a mouse button was pressed: stops the intro movie that plays. Ignored by the game.
    Skip,
    /// `award add ID`: the zoo holds the award (`awards.md` 4.1).
    Award(i32),
    /// `press ID`: the button with this id is pressed and released, as a hotkey does.
    Press(i32),
    Save(&'static str),
    Load(&'static str),
    /// `scenario FILE`: starts the scenario file from the menu, as its Play button does.
    Scenario(&'static str),
}

impl fmt::Display for Command {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match *self {
            Command::Scroll(x, y) => write!(f, "scroll {x} {y}"),
            Command::Pointer(Some((x, y))) => write!(f, "pointer {x} {y}"),
            Command::Pointer(None) => f.write_str("pointer none"),
            Command::Rotate(cw) => write!(f, "rotate {}", if cw { "cw" } else { "ccw" }),
            Command::Zoom(zoom_in) => write!(f, "zoom {}", if zoom_in { "in" } else { "out" }),
            Command::Grid => f.write_str("grid"),
            Command::DevMode => f.write_str("devmode"),
            Command::Fullscreen(on) => write!(f, "fullscreen {}", if on { "on" } else { "off" }),
            Command::Pause => f.write_str("pause"),
            Command::Space => f.write_str("space"),
            Command::Shift(on) => write!(f, "shift {}", if on { "on" } else { "off" }),
            Command::Quit => f.write_str("quit"),
            Command::Close => f.write_str("close"),
            Command::Mouse(Mouse::Move(x, y)) => write!(f, "mouse {x} {y}"),
            Command::Mouse(Mouse::Down) => f.write_str("down"),
            Command::Mouse(Mouse::Up) => f.write_str("up"),
            Command::Mouse(Mouse::Double) => f.write_str("double"),
            Command::Mouse(Mouse::Right) => f.write_str("right"),
            Command::Mouse(Mouse::Wheel(n)) => write!(f, "wheel {n}"),
            Command::Key(key, ctrl, shift) => {
                write!(f, "key {}", key.name())?;
                if ctrl {
                    f.write_str(" ctrl")?;
                }
                if shift {
                    f.write_str(" shift")?;
                }
                Ok(())
            }
            Command::Char(byte) => write!(f, "char {byte}"),
            Command::Skip => f.write_str("skip"),
            Command::Award(id) => write!(f, "award add {id}"),
            Command::Press(id) => write!(f, "press {id}"),
            Command::Save(file) => write!(f, "save {file}"),
            Command::Load(file) => write!(f, "load {file}"),
            Command::Scenario(file) => write!(f, "scenario {file}"),
        }
    }
}

impl FromStr for Command {
    type Err = String;

    fn from_str(text: &str) -> Result<Command, String> {
        let words: Vec<&str> = text.split_whitespace().collect();
        let number = |w: &str| w.parse::<i32>().map_err(|_| format!("not a number: {w:?}"));
        let file = |word: &str| match text.trim().strip_prefix(word).map(str::trim) {
            Some(file) if !file.is_empty() => Ok(&*Box::leak(file.to_string().into_boxed_str())),
            _ => Err(format!("{word} takes a file name: {text:?}")),
        };
        Ok(match words.as_slice() {
            ["award", "add", id] => Command::Award(number(id)?),
            ["press", id] => Command::Press(number(id)?),
            ["save", ..] => Command::Save(file("save")?),
            ["load", ..] => Command::Load(file("load")?),
            ["scenario", ..] => Command::Scenario(file("scenario")?),
            ["scroll", x, y] => {
                let (x, y) = (number(x)?, number(y)?);
                if !(-1..=1).contains(&x) || !(-1..=1).contains(&y) {
                    return Err("scroll takes directions from -1 to 1".into());
                }
                Command::Scroll(x, y)
            }
            ["pointer", "none"] => Command::Pointer(None),
            ["pointer", x, y] => Command::Pointer(Some((number(x)?, number(y)?))),
            ["rotate", "cw"] => Command::Rotate(true),
            ["rotate", "ccw"] => Command::Rotate(false),
            ["zoom", "in"] => Command::Zoom(true),
            ["zoom", "out"] => Command::Zoom(false),
            ["grid"] => Command::Grid,
            ["devmode"] => Command::DevMode,
            ["fullscreen", "on"] => Command::Fullscreen(true),
            ["fullscreen", "off"] => Command::Fullscreen(false),
            ["shift", "on"] => Command::Shift(true),
            ["shift", "off"] => Command::Shift(false),
            ["pause"] => Command::Pause,
            ["space"] => Command::Space,
            ["quit"] => Command::Quit,
            ["close"] => Command::Close,
            ["skip"] => Command::Skip,
            ["mouse", x, y] => Command::Mouse(Mouse::Move(number(x)?, number(y)?)),
            ["down"] => Command::Mouse(Mouse::Down),
            ["up"] => Command::Mouse(Mouse::Up),
            ["double"] => Command::Mouse(Mouse::Double),
            ["right"] => Command::Mouse(Mouse::Right),
            ["wheel", n] => Command::Mouse(Mouse::Wheel(number(n)?)),
            ["key", name, mods @ ..] => {
                let key = KEY_NAMES
                    .iter()
                    .find(|k| k.0 == *name)
                    .ok_or_else(|| format!("not a key: {name:?}"))?
                    .1;
                let (mut ctrl, mut shift) = (false, false);
                for m in mods {
                    match *m {
                        "ctrl" => ctrl = true,
                        "shift" => shift = true,
                        _ => return Err(format!("not a key modifier: {m:?}")),
                    }
                }
                Command::Key(key, ctrl, shift)
            }
            ["char", c] => match (c.parse::<u8>(), c.as_bytes()) {
                (Ok(byte), _) => Command::Char(byte),
                (_, [byte]) if byte.is_ascii() && !byte.is_ascii_digit() => Command::Char(*byte),
                _ => return Err(format!("not a character: {c:?}")),
            },
            _ => return Err(format!("not a command: {text:?}")),
        })
    }
}
