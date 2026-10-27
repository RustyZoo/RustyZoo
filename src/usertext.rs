//! The texts of user-created content (localisation 8): the language section of a definition file
//! that holds them, and the texts the interface shows for the user name ids (entity types 10.3).
//!
//! The texts are not in the libraries. Each user type has a name id (handed out by the catalogue)
//! and the texts of its name, long help and "the" text are asked for by the ids of the name plus
//! 30000 and 55000 (string tables 4.2).

use crate::config::{Config, Section};
use crate::entity::Catalogue;
use crate::text::from_cp1252;
use crate::ui::interface::Interface;
use std::ops::RangeInclusive;

/// The ids of the names of user types: the catalogue hands them out from the first to the last.
pub const NAMES: RangeInclusive<i32> = 19000..=21999;

/// The offset of the long help id from the name id.
const HELP: i32 = 30000;

/// The offset of the "the" text id from the name id.
const THE: i32 = 55000;

/// The language section of a user-created file (localisation 8.1): the first of the sections
/// named by the install language (the text of string 22015, 1033 when empty), by the user's
/// locale (its decimal text, when the host gives one) and by `LCID` of `[defaultLCID]` (1033
/// when the key is missing or empty) that has keys. The names are compared as texts: `01033`
/// names `[01033]`, not `[1033]`. `None` when none has: the type then has no texts.
pub fn section<'a>(config: &'a Config, install: &str, user: Option<&str>) -> Option<&'a Section> {
    let install = if install.is_empty() { "1033" } else { install };
    let lcid = config
        .section("defaultLCID")
        .and_then(|s| s.get("LCID"))
        .filter(|v| !v.is_empty())
        .unwrap_or(&b"1033"[..]);
    [
        Some(install.as_bytes()),
        user.map(str::as_bytes),
        Some(lcid),
    ]
    .into_iter()
    .flatten()
    .filter_map(|name| config.section(name))
    .find(|section| section.keys().next().is_some())
}

/// The texts of one user-created name (localisation 8.2), as the bytes of the file (Windows-1252).
#[derive(Debug, Default, PartialEq)]
pub struct Texts {
    /// `cName`: the name.
    pub name: Vec<u8>,
    /// `cLongHelp`: the long help.
    pub help: Vec<u8>,
    /// `cTheString`: "the name", for sentences.
    pub the: Vec<u8>,
    /// `cGeneralInfoFileName`: the member of the text file of the information panel; `None` when
    /// the key is missing or empty.
    pub info_file: Option<Vec<u8>>,
}

/// The texts of a language section.
pub fn texts(section: &Section) -> Texts {
    let value = |key: &str| section.get(key).unwrap_or_default().to_vec();
    Texts {
        name: value("cName"),
        help: value("cLongHelp"),
        the: value("cTheString"),
        info_file: Some(value("cGeneralInfoFileName")).filter(|v| !v.is_empty()),
    }
}

/// The text of every user name id of the catalogue, in the install language `install` (the user
/// language is not known to the engine, so it is not tried): the name id, the help id and the
/// "the" id of each type named in [`NAMES`], as the interface shows them.
pub fn names(catalogue: &Catalogue, install: &str) -> Vec<(i32, String)> {
    let mut out = Vec::new();
    for ty in catalogue
        .types
        .iter()
        .filter(|t| NAMES.contains(&t.name_id))
    {
        let Some(section) = ty.config.as_deref().and_then(|c| section(c, install, None)) else {
            continue;
        };
        let texts = texts(section);
        let text = |bytes: &[u8]| bytes.iter().map(|&b| from_cp1252(b)).collect::<String>();
        out.push((ty.name_id, text(&texts.name)));
        out.push((ty.name_id + HELP, text(&texts.help)));
        out.push((ty.name_id + THE, text(&texts.the)));
    }
    out
}

/// Shows the texts of the user types of `catalogue` wherever their ids are shown, in the install
/// language that string 22015 of `ui` gives.
pub fn serve(ui: &mut Interface, catalogue: &Catalogue) {
    let install = ui.string(22015);
    for (id, text) in names(catalogue, &install) {
        ui.set_string(id, text);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::entity::{Catalogue, EntityType};
    use std::sync::Arc;

    fn config(text: &str) -> Config {
        Config::parse(text.as_bytes()).unwrap()
    }

    fn name_of(config: &Config, install: &str, user: Option<&str>) -> Option<Vec<u8>> {
        section(config, install, user)?
            .get("cName")
            .map(<[u8]>::to_vec)
    }

    #[test]
    fn the_install_section_with_keys_comes_first() {
        let c = config("[1036]\ncName = fr\n[1033]\ncName = en\n[defaultLCID]\nLCID = 1033\n");
        assert_eq!(
            name_of(&c, "1036", Some("1033")).as_deref(),
            Some(&b"fr"[..])
        );
    }

    #[test]
    fn an_empty_section_gives_way_to_the_user_then_to_lcid() {
        let c =
            config("[1036]\n[1031]\ncName = de\n[1033]\ncName = en\n[defaultLCID]\nLCID = 1033\n");
        assert_eq!(
            name_of(&c, "1036", Some("1031")).as_deref(),
            Some(&b"de"[..])
        );
        assert_eq!(name_of(&c, "1036", None).as_deref(), Some(&b"en"[..]));
        assert_eq!(
            name_of(&c, "1036", Some("1040")).as_deref(),
            Some(&b"en"[..])
        );
    }

    #[test]
    fn lcid_names_the_last_section_and_1033_when_missing() {
        let with = config("[0]\ncName = zero\n[defaultLCID]\nLCID = 0\n");
        assert_eq!(name_of(&with, "1036", None).as_deref(), Some(&b"zero"[..]));
        let missing = config("[1033]\ncName = en\n");
        assert_eq!(name_of(&missing, "1036", None).as_deref(), Some(&b"en"[..]));
    }

    #[test]
    fn an_empty_lcid_is_1033() {
        let empty = config("[1033]\ncName = en\n[defaultLCID]\nLCID =\n");
        assert_eq!(name_of(&empty, "1036", None).as_deref(), Some(&b"en"[..]));
    }

    #[test]
    fn section_names_are_texts_not_numbers() {
        let fr = config("[fr]\ncName = fr\n[01033]\ncName = zeros\n[1033]\ncName = en\n");
        assert_eq!(
            name_of(&fr, "1036", Some("1033")).as_deref(),
            Some(&b"en"[..])
        );
        assert_eq!(
            name_of(&fr, "1036", Some("fr")).as_deref(),
            Some(&b"fr"[..])
        );
        let lcid =
            config("[1033]\ncName = en\n[01033]\ncName = zeros\n[defaultLCID]\nLCID = 01033\n");
        assert_eq!(name_of(&lcid, "1036", None).as_deref(), Some(&b"zeros"[..]));
        let word = config("[fr]\ncName = fr\n[defaultLCID]\nLCID = fr\n");
        assert_eq!(name_of(&word, "1036", None).as_deref(), Some(&b"fr"[..]));
        let hex = config("[0x409]\ncName = hex\n[defaultLCID]\nLCID = 0x409\n");
        assert_eq!(name_of(&hex, "1036", None).as_deref(), Some(&b"hex"[..]));
        // The install text is used as it is stored: "0" names [0], and an empty text names 1033.
        let zero = config("[0]\ncName = zero\n[1033]\ncName = en\n");
        assert_eq!(name_of(&zero, "0", None).as_deref(), Some(&b"zero"[..]));
        assert_eq!(name_of(&zero, "", None).as_deref(), Some(&b"en"[..]));
        // The install text "1033" does not name [01033].
        let none = config("[01033]\ncName = zeros\n");
        assert!(section(&none, "1033", None).is_none());
    }

    #[test]
    fn no_section_with_keys_gives_none() {
        let c = config("[1036]\n[defaultLCID]\nLCID = 1033\n[1033]\n");
        assert!(section(&c, "1036", Some("1033")).is_none());
        assert!(section(&config("[1033]\ncName = en\n"), "1036", None).is_some());
    }

    #[test]
    fn the_texts_are_read_by_their_keys() {
        let c = config(
            "[1033]\ncName = Yeti\ncLongHelp = Long\ncTheString = the yeti\ncGeneralInfoFileName = yeti.txt\n",
        );
        let read = super::texts(section(&c, "1033", None).unwrap());
        assert_eq!(read.name, b"Yeti");
        assert_eq!(read.help, b"Long");
        assert_eq!(read.the, b"the yeti");
        assert_eq!(read.info_file.as_deref(), Some(&b"yeti.txt"[..]));
        let empty = config("[1033]\ncName = Yeti\ncGeneralInfoFileName =\n");
        let read = super::texts(section(&empty, "1033", None).unwrap());
        assert_eq!(read.help, b"");
        assert_eq!(read.info_file, None);
    }

    fn user_type(name_id: i32, text: &str) -> EntityType {
        EntityType {
            class: "animals",
            type_name: b"yeti".to_vec(),
            subtype: b"m".to_vec(),
            file: None,
            config: Some(Arc::new(config(text))),
            name_id,
            footprint: [1, 1, 1],
            height: 0,
        }
    }

    #[test]
    fn names_serve_each_id_in_the_install_language_and_the_fallback() {
        let mut catalogue = Catalogue::default();
        let fr = b"[1036]\ncName = Y\xe9ti\ncLongHelp = Long\ncTheString = le y\xe9ti\n\
                   [1033]\ncName = Yeti\ncLongHelp = Lg\ncTheString = the yeti\n";
        catalogue.types.push(EntityType {
            config: Some(Arc::new(Config::parse(fr).unwrap())),
            ..user_type(19000, "")
        });
        catalogue.types.push(user_type(0, "[1033]\ncName = Nope\n"));
        catalogue
            .types
            .push(user_type(49000, "[1033]\ncName = Nope\n"));
        let served = names(&catalogue, "1036");
        assert_eq!(
            served,
            [
                (19000, "Yéti".to_string()),
                (49000, "Long".to_string()),
                (74000, "le yéti".to_string()),
            ]
        );
        assert_eq!(names(&catalogue, "1031")[0], (19000, "Yeti".to_string()));
    }

    #[test]
    fn a_type_without_a_section_serves_nothing() {
        let mut catalogue = Catalogue::default();
        catalogue.types.push(user_type(19000, "[1036]\n"));
        assert!(names(&catalogue, "1036").is_empty());
    }

    /// The Yeti of the French install has only a French section, so it is French in the French
    /// install and the English one is in the English install (the flag has no French section and
    /// keeps its English text).
    #[test]
    #[ignore = "needs the French install: set ZT_DATA_DIR_FR to its folder"]
    fn the_user_names_of_the_french_install_are_french() {
        // Without the French folder the test has nothing to check.
        let Some(dir) = std::env::var_os("ZT_DATA_DIR_FR") else {
            return;
        };
        let vfs = crate::vfs::Vfs::open(std::path::Path::new(&dir)).unwrap();
        let catalogue = Catalogue::load(&vfs).unwrap();
        let french = names(&catalogue, "1036");
        assert!(french.contains(&(19000, "Yéti".to_string())), "{french:?}");
        assert!(french.contains(&(19001, "United States Flag".to_string())));
    }
}
