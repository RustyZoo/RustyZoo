//! The map view's current tool and the panels of the building tools: the construction and objects
//! panels, their tabs and item lists, the name and cost of the chosen item (placing 3).
//!
//! A panel is a layout the left bar's buttons show and hide; [`Build::sync`] notices which one is
//! shown, fills its list and chooses an item, and notices when it is hidden again (its close
//! button, the left bar, Escape, the right button). The item chosen is the tool: the type the
//! map view will place. Every change tells one `tool ...` line.

use crate::config::{to_float, to_int};
use crate::entity::{Catalogue, EntityType};
use crate::money::{SYMBOL, Symbol};
use crate::text::format::whole_units;
use crate::text::to_cp1252;
use crate::ui::interface::Interface;
use crate::ui::layout::{HIDDEN, Picture, SELECTED};
use crate::vfs::Vfs;
use std::cmp::Ordering;

/// The expansions a type can be a member of, with their bit in [`crate::ui::layout::installed_expansions`].
const EXPANSIONS: [(&str, i32); 2] = [("dinosaur", 1), ("aqua", 2)];
/// The string of "All" (the expansion selector, which stays there).
pub const CONSTRUCTION: i32 = 8;
pub const OBJECTS: i32 = 4;
const ALL: i32 = 22900;
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
    let shown = |ty: &EntityType| available(ty) && listed(ty, tag, expansions);
    let mut list: Vec<usize> = (0..catalogue.types.len())
        .filter(|&i| shown(&catalogue.types[i]))
        .collect();
    let key = |&i: &usize| order(&catalogue.types[i], tag);
    list.sort_by(|a, b| key(a).partial_cmp(&key(b)).unwrap_or(Ordering::Equal));
    list
}

/// A number of `[Characteristics/Integers]`; fences and tank walls read the section of their
/// subtype after the plain one (entity types 7). Never infinite.
fn number(ty: &EntityType, key: &str) -> Option<f32> {
    let config = ty.config.as_deref()?;
    let read = |prefix: &[u8]| {
        config
            .section([prefix, b"Characteristics/Integers"].concat())?
            .get(key)
            .map(to_float)
            .filter(|v| v.is_finite())
    };
    let own = [&ty.subtype[..], b"/"].concat();
    let own = matches!(ty.class, "fences" | "tankwall").then(|| read(&own));
    own.flatten().or_else(|| read(b""))
}

/// `cPurchaseCost` of a type.
pub fn cost(ty: &EntityType) -> f32 {
    number(ty, "cPurchaseCost").unwrap_or(0.0)
}

/// The sort key of a type in the list of the purchase list `tag`, smallest first (placing 3.4).
fn order(ty: &EntityType, tag: &str) -> [f32; 4] {
    let has = |name: &str| {
        ty.config
            .as_deref()
            .is_some_and(|c| c.section(name).is_some())
    };
    match tag {
        "fence" if !matches!(ty.class, "fences" | "tankwall") => [0.0; 4],
        "fence" if has("TankFence") => [
            1.0 + f32::from(number(ty, "cIsShowFence").unwrap_or(0.0) != 0.0),
            cost(ty),
            0.0,
            0.0,
        ],
        "fence" if has("HabitatFence") => [3.0, -number(ty, "cHeight").unwrap_or(0.0), 0.0, 0.0],
        "fence" => [4.0, 0.0, 0.0, 0.0],
        "foliage" | "rocks" => {
            let habitat = number(ty, "cHabitat").unwrap_or(NO_HABITAT);
            let last = habitat == NO_HABITAT || habitat == BAD_HABITAT;
            [
                f32::from(last),
                habitat,
                number(ty, "cLocation").unwrap_or(NO_HABITAT),
                cost(ty),
            ]
        }
        _ => [cost(ty), 0.0, 0.0, 0.0],
    }
}

/// The icons of a type, as the animation names its sections give them (entity types 9.4): the
/// `Icon` values of the first section that has some, in file order.
fn icons(ty: &EntityType) -> Vec<String> {
    let Some(config) = ty.config.as_deref() else {
        return Vec::new();
    };
    let subtype =
        matches!(ty.class, "fences" | "tankwall").then(|| [&ty.subtype[..], b"/Icon"].concat());
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
    /// terrain panel inside it, the location and habitat pictures.
    hidden: &'static [i32],
    /// The era text, the fence descriptor's label and value: shown for the chosen item when it has
    /// them (placing 3.5).
    details: [i32; 3],
    /// The tab (a position in `tabs`) that shows the gate button, which no exhibit enables yet.
    gate: Option<(i32, usize)>,
}

const PANELS: [Panel; 2] = [
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
        hidden: &[7, 3283, 3281, 3280],
        details: [3284, 3296, 3299],
        gate: Some((3240, 0)),
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
        details: [0; 3],
        gate: None,
    },
];

/// What the player has done in the panels.
pub struct Build {
    expansions: i32,
    /// The panel shown, a position in [`PANELS`].
    open: Option<usize>,
    /// The types of the open tab, in the order of its list.
    list: Vec<usize>,
    /// The item each tab remembers (a type), by panel and tab.
    chosen: [[Option<usize>; 4]; 2],
    /// The type that is the tool, if one is.
    held: Option<usize>,
    /// The rotation step of each panel (placing 4.4).
    step: [usize; 2],
    /// The tab (a position in `tabs`) that `list` was built for.
    listed: Option<usize>,
    /// The first row each tab's list showed last, by panel and tab (placing 3.4).
    tops: [[usize; 4]; 2],
}

impl Build {
    /// The state at the start of a game on an install with these `expansions` (a mask).
    pub fn new(expansions: i32) -> Build {
        Build {
            expansions,
            open: None,
            list: Vec::new(),
            chosen: [[None; 4]; 2],
            held: None,
            step: [0; 2],
            listed: None,
            tops: [[0; 4]; 2],
        }
    }

    /// The type that is the tool: an index into the catalogue.
    pub fn tool(&self) -> Option<usize> {
        self.held
    }

    /// Notices that a panel was shown or hidden since the last call, and acts on it.
    pub fn sync(&mut self, ui: &mut Interface, vfs: &Vfs, cat: &Catalogue, log: &mut Vec<String>) {
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
        (self.open, self.held) = (None, None);
        log.push("tool panel none".into());
    }

    /// Escape or the right button drops the tool and clears the list selection; the next one
    /// closes the panel (input 4.6 steps 7 and 10). Whether it did something.
    pub fn cancel(&mut self, ui: &mut Interface, log: &mut Vec<String>) -> bool {
        let Some(p) = self.open else { return false };
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
        if panel.tabs.contains(&id) || panel.other_tabs.contains(&id) {
            self.fill(ui, vfs, cat, p, log);
        } else if let Some(turn) = panel.rotate.iter().position(|&r| r == id) {
            self.step[p] = (self.step[p] + if turn == 0 { 3 } else { 1 }) % 4;
            log.push(format!("tool rotate {} {}", panel.name, self.step[p]));
            self.fill(ui, vfs, cat, p, log);
        } else if let Some(at) = ui.item_of(panel.list, id) {
            self.choose(ui, cat, p, at, log);
        }
    }

    /// The tab whose list shows keeps the rows it shows.
    fn remember_top(&mut self, ui: &Interface, p: usize) {
        if let Some(t) = self.listed {
            self.tops[p][t] = ui.top(PANELS[p].list);
        }
    }

    /// The position in `tabs` of the tab that is selected.
    fn tab(ui: &Interface, p: usize) -> Option<usize> {
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
        let tab = Self::tab(ui, p);
        self.listed = tab;
        let button = tab.and_then(|t| ui.tree.find(panel.tabs[t]));
        let (tag, title) = button.map_or((String::new(), 0), |i| {
            let e = &ui.tree.elements[i];
            (
                String::from_utf8_lossy(&e.string_data).into_owned(),
                e.help_id.first().copied().unwrap_or(0),
            )
        });
        if tab.is_some() {
            for &id in panel
                .hidden
                .iter()
                .chain(panel.details.iter().filter(|&&d| d != 0))
            {
                ui.set_hidden(id, true);
            }
        }
        ui.set_text(panel.title, &to_cp1252(&ui.string(title)));
        ui.set_text(panel.expansion, &to_cp1252(&ui.string(ALL)));
        self.list = match tab {
            Some(_) => items(cat, &tag, self.expansions),
            None => Vec::new(),
        };
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
            ui.set_button(gate, false, false);
        }
        let want = tab
            .and_then(|t| self.chosen[p][t])
            .and_then(|ty| self.list.iter().position(|&l| l == ty))
            .or((!self.list.is_empty()).then_some(0));
        match want {
            Some(at) => self.choose(ui, cat, p, at, log),
            None => {
                self.held = None;
                for id in [panel.name_text, panel.cost_text] {
                    ui.set_text(id, b"");
                }
                ui.set_hidden(panel.rotate[0], true);
                ui.set_hidden(panel.rotate[1], true);
            }
        }
        if let Some(t) = tab {
            ui.set_top(panel.list, self.tops[p][t]);
        }
    }

    /// Item `at` of the list is chosen: it is the tool, its tab remembers it, its name and cost
    /// show, and the rotate buttons show if it turns.
    fn choose(
        &mut self,
        ui: &mut Interface,
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
        self.chosen[p][tab] = Some(index);
        self.held = Some(index);
        ui.select_item(panel.list, Some(at));
        let name = ui.string(ty.name_id);
        let price = Symbol::new(ui.string(SYMBOL)).text(whole_units(f64::from(cost(ty))));
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
        let tag = ui
            .tree
            .find(panel.tabs[tab])
            .map(|i| String::from_utf8_lossy(&ui.tree.elements[i].string_data).into_owned())
            .unwrap_or_default();
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
