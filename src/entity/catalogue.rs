//! Reading the registries and definition files into the [`Catalogue`].

use std::collections::{BTreeMap, HashMap};
use std::fmt;
use std::iter;
use std::ops::RangeInclusive;
use std::sync::Arc;

use crate::behaviour::{Behaviours, Call, Row};
use crate::config::{Config, Section};
use crate::strings::is_user_id;
use crate::usertext;
use crate::vfs::{Vfs, normalise};

use super::{
    EntityError, EntityType, MAX_DEFINITION_BYTES, MAX_ENTRIES, default_subtype, lossy, read_values,
};

/// The most warnings kept; the others are counted in a last line.
pub(super) const MAX_WARNINGS: usize = 100;

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
    /// The item types guests buy, by lower-case name.
    pub(crate) items: BTreeMap<String, crate::building::items::Item>,
    /// `cNonPathCost` of the unit class file `ztunit.ai` (pathfinding 6.5): the guests' cost off the
    /// paths; `None` when the file or the key is missing.
    pub guest_non_path_cost: Option<u32>,
    /// `cPredatorRadius` of the same file (animals 11.8), in tiles; `None` when missing.
    pub predator_radius: Option<i32>,
    /// `cPreyRadius` of the same file (animals 11.8.1), in tiles; `None` when missing.
    pub prey_radius: Option<i32>,
    /// `cChaseTimeOut` of the same file (animals 20.2), in AI ticks; `None` when missing.
    pub chase_time_out: Option<i32>,
    /// `cChaseLookAhead` of the same file (animals 21.2), in path nodes; `None` when missing.
    pub chase_look_ahead: Option<i32>,
    /// The award definitions (`award*.cfg`), read with the types.
    pub(crate) awards: crate::award::Definitions,
    /// The behaviour sets and ambient tables of the definition files that have some.
    pub(super) behaviours: BTreeMap<String, Behaviours>,
    /// The bytes of the definition files loaded.
    pub(super) loaded_bytes: usize,
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
/// of its own and answers to `unit`; `objects` and `unit` answer to no name above them (a record of
/// class `entity` finds nothing), and `ambient` answers to `Overlay`, a name the game never writes
/// (zoo format 6.16 point 3).
fn parent(class: &str) -> Option<&'static str> {
    match class {
        "building" | "fences" | "paths" | "food" | "rubble" | "filter" => Some("objects"),
        "tankwall" => Some("fences"),
        "animals" | "guests" | "staff" => Some("unit"),
        "keeper" | "tour" | "maint" | "helicopter" => Some("staff"),
        "ambient" => Some("Overlay"),
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
pub(super) fn show(bytes: &[u8]) -> String {
    bytes.escape_ascii().to_string()
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
        let refused = number_user_names(&mut catalogue.types, usertext::NAMES);
        if refused > 0 {
            catalogue.warn(|| format!("{refused} user names without an id: the ids are used up"));
        }
        let (items, warnings) = crate::building::items::load(vfs);
        catalogue.items = items;
        let unit = vfs
            .read("ztunit.ai")
            .ok()
            .and_then(|b| Config::parse(&b).ok());
        let key = |k| {
            (unit.as_ref()).and_then(|c| crate::timing::ini_whole(c, "Characteristics/Integers", k))
        };
        catalogue.guest_non_path_cost = key("cNonPathCost").map(|v| v.max(0) as u32);
        catalogue.predator_radius = key("cPredatorRadius");
        catalogue.prey_radius = key("cPreyRadius");
        catalogue.chase_time_out = key("cChaseTimeOut");
        catalogue.chase_look_ahead = key("cChaseLookAhead");
        catalogue.awards = crate::award::Definitions::load(vfs);
        warnings.into_iter().for_each(|w| catalogue.warn(|| w));
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
    pub(crate) fn push(&mut self, ty: EntityType) {
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

    /// The set `name` that a unit of type `ty` starts (specification 3.1): an animal tries its
    /// subtype, then the default subtype, then the plain section; a guest does the same with its
    /// type name, except `bWalk` and `bZooRoutine`, which are plain; every other class is plain.
    pub(crate) fn behaviour_set(&self, ty: &EntityType, name: &[u8]) -> Option<&[Call]> {
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
    pub(crate) fn ambient(&self, ty: &EntityType, table: usize) -> &[Row] {
        self.behaviours(ty).map_or(&[], |all| &all.tables[table])
    }

    /// The position in `types` of the type of this class, name and subtype.
    pub(crate) fn position(&self, class: &str, type_name: &[u8], subtype: &[u8]) -> Option<usize> {
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

/// Gives each user type (a name id that `is_user_id` accepts, entity types 10.3) its own name id
/// from `ids`, in order: the same name always gets the same id. The name is the type name for
/// units and the subtype name for the other classes. A name past the last id gets 0. Returns how
/// many got none.
pub(super) fn number_user_names(types: &mut [EntityType], ids: RangeInclusive<i32>) -> usize {
    let mut given: HashMap<Vec<u8>, i32> = HashMap::new();
    let mut next = *ids.start();
    let mut refused = 0;
    for ty in types.iter_mut().filter(|t| is_user_id(t.name_id)) {
        let name = if ty.class == "animals" {
            ty.type_name.clone()
        } else {
            ty.subtype.clone()
        };
        let id = match given.get(&name) {
            Some(&id) => id,
            None if next <= *ids.end() => {
                given.insert(name, next);
                next += 1;
                next - 1
            }
            None => {
                refused += 1;
                0
            }
        };
        ty.name_id = id;
    }
    refused
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
