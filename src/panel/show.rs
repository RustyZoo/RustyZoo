//! The show panel (information panels 19, shows 33; Marine Mania): a centred window with the
//! exhibit's name, the admission of the selected species, the species drop-down and two tabs, the
//! script tab (the frequency drop-down and the trick editor, `editor`) and the finances tab.
//!
//! [`Panel`] holds the exhibit `X` and the species `s` of shows 33.1 and fills the layouts from the
//! world; the admission and the frequency write into it. What shows and hides the window is
//! [`crate::app`]'s.

use crate::config::to_double;
use crate::entity::Catalogue;
use crate::money::{SYMBOL, Symbol};
use crate::panel::exhibit::Clock;
use crate::show::{self, KEEPER, admission};
use crate::text::{format, to_cp1252};
use crate::timing::file_time;
use crate::ui::interface::Interface;
use crate::ui::widgets::Row;
use crate::vfs::Vfs;
use crate::world::{World, filter_months_since};

// The window, its tabs and the two drop-down layouts.
pub(crate) const LAYOUT: i32 = 41;
pub(crate) const SCRIPT_TAB: i32 = 11126;
pub(crate) const FINANCES_TAB: i32 = 11127;
pub(crate) const SPECIES_LAYOUT: i32 = 45;
pub(crate) const FREQUENCY_LAYOUT: i32 = 44;
// The title, the admission field and its buttons, the species and the frequency controls.
const TITLE: i32 = 11102;
pub(crate) const ADMISSION: i32 = 11104;
pub(crate) const UP: i32 = 11105;
pub(crate) const DOWN: i32 = 11106;
pub(crate) const SPECIES_BUTTON: i32 = 11123;
pub(crate) const SPECIES_LIST: i32 = 11196;
pub(crate) const FREQUENCY_BUTTON: i32 = 11108;
pub(crate) const FREQUENCY_LIST: i32 = 11191;
/// The strings of the four frequencies, Never first.
const FREQUENCY_NAMES: i32 = 11192;
/// The trick editor's layout, buttons, lists and benefit text and face (19.9).
const SCRIPT_LAYOUT: i32 = 42;
pub(crate) const ADD: i32 = 11113;
pub(crate) const REMOVE: i32 = 11114;
pub(crate) const EARLIER: i32 = 11117;
pub(crate) const LATER: i32 = 11118;
pub(crate) const AVAILABLE: i32 = 11111;
pub(crate) const SHOWN: i32 = 11115;
const TRICK_BUTTONS: [i32; 4] = [ADD, REMOVE, EARLIER, LATER];
const TRICK_LISTS: [i32; 2] = [AVAILABLE, SHOWN];
const BENEFIT: i32 = 11120;
const FACE: i32 = 11121;
/// The first of the sixteen cells of the finances (attendance, income, upkeep, profit; current,
/// last, average, total), and the months in operation.
const CELLS: i32 = 11140;
const MONTHS: i32 = 11173;
/// The cell of the income, whose colour the positive profits take, and the colours of the others.
const INCOME: i32 = 11144;
const LOSS: [u8; 3] = [250, 7, 7];
const BREAK_EVEN: [u8; 3] = [255, 228, 173];
/// The interface clock time between two refreshes (ms).
pub(crate) const PERIOD: u64 = 10_000;

mod editor;

/// The frequencies of the four rows, in AI ticks between two shows (-1 never).
fn frequencies(world: &World) -> [i32; 4] {
    let [a, b, c] = world.units.show.intervals;
    [-1, a, b, c]
}

/// The number typed into the admission field (shows 33.5, information panels 19.6): the first
/// currency `symbol` is cut, then every `group` separator, and the rest is read as `atof` does.
pub(crate) fn typed(text: &[u8], symbol: &[u8], group: &[u8]) -> f64 {
    let mut text = text.to_vec();
    let find = |text: &[u8], part: &[u8]| text.windows(part.len()).position(|w| w == part);
    if !symbol.is_empty()
        && let Some(at) = find(&text, symbol)
    {
        text.drain(at..at + symbol.len());
    }
    while !group.is_empty()
        && let Some(at) = find(&text, group)
    {
        text.drain(at..at + group.len());
    }
    to_double(&text)
}

/// Whether `t` is the type id of an animal type.
fn is_animal(cat: &Catalogue, t: u32) -> bool {
    (cat.types.iter()).any(|x| x.class == "animals" && u32::try_from(x.name_id) == Ok(t))
}

/// The types of the species list of show tank `x` (19.5 point 3): those of its records with
/// units, but the type 0 and the marine specialist, in increasing type id and with a name.
pub(crate) fn species_of(ui: &Interface, world: &World, x: u32) -> Vec<u32> {
    let Some(info) = crate::show::info_of(world, x) else {
        return Vec::new();
    };
    let mut types: Vec<u32> = (info.records.iter())
        .filter(|r| r.key != 0 && r.key != KEEPER && !r.units.is_empty())
        .map(|r| r.key)
        .filter(|&t| !ui.string(t as i32).is_empty())
        .collect();
    types.sort_unstable();
    types
}

/// What the panel holds: the exhibit and the species it shows, and its refresh clock.
#[derive(Debug, Default)]
pub(crate) struct Panel {
    pub exhibit: Option<u32>,
    pub species: Option<u32>,
    /// The type id of each row of the species list.
    pub rows: Vec<u32>,
    /// The frequency is Never: the admission, species and trick controls are off.
    disabled: bool,
    /// The trick lists and the flag of a change not written yet.
    editor: editor::Editor,
    /// The grandstand that opened the panel on the finances tab: deselecting it closes it.
    pub stand: Option<u32>,
    pub clock: Clock,
    /// The window was shown at the last look.
    pub shown: bool,
    /// Which of the panels that hide the window were shown at the last look.
    pub seen: Vec<bool>,
    /// What the last refresh told.
    told: Vec<String>,
}

impl Panel {
    /// `set_exhibit(E)` (19.4): the panel shows exhibit `x`; none hides the window.
    pub(crate) fn set_exhibit(
        &mut self,
        ui: &mut Interface,
        world: &mut World,
        cat: &Catalogue,
        vfs: &Vfs,
        x: Option<u32>,
    ) {
        self.exhibit = x;
        self.told.clear();
        self.clock.force = true;
        self.fill_exhibit(ui, world, cat, vfs);
        if x.is_none() {
            ui.set_hidden(LAYOUT, true);
        }
    }

    /// `fill_exhibit()` (19.4): no species, the trick editor empty, the frequency of the exhibit
    /// selected, the species list collected and its first species selected.
    fn fill_exhibit(&mut self, ui: &mut Interface, world: &mut World, cat: &Catalogue, vfs: &Vfs) {
        self.species = None;
        self.update_admission(ui, world, cat);
        self.clear_tricks(ui);
        ui.set_text(BENEFIT, b"");
        ui.set_image(FACE, 1);
        let Some(x) = self.exhibit else { return };
        // The four rows; the last, Continuous, is selected until an exhibit says otherwise.
        // The first time, the button says Never (19.7 point 1).
        if ui.selected_row(FREQUENCY_LIST).is_none() {
            ui.set_text(FREQUENCY_BUTTON, &to_cp1252(&ui.string(FREQUENCY_NAMES)));
        }
        let kept = ui.selected_row(FREQUENCY_LIST).unwrap_or(3);
        let names = (0..4).map(|n| Row {
            text: to_cp1252(&ui.string(FREQUENCY_NAMES + n)),
            ..Row::default()
        });
        ui.set_rows(FREQUENCY_LIST, names.collect());
        ui.select_row(FREQUENCY_LIST, Some(kept));
        let frequency = show::info_of(world, x).map(|i| i.frequency);
        let row = frequency.and_then(|f| frequencies(world).iter().position(|&v| v == f));
        if let Some(r) = row {
            ui.select_row(FREQUENCY_LIST, Some(r));
            let text = ui.row_text(FREQUENCY_LIST, r).unwrap_or_default().to_vec();
            ui.set_text(FREQUENCY_BUTTON, &text);
        }
        let first = self.collect_species(ui, world, cat);
        if first.is_some() {
            self.set_species(ui, world, cat, vfs, first);
        }
        match frequency == Some(-1) {
            true => self.disable_all(ui),
            false => self.enable_all(ui, world, cat),
        }
    }

    /// `set_species(t)` (19.4): a species other than the selected one is selected, and the
    /// admission and the finances follow.
    pub(crate) fn set_species(
        &mut self,
        ui: &mut Interface,
        world: &mut World,
        cat: &Catalogue,
        vfs: &Vfs,
        t: Option<u32>,
    ) {
        if t != self.species {
            self.flush_tricks(ui, world, cat);
            self.species = t;
            self.fill_tricks(ui, world, cat, vfs);
            self.update_admission(ui, world, cat);
            self.update_stats(ui, world);
        }
    }

    /// `enable_all()` (19.4).
    fn enable_all(&mut self, ui: &mut Interface, world: &mut World, cat: &Catalogue) {
        self.disabled = false;
        ui.set_enabled(SPECIES_BUTTON, true);
        ui.set_enabled(ADMISSION, true);
        self.update_admission(ui, world, cat);
        for list in TRICK_LISTS {
            ui.set_enabled(list, true);
        }
        self.select_current(ui);
    }

    /// `disable_all()` (19.4): the frequency button stays on, so that Never can be undone.
    fn disable_all(&mut self, ui: &mut Interface) {
        self.disabled = true;
        let off = [SPECIES_BUTTON, UP, DOWN, ADMISSION];
        for id in off.into_iter().chain(TRICK_BUTTONS).chain(TRICK_LISTS) {
            ui.set_enabled(id, false);
        }
    }

    /// `update_admission()` (shows 33.6): the grandstands take the price, and the field and its
    /// buttons show the admission of the species.
    pub(crate) fn update_admission(&self, ui: &mut Interface, world: &mut World, cat: &Catalogue) {
        let shown = match self.exhibit {
            Some(x) => admission::update_admission(world, cat, x, self.species),
            None => 0.0,
        };
        let (min, max) = (
            world.units.show.min_admission,
            world.units.show.max_admission,
        );
        let on = self.exhibit.is_some() && self.species.is_some() && !self.disabled;
        let symbol = Symbol::new(ui.string(SYMBOL));
        let text = format::money_cents(&symbol, f64::from(shown), false);
        ui.set_text(ADMISSION, &to_cp1252(&text));
        ui.set_enabled(UP, on && shown < max);
        ui.set_enabled(DOWN, on && shown > min);
    }

    /// The up or down button (shows 33.4), with an exhibit and a species.
    pub(crate) fn press(&self, ui: &mut Interface, world: &mut World, cat: &Catalogue, up: bool) {
        if let Some(x) = self.exhibit.filter(|_| self.species.is_some()) {
            admission::press_admission(world, cat, x, self.species, up);
        }
        self.update_admission(ui, world, cat);
    }

    /// The end of an edit of the admission field (shows 33.5).
    pub(crate) fn type_admission(&self, ui: &mut Interface, world: &mut World, cat: &Catalogue) {
        let text = ui.text(ADMISSION).unwrap_or_default();
        let symbol = ui.string(SYMBOL);
        let symbol = if symbol.trim().is_empty() {
            "$"
        } else {
            &symbol
        };
        let group = to_cp1252(format::money_group());
        let value = typed(&text, &to_cp1252(symbol), &group);
        if let Some(x) = self.exhibit.filter(|_| self.species.is_some()) {
            admission::type_admission(world, cat, x, self.species, value as f32);
        }
        self.update_admission(ui, world, cat);
    }

    /// `collect_species()` (19.5): fills the species list and the title, selects the row of the
    /// species that was selected (else the first), and returns the selected species when it is
    /// an animal type.
    pub(crate) fn collect_species(
        &mut self,
        ui: &mut Interface,
        world: &World,
        cat: &Catalogue,
    ) -> Option<u32> {
        let x = self.exhibit?;
        let name = world.exhibits().get(x).map(|e| e.name.clone());
        ui.set_text(TITLE, &to_cp1252(&name.unwrap_or_default()));
        let kept = ui.selected_row(SPECIES_LIST).and_then(|r| self.rows.get(r));
        let kept = kept.copied();
        let rows = species_of(ui, world, x);
        let list = rows.iter().map(|&t| Row {
            text: to_cp1252(&ui.string(t as i32)),
            ..Row::default()
        });
        ui.set_rows(SPECIES_LIST, list.collect());
        let at = kept.and_then(|k| rows.iter().position(|&t| t == k));
        ui.select_row(SPECIES_LIST, Some(at.unwrap_or(0)));
        let selected = (ui.selected_row(SPECIES_LIST)).and_then(|r| rows.get(r).copied());
        self.rows = rows;
        self.put_species_button(ui, selected);
        ui.set_enabled(FREQUENCY_BUTTON, selected.is_some());
        ui.set_hidden(SPECIES_LAYOUT, true);
        selected.filter(|&t| is_animal(cat, t))
    }

    /// The species button shows the name of `t`, enabled unless the controls are off; with none it
    /// is empty and disabled.
    fn put_species_button(&self, ui: &mut Interface, t: Option<u32>) {
        let name = t.map(|t| ui.string(t as i32)).unwrap_or_default();
        ui.set_text(SPECIES_BUTTON, &to_cp1252(&name));
        ui.set_enabled(SPECIES_BUTTON, t.is_some() && !self.disabled);
    }

    /// A row of the species drop-down was chosen (19.5): the species is selected and the list
    /// loses the keyboard focus, which hides it.
    pub(crate) fn choose_species(
        &mut self,
        ui: &mut Interface,
        world: &mut World,
        cat: &Catalogue,
        vfs: &Vfs,
        row: i32,
    ) {
        let t = usize::try_from(row).ok().and_then(|r| self.rows.get(r));
        let t = t.copied();
        self.put_species_button(ui, t);
        self.set_species(ui, world, cat, vfs, t.filter(|&t| is_animal(cat, t)));
        ui.unfocus(SPECIES_LIST);
    }

    /// A row of the frequency drop-down was chosen (19.7 point 3).
    pub(crate) fn choose_frequency(
        &mut self,
        ui: &mut Interface,
        world: &mut World,
        cat: &Catalogue,
        row: i32,
    ) {
        let value = usize::try_from(row)
            .ok()
            .and_then(|r| Some((r, *frequencies(world).get(r)?)));
        let info = self.exhibit.and_then(|x| show::info_of(world, x));
        if let (Some((r, value)), Some(info)) = (value, info)
            && info.frequency != value
        {
            let id = info.id;
            let text = ui.row_text(FREQUENCY_LIST, r).unwrap_or_default().to_vec();
            ui.set_text(FREQUENCY_BUTTON, &text);
            show::set_frequency(world, cat, id, value);
            match value {
                -1 => self.disable_all(ui),
                _ => self.enable_all(ui, world, cat),
            }
        }
        ui.unfocus(FREQUENCY_LIST);
    }

    /// The refresh of the clock (19.4) while the window is shown: the finances and the species
    /// list again. Returns the log lines of what changed.
    pub(crate) fn refresh(
        &mut self,
        ui: &mut Interface,
        world: &mut World,
        cat: &Catalogue,
        vfs: &Vfs,
    ) -> Vec<String> {
        self.update_stats(ui, world);
        match self.collect_species(ui, world, cat) {
            None => self.fill_exhibit(ui, world, cat, vfs),
            t if t != self.species => self.set_species(ui, world, cat, vfs, t),
            _ => {}
        }
        let told = self.told(ui, world);
        match told == self.told {
            true => Vec::new(),
            false => {
                self.told.clone_from(&told);
                told
            }
        }
    }

    /// The log lines: the exhibit, species and admission, and the finances.
    fn told(&self, ui: &Interface, world: &World) -> Vec<String> {
        let Some(x) = self.exhibit else {
            return vec!["show none".into()];
        };
        let text = |id| {
            let bytes = ui.text(id).unwrap_or_default();
            bytes
                .into_iter()
                .map(crate::text::from_cp1252)
                .collect::<String>()
        };
        let species = self.species.map_or("none".into(), |t| t.to_string());
        let frequency = show::info_of(world, x).map_or(0, |i| i.frequency);
        let mut told = vec![format!(
            "show {x} species {species} admission \"{}\" frequency {frequency}",
            text(ADMISSION)
        )];
        if self.species.is_some() {
            told.push(self.tricks_told(ui));
            let cells: Vec<String> = (CELLS..CELLS + 16).map(text).collect();
            told.push(format!(
                "finances {} months {}",
                cells.join(" | "),
                text(MONTHS)
            ));
        }
        told
    }

    /// `update_stats()` (19.8): the finances tab. It looks up the record of the species the
    /// show is playing, which makes the type 0 record between shows (shows 32.7).
    fn update_stats(&self, ui: &mut Interface, world: &mut World) {
        let (Some(x), Some(_)) = (self.exhibit, self.species) else {
            for id in (CELLS..CELLS + 16).chain([MONTHS]) {
                ui.set_text(id, b"");
            }
            return;
        };
        let created = (world.exhibits().get(x)).and_then(|e| e.stats.created.as_ref());
        let created = created.map(|d| file_time(d.game_ms()));
        let (now, date) = (world.units.now, file_time(world.units.now));
        let Some(slot) = show::info_slot(world, x) else {
            return;
        };
        let Some(info) = world.zoo.shows.infos[slot].as_mut() else {
            return;
        };
        let current = info.show.species;
        let months = i64::from(filter_months_since(info.lookup(current, date).date, now)).max(1);
        let m = months as f32;
        let f = f32::from_bits;
        let [a, b, c] = info.attendance.map(i64::from);
        let attendance = [a, b, c / months, c];
        let (income, upkeep) = (info.receipts.map(f), info.upkeep.map(f));
        let income = [income[0], income[1], income[2] / m, income[2]];
        let upkeep = [upkeep[0], upkeep[1], upkeep[2] / m, upkeep[2]];
        let symbol = Symbol::new(ui.string(SYMBOL));
        let money = |v: f32| to_cp1252(&format::money_cents(&symbol, f64::from(v), true));
        let green =
            (ui.tree.find(INCOME)).map_or([83, 190, 83], |i| ui.tree.elements[i].look.fore_color);
        for i in 0..4 {
            let at = CELLS + i as i32;
            let profit = income[i] - upkeep[i];
            ui.set_text(at, &to_cp1252(&format::integer(attendance[i], true)));
            ui.set_text(at + 4, &money(income[i]));
            ui.set_text(at + 8, &money(-upkeep[i]));
            ui.set_text(at + 12, &money(profit));
            let colour = match profit {
                p if p < 0.0 => LOSS,
                0.0 => BREAK_EVEN,
                _ => green,
            };
            ui.set_fore_color(at + 12, colour);
        }
        let since = created.map_or(1, |c| i64::from(filter_months_since(c, now)).max(1));
        ui.set_text(MONTHS, &to_cp1252(&format::integer(since, true)));
    }
}
