//! Helpers the unit tests share: entity types, scratch folders and a one-archive install.

use std::io::Write;
use std::path::PathBuf;
use std::sync::Arc;

use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, ZipWriter};

use crate::config::Config;
use crate::entity::EntityType;

/// An entity type whose definition file is `ini`: id 0, footprint 1 x 1 x 1, no picture.
pub(crate) fn ty(class: &'static str, name: &str, sub: &str, ini: &str) -> EntityType {
    EntityType {
        class,
        type_name: name.into(),
        subtype: sub.into(),
        file: None,
        config: Some(Arc::new(Config::parse(ini.as_bytes()).unwrap())),
        name_id: 0,
        footprint: [1, 1, 1],
        height: 0,
    }
}

/// A type with `keys` in its characteristics: the `[sub/Characteristics/Integers]` section of an
/// animal or a keeper, the plain `[Characteristics/Integers]` one of any other.
pub(crate) fn unit_ty(
    class: &'static str,
    name: &str,
    sub: &str,
    id: i32,
    keys: &str,
) -> EntityType {
    let head = match matches!(class, "animals" | "keeper") && !sub.is_empty() {
        true => format!("[{sub}/Characteristics/Integers]"),
        false => "[Characteristics/Integers]".to_string(),
    };
    EntityType {
        name_id: id,
        ..ty(class, name, sub, &format!("{head}\n{keys}\n"))
    }
}

/// A path under the system temp folder that no other call of this process gets, and that holds
/// nothing yet.
fn unique(name: &str) -> PathBuf {
    static N: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
    let n = N.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let path = std::env::temp_dir().join(format!("rustyzoo-{}-{n}-{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&path);
    path
}

/// A fresh directory under the system temp folder, removed on drop.
pub(crate) struct TempDir(pub PathBuf);

impl TempDir {
    pub(crate) fn new(name: &str) -> TempDir {
        let dir = TempDir::absent(name);
        std::fs::create_dir_all(&dir.0).unwrap();
        dir
    }

    /// The path of a directory that does not exist yet; whatever is made there is removed on drop.
    pub(crate) fn absent(name: &str) -> TempDir {
        TempDir(unique(name))
    }

    /// Writes a file, creating the folders above it.
    pub(crate) fn write(&self, relative: &str, content: &[u8]) {
        let path = self.0.join(relative);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, content).unwrap();
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// An install folder with a `zoo.ini` and the given members in one stored archive, removed on
/// drop.
pub(crate) struct Install(pub PathBuf);

impl Install {
    pub(crate) fn new(name: &str, members: &[(&str, Vec<u8>)]) -> Install {
        let mut dir = TempDir::new(name);
        dir.write("zoo.ini", b"[resource]\npath=.\n");
        let mut zip = ZipWriter::new(std::io::Cursor::new(Vec::new()));
        let options = SimpleFileOptions::default().compression_method(CompressionMethod::Stored);
        for (member, content) in members {
            zip.start_file(*member, options).unwrap();
            zip.write_all(content).unwrap();
        }
        dir.write("a.ztd", &zip.finish().unwrap().into_inner());
        Install(std::mem::take(&mut dir.0)) // `dir` then drops an empty path
    }

    /// Writes a loose file below the install folder.
    pub(crate) fn loose(self, name: &str, content: &[u8]) -> Install {
        let path = self.0.join(name);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, content).unwrap();
        self
    }
}

impl Drop for Install {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// The path of the file of `dir` that is named `name` in any letter case (some installs have
/// upper-case names).
pub(crate) fn file_ignoring_case(dir: &std::ffi::OsStr, name: &str) -> PathBuf {
    let found = std::fs::read_dir(dir)
        .unwrap()
        .flatten()
        .find(|entry| entry.file_name().to_string_lossy().eq_ignore_ascii_case(name));
    found.map_or_else(|| PathBuf::from(dir).join(name), |entry| entry.path())
}

/// Whether the install holds both expansions (a folder of each with archives in it).
pub(crate) fn has_expansions(dir: &std::ffi::OsStr) -> bool {
    ["xpack1", "xpack2"].iter().all(|name| {
        let sub = file_ignoring_case(dir, name);
        std::fs::read_dir(sub).is_ok_and(|entries| {
            entries.flatten().any(|e| {
                e.file_name()
                    .to_string_lossy()
                    .to_ascii_lowercase()
                    .ends_with(".ztd")
            })
        })
    })
}

/// Whether the install is the complete English or French one: both expansions, and the language
/// of its `lang0.dll` (the Dutch install has an English one).
pub(crate) fn is_measured(dir: &std::ffi::OsStr) -> bool {
    matches!(crate::pe::installed_language(dir), 1033 | 1036) && has_expansions(dir)
}

/// Whether the facts of a full-game test were measured on this install (`is_measured`). Prints
/// why the test goes no further when not.
pub(crate) fn measured_on(test: &str, dir: &std::ffi::OsStr) -> bool {
    let measured = is_measured(dir);
    if !measured {
        eprintln!("skipped {test}: its counts and texts were measured on the complete English and French installs");
    }
    measured
}

/// Whether the game files hold `member`. Prints why the test goes no further when they do not
/// (the base game has no expansion maps and animals).
pub(crate) fn has_member(test: &str, vfs: &crate::vfs::Vfs, member: &str) -> bool {
    let found = vfs.contains(member);
    if !found {
        eprintln!("skipped {test}: the install has no {member}");
    }
    found
}

/// Whether `member` is missing from an install without the expansions, which is where the maps
/// and animals of the expansions are: a test goes on to the next one. A member missing from an
/// install with the expansions is a failure for the test to find.
pub(crate) fn base_lacks(vfs: &crate::vfs::Vfs, member: &str) -> bool {
    let lacks = !vfs.contains(member) && !has_expansions(vfs.root().as_os_str());
    if lacks {
        eprintln!("skipped {member}: the install has no expansions");
    }
    lacks
}

/// Whether the install can run a test that reads the maps, scenarios and animals of the
/// expansions. Prints why the test goes no further when it cannot.
pub(crate) fn with_expansions(test: &str, dir: &std::ffi::OsStr) -> bool {
    let found = has_expansions(dir);
    if !found {
        eprintln!("skipped {test}: the install has no expansions");
    }
    found
}
