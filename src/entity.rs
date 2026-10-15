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

use std::collections::{BTreeMap, HashMap};
use std::error::Error;
use std::fmt;
use std::iter;
use std::sync::Arc;

use crate::behaviour::{Behaviours, Call, Row};
use crate::config::{Config, Section, to_int};
use crate::vfs::{Vfs, normalise};

/// The most registry entries (types created or listed again) one catalogue takes. The full game
/// has 980 types; the limit only stops a registry made to multiply names.
pub const MAX_ENTRIES: usize = 100_000;

/// The most bytes of definition files kept. The full game has about 3 MB of them; the memory of a
/// parsed file is a multiple of its size.
pub const MAX_DEFINITION_BYTES: usize = 8 * 1024 * 1024;

/// The most warnings kept; the others are counted in a last line.
const MAX_WARNINGS: usize = 100;

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
    pub fn sound_member(&self, value: &[u8]) -> String {
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
    pub fn animations(&self) -> Vec<Animation> {
        self.animation_section().map_or_else(Vec::new, |section| {
            section
                .keys()
                .map(|name| self.animation(section, name))
                .collect()
        })
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

/// All types, in creation order, and what was odd on the way.
#[derive(Debug, Default)]
pub struct Catalogue {
    pub types: Vec<EntityType>,
    /// One line each, for things the original does silently or by accident: an entry without
    /// a file, a definition file that cannot be used, a registry that was skipped, a type
    /// listed again with another file, a staff entry of an unknown kind. Names from the game
    /// files are escaped. At most [`MAX_WARNINGS`] lines and a last one that counts the rest.
    pub warnings: Vec<String>,
    /// The positions in `types` by (type name, subtype name); a few classes can share a pair.
    index: HashMap<(Vec<u8>, Vec<u8>), Vec<usize>>,
    /// Every definition file asked for so far, by normalised name, with the outcome.
    files: HashMap<String, Definition>,
    /// The behaviour sets and ambient tables of the definition files that have some.
    behaviours: BTreeMap<String, Behaviours>,
    /// The bytes of the definition files loaded.
    loaded_bytes: usize,
    /// The entries taken.
    entries: usize,
    /// Whether an entry was dropped for [`MAX_ENTRIES`].
    truncated: bool,
    suppressed: usize,
}

/// What came of reading a definition file.
#[derive(Debug, Clone)]
enum Definition {
    Config(Arc<Config>),
    /// The file holds no section: the original counts it as not loaded, without a message.
    NoSection,
    Failed(String),
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
struct Entry<'a> {
    class: &'static str,
    type_name: &'a [u8],
    subtype: &'a [u8],
    file: Option<&'a [u8]>,
}

impl Entry<'_> {
    fn who(&self) -> String {
        format!(
            "{}/{}/{}",
            self.class,
            show(self.type_name),
            show(self.subtype)
        )
    }
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

/// The types a registry produces, entries in ascending byte order of their keys. Lazy: a
/// registry can name more types than anyone should build.
fn entries<'a>(class: &Class, registry: &'a Config) -> Box<dyn Iterator<Item = Entry<'a>> + 'a> {
    let Some(section) = registry.section(class.section) else {
        return Box::new(iter::empty());
    };
    let name = class.name;
    match class.shape {
        Shape::Flat => Box::new(section.keys().map(move |key| Entry {
            class: name,
            type_name: name.as_bytes(),
            subtype: key,
            file: section.get(key),
        })),
        Shape::FlatGuests => Box::new(section.keys().map(move |key| Entry {
            class: name,
            type_name: key,
            subtype: b"",
            file: section.get(key),
        })),
        Shape::TwoLevel => Box::new(
            section
                .keys()
                // Only the keys of the first level count; a name without section adds nothing.
                .filter_map(move |kind| registry.section(kind).map(|inner| (kind, inner)))
                .flat_map(move |(kind, inner)| {
                    inner.keys().map(move |key| Entry {
                        class: name,
                        type_name: kind,
                        subtype: key,
                        file: inner.get(key),
                    })
                }),
        ),
        Shape::Subtypes { .. } | Shape::Staff => {
            let (shared, default, staff) = match class.shape {
                Shape::Subtypes { shared, default } => (shared, default, false),
                _ => (false, &b""[..], true),
            };
            let has_keys = |s: &&Section| s.keys().next().is_some();
            // A shared list with at least one key applies to every type of the registry.
            let shared = shared
                .then(|| registry.section("subtypes"))
                .flatten()
                .filter(has_keys);
            Box::new(
                section
                    .keys()
                    .filter_map(move |key| {
                        let own_class = if staff { staff_class(key)? } else { name };
                        Some((key, own_class))
                    })
                    .flat_map(move |(key, own_class)| {
                        let own = registry.section([key, b"/subtypes"].concat());
                        let list = shared.or(own).filter(has_keys);
                        let file = section.get(key);
                        // No subtype listed: one type with the default subtype name.
                        list.into_iter()
                            .flat_map(Section::keys)
                            .chain(list.is_none().then_some(default))
                            .map(move |subtype| Entry {
                                class: own_class,
                                type_name: key,
                                subtype,
                                file,
                            })
                    }),
            )
        }
    }
}

impl Catalogue {
    /// Builds the catalogue of the installation behind `vfs` the way the original does.
    ///
    /// Fails only if the operating system fails to read a file. Everything else that would stop
    /// the original from loading something (a registry or a definition file that is missing,
    /// empty, damaged, without a section or too large) skips the registry or leaves the type
    /// without a configuration, with a warning.
    pub fn load(vfs: &Vfs) -> Result<Catalogue, EntityError> {
        Catalogue::load_with(vfs, &mut || {})
    }

    /// [`Catalogue::load`], calling `step` once for every type the files name (the progress of
    /// the start-up).
    pub fn load_with(vfs: &Vfs, step: &mut dyn FnMut()) -> Result<Catalogue, EntityError> {
        let mut catalogue = Catalogue::default();
        for class in &CLASSES {
            let mut loaded_any = false;
            // `*` also matches `/`: the test is on the whole name, so `items/x.cfg` matches `items`.
            for name in vfs.list(Some(&format!("{}*.cfg", class.prefix))) {
                if catalogue.truncated {
                    break;
                }
                let Some(registry) = catalogue.registry(vfs, &name)? else {
                    continue;
                };
                loaded_any = true;
                if let (Shape::Staff, Some(section)) =
                    (&class.shape, registry.section(class.section))
                {
                    // ponytail: the original ignores them; the specification (O4) recommends to
                    // ignore and log.
                    for key in section.keys().filter(|key| staff_class(key).is_none()) {
                        catalogue.warn(|| format!("staff entry of unknown kind: {}", show(key)));
                    }
                }
                catalogue.add_all(vfs, entries(class, &registry), step)?;
            }
            if let (true, Some(extension)) = (loaded_any, class.user_extension) {
                for name in vfs.list(Some(&format!("*{extension}"))) {
                    if catalogue.truncated {
                        break;
                    }
                    // The stem: the name without its extension and without the directories.
                    let stem = &name[..name.len() - extension.len()];
                    let stem = stem.rsplit('/').next().unwrap_or_default();
                    if stem.is_empty() {
                        // The original makes a type with an empty name (O14).
                        catalogue.warn(|| {
                            format!(
                                "user file {} has an empty name, skipped",
                                show(name.as_bytes())
                            )
                        });
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
                    let entries = names.into_iter().map(|(type_name, subtype)| Entry {
                        class: class.name,
                        type_name,
                        subtype,
                        file: Some(name.as_bytes()),
                    });
                    catalogue.add_all(vfs, entries, step)?;
                }
            }
        }
        if catalogue.suppressed > 0 {
            let more = catalogue.suppressed;
            catalogue
                .warnings
                .push(format!("{more} more warnings not shown"));
        }
        Ok(catalogue)
    }

    /// Adds a type that a test made, findable by its names.
    #[cfg(test)]
    pub fn push(&mut self, ty: EntityType) {
        let key = (ty.type_name.clone(), ty.subtype.clone());
        self.index.entry(key).or_default().push(self.types.len());
        self.types.push(ty);
    }

    /// The first type, in creation order, of class `class` (or of a class below it, so that
    /// `staff` finds a keeper) with exactly these names.
    pub fn find(&self, class: &str, type_name: &[u8], subtype: &[u8]) -> Option<&EntityType> {
        self.position(class, type_name, subtype)
            .map(|at| &self.types[at])
    }

    /// The behaviour sets and ambient tables of the definition file of `ty`, if it has any.
    pub fn behaviours(&self, ty: &EntityType) -> Option<&Behaviours> {
        self.behaviours.get(ty.file.as_deref()?)
    }

    /// The definition files that have behaviour sets or ambient tables, by name.
    pub fn behaviour_files(&self) -> impl Iterator<Item = (&str, &Behaviours)> {
        self.behaviours.iter().map(|(file, b)| (file.as_str(), b))
    }

    /// The set `name` that a unit of type `ty` starts (specification 3.1): an animal tries its
    /// subtype, then the default subtype, then the plain section; a guest does the same with its
    /// type name, except `bWalk` and `bZooRoutine`, which are plain; every other class is plain.
    pub fn behaviour_set(&self, ty: &EntityType, name: &[u8]) -> Option<&[Call]> {
        let all = self.behaviours(ty)?;
        let default = default_subtype(ty.config.as_deref()?);
        let own = if ty.class == "guests" {
            &ty.type_name
        } else {
            &ty.subtype
        };
        match ty.class {
            "guests" if name == b"bWalk" || name == b"bZooRoutine" => all.find(&[b""], name),
            "animals" | "guests" => all.find(&[own, default, b""], name),
            _ => all.find(&[b""], name),
        }
    }

    /// The rows of an ambient table of `ty` (an index of [`crate::behaviour::TABLES`]).
    pub fn ambient(&self, ty: &EntityType, table: usize) -> &[Row] {
        self.behaviours(ty).map_or(&[], |all| &all.tables[table])
    }

    /// The position in `types` of the type of this class, name and subtype.
    pub fn position(&self, class: &str, type_name: &[u8], subtype: &[u8]) -> Option<usize> {
        self.index
            .get(&(type_name.to_vec(), subtype.to_vec()))?
            .iter()
            .copied()
            .find(|&at| is_of_class(self.types[at].class, class))
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

    fn warn(&mut self, message: impl FnOnce() -> String) {
        if self.warnings.len() < MAX_WARNINGS {
            self.warnings.push(message());
        } else {
            self.suppressed += 1;
        }
    }

    /// The registry `name`, or `None` (with a warning, unless it simply has no section) if it
    /// does not load: the original skips it.
    fn registry(&mut self, vfs: &Vfs, name: &str) -> Result<Option<Config>, EntityError> {
        match read_config(vfs, name)? {
            Ok((config, _)) if !config.is_empty() => Ok(Some(config)),
            Ok(_) => Ok(None),
            Err(why) => {
                self.warn(|| format!("registry {} skipped: {why}", show(name.as_bytes())));
                Ok(None)
            }
        }
    }

    /// The definition file `file` (a normalised name), read once however many types name it.
    fn definition(&mut self, vfs: &Vfs, file: &str) -> Result<Definition, EntityError> {
        if let Some(known) = self.files.get(file) {
            return Ok(known.clone());
        }
        let definition = if self.loaded_bytes >= MAX_DEFINITION_BYTES {
            Definition::Failed(format!(
                "the definition files already hold {MAX_DEFINITION_BYTES} bytes, the limit"
            ))
        } else {
            match read_config(vfs, file)? {
                Ok((config, _)) if config.is_empty() => Definition::NoSection,
                Ok((config, len)) => {
                    self.loaded_bytes += len;
                    let found = Behaviours::parse(&config).map_err(|e| {
                        // The line is a courtesy: the file is read again to find it.
                        let line = vfs.read(file).ok().and_then(|bytes| e.line(&bytes));
                        let at = line.map_or_else(String::new, |line| format!(" line {line}"));
                        EntityError(format!("{}{at}: {e}", show(file.as_bytes())))
                    })?;
                    if found.sets().next().is_some() || found.tables.iter().any(|t| !t.is_empty()) {
                        self.behaviours.insert(file.to_string(), found);
                    }
                    Definition::Config(Arc::new(config))
                }
                Err(why) => Definition::Failed(why),
            }
        };
        self.files.insert(file.to_string(), definition.clone());
        Ok(definition)
    }

    /// Takes the entries one by one until none is left or [`MAX_ENTRIES`] are reached.
    fn add_all<'a>(
        &mut self,
        vfs: &Vfs,
        entries: impl Iterator<Item = Entry<'a>>,
        step: &mut dyn FnMut(),
    ) -> Result<(), EntityError> {
        for entry in entries {
            if self.entries >= MAX_ENTRIES {
                self.truncated = true;
                // Once, and whatever the number of warnings.
                self.warnings.push(format!(
                    "more than {MAX_ENTRIES} entries, the others are ignored"
                ));
                break;
            }
            self.entries += 1;
            self.add(vfs, entry)?;
            step();
        }
        Ok(())
    }

    /// Creates the type, or, if it exists, leaves it as it is.
    ///
    /// ponytail: the original replaces the definition file of an existing type without reading
    /// its values again; the specification (O2) recommends to keep the first definition and log
    /// the second. The same goes for a type that a user file names again (O11: no renaming).
    fn add(&mut self, vfs: &Vfs, entry: Entry) -> Result<(), EntityError> {
        let file = entry.file.map(|f| normalise(&lossy(f)));
        if let Some(at) = self.position(entry.class, entry.type_name, entry.subtype) {
            let existing = self.types[at].file.as_deref();
            // A loose file below nested search directories has several names for one file.
            if existing != file.as_deref() && !same_copy(vfs, existing, file.as_deref()) {
                self.warn(|| format!("{} is listed again with another file, ignored", entry.who()));
            }
            return Ok(());
        }
        let config = match &file {
            // ponytail: the original reads the file of the entry processed just before here; the
            // specification (O3) recommends to create the type with defaults and log it.
            None => {
                self.warn(|| format!("{} has no definition file", entry.who()));
                None
            }
            Some(file) => match self.definition(vfs, file)? {
                Definition::Config(config) => Some(config),
                Definition::NoSection => None,
                Definition::Failed(why) => {
                    self.warn(|| {
                        format!(
                            "{}: definition file {} cannot be used: {why}",
                            entry.who(),
                            show(file.as_bytes())
                        )
                    });
                    None
                }
            },
        };
        let mut ty = EntityType {
            class: entry.class,
            type_name: entry.type_name.to_vec(),
            subtype: entry.subtype.to_vec(),
            file,
            config,
            name_id: 0,
            footprint: [1, 1, 1],
            height: 0,
        };
        if let Some(config) = ty.config.clone() {
            read_values(&mut ty, &config);
        }
        self.index
            .entry((ty.type_name.clone(), ty.subtype.clone()))
            .or_default()
            .push(self.types.len());
        self.types.push(ty);
        Ok(())
    }
}

/// Whether two names stand for the same copy of the same file.
fn same_copy(vfs: &Vfs, a: Option<&str>, b: Option<&str>) -> bool {
    let first = |name: Option<&str>| {
        name.and_then(|name| vfs.which(name).ok())
            .and_then(|copies| copies.into_iter().next())
    };
    matches!((first(a), first(b)), (Some(a), Some(b)) if a == b)
}

/// Reads and parses a file. The inner error is a reason to show (the content is unusable); the
/// outer one is a failure of the system. The number is the size read.
fn read_config(vfs: &Vfs, member: &str) -> Result<Result<(Config, usize), String>, EntityError> {
    let escaped = |e: &dyn fmt::Display| e.to_string().escape_debug().to_string();
    let bytes = match vfs.read(member) {
        Ok(bytes) => bytes,
        Err(e) if e.is_system() => return Err(EntityError(escaped(&e))),
        Err(e) => return Ok(Err(escaped(&e))),
    };
    Ok(Config::parse(&bytes)
        .map(|config| (config, bytes.len()))
        .map_err(|e| escaped(&e)))
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

fn default_subtype(config: &Config) -> &[u8] {
    config
        .section("Global")
        .and_then(|global| global.get("DefaultSubtype"))
        .unwrap_or_default()
}

/// Reads the values of a type by the rules of its class (specification 6).
fn read_values(ty: &mut EntityType, config: &Config) {
    match ty.class {
        // Units: the name id, footprint and height belong to the ZT unit level, which reads the
        // default subtype's sections (the plain ones when there is no default subtype, and never
        // as a fallback), then the type's own on top. Guests use their type name where the
        // others use the subtype name.
        "animals" | "guests" | "keeper" | "tour" | "maint" | "helicopter" => {
            let default = default_subtype(config);
            let own = if ty.class == "guests" {
                ty.type_name.clone()
            } else {
                ty.subtype.clone()
            };
            let first = if default.is_empty() {
                Vec::new()
            } else {
                [default, b"/"].concat()
            };
            read_keys(ty, config, &first);
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
        _ => read_keys(ty, config, b""),
    }
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

#[cfg(test)]
mod tests {
    use std::io::Write;
    use std::path::PathBuf;

    use zip::write::SimpleFileOptions;
    use zip::{CompressionMethod, ZipWriter};

    use super::*;

    /// The bytes of an archive with these members.
    fn archive(members: &[(&str, &str)], method: CompressionMethod) -> Vec<u8> {
        let mut zip = ZipWriter::new(std::io::Cursor::new(Vec::new()));
        for (member, content) in members {
            zip.start_file(
                *member,
                SimpleFileOptions::default().compression_method(method),
            )
            .unwrap();
            zip.write_all(content.as_bytes()).unwrap();
        }
        zip.finish().unwrap().into_inner()
    }

    /// An installation made of one archive, in a fresh folder removed on drop.
    struct Install(PathBuf);

    impl Install {
        fn new(name: &str, members: &[(&str, &str)]) -> Install {
            let path =
                std::env::temp_dir().join(format!("rustyzoo-entity-{}-{name}", std::process::id()));
            let _ = std::fs::remove_dir_all(&path);
            std::fs::create_dir_all(&path).unwrap();
            std::fs::write(path.join("zoo.ini"), "[resource]\npath=.\n").unwrap();
            std::fs::write(
                path.join("a.ztd"),
                archive(members, CompressionMethod::Stored),
            )
            .unwrap();
            Install(path)
        }

        /// Writes a loose file below the install folder.
        fn loose(self, name: &str, content: &[u8]) -> Install {
            let path = self.0.join(name);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(path, content).unwrap();
            self
        }

        fn load(&self) -> Result<Catalogue, EntityError> {
            Catalogue::load(&Vfs::open(&self.0).unwrap())
        }
    }

    impl Drop for Install {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    /// The catalogue of an installation with these members.
    fn catalogue(name: &str, members: &[(&str, &str)]) -> Catalogue {
        Install::new(name, members).load().unwrap()
    }

    /// `class/type/subtype` of every type, in creation order.
    fn names(catalogue: &Catalogue) -> Vec<String> {
        catalogue
            .types
            .iter()
            .map(|t| format!("{}/{}/{}", t.class, show(&t.type_name), show(&t.subtype)))
            .collect()
    }

    fn get<'a>(c: &'a Catalogue, class: &str, kind: &str, subtype: &str) -> &'a EntityType {
        c.find(class, kind.as_bytes(), subtype.as_bytes())
            .unwrap_or_else(|| panic!("no type {class}/{kind}/{subtype}"))
    }

    #[test]
    fn registries_are_found_by_prefix_and_read_in_byte_order() {
        let c = catalogue(
            "order",
            &[
                ("animals.cfg", "[animals]\nzebra = z.ai\n"),
                ("Animal02.cfg", "[animals]\nyak = y.ai\n"),
                ("animal01.cfg", "[animals]\nxerus = x.ai\n"),
                ("animalx.cfg", "no section here\n"),
                ("sub/animal03.cfg", "[animals]\nnever = n.ai\n"),
                ("x.ai", "[Global]\n"),
            ],
        );
        assert_eq!(
            names(&c),
            ["animals/xerus/", "animals/yak/", "animals/zebra/"]
        );
        // The test is on the whole name: a folder name matches the prefix too.
        let c = catalogue("prefix", &[("food/x.cfg", "[food]\napple = a.ai\n")]);
        assert_eq!(names(&c), ["food/food/apple"]);
    }

    #[test]
    fn classes_are_processed_in_the_fixed_order() {
        let c = catalogue(
            "classes",
            &[
                ("twall.cfg", "[tankwall]\nw\n"),
                ("animals.cfg", "[animals]\na\n"),
                ("fences.cfg", "[fences]\nf\n"),
                ("food.cfg", "[food]\nb\n"),
            ],
        );
        assert_eq!(
            names(&c),
            ["food/food/b", "fences/f/f", "animals/a/", "tankwall/w/f"]
        );
        assert_eq!(
            c.counts().iter().map(|c| c.1).collect::<Vec<_>>(),
            [1, 0, 1, 0, 1, 0, 0, 0, 0, 0, 1, 0]
        );
    }

    #[test]
    fn entries_follow_the_order_of_the_keys_not_of_the_file() {
        let c = catalogue(
            "keys",
            &[("paths.cfg", "[paths]\nb = b.ai\nA = a.ai\na = a.ai\n")],
        );
        assert_eq!(
            names(&c),
            ["paths/paths/A", "paths/paths/a", "paths/paths/b"]
        );
    }

    #[test]
    fn flat_registries() {
        let c = catalogue(
            "flat",
            &[
                ("food.cfg", "[food]\nbone = b.ai\n"),
                ("bldg.cfg", "[building]\nshop = s.ai\n"),
                ("ambient.cfg", "[ambient]\nbird = b.ai\n"),
                ("guests.cfg", "[guests]\nwoman = w.ai\nman = m.ai\n"),
            ],
        );
        assert_eq!(
            names(&c),
            [
                "food/food/bone",
                "building/building/shop",
                "guests/man/",
                "guests/woman/",
                "ambient/ambient/bird"
            ]
        );
    }

    #[test]
    fn two_level_registries() {
        let c = catalogue(
            "two-level",
            &[
                (
                    "scenery.cfg",
                    "[objects]\nother\nfoliage = ignored\nnothing\n[foliage]\noak = o.ai\nacacia = a.ai\n[other]\nrock = r.ai\n",
                ),
                ("rubble.cfg", "[rubble]\nr1\n[r1]\nbig = b.ai\n"),
                ("filter.cfg", "[filter]\nfilter1\n[filter1]\nf = f.ai\n"),
            ],
        );
        assert_eq!(
            names(&c),
            [
                "objects/foliage/acacia",
                "objects/foliage/oak",
                "objects/other/rock",
                "rubble/r1/big",
                "filter/filter1/f"
            ]
        );
    }

    #[test]
    fn subtypes_come_from_the_shared_or_the_own_section() {
        let c = catalogue(
            "subtypes",
            &[
                // A shared section with a key wins over the own ones.
                (
                    "fences.cfg",
                    "[fences]\nwall = w.ai\nrail = r.ai\n[subtypes]\ng\nf\n[wall/subtypes]\nz\n",
                ),
                // An empty shared section counts as absent.
                (
                    "animals.cfg",
                    "[animals]\nlion = l.ai\nyak = y.ai\n[subtypes]\n[lion/subtypes]\nm\nf\n",
                ),
                // Staff only reads the own section, and the key decides the class.
                (
                    "staff.cfg",
                    "[staff]\nscient = s.ai\ntour = t.ai\nmaint\nguide = g.ai\nhelicopter = h.ai\n[subtypes]\nx\n[scient/subtypes]\nm\nf\n",
                ),
                ("twall.cfg", "[tankwall]\nglass = g.ai\n"),
            ],
        );
        assert_eq!(
            names(&c),
            [
                "fences/rail/f",
                "fences/rail/g",
                "fences/wall/f",
                "fences/wall/g",
                "animals/lion/f",
                "animals/lion/m",
                "animals/yak/",
                "helicopter/helicopter/",
                "maint/maint/",
                "keeper/scient/f",
                "keeper/scient/m",
                "tour/tour/",
                "tankwall/glass/f",
            ]
        );
        assert!(c.warnings.iter().any(|w| w.contains("unknown kind: guide")));
        // Staff is one group of the counts.
        assert_eq!(c.counts()[7], ("staff", 5));
    }

    #[test]
    fn a_type_listed_again_keeps_the_first_definition() {
        let c = catalogue(
            "again",
            &[
                ("animal01.cfg", "[animals]\nyak = Y.ai\n"),
                ("animal02.cfg", "[animals]\nyak = y.ai\n"),
                ("animal03.cfg", "[animals]\nyak = z.ai\n"),
                ("y.ai", "[Global]\n[Animations]\nidle = idle\n"),
                ("z.ai", "[Global]\n"),
            ],
        );
        assert_eq!(names(&c), ["animals/yak/"]);
        assert_eq!(get(&c, "animals", "yak", "").file.as_deref(), Some("y.ai"));
        // Another letter case of the same file is the same file; z.ai is another.
        assert_eq!(c.warnings.len(), 1, "{:?}", c.warnings);
        assert!(c.warnings[0].contains("another file"));
    }

    #[test]
    fn a_missing_or_empty_definition_file_leaves_defaults() {
        let c = catalogue(
            "missing",
            &[
                (
                    "fences.cfg",
                    "[fences]\nnovalue\nempty =\ngone = gone.ai\nblank = blank.ai\n",
                ),
                ("blank.ai", "nothing but text\n"),
                ("paths.cfg", "[paths]\nbare\n"),
            ],
        );
        assert_eq!(c.types.len(), 5);
        for t in &c.types {
            assert!(t.config.is_none(), "{:?}", t.type_name);
            assert_eq!((t.name_id, t.footprint, t.height), (0, [1, 1, 1], 0));
            assert!(t.animations().is_empty() && t.base_animation().is_none());
        }
        // The fences without a file are not an error and keep the default subtype `f`.
        assert_eq!(get(&c, "fences", "novalue", "f").file, None);
        assert_eq!(c.warnings.len(), 4, "{:?}", c.warnings);
        assert_eq!(
            get(&c, "fences", "gone", "f").file.as_deref(),
            Some("gone.ai")
        );
    }

    #[test]
    fn user_files_come_after_the_registries_of_their_class() {
        let user = "[Global]\n[Characteristics/Integers]\ncNameID = 19000\n";
        let members = [
            ("animals.cfg", "[animals]\nyak = y.ai\n"),
            ("scenery.cfg", "[objects]\nother\n[other]\nrock = r.ai\n"),
            ("bldg.cfg", "[building]\nshop = s.ai\n"),
            ("scenery/other/F554B047.ucs", user),
            ("scenery/other/rock.ucs", user),
            ("x/Pony.uca", user),
            ("home.ucb", user),
            ("a/.ucb", user),
            ("y.ai", user),
        ];
        let c = catalogue("user", &members);
        assert_eq!(
            names(&c),
            [
                "building/building/shop",
                "building/building/home",
                "animals/yak/",
                "animals/pony/m",
                "animals/pony/f",
                "animals/pony/y",
                "objects/other/rock",
                "objects/other/f554b047",
            ]
        );
        // The type that a registry names keeps its file.
        assert_eq!(
            get(&c, "objects", "other", "rock").file.as_deref(),
            Some("r.ai")
        );
        assert_eq!(
            get(&c, "building", "building", "home").file.as_deref(),
            Some("home.ucb")
        );
        assert!(c.warnings.iter().any(|w| w.contains("another file")));

        // Without a loaded registry of the class, its user files are not looked at.
        let c = catalogue(
            "user-alone",
            &[("a.uca", user), ("b.ucb", user), ("c.ucs", user)],
        );
        assert!(c.types.is_empty());
        let c = catalogue(
            "user-other",
            &[("bldg.cfg", "[building]\nshop = s.ai\n"), ("a.uca", user)],
        );
        assert_eq!(names(&c), ["building/building/shop"]);
    }

    #[test]
    fn a_registry_or_definition_file_that_cannot_be_read_is_skipped_with_a_warning() {
        let huge = format!("[food]\n{}", "; padding\n".repeat(200_000));
        let c = catalogue(
            "huge-registry",
            &[
                ("food.cfg", &huge),
                ("food2.cfg", "[food]\nbone = b.ai\n"),
                ("b.ai", "[Animations]\nidle = idle\n"),
            ],
        );
        assert_eq!(names(&c), ["food/food/bone"]);
        assert!(
            c.warnings.len() == 1
                && c.warnings[0].contains("food.cfg")
                && c.warnings[0].contains("limit"),
            "{:?}",
            c.warnings
        );
        let c = catalogue(
            "huge-definition",
            &[
                ("food.cfg", "[food]\nbone = b.ai\nmeat = m.ai\n"),
                ("b.ai", &huge),
                ("m.ai", "[Animations]\nidle = idle\n"),
            ],
        );
        assert_eq!(names(&c), ["food/food/bone", "food/food/meat"]);
        assert!(get(&c, "food", "food", "bone").config.is_none());
        assert!(get(&c, "food", "food", "meat").config.is_some());
        assert!(c.warnings.len() == 1 && c.warnings[0].contains("b.ai"));
    }

    #[test]
    fn a_damaged_member_leaves_the_other_types_alone() {
        let mut bytes = archive(
            &[
                ("b.ai", &"[Animations]\nidle = idle\n".repeat(50)),
                ("food.cfg", "[food]\nbone = b.ai\nmeat = m.ai\n"),
                ("m.ai", "[Animations]\nidle = idle\n"),
            ],
            CompressionMethod::Deflated,
        );
        // Overwrite the start of the data of the first member (it is at offset 0).
        let header = 30 + usize::from(bytes[26]) + usize::from(bytes[28]);
        bytes[header..header + 20].fill(0xff);
        let install = Install::new("damaged", &[]);
        std::fs::write(install.0.join("a.ztd"), bytes).unwrap();
        let c = install.load().unwrap();
        assert_eq!(names(&c), ["food/food/bone", "food/food/meat"]);
        assert!(get(&c, "food", "food", "bone").config.is_none());
        assert!(get(&c, "food", "food", "meat").config.is_some());
        assert!(c.warnings.len() == 1 && c.warnings[0].contains("b.ai"));
    }

    #[test]
    fn an_empty_loose_file_hides_the_member_and_the_type_keeps_the_defaults() {
        let install = Install::new(
            "empty-loose",
            &[
                ("food.cfg", "[food]\nbone = b.ai\n"),
                ("b.ai", "[Animations]\nidle = idle\n"),
            ],
        )
        .loose("b.ai", b"");
        let c = install.load().unwrap();
        assert!(get(&c, "food", "food", "bone").config.is_none());
        assert_eq!(c.warnings.len(), 1, "{:?}", c.warnings);
    }

    /// A failure of the system, unlike a file with the wrong content, stops the catalogue.
    #[cfg(unix)]
    #[test]
    fn a_failure_of_the_system_is_an_error() {
        use std::os::unix::fs::PermissionsExt;
        let install = Install::new("denied", &[("food.cfg", "[food]\nbone = b.ai\n")])
            .loose("b.ai", b"[Animations]\n");
        let path = install.0.join("b.ai");
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o0)).unwrap();
        if std::fs::File::open(&path).is_ok() {
            return; // running as root
        }
        let message = install.load().unwrap_err().to_string();
        assert!(message.contains("b.ai"), "{message}");
    }

    #[test]
    fn a_small_registry_cannot_make_an_unbounded_catalogue() {
        // 300 fences x 300 shared subtypes: all of them are built, and quickly.
        let keys =
            |prefix: &str, n: usize| (0..n).map(|i| format!("{prefix}{i}\n")).collect::<String>();
        let registry = format!("[fences]\n{}[subtypes]\n{}", keys("w", 300), keys("s", 300));
        let c = catalogue("many", &[("fences.cfg", &registry)]);
        assert_eq!(c.types.len(), 90_000);
        assert!(c.find("fences", b"w299", b"s299").is_some());
        // 1000 x 1000 is cut at the limit, with a warning.
        let registry = format!(
            "[fences]\n{}[subtypes]\n{}",
            keys("w", 1000),
            keys("s", 1000)
        );
        let c = catalogue("too-many", &[("fences.cfg", &registry)]);
        assert_eq!(c.types.len(), MAX_ENTRIES);
        assert!(c.warnings.iter().any(|w| w.contains("ignored")));
        // Warnings are bounded too: every entry here lacks a file.
        assert!(c.warnings.len() <= MAX_WARNINGS + 2, "{}", c.warnings.len());
        assert!(c.warnings.last().unwrap().contains("more warnings"));
    }

    #[test]
    fn types_that_name_the_same_file_share_its_content() {
        let big = "[Characteristics/Integers]\n".to_string()
            + &(0..20_000)
                .map(|i| format!("c{i:x}=1\n"))
                .collect::<String>();
        let registry = "[food]\n".to_string()
            + &(0..200)
                .map(|i| format!("f{i} = big.ai\n"))
                .collect::<String>();
        let c = catalogue("shared", &[("food.cfg", &registry), ("big.ai", &big)]);
        assert_eq!(c.types.len(), 200);
        let first = c.types[0].config.as_ref().unwrap();
        assert!(
            c.types
                .iter()
                .all(|t| Arc::ptr_eq(t.config.as_ref().unwrap(), first))
        );
        // The three subtypes of an animal share the file too, and the file is read once.
        let c = catalogue(
            "shared-animal",
            &[
                (
                    "animals.cfg",
                    "[animals]\nyak = y.ai\n[yak/subtypes]\nm\nf\ny\n",
                ),
                ("y.ai", "[Global]\n"),
            ],
        );
        assert!(Arc::ptr_eq(
            get(&c, "animals", "yak", "m").config.as_ref().unwrap(),
            get(&c, "animals", "yak", "y").config.as_ref().unwrap()
        ));
    }

    #[test]
    fn behaviour_sets_are_looked_up_by_class() {
        let c = catalogue(
            "behaviours",
            &[
                (
                    "animals.cfg",
                    "[animals]\nyak = y.ai\n[yak/subtypes]\nm\nf\n",
                ),
                ("guests.cfg", "[guests]\nman = g.ai\n"),
                (
                    "staff.cfg",
                    "[staff]\nkeeper = k.ai\n[keeper/subtypes]\nf\n",
                ),
                (
                    "y.ai",
                    "[Global]\nDefaultSubtype = f\n[m\\BehaviorSet\\bA]\nf = fHalt()\n\
                     [f\\BehaviorSet\\bA]\nf = fDie()\n[BehaviorSet\\bA]\nf = fHatch()\n\
                     [f\\BehaviorSet\\bB]\nf = fDie()\n[AmbientAnims]\na = -100\na = 100\nb = bA\n",
                ),
                (
                    "g.ai",
                    "[BehaviorSet\\bWalk]\nf = fWalk(0,0)\n[man\\BehaviorSet\\bWalk]\nf = fHalt()\n\
                     [man\\BehaviorSet\\bGawk]\nf = fHalt()\n[BehaviorSet\\bGawk]\nf = fDie()\n",
                ),
                (
                    "k.ai",
                    "[f\\BehaviorSet\\bX]\nf = fDie()\n[BehaviorSet\\bX]\nf = fHalt()\n",
                ),
            ],
        );
        let first = |class, kind, sub, name: &str| {
            let ty = get(&c, class, kind, sub);
            c.behaviour_set(ty, name.as_bytes())
                .map(|calls| calls[0].name)
        };
        // An animal: its subtype, then the default subtype, then the plain section.
        assert_eq!(first("animals", "yak", "m", "bA"), Some("fHalt"));
        assert_eq!(first("animals", "yak", "f", "bA"), Some("fDie"));
        assert_eq!(first("animals", "yak", "m", "bB"), Some("fDie"));
        assert_eq!(first("animals", "yak", "f", "bC"), None);
        // A guest: its type name, but bWalk is the plain one. Staff: the plain one only.
        assert_eq!(first("guests", "man", "", "bWalk"), Some("fWalk"));
        assert_eq!(first("guests", "man", "", "bGawk"), Some("fHalt"));
        assert_eq!(first("keeper", "keeper", "f", "bX"), Some("fHalt"));
        let yak = get(&c, "animals", "yak", "m");
        assert_eq!(c.ambient(yak, 0).len(), 1);
        assert!(c.ambient(yak, 1).is_empty());
        assert_eq!(c.behaviour_files().count(), 3);
        // The file is parsed once however many types name it.
        assert!(std::ptr::eq(
            c.behaviours(yak).unwrap(),
            c.behaviours(get(&c, "animals", "yak", "f")).unwrap()
        ));
    }

    #[test]
    fn an_unknown_operation_fails_the_load_with_the_file_and_line() {
        let install = Install::new(
            "bad-call",
            &[
                ("animals.cfg", "[animals]\nyak = y.ai\n"),
                (
                    "y.ai",
                    "[Global]\n\n[BehaviorSet\\bA]\nf = fHalt()\nf = fWait(3)\n",
                ),
            ],
        );
        let error = install.load().unwrap_err().to_string();
        assert_eq!(
            error,
            "y.ai line 5: section [BehaviorSet\\bA], call 2: unknown operation in fWait(3)"
        );
        // The names of the data that fail when run, and bad arguments, load with a warning.
        let good = catalogue(
            "bad-arguments",
            &[
                ("animals.cfg", "[animals]\nyak = y.ai\n"),
                (
                    "y.ai",
                    "[BehaviorSet\\bA]\nf = fFollow(keeper)\nf = fPlayTime(stand)\n",
                ),
            ],
        );
        let yak = get(&good, "animals", "yak", "");
        assert_eq!(good.behaviours(yak).unwrap().warnings.len(), 1);
    }

    #[test]
    fn a_key_without_value_does_not_resolve() {
        let c = catalogue(
            "no-value",
            &[
                (
                    "animals.cfg",
                    "[animals]\napato = a.ai\n[apato/subtypes]\nm\ny\n",
                ),
                (
                    "a.ai",
                    "[AnimPath]\nm = animals/apato/m\ny = animals/apato/y\n\
                     [m/Animations]\nidle = idle\n[y/Animations]\nFrom stand\nidle = idle\n",
                ),
            ],
        );
        // The first key in byte order is the base animation, even without a member.
        let young = get(&c, "animals", "apato", "y");
        let base = young.base_animation().unwrap();
        assert_eq!((base.name.as_str(), base.member), ("From stand", None));
        assert_eq!(young.animations().len(), 2);
        assert_eq!(
            get(&c, "animals", "apato", "m")
                .base_animation()
                .unwrap()
                .member
                .as_deref(),
            Some("animals/apato/m/idle/idle.ani")
        );
    }

    #[test]
    fn a_unit_with_a_default_subtype_never_reads_the_plain_name_id() {
        let c = catalogue(
            "no-fallback",
            &[
                (
                    "animals.cfg",
                    "[animals]\nleopard = l.ai\nlynx = x.ai\n[leopard/subtypes]\nm\n[lynx/subtypes]\nm\n",
                ),
                (
                    "l.ai",
                    "[Global]\nDefaultSubtype = f\n[Characteristics/Integers]\ncNameID = 7\n",
                ),
                ("x.ai", "[Characteristics/Integers]\ncNameID = 8\n"),
            ],
        );
        assert_eq!(get(&c, "animals", "leopard", "m").name_id, 0);
        assert_eq!(get(&c, "animals", "lynx", "m").name_id, 8);
    }

    #[test]
    fn nested_search_directories_name_a_loose_user_file_twice() {
        let install = Install::new(
            "nested",
            &[
                ("scener.cfg", "[objects]\nother\n[other]\nrock = r.ai\n"),
                ("r.ai", "[Global]\n"),
            ],
        )
        .loose("updates/pony.ucs", b"[Global]\n")
        .loose("a/rock2.ucs", b"[Global]\n")
        .loose("b/rock2.ucs", b"[Global]\n");
        std::fs::write(install.0.join("zoo.ini"), "[resource]\npath=.;./updates\n").unwrap();
        let c = install.load().unwrap();
        // `updates/pony.ucs` and `pony.ucs` are one file: one type, no warning.
        assert!(c.find("objects", b"other", b"pony").is_some());
        // Two files that share a stem are two files.
        assert!(
            c.warnings.iter().any(|w| w.contains("rock2")),
            "{:?}",
            c.warnings
        );
        assert_eq!(c.warnings.len(), 1, "{:?}", c.warnings);
    }

    #[test]
    fn a_user_file_without_a_stem_is_skipped_with_a_warning() {
        let c = catalogue(
            "empty-stem",
            &[
                ("scener.cfg", "[objects]\nother\n[other]\nrock = r.ai\n"),
                ("scenery/.ucs", "[Global]\n"),
            ],
        );
        assert_eq!(names(&c), ["objects/other/rock"]);
        assert!(c.warnings.iter().any(|w| w.contains("empty name")));
    }

    /// One definition file per reading rule of the values.
    #[test]
    fn values_follow_the_passes_of_their_class() {
        let rates = "cSlowRate=1\ncMediumRate=2\ncFastRate=3\n";
        let lion = format!(
            "[Global]\nDefaultSubtype = f\n[f/Characteristics/Integers]\ncNameID = 5006\ncFootprintX = 1\ncFootprintY = 2\ncHeight = 0x10\n{rates}\
             [m/Characteristics/Integers]\ncFootprintY = 3\ncNameID =\n[Characteristics/Integers]\ncNameID = 999\n"
        );
        // Name id and footprint are read at the ZT unit level, which never falls back to the
        // plain sections when there is a default subtype: the plain name id is not used.
        let tiger = format!(
            "[Global]\nDefaultSubtype = f\n[f/Characteristics/Integers]\ncNameID = 1\ncFootprintZ = 4\n[Characteristics/Integers]\ncNameID = 2\ncFootprintX = 6\n{rates}"
        );
        let guests = format!(
            "[Global]\nDefaultSubtype = man\n[man/Characteristics/Integers]\ncNameID = 3\n{rates}[woman/Characteristics/Integers]\ncNameID = 4\n"
        );
        let fence = "[Characteristics/Integers]\ncNameID = 1\ncHeight = 2\ncFootprintX = 3\n\
                     [g/Characteristics/Integers]\ncNameID = 9\ncFootprintX = 8\n";
        let scenery = "[Characteristics/Integers]\ncNameID = 7\ncFootprintX = 5\ncFootprintY = 6\n\
                       [tree/Characteristics/Integers]\ncNameID = 70\n[Global]\nDefaultSubtype = tree\n";
        let c = catalogue(
            "values",
            &[
                (
                    "animals.cfg",
                    "[animals]\nlion = lion.ai\ntiger = tiger.ai\n[lion/subtypes]\nf\nm\ny\n[tiger/subtypes]\nm\n",
                ),
                (
                    "guests.cfg",
                    "[guests]\nman = guests.ai\nwoman = guests.ai\n",
                ),
                (
                    "fences.cfg",
                    "[fences]\nrail = fence.ai\n[rail/subtypes]\nf\ng\n",
                ),
                (
                    "scenery.cfg",
                    "[objects]\nfoliage\n[foliage]\ntree = tree.ai\n",
                ),
                ("lion.ai", &lion),
                ("tiger.ai", &tiger),
                ("guests.ai", &guests),
                ("fence.ai", fence),
                ("tree.ai", scenery),
            ],
        );
        let values = |class, kind, subtype| {
            let t = get(&c, class, kind, subtype);
            (t.name_id, t.footprint, t.height)
        };
        assert_eq!(values("animals", "lion", "f"), (5006, [1, 2, 1], 16));
        // The own pass is on top of the default one, key by key; a key without value is skipped.
        assert_eq!(values("animals", "lion", "m"), (5006, [1, 3, 1], 16));
        assert_eq!(values("animals", "lion", "y"), (5006, [1, 2, 1], 16));
        assert_eq!(values("animals", "tiger", "m"), (1, [1, 1, 4], 0));
        assert_eq!(values("guests", "man", ""), (3, [1, 1, 1], 0));
        assert_eq!(values("guests", "woman", ""), (4, [1, 1, 1], 0));
        // Fences: the plain sections, then name id and height again from the subtype's.
        assert_eq!(values("fences", "rail", "f"), (1, [3, 1, 1], 2));
        assert_eq!(values("fences", "rail", "g"), (9, [3, 1, 1], 2));
        // Scenery never reads a prefixed section.
        assert_eq!(values("objects", "foliage", "tree"), (7, [5, 6, 1], 0));
        assert!(get(&c, "objects", "foliage", "tree").config.is_some());
    }

    #[test]
    fn animation_sections_and_roots_by_class() {
        let ani = "[Animations]\nidle = idle\nused = used\nfull = X\\Y.ani\n";
        let c = catalogue(
            "animations",
            &[
                (
                    "scenery.cfg",
                    "[objects]\nfoliage\n[foliage]\ntree = ani.ai\n",
                ),
                ("bldg.cfg", "[building]\nshop = ani.ai\n"),
                ("paths.cfg", "[paths]\nroad = ani.ai\n"),
                ("food.cfg", "[food]\nbone = ani.ai\n"),
                ("rubble.cfg", "[rubble]\nr1\n[r1]\nbig = ani.ai\n"),
                ("filter.cfg", "[filter]\nf1\n[f1]\nclean = ani.ai\n"),
                ("ambient.cfg", "[ambient]\nbird = ani.ai\n"),
                ("helicopter.cfg", "not a staff registry\n"),
                (
                    "fences.cfg",
                    "[fences]\nrail = fence.ai\n[rail/subtypes]\nf\ng\n",
                ),
                (
                    "animals.cfg",
                    "[animals]\nlion = lion.ai\nyak = yak.ai\n[lion/subtypes]\nm\nf\n",
                ),
                ("guests.cfg", "[guests]\nman = guest.ai\n"),
                (
                    "staff.cfg",
                    "[staff]\nkeeper = ani.ai\nhelicopter = ani.ai\n[keeper/subtypes]\nm\n",
                ),
                ("ani.ai", ani),
                // The subtype's own section wins; an empty one does not count.
                (
                    "fence.ai",
                    "[f/Animations]\nidle = idle\n[g/Animations]\n[Animations]\nbroke = broke\n",
                ),
                (
                    "lion.ai",
                    "[Global]\nDefaultSubtype = m\n[AnimPath]\nm = animals/lion/m\nf = Animals\\Lion\\F\n\
                     [m/Animations]\nidle = stand\n[f/Animations]\n[Animations]\nidle = plain\n",
                ),
                ("yak.ai", "[Animations]\nidle = idle\nwalk = w/w.ani\n"),
                (
                    "guest.ai",
                    "[Global]\nDefaultSubtype = man\n[man/Animations]\nidle = idle\n[Animations]\nwalk = walk\n",
                ),
            ],
        );
        let member = |class, kind, subtype, name: &str| {
            get(&c, class, kind, subtype)
                .animations()
                .into_iter()
                .find(|a| a.name == name)
                .and_then(|a| a.member)
        };
        for (class, kind, subtype, expected) in [
            ("objects", "foliage", "tree", "objects/tree/idle/idle.ani"),
            ("building", "building", "shop", "objects/shop/idle/idle.ani"),
            ("paths", "paths", "road", "paths/road/idle/idle.ani"),
            ("food", "food", "bone", "food/bone/idle/idle.ani"),
            ("rubble", "r1", "big", "rubble/big/idle/idle.ani"),
            ("filter", "f1", "clean", "filter/clean/idle/idle.ani"),
            (
                "ambient",
                "ambient",
                "bird",
                "ambient/ambient/bird/idle/idle.ani",
            ),
            ("fences", "rail", "f", "fences/rail/f/idle/idle.ani"),
            ("animals", "lion", "m", "animals/lion/m/stand/stand.ani"),
            ("guests", "man", "", "guests/man/idle/idle.ani"),
            ("keeper", "keeper", "m", "staff/keeper/m/idle/idle.ani"),
            (
                "helicopter",
                "helicopter",
                "",
                "helicopter/helicopter/idle/idle.ani",
            ),
        ] {
            assert_eq!(
                member(class, kind, subtype, "idle").as_deref(),
                Some(expected)
            );
        }
        // A value with `.ani` is the member itself, in its normalised form.
        assert_eq!(
            member("objects", "foliage", "tree", "full").as_deref(),
            Some("x/y.ani")
        );
        assert_eq!(
            member("animals", "yak", "", "walk").as_deref(),
            Some("w/w.ani")
        );
        // No `[AnimPath]` entry for the subtype: the root is empty.
        assert_eq!(
            member("animals", "yak", "", "idle").as_deref(),
            Some("idle/idle.ani")
        );
        // The animals of the empty subtype section and of the plain one: g has no animations of
        // its own, so the plain section is used; f's empty section falls back to the default's.
        assert_eq!(
            member("fences", "rail", "g", "broke").as_deref(),
            Some("fences/rail/g/broke/broke.ani")
        );
        assert_eq!(
            member("animals", "lion", "f", "idle").as_deref(),
            Some("animals/lion/f/stand/stand.ani")
        );
        // Names come in ascending byte order.
        let order: Vec<_> = get(&c, "objects", "foliage", "tree")
            .animations()
            .into_iter()
            .map(|a| a.name)
            .collect();
        assert_eq!(order, ["full", "idle", "used"]);
    }

    #[test]
    fn lookup_accepts_parent_classes_and_base_animation_follows_the_class() {
        let ani = "[Animations]\nempty = empty\nfull = full\nidle = idle\n";
        let c = catalogue(
            "lookup",
            &[
                (
                    "staff.cfg",
                    "[staff]\nkeeper = a.ai\n[keeper/subtypes]\nm\n",
                ),
                ("bldg.cfg", "[building]\nshop = a.ai\n"),
                ("food.cfg", "[food]\nbone = a.ai\nmeat = b.ai\n"),
                ("fences.cfg", "[fences]\nrail = c.ai\n"),
                ("a.ai", ani),
                ("b.ai", "[Animations]\nempty = empty\nfull = full\n"),
                ("c.ai", "[Animations]\nbroke = broke\nidle = idle\n"),
            ],
        );
        assert_eq!(c.find("staff", b"keeper", b"m").unwrap().class, "keeper");
        assert!(c.find("keeper", b"keeper", b"m").is_some());
        assert!(c.find("animals", b"keeper", b"m").is_none());
        assert_eq!(
            c.find("objects", b"building", b"shop").unwrap().class,
            "building"
        );
        assert!(c.find("building", b"building", b"").is_none());
        let base = |class, kind, subtype| {
            get(&c, class, kind, subtype)
                .base_animation()
                .map(|a| a.name)
        };
        assert_eq!(
            base("building", "building", "shop").as_deref(),
            Some("empty")
        );
        assert_eq!(base("food", "food", "bone").as_deref(), Some("idle"));
        assert_eq!(base("food", "food", "meat").as_deref(), Some("full"));
        assert_eq!(base("fences", "rail", "f").as_deref(), Some("idle"));
    }

    /// The numbers of the catalogue table of the specification, for the full game.
    #[test]
    #[ignore = "needs the game: set ZT_DATA_DIR to the install folder"]
    fn full_game_counts() {
        let dir = std::env::var_os("ZT_DATA_DIR").expect("ZT_DATA_DIR is not set");
        let vfs = Vfs::open(std::path::Path::new(&dir)).unwrap();
        let c = Catalogue::load(&vfs).unwrap();
        assert_eq!(c.types.len(), 980);
        assert_eq!(
            c.counts().iter().map(|c| c.1).collect::<Vec<_>>(),
            [14, 17, 73, 173, 360, 4, 295, 11, 9, 7, 16, 1]
        );
        assert!(c.warnings.is_empty(), "{:?}", c.warnings);
        // Every definition file loads; five scenery types give the name id 0 themselves.
        assert!(c.types.iter().all(|t| t.config.is_some()));
        let unnamed: Vec<_> = c.types.iter().filter(|t| t.name_id == 0).collect();
        assert_eq!(unnamed.len(), 5);
        assert!(unnamed.iter().all(|t| t.class == "objects"));
        // Every base animation exists except for two animals.
        let missing: Vec<_> = c
            .types
            .iter()
            .filter(|t| {
                !t.base_animation()
                    .and_then(|a| a.member)
                    .is_some_and(|member| vfs.contains(&member))
            })
            .map(|t| t.class)
            .collect();
        assert_eq!(missing, ["animals", "animals"]);
        // The young apatosaurus has a key without value that sorts first: no member.
        let base = get(&c, "animals", "apato", "y").base_animation().unwrap();
        assert_eq!(base.member, None);
        // The loaded definition files are a small part of the budget.
        assert!(
            c.loaded_bytes < MAX_DEFINITION_BYTES / 2,
            "{}",
            c.loaded_bytes
        );
        // Lookups as the map files spell them.
        assert!(c.find("fences", b"zoowall", b"f").is_some());
        assert!(c.find("animals", b"lion", b"m").is_some());
        assert!(c.find("staff", b"keeper", b"f").is_some());
        assert!(c.find("building", b"building", b"restrant").is_some());
        assert!(c.find("objects", b"foliage", b"acacia").is_some());
        assert!(c.find("paths", b"paths", b"dirtpath").is_some());
    }
}
