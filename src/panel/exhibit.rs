//! The exhibit panel (information panels 3.5, 3.6, 10) and the keeper recommendations plaque it
//! opens for an animal: the list of the exhibits, the name, the status, the donations and the
//! popularity, the animals and the thoughts, and the sentences of an animal's recommendations,
//! the important ones in red. The panel shows no suitability and has no keeper assignment
//! button (13.5).
//!
//! [`Panel`] holds what the player chose (the current exhibit, the animal of the plaque) and
//! the refresh clocks; [`Panel::refresh`] writes the layout from the world and returns the
//! `panel` log lines of what changed. The rating is computed from the world at each refresh.

use crate::build::number;
use crate::entity::{Catalogue, EntityType};
use crate::exhibit::rating::{self, Record};
use crate::exhibit::viewing::{self, Wear};
use crate::exhibit::{EXTERIOR, GROUNDS, Tank};
use crate::panel::guest::{thought_rows, thought_texts};
use crate::status::Words;
use crate::text::format::{self, Arg};
use crate::text::to_cp1252;
use crate::ui::interface::Interface;
use crate::ui::layout::{Picture, Rgb};
use crate::ui::widgets::Row;
use crate::unit::Part;
use crate::vfs::Vfs;
use crate::world::World;

// The layout of the panel, the exhibit list button that shows it, the name field and the list of
// the exhibits.
pub const LAYOUT: i32 = 30;
pub const BUTTON: i32 = 1050;
pub const NAME: i32 = 4304;
pub const LIST: i32 = 4305;
// The list of the animals, the thoughts and the tabs that can be selected without a click: status
// and tank modification, each with its sub-layout.
pub const ANIMALS: i32 = 4326;
const THOUGHTS: i32 = 4330;
const STATUS_TAB: i32 = 4310;
const TANK_TAB: i32 = 4365;
const STATUS_LAYOUT: i32 = 700;
const TANK_LAYOUT: i32 = 704;
/// Each tab button and the sub-layout it shows: status, donations, animals, thoughts, tank.
const TABS: [(i32, i32); 5] = [
    (STATUS_TAB, STATUS_LAYOUT),
    (4313, 703),
    (4311, 701),
    (4312, 702),
    (TANK_TAB, TANK_LAYOUT),
];
// The status tab: the date, the upkeep (last month, this month, total) and the keeper, whose
// text is the string 4360 when assigned ("Yes") and the next one when not ("No").
const DATE: i32 = 4349;
const UPKEEP: [i32; 3] = [4325, 4323, 4347];
const KEEPER: i32 = 4364;
const YES: i32 = 4360;
// The donations tab: the stars and the donations (last month, this month, total).
const STARS: i32 = 4315;
const DONATIONS: [i32; 3] = [4319, 4317, 4321];
/// The string of the date of an exhibit.
const DATE_PATTERN: i32 = 22014;
/// The interface clock time between two refreshes of the panel and of the plaque (ms).
pub const PERIOD: u64 = 10_000;

// The recommendations plaque: its layout, the name, the list, the background and the title.
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
/// further. This string itself says an exhibit is well suited when no bit is set.
const RECOMMENDATION: i32 = 9900;
const FEMALE: i32 = 30000;
/// The bit of the water that does not suit the species and the line of an animal in a show tank
/// (the string is the base plus this).
const SALINITY: u32 = 11;
const SHOW_TANK: i32 = 91;
/// The line about a kind of scenery the species dislikes, and the first string of the names of
/// object types.
const DISLIKED: i32 = 90;
const OBJECT_NAMES: i32 = 55000;

/// The picture of the row of a land exhibit by its fence state, and that of a tank.
const FENCES: [&str; 3] = [
    "fences/lsgfen/lsgfen",
    "fences/lsyfen/lsyfen",
    "fences/lsrfen/lsrfen",
];
/// The pictures of a tank by water: clean, light, heavy; for a tank and for a show tank.
const TANK_ICONS: [[&str; 3]; 2] = [
    ["exbtank", "exbdirt1", "exbdirt2"],
    ["shwtank", "shwdirt1", "shwdirt2"],
];

/// The picture of a popularity (0 to 100) in the image set of 11 stars by halves.
pub fn stars(popularity: u32) -> usize {
    (popularity.min(100) * 10 / 100) as usize
}

/// The picture of a tank in the list (information panels 10.1): by its water purity, heavy up to
/// 1 and light below `murky` (the tank water pictures' limits), and the show tank's own set.
fn tank_icon(tank: &Tank, murky: i32) -> String {
    let purity = i64::from(tank.purity);
    let band = match purity {
        ..=1 => 2,
        p if p < i64::from(murky) => 1,
        _ => 0,
    };
    let name = TANK_ICONS[usize::from(tank.show)][band];
    format!("ui/tanks/{name}/{name}")
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

/// A line of the recommendations: its string, whether it is important, and for a line about
/// scenery the name id of the object type.
pub type Line = (i32, bool, Option<i32>);

/// The lines of an animal's recommendations (habitat rating 7): the important bits, the
/// important scenery the species dislikes (`scenery[0]`, name ids), the other bits, the other
/// scenery (`scenery[1]`); when nothing is added the exhibit is well suited. An animal in a show
/// tank has its own line, and the salinity bit after it in red.
pub fn recommendations(
    record: Option<&Record>,
    female: bool,
    show_tank: bool,
    scenery: [&[i32]; 2],
) -> Vec<Line> {
    let base = RECOMMENDATION + if female { FEMALE } else { 0 };
    if show_tank {
        let salt = record.is_some_and(|r| r.has(SALINITY));
        return [
            Some((base + SHOW_TANK, false, None)),
            salt.then_some((base + 1 + SALINITY as i32, true, None)),
        ]
        .into_iter()
        .flatten()
        .collect();
    }
    let part = |important: bool| {
        let bits = (0..64)
            .filter(move |&b| {
                record.is_some_and(|r| r.has(b) && (r.important >> b & 1 == 1) == important)
            })
            .map(move |b| (base + 1 + b as i32, important, None));
        let about = scenery[usize::from(!important)]
            .iter()
            .map(move |&name| (base + DISLIKED, important, Some(name)));
        bits.chain(about)
    };
    let lines: Vec<Line> = part(true).chain(part(false)).collect();
    match lines.is_empty() {
        true => vec![(base, false, None)],
        false => lines,
    }
}

/// The type of the entity at position `at` of the entity list.
pub(super) fn type_at<'a>(world: &World, cat: &'a Catalogue, at: usize) -> Option<&'a EntityType> {
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
        .filter(|(_, e)| !e.common.as_ref().is_some_and(|c| world.is_carried(c.id)))
        .map(|(at, _)| at)
        .collect()
}

/// The first icon of an animal type (entity types 9.4): the first value of the key `Icon` in the
/// first of the sections `[<subtype>/Icon]`, `[<default subtype>/Icon]` and `[Icon]` that has one.
/// It is the picture of its row in a list.
pub(super) fn list_icon(ty: &EntityType) -> Option<String> {
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

/// The refresh clock of a panel (5.1): the due time and a refresh that is asked for.
#[derive(Debug, Default)]
pub struct Clock {
    due: u64,
    pub force: bool,
    /// The last [`Clock::due`] that said yes was a forced one: the panel sets what only a shown
    /// or forced refresh sets (the follow button's picture).
    pub forced: bool,
}

impl Clock {
    /// Whether to refresh at clock time `now`: when asked for, or at the due time while
    /// `ticking` (the interface update runs and the game is not paused). A refresh that was
    /// asked for sets the due time to `period` itself, as a call with the clock at 0 does, so
    /// the next ordinary call refreshes again.
    pub fn due(&mut self, now: u64, ticking: bool, period: u64) -> bool {
        self.forced = self.force;
        if self.force {
            (self.force, self.due) = (false, period);
            return true;
        }
        let run = ticking && self.due <= now;
        if run {
            self.due = now.saturating_add(period);
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
    /// The exhibit the tab layouts were last set for (10.7).
    laid: Option<u32>,
    listed: Vec<u32>,
    /// The id of the entity of each row of the animal list.
    animals: Vec<u32>,
    /// The animal whose recommendations the plaque shows, by entity id.
    pub plaque: Option<u32>,
    /// The selected animal, whose row of the animal list is selected.
    pub animal: Option<u32>,
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
        self.laid = None;
        self.clock.force = true;
    }

    /// The panel was hidden: the lists are emptied and no exhibit is current.
    pub fn closed(&mut self) {
        self.shown = false;
        self.current = None;
        self.laid = None;
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
        let money = |amount: f32| format::money_cents(&words, f64::from(amount), false);
        let wear = viewing::wear(world, viewing::decayed(cat));
        // The list of the exhibits: every one but the grounds, its fence state or tank.
        let exhibits: Vec<_> = (world.exhibits().list().iter())
            .filter(|e| e.id != GROUNDS && e.id != EXTERIOR)
            .collect();
        let rows: Vec<Row> = exhibits
            .iter()
            .map(|e| {
                let icon = match e.tank {
                    Some(t) => tank_icon(&t, world.tank_config().murky),
                    None => FENCES[wear.get(&e.id).copied().unwrap_or(Wear::Sound) as usize].into(),
                };
                Row {
                    text: to_cp1252(&e.name),
                    icon: ui.tree.picture(vfs, &icon),
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
            ui.show(STATUS_LAYOUT);
        }
        ui.set_button(TANK_TAB, tank && ui.shown(TANK_LAYOUT), tank);
        // An exhibit that becomes current shows the selected tab's layout and hides the others
        // (10.7); with no exhibit the status layout is hidden.
        let now = current.map(|c| c.id);
        if now.is_some() && now != self.laid {
            for (tab, layout) in TABS {
                match ui.is_down(tab) {
                    true => ui.show(layout),
                    false => ui.set_hidden(layout, true),
                };
            }
        }
        self.laid = now;
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
        // The animals of the exhibit, each with its type's list icon.
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
        let shown = self.animal.and_then(|p| ids.iter().position(|&a| a == p));
        ui.select_row(ANIMALS, shown);
        // The twenty newest thoughts about the exhibit.
        let list = current.map_or_else(Vec::new, |c| world.units.thoughts.in_exhibit(c.id));
        ui.refill_rows(
            THOUGHTS,
            thought_rows(&thought_texts(ui, world, cat, &list)),
        );
        let thoughts: Vec<i32> = list.iter().map(|t| t.text).collect();
        self.animals = ids;
        let told = match current {
            Some(c) => vec![
                format!("exhibit {} {:?}", c.id, c.name),
                format!("status {date:?} upkeep {} staff {staff}", upkeep.join(" ")),
                format!("donations stars {star} {}", donations.join(" ")),
                match thoughts.is_empty() {
                    true => format!("animals {count} thoughts 0"),
                    false => format!("animals {count} thoughts {} {thoughts:?}", thoughts.len()),
                },
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

    /// Writes the plaque for the animal `self.plaque`, if it is still in the world: its name, the
    /// background and title of its keeper kind, and the recommendations of its species in the
    /// exhibit of its record (`record_exhibit`), rated from the world now. Returns the log lines of what changed.
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
        let exhibit = record_exhibit(world, cat, at);
        let kind = ty.map_or(0, |t| {
            keeper_kind(number(t, "cKeeper").map_or(9500, |v| v as i32))
        });
        let input = exhibit.map(|id| rating::input(world, cat, id));
        let species = ty.map(|t| String::from_utf8_lossy(&t.type_name).into_owned());
        let records = input.as_ref().map(rating::rate).unwrap_or_default();
        let record = records.iter().find(|r| Some(&r.key) == species.as_ref());
        let female = ty.is_some_and(|t| t.subtype.first() == Some(&b'f'));
        let show = exhibit
            .and_then(|id| world.exhibits().get(id))
            .is_some_and(|e| e.tank.is_some_and(|t| t.show));
        let dislikes = |important| match (&input, ty) {
            (Some(i), Some(t)) => rating::disliked(&i.site, &rating::Likes::of(t), important),
            _ => Vec::new(),
        };
        let lines = recommendations(record, female, show, [&dislikes(true), &dislikes(false)]);
        let none = Words::new(|_| String::new());
        let animal = entity
            .common
            .as_ref()
            .map_or(String::new(), |c| c.name.clone());
        let texts: Vec<String> = (lines.iter())
            .map(|&(id, _, object)| {
                let object = object.map_or(String::new(), |n| ui.string(OBJECT_NAMES + n));
                let args = [Arg::Text(&animal), Arg::Text(&object)];
                format::expand(&none, &ui.string(id), &args)
            })
            .collect();
        let rows: Vec<Row> = (texts.iter())
            .map(|t| Row {
                text: to_cp1252(t),
                ..Row::default()
            })
            .collect();
        ui.set_text(PLAQUE_NAME, &to_cp1252(&animal));
        let title = ui.string(PLAQUE_TITLE + kind as i32);
        ui.set_text(PLAQUE_TITLE, &to_cp1252(&title));
        let back: Option<Picture> = ui.tree.picture(vfs, BACKS[kind]);
        ui.set_picture(PLAQUE_BACK, back);
        ui.refill_rows(PLAQUE_LIST, rows);
        ui.set_row_colors(
            PLAQUE_LIST,
            lines
                .iter()
                .map(|&(_, important, _)| important.then_some(RED))
                .collect(),
        );
        let mut told = vec![format!("plaque {animal:?}")];
        told.extend(lines.iter().zip(&texts).map(|(&(_, important, _), t)| {
            format!("line{} {t:?}", if important { "!" } else { "" })
        }));
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

/// The exhibit whose record the animal panel shows for the animal at `at`: the home of an
/// amphibious animal in a tank (tank fauna 12.3), else the exhibit it stands in.
pub fn record_exhibit(world: &World, cat: &Catalogue, at: usize) -> Option<u32> {
    let here = animals_exhibit(world, at)?;
    let tank = (world.exhibits().get(here)).is_some_and(|e| e.tank.is_some_and(|t| !t.show));
    let amphibious = type_at(world, cat, at).is_some_and(|t| rating::Likes::of(t).amphibious);
    let id = world.zoo.entities[at].common.as_ref()?.id;
    let home = match world.units.get(id).map(|u| &u.part) {
        Some(Part::Animal(a)) => a.home,
        _ => None,
    };
    let home = home.filter(|&h| world.exhibits().get(h).is_some());
    Some(home.filter(|_| tank && amphibious).unwrap_or(here))
}

#[cfg(test)]
mod tests;
