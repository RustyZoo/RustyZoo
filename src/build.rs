//! The map view's current tool and the panels of the building tools: the construction and objects
//! panels, their tabs and item lists, the name and cost of the chosen item (placing 3).
//!
//! A panel is a layout the left bar's buttons show and hide; [`Build::sync`] notices which one is
//! shown, fills its list and chooses an item, and notices when it is hidden again (its close
//! button, the left bar, Escape, the right button). The item chosen is the tool: the type the
//! map view will place. Every change tells one `tool ...` line.

mod choices;
mod items;
mod panel;
pub mod terrain;

use crate::avail::Availability;
use crate::config::{Config, to_float, to_int};
use crate::entity::{Catalogue, EntityType};
use crate::money::{SYMBOL, Symbol};
use crate::month::HELICOPTER_BASE;
use crate::place::unit::{is_unit, price};
use crate::place::{Class, Leg, MIDDLE, fence_run, path_run, side};
use crate::scenario::{Avail, Which};
use crate::staff;
use crate::terrain::Mode;
use crate::text::format::whole_units;
use crate::text::to_cp1252;
use crate::ui::interface::Interface;
use crate::ui::layout::{Expansion, HIDDEN, Picture, SELECTED};
use crate::ui::widgets::Row;
use crate::vfs::Vfs;
use std::cmp::Ordering;
use std::collections::HashMap;

use items::*;
pub(crate) use items::{cost, drt_base, infoimg, number, plain, preferred_object, unit_text};
use panel::*;

/// The expansions a type can be a member of, by selector value minus 1 (the entry ids of
/// research 3.5: base game 1, Dinosaur Digs 2, Marine Mania 3), with their bit in a [`Filter`]'s
/// `on`: the base game is bit 4, the others are those of [`crate::ui::layout::installed_expansions`].
const EXPANSIONS: [(&str, i32); 3] = [("zoo", 4), ("dinosaur", 1), ("aqua", 2)];
/// The string of "All", the selector's first row and its text while nothing is chosen.
const ALL: i32 = 22900;
/// The selector's layout and its list.
const SELECTOR: i32 = 152;
pub(crate) const SELECTOR_LIST: i32 = 22900;
pub(crate) const CONSTRUCTION: i32 = 8;
pub(crate) const OBJECTS: i32 = 4;
pub(crate) const ADOPT: i32 = 3;
pub(crate) const HIRE: i32 = 10;
/// The Hire panel's title (string 3697) and the text of the duties of the chosen staff type
/// (unit placing 4.1; its salary goes to the panel's cost text).
const HIRE_TITLE: i32 = 3697;
const HIRE_DUTIES: i32 = 3613;
const PANEL_COUNT: usize = 4;
/// The position of the Hire panel in [`PANELS`].
const HIRE_PANEL: usize = 3;
/// The position of the Adopt panel in [`PANELS`].
const ADOPT_PANEL: usize = 2;
/// The string of the Animals tab's sub-title while the expansion selector is on All.
const CREATURES: i32 = 2025;
/// The Animals tab's sub-title on Dinosaur Digs (unit placing 3.2).
const DINOSAURS: i32 = 2024;
/// The preferred object's picture and its border on the Adopt panel.
const PREFERRED: (i32, i32) = (2060, 2070);
/// The capacity label and number of a shelter on the Adopt panel.
const CAPACITY: [i32; 2] = [2090, 2091];
/// The bulldozer button of the left bar.
pub(crate) const BULLDOZER: i32 = 1028;
/// A bulldozer press becomes an area drag when the pointer moves more than this many pixels.
const SWEEP_PIXELS: i64 = 16;
/// The `cHabitat` and `cLocation` of a type that has none, and the habitat of one that has no good one.
const NO_HABITAT: f32 = 9410.0;
const BAD_HABITAT: f32 = 9414.0;

/// A drag: the tile of the press, the mode of the first leg of a path and the side of a fence
/// chosen at the press, which the whole run keeps.
struct Drag {
    start: (i32, i32),
    leg: Option<Leg>,
    side: Option<u32>,
    /// The last computation: the world point, its tile and height step, and the pieces it gave.
    last: Option<Last>,
}

/// A bulldozer press: the tile and the window point it went down at, and the last tile the
/// pointer was on.
struct Sweep {
    start: (i32, i32),
    pixel: (i32, i32),
    last: (i32, i32),
}

/// What the last computation of a run kept (placing 6.2 item 2).
struct Last {
    point: (i64, i64),
    tile: (i64, i64),
    step: i32,
    points: Vec<(i64, i64)>,
}

/// What the player has done in the panels.
pub(crate) struct Build {
    /// The expansions installed (a mask).
    expansions: i32,
    /// The rows of the selector after All, and its value: 0 for All, else the entry id plus 1 of
    /// one of them (a value with no row is held as 0).
    entries: Vec<Expansion>,
    selector: i32,
    /// The expansion entries a scenario disables (research 3.5): all, base game, Dinosaur Digs,
    /// Marine Mania.
    off: [bool; 4],
    /// The selector's list was filled.
    rowed: bool,
    /// The open list has taken the focus.
    taken: bool,
    /// Which types the lists show (entity types 10.4).
    avail: Availability,
    /// The panel shown, a position in [`PANELS`].
    open: Option<usize>,
    /// The types of the open tab, in the order of its list.
    list: Vec<usize>,
    /// The item each tab remembers (a type), by panel and tab.
    chosen: [[Option<usize>; 4]; PANEL_COUNT],
    /// The type that is the tool, if one is.
    held: Option<usize>,
    /// The rotation step of each panel (placing 4.4).
    step: [usize; PANEL_COUNT],
    /// The rotation step of the pending object: its panel's step, until a placement of a type
    /// that turns by itself advances it.
    turn: usize,
    /// The tab (a position in `tabs`) that `list` was built for.
    listed: Option<usize>,
    /// The first row each tab's list showed last, by panel and tab (placing 3.4).
    tops: [[usize; 4]; PANEL_COUNT],
    /// The mode of the terrain tab that is selected while the construction panel is open: that
    /// tab is the map view's tool.
    terrain: Option<Mode>,
    /// The picture of each `cLocation` and `cHabitat` value: its animation, from `ui/infoimg.cfg`;
    /// read when first needed.
    infoimg: Option<HashMap<i32, String>>,
    /// A path or fence drag in progress (placing 6.2).
    drag: Option<Drag>,
    /// The bulldozer is the tool (the bulldozer button is selected), and the press it holds.
    bulldozer: bool,
    sweep: Option<Sweep>,
    /// How many exhibits the zoo has: the gate button is enabled while there is one.
    exhibits: usize,
    /// The gate button is selected: "place a gate" is the tool (placing 6.4).
    gate: bool,
    /// The game's month is October (the half price of the dinosaur expansion's animals).
    october: bool,
    /// The Hire panel lists the male staff (unit placing 4.3).
    hire_male: bool,
    /// The staff and building types chosen since the last [`Build::take_made`]: each is a pending
    /// staff member made (staff 4.2) or a pending building (its random use time, guest buildings
    /// 10.1).
    made: Vec<usize>,
    /// The Hire panel was closed, and the Adopt list was emptied in full (closed, a tab or the
    /// expansion changed), since the last [`Build::take_ended`]: the facts plaques follow (unit
    /// placing 17.2 points 11 and 12).
    hire_closed: bool,
    adopt_emptied: bool,
}

impl Build {
    /// The state at the start of a game on an install with these `expansions` (a mask).
    pub(crate) fn new(expansions: i32) -> Build {
        Build {
            expansions,
            entries: Vec::new(),
            selector: 0,
            off: [false; 4],
            rowed: false,
            taken: false,
            avail: Availability::default(),
            open: None,
            list: Vec::new(),
            chosen: [[None; 4]; PANEL_COUNT],
            held: None,
            step: [0; PANEL_COUNT],
            turn: 0,
            listed: None,
            tops: [[0; 4]; PANEL_COUNT],
            terrain: None,
            infoimg: None,
            drag: None,
            bulldozer: false,
            sweep: None,
            exhibits: 0,
            gate: false,
            october: false,
            hire_male: false,
            made: Vec::new(),
            hire_closed: false,
            adopt_emptied: false,
        }
    }

    /// Whether the Hire panel was closed and whether the Adopt list was emptied in full since the
    /// last call.
    pub(crate) fn take_ended(&mut self) -> (bool, bool) {
        let ended = (self.hire_closed, self.adopt_emptied);
        (self.hire_closed, self.adopt_emptied) = (false, false);
        ended
    }

    /// The Adopt list is emptied in full when its panel closes, a tab is released or the
    /// expansion or an availability changes; the sex and rotate buttons keep the pending unit.
    pub(super) fn emptied(&mut self, p: usize) {
        self.adopt_emptied |= p == ADOPT_PANEL;
    }

    /// Makes the panel `p` the open one without showing it: the next [`Build::sync`] closes it.
    #[cfg(test)]
    pub(crate) fn open_hidden(&mut self, p: usize) {
        self.open = Some(p);
    }

    /// The staff and building types chosen since the last call, in order: the app creates a
    /// pending object for each.
    pub(crate) fn take_made(&mut self) -> Vec<usize> {
        std::mem::take(&mut self.made)
    }

    /// Applies the rewards of the ini file after a load or a start (entity types 10.4).
    pub(crate) fn unlocked(mut self, cat: &Catalogue, ini: &Config) -> Build {
        self.avail.unlock(cat, ini);
        self
    }

    /// The selector has the rows of `entries` (those of the three known expansions) and the value
    /// a file stored (zoo format 9.2).
    pub(crate) fn with_selector(mut self, entries: Vec<Expansion>, stored: u32) -> Build {
        entries
            .into_iter()
            .filter(|e| (0..EXPANSIONS.len() as i32).contains(&e.id))
            .for_each(|e| self.entries.push(e));
        self.set(i32::try_from(stored).unwrap_or(0));
        self
    }

    /// The month of the game's date, 1 to 12.
    pub(crate) fn set_month(&mut self, month: u32) {
        self.october = month == 10;
    }

    /// Whether the game's month is October.
    pub(crate) fn october(&self) -> bool {
        self.october
    }

    /// The zoo has `n` exhibits at the start of a game.
    pub(crate) fn with_exhibits(mut self, n: usize) -> Build {
        self.exhibits = n;
        self
    }

    /// The zoo has `n` exhibits now: the gate button is enabled while there is one, and the gate
    /// tool ends with the last one (placing 6.4).
    pub(crate) fn set_exhibits(&mut self, ui: &mut Interface, n: usize, log: &mut Vec<String>) {
        self.exhibits = n;
        if n == 0 {
            self.stop_gate(ui, log);
        }
        if let Some((gate, _)) = PANELS[0].gate {
            ui.set_enabled(gate, n > 0);
        }
    }

    /// Whether the gate tool is the tool.
    pub(crate) fn gating(&self) -> bool {
        self.gate
    }

    /// The gate tool ends: its button is released.
    fn stop_gate(&mut self, ui: &mut Interface, log: &mut Vec<String>) {
        if std::mem::take(&mut self.gate) {
            if let Some((gate, _)) = PANELS[0].gate {
                ui.set_button(gate, false, true);
            }
            log.push("tool gate off".into());
        }
    }

    /// The gate button was pressed or released: noticed from its state, the gate tool becomes
    /// the tool, which clears the item of the panel and the bulldozer, and a second press ends
    /// it (placing 6.4).
    fn notice_gate(&mut self, ui: &mut Interface, log: &mut Vec<String>) {
        let Some((button, _)) = PANELS[0].gate else {
            return;
        };
        let on = ui
            .tree
            .find(button)
            .is_some_and(|i| ui.tree.elements[i].state & SELECTED != 0);
        if on == self.gate {
            return;
        }
        self.gate = on;
        if !on {
            return log.push("tool gate off".into());
        }
        self.drag = None;
        self.stop_bulldozer(ui, log);
        let held = self.held.take().is_some();
        if let Some(p) = self.open {
            if held {
                ui.select_item(PANELS[p].list, None);
            }
            Self::no_item(ui, &PANELS[p]);
        }
        log.push("tool gate on".into());
    }

    /// Whether the bulldozer is the tool.
    pub(crate) fn bulldozing(&self) -> bool {
        self.bulldozer
    }

    /// The left button went down on the map with the bulldozer as the tool, at the window point
    /// `pixel`, on `tile` (none off the map).
    pub(crate) fn sweep_press(&mut self, tile: Option<(u32, u32)>, pixel: (i32, i32)) {
        let tile = tile.and_then(|(x, y)| Some((i32::try_from(x).ok()?, i32::try_from(y).ok()?)));
        self.sweep = tile.filter(|_| self.bulldozer).map(|start| Sweep {
            start,
            pixel,
            last: start,
        });
    }

    /// The rectangle of tiles (the press tile and the one under the pointer, both included) while
    /// the press is an area drag: the pointer left the press tile or is more than 16 pixels from
    /// the press point on an axis; back on the press tile within 16 pixels on both it is a click
    /// again. A pointer
    /// off the map keeps the last tile it was on. `None` without a press or in a click.
    pub(crate) fn sweep_area(
        &mut self,
        tile: Option<(i32, i32)>,
        pixel: (i32, i32),
    ) -> Option<((i32, i32), (i32, i32))> {
        let sweep = self.sweep.as_mut()?;
        sweep.last = tile.unwrap_or(sweep.last);
        let (dx, dy) = (
            i64::from(pixel.0) - i64::from(sweep.pixel.0),
            i64::from(pixel.1) - i64::from(sweep.pixel.1),
        );
        let far = dx.abs() > SWEEP_PIXELS || dy.abs() > SWEEP_PIXELS;
        (sweep.last != sweep.start || far).then_some((sweep.start, sweep.last))
    }

    /// The bulldozer press is over: released, or dropped with the tool.
    pub(crate) fn end_sweep(&mut self) {
        self.sweep = None;
    }

    /// The bulldozer stops being the tool: its button is released and a held press dropped.
    fn stop_bulldozer(&mut self, ui: &mut Interface, log: &mut Vec<String>) {
        if std::mem::take(&mut self.bulldozer) {
            ui.set_button(BULLDOZER, false, true);
            log.push("tool bulldozer off".into());
        }
        self.sweep = None;
    }

    /// The bulldozer button was pressed or released: noticed from its state, the bulldozer
    /// becomes the tool, which clears the item (and the terrain tab) of the open panel, and a
    /// second press ends it (placing 7.1).
    fn notice_bulldozer(&mut self, ui: &mut Interface, log: &mut Vec<String>) {
        let on = ui
            .tree
            .find(BULLDOZER)
            .is_some_and(|i| ui.tree.elements[i].state & SELECTED != 0);
        if on == self.bulldozer {
            return;
        }
        self.bulldozer = on;
        self.sweep = None;
        if !on {
            return log.push("tool bulldozer off".into());
        }
        self.drag = None;
        self.stop_gate(ui, log);
        if let Some(p) = self.open {
            if self.terrain.take().is_some() {
                terrain::hide(ui);
            }
            if self.held.take().is_some() {
                ui.select_item(PANELS[p].list, None);
            }
            Self::no_item(ui, &PANELS[p]);
        }
        log.push("tool bulldozer on".into());
    }

    /// The left button went down on `tile` of the map, at the world point `point`: with a path or
    /// a fence as the tool, the tile starts a drag, and a fence keeps the side of the point
    /// (placing 6.2 item 1).
    pub(crate) fn press(&mut self, cat: &Catalogue, tile: Option<(u32, u32)>, point: (i64, i64)) {
        let class = self.held.and_then(|ty| cat.types.get(ty)).map(Class::of);
        let tile = tile.and_then(|(x, y)| Some((i32::try_from(x).ok()?, i32::try_from(y).ok()?)));
        let side = (class == Some(Class::Fence))
            .then(|| side(point.0.rem_euclid(64) as i32, point.1.rem_euclid(64) as i32));
        self.drag = tile
            .filter(|_| matches!(class, Some(Class::Path | Class::Fence)))
            .map(|start| Drag {
                start,
                leg: None,
                side,
                last: None,
            });
    }

    /// Whether a path or fence drag is in progress.
    pub(crate) fn dragging(&self) -> bool {
        self.drag.is_some()
    }

    /// The world points of the pieces of the drag if the mouse is at `point` (height `z`, on tile
    /// `cell`) now, the start first: the middle of each tile of a path, the middle of the side of
    /// each piece of a fence; none without a drag. The run is computed again only when the tile or
    /// the height step (`z / 16`) changes or the point is more than 16 world units from the last
    /// computation (in y, and in x for a fence; placing 6.2 item 2); otherwise the last run stays,
    /// and so does the last point.
    pub(crate) fn run(&mut self, cell: (i32, i32), point: (i64, i64), z: i32) -> Vec<(i64, i64)> {
        let Some(drag) = &mut self.drag else {
            return Vec::new();
        };
        let tile = (i64::from(cell.0), i64::from(cell.1));
        if let Some(last) = &drag.last
            && last.tile == tile
            && last.step == z / 16
            && last.point.1.abs_diff(point.1) <= 16
            && (drag.side.is_none() || last.point.0.abs_diff(point.0) <= 16)
        {
            return last.points.clone();
        }
        let at = |(x, y): (i32, i32), (dx, dy): (i32, i32)| {
            (
                i64::from(x) * 64 + i64::from(dx),
                i64::from(y) * 64 + i64::from(dy),
            )
        };
        let points: Vec<(i64, i64)> = match drag.side {
            Some(first) => fence_run(drag.start, first, point)
                .into_iter()
                .map(|(t, s)| at(t, MIDDLE[s as usize / 2]))
                .collect(),
            None => path_run(drag.start, cell, &mut drag.leg)
                .into_iter()
                .map(|t| at(t, (32, 32)))
                .collect(),
        };
        drag.last = Some(Last {
            point,
            tile,
            step: z / 16,
            points: points.clone(),
        });
        points
    }

    /// The drag is over: released, or dropped with the tool.
    pub(crate) fn end_drag(&mut self) {
        self.drag = None;
    }

    /// A game was opened: both panels reopen on their first tab (placing 3.5).
    pub(crate) fn first_tabs(ui: &mut Interface) {
        for panel in &PANELS {
            Self::first_tab(ui, panel);
        }
    }

    fn first_tab(ui: &mut Interface, panel: &Panel) {
        for (n, &tab) in panel.tabs.iter().chain(panel.other_tabs).enumerate() {
            ui.set_button(tab, n == 0, true);
        }
        for (n, &sex) in panel.sexes.iter().enumerate() {
            ui.set_button(sex, n == 0, true);
        }
    }

    /// The type that is the tool: an index into the catalogue.
    pub(crate) fn tool(&self) -> Option<usize> {
        self.held
    }

    /// Makes the type `ty` (a position in the catalogue) the tool, as a test needs it without
    /// the panels.
    #[cfg(test)]
    pub(crate) fn hold(&mut self, ty: usize) {
        self.held = Some(ty);
    }

    /// The catalogue positions the chosen tab lists, in its order, as a test looks for an item.
    #[cfg(test)]
    pub(crate) fn listed(&self) -> &[usize] {
        &self.list
    }

    /// The staff type `ty` makes a pending staff member without a choice in the panels: the new
    /// pending unit of the same item after a hire (unit placing 7.1 step 6), or a test.
    pub(crate) fn chose(&mut self, ty: usize) {
        self.made.push(ty);
    }

    /// The rotation step of the pending object, 0 to 3 (placing 4.4).
    pub(crate) fn step(&self) -> usize {
        self.turn
    }

    /// An object of the type was placed: a type with `cAutoRotate` turns by one step, so that a
    /// row of trees does not look cloned (placing 4.4). Whether it did.
    pub(crate) fn placed(&mut self, ty: &EntityType) -> bool {
        let turns = number(ty, "cAutoRotate") == Some(1.0);
        if turns {
            self.turn = (self.turn + 1) % 4;
        }
        turns
    }

    /// The mode of the terrain tab that is the tool, if one is.
    pub(crate) fn terrain_mode(&self) -> Option<Mode> {
        self.terrain
    }

    /// Hides every tool panel: the next [`Build::sync`] closes the open one and drops its tool.
    pub(crate) fn hide_panels(ui: &mut Interface) {
        for panel in &PANELS {
            ui.set_hidden(panel.layout, true);
        }
    }

    /// Notices that a panel was shown or hidden since the last call, and acts on it.
    pub fn sync(&mut self, ui: &mut Interface, vfs: &Vfs, cat: &Catalogue, log: &mut Vec<String>) {
        if !std::mem::replace(&mut self.rowed, true) {
            self.fill_selector(ui);
        }
        self.watch_selector(ui);
        self.notice_bulldozer(ui, log);
        self.notice_gate(ui, log);
        let shown = PANELS.iter().position(|p| {
            ui.tree
                .find(p.layout)
                .is_some_and(|i| ui.tree.elements[i].state & HIDDEN == 0)
        });
        if shown == self.open {
            return;
        }
        if let Some(old) = self.open {
            self.close(ui, old, log);
        }
        if let Some(new) = shown {
            self.open = Some(new);
            log.push(format!("tool panel {}", PANELS[new].name));
            self.fill(ui, vfs, cat, new, log);
        }
    }

    /// The panel `p` is closed: hidden, its left bar button released, no tool.
    fn close(&mut self, ui: &mut Interface, p: usize, log: &mut Vec<String>) {
        self.remember_top(ui, p);
        self.listed = None;
        self.hire_closed |= p == HIRE_PANEL;
        self.emptied(p);
        if let Some(i) = ui.tree.find(PANELS[p].layout) {
            ui.tree.hide(i);
        }
        ui.set_button(PANELS[p].bar, false, true);
        ui.touch();
        self.close_selector(ui);
        self.stop_gate(ui, log);
        (self.open, self.held, self.terrain, self.drag) = (None, None, None, None);
        log.push("tool panel none".into());
    }

    /// Escape or the right button drops the tool and clears the list selection; the next one
    /// closes the panel (input 4.6 steps 7 and 10). Whether it did something.
    #[cfg(test)]
    pub(crate) fn cancel(&mut self, ui: &mut Interface, log: &mut Vec<String>) -> bool {
        self.cancel_modes(ui, log) || self.cancel_held(ui, log)
    }

    /// The tool modes of the cancel cascade (input 4.6 steps 2 to 5): the bulldozer, the gate
    /// placement, then the terrain editing. Whether one was left.
    pub(crate) fn cancel_modes(&mut self, ui: &mut Interface, log: &mut Vec<String>) -> bool {
        if self.bulldozer {
            self.stop_bulldozer(ui, log);
            log.push("tool drop".into());
            return true;
        }
        if self.gate {
            self.stop_gate(ui, log);
            log.push("tool drop".into());
            return true;
        }
        if self.open.is_some() {
            self.drag = None;
            if self.terrain.take().is_some() {
                terrain::hide(ui);
                log.push("tool drop".into());
                return true;
            }
        }
        false
    }

    /// The held item of the cancel cascade (input 4.6 step 7): dropped, or the open panel closed
    /// when none is held. Whether a panel was open.
    pub(crate) fn cancel_held(&mut self, ui: &mut Interface, log: &mut Vec<String>) -> bool {
        let Some(p) = self.open else { return false };
        match self.held.take() {
            Some(_) => {
                ui.select_item(PANELS[p].list, None);
                // The shelter's capacity goes with the item (unit placing 3.3 item 6).
                if PANELS[p].layout == ADOPT {
                    for id in CAPACITY {
                        ui.set_hidden(id, true);
                    }
                }
                log.push("tool drop".into());
            }
            None => self.close(ui, p, log),
        }
        true
    }

    /// A button of the interface was released as selected: a tab, a rotate button or an item.
    pub(crate) fn selected(
        &mut self,
        ui: &mut Interface,
        vfs: &Vfs,
        cat: &Catalogue,
        id: i32,
        log: &mut Vec<String>,
    ) {
        let Some(p) = self.open else { return };
        let panel = &PANELS[p];
        if panel.tabs.contains(&id) || panel.other_tabs.contains(&id) || panel.sexes.contains(&id) {
            if !panel.sexes.contains(&id) {
                self.emptied(p);
            }
            self.fill(ui, vfs, cat, p, log);
        } else if let Some(turn) = panel.rotate.iter().position(|&r| r == id) {
            self.step[p] = (self.step[p] + if turn == 0 { 3 } else { 1 }) % 4;
            log.push(format!("tool rotate {} {}", panel.name, self.step[p]));
            self.fill(ui, vfs, cat, p, log);
        } else if let Some(at) = ui.item_of(panel.list, id) {
            self.choose(ui, vfs, cat, p, at, log);
        }
    }

    /// The tab whose list shows keeps the rows it shows.
    fn remember_top(&mut self, ui: &Interface, p: usize) {
        if let Some(t) = self.listed {
            self.tops[p][t] = ui.top(PANELS[p].list);
        }
    }

    /// Whether the Female button is selected.
    fn female(ui: &Interface) -> bool {
        PANELS[2].sexes.get(1).is_some_and(|&id| {
            ui.tree
                .find(id)
                .is_some_and(|i| ui.tree.elements[i].state & SELECTED != 0)
        })
    }

    /// The position in `tabs` of the tab that is selected.
    fn tab(ui: &Interface, p: usize) -> Option<usize> {
        if PANELS[p].tabs.is_empty() {
            return Some(0);
        }
        PANELS[p].tabs.iter().position(|&t| {
            ui.tree
                .find(t)
                .is_some_and(|i| ui.tree.elements[i].state & SELECTED != 0)
        })
    }

    /// Fills the panel `p` for its selected tab: title, list, gate button, and chooses the item
    /// that tab remembers, or the first.
    fn fill(
        &mut self,
        ui: &mut Interface,
        vfs: &Vfs,
        cat: &Catalogue,
        p: usize,
        log: &mut Vec<String>,
    ) {
        let panel = &PANELS[p];
        self.remember_top(ui, p);
        self.stop_gate(ui, log);
        let tab = Self::tab(ui, p);
        self.listed = tab;
        if panel.layout == CONSTRUCTION {
            self.terrain = terrain::tab(ui, tab.is_some());
            if self.terrain.is_some() {
                self.stop_bulldozer(ui, log);
            }
        }
        let button = tab.and_then(|t| ui.tree.find(*panel.tabs.get(t)?));
        let (tag, title) = match button {
            Some(i) => {
                let e = &ui.tree.elements[i];
                (
                    String::from_utf8_lossy(&e.string_data).into_owned(),
                    e.help_id.first().copied().unwrap_or(0),
                )
            }
            None if panel.layout == HIRE => ("staff".into(), HIRE_TITLE),
            None => (String::new(), 0),
        };
        if tab.is_some() {
            for &id in panel.hidden.iter().chain(
                panel
                    .details
                    .iter()
                    .chain(&panel.pictures)
                    .filter(|&&d| d != 0),
            ) {
                ui.set_hidden(id, true);
            }
        }
        ui.set_text(panel.title, &to_cp1252(&ui.string(title)));
        ui.set_text(panel.expansion, &to_cp1252(&self.label(ui)));
        if panel.layout == ADOPT && tag == "animals" {
            let title = match self.value() {
                0 => CREATURES,
                2 => DINOSAURS,
                _ => title,
            };
            ui.set_text(panel.title, &to_cp1252(&ui.string(title)));
        }
        // The Animals tab lists each species once, as the sex of the sex buttons, and the Hire
        // panel the staff of the sex drawn.
        let male = match (panel.layout, tag.as_str()) {
            (HIRE, _) => Some(self.hire_male),
            (ADOPT, "animals") => Some(!Self::female(ui)),
            _ => None,
        };
        self.list = match (tab, male) {
            (None, _) => Vec::new(),
            (Some(_), Some(male)) => {
                let sex = [&b"f"[..], b"m"][usize::from(male)];
                let mut list = items_of(cat, &self.avail, &tag, self.filter(), |ty| {
                    is_unit(ty) && ty.subtype == sex
                });
                // The DRT base comes after the sort (unit placing 4.2).
                if panel.layout == HIRE {
                    list.extend(items_of(cat, &self.avail, &tag, self.filter(), drt_base));
                }
                list
            }
            (Some(_), None) => items(cat, &self.avail, &tag, self.filter()),
        };
        if panel.empty != 0 {
            ui.set_hidden(panel.empty, !self.list.is_empty());
        }
        let step = self.step[p];
        let pictures: Vec<Option<Picture>> = self
            .list
            .iter()
            .map(|&ty| {
                let names = icons(&cat.types[ty]);
                let name = icon_at(&names, step)?;
                ui.tree.picture(vfs, name)
            })
            .collect();
        ui.set_items(panel.list, &pictures);
        if let Some((gate, on)) = panel.gate {
            ui.set_hidden(gate, tab != Some(on));
            ui.set_button(gate, false, self.exhibits > 0);
        }
        let want = tab
            .and_then(|t| self.chosen[p][t])
            .and_then(|ty| {
                // The species stays when it is listed again as the other sex.
                let same = |l: &usize| {
                    let (a, b) = (&cat.types[*l], &cat.types[ty]);
                    is_unit(a) && a.class == b.class && a.type_name == b.type_name
                };
                let list = &self.list;
                (list.iter().position(|&l| l == ty)).or_else(|| list.iter().position(same))
            })
            .or((!self.list.is_empty()).then_some(0));
        match want {
            Some(at) => self.choose(ui, vfs, cat, p, at, log),
            // An empty Hire list does not call the select step: the choice, the details and the
            // pending staff member stay (unit placing 4.3).
            None if panel.layout == HIRE && self.list.is_empty() => {}
            None => {
                self.held = None;
                Self::no_item(ui, panel);
            }
        }
        if let Some(t) = tab {
            ui.set_top(panel.list, self.tops[p][t]);
        }
    }

    /// The panel shows no item: no name, cost, rotate buttons, era, fence descriptor or pictures.
    fn no_item(ui: &mut Interface, panel: &Panel) {
        for id in [panel.name_text, panel.cost_text] {
            ui.set_text(id, b"");
        }
        if panel.layout == HIRE {
            ui.set_text(HIRE_DUTIES, b"");
        }
        let shown = (panel.rotate.into_iter())
            .chain(panel.details)
            .chain(panel.pictures)
            .chain(
                (panel.layout == ADOPT)
                    .then_some([PREFERRED.0, PREFERRED.1])
                    .into_iter()
                    .flatten(),
            );
        for id in shown.filter(|&id| id != 0) {
            ui.set_hidden(id, true);
        }
    }

    /// The location and habitat pictures of the chosen type, with their values as tooltips; all
    /// three are hidden for a type with no good habitat (placing 3.5).
    fn show_pictures(
        &mut self,
        ui: &mut Interface,
        vfs: &Vfs,
        cat: &Catalogue,
        panel: &Panel,
        ty: &EntityType,
    ) {
        let [location, habitat, border] = panel.pictures;
        if location == 0 {
            return;
        }
        let table = self.infoimg.get_or_insert_with(|| infoimg(vfs));
        let value = |key| number(ty, key).unwrap_or(NO_HABITAT) as i32;
        let (loc, hab) = (value("cLocation"), value("cHabitat"));
        // Every animal shows both pictures (unit placing 3.3).
        let shown =
            ty.class == "animals" || (hab != NO_HABITAT as i32 && hab != BAD_HABITAT as i32);
        for (id, v) in [(location, loc), (habitat, hab)] {
            let name = table.get(&v).filter(|_| shown);
            let picture = name.and_then(|n| ui.tree.picture(vfs, n));
            ui.set_picture(id, picture);
            if let Some(i) = ui.tree.find(id) {
                ui.tree.elements[i].help_id = vec![v];
            }
        }
        ui.set_hidden(border, !shown);
        if panel.layout == ADOPT {
            // The preferred object of an animal: its icon, with the id as tooltip (unit placing 3.3).
            let icon = unit_text(ty, "cPrefIcon").and_then(|n| ui.tree.picture(vfs, &n));
            let (picture, frame) = PREFERRED;
            for id in [picture, frame] {
                ui.set_hidden(id, icon.is_none());
            }
            ui.set_picture(picture, icon);
            if let Some(i) = ui.tree.find(picture) {
                ui.tree.elements[i].help_id = vec![preferred_object(cat, ty)];
            }
        }
    }

    /// Item `at` of the list is chosen: it is the tool, its tab remembers it, its name and cost
    /// show, the rotate buttons show if it turns, and so do its location and habitat pictures
    /// if it has a habitat.
    fn choose(
        &mut self,
        ui: &mut Interface,
        vfs: &Vfs,
        cat: &Catalogue,
        p: usize,
        at: usize,
        log: &mut Vec<String>,
    ) {
        let panel = &PANELS[p];
        let (Some(tab), Some(&index)) = (Self::tab(ui, p), self.list.get(at)) else {
            return;
        };
        let ty = &cat.types[index];
        self.stop_bulldozer(ui, log);
        self.stop_gate(ui, log);
        self.chosen[p][tab] = Some(index);
        self.held = Some(index);
        if is_unit(ty) || ty.class == "building" {
            self.made.push(index);
        }
        self.turn = self.step[p];
        ui.select_item(panel.list, Some(at));
        // The DRT base shows its helicopter's details (unit placing 4.2).
        let facts = Some(ty)
            .filter(|t| drt_base(t))
            .and_then(|_| cat.types.iter().find(|t| t.class == "helicopter"))
            .unwrap_or(ty);
        let name = ui.string(facts.name_id);
        // The Hire panel shows the monthly salary and the duties instead of the price.
        let hire = panel.layout == HIRE;
        let shown = if hire {
            staff::monthly(facts)
        } else {
            price(ty, self.october)
        };
        let price = Symbol::new(ui.string(SYMBOL)).text(whole_units(f64::from(shown)));
        if hire {
            ui.set_text(HIRE_DUTIES, &to_cp1252(&ui.string(staff::duties(facts))));
        }
        ui.set_text(panel.name_text, &to_cp1252(&name));
        ui.set_text(panel.cost_text, &to_cp1252(&price));
        for id in panel.rotate {
            ui.set_hidden(id, !rotates(ty));
        }
        let [era_id, label, value] = panel.details;
        for id in panel.details.into_iter().filter(|&id| id != 0) {
            ui.set_hidden(id, true);
        }
        let era = number(ty, "cEra").unwrap_or(0.0) as i32;
        if era != 0 && era_id != 0 {
            ui.set_text(era_id, &to_cp1252(&ui.string(era)));
            ui.set_hidden(era_id, false);
        }
        if matches!(ty.class, "fences" | "tankwall") && label != 0 {
            let show = number(ty, "cIsShowFence").unwrap_or(0.0) != 0.0;
            let has = |name: &str| {
                ty.config
                    .as_deref()
                    .is_some_and(|c| c.section(name).is_some())
            };
            let id = if show {
                3299
            } else if has("HabitatFence") {
                3298
            } else {
                3297
            };
            ui.set_text(value, &to_cp1252(&ui.string(id)));
            ui.set_hidden(label, false);
            ui.set_hidden(value, false);
        }
        self.show_pictures(ui, vfs, cat, panel, ty);
        let tag = match panel.tabs.get(tab).and_then(|&t| ui.tree.find(t)) {
            Some(i) => String::from_utf8_lossy(&ui.tree.elements[i].string_data).into_owned(),
            None if hire => "staff".into(),
            None => String::new(),
        };
        // A shelter shows its capacity as a plain number (unit placing 3.3 items 4 and 5).
        if panel.layout == ADOPT {
            let shelters = tag == "shelters";
            let [label, number_id] = CAPACITY;
            let capacity = number(ty, "cCapacity").unwrap_or(1.0) as i64;
            ui.set_text(
                number_id,
                &to_cp1252(&crate::text::format::integer(capacity, false)),
            );
            ui.set_hidden(label, !shelters);
            ui.set_hidden(number_id, !(shelters && ty.class == "building"));
        }
        log.push(format!(
            "tool item {tag} {}/{}/{} name={name} cost={price}",
            ty.class,
            String::from_utf8_lossy(&ty.type_name),
            String::from_utf8_lossy(&ty.subtype),
        ));
    }
}

#[cfg(test)]
pub(crate) mod tests;
