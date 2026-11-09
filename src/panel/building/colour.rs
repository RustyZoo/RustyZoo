//! The colour tab of the building panels (guest buildings 20): the tab shows for a type with
//! colour replacement only; layout 302 has a colour button and a label for each of the first two
//! colour groups of the type; a press on a button opens the palette (layout 37) with the swatches of
//! the group's variants sorted by red, green and blue; a press on a swatch is the building's new
//! choice for the group.

use super::{Panel, TABS};
use crate::config::{Config, to_int};
use crate::entity::{Catalogue, EntityType};
use crate::exhibit::members::type_at;
use crate::render::default_choice;
use crate::text::to_cp1252;
use crate::ui::interface::Interface;
use crate::vfs::Vfs;
use crate::world::World;
use std::ops::RangeInclusive;

/// The layout of the two colour buttons and the layout of the palette.
pub(crate) const LAYOUT: i32 = 302;
pub(crate) const PALETTE: i32 = 37;
/// The colour buttons and their labels, one pair for each of the first two groups.
pub(crate) const BUTTONS: [i32; 2] = [2461, 2462];
const LABELS: [i32; 2] = [2451, 2452];
/// The swatch buttons of the palette.
pub(crate) const SWATCHES: RangeInclusive<i32> = 2401..=2424;
/// The colour tab of the charging panel and of the other one, and the first tab of each.
const TAB: [i32; 2] = [TABS[5], TABS[2]];
const FIRST: [i32; 2] = [TABS[3], TABS[0]];

/// One colour group of a building type: the string of its title, the swatch of each variant in
/// the group's own order, and the position of its default variant.
struct Group {
    title: i32,
    swatches: Vec<[u8; 3]>,
    default: usize,
}

impl Group {
    /// The positions of the variants sorted by swatch, red first (ties keep the group's order).
    fn sorted(&self) -> Vec<usize> {
        let mut order: Vec<usize> = (0..self.swatches.len()).collect();
        order.sort_by_key(|&v| self.swatches[v]);
        order
    }

    /// The swatch of the variant a building stores, else the default one; black past the list.
    fn shown(&self, stored: Option<&u8>) -> [u8; 3] {
        let at = stored.map_or(self.default, |&n| usize::from(n));
        self.swatches.get(at).copied().unwrap_or_default()
    }
}

/// Whether the type has colour replacement (`cIsColorReplaced` is 1).
fn replaced(ty: &EntityType) -> bool {
    let flag = ty.config.as_deref().and_then(|c| {
        let value = c
            .section("Characteristics/Integers")?
            .get("cIsColorReplaced");
        value.and_then(to_int)
    });
    flag == Some(1)
}

/// The colour groups of the type: the groups its `colorrep` names, found in `building.ai`, with the
/// title string of each and the swatch (`forecolor` of the `ui_info` section) of each variant.
fn groups(ty: &EntityType, vfs: &Vfs) -> Vec<Group> {
    let Some(config) = ty.config.as_deref().filter(|_| replaced(ty)) else {
        return Vec::new();
    };
    let Some(rep) = config.section("colorrep") else {
        return Vec::new();
    };
    let shared = (vfs.read("building.ai").ok())
        .and_then(|b| Config::parse(&b).ok())
        .unwrap_or_default();
    let (titles, defaults) = (rep.get_all("title"), rep.get_all("defaultpal"));
    let swatch = |name: Option<&Vec<u8>>| {
        let colors = name
            .and_then(|n| shared.section(n))
            .map(|s| s.get_all("forecolor"));
        let part = |i: usize| {
            let value = colors.and_then(|c| c.get(i)).and_then(|v| to_int(v));
            value.map_or(0, |n| n.clamp(0, 255) as u8)
        };
        [part(0), part(1), part(2)]
    };
    let mut found = Vec::new();
    for (g, name) in rep.get_all("replace").iter().enumerate() {
        let group = shared.section(name);
        let infos = group.map_or(&[][..], |s| s.get_all("ui_info"));
        let variants = group.map_or(0, |s| s.get_all("pal").len());
        found.push(Group {
            title: titles.get(g).and_then(|t| to_int(t)).unwrap_or(0),
            swatches: (0..variants).map(|v| swatch(infos.get(v))).collect(),
            default: usize::from(default_choice(group, defaults.get(g))),
        });
    }
    found
}

impl Panel {
    /// The selected building and its type.
    fn building<'a>(&self, world: &World, cat: &'a Catalogue) -> Option<(usize, &'a EntityType)> {
        let at = world.find(self.selected?)?;
        Some((at, type_at(world, cat, at)?))
    }

    /// Shows the colour tab for a type with colour replacement and hides it for any other (point
    /// 2); a panel left on the colour page by the last building goes to its first tab. For a type
    /// with the tab, the colour buttons and labels of layout 302 are written (point 3): group `g`
    /// has its swatch and title, the unused pair is hidden and never hit. Run when the panel is
    /// shown.
    pub(super) fn colours(
        &self,
        ui: &mut Interface,
        vfs: &Vfs,
        (ty, colors): (&EntityType, &[u8]),
        charging: bool,
    ) {
        let which = usize::from(!charging);
        let on = replaced(ty);
        ui.set_hidden(TAB[which], !on);
        ui.set_enabled(TAB[which], on);
        if !on && ui.is_down(TAB[which]) {
            ui.press_button(FIRST[which]);
        }
        if !on {
            return;
        }
        let groups = groups(ty, vfs);
        for (g, (&button, &label)) in BUTTONS.iter().zip(&LABELS).enumerate() {
            let group = groups.get(g);
            if let Some(group) = group {
                ui.set_fore_color(button, group.shown(colors.get(g)));
                ui.set_text(label, &to_cp1252(&ui.string(group.title)));
            }
            for id in [button, label] {
                ui.reveal(id, group.is_some());
            }
        }
    }

    /// A press on the colour button `g` (point 4): the button is remembered, the palette opens with
    /// the swatches of the group's variants sorted, the buttons past the variants hidden.
    pub(crate) fn pick(
        &mut self,
        ui: &mut Interface,
        world: &World,
        cat: &Catalogue,
        vfs: &Vfs,
        g: usize,
    ) {
        let group =
            (self.building(world, cat)).and_then(|(_, ty)| groups(ty, vfs).into_iter().nth(g));
        let Some(group) = group else { return };
        self.choosing = Some(g);
        let order = group.sorted();
        for (n, button) in SWATCHES.enumerate() {
            let swatch = order.get(n).map(|&v| group.swatches[v]);
            if let Some(swatch) = swatch {
                ui.set_fore_color(button, swatch);
            }
            ui.reveal(button, swatch.is_some());
        }
        ui.show(PALETTE);
    }

    /// A press on the swatch `k` of the palette (point 5): the building's choice for the group
    /// remembered is the first variant, in the group's own order, with the swatch of the `k`-th
    /// sorted one; the colour button takes the swatch and the palette hides. Returns the log line.
    pub(crate) fn choose(
        &mut self,
        ui: &mut Interface,
        world: &mut World,
        cat: &Catalogue,
        vfs: &Vfs,
        k: usize,
    ) -> Option<String> {
        let g = self.choosing?;
        let (at, ty) = self.building(world, cat)?;
        let all = groups(ty, vfs);
        let group = all.get(g)?;
        let swatch = group.swatches[*group.sorted().get(k)?];
        let position = (group.swatches.iter()).position(|&s| s == swatch);
        let position = position.unwrap_or(group.swatches.len());
        // A building that stores fewer choices than the type has groups starts from the defaults.
        let mut colors = world.zoo.entities[at].common.as_ref()?.colors.clone();
        let from = colors.len();
        colors.extend(all.iter().skip(from).map(|g| g.default as u8));
        colors[g] = u8::try_from(position).unwrap_or(u8::MAX);
        world.set_colors(at, colors);
        ui.set_fore_color(BUTTONS[g], swatch);
        ui.set_hidden(PALETTE, true);
        self.choosing = None;
        Some(format!("colour {} {g} {position}", self.selected?))
    }
}
