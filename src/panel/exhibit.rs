//! The exhibit panel (information panels 3.5, 3.6, 10) and the keeper recommendations plaque it
//! opens for an animal: the list of the exhibits, the name, the status, the donations and the
//! popularity, the animals and the thoughts, and the sentences of an animal's recommendations,
//! the important ones in red. The panel shows no suitability and has no keeper assignment
//! button (13.5).
//!
//! [`Panel`] holds what the player chose (the current exhibit, the animal of the plaque) and
//! the refresh clock; [`Panel::refresh`] writes the layout from the world and returns the
//! `panel` log lines of what changed. The rating is computed from the world at each refresh.

use crate::build::number;
use crate::entity::{Catalogue, EntityType};
use crate::exhibit::rating::{self, Record};
use crate::exhibit::viewing::{self, Wear};
use crate::exhibit::{EXTERIOR, GROUNDS};
use crate::status::Words;
use crate::text::format::{self, Arg, whole_units};
use crate::text::to_cp1252;
use crate::ui::interface::Interface;
use crate::ui::layout::{Picture, Rgb};
use crate::ui::widgets::Row;
use crate::vfs::Vfs;
use crate::world::World;

/// The layout of the panel, the Exhibit List button that shows it and the close button.
pub const LAYOUT: i32 = 30;
pub const BUTTON: i32 = 1050;
/// The name field and the list of the exhibits.
pub const NAME: i32 = 4304;
pub const LIST: i32 = 4305;
/// The list of the animals (its tab) and the tab buttons: status and tank modification.
pub const ANIMALS: i32 = 4326;
const STATUS_TAB: i32 = 4310;
const TANK_TAB: i32 = 4365;
const TANK_LAYOUT: i32 = 704;
const STATUS_LAYOUT: i32 = 700;
const THOUGHTS: i32 = 4330;
/// The status tab: the date, the upkeep (last month, this month, total) and the keeper.
const DATE: i32 = 4349;
const UPKEEP: [i32; 3] = [4325, 4323, 4347];
const KEEPER: i32 = 4364;
/// The donations tab: the stars and the donations (last month, this month, total).
const STARS: i32 = 4315;
const DONATIONS: [i32; 3] = [4319, 4317, 4321];
/// The texts of the keeper row, "Yes" and "No".
const YES: i32 = 4360;
/// The string of the date of an exhibit.
const DATE_PATTERN: i32 = 22014;
/// The interface clock time between two refreshes of the panel and of the plaque (ms).
pub const PERIOD: u64 = 10_000;

/// The recommendations plaque: its layout, the name, the list, the background and the title.
pub const PLAQUE: i32 = 28;
const PLAQUE_NAME: i32 = 26001;
const PLAQUE_LIST: i32 = 26002;
const PLAQUE_BACK: i32 = 26000;
const PLAQUE_TITLE: i32 = 26020;
/// The colour of an important line.
const RED: Rgb = [212, 0, 0];
/// The backgrounds of the plaque by keeper kind.
const BACKS: [&str; 3] = [
    "ui/keprinfo/kinfobk/kinfobk",
    "ui/sciinfo/scinfobk/scinfobk",
    "ui/trnrinfo/tinfobk/tinfobk",
];
/// The string of a recommendation is this plus one plus its bit; a female animal's are 30000
/// further. The same sentence says an exhibit is well suited when no bit is set.
const RECOMMENDATION: i32 = 9900;
const FEMALE: i32 = 30000;

/// The picture of the row of a land exhibit by its fence state, and that of a tank.
const FENCES: [&str; 3] = [
    "fences/lsgfen/lsgfen",
    "fences/lsyfen/lsyfen",
    "fences/lsrfen/lsrfen",
];
const TANK_ICON: &str = "ui/tanks/exbtank/exbtank";

/// The picture of a popularity (0 to 100) in the image set of 11 stars by halves.
pub fn stars(popularity: u32) -> usize {
    (popularity.min(100) * 10 / 100) as usize
}

/// The keeper kind of an animal type by its `cKeeper` (information panels 2): 1 for the
/// scientist, 2 for the marine specialist, else the zookeeper's 0.
pub fn keeper_kind(keeper: i32) -> usize {
    match keeper {
        9551 => 1,
        9552 => 2,
        _ => 0,
    }
}

/// The lines of an animal's recommendations (habitat rating 7): the string number and whether
/// the line is important. The important bits come first, then the others, each in the order of
/// the bits; when nothing is set the exhibit is well suited. The sentences about the scenery
/// the species dislikes are not here yet.
pub fn recommendations(record: Option<&Record>, female: bool) -> Vec<(i32, bool)> {
    let base = RECOMMENDATION + if female { FEMALE } else { 0 };
    let bit = move |important: bool| {
        (0..64)
            .filter(move |&b| {
                record.is_some_and(|r| r.has(b) && (r.important >> b & 1 == 1) == important)
            })
            .map(move |b| (base + 1 + b as i32, important))
    };
    let lines: Vec<(i32, bool)> = bit(true).chain(bit(false)).collect();
    match lines.is_empty() {
        true => vec![(base, false)],
        false => lines,
    }
}

/// The type of the entity at position `at` of the entity list.
fn type_at<'a>(world: &World, cat: &'a Catalogue, at: usize) -> Option<&'a EntityType> {
    crate::place::bulldozer::type_of(cat, world.zoo.entities.get(at)?)
}

/// The entities of class animals that stand in exhibit `id`, in the order of the entity list.
pub fn animals_in(world: &World, id: u32) -> Vec<usize> {
    let width = i64::from(world.zoo.width);
    let on = |c: &crate::zoo::Common| {
        let (x, y) = (i64::from(c.x.div_euclid(64)), i64::from(c.y.div_euclid(64)));
        let on_map = (0..width).contains(&x) && y >= 0;
        on_map
            .then(|| world.exhibits().region((y * width + x) as usize))
            .flatten()
    };
    (world.zoo.entities.iter().enumerate())
        .filter(|(_, e)| e.class == "animals")
        .filter(|(_, e)| e.common.as_ref().and_then(on) == Some(id))
        .map(|(at, _)| at)
        .collect()
}

/// The first icon of an animal type (entity types 9.4): the first value of the key `Icon` in the
/// first of the sections `[<subtype>/Icon]`, `[<default subtype>/Icon]` and `[Icon]` that has one.
/// It is the picture of its row in a list.
fn list_icon(ty: &EntityType) -> Option<String> {
    let config = ty.config.as_deref()?;
    let default = config
        .section("Global")
        .and_then(|g| g.get("DefaultSubtype"));
    let named = |prefix: &[u8]| [prefix, b"/Icon"].concat();
    let sections = [
        Some(named(&ty.subtype)),
        default.map(named),
        Some(b"Icon".to_vec()),
    ];
    sections
        .into_iter()
        .flatten()
        .filter_map(|name| config.section(name))
        .find_map(|section| section.get_all("Icon").first().map(|v| v.to_vec()))
        .map(|v| String::from_utf8_lossy(&v).into_owned())
}

/// The text of a string as the bytes of a field.
fn bytes(ui: &Interface, id: i32) -> Vec<u8> {
    to_cp1252(&ui.string(id))
}

/// The refresh clock of a panel (5.1): the due time and a refresh that is asked for.
#[derive(Debug, Default)]
pub struct Clock {
    due: u64,
    pub force: bool,
}

impl Clock {
    /// Whether to refresh at clock time `now`: when asked for, or at the due time while
    /// `ticking` (the interface update runs and the game is not paused). A refresh that was
    /// asked for sets the due time to the period itself, as a call with the clock at 0 does, so
    /// the next ordinary call refreshes again.
    pub fn due(&mut self, now: u64, ticking: bool) -> bool {
        if self.force {
            (self.force, self.due) = (false, PERIOD);
            return true;
        }
        let run = ticking && self.due <= now;
        if run {
            self.due = now.saturating_add(PERIOD);
        }
        run
    }
}

/// What the panel and the plaque hold: the player's choices and the refresh clocks.
#[derive(Debug, Default)]
pub struct Panel {
    /// The layout was shown at the last look.
    pub shown: bool,
    /// The exhibit shown, and the exhibit of each row of the list.
    pub current: Option<u32>,
    listed: Vec<u32>,
    /// The id of the entity of each row of the animal list.
    animals: Vec<u32>,
    /// The animal whose recommendations the plaque shows, by entity id.
    pub plaque: Option<u32>,
    pub clock: Clock,
    pub plaque_clock: Clock,
    /// What the last refresh of the panel and of the plaque told.
    told: Vec<String>,
    plaque_told: Vec<String>,
}

impl Panel {
    /// The panel was just shown (4.2): no exhibit is current, nothing is selected.
    pub fn opened(&mut self) {
        self.shown = true;
        self.current = None;
        self.clock.force = true;
    }

    /// The panel was hidden: the lists are emptied and no exhibit is current.
    pub fn closed(&mut self) {
        self.shown = false;
        self.current = None;
        self.animals.clear();
        self.told.clear();
    }

    /// The plaque shows the animal with entity id `id` from now on, refreshed at once.
    pub fn show_plaque(&mut self, id: u32) {
        self.plaque = Some(id);
        self.plaque_told.clear();
        self.plaque_clock.force = true;
    }

    /// The exhibit of row `row` of the list, if there is such a row.
    pub fn exhibit_of(&self, row: usize) -> Option<u32> {
        self.listed.get(row).copied()
    }

    /// The entity id of row `row` of the animal list.
    pub fn animal_of(&self, row: usize) -> Option<u32> {
        self.animals.get(row).copied()
    }

    /// Writes the panel from the world and returns the log lines of what changed since the last
    /// refresh: the exhibit, the status row, the donations row and the lists.
    pub fn refresh(
        &mut self,
        ui: &mut Interface,
        world: &World,
        cat: &Catalogue,
        vfs: &Vfs,
    ) -> Vec<String> {
        let words = Words::new(|id| ui.string(id as i32));
        let money = |amount: f32| format::money(&words, whole_units(f64::from(amount)), true);
        let wear = viewing::wear(world, viewing::decayed(cat));
        // The list of the exhibits: every one but the grounds, its fence state or tank.
        let exhibits: Vec<_> = (world.exhibits().list().iter())
            .filter(|e| e.id != GROUNDS && e.id != EXTERIOR)
            .collect();
        let rows: Vec<Row> = exhibits
            .iter()
            .map(|e| {
                let icon = match e.tank {
                    Some(_) => TANK_ICON,
                    None => FENCES[wear.get(&e.id).copied().unwrap_or(Wear::Sound) as usize],
                };
                Row {
                    text: to_cp1252(&e.name),
                    icon: ui.tree.picture(vfs, icon),
                    ..Row::default()
                }
            })
            .collect();
        self.listed = exhibits.iter().map(|e| e.id).collect();
        let current = self.current.and_then(|id| world.exhibits().get(id));
        let at = current.and_then(|c| exhibits.iter().position(|e| e.id == c.id));
        ui.refill_rows(LIST, rows);
        ui.select_row(LIST, at);
        let name = current.map_or(String::new(), |c| c.name.clone());
        ui.set_text(NAME, &to_cp1252(&name));
        ui.set_enabled(NAME, current.is_some());
        // The tank tab serves a tank only: with a land exhibit or none, the status tab takes
        // its place (as its button's action would, without the click).
        let tank = current.is_some_and(|c| c.tank.is_some());
        if !tank && ui.shown(TANK_LAYOUT) {
            ui.set_button(STATUS_TAB, true, true);
            ui.set_hidden(TANK_LAYOUT, true);
            ui.set_hidden(STATUS_LAYOUT, false);
        }
        ui.set_button(TANK_TAB, tank && ui.shown(TANK_LAYOUT), tank);
        if current.is_none() {
            ui.set_hidden(STATUS_LAYOUT, true);
        }
        // The status and donations tabs.
        let stats = current.map(|c| &c.stats);
        let date = stats
            .and_then(|s| s.created.as_ref())
            .map_or(String::new(), |d| format::date(&ui.string(DATE_PATTERN), d));
        let zero = [0.0; 3];
        let upkeep = stats.map_or(zero, |s| s.upkeep).map(&money);
        let donations = stats.map_or(zero, |s| s.donations).map(&money);
        let keeper = current.and_then(|c| c.rating.keeper);
        let staff = match current {
            Some(_) => ui.string(YES + 1 - i32::from(keeper.is_some())),
            None => String::new(),
        };
        let star = stars(stats.map_or(0, |s| s.popularity));
        ui.set_text(DATE, &to_cp1252(&date));
        for (id, text) in UPKEEP
            .iter()
            .zip(&upkeep)
            .chain(DONATIONS.iter().zip(&donations))
        {
            ui.set_text(*id, &to_cp1252(text));
        }
        ui.set_text(KEEPER, &to_cp1252(&staff));
        ui.set_image(STARS, star);
        // The animals of the exhibit, each with its type's list icon; the thoughts are the
        // guests', which the simulation does not make yet.
        let at_all = current.map_or_else(Vec::new, |c| animals_in(world, c.id));
        let (mut rows, mut ids) = (Vec::new(), Vec::new());
        for at in at_all {
            let (Some(entity), ty) = (world.zoo.entities.get(at), type_at(world, cat, at)) else {
                continue;
            };
            let Some(c) = &entity.common else { continue };
            ids.push(c.id);
            rows.push(Row {
                text: to_cp1252(&c.name),
                icon: ty
                    .and_then(list_icon)
                    .and_then(|n| ui.tree.picture(vfs, &n)),
                ..Row::default()
            });
        }
        let count = ids.len();
        ui.refill_rows(ANIMALS, rows);
        let shown = self.plaque.and_then(|p| ids.iter().position(|&a| a == p));
        ui.select_row(ANIMALS, shown);
        ui.refill_rows(THOUGHTS, Vec::new());
        self.animals = ids;
        let told = match current {
            Some(c) => vec![
                format!("exhibit {} {:?}", c.id, c.name),
                format!("status {date:?} upkeep {} staff {staff}", upkeep.join(" ")),
                format!("donations stars {star} {}", donations.join(" ")),
                format!("animals {count} thoughts 0"),
            ],
            None => vec!["exhibit none".to_string()],
        };
        match told == self.told {
            true => Vec::new(),
            false => {
                self.told.clone_from(&told);
                told
            }
        }
    }

    /// Writes the plaque for the animal `self.plaque`, if the layout is shown: its name, the
    /// background and title of its keeper kind, and the recommendations of its species in the
    /// exhibit it stands in. Returns the log lines.
    pub fn refresh_plaque(
        &mut self,
        ui: &mut Interface,
        world: &World,
        cat: &Catalogue,
        vfs: &Vfs,
    ) -> Vec<String> {
        let Some(at) = self.plaque.and_then(|id| world.find(id)) else {
            return Vec::new();
        };
        let (entity, ty) = (&world.zoo.entities[at], type_at(world, cat, at));
        let exhibit = animals_exhibit(world, at);
        let kind = ty.map_or(0, |t| {
            keeper_kind(number(t, "cKeeper").map_or(9500, |v| v as i32))
        });
        let input = exhibit.map(|id| rating::input(world, cat, id));
        let species = ty.map(|t| String::from_utf8_lossy(&t.type_name).into_owned());
        let records = input.map(|i| rating::rate(&i)).unwrap_or_default();
        let record = records.iter().find(|r| Some(&r.key) == species.as_ref());
        let female = ty.is_some_and(|t| t.subtype.first() == Some(&b'f'));
        let lines = recommendations(record, female);
        let none = Words::new(|_| String::new());
        let name = ty.map_or(String::new(), |t| ui.string(t.name_id));
        let texts: Vec<String> = (lines.iter())
            .map(|&(id, _)| format::expand(&none, &ui.string(id), &[Arg::Text(&name)]))
            .collect();
        let rows: Vec<Row> = (texts.iter())
            .map(|t| Row {
                text: to_cp1252(t),
                ..Row::default()
            })
            .collect();
        let animal = entity
            .common
            .as_ref()
            .map_or(String::new(), |c| c.name.clone());
        ui.set_text(PLAQUE_NAME, &to_cp1252(&animal));
        let title = bytes(ui, PLAQUE_TITLE + kind as i32);
        ui.set_text(PLAQUE_TITLE, &title);
        let back: Option<Picture> = ui.tree.picture(vfs, BACKS[kind]);
        ui.set_picture(PLAQUE_BACK, back);
        ui.refill_rows(PLAQUE_LIST, rows);
        ui.set_row_colors(
            PLAQUE_LIST,
            lines
                .iter()
                .map(|&(_, important)| important.then_some(RED))
                .collect(),
        );
        let mut told = vec![format!("plaque {animal:?}")];
        told.extend(
            lines.iter().zip(&texts).map(|(&(_, important), t)| {
                format!("line{} {t:?}", if important { "!" } else { "" })
            }),
        );
        match told == self.plaque_told {
            true => Vec::new(),
            false => {
                self.plaque_told.clone_from(&told);
                told
            }
        }
    }
}

/// The exhibit that the entity at `at` stands in, if it is an exhibit and not the grounds.
pub fn animals_exhibit(world: &World, at: usize) -> Option<u32> {
    let c = world.zoo.entities.get(at)?.common.as_ref()?;
    let width = i64::from(world.zoo.width);
    let (x, y) = (i64::from(c.x.div_euclid(64)), i64::from(c.y.div_euclid(64)));
    let t = ((0..width).contains(&x) && y >= 0).then(|| (y * width + x) as usize)?;
    world
        .exhibits()
        .region(t)
        .filter(|&r| r != GROUNDS && r != EXTERIOR)
}

#[cfg(test)]
mod tests;
