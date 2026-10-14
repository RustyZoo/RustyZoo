//! The texts of the game: strings and `TEXT` entries, looked up the way the original does.
//!
//! Every `lang*.dll` of the install folder is loaded, whatever its name beyond the pattern.
//! They are ordered by file name, descending, comparing the bytes of the names as stored on
//! disk: `lang300.dll` first, `lang0.dll` last, so the base game is the fallback and each
//! expansion overrides it. A string number or a `TEXT` name resolves to the first library that
//! has it non-empty; one that no library has is empty. The `[lib]` and `[language]` entries of
//! the ini file are not used.

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::error::Error;
use std::fmt;
use std::path::Path;

use crate::pe;
use crate::vfs::MAX_MEMBER_SIZE;

/// Why the libraries could not be loaded. The message is a single line.
#[derive(Debug)]
pub struct StringsError(String);

impl fmt::Display for StringsError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl Error for StringsError {}

/// One library: its file name, its non-empty strings and its `TEXT` entries by upper-case name.
struct Library {
    name: String,
    strings: BTreeMap<u32, String>,
    texts: HashMap<String, Vec<u8>>,
}

impl Library {
    /// `None` if the file cannot be read as a library with resources.
    fn load(name: &str, file: &[u8]) -> Option<Library> {
        let resources = pe::resources(file).ok()?;
        let strings = pe::strings(&resources).ok()?;
        let mut texts = HashMap::new();
        for (text, data) in pe::texts(&resources) {
            texts
                .entry(text.to_ascii_uppercase())
                .or_insert_with(|| data.to_vec());
        }
        Some(Library {
            name: name.to_string(),
            strings,
            texts,
        })
    }
}

/// The names of the files of the folder that match `<prefix>*.dll` (letter case ignored), the
/// greatest first, comparing the names as stored: the order of lookup.
pub(crate) fn library_names(install_dir: &Path, prefix: &str) -> Result<Vec<String>, StringsError> {
    let listing = std::fs::read_dir(install_dir)
        .map_err(|e| StringsError(format!("cannot read folder {}: {e}", install_dir.display())))?;
    let names: BTreeSet<String> = listing
        .flatten()
        .filter(|entry| entry.path().is_file())
        .filter_map(|entry| entry.file_name().into_string().ok())
        .filter(|name| {
            let lower = name.to_ascii_lowercase();
            lower.starts_with(prefix) && lower.ends_with(".dll")
        })
        .collect();
    Ok(names.into_iter().rev().collect())
}

/// The `lang*.dll` libraries of one install, in lookup order.
pub struct StringTables {
    libraries: Vec<Library>,
}

impl StringTables {
    /// Loads the libraries of the install folder `install_dir`.
    ///
    /// Fails if the folder cannot be read or holds no usable `lang*.dll`. A matching file that
    /// is not a PE file with resources is skipped.
    pub fn open(install_dir: &Path) -> Result<StringTables, StringsError> {
        let names = library_names(install_dir, "lang")?;
        let mut libraries = Vec::new();
        for name in &names {
            let path = install_dir.join(name);
            let size =
                std::fs::metadata(&path).map_err(|e| StringsError(format!("{name}: {e}")))?;
            if size.len() > MAX_MEMBER_SIZE {
                continue;
            }
            let file = std::fs::read(&path).map_err(|e| StringsError(format!("{name}: {e}")))?;
            libraries.extend(Library::load(name, &file));
        }
        if libraries.is_empty() {
            return Err(StringsError(format!(
                "no usable lang*.dll in {}",
                install_dir.display()
            )));
        }
        Ok(StringTables { libraries })
    }

    /// The file names of the libraries, in lookup order.
    pub fn libraries(&self) -> impl Iterator<Item = &str> {
        self.libraries.iter().map(|l| l.name.as_str())
    }

    /// Whether the names differ in letter case. The order compares the names as stored, so an
    /// upper-case name sorts before the lower-case ones and is probably not where the user
    /// expects it.
    pub fn mixed_case(&self) -> bool {
        let lower = |l: &Library| l.name == l.name.to_ascii_lowercase();
        self.libraries.iter().any(lower) && !self.libraries.iter().all(lower)
    }

    /// The text of string `id` and the library that supplies it; `None` if no library has it.
    pub fn string_source(&self, id: u32) -> Option<(&str, &str)> {
        // ponytail: the numbers of user-created content (low 24 bits in 19000..=21999,
        // 49000..=51999 or 74000..=76999; a string table holds numbers up to 65535 only, so
        // the last range never meets a library) are not in the libraries; the text of that content
        // is not specified yet, so they are empty.
        let user = id & 0xFF_FFFF;
        if [19000..=21999, 49000..=51999, 74000..=76999]
            .iter()
            .any(|range| range.contains(&user))
        {
            return None;
        }
        self.libraries.iter().find_map(|l| {
            let text = l.strings.get(&id)?;
            Some((l.name.as_str(), text.as_str()))
        })
    }

    /// The text of string `id`, or an empty text if no library has it.
    pub fn string(&self, id: u32) -> &str {
        self.string_source(id).map_or("", |(_, text)| text)
    }

    /// The bytes of the `TEXT` entry `name` (matched ignoring letter case) and the library that
    /// supplies it; `None` if no library has it.
    pub fn text_source(&self, name: &str) -> Option<(&str, &[u8])> {
        // ponytail: an empty name is an empty text; the bytes end at the first zero byte, and
        // are returned undecoded (Windows-1252 in the English version).
        let name = name.to_ascii_uppercase();
        if name.is_empty() {
            return None;
        }
        self.libraries.iter().find_map(|l| {
            let data = l.texts.get(&name)?;
            let end = data.iter().position(|&b| b == 0).unwrap_or(data.len());
            Some((l.name.as_str(), &data[..end]))
        })
    }

    /// The bytes of the `TEXT` entry `name`, or an empty text if no library has it.
    pub fn text(&self, name: &str) -> &[u8] {
        self.text_source(name).map_or(&[], |(_, data)| data)
    }

    /// The merged view: every string number that resolves to a text, with the library that
    /// supplies it and the text.
    pub fn strings(&self) -> BTreeMap<u32, (&str, &str)> {
        let ids: BTreeSet<u32> = self
            .libraries
            .iter()
            .flat_map(|l| l.strings.keys().copied())
            .collect();
        ids.into_iter()
            .filter_map(|id| Some((id, self.string_source(id)?)))
            .collect()
    }

    /// The merged view of the `TEXT` entries: upper-case name to the supplying library and the
    /// bytes.
    pub fn texts(&self) -> BTreeMap<String, (&str, &[u8])> {
        let names: BTreeSet<&String> = self.libraries.iter().flat_map(|l| l.texts.keys()).collect();
        names
            .into_iter()
            .filter_map(|name| Some((name.clone(), self.text_source(name)?)))
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pe::tests::library;
    use std::path::PathBuf;

    struct TempDir(PathBuf);

    impl TempDir {
        fn new(name: &str) -> TempDir {
            let path = std::env::temp_dir()
                .join(format!("rustyzoo-strings-{}-{name}", std::process::id()));
            let _ = std::fs::remove_dir_all(&path);
            std::fs::create_dir_all(&path).unwrap();
            TempDir(path)
        }

        fn write(&self, name: &str, content: &[u8]) {
            std::fs::write(self.0.join(name), content).unwrap();
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    /// Four libraries that overlap, in the layout of an install with one expansion.
    fn install(name: &str) -> TempDir {
        let dir = TempDir::new(name);
        dir.write(
            "lang0.dll",
            &library(
                &[
                    (107, "base"),
                    (314, "base"),
                    (5006, "Lion"),
                    (5007, "Zebra"),
                ],
                &[("LION_GENERAL", b"base text\0junk"), ("ZEBRA", b"zebra")],
            ),
        );
        dir.write(
            "lang001.dll",
            &library(&[(107, "patch"), (5184, "Megatherium")], &[]),
        );
        // 5007 is empty here: the search goes on to lang0.dll.
        dir.write(
            "lang100.dll",
            &library(
                &[(314, "expansion"), (5007, "")],
                &[("LION_GENERAL", b"expansion text")],
            ),
        );
        dir.write("lang300.dll", &library(&[(314, "top")], &[]));
        // Files that the pattern does not take or that are not libraries.
        dir.write("res200.dll", &library(&[(314, "wrong list")], &[]));
        dir.write("lang5.dll", b"not a PE file");
        dir.write("lang.txt", b"x");
        std::fs::create_dir(dir.0.join("lang7.dll")).unwrap();
        dir
    }

    #[test]
    fn libraries_are_ordered_by_name_descending() {
        let dir = install("order");
        let tables = StringTables::open(&dir.0).unwrap();
        // "lang0.dll" < "lang001.dll" because "." sorts before the digits.
        let order: Vec<&str> = tables.libraries().collect();
        assert_eq!(
            order,
            ["lang300.dll", "lang100.dll", "lang001.dll", "lang0.dll"]
        );
        assert!(!tables.mixed_case());
    }

    #[test]
    fn the_first_library_with_a_non_empty_string_wins() {
        let dir = install("strings");
        let tables = StringTables::open(&dir.0).unwrap();
        assert_eq!(tables.string_source(314), Some(("lang300.dll", "top")));
        assert_eq!(tables.string_source(107), Some(("lang001.dll", "patch")));
        assert_eq!(tables.string_source(5007), Some(("lang0.dll", "Zebra")));
        assert_eq!(tables.string(5006), "Lion");
        assert_eq!(tables.string(5184), "Megatherium");
        assert_eq!(tables.string(9999), "");
        assert_eq!(tables.string_source(9999), None);
    }

    #[test]
    fn user_content_numbers_are_empty() {
        let dir = TempDir::new("user");
        let ids = [18999, 19000, 21999, 22000, 49000, 51999, 52000];
        let strings: Vec<(u32, &str)> = ids.iter().map(|&id| (id, "x")).collect();
        dir.write("lang0.dll", &library(&strings, &[]));
        let tables = StringTables::open(&dir.0).unwrap();
        let found: Vec<u32> = ids
            .into_iter()
            .filter(|&id| !tables.string(id).is_empty())
            .collect();
        assert_eq!(found, [18999, 22000, 52000]);
    }

    #[test]
    fn text_entries_follow_the_same_order() {
        let dir = install("texts");
        let tables = StringTables::open(&dir.0).unwrap();
        assert_eq!(
            tables.text_source("LION_GENERAL"),
            Some(("lang100.dll", &b"expansion text"[..]))
        );
        assert_eq!(tables.text("lion_general"), b"expansion text");
        assert_eq!(tables.text("ZEBRA"), b"zebra");
        assert_eq!(tables.text("NOPE"), b"");
        assert_eq!(tables.text(""), b"");
        // Without the override of lang100.dll the bytes end at the first zero byte.
        let dir = TempDir::new("nul");
        dir.write(
            "lang0.dll",
            &library(&[], &[("B", b""), ("A", b"base text\0junk")]),
        );
        let tables = StringTables::open(&dir.0).unwrap();
        assert_eq!(tables.text("a"), b"base text");
        assert_eq!(tables.text_source("B"), Some(("lang0.dll", &b""[..])));
    }

    #[test]
    fn the_merged_view_names_the_supplier() {
        let dir = install("merged");
        let tables = StringTables::open(&dir.0).unwrap();
        let merged = tables.strings();
        let ids: Vec<u32> = merged.keys().copied().collect();
        assert_eq!(ids, [107, 314, 5006, 5007, 5184]);
        assert_eq!(merged[&314], ("lang300.dll", "top"));
        let texts = tables.texts();
        assert_eq!(texts.len(), 2);
        assert_eq!(
            texts["LION_GENERAL"],
            ("lang100.dll", &b"expansion text"[..])
        );
        assert_eq!(texts["ZEBRA"], ("lang0.dll", &b"zebra"[..]));
    }

    #[test]
    fn the_name_pattern_ignores_case_and_the_order_does_not() {
        let dir = TempDir::new("case");
        dir.write("lang0.dll", &library(&[(1, "base")], &[]));
        dir.write("LANG300.DLL", &library(&[(1, "upper")], &[]));
        let tables = StringTables::open(&dir.0).unwrap();
        // "L" sorts before "l": the upper-case name comes last and loses.
        assert_eq!(
            tables.libraries().collect::<Vec<_>>(),
            ["lang0.dll", "LANG300.DLL"]
        );
        assert_eq!(tables.string(1), "base");
        assert!(tables.mixed_case());
    }

    #[test]
    fn no_usable_library_is_an_error() {
        let dir = TempDir::new("none");
        dir.write("res0.dll", &library(&[(1, "x")], &[]));
        dir.write("lang0.dll", b"MZ");
        let err = StringTables::open(&dir.0).err().unwrap().to_string();
        assert!(err.contains("no usable lang*.dll"), "{err}");
        let missing = std::env::temp_dir().join("rustyzoo-strings-does-not-exist");
        let err = StringTables::open(&missing).err().unwrap().to_string();
        assert!(err.contains("cannot read folder"), "{err}");
    }

    /// The facts below were measured on Zoo Tycoon: Complete Collection.
    #[test]
    #[ignore = "needs the game: set ZT_DATA_DIR to the install folder"]
    fn full_game_facts() {
        let dir = std::env::var_os("ZT_DATA_DIR").expect("ZT_DATA_DIR is not set");
        let tables = StringTables::open(Path::new(&dir)).unwrap();
        assert_eq!(tables.libraries().count(), 19);
        assert_eq!(tables.libraries().next(), Some("lang300.dll"));
        assert_eq!(tables.libraries().last(), Some("lang0.dll"));
        assert_eq!(tables.string_source(314).map(|s| s.0), Some("lang300.dll"));
        assert_eq!(
            tables.string_source(7077),
            Some(("lang300.dll", "Holiday Tree"))
        );
        assert_eq!(
            tables.string_source(5184),
            Some(("lang019.dll", "Komodo Dragon"))
        );
        assert_eq!(tables.string_source(5006), Some(("lang0.dll", "Lion")));
        assert_eq!(tables.text("lion_general").len(), 1595);
    }
}
