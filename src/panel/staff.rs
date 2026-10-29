//! The staff panel (information panels 3.4, 9; staff 6, 8.1, 9): the name, the monthly salary, the
//! text of the current job, and on the other tab the exhibits assigned to a keeper or guide, or
//! the duty switches of a maintenance worker; refreshed every second of the interface clock while
//! shown and not while the game is paused.
//!
//! [`Panel`] holds the selected staff member and the clock; [`Panel::refresh`] writes the layout
//! from the world and returns the `panel` log line when what it shows changed.

use crate::config::to_int;
use crate::entity::{Catalogue, EntityType};
use crate::goal;
use crate::money::{SYMBOL, Symbol};
use crate::panel::exhibit::{Clock, type_at};
use crate::panel::follow::set_follow_icon;
use crate::staff;
use crate::status::Words;
use crate::text::format::{self, Arg};
use crate::text::to_cp1252;
use crate::ui::interface::Interface;
use crate::ui::widgets::Row;
use crate::unit::{Kind, Part, Unit};
use crate::vfs::Vfs;
use crate::world::World;

/// The layout of the panel, the name field, the buttons (follow, pick up, fire, assign mode) and the
/// tab buttons: status, the exhibits of a keeper or guide, the duties of a worker.
pub const LAYOUT: i32 = 17;
pub const NAME: i32 = 3502;
pub const FOLLOW: i32 = 3515;
pub const PICK_UP: i32 = 3570;
pub const FIRE: i32 = 3606;
pub const ASSIGN: i32 = 3578;
pub const TABS: [i32; 3] = [3562, 3564, 3565];
/// The exhibits list with its two buttons, and the layout of its tab.
pub const LIST: i32 = 3569;
pub const REMOVE: i32 = 3579;
pub const REMOVE_ALL: i32 = 3580;
pub const EXHIBITS_LAYOUT: i32 = 601;
/// The list of the worker's duty switches.
pub const DUTIES: i32 = 3587;
/// The training icons, one per bit of the type's training mask (research 6.5).
pub const ICONS: [i32; 5] = [3581, 3582, 3583, 3584, 3585];
const SALARY: i32 = 3503;
const DUTY: i32 = 3505;
/// The string of the first duty kind, and the pictures of a check box off and on.
const DUTY_STRING: i32 = 3530;
const CHECKS: [&str; 2] = [
    "ui/sharedui/checkbx1/checkbx1",
    "ui/sharedui/checkbx2/checkbx2",
];
/// The interface clock time between two refreshes (ms).
pub const PERIOD: u64 = 1000;

/// The tasks the job text looks at: go to an exhibit, enter one, leave one, and the guide's two.
const EXIT: u8 = 127;
const GUIDE_VIEW: u8 = 118;
const GUIDE_TALK: u8 = 119;
/// The jobs of a keeper: put food, heal, rake dung, clean a tank.
const FOOD: [u8; 2] = [1, 2];
const HEAL: [u8; 2] = [3, 4];
const DUNG: [u8; 2] = [5, 6];
const TANK: [u8; 2] = [24, 25];
/// The strings of the keeper's texts.
const PLACING: i32 = 10300;
const HEALING: i32 = 10301;
const RAKING: i32 = 10302;
const GOING: i32 = 10303;
const MONITORING: i32 = 10304;
const EXITING: i32 = 10306;
const EGGS: i32 = 10321;
const CLEANING_TANK: i32 = 10327;
const CLEANING_POO: i32 = 10514;
/// The scientist's type id, which has a text of its own for raking.
const SCIENTIST: i32 = 9551;
/// The worker's texts by job, and when it has none the text of the wander.
const WORKER_JOBS: [(u8, i32); 10] = [
    (7, 10500),
    (8, 10501),
    (9, 10502),
    (10, 10503),
    (11, 10504),
    (12, 10505),
    (16, 10509),
    (17, 10510),
    (26, 10515),
    (27, 10516),
];
const WORKER_MONITORING: i32 = 10506;
/// The guide's texts: going to an exhibit, speaking, monitoring.
const GUIDE_GOING: i32 = 10400;
const SPEAKING: i32 = 10401;
const GUIDE_MONITORING: i32 = 10402;

/// What the exhibit of a tile is called in the job text.
pub enum Place {
    /// No exhibit: the tile is outside the zoo or there is no tile.
    None,
    Grounds,
    Exhibit(String),
}

/// The job text of a staff member (staff 9): the string and the name that goes in its `%s`;
/// `None` for an empty text. `at` gives the place of a tile, `animal` whether an entity is an
/// animal and then an egg. The shows and the captures are M7's.
pub fn duty(
    unit: &Unit,
    type_id: i32,
    at: impl Fn(Option<usize>) -> Place,
    animal: impl Fn(u32) -> Option<bool>,
) -> Option<(i32, Option<String>)> {
    let r = &unit.goal.record;
    let plain = |id| Some((id, None));
    let going = |id| match at(r.target.tile) {
        Place::None => None,
        Place::Grounds => plain(EXITING),
        Place::Exhibit(name) => Some((id, Some(name))),
    };
    match unit.kind {
        Kind::Keeper => match (r.task, r.job) {
            (_, j) if FOOD.contains(&j) => plain(PLACING),
            (_, j) if HEAL.contains(&j) => {
                animal(r.job_target.id).map(|egg| (if egg { EGGS } else { HEALING }, None))
            }
            (_, j) if DUNG.contains(&j) => plain(if type_id == SCIENTIST {
                CLEANING_POO
            } else {
                RAKING
            }),
            (_, j) if TANK.contains(&j) => plain(CLEANING_TANK),
            (goal::HABITAT | 126, _) => going(GOING),
            (EXIT, _) => plain(EXITING),
            (goal::RANDOM, _) => plain(MONITORING),
            _ => None,
        },
        Kind::Worker => match WORKER_JOBS.iter().find(|&&(j, _)| j == r.job) {
            Some(&(_, id)) => plain(id),
            None if r.job == 0 && r.task == goal::RANDOM => plain(WORKER_MONITORING),
            None => None,
        },
        // The original reads the viewing area at the target tile; the exhibit of the tile is
        // what that gives. Whether the guide is speaking is its task.
        Kind::Guide => match r.task {
            GUIDE_VIEW => match at(r.target.tile) {
                Place::Exhibit(name) => Some((GUIDE_GOING, Some(name))),
                _ => None,
            },
            GUIDE_TALK => plain(SPEAKING),
            _ => plain(GUIDE_MONITORING),
        },
        _ => None,
    }
}

/// The selected staff member and the refresh clock.
#[derive(Debug, Default)]
pub struct Panel {
    /// The selected staff member, by entity id.
    pub selected: Option<u32>,
    pub clock: Clock,
    /// The exhibit under the pointer in the assign mode, and whether the keeper may serve it.
    pub over: Option<(u32, bool)>,
    /// The assign button was down at the last look.
    pub assigning: bool,
    /// Just selected: the next refresh also sets the name field and the follow button.
    fresh: bool,
    told: String,
}

/// The values of a repeated key of a unit type's section, taken from its subtype or else from its
/// default subtype (entity types 6.2).
fn list(ty: &EntityType, section: &str, key: &str) -> Vec<Vec<u8>> {
    let Some(config) = ty.config.as_deref() else {
        return Vec::new();
    };
    let default = config
        .section("Global")
        .and_then(|g| g.get("DefaultSubtype"));
    let lists = [&ty.subtype[..], default.unwrap_or_default()].map(|prefix| {
        let name = [prefix, b"/", section.as_bytes()].concat();
        config
            .section(name)
            .map_or(Vec::new(), |s| s.get_all(key).to_vec())
    });
    lists
        .into_iter()
        .find(|l| !l.is_empty())
        .unwrap_or_default()
}

impl Panel {
    /// Shows icon `i` of the type's `cTrainingIconName` with the tooltip `i` of `cTrainingTooltip`
    /// when bit `i` of the training mask is set, else nothing (information panels 9.1). The text
    /// for the log line: the icons shown.
    fn training(
        &self,
        ui: &mut Interface,
        vfs: &Vfs,
        ty: Option<&EntityType>,
        mask: u16,
    ) -> String {
        let icons = ty.map_or(Vec::new(), |t| {
            list(t, "Characteristics/Strings", "cTrainingIconName")
        });
        let tips = ty.map_or(Vec::new(), |t| {
            list(t, "Characteristics/Integers", "cTrainingTooltip")
        });
        let mut shown = String::new();
        for (i, &element) in ICONS.iter().enumerate() {
            let name = icons.get(i).filter(|_| mask >> i & 1 == 1);
            let name = name.map(|n| String::from_utf8_lossy(n.trim_ascii()).into_owned());
            let picture = name.as_deref().and_then(|n| ui.tree.picture(vfs, n));
            if picture.is_some() {
                shown += &format!(" icon{i}");
            }
            ui.set_picture(element, picture);
            let tip = tips
                .get(i)
                .and_then(|t| to_int(t))
                .filter(|_| picture.is_some());
            if let Some(e) = ui.tree.find(element) {
                ui.tree.elements[e].help_id = tip.into_iter().collect();
            }
        }
        shown
    }

    /// Selects the staff member with entity id `id`: the panel is filled at once (4.3).
    pub fn select(&mut self, id: u32) {
        self.selected = Some(id);
        self.fresh = true;
        self.clock.force = true;
        self.told.clear();
    }

    /// Fills the panel again from the start: the name field shows the staff member's name.
    pub fn reselect(&mut self) {
        if let Some(id) = self.selected {
            self.select(id);
        }
    }

    pub fn deselect(&mut self) {
        self.selected = None;
        self.over = None;
        self.told.clear();
    }

    /// Writes the panel for the selected staff member and returns the log line when something it
    /// shows changed. `placing`: the map view holds a pending entity (the assign button is off).
    pub fn refresh(
        &mut self,
        ui: &mut Interface,
        world: &World,
        cat: &Catalogue,
        vfs: &Vfs,
        placing: bool,
    ) -> Option<String> {
        let id = self.selected?;
        let (at, unit) = (world.find(id)?, world.units.get(id)?);
        let Part::Staff(staff) = &unit.part else {
            return None;
        };
        let ty = type_at(world, cat, at);
        let money = Symbol::new(ui.string(SYMBOL));
        let salary = money.text(ty.map_or(0.0, staff::monthly).round() as i64);
        ui.set_text(SALARY, &to_cp1252(&salary));
        let place = |tile: Option<usize>| {
            let region = tile.and_then(|t| world.exhibits().region(t));
            match region.and_then(|r| world.exhibits().get(r)) {
                Some(x) => Place::Exhibit(x.name.clone()),
                None if region == Some(crate::exhibit::GROUNDS) => Place::Grounds,
                None => Place::None,
            }
        };
        let egg = |id: u32| match &world.units.get(id)?.part {
            Part::Animal(a) => Some(a.egg),
            _ => None,
        };
        let text = duty(unit, ty.map_or(0, |t| t.name_id), place, egg).map_or(
            String::new(),
            |(s, name)| {
                let none = Words::new(|_| String::new());
                let args = name.iter().map(|n| Arg::Text(n)).collect::<Vec<_>>();
                format::expand(&none, &ui.string(s), &args)
            },
        );
        ui.set_text(DUTY, &to_cp1252(&text));
        let name = world.zoo.entities[at]
            .common
            .as_ref()
            .map_or("", |c| &c.name);
        let fresh = std::mem::take(&mut self.fresh);
        if fresh {
            ui.set_text(NAME, &to_cp1252(name));
            ui.set_button(FOLLOW, false, !unit.carried);
        }
        if fresh || self.clock.forced {
            set_follow_icon(ui, vfs, FOLLOW, ty);
        }
        for button in [PICK_UP, FIRE] {
            ui.set_enabled(button, !unit.carried);
        }
        ui.set_enabled(ASSIGN, !unit.carried && !placing);
        let trained = self.training(ui, vfs, ty, world.research.training_of(ty));
        let told = match unit.kind {
            Kind::Worker => {
                let kinds = staff::duty_kinds(cat);
                let rows = (0..kinds)
                    .map(|k| Row {
                        text: to_cp1252(&ui.string(DUTY_STRING + k as i32)),
                        icon: ui.tree.picture(vfs, CHECKS[usize::from(!staff.off[k])]),
                        ..Row::default()
                    })
                    .collect();
                ui.refill_rows(DUTIES, rows);
                let on: String = (0..kinds)
                    .map(|k| if staff.off[k] { '0' } else { '1' })
                    .collect();
                format!("duties {on}")
            }
            _ => {
                let names: Vec<String> = (staff.assigned.iter())
                    .filter_map(|&e| world.exhibits().get(e))
                    .map(|x| x.name.clone())
                    .collect();
                let keep = ui.selected_row(LIST);
                let rows = (names.iter())
                    .map(|n| Row {
                        text: to_cp1252(n),
                        ..Row::default()
                    })
                    .collect();
                ui.refill_rows(LIST, rows);
                ui.select_row(LIST, keep.filter(|&r| r < names.len()));
                ui.set_enabled(REMOVE, ui.selected_row(LIST).is_some());
                format!("exhibits {names:?}")
            }
        };
        let told = format!("staff {id} {name:?} salary {salary} duty {text:?}{trained} {told}");
        match told == self.told {
            true => None,
            false => {
                self.told.clone_from(&told);
                Some(told)
            }
        }
    }
}

#[cfg(test)]
mod tests;
