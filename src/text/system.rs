//! The system's Arial, read at run time and never shipped (text 9). Only a player's plain window asks
//! for it; every other run, test and CI keeps the embedded Liberation Sans, so the pictures are
//! the same on every system.

use std::ffi::OsString;
use std::path::PathBuf;

/// The folders that may hold Arial on `os`, given the environment `env`.
pub(super) fn dirs(os: &str, env: impl Fn(&str) -> Option<OsString>) -> Vec<PathBuf> {
    let home = env("HOME").map(PathBuf::from);
    let under = |sub: &str| home.as_ref().map(|h| h.join(sub));
    let fixed = |list: &[&str]| list.iter().map(PathBuf::from).collect::<Vec<_>>();
    match os {
        "windows" => {
            let win = env("WINDIR").or_else(|| env("SystemRoot"));
            let win = win.map_or_else(|| PathBuf::from(r"C:\Windows"), PathBuf::from);
            vec![win.join("Fonts")]
        }
        "macos" => {
            let mut list = fixed(&["/System/Library/Fonts/Supplemental", "/Library/Fonts"]);
            list.extend(under("Library/Fonts"));
            list
        }
        _ => {
            let mut list = fixed(&[
                "/usr/share/fonts/truetype/msttcorefonts",
                "/usr/share/fonts/msttcore",
                "/usr/share/fonts/msttcorefonts",
                "/usr/share/fonts/TTF",
                "/usr/share/fonts/truetype",
                "/usr/share/fonts",
                "/usr/local/share/fonts",
            ]);
            list.extend(under(".local/share/fonts"));
            list.extend(under(".fonts"));
            list
        }
    }
}

/// The file names Arial Regular or Bold has on the three systems and in the msttcorefonts packages.
fn names(bold: bool) -> [&'static str; 3] {
    match bold {
        false => ["arial.ttf", "Arial.ttf", "Arial.TTF"],
        true => ["arialbd.ttf", "Arial Bold.ttf", "Arial_Bold.ttf"],
    }
}

/// The bytes of the first file in `dirs` that is Arial in the wanted weight, if any; the
/// caller checks that they parse as a font.
pub(super) fn read(dirs: &[PathBuf], bold: bool) -> impl Iterator<Item = Vec<u8>> + '_ {
    dirs.iter()
        .flat_map(move |dir| names(bold).map(|name| dir.join(name)))
        .filter_map(|path| std::fs::read(path).ok())
}

/// Whether the run draws with the system's Arial: an interactive one, unless the environment
/// variable `ZT_EMBEDDED_FONT` is set (`forced`) to keep the embedded font.
pub(super) fn wanted(headless: bool, forced: bool) -> bool {
    !headless && !forced
}
