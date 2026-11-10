//! The animal, guest and staff list windows (information panels 20): what each list holds, its
//! rows, the filters with their counts, titles and colours, and the refill every 10 s of the
//! interface clock. [`Window`] keeps one list; the app shows and hides them, reads the buttons
//! and the row clicks ([`crate::app`]).
//!
//! The list box holds one row per entry in the list's order, the rows the current filter fails
//! hidden. The animals are sorted by name (then entity id), the guests and the staff are in the
//! entity list's order; the helicopter's row shows its base's name.

use std::collections::HashMap;

use crate::animal;
use crate::entity::Catalogue;
use crate::exhibit::members::{animals_exhibit, suitability_in, type_at};
use crate::guest;
use crate::text::to_cp1252;
use crate::ui::interface::Interface;
use crate::ui::layout::Rgb;
use crate::ui::widgets::Row;
use crate::unit::{Kind, Part};
use crate::vfs::Vfs;
use crate::world::World;

/// The interface clock time between two refills (ms).
pub(crate) const PERIOD: u64 = 10_000;
/// The colour of the count of the filter in use.
const WHITE: Rgb = [255; 3];

pub(crate) const ANIMALS: usize = 0;
pub(crate) const GUESTS: usize = 1;
pub(crate) const STAFF: usize = 2;

/// A filter: its button, its count text and the string of its title.
struct Filter {
    button: i32,
    count: i32,
    title: i32,
}

const fn f(button: i32, count: i32, title: i32) -> Filter {
    Filter {
        button,
        count,
        title,
    }
}

/// A window: its layout, the main bar button that shows it, its list box and title text, and
/// its filters in the order of their tests.
pub(crate) struct Spec {
    pub layout: i32,
    pub button: i32,
    pub list: i32,
    title: i32,
    filters: &'static [Filter],
}

pub(crate) const SPECS: [Spec; 3] = [
    Spec {
        layout: 16,
        button: 1010,
        list: 3805,
        title: 3835,
        filters: &[
            f(3815, 3816, 3825),
            f(3817, 3818, 3826),
            f(3819, 3820, 3827),
            f(3821, 3822, 3828),
            f(3823, 3824, 3829),
            f(3838, 3839, 3840),
        ],
    },
    Spec {
        layout: 12,
        button: 1012,
        list: 3705,
        title: 3727,
        filters: &[
            f(3715, 3716, 3727),
            f(3717, 3718, 3728),
            f(3719, 3720, 3729),
            f(3721, 3722, 3730),
            f(3723, 3724, 3731),
            f(3725, 3726, 3732),
        ],
    },
    Spec {
        layout: 31,
        button: 1051,
        list: 4205,
        title: 4235,
        filters: &[
            f(4215, 4216, 4232),
            f(4217, 4218, 4233),
            f(4219, 4220, 4234),
            f(4221, 4222, 4235),
            f(4223, 4224, 4236),
            f(4225, 4226, 4237),
            f(4240, 4241, 4241),
            f(4242, 4243, 4243),
            f(4244, 4245, 4245),
        ],
    },
];

/// The layouts a shown window hides: the zoo status panels and the show panel; the guest and
/// the staff windows also hide the species and the recommendations plaques.
pub(crate) const HIDES: [i32; 3] = [14, 803, 41];
pub(crate) const PLAQUES: [i32; 2] = [27, 28];

/// Whether one of the windows is shown: the unit panels then stand above it (4.2).
pub(crate) fn any_shown(ui: &Interface) -> bool {
    SPECS.iter().any(|s| ui.shown(s.layout))
}

/// The window whose list box is `id`.
pub(crate) fn list_of(id: i32) -> Option<usize> {
    SPECS.iter().position(|s| s.list == id)
}

/// The buttons of every filter.
pub(crate) fn buttons() -> Vec<i32> {
    SPECS
        .iter()
        .flat_map(|s| s.filters.iter().map(|f| f.button))
        .collect()
}

/// The number of filters of window `which`.
pub(crate) fn filters(which: usize) -> usize {
    SPECS[which].filters.len()
}

/// The button of filter `i` of window `which`.
pub(crate) fn button(which: usize, i: usize) -> i32 {
    SPECS[which].filters[i].button
}

/// The window whose filter button is `id`, and the filter.
pub(crate) fn button_of(id: i32) -> Option<(usize, usize)> {
    SPECS.iter().enumerate().find_map(|(w, s)| {
        let i = s.filters.iter().position(|f| f.button == id)?;
        Some((w, i))
    })
}

/// One unit of a list.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Entry {
    pub id: u32,
    pub at: usize,
    pub name: Vec<u8>,
}

/// The units of window `which`, in the order of its list (20.2): the animals sorted by name, a
/// dying one left out; the guests and the staff as the entity list has them.
pub(crate) fn entries(which: usize, world: &World) -> Vec<Entry> {
    let wanted = |k: Kind| match which {
        ANIMALS => k == Kind::Animal,
        GUESTS => k == Kind::Guest,
        _ => matches!(k, Kind::Keeper | Kind::Worker | Kind::Guide | Kind::Heli),
    };
    let mut list: Vec<Entry> = (world.zoo.entities.iter().enumerate())
        .filter_map(|(at, e)| {
            let id = e.common.as_ref()?.id;
            let unit = world.units.get(id).filter(|u| wanted(u.kind))?;
            if matches!(&unit.part, Part::Animal(a) if a.dying) {
                return None;
            }
            let name = e.common.as_ref().map_or(Vec::new(), |c| to_cp1252(&c.name));
            Some(Entry { id, at, name })
        })
        .collect();
    if which == ANIMALS {
        // The bytes of the names as unsigned values, the shorter first, then the id.
        list.sort_by(|a, b| a.name.cmp(&b.name).then(a.id.cmp(&b.id)));
    }
    list
}

/// What the tests of the filters read of the world, with the ratings they computed.
struct Tests<'a> {
    world: &'a World,
    cat: &'a Catalogue,
    /// `cUIThreshold` of the animal class and of the guest class.
    animal: i32,
    guest: i32,
    ratings: HashMap<(u32, String), i32>,
}

impl<'a> Tests<'a> {
    fn new(world: &'a World, cat: &'a Catalogue, first: Option<&Entry>) -> Tests<'a> {
        let guest = first
            .filter(|_| world.units.iter().any(|u| u.kind == Kind::Guest))
            .map_or(50, |e| {
                crate::status::zoo::ui_threshold(cat, &world.zoo.entities[e.at])
            });
        Tests {
            world,
            cat,
            animal: world.units.class.ui_threshold,
            guest,
            ratings: HashMap::new(),
        }
    }

    /// Whether the unit passes filter `i` of window `which` (20.4).
    fn pass(&mut self, which: usize, i: usize, e: &Entry) -> bool {
        if i == 0 {
            return true;
        }
        let Some(unit) = self.world.units.get(e.id) else {
            return false;
        };
        match (&unit.part, which) {
            (Part::Animal(a), ANIMALS) => {
                let low = |t: i32| t.saturating_mul(2).saturating_sub(201);
                match i {
                    1 => a.happiness <= low(self.animal),
                    2 => a.at_minimum_health,
                    3 => a.hunger > self.animal,
                    4 => !unit.carried && !a.egg && self.unhappy(e),
                    _ => a.escaped,
                }
            }
            (Part::Guest(g), GUESTS) => {
                let t = self.guest;
                match i {
                    1 => g.values[guest::HAPPINESS] <= t.saturating_mul(2).saturating_sub(201),
                    2 => g.values[guest::THIRST] > t,
                    3 => g.values[guest::HUNGER] > t,
                    4 => g.values[guest::BATHROOM] > t,
                    _ => g.values[guest::ENERGY] > t,
                }
            }
            (Part::Staff(s), STAFF) => {
                let name = self.world.zoo.entities[e.at].type_name.as_str();
                let kind = |k: Kind| unit.kind == k;
                let ordinary = kind(Kind::Keeper) || kind(Kind::Guide);
                match i {
                    1 => name == "keeper",
                    2 => name == "tour",
                    3 => name == "maint",
                    4 => ordinary && !s.assigned.is_empty(),
                    5 => ordinary && s.assigned.is_empty(),
                    6 => name == "scient",
                    7 => name == "helicopter",
                    _ => name == "trainer",
                }
            }
            _ => false,
        }
    }

    /// The rating the animal shows (0 on the grounds) is below its species' preference.
    fn unhappy(&mut self, e: &Entry) -> bool {
        let (world, cat) = (self.world, self.cat);
        let key = &world.zoo.entities[e.at].type_name;
        let rating = match animals_exhibit(world, e.at) {
            None => 0,
            Some(x) => *(self.ratings.entry((x, key.clone())))
                .or_insert_with(|| suitability_in(world, cat, x, key)),
        };
        rating < animal::keys(world, cat, e.at).habitat_preference
    }
}

/// The list picture of a type (`cListImageName`): the section of its subtype, then of the
/// default subtype, then the plain one.
fn list_picture(ty: &crate::entity::EntityType) -> Option<String> {
    let config = ty.config.as_deref()?;
    let default = config
        .section("Global")
        .and_then(|g| g.get("DefaultSubtype"));
    let sections = [
        [&ty.subtype[..], b"/Characteristics/Strings"].concat(),
        [default.unwrap_or_default(), b"/Characteristics/Strings"].concat(),
        b"Characteristics/Strings".to_vec(),
    ];
    sections.into_iter().find_map(|name| {
        let value = config.section(name)?.get("cListImageName")?;
        Some(String::from_utf8_lossy(value.trim_ascii()).into_owned())
    })
}

/// The row of a unit: its name and the picture of its type.
fn row_of(e: &Entry, (world, cat, vfs): (&World, &Catalogue, &Vfs), ui: &mut Interface) -> Row {
    Row {
        text: e.name.clone(),
        icon: type_at(world, cat, e.at)
            .and_then(list_picture)
            .and_then(|n| ui.tree.picture(vfs, &n)),
        ..Row::default()
    }
}

/// Keeps the items whose flag is set.
fn keep_only<T>(items: &mut Vec<T>, keep: &[bool]) {
    let mut flags = keep.iter();
    items.retain(|_| flags.next().copied().unwrap_or(true));
}

/// One window's state: the filter, what the last refill held, and the refresh clock.
#[derive(Debug, Default)]
pub(crate) struct Window {
    /// The filter in use, kept for the whole session (20.1 point 3).
    pub filter: usize,
    /// The layout was shown at the last look.
    pub shown: bool,
    /// A refill is asked for: by the first look at a shown window, a rename.
    pub force: bool,
    /// A refill is due now whatever the clock says: the window was just shown or a filter chosen.
    pub now: bool,
    due: u64,
    /// The entity of each row, and the name each had.
    pub ids: Vec<u32>,
    /// Which rows are the helicopter's.
    helis: Vec<bool>,
    /// A refill ran once: before it the list box has no rows to select.
    pub filled: bool,
    names: Vec<Vec<u8>>,
    rows: Vec<Row>,
    hidden: Vec<bool>,
    /// The text colour each count had when the interface was loaded.
    colours: Vec<Option<Rgb>>,
    /// The selected entity at the last look.
    pub seen: Option<u32>,
}

impl Window {
    /// The unit of row `row`.
    pub(crate) fn unit_of(&self, row: usize) -> Option<u32> {
        self.ids.get(row).copied()
    }

    /// Whether the refill is due at `now` (20.7 point 3): asked for or never run, or its time came;
    /// the clock only ticks with the interface update, and not while the game is paused.
    pub(crate) fn due(&mut self, now: u64, ticking: bool) -> bool {
        let run = std::mem::take(&mut self.now) || (ticking && (self.force || self.due <= now));
        if run {
            (self.due, self.force) = (now.saturating_add(PERIOD), false);
        }
        run
    }

    /// Whether the units of the list changed against `list` (an addition, a removal or a move).
    pub(crate) fn changed(&self, list: &[Entry]) -> bool {
        self.ids.len() != list.len() || self.ids.iter().zip(list).any(|(&id, e)| id != e.id)
    }

    /// A unit was renamed.
    pub(crate) fn renamed(&self, list: &[Entry]) -> bool {
        self.names.iter().zip(list).any(|(n, e)| *n != e.name)
    }

    /// Chooses filter `i` (20.5 points 1 to 3): the old count gets its own colour back, the new
    /// one white, and the title is the filter's. The refill is [`Window::now`].
    pub(crate) fn choose(&mut self, which: usize, i: usize, ui: &mut Interface) {
        let spec = &SPECS[which];
        if self.colours.is_empty() {
            self.colours = spec
                .filters
                .iter()
                .map(|f| ui.fore_color(f.count))
                .collect();
        }
        let i = i.min(spec.filters.len() - 1);
        if let Some(c) = self.colours.get(self.filter).copied().flatten() {
            ui.set_fore_color(spec.filters[self.filter].count, c);
        }
        self.filter = i;
        ui.set_fore_color(spec.filters[i].count, WHITE);
        let title = ui.string(spec.filters[i].title);
        ui.set_text(spec.title, &to_cp1252(&title));
        self.now = true;
    }

    /// The refill (20.3): the rows, which the filter hides, the counts, and the selection of
    /// the row of `selected`; the first row shown is kept.
    pub(crate) fn refill(
        &mut self,
        which: usize,
        list: Vec<Entry>,
        (ui, world, cat, vfs): (&mut Interface, &World, &Catalogue, &Vfs),
        selected: Option<u32>,
    ) {
        let spec = &SPECS[which];
        let mut tests = Tests::new(world, cat, list.first());
        let rebuild = which == ANIMALS || self.changed(&list);
        if rebuild {
            self.rows = (list.iter())
                .map(|e| row_of(e, (world, cat, vfs), ui))
                .collect();
            self.hidden = vec![false; list.len()];
        }
        // The unit each row stands for in the selection: a helicopter's base, when it shows it.
        let mut targets: Vec<u32> = list.iter().map(|e| e.id).collect();
        let mut helis = vec![false; list.len()];
        let mut counts = vec![0; spec.filters.len()];
        for (k, e) in list.iter().enumerate() {
            let results: Vec<bool> = (0..counts.len()).map(|i| tests.pass(which, i, e)).collect();
            counts
                .iter_mut()
                .zip(&results)
                .for_each(|(c, &r)| *c += i32::from(r));
            let passes = results.get(self.filter).copied().unwrap_or(true);
            let heli = (world.units.get(e.id)).and_then(|u| match &u.part {
                Part::Staff(s) if u.kind == Kind::Heli => Some(s.heli.base),
                _ => None,
            });
            let base = heli
                .and_then(|b| world.find(b))
                .filter(|&b| world.zoo.entities[b].class == "building");
            helis[k] = heli.is_some();
            match (heli.is_some(), passes, base) {
                // The helicopter's row shows its base; with none it stays as it was (20.3 point 4).
                (true, true, Some(b)) => {
                    let c = world.zoo.entities[b].common.as_ref();
                    self.rows[k].text = c.map_or(Vec::new(), |c| to_cp1252(&c.name));
                    self.hidden[k] = false;
                    targets[k] = c.map_or(e.id, |c| c.id);
                }
                (true, true, None) => {}
                _ => {
                    self.rows[k].text.clone_from(&e.name);
                    self.hidden[k] = !passes;
                }
            }
        }
        // The row selected before the refill, by its unit: a refill of the guest and staff lists
        // leaves the selection as it is (20.3 point 3), the helicopter's row included.
        let kept = (ui.selected_row(spec.list)).and_then(|r| self.ids.get(r).copied());
        self.ids = list.iter().map(|e| e.id).collect();
        self.helis = helis;
        self.names = list.into_iter().map(|e| e.name).collect();
        let row = (0..self.ids.len())
            .find(|&k| !self.hidden[k] && Some(targets[k]) == selected)
            .filter(|&k| targets[k] == self.ids[k] || kept == Some(self.ids[k]));
        let top = ui.list_top(spec.list);
        ui.fill_rows(
            spec.list,
            self.rows.clone(),
            self.hidden.clone(),
            (row, top),
        );
        // A unit selected while the list was never shown has its row in view when it opens.
        if !std::mem::replace(&mut self.filled, true) && row.is_some() {
            ui.select_row(spec.list, row);
        }
        for (f, n) in spec.filters.iter().zip(counts) {
            ui.set_text(f.count, n.to_string().as_bytes());
        }
    }

    /// Rows that came or went outside a refill (20.11 points 2 and 4): the rows of the units no
    /// longer in `list` are removed and a new unit's row is appended, visible when it passes the
    /// filter. The counts and the other rows stay; the selection and the first shown row are row
    /// numbers that follow the removals (20.11 point 8); the refill is asked for.
    pub(crate) fn edit(
        &mut self,
        which: usize,
        list: &[Entry],
        (ui, world, cat, vfs): (&mut Interface, &World, &Catalogue, &Vfs),
    ) {
        let spec = &SPECS[which];
        let keep: Vec<bool> = (self.ids.iter())
            .map(|id| list.iter().any(|e| e.id == *id))
            .collect();
        // The selection and the first shown row are row numbers (20.11 point 8): a row removed
        // above one lowers it, the selected row's own removal leaves the number on the next row.
        let (mut row, mut top) = (ui.selected_row(spec.list), ui.list_top(spec.list));
        let mut gone = 0;
        for k in (0..keep.len()).filter(|&k| !keep[k]) {
            let r = k - gone;
            gone += 1;
            row = row.map(|s| s - usize::from(r < s));
            top -= usize::from(r < top);
        }
        keep_only(&mut self.ids, &keep);
        keep_only(&mut self.names, &keep);
        keep_only(&mut self.helis, &keep);
        keep_only(&mut self.rows, &keep);
        keep_only(&mut self.hidden, &keep);
        let mut tests = Tests::new(world, cat, list.first());
        let old = self.ids.clone();
        for e in list.iter().filter(|e| !old.contains(&e.id)) {
            self.hidden.push(!tests.pass(which, self.filter, e));
            self.rows.push(row_of(e, (world, cat, vfs), ui));
            self.helis
                .push(world.units.get(e.id).is_some_and(|u| u.kind == Kind::Heli));
            self.names.push(e.name.clone());
            self.ids.push(e.id);
        }
        ui.fill_rows(
            spec.list,
            self.rows.clone(),
            self.hidden.clone(),
            (row, top),
        );
        self.force = true;
    }

    /// The selected entity changed (20.6 point 5): the old unit's list is unselected and the new
    /// one's row selected and scrolled into view, whether the window is shown or not. Only a staff
    /// member's own row follows, not its base.
    pub(crate) fn follow(&mut self, which: usize, ui: &mut Interface, selected: Option<u32>) {
        let old = std::mem::replace(&mut self.seen, selected);
        if old == selected {
            return;
        }
        let spec = &SPECS[which];
        let row_of = |this: &Window, id: Option<u32>| {
            id.and_then(|id| this.ids.iter().position(|&i| i == id))
        };
        // The old animal, guest or staff member clears the whole selection of its list; the
        // helicopter and what is not in the list leave it alone.
        if row_of(self, old).is_some_and(|r| !self.helis[r]) {
            ui.select_row(spec.list, None);
        }
        let new = row_of(self, selected).filter(|&r| !self.hidden[r]);
        if new.is_some() {
            ui.select_row(spec.list, new);
        }
    }
}

/// The three windows.
#[derive(Debug, Default)]
pub(crate) struct Lists {
    pub windows: [Window; 3],
}

#[cfg(test)]
mod tests;
