//! What the rating reads of an animal type's file.

use super::*;

/// What the rating reads of an animal type's file.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct Likes {
    /// The species key: the type name. Records are kept per key; the list per key and subtype.
    pub key: String,
    pub subtype: String,
    /// The type has an ambient sound (`cAmbients`).
    pub ambient: bool,
    /// `cCompatibleTerrain`: the wanted percentage of each terrain type, 0 when absent.
    pub terrain: [i32; TERRAINS],
    /// `cSuitableObjects`: pairs of an id (habitat number or name id) and a value.
    pub objects: Vec<(i32, i32)>,
    pub trees: i32,
    pub rocks: i32,
    pub elevation: i32,
    pub shelter: bool,
    pub toys: bool,
    /// `cDepthMin`, `cDepthMax`.
    pub depth: (i32, i32),
    /// `cUnderwater` 1 and `cOnlyUnderwater` 0 (tank fauna 2).
    pub amphibious: bool,
    pub attractiveness: i32,
    /// `cNameID`, `cGenus` and `cFamily`, and `cCompatibleAnimals`: pairs of such an id and a
    /// value.
    pub ids: [i32; 3],
    pub compatible: Vec<(i32, i32)>,
    /// `cBashStrength` above 0 (habitat rating 12).
    pub bash: bool,
    /// Found on a show neighbour of an amphibious neighbour: a record, never in the surrounding
    /// list (5.1 point 4, 9.2).
    pub via: bool,
}

/// The value of an id in a list of pairs: the last pair that names it, 0 when none does.
pub(super) fn lookup(pairs: &[(i32, i32)], id: i32) -> i64 {
    (pairs.iter().rev())
        .find(|p| p.0 == id)
        .map_or(0, |p| i64::from(p.1))
}

impl Likes {
    pub(super) fn value(&self, id: i32) -> i64 {
        lookup(&self.objects, id)
    }

    /// Whether salt water suits the species (`isRightSalinity`, tank fauna 11): it wants salt
    /// water (terrain 10) at least as much as fresh water (terrain 9).
    pub(crate) fn salt(&self) -> bool {
        self.terrain[SALT] >= self.terrain[FRESH]
    }

    /// The likes of an animal type. An integer is looked up in the sections of the subtype, of
    /// the default subtype, then in the plain ones (entity types 6.2); the lists are plain.
    pub(crate) fn of(ty: &EntityType) -> Likes {
        let Some(config) = ty.config.as_deref() else {
            return Likes::default();
        };
        let text = |b: &[u8]| String::from_utf8_lossy(b).into_owned();
        let default = config
            .section("Global")
            .and_then(|g| g.get("DefaultSubtype"))
            .unwrap_or_default();
        let int = |key: &str, fallback: i32| {
            [&ty.subtype[..], default, b""]
                .into_iter()
                .find_map(|prefix| {
                    let name = if prefix.is_empty() {
                        b"Characteristics/Integers".to_vec()
                    } else {
                        [prefix, b"/Characteristics/Integers"].concat()
                    };
                    config.section(name)?.get(key).and_then(to_int)
                })
                .unwrap_or(fallback)
        };
        let pairs = |list: &str| -> Vec<(i32, i32)> {
            let values = config.section(list).map_or(&[][..], |s| s.get_all("v"));
            let numbers: Vec<i32> = values.iter().map(|v| to_int(v).unwrap_or(0)).collect();
            numbers
                .as_chunks::<2>()
                .0
                .iter()
                .map(|&[k, v]| (k, v))
                .collect()
        };
        let mut terrain = [0; TERRAINS];
        for (k, w) in pairs("cCompatibleTerrain") {
            if let Some(slot) = usize::try_from(k).ok().and_then(|k| terrain.get_mut(k)) {
                *slot = w;
            }
        }
        let ambient = [&ty.subtype[..], default, b""].into_iter().any(|prefix| {
            let name = [
                prefix,
                if prefix.is_empty() { b"" } else { b"/" },
                b"Characteristics/Strings",
            ]
            .concat();
            config
                .section(name)
                .and_then(|s| s.get("cAmbients"))
                .is_some_and(|v| !v.is_empty())
        });
        Likes {
            key: text(&ty.type_name),
            subtype: text(&ty.subtype),
            ambient,
            terrain,
            objects: pairs("cSuitableObjects"),
            trees: int("cTreePref", 0),
            rocks: int("cRockPref", 0),
            elevation: int("cElevationPref", 0),
            shelter: int("cNeedShelter", 0) == 1,
            toys: int("cNeedToys", 0) == 1,
            depth: (int("cDepthMin", 0), int("cDepthMax", 0)),
            amphibious: int("cUnderwater", 0) == 1 && int("cOnlyUnderwater", 0) == 0,
            attractiveness: int("cAttractiveness", 1),
            ids: [ty.name_id, int("cGenus", 0), int("cFamily", 0)],
            compatible: pairs("cCompatibleAnimals"),
            bash: int("cBashStrength", 0) > 0,
            via: false,
        }
    }
}
