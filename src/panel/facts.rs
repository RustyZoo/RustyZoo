//! The facts plaques (unit placing 17): the species plaque (layout 27), opened by the Adopt
//! panel's facts button or by the recommendations plaque, and the staff plaque (layout 33), opened
//! by the Hire panel's facts button.
//!
//! [`Panel`] looks at both on the interface clock: a plaque just shown is written for the type
//! the map view holds (or the animal selected), one just hidden is forgotten and releases its
//! button, and the species plaque follows the animal panel and the Adopt choice.

use crate::build::{infoimg, number, preferred_object, unit_text};
use crate::entity::{Catalogue, EntityType};
use crate::panel::exhibit::{PLAQUE, keeper_kind};
use crate::show::trick::performs;
use crate::text::to_cp1252;
use crate::ui::interface::Interface;
use crate::vfs::Vfs;

/// The species plaque, the Adopt panel's facts button and the button back to the recommendations.
pub(crate) const SPECIES: i32 = 27;
pub(crate) const ADOPT_BUTTON: i32 = 2050;
pub(crate) const KEEPER: i32 = 25011;
/// The research and developer windows, which hide both plaques when shown (17.3 point 13 b).
const WINDOWS: [i32; 2] = [15, 34];
/// The staff plaque and the Hire panel's facts button.
pub(crate) const STAFF: i32 = 33;
pub(crate) const HIRE_BUTTON: i32 = 3699;

const NAME: i32 = 25001;
const PICTURE: i32 = 25002;
const TEXT: i32 = 25003;
/// The location and habitat pictures (as the Adopt panel lays them out), and the border of the second.
const INFO: [i32; 2] = [25050, 25051];
const INFO_BORDER: i32 = 25052;
/// The preferred object's picture and its border.
const PREFERRED: [i32; 2] = [25053, 25054];
const ERA: i32 = 25055;
/// The picture and the text that tell the species performs in shows.
const SHOWS: [i32; 2] = [25098, 25099];
/// The jellyfish never perform, whatever its tricks say.
const NO_SHOWS: i32 = 5403;
/// The picture a plaque shows for a type that has none.
const NO_PICTURE: &str = "ui/plaque/noanimal/noanimal";

const STAFF_NAME: i32 = 40001;
const STAFF_PICTURE: i32 = 40002;
const STAFF_TEXT: i32 = 40003;
/// The name id of the helicopter, which stands for the DRT base in the Hire list.
const HELICOPTER: i32 = 9550;

/// What the plaques look at: the types and animals of the map view.
pub(crate) struct Look {
    /// The animal the Adopt panel holds: its type and entity id.
    pub held: Option<(usize, u32)>,
    /// The type the Hire panel holds, when it is staff or the DRT base.
    pub staff: Option<usize>,
    /// The animal selected: its entity id and type.
    pub selected: Option<(u32, usize)>,
    /// The Hire panel was closed since the last look (point 11).
    pub hire_closed: bool,
    /// The Adopt list was emptied in full since the last look (point 12).
    pub adopt_emptied: bool,
}

/// What the two plaques showed at the last look.
#[derive(Debug, Default)]
pub(crate) struct Panel {
    /// The species plaque was shown, and its Adopt button down.
    shown: bool,
    down: bool,
    /// The type of the animal it shows, and the animal of the animal panel it was opened from
    /// (the keeper button goes back to that one's recommendations).
    species: Option<usize>,
    from: Option<u32>,
    /// The animal selected and the one held at the last look.
    seen: Option<u32>,
    held: Option<u32>,
    /// The staff plaque was shown, and the type it was written for.
    staff: bool,
    written: Option<usize>,
    /// The research and developer windows were shown at the last look.
    windows: [bool; 2],
    /// The Adopt button chose the species of the plaque about to show.
    chosen: bool,
}

impl Panel {
    /// The species the plaque holds.
    #[cfg(test)]
    pub(crate) fn species(&self) -> Option<usize> {
        self.species
    }

    /// Looks at both plaques. The result is the animal whose recommendations plaque the keeper
    /// button of the species plaque switched to.
    pub(crate) fn sync(
        &mut self,
        ui: &mut Interface,
        vfs: &Vfs,
        cat: &Catalogue,
        look: &Look,
    ) -> Option<u32> {
        // Closing the Hire panel with the facts button down hides the staff plaque.
        if look.hire_closed && ui.is_down(HIRE_BUTTON) {
            ui.set_hidden(STAFF, true);
        }
        let up = WINDOWS.map(|id| ui.shown(id));
        if up.iter().zip(self.windows).any(|(&now, was)| now && !was) {
            ui.set_hidden(SPECIES, true);
            ui.set_hidden(PLAQUE, true);
        }
        self.windows = up;
        let back = self.species_sync(ui, vfs, cat, look);
        let staff = ui.shown(STAFF);
        if staff != self.staff {
            ui.set_button(HIRE_BUTTON, staff, true);
            self.staff = staff;
            self.written = None;
        }
        // The plaque follows the Hire list while its button is down (point 7).
        if let Some(ty) = look.staff.filter(|&t| staff && self.written != Some(t)) {
            write_staff(ui, vfs, &cat.types[ty]);
            self.written = Some(ty);
        }
        back
    }

    fn species_sync(
        &mut self,
        ui: &mut Interface,
        vfs: &Vfs,
        cat: &Catalogue,
        look: &Look,
    ) -> Option<u32> {
        // The Adopt button: down shows the held animal, up hides the plaque; with no animal held
        // nothing happens (point 2).
        let down = ui.is_down(ADOPT_BUTTON);
        // An emptied Adopt list hides the plaque that shows the pending animal (the one of the
        // last look: the tool is already none); one opened from a placed animal, or with no
        // animal pending, stays (point 12).
        if look.adopt_emptied
            && down
            && self.species.is_some()
            && self.from.is_none()
            && self.held.is_some()
        {
            ui.set_hidden(SPECIES, true);
        }
        if down != std::mem::replace(&mut self.down, down) {
            match (down, look.held) {
                (true, Some((ty, _))) if !ui.shown(SPECIES) => {
                    (self.species, self.chosen) = (Some(ty), true);
                    ui.show(SPECIES);
                }
                (true, Some((ty, _))) => self.set(ui, vfs, cat, Some(ty), None),
                (false, _) => {
                    ui.set_hidden(SPECIES, true);
                }
                _ => {}
            }
        }
        let shown = ui.shown(SPECIES);
        let selected = look.selected.map(|s| s.0);
        let mut back = None;
        if shown && !self.shown {
            // Opened by the Adopt button, which chose the species, or by the recommendations
            // plaque, which means the selected animal's.
            ui.set_hidden(PLAQUE, true);
            if !std::mem::take(&mut self.chosen) {
                (self.species, self.from) = match look.selected {
                    Some((id, ty)) => (Some(ty), Some(id)),
                    None => (None, None),
                };
            }
            self.write(ui, vfs, cat);
            ui.set_button(ADOPT_BUTTON, true, true);
            self.down = true;
        } else if !shown && self.shown {
            ui.set_button(ADOPT_BUTTON, false, true);
            self.down = false;
            back = self.from.filter(|_| ui.shown(PLAQUE));
            // The hide forgets the animal, not the species (17.3 point 15).
            (self.from, self.chosen) = (None, false);
        } else if shown && selected != self.seen {
            // Another animal's panel: its species; no animal: the plaque goes unless the Adopt
            // panel holds one (information panels 4.1, 4.3).
            match look.selected {
                Some((id, ty)) => self.set(ui, vfs, cat, Some(ty), Some(id)),
                None if look.held.is_none() => {
                    ui.set_hidden(SPECIES, true);
                }
                None => {}
            }
        } else if shown
            && let Some((ty, id)) = look.held
            && Some(id) != self.held
        {
            self.set(ui, vfs, cat, Some(ty), None);
        }
        (self.shown, self.seen, self.held) = (shown, selected, look.held.map(|h| h.1));
        back
    }

    /// The plaque shows `ty`, written again only when the species changed (point 4).
    fn set(
        &mut self,
        ui: &mut Interface,
        vfs: &Vfs,
        cat: &Catalogue,
        ty: Option<usize>,
        from: Option<u32>,
    ) {
        let changed = ty != self.species;
        (self.species, self.from) = (ty, from);
        match changed {
            true => self.write(ui, vfs, cat),
            false => {
                ui.set_enabled(KEEPER, from.is_some());
            }
        }
    }

    fn write(&self, ui: &mut Interface, vfs: &Vfs, cat: &Catalogue) {
        if let Some(ty) = self.species {
            write_species(ui, vfs, cat, &cat.types[ty], self.from.is_some());
        }
    }
}

/// The staff type the Hire panel's facts button shows for the item `tool`: its type, or the
/// helicopter's for the DRT base; none for what is no staff (point 7).
pub(crate) fn staff_of(cat: &Catalogue, tool: Option<usize>) -> Option<usize> {
    let tool = tool?;
    let at = match crate::build::drt_base(&cat.types[tool]) {
        true => cat.types.iter().position(|t| t.name_id == HELICOPTER)?,
        false => tool,
    };
    let ty = &cat.types[at];
    (crate::staff::is_staff(ty) || ty.class == "helicopter").then_some(at)
}

/// The picture in `id`: the type's plaque picture, else the empty one.
fn picture(ui: &mut Interface, vfs: &Vfs, id: i32, ty: &EntityType) {
    let name = unit_text(ty, "cPlaqueImageName");
    let shown =
        (name.and_then(|n| ui.tree.picture(vfs, &n))).or_else(|| ui.tree.picture(vfs, NO_PICTURE));
    ui.set_picture(id, shown);
}

/// The scrolled text of the type: its general text entry.
fn general_text(ui: &mut Interface, id: i32, ty: &EntityType) {
    let entry = unit_text(ty, "cGeneralInfoTextName").map(|n| ui.text_entry(n.as_bytes()));
    ui.set_text(id, &entry.unwrap_or_default());
}

/// The text of the species plaque: for a user-made animal, the file its language section names,
/// read each time (empty when the key or the member is missing); else the general text entry.
fn species_text(ui: &mut Interface, vfs: &Vfs, id: i32, ty: &EntityType) {
    if !crate::usertext::NAMES.contains(&ty.name_id) {
        return general_text(ui, id, ty);
    }
    let file = crate::usertext::info_file(ui, ty).and_then(|n| vfs.read(&n).ok());
    ui.set_text(id, &file.unwrap_or_default());
}

/// Writes the species plaque for `ty` (point 5) and sets the keeper button (point 6), enabled
/// when the plaque was opened from an animal's panel.
fn write_species(
    ui: &mut Interface,
    vfs: &Vfs,
    cat: &Catalogue,
    ty: &EntityType,
    from_animal: bool,
) {
    ui.set_text(NAME, &to_cp1252(&ui.string(ty.name_id)));
    picture(ui, vfs, PICTURE, ty);
    let table = infoimg(vfs);
    for (id, key) in INFO.into_iter().zip(["cLocation", "cHabitat"]) {
        let value = number(ty, key).map(|v| v as i32);
        let shown = value.and_then(|v| table.get(&v));
        let shown = shown.and_then(|n| ui.tree.picture(vfs, n));
        ui.set_picture(id, shown);
        if let Some(i) = ui.tree.find(id) {
            ui.tree.elements[i].help_id = vec![value.unwrap_or(0)];
        }
        if id == INFO[1] {
            ui.set_hidden(INFO_BORDER, shown.is_none());
        }
    }
    let era = number(ty, "cEra").map_or(0, |v| v as i32);
    ui.set_text(ERA, &to_cp1252(&ui.string(era)));
    ui.set_hidden(ERA, era == 0);
    let icon = unit_text(ty, "cPrefIcon").and_then(|n| ui.tree.picture(vfs, &n));
    for id in PREFERRED {
        ui.set_hidden(id, icon.is_none());
    }
    ui.set_picture(PREFERRED[0], icon);
    if let Some(i) = ui.tree.find(PREFERRED[0]) {
        ui.tree.elements[i].help_id = vec![preferred_object(cat, ty)];
    }
    let shows = performs(ty) && ty.name_id != NO_SHOWS;
    for id in SHOWS {
        ui.set_hidden(id, !shows);
    }
    species_text(ui, vfs, TEXT, ty);
    let kind = keeper_kind(number(ty, "cKeeper").map_or(9500, |v| v as i32));
    ui.set_image(KEEPER, kind);
    ui.use_tooltip(KEEPER, kind);
    ui.set_enabled(KEEPER, from_animal);
}

/// Writes the staff plaque for `ty` (point 9).
fn write_staff(ui: &mut Interface, vfs: &Vfs, ty: &EntityType) {
    ui.set_text(STAFF_NAME, &to_cp1252(&ui.string(ty.name_id)));
    picture(ui, vfs, STAFF_PICTURE, ty);
    general_text(ui, STAFF_TEXT, ty);
}
