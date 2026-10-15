//! The map view's current tool and the panels of the building tools: the construction and objects
//! panels, their tabs and item lists, the name and cost of the chosen item (placing 3).
//!
//! A panel is a layout the left bar's buttons show and hide; [`Build::sync`] notices which one is
//! shown, fills its list and chooses an item, and notices when it is hidden again (its close
//! button, the left bar, Escape, the right button). The item chosen is the tool: the type the
//! map view will place. Every change tells one `tool ...` line.

pub mod terrain;

use crate::config::{Config, to_float, to_int};
use crate::entity::{Catalogue, EntityType};
use crate::money::{SYMBOL, Symbol};
use crate::place::unit::{is_unit, price};
use crate::place::{Class, Leg, MIDDLE, fence_run, path_run, side};
use crate::staff;
use crate::terrain::Mode;
use crate::text::format::whole_units;
use crate::text::to_cp1252;
use crate::ui::interface::Interface;
use crate::ui::layout::{HIDDEN, Picture, SELECTED};
use crate::vfs::Vfs;
use std::cmp::Ordering;
use std::collections::HashMap;

/// The expansions a type can be a member of, with their bit in [`crate::ui::layout::installed_expansions`].
const EXPANSIONS: [(&str, i32); 2] = [("dinosaur", 1), ("aqua", 2)];
/// The string of "All" (the expansion selector, which stays there).
pub const CONSTRUCTION: i32 = 8;
pub const OBJECTS: i32 = 4;
pub const ADOPT: i32 = 3;
pub const HIRE: i32 = 10;
/// The Hire panel's title (string 3697) and the text of the duties of the chosen staff type
/// (unit placing 4.1; its salary goes to the panel's cost text).
const HIRE_TITLE: i32 = 3697;
const HIRE_DUTIES: i32 = 3613;
const PANEL_COUNT: usize = 4;
/// The position of the Hire panel in [`PANELS`].
const HIRE_PANEL: usize = 3;
/// The string of the Animals tab's sub-title while the expansion selector is on All.
const CREATURES: i32 = 2025;
/// The preferred object's picture and its border on the Adopt panel.
const PREFERRED: (i32, i32) = (2060, 2070);
const ALL: i32 = 22900;
/// The bulldozer button of the left bar.
pub const BULLDOZER: i32 = 1028;
/// A bulldozer press becomes an area drag when the pointer moves more than this many pixels.
const SWEEP_PIXELS: i64 = 16;
/// The `cHabitat` and `cLocation` of a type that has none, and the habitat of one that has no good one.
const NO_HABITAT: f32 = 9410.0;
const BAD_HABITAT: f32 = 9414.0;

/// Whether the type is a member of the purchase list `tag` (entity types 10.2, with the selector
/// on All): a member of the tag, and either of `all` or not only of expansions that are not
/// installed (`expansions`, a mask).
fn listed(ty: &EntityType, tag: &str, expansions: i32) -> bool {
    let Some(members) = ty.config.as_deref().and_then(|c| c.section("Member")) else {
        return false;
    };
    if !members.has(tag) || (tag == "fence" && ty.subtype != b"f") {
        return false;
    }
    if tag == "developer" || members.has("all") {
        return true;
    }
    let own: Vec<i32> = EXPANSIONS
        .iter()
        .filter(|(name, _)| members.has(name))
        .map(|e| e.1)
        .collect();
    own.is_empty() || own.iter().any(|bit| expansions & bit != 0)
}

/// Whether the type is available at the start of a game: `[Global] AvailableAtStartup` is 1 or
/// missing (entity types 10.4).
fn available(ty: &EntityType) -> bool {
    ty.config
        .as_deref()
        .and_then(|c| c.section("Global")?.get("AvailableAtStartup"))
        .is_none_or(|v| to_int(v) == Some(1))
}

/// The types of the purchase list `tag` that a panel shows, as indices into the catalogue, in the
/// order of the tab: the filtered types in catalogue order, then a stable sort by [`order`]
/// (placing 3.4).
pub fn items(catalogue: &Catalogue, tag: &str, expansions: i32) -> Vec<usize> {
    items_of(catalogue, tag, expansions, |_| true)
}

/// [`items`] keeping the types that satisfy `keep`.
fn items_of(
    catalogue: &Catalogue,
    tag: &str,
    expansions: i32,
    keep: impl Fn(&EntityType) -> bool,
) -> Vec<usize> {
    let shown = |ty: &EntityType| available(ty) && listed(ty, tag, expansions) && keep(ty);
    let mut list: Vec<usize> = (0..catalogue.types.len())
        .filter(|&i| shown(&catalogue.types[i]))
        .collect();
    let key = |&i: &usize| order(&catalogue.types[i], tag);
    list.sort_by(|a, b| key(a).partial_cmp(&key(b)).unwrap_or(Ordering::Equal));
    list
}

/// A number of `[Characteristics/Integers]`; fences and tank walls read the section of their
/// subtype after the plain one (entity types 7), and a unit the section of its subtype, then of
/// the default subtype, or the plain one when there is none (entity types 6.2). Never infinite.
pub(crate) fn number(ty: &EntityType, key: &str) -> Option<f32> {
    let config = ty.config.as_deref()?;
    let read = |prefix: &[u8]| {
        config
            .section([prefix, b"Characteristics/Integers"].concat())?
            .get(key)
            .map(to_float)
            .filter(|v| v.is_finite())
    };
    let own = [&ty.subtype[..], b"/"].concat();
    match ty.class {
        "fences" | "tankwall" => read(&own).or_else(|| read(b"")),
        "animals" | "keeper" | "tour" | "maint" => {
            let default = config
                .section("Global")
                .and_then(|g| g.get("DefaultSubtype"));
            let default = default.filter(|d| !d.is_empty());
            read(&own).or_else(|| read(&default.map_or(vec![], |d| [d, b"/"].concat())))
        }
        _ => read(b""),
    }
}

/// A number of the plain `[Characteristics/Integers]`, whatever the subtype (`cExpansionID`,
/// `cShow`, `cUseNumbersInName`; entity types 6.1).
pub(crate) fn plain(ty: &EntityType, key: &str) -> Option<f32> {
    let section = ty.config.as_deref()?.section("Characteristics/Integers")?;
    Some(to_float(section.get(key)?)).filter(|v| v.is_finite())
}

/// A text of the `[Characteristics/Strings]` of a unit: the section of its subtype, then of the
/// default subtype.
fn unit_text(ty: &EntityType, key: &str) -> Option<String> {
    let config = ty.config.as_deref()?;
    let default = config
        .section("Global")
        .and_then(|g| g.get("DefaultSubtype"));
    let prefixes = [ty.subtype.clone(), default.unwrap_or_default().to_vec()];
    prefixes.iter().find_map(|prefix| {
        let name = [prefix, &b"/Characteristics/Strings"[..]].concat();
        let value = config.section(name)?.get(key)?;
        Some(String::from_utf8_lossy(value.trim_ascii()).into_owned())
    })
}

/// `cPurchaseCost` of a type.
pub fn cost(ty: &EntityType) -> f32 {
    number(ty, "cPurchaseCost").unwrap_or(0.0)
}

/// The sort key of a type in the list of the purchase list `tag`, smallest first (placing 3.4).
fn order(ty: &EntityType, tag: &str) -> [f32; 5] {
    let has = |name: &str| {
        ty.config
            .as_deref()
            .is_some_and(|c| c.section(name).is_some())
    };
    match tag {
        // The staff list is in catalogue order (unit placing 4.2).
        "staff" => [0.0; 5],
        "fence" if !matches!(ty.class, "fences" | "tankwall") => [0.0; 5],
        "fence" if has("TankFence") => [
            1.0 + f32::from(number(ty, "cIsShowFence").unwrap_or(0.0) != 0.0),
            cost(ty),
            0.0,
            0.0,
            0.0,
        ],
        "fence" if has("HabitatFence") => {
            [3.0, -number(ty, "cHeight").unwrap_or(0.0), 0.0, 0.0, 0.0]
        }
        "fence" => [4.0, 0.0, 0.0, 0.0, 0.0],
        "animals" => [
            "cHabitat",
            "cLocation",
            "cFamily",
            "cGenus",
            "cPurchaseCost",
        ]
        .map(|key| number(ty, key).unwrap_or(0.0)),
        "foliage" | "rocks" => {
            let habitat = number(ty, "cHabitat").unwrap_or(NO_HABITAT);
            let last = habitat == NO_HABITAT || habitat == BAD_HABITAT;
            [
                f32::from(last),
                habitat,
                number(ty, "cLocation").unwrap_or(NO_HABITAT),
                cost(ty),
                0.0,
            ]
        }
        _ => [cost(ty), 0.0, 0.0, 0.0, 0.0],
    }
}

/// The icons of a type, as the animation names its sections give them (entity types 9.4): the
/// `Icon` values of the first section that has some, in file order.
fn icons(ty: &EntityType) -> Vec<String> {
    let Some(config) = ty.config.as_deref() else {
        return Vec::new();
    };
    let subtype = (matches!(ty.class, "fences" | "tankwall") || is_unit(ty))
        .then(|| [&ty.subtype[..], b"/Icon"].concat());
    let default = config
        .section("Global")
        .and_then(|g| g.get("DefaultSubtype"))
        .filter(|d| !d.is_empty() && subtype.is_some())
        .map(|d| [d, b"/Icon"].concat());
    subtype
        .into_iter()
        .chain(default)
        .chain([b"Icon".to_vec()])
        .filter_map(|name| config.section(name))
        .map(|section| section.get_all("Icon"))
        .find(|values| !values.is_empty())
        .map_or_else(Vec::new, |values| {
            values
                .iter()
                .map(|v| String::from_utf8_lossy(v).into_owned())
                .collect()
        })
}

/// The icon a list button shows at the rotation `step`: that number of the type's icons, or the
/// first when it has no such number (placing 3.4).
fn icon_at(names: &[String], step: usize) -> Option<&String> {
    names.get(step).or(names.first())
}

/// The animation of each value of `[Images]` in `ui/infoimg.cfg` (a missing file or a key that is
/// not a number gives nothing for it).
fn infoimg(vfs: &Vfs) -> HashMap<i32, String> {
    let config = vfs.read("ui/infoimg.cfg").ok();
    let config = config.and_then(|b| Config::parse(&b).ok());
    let Some(section) = config.as_ref().and_then(|c| c.section("Images")) else {
        eprintln!(
            "rustyzoo: warning: no [Images] in ui/infoimg.cfg: no location and habitat pictures"
        );
        return HashMap::new();
    };
    section
        .keys()
        .filter_map(|key| {
            let name = section.get(key)?.trim_ascii();
            Some((to_int(key)?, String::from_utf8_lossy(name).into_owned()))
        })
        .collect()
}

/// Whether the rotate buttons show for the type: scenery and buildings turn, paths and fences
/// follow the tile and its side, filters do not.
fn rotates(ty: &EntityType) -> bool {
    matches!(ty.class, "objects" | "building")
}

/// One panel: its layout, the left bar button that shows it, its elements, the tabs that list
/// items, and the other tabs that the terrain tools own.
struct Panel {
    name: &'static str,
    layout: i32,
    bar: i32,
    title: i32,
    expansion: i32,
    list: i32,
    rotate: [i32; 2],
    name_text: i32,
    cost_text: i32,
    tabs: &'static [i32],
    other_tabs: &'static [i32],
    /// Elements that showing the layout brings back and that this milestone leaves hidden: the
    /// terrain panel inside it.
    hidden: &'static [i32],
    /// The location picture, the habitat picture and its border; none on a panel without them
    /// (placing 3.5).
    pictures: [i32; 3],
    /// The era text, the fence descriptor's label and value: shown for the chosen item when it has
    /// them (placing 3.5).
    details: [i32; 3],
    /// The gate button and the tab (a position in `tabs`) that shows it.
    gate: Option<(i32, usize)>,
    /// The sex buttons (male, female) and the "no items to display" text; none, 0 without them.
    sexes: &'static [i32],
    empty: i32,
}

const PANELS: [Panel; PANEL_COUNT] = [
    Panel {
        name: "construction",
        layout: CONSTRUCTION,
        bar: 1002,
        title: 3261,
        expansion: 3204,
        list: 3208,
        rotate: [3258, 3259],
        name_text: 3263,
        cost_text: 3207,
        tabs: &[3251, 3056, 3252, 3256],
        other_tabs: &[3362, 3361],
        hidden: &[7],
        pictures: [3283, 3281, 3280],
        details: [3284, 3296, 3299],
        gate: Some((3240, 0)),
        sexes: &[],
        empty: 0,
    },
    Panel {
        name: "objects",
        layout: OBJECTS,
        bar: 1001,
        title: 3061,
        expansion: 3005,
        list: 3019,
        rotate: [3054, 3055],
        name_text: 3058,
        cost_text: 3016,
        tabs: &[3050, 3053],
        other_tabs: &[],
        hidden: &[],
        pictures: [0; 3],
        details: [0; 3],
        gate: None,
        sexes: &[],
        empty: 0,
    },
    Panel {
        name: "adopt",
        layout: ADOPT,
        bar: 1000,
        title: 2023,
        expansion: 2005,
        list: 2019,
        rotate: [2054, 2055],
        name_text: 2043,
        cost_text: 2014,
        tabs: &[2075, 3255, 3254, 2076],
        other_tabs: &[],
        // The Young button and the shelter capacity (not drawn yet).
        hidden: &[2051, 2090, 2091],
        pictures: [2045, 2047, 2046],
        details: [2082, 0, 0],
        gate: None,
        sexes: &[2000, 2001],
        empty: 2009,
    },
    Panel {
        name: "hire",
        layout: HIRE,
        bar: 1005,
        title: HIRE_TITLE,
        expansion: 3601,
        list: 3602,
        rotate: [3676, 3677],
        name_text: 3620,
        cost_text: 3611,
        tabs: &[],
        other_tabs: &[],
        hidden: &[],
        pictures: [0; 3],
        details: [0; 3],
        gate: None,
        sexes: &[],
        empty: 3609,
    },
];

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
pub struct Build {
    expansions: i32,
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
}

impl Build {
    /// The state at the start of a game on an install with these `expansions` (a mask).
    pub fn new(expansions: i32) -> Build {
        Build {
            expansions,
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
        }
    }

    /// The sex of the staff the Hire panel lists.
    pub fn set_hire(&mut self, male: bool) {
        self.hire_male = male;
    }

    /// A staff member was hired: the list of the open Hire panel is made again for the new sex.
    pub fn rehire(
        &mut self,
        ui: &mut Interface,
        vfs: &Vfs,
        cat: &Catalogue,
        log: &mut Vec<String>,
    ) {
        if self.open == Some(HIRE_PANEL) {
            self.fill(ui, vfs, cat, HIRE_PANEL, log);
        }
    }

    /// The month of the game's date, 1 to 12.
    pub fn set_month(&mut self, month: u32) {
        self.october = month == 10;
    }

    /// Whether the game's month is October.
    pub fn october(&self) -> bool {
        self.october
    }

    /// The zoo has `n` exhibits at the start of a game.
    pub fn with_exhibits(mut self, n: usize) -> Build {
        self.exhibits = n;
        self
    }

    /// The zoo has `n` exhibits now: the gate button is enabled while there is one, and the gate
    /// tool ends with the last one (placing 6.4).
    pub fn set_exhibits(&mut self, ui: &mut Interface, n: usize, log: &mut Vec<String>) {
        self.exhibits = n;
        if n == 0 {
            self.stop_gate(ui, log);
        }
        if let Some((gate, _)) = PANELS[0].gate {
            ui.set_enabled(gate, n > 0);
        }
    }

    /// Whether the gate tool is the tool.
    pub fn gating(&self) -> bool {
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
    pub fn bulldozing(&self) -> bool {
        self.bulldozer
    }

    /// The left button went down on the map with the bulldozer as the tool, at the window point
    /// `pixel`, on `tile` (none off the map).
    pub fn sweep_press(&mut self, tile: Option<(u32, u32)>, pixel: (i32, i32)) {
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
    pub fn sweep_area(
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

    /// Whether a bulldozer press is held.
    pub fn sweeping(&self) -> bool {
        self.sweep.is_some()
    }

    /// The bulldozer press is over: released, or dropped with the tool.
    pub fn end_sweep(&mut self) {
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
    pub fn press(&mut self, cat: &Catalogue, tile: Option<(u32, u32)>, point: (i64, i64)) {
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
    pub fn dragging(&self) -> bool {
        self.drag.is_some()
    }

    /// The world points of the pieces of the drag if the mouse is at `point` (height `z`, on tile
    /// `cell`) now, the start first: the middle of each tile of a path, the middle of the side of
    /// each piece of a fence; none without a drag. The run is computed again only when the tile or
    /// the height step (`z / 16`) changes or the point is more than 16 world units from the last
    /// computation (in y, and in x for a fence; placing 6.2 item 2); otherwise the last run stays,
    /// and so does the last point.
    pub fn run(&mut self, cell: (i32, i32), point: (i64, i64), z: i32) -> Vec<(i64, i64)> {
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
    pub fn end_drag(&mut self) {
        self.drag = None;
    }

    /// A game was opened: both panels reopen on their first tab (placing 3.5).
    pub fn first_tabs(ui: &mut Interface) {
        for panel in &PANELS {
            for (n, &tab) in panel.tabs.iter().chain(panel.other_tabs).enumerate() {
                ui.set_button(tab, n == 0, true);
            }
            for (n, &sex) in panel.sexes.iter().enumerate() {
                ui.set_button(sex, n == 0, true);
            }
        }
    }

    /// The type that is the tool: an index into the catalogue.
    pub fn tool(&self) -> Option<usize> {
        self.held
    }

    /// Makes the type `ty` (a position in the catalogue) the tool, as a test needs it without
    /// the panels.
    #[cfg(test)]
    pub fn hold(&mut self, ty: usize) {
        self.held = Some(ty);
    }

    /// The rotation step of the pending object, 0 to 3 (placing 4.4).
    pub fn step(&self) -> usize {
        self.turn
    }

    /// An object of the type was placed: a type with `cAutoRotate` turns by one step, so that a
    /// row of trees does not look cloned (placing 4.4). Whether it did.
    pub fn placed(&mut self, ty: &EntityType) -> bool {
        let turns = number(ty, "cAutoRotate") == Some(1.0);
        if turns {
            self.turn = (self.turn + 1) % 4;
        }
        turns
    }

    /// The mode of the terrain tab that is the tool, if one is.
    pub fn terrain_mode(&self) -> Option<Mode> {
        self.terrain
    }

    /// Notices that a panel was shown or hidden since the last call, and acts on it.
    pub fn sync(&mut self, ui: &mut Interface, vfs: &Vfs, cat: &Catalogue, log: &mut Vec<String>) {
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
        if let Some(i) = ui.tree.find(PANELS[p].layout) {
            ui.tree.hide(i);
        }
        ui.set_button(PANELS[p].bar, false, true);
        ui.touch();
        self.stop_gate(ui, log);
        (self.open, self.held, self.terrain, self.drag) = (None, None, None, None);
        log.push("tool panel none".into());
    }

    /// Escape or the right button drops the tool and clears the list selection; the next one
    /// closes the panel (input 4.6 steps 7 and 10). Whether it did something.
    pub fn cancel(&mut self, ui: &mut Interface, log: &mut Vec<String>) -> bool {
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
        let Some(p) = self.open else { return false };
        self.drag = None;
        if self.terrain.take().is_some() {
            terrain::hide(ui);
            log.push("tool drop".into());
            return true;
        }
        match self.held.take() {
            Some(_) => {
                ui.select_item(PANELS[p].list, None);
                log.push("tool drop".into());
            }
            None => self.close(ui, p, log),
        }
        true
    }

    /// A button of the interface was released as selected: a tab, a rotate button or an item.
    pub fn selected(
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
        ui.set_text(panel.expansion, &to_cp1252(&ui.string(ALL)));
        if panel.layout == ADOPT && tag == "animals" {
            ui.set_text(panel.title, &to_cp1252(&ui.string(CREATURES)));
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
                items_of(cat, &tag, self.expansions, |ty| {
                    is_unit(ty) && ty.subtype == sex
                })
            }
            (Some(_), None) => items(cat, &tag, self.expansions),
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
    fn show_pictures(&mut self, ui: &mut Interface, vfs: &Vfs, panel: &Panel, ty: &EntityType) {
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
                let id = number(ty, "cPrefIconID").unwrap_or(0.0) as i32;
                ui.tree.elements[i].help_id = vec![id];
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
        self.turn = self.step[p];
        ui.select_item(panel.list, Some(at));
        let name = ui.string(ty.name_id);
        // The Hire panel shows the monthly salary and the duties instead of the price.
        let hire = panel.layout == HIRE;
        let shown = if hire {
            staff::monthly(ty)
        } else {
            price(ty, self.october)
        };
        let price = Symbol::new(ui.string(SYMBOL)).text(whole_units(f64::from(shown)));
        if hire {
            ui.set_text(HIRE_DUTIES, &to_cp1252(&ui.string(staff::duties(ty))));
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
        self.show_pictures(ui, vfs, panel, ty);
        let tag = match panel.tabs.get(tab).and_then(|&t| ui.tree.find(t)) {
            Some(i) => String::from_utf8_lossy(&ui.tree.elements[i].string_data).into_owned(),
            None if hire => "staff".into(),
            None => String::new(),
        };
        log.push(format!(
            "tool item {tag} {}/{}/{} name={name} cost={price}",
            ty.class,
            String::from_utf8_lossy(&ty.type_name),
            String::from_utf8_lossy(&ty.subtype),
        ));
    }
}

#[cfg(test)]
mod tests;
