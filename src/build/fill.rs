//! Filling a panel: the rows of its list, the pictures and the choice of an item.

use super::*;

impl Build {
    /// The tab whose list shows keeps the rows it shows.
    pub(super) fn remember_top(&mut self, ui: &Interface, p: usize) {
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
    pub(super) fn tab(ui: &Interface, p: usize) -> Option<usize> {
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
    pub(super) fn fill(
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

    /// The panel shows no item: no rotate buttons, and the release of the list item.
    pub(super) fn no_item(ui: &mut Interface, panel: &Panel) {
        for id in panel.rotate.into_iter().filter(|&id| id != 0) {
            ui.set_hidden(id, true);
        }
        Self::release(ui, panel);
    }

    /// The release of a list item (unit placing 3.3 items 6 and 9): no name, cost, era, fence
    /// descriptor, pictures or shelter capacity. The rotate buttons are not touched.
    pub(super) fn release(ui: &mut Interface, panel: &Panel) {
        for id in [panel.name_text, panel.cost_text] {
            ui.set_text(id, b"");
        }
        if panel.layout == HIRE {
            ui.set_text(HIRE_DUTIES, b"");
        }
        let adopt =
            (panel.layout == ADOPT).then_some([PREFERRED.0, PREFERRED.1, CAPACITY[0], CAPACITY[1]]);
        let shown = (panel.details.into_iter())
            .chain(panel.pictures)
            .chain(adopt.into_iter().flatten());
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
    pub(super) fn choose(
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
        // An egg layer with no young type makes nothing: the other buttons are released first, so
        // the panel ends with the species selected, no details and nothing held (3.3 item 8.1).
        if ty.class == "animals"
            && number(ty, "cLaysEggs") == Some(1.0)
            && !cat
                .types
                .iter()
                .any(|t| t.class == "animals" && t.type_name == ty.type_name && t.subtype == b"y")
        {
            self.held = None;
            ui.select_item(panel.list, Some(at));
            return Self::release(ui, panel);
        }
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
