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
//! footprint and height, the animation table, and the raw configuration of the definition file
//! for later use. Names are bytes, compared byte for byte, like in the configuration reader.

use std::error::Error;
use std::fmt;

use crate::config::{Config, Section, to_int};
use crate::vfs::{Vfs, normalise};

/// Why the catalogue could not be built: a registry or a definition file that exists but cannot
/// be read or is too large. The message is a single line meant to be shown to the user.
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
    pub member: String,
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
    /// The definition file as read; `None` if there is no file, it does not exist or it holds
    /// no section (the original then keeps the program's defaults and no animations).
    pub config: Option<Config>,
    /// `cNameID`, a string table id; 0 if the file does not give one.
    pub name_id: i32,
    /// `cFootprintX/Y/Z` in half tiles (a tile is 2 x 2); 1, 1, 1 by default.
    pub footprint: [i32; 3],
    /// `cHeight`; 0 by default.
    pub height: i32,
    /// The animation table, by animation name in ascending byte order.
    pub animations: Vec<Animation>,
}

impl EntityType {
    /// The animation a still picture of the type shows.
    ///
    /// ponytail: the original takes the first name in byte order. For fences and tank walls the
    /// state decides (`idle` is the intact one); for food, rubble and filters, whose entity code
    /// chooses the state, this takes `idle`, else `full`, until that code is specified (open
    /// point O7 of the specification).
    pub fn base_animation(&self) -> Option<&Animation> {
        let named = |name: &str| self.animations.iter().find(|a| a.name == name);
        let first = self.animations.first();
        match self.class {
            "fences" | "tankwall" => named("idle").or(first),
            "food" | "rubble" | "filter" => named("idle").or(named("full")).or(first),
            _ => first,
        }
    }
}

/// All types, in creation order, and what was odd on the way.
#[derive(Debug, Default)]
pub struct Catalogue {
    pub types: Vec<EntityType>,
    /// One line each, for things the original does silently or by accident: an entry without
    /// a file, a missing definition file, a type listed again with another file, a staff entry
    /// of an unknown kind. Names from the game files are escaped.
    pub warnings: Vec<String>,
}

/// How a registry section turns into types (specification 3.3).
enum Shape {
    /// `name = file`: the type name is the class name, the subtype name is the key.
    Flat,
    /// `name = file`: the type name is the key, the subtype name is empty.
    FlatGuests,
    /// `name = file` plus subtypes. `shared`: a `[subtypes]` section of the registry applies to
    /// every type. `default` is the subtype name when none is listed.
    Subtypes {
        shared: bool,
        default: &'static [u8],
    },
    /// As `Subtypes` with `[<name>/subtypes]` only; the class is chosen by the key.
    Staff,
    /// The section lists section names, each of which holds `name = file`: the type name is
    /// the section, the subtype name the key.
    TwoLevel,
}

struct Class {
    name: &'static str,
    prefix: &'static str,
    section: &'static str,
    shape: Shape,
    /// The extension of user-made files, which are found after the registries.
    user_extension: Option<&'static str>,
}

/// The registry readers in the order of the original. (Items are not entity types.)
const CLASSES: [Class; 12] = [
    class("food", "food", "food", Shape::Flat, None),
    class("paths", "paths", "paths", Shape::Flat, None),
    class("fences", "fences", "fences", subtypes(b"f"), None),
    class("building", "bldg", "building", Shape::Flat, Some(".ucb")),
    class("animals", "animal", "animals", subtypes(b""), Some(".uca")),
    class("guests", "guests", "guests", Shape::FlatGuests, None),
    class(
        "objects",
        "scener",
        "objects",
        Shape::TwoLevel,
        Some(".ucs"),
    ),
    class("staff", "staff", "staff", Shape::Staff, None),
    class("ambient", "ambient", "ambient", Shape::Flat, None),
    class("rubble", "rubble", "rubble", Shape::TwoLevel, None),
    class("tankwall", "twall", "tankwall", subtypes(b"f"), None),
    class("filter", "filter", "filter", Shape::TwoLevel, None),
];

const fn class(
    name: &'static str,
    prefix: &'static str,
    section: &'static str,
    shape: Shape,
    user_extension: Option<&'static str>,
) -> Class {
    Class {
        name,
        prefix,
        section,
        shape,
        user_extension,
    }
}

const fn subtypes(default: &'static [u8]) -> Shape {
    Shape::Subtypes {
        shared: true,
        default,
    }
}

/// What a registry produced for one type.
struct Entry {
    class: &'static str,
    type_name: Vec<u8>,
    subtype: Vec<u8>,
    file: Option<Vec<u8>>,
}

/// The class a staff registry key stands for; any other key is ignored.
fn staff_class(key: &[u8]) -> Option<&'static str> {
    match key {
        b"keeper" | b"scient" | b"trainer" => Some("keeper"),
        b"tour" => Some("tour"),
        b"maint" => Some("maint"),
        b"helicopter" => Some("helicopter"),
        _ => None,
    }
}

/// The class a class descends from in the program's fixed tree. The ZT unit level has no name
/// of its own and answers to `unit`.
fn parent(class: &str) -> Option<&'static str> {
    match class {
        "building" | "fences" | "paths" | "food" | "rubble" | "filter" => Some("objects"),
        "tankwall" => Some("fences"),
        "animals" | "guests" | "staff" => Some("unit"),
        "keeper" | "tour" | "maint" | "helicopter" => Some("staff"),
        "objects" | "unit" | "ambient" => Some("entity"),
        _ => None,
    }
}

/// Whether a type of class `own` is of class `asked`: the class itself or any class above it.
fn is_of_class(own: &str, asked: &str) -> bool {
    let mut class = Some(own);
    while let Some(c) = class {
        if c == asked {
            return true;
        }
        class = parent(c);
    }
    false
}

/// Names from game files, escaped for a message.
fn show(bytes: &[u8]) -> String {
    bytes.escape_ascii().to_string()
}

fn lossy(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes).into_owned()
}

impl Catalogue {
    /// Builds the catalogue of the installation behind `vfs` the way the original does.
    ///
    /// Fails if a registry or a definition file exists but cannot be read or is too large.
    /// Everything else that would stop the original from loading something (a registry or a
    /// definition file without a section, a definition file that does not exist) leaves the
    /// type without a configuration, or skips the registry, as the original does.
    pub fn load(vfs: &Vfs) -> Result<Catalogue, EntityError> {
        let mut catalogue = Catalogue::default();
        for class in &CLASSES {
            let mut loaded_any = false;
            // `*` also matches `/`: the test is on the whole name, so `items/x.cfg` matches `items`.
            for name in vfs.list(Some(&format!("{}*.cfg", class.prefix))) {
                let registry = read_config(vfs, &name)?;
                if registry.is_empty() {
                    continue;
                }
                loaded_any = true;
                for entry in catalogue.entries(class, &registry) {
                    catalogue.add(vfs, entry)?;
                }
            }
            if let (true, Some(extension)) = (loaded_any, class.user_extension) {
                for name in vfs.list(Some(&format!("*{extension}"))) {
                    // The stem: the name without its extension and without the directories.
                    let stem = &name[..name.len() - extension.len()];
                    let stem = stem.rsplit('/').next().unwrap_or_default();
                    if stem.is_empty() {
                        continue;
                    }
                    // Names of the types a user file creates: (type name, subtype name).
                    let names: Vec<(&[u8], &[u8])> = match class.name {
                        "animals" => [&b"m"[..], b"f", b"y"]
                            .iter()
                            .map(|&subtype| (stem.as_bytes(), subtype))
                            .collect(),
                        "building" => vec![(b"building", stem.as_bytes())],
                        _ => vec![(b"other", stem.as_bytes())],
                    };
                    for (type_name, subtype) in names {
                        catalogue.add(
                            vfs,
                            Entry {
                                class: class.name,
                                type_name: type_name.to_vec(),
                                subtype: subtype.to_vec(),
                                file: Some(name.clone().into_bytes()),
                            },
                        )?;
                    }
                }
            }
        }
        Ok(catalogue)
    }

    /// The first type, in creation order, of class `class` (or of a class below it, so that
    /// `staff` finds a keeper) with exactly these names.
    pub fn find(&self, class: &str, type_name: &[u8], subtype: &[u8]) -> Option<&EntityType> {
        self.types.iter().find(|t| {
            is_of_class(t.class, class) && t.type_name == type_name && t.subtype == subtype
        })
    }

    /// The number of types per registry class, in the order of the original (all staff
    /// classes together).
    pub fn counts(&self) -> Vec<(&'static str, usize)> {
        CLASSES
            .iter()
            .map(|class| {
                let count = self
                    .types
                    .iter()
                    .filter(|t| match class.name {
                        "staff" => is_of_class(t.class, "staff"),
                        name => t.class == name,
                    })
                    .count();
                (class.name, count)
            })
            .collect()
    }

    /// The types a registry produces, entries in ascending byte order of their keys.
    fn entries(&mut self, class: &Class, registry: &Config) -> Vec<Entry> {
        let Some(section) = registry.section(class.section) else {
            return Vec::new();
        };
        let file_of = |section: &Section, key: &[u8]| section.get(key).map(<[u8]>::to_vec);
        let mut entries = Vec::new();
        match class.shape {
            Shape::Flat => entries.extend(section.keys().map(|key| Entry {
                class: class.name,
                type_name: class.name.as_bytes().to_vec(),
                subtype: key.to_vec(),
                file: file_of(section, key),
            })),
            Shape::FlatGuests => entries.extend(section.keys().map(|key| Entry {
                class: class.name,
                type_name: key.to_vec(),
                subtype: Vec::new(),
                file: file_of(section, key),
            })),
            Shape::TwoLevel => {
                for type_name in section.keys() {
                    // Only the keys of the first level count; a name without section adds nothing.
                    let Some(inner) = registry.section(type_name) else {
                        continue;
                    };
                    entries.extend(inner.keys().map(|key| Entry {
                        class: class.name,
                        type_name: type_name.to_vec(),
                        subtype: key.to_vec(),
                        file: file_of(inner, key),
                    }));
                }
            }
            Shape::Subtypes { .. } | Shape::Staff => {
                let (shared, default) = match class.shape {
                    Shape::Subtypes { shared, default } => (shared, default),
                    _ => (false, &b""[..]),
                };
                // A shared list with at least one key applies to every type of the registry.
                let shared = shared
                    .then(|| registry.section("subtypes"))
                    .flatten()
                    .filter(|s| s.keys().next().is_some());
                for key in section.keys() {
                    let own_class = match class.shape {
                        Shape::Staff => match staff_class(key) {
                            Some(own_class) => own_class,
                            None => {
                                // ponytail: the original ignores it; the specification (O4)
                                // recommends to ignore and log.
                                self.warnings
                                    .push(format!("staff entry of unknown kind: {}", show(key)));
                                continue;
                            }
                        },
                        _ => class.name,
                    };
                    let own = registry.section([key, b"/subtypes"].concat());
                    let mut subtypes: Vec<Vec<u8>> = shared
                        .or(own)
                        .map(|s| s.keys().map(<[u8]>::to_vec).collect())
                        .unwrap_or_default();
                    if subtypes.is_empty() {
                        subtypes.push(default.to_vec());
                    }
                    entries.extend(subtypes.into_iter().map(|subtype| Entry {
                        class: own_class,
                        type_name: key.to_vec(),
                        subtype,
                        file: file_of(section, key),
                    }));
                }
            }
        }
        entries
    }

    /// Creates the type, or, if it exists, leaves it as it is.
    ///
    /// ponytail: the original replaces the definition file of an existing type without reading
    /// its values again; the specification (O2) recommends to keep the first definition and log
    /// the second. The same goes for a type that a user file names again (O11: no renaming).
    fn add(&mut self, vfs: &Vfs, entry: Entry) -> Result<(), EntityError> {
        let file = entry.file.as_deref().map(|f| normalise(&lossy(f)));
        let who = format!(
            "{}/{}/{}",
            entry.class,
            show(&entry.type_name),
            show(&entry.subtype)
        );
        if let Some(existing) = self.find(entry.class, &entry.type_name, &entry.subtype) {
            if existing.file != file {
                self.warnings
                    .push(format!("{who} is listed again with another file, ignored"));
            }
            return Ok(());
        }
        let config = match &file {
            // ponytail: the original reads the file of the entry processed just before here; the
            // specification (O3) recommends to create the type with defaults and log it.
            None => {
                self.warnings.push(format!("{who} has no definition file"));
                None
            }
            Some(file) if !vfs.contains(file) => {
                self.warnings.push(format!(
                    "{who}: definition file {} not found",
                    show(file.as_bytes())
                ));
                None
            }
            Some(file) => Some(read_config(vfs, file)?).filter(|c| !c.is_empty()),
        };
        let mut ty = EntityType {
            class: entry.class,
            type_name: entry.type_name,
            subtype: entry.subtype,
            file,
            config: None,
            name_id: 0,
            footprint: [1, 1, 1],
            height: 0,
            animations: Vec::new(),
        };
        if let Some(config) = &config {
            read_values(&mut ty, config);
            ty.animations = resolve_animations(&ty, config);
        }
        ty.config = config;
        self.types.push(ty);
        Ok(())
    }
}

fn read_config(vfs: &Vfs, member: &str) -> Result<Config, EntityError> {
    let bytes = vfs
        .read(member)
        .map_err(|e| EntityError(e.to_string().escape_debug().to_string()))?;
    Config::parse(&bytes).map_err(|e| EntityError(format!("{}: {e}", member.escape_debug())))
}

/// Replaces `target` by the integer value of `key` if the section has one.
fn set(section: &Section, key: &str, target: &mut i32) {
    if let Some(value) = section.get(key).and_then(to_int) {
        *target = value;
    }
}

/// Reads the name id, footprint and height from `[<prefix>Characteristics/Integers]`.
/// Returns whether the pass succeeded: the section exists and has the three movement rates.
fn read_keys(ty: &mut EntityType, config: &Config, prefix: &[u8]) -> bool {
    let Some(section) = config.section([prefix, b"Characteristics/Integers"].concat()) else {
        return false;
    };
    set(section, "cNameID", &mut ty.name_id);
    set(section, "cFootprintX", &mut ty.footprint[0]);
    set(section, "cFootprintY", &mut ty.footprint[1]);
    set(section, "cFootprintZ", &mut ty.footprint[2]);
    set(section, "cHeight", &mut ty.height);
    ["cSlowRate", "cMediumRate", "cFastRate"]
        .iter()
        .all(|key| section.get(key).is_some())
}

fn default_subtype(config: &Config) -> &[u8] {
    config
        .section("Global")
        .and_then(|global| global.get("DefaultSubtype"))
        .unwrap_or_default()
}

/// Reads the values of a type by the rules of its class (specification 6).
fn read_values(ty: &mut EntityType, config: &Config) {
    match ty.class {
        // Units read in passes: the default subtype's sections, then the type's own on top.
        // Guests use their type name where the others use the subtype name.
        "animals" | "guests" | "keeper" | "tour" | "maint" | "helicopter" => {
            let default = default_subtype(config);
            let own = if ty.class == "guests" {
                ty.type_name.clone()
            } else {
                ty.subtype.clone()
            };
            let mut passed = false;
            if !default.is_empty() {
                passed = read_keys(ty, config, &[default, b"/"].concat());
            }
            if default.is_empty() || !passed {
                read_keys(ty, config, b"");
            }
            // With an empty own name and a default, the prefix is `/`, which matches nothing.
            if own != default {
                read_keys(ty, config, &[&own, &b"/"[..]].concat());
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
        _ => {
            read_keys(ty, config, b"");
        }
    }
}

/// The animation table of a type (specification 9.1 and 9.2).
fn resolve_animations(ty: &EntityType, config: &Config) -> Vec<Animation> {
    let default = default_subtype(config);
    // The subtype sections come first; the other classes have only the plain section.
    // ponytail: the helicopter is not in the specification's list of classes with subtype
    // sections, so it reads the plain section only.
    let mut prefixes: Vec<&[u8]> = Vec::new();
    if matches!(
        ty.class,
        "guests" | "animals" | "keeper" | "maint" | "tour" | "fences" | "tankwall"
    ) {
        if ty.class == "guests" {
            prefixes.push(&ty.type_name);
        }
        prefixes.push(&ty.subtype);
        if !default.is_empty() {
            prefixes.push(default);
        }
    }
    let names = prefixes
        .iter()
        .map(|prefix| [prefix, &b"/Animations"[..]].concat())
        .chain(std::iter::once(b"Animations".to_vec()));
    let section = names
        .filter_map(|name| config.section(name))
        .find(|section| section.keys().next().is_some());
    let Some(section) = section else {
        return Vec::new();
    };

    let root = animation_root(ty, config);
    let mut animations = Vec::new();
    for name in section.keys() {
        let value = section.get(name).unwrap_or_default();
        // A value with `.ani` is the member as it stands.
        let member = if value.windows(4).any(|w| w == b".ani") {
            value.to_vec()
        } else if let Some(root) = &root {
            [root, value, b"/", value, b".ani"].concat()
        } else {
            // An animal whose subtype has no `[AnimPath]` entry has no root.
            continue;
        };
        animations.push(Animation {
            name: lossy(name),
            member: normalise(&lossy(&member)),
        });
    }
    animations
}

/// The folder an animation value is relative to, ending in `/`.
fn animation_root(ty: &EntityType, config: &Config) -> Option<Vec<u8>> {
    let (kind, subtype) = (&ty.type_name[..], &ty.subtype[..]);
    let joined = |parts: &[&[u8]]| {
        let mut root = Vec::new();
        for part in parts.iter().filter(|part| !part.is_empty()) {
            root.extend_from_slice(part);
            root.push(b'/');
        }
        root
    };
    Some(match ty.class {
        "objects" | "building" => joined(&[b"objects", subtype]),
        "paths" | "food" | "rubble" | "filter" => joined(&[ty.class.as_bytes(), subtype]),
        "fences" | "tankwall" => joined(&[ty.class.as_bytes(), kind, subtype]),
        "guests" => joined(&[b"guests", kind]),
        "animals" => {
            let path = config.section("AnimPath")?.get(subtype)?;
            joined(&[path])
        }
        "ambient" | "helicopter" => joined(&[ty.class.as_bytes(), kind, subtype]),
        // ponytail: not read in the original; the data fits `staff/<type>/<subtype>/`.
        _ => joined(&[b"staff", kind, subtype]),
    })
}
