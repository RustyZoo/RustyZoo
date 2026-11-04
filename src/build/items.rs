//! The items of the lists: which types a panel lists, in which order, with which icons and cost.

use super::*;

/// What the purchase lists filter by: the expansions that are on (a mask) and the selector.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct Filter {
    pub(super) on: i32,
    /// The selector's value, 1 to 3 for an expansion; none for All.
    pub(super) pick: Option<usize>,
}

/// A mask of the installed expansions with the selector on All (the base game is always on).
impl From<i32> for Filter {
    fn from(on: i32) -> Filter {
        Filter {
            on: on | EXPANSIONS[0].1,
            pick: None,
        }
    }
}

/// Whether the type is the DRT base building, the last item of the Hire panel (unit placing 4.2).
pub(crate) fn drt_base(ty: &EntityType) -> bool {
    ty.class == "building" && ty.name_id == HELICOPTER_BASE
}

/// Whether the type is a member of the purchase list `tag` (entity types 10.2).
pub(super) fn listed(ty: &EntityType, tag: &str, filter: Filter) -> bool {
    let Some(members) = ty.config.as_deref().and_then(|c| c.section("Member")) else {
        return false;
    };
    if !members.has(tag) || (tag == "fence" && ty.subtype != b"f") {
        return false;
    }
    if tag == "developer" || members.has("all") {
        return true;
    }
    let own: Vec<usize> = (0..EXPANSIONS.len())
        .filter(|&i| members.has(EXPANSIONS[i].0))
        .collect();
    match filter.pick {
        // On one expansion: its members, and the types of no expansion under the base game.
        Some(v) => own.contains(&(v - 1)) || (own.is_empty() && v == 1),
        None => own.is_empty() || own.iter().any(|&i| filter.on & EXPANSIONS[i].1 != 0),
    }
}

/// The types of the purchase list `tag` that a panel shows, as indices into the catalogue, in the
/// order of the tab: the filtered types in catalogue order, then a stable sort by [`order`]
/// (placing 3.4).
pub(crate) fn items(
    catalogue: &Catalogue,
    avail: &Availability,
    tag: &str,
    filter: impl Into<Filter>,
) -> Vec<usize> {
    items_of(catalogue, avail, tag, filter.into(), |_| true)
}

/// [`items`] keeping the types that satisfy `keep`.
pub(super) fn items_of(
    catalogue: &Catalogue,
    avail: &Availability,
    tag: &str,
    filter: Filter,
    keep: impl Fn(&EntityType) -> bool,
) -> Vec<usize> {
    let shown = |i: usize| {
        let ty = &catalogue.types[i];
        avail.is(catalogue, i) && listed(ty, tag, filter) && keep(ty)
    };
    let mut list: Vec<usize> = (0..catalogue.types.len()).filter(|&i| shown(i)).collect();
    let key = |&i: &usize| order(&catalogue.types[i], tag);
    list.sort_by(|a, b| key(a).partial_cmp(&key(b)).unwrap_or(Ordering::Equal));
    list
}

/// A number of `[Characteristics/Integers]`; fences and tank walls read the section of their
/// subtype after the plain one (entity types 7), and a unit the section of its subtype, then of
/// the default subtype, or the plain one when there is none (entity types 6.2); a guest the one of
/// its default subtype (`man`), whose pass is the only one of the unit level. Never infinite.
pub(crate) fn number(ty: &EntityType, key: &str) -> Option<f32> {
    let config = ty.config.as_deref()?;
    let read = |prefix: &[u8]| {
        config
            .section([prefix, b"Characteristics/Integers"].concat())?
            .get(key)
            .map(to_float)
            .filter(|v| v.is_finite())
    };
    let own = [&ty.subtype[..], b"/"].concat();
    match ty.class {
        "fences" | "tankwall" => read(&own).or_else(|| read(b"")),
        "animals" | "keeper" | "tour" | "maint" => {
            let default = config
                .section("Global")
                .and_then(|g| g.get("DefaultSubtype"));
            let default = default.filter(|d| !d.is_empty());
            read(&own).or_else(|| read(&default.map_or(vec![], |d| [d, b"/"].concat())))
        }
        "guests" => {
            let default = config
                .section("Global")
                .and_then(|g| g.get("DefaultSubtype"));
            let default = default.filter(|d| !d.is_empty());
            default
                .and_then(|d| read(&[d, b"/"].concat()))
                .or_else(|| read(b""))
        }
        _ => read(b""),
    }
}

/// A number of the plain `[Characteristics/Integers]`, whatever the subtype (`cExpansionID`,
/// `cShow`, `cUseNumbersInName`; entity types 6.1).
pub(crate) fn plain(ty: &EntityType, key: &str) -> Option<f32> {
    let section = ty.config.as_deref()?.section("Characteristics/Integers")?;
    Some(to_float(section.get(key)?)).filter(|v| v.is_finite())
}

/// A text of the `[Characteristics/Strings]` of a unit: the section of its subtype, then of the
/// default subtype.
pub(super) fn unit_text(ty: &EntityType, key: &str) -> Option<String> {
    let config = ty.config.as_deref()?;
    let default = config
        .section("Global")
        .and_then(|g| g.get("DefaultSubtype"));
    let prefixes = [ty.subtype.clone(), default.unwrap_or_default().to_vec()];
    prefixes.iter().find_map(|prefix| {
        let name = [prefix, &b"/Characteristics/Strings"[..]].concat();
        let value = config.section(name)?.get(key)?;
        Some(String::from_utf8_lossy(value.trim_ascii()).into_owned())
    })
}

/// `cPurchaseCost` of a type.
pub(crate) fn cost(ty: &EntityType) -> f32 {
    number(ty, "cPurchaseCost").unwrap_or(0.0)
}

/// The sort key of a type in the list of the purchase list `tag`, smallest first (placing 3.4).
pub(super) fn order(ty: &EntityType, tag: &str) -> [f32; 5] {
    let has = |name: &str| {
        ty.config
            .as_deref()
            .is_some_and(|c| c.section(name).is_some())
    };
    match tag {
        // The staff list is sorted by kind, the catalogue order kept within one (unit placing 4.2).
        "staff" => [
            match ty.class {
                "keeper" => 0.0,
                "tour" => 1.0,
                _ => 2.0,
            },
            0.0,
            0.0,
            0.0,
            0.0,
        ],
        "fence" if !matches!(ty.class, "fences" | "tankwall") => [0.0; 5],
        "fence" if has("TankFence") => [
            1.0 + f32::from(number(ty, "cIsShowFence").unwrap_or(0.0) != 0.0),
            cost(ty),
            0.0,
            0.0,
            0.0,
        ],
        "fence" if has("HabitatFence") => {
            [3.0, -number(ty, "cHeight").unwrap_or(0.0), 0.0, 0.0, 0.0]
        }
        "fence" => [4.0, 0.0, 0.0, 0.0, 0.0],
        "animals" => [
            "cHabitat",
            "cLocation",
            "cFamily",
            "cGenus",
            "cPurchaseCost",
        ]
        .map(|key| number(ty, key).unwrap_or(0.0)),
        "foliage" | "rocks" => {
            let habitat = number(ty, "cHabitat").unwrap_or(NO_HABITAT);
            let last = habitat == NO_HABITAT || habitat == BAD_HABITAT;
            [
                f32::from(last),
                habitat,
                number(ty, "cLocation").unwrap_or(NO_HABITAT),
                cost(ty),
                0.0,
            ]
        }
        _ => [cost(ty), 0.0, 0.0, 0.0, 0.0],
    }
}

/// The icons of a type, as the animation names its sections give them (entity types 9.4): the
/// `Icon` values of the first section that has some, in file order.
pub(super) fn icons(ty: &EntityType) -> Vec<String> {
    let Some(config) = ty.config.as_deref() else {
        return Vec::new();
    };
    let subtype = (matches!(ty.class, "fences" | "tankwall") || is_unit(ty))
        .then(|| [&ty.subtype[..], b"/Icon"].concat());
    let default = config
        .section("Global")
        .and_then(|g| g.get("DefaultSubtype"))
        .filter(|d| !d.is_empty() && subtype.is_some())
        .map(|d| [d, b"/Icon"].concat());
    subtype
        .into_iter()
        .chain(default)
        .chain([b"Icon".to_vec()])
        .filter_map(|name| config.section(name))
        .map(|section| section.get_all("Icon"))
        .find(|values| !values.is_empty())
        .map_or_else(Vec::new, |values| {
            values
                .iter()
                .map(|v| crate::vfs::name_from_bytes(v))
                .collect()
        })
}

/// The icon a list button shows at the rotation `step`: that number of the type's icons, or the
/// first when it has no such number (placing 3.4).
pub(super) fn icon_at(names: &[String], step: usize) -> Option<&String> {
    names.get(step).or(names.first())
}

/// The animation of each value of `[Images]` in `ui/infoimg.cfg` (a missing file or a key that is
/// not a number gives nothing for it).
pub(super) fn infoimg(vfs: &Vfs) -> HashMap<i32, String> {
    let config = vfs.read("ui/infoimg.cfg").ok();
    let config = config.and_then(|b| Config::parse(&b).ok());
    let Some(section) = config.as_ref().and_then(|c| c.section("Images")) else {
        eprintln!(
            "rustyzoo: warning: no [Images] in ui/infoimg.cfg: no location and habitat pictures"
        );
        return HashMap::new();
    };
    section
        .keys()
        .filter_map(|key| {
            let name = section.get(key)?.trim_ascii();
            Some((to_int(key)?, String::from_utf8_lossy(name).into_owned()))
        })
        .collect()
}

/// Whether the rotate buttons show for the type: scenery and buildings turn, paths and fences
/// follow the tile and its side, filters do not.
pub(super) fn rotates(ty: &EntityType) -> bool {
    matches!(ty.class, "objects" | "building")
}
