//! A read-only virtual file system over the resource search path of an installation.
//!
//! The game reads `[resource] path` from `zoo.ini` (or `zoodemo.ini` in the base trial): a
//! `;`-separated list of directories relative to the install folder. Every `*.ztd` archive (a
//! plain ZIP file) in those directories holds members. A member that exists in several archives
//! is served from the directory listed first. Directories, archives and members are matched
//! case-insensitively, and `/` and `\` are the same separator in member paths.

use std::collections::HashMap;
use std::error::Error;
use std::fmt;
use std::fs::File;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use zip::ZipArchive;

/// The largest member `Vfs::read` accepts, by its declared uncompressed size: 256 MiB. The
/// biggest member in the game is below 2 MiB; the limit only stops absurd sizes in corrupt or
/// hostile archives from being read into memory.
pub const MAX_MEMBER_SIZE: u64 = 256 * 1024 * 1024;

/// Why an installation could not be opened or a member could not be read. The message is a
/// single line meant to be shown to the user.
#[derive(Debug)]
pub struct VfsError(String);

impl fmt::Display for VfsError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl Error for VfsError {}

fn error<T>(message: String) -> Result<T, VfsError> {
    Err(VfsError(message))
}

/// Where a member is stored: the archive and its entry number inside the archive.
struct Member {
    archive: usize,
    index: usize,
}

/// The members of every archive on the resource search path of one installation.
pub struct Vfs {
    /// Every archive stays open: opening one means parsing its whole directory of entries,
    /// which takes milliseconds for the big ones. The lock serialises reads of one archive.
    archives: Vec<(PathBuf, Mutex<ZipArchive<File>>)>,
    /// Normalised member path to the copy that wins.
    members: HashMap<String, Member>,
    shadowed: usize,
}

impl Vfs {
    /// Opens the installation in `install_dir` and indexes the members of all its archives.
    ///
    /// Fails if the folder or its ini file is missing or unusable, or if any archive in the
    /// search path cannot be read as a ZIP file. Directories of the search path that do not
    /// exist are skipped.
    pub fn open(install_dir: &Path) -> Result<Vfs, VfsError> {
        let ini_path = ["zoo.ini", "zoodemo.ini"]
            .iter()
            .find_map(|name| find_ignoring_case(install_dir, name, false).transpose())
            .transpose()?
            .ok_or_else(|| {
                VfsError(format!(
                    "no zoo.ini or zoodemo.ini in {}",
                    install_dir.display()
                ))
            })?;
        let ini = std::fs::read(&ini_path)
            .map_err(|e| VfsError(format!("cannot read {}: {e}", ini_path.display())))?;
        let search_path = resource_path(&String::from_utf8_lossy(&ini))
            .ok_or_else(|| VfsError(format!("{} has no [resource] path", ini_path.display())))?;

        let mut directories: Vec<PathBuf> = Vec::new();
        for entry in search_path
            .split(';')
            .map(str::trim)
            .filter(|entry| !entry.is_empty())
        {
            if let Some(directory) = resolve_directory(install_dir, entry)?
                && !directories.contains(&directory)
            {
                directories.push(directory);
            }
        }
        if directories.is_empty() {
            return error(format!(
                "[resource] path in {} lists no directory",
                ini_path.display()
            ));
        }

        let mut vfs = Vfs {
            archives: Vec::new(),
            members: HashMap::new(),
            shadowed: 0,
        };
        for directory in &directories {
            for archive_path in archives_in(directory)? {
                vfs.add_archive(archive_path)?;
            }
        }
        Ok(vfs)
    }

    fn add_archive(&mut self, path: PathBuf) -> Result<(), VfsError> {
        let archive = open_archive(&path)?;
        let archive_number = self.archives.len();
        for index in 0..archive.len() {
            let name = match archive.name_for_index(index) {
                Some(Ok(name)) => name,
                _ => return error(format!("{}: bad entry {index}", path.display())),
            };
            if name.ends_with('/') {
                continue; // a directory entry
            }
            let member = Member {
                archive: archive_number,
                index,
            };
            if self
                .members
                .entry(normalise(&name))
                .or_insert(member)
                .archive
                != archive_number
            {
                self.shadowed += 1;
            }
        }
        self.archives.push((path, Mutex::new(archive)));
        Ok(())
    }

    /// Reads the whole member into memory. The path is matched without regard to letter case,
    /// and `/` and `\` are the same separator.
    ///
    /// Fails if the member does not exist, if its declared size exceeds [`MAX_MEMBER_SIZE`], or
    /// if its archive turns out to be unreadable.
    pub fn read(&self, path: &str) -> Result<Vec<u8>, VfsError> {
        let Some(member) = self.members.get(&normalise(path)) else {
            return error(format!("no such member: {path}"));
        };
        let (archive_path, archive) = &self.archives[member.archive];
        let mut archive = archive
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let mut file = archive.by_index(member.index).map_err(|e| {
            VfsError(format!(
                "{}: cannot read {path}: {e}",
                archive_path.display()
            ))
        })?;
        let size = file.size();
        check_size(size, path)?;
        let mut data = Vec::new();
        // `take` keeps a lying header from producing more than the declared size.
        (&mut file).take(size).read_to_end(&mut data).map_err(|e| {
            VfsError(format!(
                "{}: cannot read {path}: {e}",
                archive_path.display()
            ))
        })?;
        if data.len() as u64 != size {
            return error(format!(
                "{}: {path} is shorter than its declared size",
                archive_path.display()
            ));
        }
        Ok(data)
    }

    /// The normalised paths (lower case, `/` separators) of all members, or of those matching
    /// `pattern`, sorted. In a pattern `*` matches any run of characters, `/` included; every
    /// other character stands for itself, regardless of letter case or separator.
    pub fn list(&self, pattern: Option<&str>) -> Vec<String> {
        let pattern = pattern.map(normalise);
        let mut paths: Vec<String> = self
            .members
            .keys()
            .filter(|path| {
                pattern
                    .as_ref()
                    .is_none_or(|p| wildcard_matches(p.as_bytes(), path.as_bytes()))
            })
            .cloned()
            .collect();
        paths.sort();
        paths
    }

    /// The number of `*.ztd` archives indexed.
    pub fn archive_count(&self) -> usize {
        self.archives.len()
    }

    /// The number of distinct member paths, that is, of members that can be read.
    pub fn member_count(&self) -> usize {
        self.members.len()
    }

    /// The number of members hidden by a copy of the same path found earlier on the search path.
    pub fn shadowed_count(&self) -> usize {
        self.shadowed
    }
}

fn check_size(size: u64, path: &str) -> Result<(), VfsError> {
    if size > MAX_MEMBER_SIZE {
        return error(format!(
            "{path} declares {size} bytes, more than the {MAX_MEMBER_SIZE} byte limit"
        ));
    }
    Ok(())
}

fn open_archive(path: &Path) -> Result<ZipArchive<File>, VfsError> {
    let file =
        File::open(path).map_err(|e| VfsError(format!("cannot open {}: {e}", path.display())))?;
    ZipArchive::new(file).map_err(|e| VfsError(format!("{}: not a ZIP file: {e}", path.display())))
}

/// Lower-cases ASCII letters and turns `\` into `/`.
fn normalise(path: &str) -> String {
    path.chars()
        .map(|c| {
            if c == '\\' {
                '/'
            } else {
                c.to_ascii_lowercase()
            }
        })
        .collect()
}

/// Whether `text` matches `pattern`, where `*` matches any run of bytes.
fn wildcard_matches(pattern: &[u8], text: &[u8]) -> bool {
    let (mut p, mut t) = (0, 0);
    // Where the last `*` was, and how much of the text it has swallowed so far.
    let mut star: Option<(usize, usize)> = None;
    while t < text.len() {
        if p < pattern.len() && pattern[p] == b'*' {
            star = Some((p, t));
            p += 1;
        } else if p < pattern.len() && pattern[p] == text[t] {
            p += 1;
            t += 1;
        } else if let Some((star_p, star_t)) = star {
            p = star_p + 1;
            t = star_t + 1;
            star = Some((star_p, star_t + 1));
        } else {
            return false;
        }
    }
    pattern[p..].iter().all(|&c| c == b'*')
}

/// The value of `path` in the `[resource]` section of the ini file next to the executable. The
/// game reads that file through the ini reader of Windows, not through its own configuration
/// reader (`crate::config`): names ignore letter case, only `[section]` headers, `key=value`
/// lines and `;` comment lines count, the first `path` wins, and a value in a pair of single
/// or double quotes loses them.
fn resource_path(ini: &str) -> Option<String> {
    let mut in_resource = false;
    for line in ini.trim_start_matches('\u{feff}').lines() {
        let line = line.trim();
        if line.starts_with(';') {
            continue;
        }
        if let Some(section) = line.strip_prefix('[') {
            in_resource = section
                .strip_suffix(']')
                .is_some_and(|name| name.trim().eq_ignore_ascii_case("resource"));
        } else if let Some((key, value)) = line.split_once('=')
            && in_resource
            && key.trim().eq_ignore_ascii_case("path")
        {
            let value = value.trim();
            let quoted = value.len() >= 2
                && (value.starts_with('"') && value.ends_with('"')
                    || value.starts_with('\'') && value.ends_with('\''));
            return Some(
                if quoted {
                    &value[1..value.len() - 1]
                } else {
                    value
                }
                .to_string(),
            );
        }
    }
    None
}

/// The entry of `directory` called `name` in any letter case, of the given kind (directory or
/// file). If several differ only in case, the first in sorted order is taken.
fn find_ignoring_case(
    directory: &Path,
    name: &str,
    want_directory: bool,
) -> Result<Option<PathBuf>, VfsError> {
    let entries = std::fs::read_dir(directory)
        .map_err(|e| VfsError(format!("cannot read folder {}: {e}", directory.display())))?;
    let mut found: Vec<PathBuf> = entries
        .flatten()
        .filter(|entry| {
            entry
                .file_name()
                .to_str()
                .is_some_and(|n| n.eq_ignore_ascii_case(name))
        })
        .map(|entry| entry.path())
        .filter(|path| path.is_dir() == want_directory)
        .collect();
    found.sort();
    Ok(found.into_iter().next())
}

/// Resolves one entry of the search path against the install folder. `None` if the directory
/// does not exist. Entries that leave the install folder are an error.
fn resolve_directory(install_dir: &Path, entry: &str) -> Result<Option<PathBuf>, VfsError> {
    let entry = entry.replace('\\', "/");
    let mut directory = install_dir.to_path_buf();
    for component in entry.split('/').filter(|c| !c.is_empty() && *c != ".") {
        if component == ".." || component.contains(':') {
            return error(format!(
                "[resource] path entry \"{entry}\" leaves the install folder"
            ));
        }
        match find_ignoring_case(&directory, component, true)? {
            Some(next) => directory = next,
            None => return Ok(None),
        }
    }
    Ok(Some(directory))
}

/// The `*.ztd` files in `directory`, sorted by lower-cased name (then by exact name).
fn archives_in(directory: &Path) -> Result<Vec<PathBuf>, VfsError> {
    let entries = std::fs::read_dir(directory)
        .map_err(|e| VfsError(format!("cannot read folder {}: {e}", directory.display())))?;
    let mut archives: Vec<(String, PathBuf)> = entries
        .flatten()
        .map(|entry| {
            (
                entry.file_name().to_string_lossy().into_owned(),
                entry.path(),
            )
        })
        .filter(|(name, path)| name.to_ascii_lowercase().ends_with(".ztd") && path.is_file())
        .collect();
    archives.sort_by(|(a, _), (b, _)| {
        a.to_ascii_lowercase()
            .cmp(&b.to_ascii_lowercase())
            .then_with(|| a.cmp(b))
    });
    Ok(archives.into_iter().map(|(_, path)| path).collect())
}

#[cfg(test)]
mod tests {
    use std::io::Write;

    use zip::write::SimpleFileOptions;
    use zip::{CompressionMethod, ZipWriter};

    use super::*;

    /// A fresh directory under the system temp folder, removed on drop.
    struct TempDir(PathBuf);

    impl TempDir {
        fn new(name: &str) -> TempDir {
            let path = std::env::temp_dir().join(format!("rustyzoo-{}-{name}", std::process::id()));
            let _ = std::fs::remove_dir_all(&path);
            std::fs::create_dir_all(&path).unwrap();
            TempDir(path)
        }

        /// Writes a file, creating the folders above it.
        fn write(&self, relative: &str, content: &[u8]) {
            let path = self.0.join(relative);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(path, content).unwrap();
        }

        /// Writes a `.ztd` whose members are stored and deflated alternately.
        fn archive(&self, relative: &str, members: &[(&str, &str)]) {
            let mut zip = ZipWriter::new(std::io::Cursor::new(Vec::new()));
            for (i, (name, content)) in members.iter().enumerate() {
                let method = if i % 2 == 0 {
                    CompressionMethod::Stored
                } else {
                    CompressionMethod::Deflated
                };
                zip.start_file(
                    *name,
                    SimpleFileOptions::default().compression_method(method),
                )
                .unwrap();
                zip.write_all(content.as_bytes()).unwrap();
            }
            self.write(relative, &zip.finish().unwrap().into_inner());
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    fn message(result: Result<Vfs, VfsError>) -> String {
        result.err().expect("open should fail").to_string()
    }

    /// Two search directories (one missing, one in another letter case) and overlapping members.
    fn sample(name: &str) -> TempDir {
        let dir = TempDir::new(name);
        dir.write("ZOO.INI", b"[resource]\r\npath=./Updates;./nowhere;.\r\n");
        dir.archive("UPDATES/new.ztd", &[("Animals/Lion.AI", "new")]);
        dir.archive(
            "root-b.ztd",
            &[("animals/lion.ai", "old"), ("animals\\Zebra.ai", "zebra")],
        );
        dir.archive(
            "ROOT-A.ZTD",
            &[("Sounds/roar.wav", "roar"), ("animals/zebra.ai", "x")],
        );
        dir.write("readme.txt", b"not an archive");
        dir
    }

    #[test]
    fn earlier_directory_wins_and_lookup_ignores_case_and_separator() {
        let dir = sample("precedence");
        let vfs = Vfs::open(&dir.0).unwrap();
        assert_eq!(vfs.read("animals/lion.ai").unwrap(), b"new");
        assert_eq!(vfs.read("ANIMALS\\LION.AI").unwrap(), b"new");
        assert_eq!(vfs.read("sounds\\Roar.WAV").unwrap(), b"roar");
        assert_eq!(vfs.archive_count(), 3);
        assert_eq!(vfs.member_count(), 3);
        assert_eq!(vfs.shadowed_count(), 2);
    }

    #[test]
    fn inside_a_directory_the_lower_cased_file_name_decides() {
        let dir = sample("inside");
        // root-a.ztd sorts before root-b.ztd although the names differ in case and style.
        let vfs = Vfs::open(&dir.0).unwrap();
        assert_eq!(vfs.read("animals/zebra.ai").unwrap(), b"x");
    }

    #[test]
    fn list_is_sorted_and_normalised_and_filters_with_wildcards() {
        let dir = sample("list");
        let vfs = Vfs::open(&dir.0).unwrap();
        assert_eq!(
            vfs.list(None),
            ["animals/lion.ai", "animals/zebra.ai", "sounds/roar.wav"]
        );
        assert_eq!(
            vfs.list(Some("ANIMALS\\*")),
            ["animals/lion.ai", "animals/zebra.ai"]
        );
        assert_eq!(vfs.list(Some("*.wav")), ["sounds/roar.wav"]);
        assert_eq!(vfs.list(Some("animals/lion.ai")), ["animals/lion.ai"]);
        assert!(vfs.list(Some("animals")).is_empty());
    }

    #[test]
    fn wildcard_cases() {
        for (pattern, text, expected) in [
            ("", "", true),
            ("*", "", true),
            ("*", "a/b", true),
            ("a*c", "abbc", true),
            ("a*c", "abbd", false),
            ("a*b*c", "aXbYbZc", true),
            ("*b", "abab", true),
            ("*b", "aba", false),
            ("a", "ab", false),
            ("ab", "a", false),
            ("**x", "x", true),
        ] {
            assert_eq!(
                wildcard_matches(pattern.as_bytes(), text.as_bytes()),
                expected,
                "{pattern:?} against {text:?}"
            );
        }
    }

    #[test]
    fn demo_ini_is_used_when_there_is_no_zoo_ini() {
        let dir = TempDir::new("demo-ini");
        dir.write("ZooDemo.ini", b"[Resource]\nPATH = .\n");
        dir.archive("a.ztd", &[("x.txt", "demo")]);
        assert_eq!(Vfs::open(&dir.0).unwrap().read("x.txt").unwrap(), b"demo");

        dir.write("zoo.ini", b"[resource]\npath=./other\n");
        dir.archive("other/b.ztd", &[("y.txt", "full")]);
        let vfs = Vfs::open(&dir.0).unwrap();
        assert_eq!(vfs.list(None), ["y.txt"], "zoo.ini comes first");
    }

    #[test]
    fn resource_path_reads_only_the_resource_section() {
        let ini = "\u{feff}[user]\r\npath=wrong\r\n; path=comment\r\n[resource]\r\n; for the demo\r\n  Path = ./a;.  \r\n[other]\r\npath=wrong\r\n";
        assert_eq!(resource_path(ini).as_deref(), Some("./a;."));
        assert_eq!(
            resource_path("[RESOURCE]\nPATH=\"./q;.\"\n").as_deref(),
            Some("./q;.")
        );
        assert_eq!(
            resource_path("[resource]\npath='.'\n").as_deref(),
            Some(".")
        );
        assert_eq!(resource_path("[resource]\nkey=1\n"), None);
        assert_eq!(resource_path("path=.\n"), None);
        assert_eq!(resource_path(""), None);
    }

    #[test]
    fn open_errors() {
        let missing = std::env::temp_dir().join("rustyzoo-does-not-exist");
        assert!(message(Vfs::open(&missing)).contains("cannot read folder"));

        let dir = TempDir::new("no-ini");
        assert!(message(Vfs::open(&dir.0)).contains("no zoo.ini or zoodemo.ini"));

        dir.write("zoo.ini", b"[user]\nfullscreen=1\n");
        assert!(message(Vfs::open(&dir.0)).contains("has no [resource] path"));

        dir.write("zoo.ini", b"[resource]\npath=;\n");
        assert!(message(Vfs::open(&dir.0)).contains("lists no directory"));

        dir.write("zoo.ini", b"[resource]\npath=./a;../b\n");
        assert!(message(Vfs::open(&dir.0)).contains("leaves the install folder"));

        dir.write("zoo.ini", b"[resource]\npath=.\n");
        dir.write("garbage.ztd", b"this is not a zip file");
        let text = message(Vfs::open(&dir.0));
        assert!(
            text.contains("garbage.ztd") && text.contains("not a ZIP file"),
            "{text}"
        );
    }

    #[test]
    fn missing_member_and_absurd_sizes_are_errors() {
        let dir = sample("read-errors");
        let vfs = Vfs::open(&dir.0).unwrap();
        let text = vfs.read("animals/tiger.ai").unwrap_err().to_string();
        assert_eq!(text, "no such member: animals/tiger.ai");
        assert!(vfs.read("animals").is_err(), "a directory is not a member");

        assert!(check_size(MAX_MEMBER_SIZE, "a").is_ok());
        assert!(check_size(MAX_MEMBER_SIZE + 1, "a").is_err());
        assert!(check_size(u64::MAX, "a").is_err());
    }

    /// The facts below were measured on Zoo Tycoon: Complete Collection.
    #[test]
    #[ignore = "needs the game: set ZT_DATA_DIR to the install folder"]
    fn full_game_facts() {
        let dir = std::env::var_os("ZT_DATA_DIR").expect("ZT_DATA_DIR is not set");
        let vfs = Vfs::open(Path::new(&dir)).unwrap();
        // Four archives hold the member; Updates/animalsa.ztd (26,986 bytes) is listed first.
        assert_eq!(vfs.read("animals/lion.ai").unwrap().len(), 26_986);
        assert_eq!(vfs.read("ANIMALS\\LION.AI").unwrap().len(), 26_986);
        assert_eq!(
            (
                vfs.archive_count(),
                vfs.member_count(),
                vfs.shadowed_count()
            ),
            (174, 328_486, 3_563)
        );
        assert!(
            vfs.list(Some("animals/lion*"))
                .contains(&"animals/lion.ai".to_string())
        );
    }
}
