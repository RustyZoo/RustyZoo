//! The item types guests buy (guests 9.1): a name, the change record, and how it is consumed.

use crate::config::{Config, to_int};
use crate::vfs::Vfs;
use std::collections::BTreeMap;

/// An item type.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Item {
    /// The name id, which a guest holds in its slot.
    pub id: i32,
    /// The change record: adult happiness, child happiness, hunger, thirst, bathroom, energy.
    pub record: [i32; 6],
    /// `consumenow`, `trashwhenconsumed`, `removewhenconsumed`.
    pub now: bool,
    pub litter: bool,
    pub remove: bool,
    /// `consumedThoughtID`, 0 for none.
    pub thought: i32,
    /// The picture of the item in the guest panel (`icon`, no extension); empty for none.
    pub icon: String,
}

/// The litter item's name id.
pub const LITTER: i32 = 6310;
/// The slots of a guest's items: food, drink, fun gift, gift and litter.
pub const SLOTS: usize = 5;

impl Item {
    fn read(file: &Config) -> Option<Item> {
        let s = file.section("characteristics")?;
        let int = |key: &str| s.get(key).and_then(to_int).unwrap_or(0);
        Some(Item {
            id: int("nameID"),
            record: [
                "adulthappydelta",
                "childhappydelta",
                "hungerdelta",
                "thirstdelta",
                "bathroomdelta",
                "energydelta",
            ]
            .map(int),
            now: int("consumenow") == 1,
            litter: int("trashwhenconsumed") == 1,
            remove: int("removewhenconsumed") == 1,
            thought: int("consumedThoughtID"),
            icon: s
                .get("icon")
                .map(|v| String::from_utf8_lossy(v).replace('\\', "/"))
                .unwrap_or_default(),
        })
    }
}

/// The item types of the registries `items*.cfg` (a name per file), by lower-case name, and the
/// warnings of the files that could not be used.
pub fn load(vfs: &Vfs) -> (BTreeMap<String, Item>, Vec<String>) {
    let (mut items, mut warnings) = (BTreeMap::new(), Vec::new());
    let read = |name: &str| {
        let bytes = vfs.read(name).map_err(|e| format!("{name}: {e}"))?;
        Config::parse(&bytes).map_err(|e| format!("{name}: {e}"))
    };
    for registry in vfs
        .list(Some("items*.cfg"))
        .iter()
        .filter(|n| !n.contains('/'))
    {
        let Some(list) = read(registry).map_err(|e| warnings.push(e)).ok() else {
            continue;
        };
        let section = list.section("items");
        for name in section.into_iter().flat_map(|s| s.keys()) {
            let file = section.and_then(|s| s.get(name)).unwrap_or_default();
            let file = String::from_utf8_lossy(file).replace('\\', "/");
            match read(&file) {
                Ok(c) => {
                    let name = String::from_utf8_lossy(name).to_ascii_lowercase();
                    items.extend(Item::read(&c).map(|i| (name, i)));
                }
                Err(e) => warnings.push(e),
            }
        }
    }
    (items, warnings)
}
