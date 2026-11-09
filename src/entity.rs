//! The catalogue of entity types: what each object placed on a map is, and which animation
//! files draw it.
//!
//! The original builds the catalogue from registries, not from the definition files. For each
//! class, in a fixed order, it lists the configuration resources whose name starts with the
//! class prefix and ends in `.cfg`, in ascending byte order, and reads in each a section that
//! names the types and their definition files. A definition file that no registry names is
//! never read. Entries are processed in ascending byte order of their keys. A type is identified
//! by three names: the class, the type name and the subtype name, as a map file stores them.
//!
//! Only what drawing a static map needs is kept per type: the identity, the name id, the
//! footprint and height, and the configuration of the definition file for later use (shared
//! between the types that name the same file). The animation table is resolved on demand.
//! Names are bytes, compared byte for byte, like in the configuration reader.
//!
//! A damaged or oversized file never stops the catalogue: a registry that cannot be read is
//! skipped, a type whose definition file cannot be used keeps the defaults, each with a warning.
//! Only a failure of the operating system does. The catalogue is also bounded on hostile input:
//! the number of entries, the total size of the definition files kept and the number of
//! warnings have a limit.

mod catalogue;
pub use catalogue::Catalogue;
pub(crate) use catalogue::is_of_class;

/// The type of an entity: the first type of its class with its names, `None` when there is none.
/// A name outside ASCII is looked up by its low bytes (one byte per character).
pub(crate) fn type_of<'a>(cat: &'a Catalogue, e: &crate::zoo::Entity) -> Option<&'a EntityType> {
    fn bytes(s: &str) -> Cow<'_, [u8]> {
        match s.is_ascii() {
            true => Cow::Borrowed(s.as_bytes()),
            false => Cow::Owned(s.chars().map(|c| c as u8).collect()),
        }
    }
    cat.find(&e.class, &bytes(&e.type_name), &bytes(&e.subtype))
}
#[cfg(test)]
mod tests;

use std::borrow::Cow;
use std::error::Error;
use std::fmt;
use std::iter;
use std::sync::Arc;

use crate::config::{Config, Section, to_int};
use crate::vfs::normalise;

/// The most registry entries (types created or listed again) one catalogue takes. The full game
/// has 980 types; the limit only stops a registry made to multiply names.
pub(crate) const MAX_ENTRIES: usize = 100_000;

/// The most bytes of definition files kept. The full game has about 3 MB of them; the memory of a
/// parsed file is a multiple of its size.
pub(crate) const MAX_DEFINITION_BYTES: usize = 8 * 1024 * 1024;

/// Why the catalogue could not be built: the operating system failed to read a file. The
/// message is a single line meant to be shown to the user.
#[derive(Debug)]
pub struct EntityError(String);

impl fmt::Display for EntityError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl Error for EntityError {}

/// One animation of a type: its name in the animation section and the `.ani` member that
/// holds it (a normalised path; the member may not exist).
#[derive(Debug, PartialEq, Eq)]
pub struct Animation {
    pub name: String,
    /// `None` if the key has no value: the original does not resolve it.
    pub member: Option<String>,
}

/// One entity type.
#[derive(Debug)]
pub struct EntityType {
    /// The class the type belongs to, as a map file stores it: `animals`, `keeper`, `building`,
    /// `objects`, `fences`, `tankwall`, `paths`, `food`, `rubble`, `filter`, `ambient`, ...
    pub class: &'static str,
    pub type_name: Vec<u8>,
    pub subtype: Vec<u8>,
    /// The definition file the registry names (normalised), if it names one.
    pub file: Option<String>,
    /// The definition file as read, shared with the other types that name it; `None` if there is
    /// no file, it cannot be used or it holds no section (the original then keeps the program's
    /// defaults and no animations).
    pub config: Option<Arc<Config>>,
    /// `cNameID`, a string table id; 0 if the file does not give one.
    pub name_id: i32,
    /// `cFootprintX/Y/Z` in half tiles (a tile is 2 x 2); 1, 1, 1 by default.
    pub footprint: [i32; 3],
    /// `cHeight`; 0 by default.
    pub height: i32,
}

impl EntityType {
    /// The egg and the crate footprints of an animal type, in that order (see
    /// [`EntityType::form_footprint`]).
    pub(crate) fn forms(&self) -> [(i32, i32); 2] {
        [true, false].map(|egg| {
            let [x, y] = self.form_footprint(egg);
            (x, y)
        })
    }

    /// The footprint `(x, y)` in half tiles of an animal in its egg (`cEggFootprintX/Y`) or in its
    /// crate (`cBoxFootprintX/Y`), read like the plain footprint; 2 by 2 when the file gives none.
    pub(crate) fn form_footprint(&self, egg: bool) -> [i32; 2] {
        let (x, y) = match egg {
            true => ("cEggFootprintX", "cEggFootprintY"),
            false => ("cBoxFootprintX", "cBoxFootprintY"),
        };
        let mut out = [2, 2];
        if let Some(config) = self.config.as_deref() {
            for prefix in unit_prefixes(self, config) {
                let section = config.section([&prefix[..], b"Characteristics/Integers"].concat());
                if let Some(section) = section {
                    set(section, x, &mut out[0]);
                    set(section, y, &mut out[1]);
                }
            }
        }
        out
    }

    /// The number of palettes of each replaceable colour group of a staff or guest type, in the
    /// order of its `colorrep` section (sprite drawing 8.1; the section of the subtype, or of the
    /// type name for a guest); empty for a type with none.
    pub(crate) fn palettes(&self) -> Vec<usize> {
        let Some(config) = self.config.as_deref() else {
            return Vec::new();
        };
        let section = |prefix: &[u8]| config.section([prefix, b"/colorrep"].concat());
        let Some(replaced) = section(&self.subtype).or_else(|| section(&self.type_name)) else {
            return Vec::new();
        };
        let count = |name: &Vec<u8>| config.section(name).map_or(0, |g| g.get_all("pal").len());
        replaced.get_all("replace").iter().map(count).collect()
    }

    /// The animation section of the type (specification 9.1): the first candidate that exists
    /// with at least one key.
    fn animation_section(&self) -> Option<&Section> {
        let config = self.config.as_deref()?;
        let default = default_subtype(config);
        // The subtype sections come first; the other classes, the helicopter included, have only
        // the plain section.
        let mut prefixes: Vec<&[u8]> = Vec::new();
        match self.class {
            "guests" => prefixes.push(&self.type_name),
            "animals" | "keeper" | "maint" | "tour" | "fences" | "tankwall" => {
                prefixes.push(&self.subtype)
            }
            _ => {}
        }
        if !prefixes.is_empty() && !default.is_empty() {
            prefixes.push(default);
        }
        prefixes
            .iter()
            .map(|prefix| [prefix, &b"/Animations"[..]].concat())
            .chain(iter::once(b"Animations".to_vec()))
            .filter_map(|name| config.section(name))
            .find(|section| section.keys().next().is_some())
    }

    /// The animation `name` of `section` (specification 9.2).
    fn animation(&self, section: &Section, name: &[u8]) -> Animation {
        // A key without value does not resolve.
        let member = section
            .get(name)
            .filter(|value| !value.is_empty())
            .map(|value| {
                // A value with `.ani` is the member as it stands.
                let member = if value.windows(4).any(|w| w == b".ani") {
                    value.to_vec()
                } else {
                    let root = self.config.as_deref().map(|c| animation_root(self, c));
                    [&root.unwrap_or_default()[..], value, b"/", value, b".ani"].concat()
                };
                normalise(&lossy(&member))
            });
        Animation {
            name: lossy(name),
            member,
        }
    }

    /// The member a `[Sounds]` value names (behaviours 5.2): a value with `.wav` is the path as
    /// written; any other is `<value>.wav` in `<class folder>/<type name>/` for an animal or a
    /// guest and in the animation root of the other classes.
    pub(crate) fn sound_member(&self, value: &[u8]) -> String {
        let member = if value.windows(4).any(|w| w == b".wav") {
            value.to_vec()
        } else {
            let root = match self.class {
                "animals" | "guests" => {
                    [self.class.as_bytes(), b"/", &self.type_name, b"/"].concat()
                }
                _ => self
                    .config
                    .as_deref()
                    .map_or_else(Vec::new, |c| animation_root(self, c)),
            };
            [&root[..], value, b".wav"].concat()
        };
        normalise(&lossy(&member))
    }

    /// The animations of the type, by name in ascending byte order; empty if it has no
    /// animation section.
    pub(crate) fn animations(&self) -> Vec<Animation> {
        self.animation_section().map_or_else(Vec::new, |section| {
            section
                .keys()
                .map(|name| self.animation(section, name))
                .collect()
        })
    }

    /// The first of [`EntityType::animations`] named `name`, without making the others.
    pub(crate) fn animation_named(&self, name: &str) -> Option<Animation> {
        let section = self.animation_section()?;
        let key = section
            .keys()
            .find(|key| crate::vfs::name_from_bytes(key) == name)?;
        Some(self.animation(section, key))
    }

    /// The animation a still picture of the type shows. Its member is `None` if the key has no
    /// value.
    ///
    /// ponytail: the original takes the first name in byte order. For fences and tank walls the
    /// state decides (`idle` is the intact one); for food, rubble and filters, whose entity code
    /// chooses the state, this takes `idle`, else `full`, until that code is specified (open
    /// point O7 of the specification).
    pub fn base_animation(&self) -> Option<Animation> {
        let section = self.animation_section()?;
        let named = |name: &'static str| section.has(name).then_some(name.as_bytes());
        let name = match self.class {
            "fences" | "tankwall" => named("idle"),
            "food" | "rubble" | "filter" => named("idle").or(named("full")),
            _ => None,
        }
        .or_else(|| section.keys().next())?;
        Some(self.animation(section, name))
    }
}

/// Replaces `target` by the integer value of `key` if the section has one.
fn set(section: &Section, key: &str, target: &mut i32) {
    if let Some(value) = section.get(key).and_then(to_int) {
        *target = value;
    }
}

/// Reads the name id, footprint and height from `[<prefix>Characteristics/Integers]`.
fn read_keys(ty: &mut EntityType, config: &Config, prefix: &[u8]) {
    let Some(section) = config.section([prefix, b"Characteristics/Integers"].concat()) else {
        return;
    };
    set(section, "cNameID", &mut ty.name_id);
    set(section, "cFootprintX", &mut ty.footprint[0]);
    set(section, "cFootprintY", &mut ty.footprint[1]);
    set(section, "cFootprintZ", &mut ty.footprint[2]);
    set(section, "cHeight", &mut ty.height);
}

pub(crate) fn default_subtype(config: &Config) -> &[u8] {
    config
        .section("Global")
        .and_then(|global| global.get("DefaultSubtype"))
        .unwrap_or_default()
}

/// The prefixes of the sections a unit type reads its integers from, in the order they apply:
/// the default subtype's (the plain ones when there is none), then the type's own, unless it is
/// the default. With an empty own name and a default, the prefix is `/`, which matches nothing.
pub(crate) fn unit_prefixes(ty: &EntityType, config: &Config) -> Vec<Vec<u8>> {
    let default = default_subtype(config);
    let own = if ty.class == "guests" {
        &ty.type_name
    } else {
        &ty.subtype
    };
    let first = if default.is_empty() {
        Vec::new()
    } else {
        [default, b"/"].concat()
    };
    let mut prefixes = vec![first];
    if own[..] != *default {
        prefixes.push([&own[..], b"/"].concat());
    }
    prefixes
}

/// Reads the values of a type by the rules of its class (specification 6).
fn read_values(ty: &mut EntityType, config: &Config) {
    match ty.class {
        // Units: the name id, footprint and height belong to the ZT unit level, which reads the
        // default subtype's sections (the plain ones when there is no default subtype, and never
        // as a fallback), then the type's own on top. Guests use their type name where the
        // others use the subtype name.
        "animals" | "guests" | "keeper" | "tour" | "maint" | "helicopter" => {
            for prefix in unit_prefixes(ty, config) {
                read_keys(ty, config, &prefix);
            }
        }
        // The scenery keys from the plain sections, then name id and height again from the
        // sections of the type's subtype.
        "fences" | "tankwall" => {
            read_keys(ty, config, b"");
            let own = [&ty.subtype, &b"/"[..]].concat();
            if let Some(section) = config.section([&own[..], b"Characteristics/Integers"].concat())
            {
                set(section, "cNameID", &mut ty.name_id);
                set(section, "cHeight", &mut ty.height);
            }
        }
        // Scenery, buildings, paths, food, rubble, filters, ambient: plain sections only.
        _ => read_keys(ty, config, b""),
    }
}

fn lossy(bytes: &[u8]) -> String {
    crate::vfs::name_from_bytes(bytes)
}

/// The folder an animation value is relative to, ending in `/` (empty if there is none).
fn animation_root(ty: &EntityType, config: &Config) -> Vec<u8> {
    let (kind, subtype) = (&ty.type_name[..], &ty.subtype[..]);
    let joined = |parts: &[&[u8]]| {
        let mut root = Vec::new();
        for part in parts.iter().filter(|part| !part.is_empty()) {
            root.extend_from_slice(part);
            root.push(b'/');
        }
        root
    };
    match ty.class {
        "objects" | "building" => joined(&[b"objects", subtype]),
        "paths" | "food" | "rubble" | "filter" => joined(&[ty.class.as_bytes(), subtype]),
        "fences" | "tankwall" => joined(&[ty.class.as_bytes(), kind, subtype]),
        "guests" => joined(&[b"guests", kind]),
        // An animal whose subtype has no `[AnimPath]` entry has an empty root.
        "animals" => config
            .section("AnimPath")
            .and_then(|paths| paths.get(subtype))
            .map_or_else(Vec::new, |path| joined(&[path])),
        "ambient" | "helicopter" => joined(&[ty.class.as_bytes(), kind, subtype]),
        _ => joined(&[b"staff", kind, subtype]),
    }
}
