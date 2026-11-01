//! The awards of the original game (`awards.md` 3 and 4): the definitions read from the
//! `award*.cfg` files at start, and the list of awards a zoo holds.

use std::collections::BTreeMap;

use crate::config::{Config, to_int};
use crate::text::from_cp1252;
use crate::vfs::Vfs;

/// What the game keeps of an award: the string id of its name, the string id of its help text
/// and the name of its icon animation.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Definition {
    pub name: i32,
    pub tooltip: i32,
    pub icon: String,
}

/// The definitions by award id.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Definitions(BTreeMap<i32, Definition>);

impl Definitions {
    /// The definitions of the `award*.cfg` files of the install, merged by id: the files in
    /// ascending order of their names, a later one replacing an earlier definition of the same id.
    pub fn load(vfs: &Vfs) -> Definitions {
        let mut found = Definitions::default();
        for file in vfs.list(Some("award*.cfg")) {
            let Some(config) = vfs.read(&file).ok().and_then(|b| Config::parse(&b).ok()) else {
                continue;
            };
            let version = config
                .section("Version")
                .and_then(|s| s.get("version"))
                .and_then(to_int);
            if version.is_none_or(|v| v <= 0) {
                continue;
            }
            found.read(&config);
        }
        found
    }

    /// Adds the `[Awards]` entries of a file: each names a section, which gives the id, the name
    /// and the tooltip string ids (both must be non-zero) and the icon. A missing key is 0 or none.
    fn read(&mut self, config: &Config) {
        let names = config
            .section("Awards")
            .map(|s| s.get_all("award").to_vec())
            .unwrap_or_default();
        for name in names {
            let Some(section) = config.section(&name) else {
                continue;
            };
            let number = |key: &str| section.get(key).and_then(to_int).unwrap_or(0);
            let (id, name_id) = (number("id"), number("nameID"));
            if id == 0 || name_id == 0 {
                continue;
            }
            let icon = section
                .get("icon")
                .unwrap_or_default()
                .iter()
                .map(|&b| from_cp1252(b))
                .collect();
            let definition = Definition {
                name: name_id,
                tooltip: number("tooltipID"),
                icon,
            };
            self.0.insert(id, definition);
        }
    }

    /// The definition of an award, if the install has one.
    pub fn get(&self, id: i32) -> Option<&Definition> {
        self.0.get(&id)
    }

    /// A set of definitions made in a test.
    #[cfg(test)]
    pub fn of(list: &[(i32, Definition)]) -> Definitions {
        Definitions(list.iter().cloned().collect())
    }

    /// The definitions in ascending order of id.
    pub fn iter(&self) -> impl Iterator<Item = (i32, &Definition)> {
        self.0.iter().map(|(&id, d)| (id, d))
    }
}

/// Gives an award to a held list (`awards.md` 4.1): nothing when the zoo has it already, else the
/// id is kept in the list, which stays largest first. The id needs no definition.
pub fn add(held: &mut Vec<i32>, id: i32) {
    if let Err(at) = held.binary_search_by(|h| id.cmp(h)) {
        held.insert(at, id);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn held(ids: &[i32]) -> Vec<i32> {
        let mut list = Vec::new();
        for &id in ids {
            add(&mut list, id);
        }
        list
    }

    #[test]
    fn the_held_list_is_unique_and_largest_first() {
        assert_eq!(held(&[1, 8]), [8, 1]);
        assert_eq!(held(&[8, 1, 8, 1, 4]), [8, 4, 1]);
        assert_eq!(held(&[]), Vec::<i32>::new());
    }

    #[test]
    fn zero_negative_and_huge_ids_are_kept_as_signed_numbers() {
        assert_eq!(
            held(&[0, -1, i32::MAX, i32::MIN]),
            [i32::MAX, 0, -1, i32::MIN]
        );
        assert_eq!(held(&[-1, -1]), [-1]);
    }

    /// The folder of an install, removed when dropped.
    struct Dir(std::path::PathBuf);

    impl Drop for Dir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    /// An install with the given loose files and a `zoo.ini`.
    fn install(name: &str, files: &[(&str, &str)]) -> (Vfs, Dir) {
        let dir =
            std::env::temp_dir().join(format!("rustyzoo-award-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("zoo.ini"), "[resource]\npath=.\n").unwrap();
        for (path, text) in files {
            let path = dir.join(path);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(path, text).unwrap();
        }
        (Vfs::open(&dir).unwrap(), Dir(dir))
    }

    const ONE: &str = "[Version]\nversion = 1\n[Awards]\naward = 13\naward = 14\n\
        [13]\nid = 13\nnameID = 29013\ntooltipID = 29113\nicon = awards/award13/award13\n\
        [14]\nid = 14\nnameID = 0\ntooltipID = 29114\n";

    #[test]
    fn definitions_are_read_and_merged_by_id() {
        let (vfs, _dir) = install(
            "merge",
            &[
                ("awards/award001.cfg", ONE),
                (
                    "awards/award002.cfg",
                    "[Version]\nversion = 1\n[Awards]\naward = a\n[a]\nid = 13\nnameID = 5\n",
                ),
                (
                    "awards/awards.cfg",
                    "[Awards]\naward = b\n[b]\nid = 2\nnameID = 3\n",
                ),
            ],
        );
        let found = Definitions::load(&vfs);
        assert_eq!(found.iter().count(), 1, "{found:?}");
        assert_eq!(
            found.get(13),
            Some(&Definition {
                name: 5,
                tooltip: 0,
                icon: String::new()
            })
        );
        assert_eq!(found.get(14), None, "a zero name id is no definition");
    }

    #[test]
    fn a_file_without_a_version_is_ignored() {
        let text = "[Awards]\naward = 1\n[1]\nid = 1\nnameID = 2\n";
        let (vfs, _dir) = install(
            "version",
            &[
                ("awards/award001.cfg", text),
                (
                    "awards/award002.cfg",
                    "[Version]\nversion = 0\n[Awards]\naward = 1\n[1]\nid = 1\nnameID = 2\n",
                ),
            ],
        );
        assert_eq!(Definitions::load(&vfs), Definitions::default());
    }

    #[test]
    fn the_icon_and_the_tooltip_are_read() {
        let (vfs, _dir) = install("icon", &[("awards/award001.cfg", ONE)]);
        let found = Definitions::load(&vfs);
        assert_eq!(
            found.get(13),
            Some(&Definition {
                name: 29013,
                tooltip: 29113,
                icon: "awards/award13/award13".into()
            })
        );
    }

    /// Every award file of the game: 31 definitions, 17 included (its definition exists; it can
    /// never be won, `awards.md` 6.4).
    #[test]
    #[ignore = "needs the game: set ZT_DATA_DIR to the install folder"]
    fn full_game_definitions() {
        let dir = std::env::var_os("ZT_DATA_DIR").expect("ZT_DATA_DIR is not set");
        let vfs = Vfs::open(std::path::Path::new(&dir)).unwrap();
        let found = Definitions::load(&vfs);
        assert_eq!(
            found.iter().map(|(id, _)| id).collect::<Vec<_>>(),
            (1..=31).collect::<Vec<_>>()
        );
        assert_eq!(
            found.get(21).map(|d| d.icon.clone()),
            found.get(1).map(|d| d.icon.clone())
        );
    }
}
