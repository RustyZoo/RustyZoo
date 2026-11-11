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
use crate::text::from_cp1252;
use crate::vfs::MAX_MEMBER_SIZE;

/// The install language that the text of string 22015 gives: a decimal number, else (missing,
/// zero or not a number) [`SHIPPED_LANGUAGE`].
pub(crate) fn language_number(text: &str) -> u32 {
    match text.trim().parse() {
        Ok(number) if number != 0 => number,
        _ => SHIPPED_LANGUAGE,
    }
}

/// The language the game ships in, US English (Windows language identifier 1033): the install
/// language of a folder that does not give one.
pub(crate) const SHIPPED_LANGUAGE: u32 = 1033;

/// The install language: its number (string 22015) and the words of the default exhibit names
/// (strings 6208 to 6210: exhibit, tank, show tank).
#[derive(Clone, Debug, PartialEq)]
pub struct Language {
    pub number: u32,
    pub exhibit_words: [String; 3],
}

impl Language {
    /// The language the game ships in, with its English words.
    pub(crate) fn shipped() -> Language {
        Language {
            number: SHIPPED_LANGUAGE,
            exhibit_words: ["Exhibit", "Tank", "Show Tank"].map(String::from),
        }
    }

    /// The language of the game folder `install_dir`. A folder whose libraries cannot be read is
    /// a warning and the shipped language.
    pub fn of(install_dir: &Path) -> Language {
        match StringTables::open(install_dir) {
            Ok(tables) => tables.language(),
            Err(why) => {
                eprintln!("rustyzoo: warning: install language: {why}");
                Language::shipped()
            }
        }
    }
}

/// Why the libraries could not be loaded. The message is a single line.
#[derive(Debug)]
pub struct StringsError(String);

impl fmt::Display for StringsError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl Error for StringsError {}

/// The text of a `TEXT` entry in `language` (a Windows language identifier), cut at the first
/// zero byte. The code page comes from the language: the data entries say 1252 whatever the
/// language. Japanese is 932, Simplified Chinese 936, Korean 949, Traditional Chinese 950; any
/// other language is Windows-1252. Bytes that the code page lacks become U+FFFD.
pub fn decode_text(language: u32, data: &[u8]) -> String {
    decode(
        language,
        &data[..data.iter().position(|&b| b == 0).unwrap_or(data.len())],
    )
}

/// The text of `data` in the code page of `language`, zero bytes included.
pub(crate) fn decode(language: u32, data: &[u8]) -> String {
    match encoding(language) {
        Some(encoding) => encoding.decode_without_bom_handling(data).0.into_owned(),
        None => data.iter().map(|&b| from_cp1252(b)).collect(),
    }
}

/// The bytes of `text` in the code page of `language`: the inverse of [`decode_text`].
pub(crate) fn encode_text(language: u32, text: &str) -> Vec<u8> {
    match encoding(language) {
        Some(encoding) => encoding.encode(text).0.into_owned(),
        None => crate::text::to_cp1252(text),
    }
}

/// The bytes of the character `c` in the code page of `language`, `None` when the page lacks it.
pub(crate) fn encode_char(language: u32, c: char) -> Option<Vec<u8>> {
    match encoding(language) {
        Some(encoding) => {
            let mut buf = [0; 4];
            let (bytes, _, lacks) = encoding.encode(c.encode_utf8(&mut buf));
            (!lacks).then(|| bytes.into_owned())
        }
        None => crate::text::to_cp1252_exact(c).map(|b| vec![b]),
    }
}

/// The double-byte code page of `language`, `None` for the Windows-1252 ones.
fn encoding(language: u32) -> Option<&'static encoding_rs::Encoding> {
    match language {
        1041 => Some(encoding_rs::SHIFT_JIS),
        2052 | 4100 => Some(encoding_rs::GBK),
        1042 => Some(encoding_rs::EUC_KR),
        1028 | 3076 | 5124 => Some(encoding_rs::BIG5),
        _ => None,
    }
}

/// One library: its file name, its non-empty strings and its `TEXT` entries by upper-case name.
struct Library {
    name: String,
    strings: BTreeMap<u32, String>,
    texts: HashMap<String, String>,
}

impl Library {
    /// `Err` with the reason if the file cannot be read as a library with resources.
    fn load(name: &str, file: &[u8]) -> Result<Library, String> {
        let resources = pe::resources(file).map_err(|e| e.to_string())?;
        let strings = pe::strings(&resources).map_err(|e| e.to_string())?;
        let mut texts = HashMap::new();
        for (text, language, data) in pe::texts(&resources) {
            texts
                .entry(text.to_ascii_uppercase())
                .or_insert_with(|| decode_text(language, data));
        }
        Ok(Library {
            name: name.to_string(),
            strings,
            texts,
        })
    }
}

/// Whether `warning` was not printed yet in this run. The original loads its libraries once, so a
/// skipped library is reported once, however often the tables are opened again.
fn first_time(warning: String) -> bool {
    static SEEN: std::sync::Mutex<BTreeSet<String>> = std::sync::Mutex::new(BTreeSet::new());
    SEEN.lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .insert(warning)
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

/// One file of the install folder as a library.
fn read_library(install_dir: &Path, name: &str) -> Result<Library, String> {
    let path = install_dir.join(name);
    let size = std::fs::metadata(&path).map_err(|e| e.to_string())?.len();
    if size > MAX_MEMBER_SIZE {
        return Err(format!("it is {size} bytes, more than the limit"));
    }
    Library::load(name, &std::fs::read(&path).map_err(|e| e.to_string())?)
}

/// The `lang*.dll` libraries of one install, in lookup order.
pub struct StringTables {
    libraries: Vec<Library>,
    /// The libraries that were left out, and why.
    warnings: Vec<String>,
}

/// Whether a name id belongs to user-created content: the whole id is in 19000..=21999,
/// 49000..=51999 or 74000..=76999 (entity types 10.3). No mask: high bits make it another id.
pub(crate) fn is_user_id(id: i32) -> bool {
    [19000..=21999, 49000..=51999, 74000..=76999]
        .iter()
        .any(|range| range.contains(&id))
}

impl StringTables {
    /// Loads the libraries of the install folder `install_dir`.
    ///
    /// Fails if the folder cannot be read or holds no usable `lang*.dll`. A matching file that
    /// cannot be read or is not a PE file with resources is skipped, with a warning on standard
    /// error and in [`warnings`](StringTables::warnings): like the original, the others load.
    /// The original loads a library as a program, the engine only reads its resources.
    pub fn open(install_dir: &Path) -> Result<StringTables, StringsError> {
        let names = library_names(install_dir, "lang")?;
        let mut libraries = Vec::new();
        let mut warnings = Vec::new();
        for name in &names {
            match read_library(install_dir, name) {
                Ok(library) => libraries.push(library),
                Err(why) => {
                    let line = format!("{name} is skipped: {why}");
                    if first_time(format!("{}\0{line}", install_dir.display())) {
                        eprintln!("rustyzoo: warning: {line}");
                    }
                    warnings.push(line);
                }
            }
        }
        if libraries.is_empty() {
            return Err(StringsError(format!(
                "no usable lang*.dll in {}",
                install_dir.display()
            )));
        }
        Ok(StringTables {
            libraries,
            warnings,
        })
    }

    /// The libraries that were left out, one line each.
    pub fn warnings(&self) -> &[String] {
        &self.warnings
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
    pub(crate) fn string_source(&self, id: u32) -> Option<(&str, &str)> {
        // The texts of user-created content (`is_user_id`) are not in the libraries: the
        // interface gets them from the definition files (`usertext`). A string table holds
        // numbers up to 65535 only, so the last range never meets a library.
        if is_user_id(id as i32) {
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

    /// The text of the `TEXT` entry `name` (matched ignoring letter case) and the library that
    /// supplies it; `None` if no library has it.
    pub(crate) fn text_source(&self, name: &str) -> Option<(&str, &str)> {
        // ponytail: an empty name is an empty text.
        let name = name.to_ascii_uppercase();
        if name.is_empty() {
            return None;
        }
        self.libraries
            .iter()
            .find_map(|l| Some((l.name.as_str(), l.texts.get(&name)?.as_str())))
    }

    /// The text of the `TEXT` entry `name`, or an empty text if no library has it.
    pub(crate) fn text(&self, name: &str) -> &str {
        self.text_source(name).map_or("", |(_, text)| text)
    }

    /// The install language: the number of string 22015 (1033 English, 1036 French). A missing,
    /// zero or non-decimal text reads as [`SHIPPED_LANGUAGE`] (localisation 4 and 11).
    pub(crate) fn install_language(&self) -> u32 {
        language_number(self.string(22015))
    }

    /// The install language with the words of its default exhibit names.
    pub(crate) fn language(&self) -> Language {
        Language {
            number: self.install_language(),
            exhibit_words: [6208, 6209, 6210].map(|id| self.string(id).to_string()),
        }
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
    /// text.
    pub fn texts(&self) -> BTreeMap<String, (&str, &str)> {
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
    use crate::fixtures::TempDir;
    use crate::pe::tests::{library, library_in};

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
    fn the_install_language_is_string_22015() {
        for (text, language) in [
            ("1036", 1036),
            (" 1033 ", 1033),
            ("0", SHIPPED_LANGUAGE),
            ("", SHIPPED_LANGUAGE),
            ("-1", SHIPPED_LANGUAGE),
            ("french", SHIPPED_LANGUAGE),
            ("99999999999999999999", SHIPPED_LANGUAGE),
        ] {
            let dir = TempDir::new("language");
            dir.write("lang0.dll", &library(&[(22015, text)], &[]));
            let tables = StringTables::open(&dir.0).unwrap();
            assert_eq!(tables.install_language(), language, "{text:?}");
        }
        let dir = TempDir::new("no-language");
        dir.write("lang0.dll", &library(&[(314, "x")], &[]));
        let tables = StringTables::open(&dir.0).unwrap();
        assert_eq!(tables.install_language(), SHIPPED_LANGUAGE);
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
    fn a_user_id_is_the_whole_id_in_a_range() {
        for id in [19000, 21999, 49000, 51999, 74000, 76999] {
            assert!(is_user_id(id), "{id}");
        }
        for id in [
            18999,
            22000,
            48999,
            52000,
            73999,
            77000,
            0,
            -1,
            i32::MAX,
            i32::MIN,
        ] {
            assert!(!is_user_id(id), "{id}");
        }
        assert!(
            !is_user_id((1 << 24) | 19000),
            "high bits are not masked away"
        );
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
            Some(("lang100.dll", "expansion text"))
        );
        assert_eq!(tables.text("lion_general"), "expansion text");
        assert_eq!(tables.text("ZEBRA"), "zebra");
        assert_eq!(tables.text("NOPE"), "");
        assert_eq!(tables.text(""), "");
        // Without the override of lang100.dll the bytes end at the first zero byte.
        let dir = TempDir::new("nul");
        dir.write(
            "lang0.dll",
            &library(&[], &[("B", b""), ("A", b"base text\0junk")]),
        );
        let tables = StringTables::open(&dir.0).unwrap();
        assert_eq!(tables.text("a"), "base text");
        assert_eq!(tables.text_source("B"), Some(("lang0.dll", "")));
    }

    #[test]
    fn a_text_is_decoded_by_the_language_of_its_entry() {
        // Each pair is the same two bytes: they are two Latin letters in Windows-1252, one
        // character in a double-byte code page. The data entry says 1252 in every case.
        for (language, bytes, text) in [
            (1041, [0x82, 0xA0], "\u{3042}"),
            (2052, [0xD6, 0xD0], "\u{4E2D}"),
            (1028, [0xA4, 0xA4], "\u{4E2D}"),
            (1042, [0xB0, 0xA1], "\u{AC00}"),
            (1033, [0xE9, 0x80], "\u{E9}\u{20AC}"),
            (1036, [0xE9, 0x80], "\u{E9}\u{20AC}"),
            (0, [0xE9, 0x80], "\u{E9}\u{20AC}"),
            (u32::MAX, [0xE9, 0x80], "\u{E9}\u{20AC}"),
        ] {
            assert_eq!(decode_text(language, &bytes), text, "{language}");
        }
        // A cut lead byte, a zero byte and no bytes at all never end the run.
        assert_eq!(decode_text(1041, &[b'a', 0x82]), "a\u{FFFD}");
        assert_eq!(decode_text(1041, b"ab\0\x82\xA0"), "ab");
        assert_eq!(decode_text(1041, b""), "");
        assert_eq!(decode_text(1033, b"\0"), "");
        // The bytes of a Japanese library are read as Japanese, those of the base as 1252.
        let dir = TempDir::new("japanese");
        dir.write(
            "lang0.dll",
            &library(&[], &[("A", &[0xE9]), ("B", b"base")]),
        );
        dir.write(
            "lang001.dll",
            &library_in(1041, &[], &[("A", &[0x82, 0xA0])]),
        );
        let tables = StringTables::open(&dir.0).unwrap();
        assert_eq!(tables.text_source("A"), Some(("lang001.dll", "\u{3042}")));
        assert_eq!(tables.text("B"), "base");
        // The interface still draws Windows-1252: a Japanese character is a `?` until #1279.
        assert_eq!(crate::ui::draw::Strings::text_entry(&tables, b"A"), b"?");
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
        assert_eq!(texts["LION_GENERAL"], ("lang100.dll", "expansion text"));
        assert_eq!(texts["ZEBRA"], ("lang0.dll", "zebra"));
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

    /// The libraries of the mod compatibility cases, in an install folder.
    fn mod_install(name: &str) -> TempDir {
        let dir = TempDir::new(name);
        let put = |file: &str, strings: &[(u32, &str)], texts: &[(&str, &[u8])]| {
            dir.write(file, &library(strings, texts));
        };
        put("lang0.dll", &[(100, "base"), (101, "base101")], &[]);
        put("lang300.dll", &[(100, "cc")], &[]);
        put("langZA01.dll", &[(100, "mod"), (102, "modnew")], &[]);
        put("lang--extra.dll", &[(101, "tail"), (103, "tail103")], &[]);
        put("LANG--ZTCDD.DLL", &[(103, "upper"), (104, "last")], &[]);
        dir
    }

    #[test]
    fn a_mod_library_that_sorts_first_overrides_and_one_that_sorts_last_only_adds() {
        let dir = mod_install("mod-order");
        let tables = StringTables::open(&dir.0).unwrap();
        assert_eq!(
            tables.libraries().collect::<Vec<_>>(),
            [
                "langZA01.dll",
                "lang300.dll",
                "lang0.dll",
                "lang--extra.dll",
                "LANG--ZTCDD.DLL"
            ]
        );
        let found: Vec<&str> = (100..=104).map(|id| tables.string(id)).collect();
        assert_eq!(found, ["mod", "base101", "modnew", "tail103", "last"]);
        assert!(tables.warnings().is_empty());
    }

    #[test]
    fn an_empty_entry_of_a_mod_library_does_not_blank_the_shipped_string() {
        let dir = mod_install("mod-empty");
        dir.write(
            "langZA01.dll",
            &library(&[(100, "mod"), (101, ""), (102, "modnew")], &[]),
        );
        let tables = StringTables::open(&dir.0).unwrap();
        assert_eq!(tables.string(101), "base101");
    }

    #[test]
    fn a_library_that_does_not_load_is_skipped_with_a_warning_and_the_others_load() {
        let dir = mod_install("mod-bad");
        dir.write("langbad.dll", b"MZ");
        std::fs::create_dir(dir.0.join("langdir.dll")).unwrap();
        let tables = StringTables::open(&dir.0).unwrap();
        assert_eq!(tables.string(100), "mod");
        assert_eq!(tables.libraries().count(), 5);
        assert_eq!(tables.warnings().len(), 1, "{:?}", tables.warnings());
        assert!(tables.warnings()[0].contains("langbad.dll"));
    }

    /// Child half of the test below: opens the tables of the install named by the environment
    /// twice; does nothing in a normal run.
    #[test]
    fn open_twice_for_the_parent_test() {
        if let Some(dir) = std::env::var_os("ZT_TEST_OPEN_TWICE") {
            StringTables::open(Path::new(&dir)).unwrap();
            StringTables::open(Path::new(&dir)).unwrap();
        }
    }

    #[test]
    fn opening_the_tables_twice_prints_the_skipped_library_warning_once() {
        let dir = mod_install("mod-once");
        dir.write("langbad.dll", b"MZ");
        let out = std::process::Command::new(std::env::current_exe().unwrap())
            .args([
                "strings::tests::open_twice_for_the_parent_test",
                "--exact",
                "--nocapture",
            ])
            .env("ZT_TEST_OPEN_TWICE", &dir.0)
            .output()
            .unwrap();
        let stderr = String::from_utf8_lossy(&out.stderr);
        assert!(out.status.success(), "{stderr}");
        assert_eq!(
            stderr.matches("langbad.dll is skipped").count(),
            1,
            "{stderr}"
        );
    }

    #[test]
    fn a_library_in_a_search_folder_is_not_loaded() {
        let dir = mod_install("mod-folder");
        dir.write("dlupdate/lang999.dll", &library(&[(100, "nope")], &[]));
        dir.write("updates/langZZ.dll", &library(&[(100, "nope")], &[]));
        let tables = StringTables::open(&dir.0).unwrap();
        assert_eq!(tables.string(100), "mod");
        assert_eq!(tables.libraries().count(), 5);
    }

    #[test]
    fn a_mod_library_can_change_the_install_language_but_not_user_content() {
        let dir = mod_install("mod-settings");
        dir.write(
            "langZA01.dll",
            &library(&[(22015, "1036"), (19500, "user"), (100, "mod")], &[]),
        );
        let tables = StringTables::open(&dir.0).unwrap();
        assert_eq!(tables.install_language(), 1036);
        assert_eq!(tables.string(19500), "");
        assert_eq!(tables.string(100), "mod");
    }

    #[test]
    fn a_present_text_entry_wins_even_when_it_is_empty() {
        let dir = mod_install("mod-text");
        dir.write("lang300.dll", &library(&[(100, "cc")], &[("FOO", b"")]));
        dir.write("lang0.dll", &library(&[(100, "base")], &[("FOO", b"x")]));
        let tables = StringTables::open(&dir.0).unwrap();
        assert_eq!(tables.text_source("foo"), Some(("lang300.dll", "")));
        assert_eq!(tables.text("FOO"), "");
    }

    /// The facts below were measured on Zoo Tycoon: Complete Collection.
    #[test]
    #[ignore = "needs the game: set ZT_DATA_DIR to the install folder"]
    fn full_game_facts() {
        let dir = std::env::var_os("ZT_DATA_DIR").expect("ZT_DATA_DIR is not set");
        let tables = StringTables::open(Path::new(&dir)).unwrap();
        let language = crate::pe::installed_language(&dir);
        assert_eq!(tables.install_language(), language);
        // The upper-case names of some installs are matched too.
        assert!(
            (tables.libraries().last().unwrap()).eq_ignore_ascii_case("lang0.dll"),
            "{:?}",
            tables.libraries().last()
        );
        // The count, the order and the texts below are those of the complete installs.
        if !crate::fixtures::measured_on("full_game_facts (libraries)", &dir) {
            return;
        }
        assert_eq!(tables.libraries().count(), 19);
        assert_eq!(tables.libraries().next(), Some("lang300.dll"));
        assert_eq!(tables.libraries().last(), Some("lang0.dll"));
        if language == 1036 {
            // The same libraries supply the same ids; the texts are not English.
            for (id, library) in [
                (7077, "lang300.dll"),
                (5184, "lang019.dll"),
                (5006, "lang0.dll"),
            ] {
                let (found, text) = tables.string_source(id).unwrap();
                assert_eq!((found, text.is_empty()), (library, false), "{id}");
            }
            assert_eq!(tables.text("lion_general").chars().count(), 1905);
            return;
        }
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
        assert_eq!(tables.text("lion_general").chars().count(), 1595);
    }
}
