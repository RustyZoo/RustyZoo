//! The per-user folder: the settings RustyZoo changes and, from M7, the saved games. The game's
//! folder is never written. `zoo.ini` in the user folder holds only the keys RustyZoo changed;
//! [`apply`] reads it on top of the game's `zoo.ini`.
//!
//! The folder of the settings is `$XDG_CONFIG_HOME/RustyZoo` (else `~/.config/RustyZoo`) on
//! Linux, `%APPDATA%\RustyZoo` on Windows and `~/Library/Application Support/RustyZoo` on macOS.
//! The saved games go in `Saved Games` in `$XDG_DATA_HOME/RustyZoo` (else
//! `~/.local/share/RustyZoo`) on Linux, and in the folder of the settings elsewhere; the
//! snapshots go in a folder next to it. The environment variable `RUSTYZOO_USER_DIR` replaces
//! both folders, for tests and CI.

use std::ffi::OsString;
use std::io;
use std::path::{Path, PathBuf};

use crate::config::Config;
use crate::vfs::{Vfs, read_ini};

/// The folder of the settings on this system, or `None` if the system does not say where.
pub fn dir() -> Option<PathBuf> {
    dir_of(std::env::consts::OS, |name| std::env::var_os(name))
}

/// [`dir`] for the operating system `os` and the environment `env`.
pub(crate) fn dir_of(os: &str, env: impl Fn(&str) -> Option<OsString>) -> Option<PathBuf> {
    folder(os, env, "XDG_CONFIG_HOME", ".config")
}

/// The folder where the saved games go, `Saved Games` in the user folder, or `None` if the system
/// does not say where.
pub(crate) fn saved_games() -> Option<PathBuf> {
    saved_of(std::env::consts::OS, |name| std::env::var_os(name))
}

/// [`saved_games`] for the operating system `os` and the environment `env`.
fn saved_of(os: &str, env: impl Fn(&str) -> Option<OsString>) -> Option<PathBuf> {
    let base = folder(os, env, "XDG_DATA_HOME", ".local/share")?;
    Some(base.join("Saved Games"))
}

/// The folder where the snapshots go, named `name` (string 22006) next to the saved games, or
/// `None` if the system does not say where. A name that is empty or holds a path separator, `:`
/// or `.` at its start is replaced by `Screenshots`.
pub(crate) fn screenshots(name: &str) -> Option<PathBuf> {
    saved_games().map(|saved| beside(&saved, name))
}

/// The folder `name` next to the saved games folder `saved`.
fn beside(saved: &Path, name: &str) -> PathBuf {
    let bad = name.is_empty() || name.starts_with('.') || name.contains(['/', '\\', ':']);
    saved.with_file_name(if bad { "Screenshots" } else { name })
}

/// The user folder of `os`: the override, else the base of the system (on Linux `$xdg`, else
/// `~/<under_home>`) and `RustyZoo`.
fn folder(
    os: &str,
    env: impl Fn(&str) -> Option<OsString>,
    xdg: &str,
    under_home: &str,
) -> Option<PathBuf> {
    let var = |name| {
        env(name)
            .filter(|value| !value.is_empty())
            .map(PathBuf::from)
    };
    if let Some(dir) = var("RUSTYZOO_USER_DIR") {
        return Some(dir);
    }
    let base = match os {
        "windows" => var("APPDATA")?,
        "macos" => var("HOME")?.join("Library/Application Support"),
        // The base directory specification ignores a relative path.
        _ => var(xdg)
            .filter(|dir| dir.is_absolute())
            .or_else(|| var("HOME").map(|home| home.join(under_home)))?,
    };
    Some(base.join("RustyZoo"))
}

/// Puts the `zoo.ini` of the folder `dir`, if there is one, on top of the ini file of `vfs`. A
/// folder without that file changes nothing; a file that cannot be read gives an error.
pub fn apply(vfs: &mut Vfs, dir: &Path) -> Result<(), String> {
    let over = read(dir)?;
    vfs.overlay_ini(&over).map_err(|e| e.to_string())
}

/// The value of `key` in `section` of the settings file of the folder `dir`, if it has one.
pub(crate) fn setting(dir: &Path, section: &str, key: &str) -> Option<String> {
    let config = read(dir).ok()?;
    let value = config.section(section)?.get(key)?;
    Some(String::from_utf8_lossy(value).into_owned())
}

/// The settings file of the folder `dir`; empty if it does not exist.
fn read(dir: &Path) -> Result<Config, String> {
    let path = dir.join("zoo.ini");
    match read_ini(&path) {
        Ok(bytes) => Config::parse(&bytes).map_err(|e| format!("{}: {e}", path.display())),
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(Config::default()),
        Err(e) => Err(format!("cannot read {}: {e}", path.display())),
    }
}

/// Writes `key = value` of `section` into the settings file of the folder `dir`, keeping the
/// other keys, and creates the folder and the file if needed. The value must fit on one line.
pub fn save(dir: &Path, section: &str, key: &str, value: &str) -> Result<(), String> {
    let mut config = read(dir)?;
    config.set(section, key, value);
    let path = dir.join("zoo.ini");
    let write = || {
        std::fs::create_dir_all(dir)?;
        // Written aside and renamed, so a failure never leaves half a file.
        let aside = dir.join("zoo.ini.new");
        std::fs::write(&aside, config.to_bytes())?;
        std::fs::rename(aside, &path)
    };
    write().map_err(|e| format!("cannot write {}: {e}", path.display()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fixtures::TempDir;

    fn paths(os: &str, vars: &[(&str, &str)]) -> Option<PathBuf> {
        dir_of(os, |name| {
            vars.iter()
                .find(|(n, _)| *n == name)
                .map(|(_, value)| OsString::from(value))
        })
    }

    #[test]
    fn the_folder_depends_on_the_system() {
        let home = [("HOME", "/h")];
        let path = |text: &str| Some(PathBuf::from(text));
        assert_eq!(paths("linux", &home), path("/h/.config/RustyZoo"));
        // Absolute on the host that runs the test, whatever its rules for paths.
        let absolute = std::env::temp_dir();
        let xdg = absolute.to_str().unwrap();
        let set = [("HOME", "/h"), ("XDG_CONFIG_HOME", xdg)];
        assert_eq!(paths("linux", &set), Some(absolute.join("RustyZoo")));
        let relative = [("HOME", "/h"), ("XDG_CONFIG_HOME", "x")];
        assert_eq!(paths("linux", &relative), path("/h/.config/RustyZoo"));
        let empty = [("HOME", "/h"), ("XDG_CONFIG_HOME", "")];
        assert_eq!(paths("linux", &empty), path("/h/.config/RustyZoo"));
        assert_eq!(
            paths("macos", &home),
            path("/h/Library/Application Support/RustyZoo")
        );
        assert_eq!(
            paths("windows", &[("APPDATA", "C:/a")]),
            path("C:/a/RustyZoo")
        );
    }

    #[test]
    fn the_snapshot_folder_is_next_to_the_saved_games_and_never_leaves_the_user_folder() {
        let saved = Path::new("/u/RustyZoo/Saved Games");
        assert_eq!(
            beside(saved, "screenshots"),
            Path::new("/u/RustyZoo/screenshots")
        );
        for bad in ["", ".", "..", "a/b", "a\\b", "c:", "../x"] {
            assert_eq!(
                beside(saved, bad),
                Path::new("/u/RustyZoo/Screenshots"),
                "{bad}"
            );
        }
    }

    #[test]
    fn the_saved_games_are_in_the_data_folder_on_linux_and_with_the_settings_elsewhere() {
        let saved = |os: &str, vars: &[(&str, &str)]| {
            saved_of(os, |name| {
                vars.iter()
                    .find(|(n, _)| *n == name)
                    .map(|(_, value)| OsString::from(value))
            })
        };
        let path = |text: &str| Some(PathBuf::from(text));
        let home = [("HOME", "/h")];
        assert_eq!(
            saved("linux", &home),
            path("/h/.local/share/RustyZoo/Saved Games")
        );
        let relative = [("HOME", "/h"), ("XDG_DATA_HOME", "x")];
        assert_eq!(
            saved("linux", &relative),
            path("/h/.local/share/RustyZoo/Saved Games")
        );
        assert_eq!(
            saved("macos", &home),
            path("/h/Library/Application Support/RustyZoo/Saved Games")
        );
        assert_eq!(
            saved("windows", &[("APPDATA", "C:/a")]),
            path("C:/a/RustyZoo/Saved Games")
        );
        let both = [("HOME", "/h"), ("RUSTYZOO_USER_DIR", "/t")];
        assert_eq!(saved("linux", &both), path("/t/Saved Games"));
        assert_eq!(saved("linux", &[]), None);
    }

    #[test]
    fn a_missing_variable_gives_no_folder_and_the_override_wins() {
        assert_eq!(paths("linux", &[]), None);
        assert_eq!(paths("linux", &[("HOME", "")]), None);
        assert_eq!(paths("windows", &[("HOME", "/h")]), None);
        assert_eq!(paths("macos", &[]), None);
        let both = [("HOME", "/h"), ("RUSTYZOO_USER_DIR", "/t")];
        assert_eq!(paths("linux", &both), Some(PathBuf::from("/t")));
        assert_eq!(paths("windows", &both), Some(PathBuf::from("/t")));
        assert_eq!(paths("linux", &[("RUSTYZOO_USER_DIR", "")]), None);
    }

    fn game(text: &str) -> (TempDir, Vfs) {
        let install = TempDir::absent(&format!("game-{}", text.len()));
        std::fs::create_dir_all(&install.0).unwrap();
        std::fs::write(install.0.join("zoo.ini"), text).unwrap();
        let vfs = Vfs::open(&install.0).unwrap();
        (install, vfs)
    }

    #[test]
    fn saved_settings_come_back_over_the_game_ini_and_the_game_is_not_written() {
        let text = "[resource]\npath=.\n[User]\nFullScreen=0\nscreenwidth=800\n";
        let (install, mut vfs) = game(text);
        let user = TempDir::absent("save");
        let dir = user.0.join("nested/RustyZoo");
        // No folder yet: nothing changes.
        apply(&mut vfs, &dir).unwrap();
        assert_eq!(
            Config::parse(vfs.ini()).unwrap(),
            Config::parse(text.as_bytes()).unwrap()
        );

        for value in ["1", "0", "-1", "99999999999"] {
            save(&dir, "user", "fullscreen", value).unwrap();
        }
        save(&dir, "user", "other", "5").unwrap();
        let file = std::fs::read_to_string(dir.join("zoo.ini")).unwrap();
        assert_eq!(
            file, "[user]\nfullscreen=99999999999\nother=5\n",
            "only changed keys"
        );

        apply(&mut vfs, &dir).unwrap();
        let ini = Config::parse(vfs.ini()).unwrap();
        let user_section = ini.section("User").unwrap();
        assert_eq!(user_section.get("FullScreen"), Some(&b"99999999999"[..]));
        assert_eq!(user_section.get("screenwidth"), Some(&b"800"[..]), "kept");
        assert_eq!(user_section.get("other"), Some(&b"5"[..]));
        assert!(ini.section("resource").is_some());
        assert_eq!(
            std::fs::read_to_string(install.0.join("zoo.ini")).unwrap(),
            text
        );
        assert!(!dir.join("zoo.ini.new").exists());
    }

    #[test]
    fn a_damaged_settings_file_is_an_error_not_a_panic() {
        let (_install, mut vfs) = game("[resource]\npath=.\n");
        let user = TempDir::absent("bad");
        std::fs::create_dir_all(user.0.join("zoo.ini")).unwrap();
        assert!(
            apply(&mut vfs, &user.0)
                .unwrap_err()
                .contains("cannot read")
        );
        assert!(save(&user.0, "user", "k", "1").is_err());
        std::fs::remove_dir(user.0.join("zoo.ini")).unwrap();
        std::fs::write(user.0.join("zoo.ini"), vec![b'a'; 2 * 1024 * 1024]).unwrap();
        assert!(apply(&mut vfs, &user.0).is_err());
        std::fs::write(user.0.join("zoo.ini"), b"garbage \x00\xff\n[x\n").unwrap();
        apply(&mut vfs, &user.0).unwrap();
    }
}
