//! What the completed programs did to the entity types (research 6.4, 6.5, 6.7): an overlay on the
//! unmodified types, rebuilt for each game and each load (6.9, R4), and the readers' way to it.

use crate::build::{number, plain};
use crate::entity::{Catalogue, EntityType};
use crate::zoo::Entity;
use std::collections::{BTreeSet, HashMap};

/// A type by its type name and subtype, the way a map file names an entity.
type Key = (Vec<u8>, Vec<u8>);

fn key_of(ty: &EntityType) -> Key {
    (ty.type_name.clone(), ty.subtype.clone())
}

/// The change of a stored number (6.1): 0 adds, 1 puts, 2 adds `old * value / 100`, truncated, in
/// 32 bits; any other way changes nothing.
fn changed(old: i32, how: i32, value: i32) -> Option<i32> {
    match how {
        0 => Some(old.wrapping_add(value)),
        1 => Some(value),
        2 => Some(old.wrapping_add(old.wrapping_mul(value) / 100)),
        _ => None,
    }
}

/// `hundredths` times the single-precision 0.01, with one rounding (6.4, 6.7).
fn hundredth(hundredths: i32) -> f32 {
    (f64::from(hundredths) * f64::from(0.01_f32)) as f32
}

/// A change of the life counter of the live animals of a type (6.4): its type, the way and the
/// value. A percent change leaves a counter that is not above 0.
pub struct Life {
    key: Key,
    how: i32,
    value: i32,
}

impl Life {
    /// Whether the animal of this entity is of the type.
    pub fn reaches(&self, e: &Entity) -> bool {
        let bytes = |s: &str| s.chars().map(|c| c as u8).collect::<Vec<u8>>();
        self.key == (bytes(&e.type_name), bytes(&e.subtype))
    }

    /// The counter after the change.
    pub fn of(&self, life: i32) -> i32 {
        if self.how == 2 && life <= 0 {
            return life;
        }
        changed(life, self.how, self.value).unwrap_or(life)
    }
}

/// The changes of the characteristics of each type, in the order the programs completed, and the
/// training masks of the staff types.
#[derive(Debug, Default, Clone, PartialEq)]
pub struct Overlay {
    changes: HashMap<Key, Vec<(&'static str, i32, i32)>>,
    training: HashMap<Key, u16>,
}

/// The characteristic of a type that `effectval1` names (6.4), with the training bit a change of
/// it sets (6.5) when the type is a staff type.
fn characteristic(ty: &EntityType, which: i32) -> Option<(&'static str, Option<u32>)> {
    let expansion = plain(ty, "cExpansionID").unwrap_or(0.0) as i32;
    Some(match (ty.class, which) {
        ("animals", 0) => ("cKeeperArrivesChange", None),
        ("animals", 1) => ("cFoodUnitValue", None),
        ("animals", 2) => ("cReproductionChance", None),
        ("animals", 3) => ("cSickChance", None),
        ("animals", 4) => ("cTimeDeath", None),
        ("animals", 5) => ("cNumberAnimalsMax", None),
        ("food", 0) => ("cPurchaseCost", None),
        ("keeper", 0) => ("cSlowRate", Some([1, 2, 0][expansion.clamp(0, 2) as usize])),
        ("keeper", 1) if expansion == 1 => ("cFoodPerTile", Some(1)),
        ("maint", 0) => ("cCleanTrashRadius", Some(0)),
        ("maint", 1) => ("cFixFenceModifier", Some(1)),
        ("tour", 0) => ("cTourGuideBonus", Some(0)),
        ("filter", 0) => ("cDecayTime", None),
        _ => return None,
    })
}

/// Whether a change of this type reaches its whole group (6.4): the animals and the staff.
fn grouped(ty: &EntityType) -> bool {
    matches!(ty.class, "animals" | "keeper" | "maint" | "tour")
}

impl Overlay {
    fn fold(&self, key: &Key, name: &str, base: i32) -> i32 {
        let list = self.changes.get(key).into_iter().flatten();
        let list = list.filter(|c| c.0 == name);
        list.fold(base, |v, &(_, how, value)| {
            changed(v, how, value).unwrap_or(v)
        })
    }

    /// `base`, the value of characteristic `name` of the type without research, changed by the
    /// completed programs.
    pub fn int(&self, ty: &EntityType, name: &str, base: i32) -> i32 {
        match self.changes.is_empty() {
            true => base,
            false => self.fold(&key_of(ty), name, base),
        }
    }

    /// [`Overlay::int`] for the type of a map entity.
    pub fn int_of(&self, e: &Entity, name: &str, base: i32) -> i32 {
        if self.changes.is_empty() {
            return base;
        }
        let bytes = |s: &str| s.chars().map(|c| c as u8).collect::<Vec<u8>>();
        self.fold(&(bytes(&e.type_name), bytes(&e.subtype)), name, base)
    }

    /// The purchase price of a food type: the changes work on hundredths (6.4).
    pub fn price(&self, ty: &EntityType, base: f32) -> f32 {
        if self.changes.is_empty() {
            return base;
        }
        let changed =
            (self.changes.get(&key_of(ty)).into_iter().flatten()).any(|c| c.0 == "cPurchaseCost");
        let hundredths = (base * 100.0).round() as i32;
        match changed {
            false => base,
            true => hundredth(self.int(ty, "cPurchaseCost", hundredths)),
        }
    }

    /// The training mask of a staff type (6.5).
    pub fn training(&self, ty: &EntityType) -> u16 {
        self.training.get(&key_of(ty)).copied().unwrap_or(0)
    }

    /// [`Overlay::training`] for a type that may be unknown: 0.
    pub fn training_of(&self, ty: Option<&EntityType>) -> u16 {
        ty.map_or(0, |t| self.training(t))
    }

    fn mark(&mut self, ty: &EntityType, bit: u32) {
        if let Some(bit) = 1u16.checked_shl(bit) {
            *self.training.entry(key_of(ty)).or_default() |= bit;
        }
    }

    /// Applies the effect 2, 4 or 5 of a completed program (6.4): `target` and `vals`
    /// (`effectval1` to 3). The changes of the life counter are returned for the live animals.
    pub fn apply(
        &mut self,
        cat: &Catalogue,
        effect: i32,
        target: i32,
        vals: [i32; 3],
    ) -> Vec<Life> {
        let [which, value, how] = vals;
        let expansion = |t: &EntityType| plain(t, "cExpansionID").unwrap_or(0.0) as i32;
        let reached = |t: &EntityType| match effect {
            2 => t.name_id == target,
            4 if target < 100 => t.class == "animals" && expansion(t) == target,
            4 => t.class == "animals" && number(t, "cFamily") == Some(target as f32),
            5 => t.class == "food" && expansion(t) == target,
            _ => false,
        };
        let mut hit = BTreeSet::new();
        for (i, ty) in cat.types.iter().enumerate().filter(|(_, t)| reached(t)) {
            let same = |t: &EntityType| {
                effect == 2 && grouped(ty) && (t.class, &t.type_name) == (ty.class, &ty.type_name)
            };
            hit.extend((0..cat.types.len()).filter(|&j| j == i || same(&cat.types[j])));
        }
        // A "keeper arrives" change of animals (effects 2 and 4) trains the keepers of their
        // expansion, and a change of a tank filter the maintenance workers (6.5).
        let trained: BTreeSet<i32> = (hit.iter().map(|&i| &cat.types[i]))
            .filter(|t| which == 0 && t.class == "animals")
            .map(expansion)
            .collect();
        for ty in &cat.types {
            let keeper = ty.class == "keeper" && trained.contains(&expansion(ty));
            let filter = effect == 2
                && which == 0
                && ty.class == "maint"
                && hit.iter().any(|&i| cat.types[i].class == "filter");
            if keeper || filter {
                self.mark(ty, if keeper { 0 } else { 2 });
            }
        }
        let mut life = Vec::new();
        for ty in hit.into_iter().map(|i| &cat.types[i]) {
            let Some((name, bit)) = characteristic(ty, which) else {
                continue;
            };
            if let Some(bit) = bit {
                self.mark(ty, bit);
            }
            if (0..=2).contains(&how) {
                self.changes
                    .entry(key_of(ty))
                    .or_default()
                    .push((name, how, value));
                if name == "cTimeDeath" {
                    life.push(Life {
                        key: key_of(ty),
                        how,
                        value,
                    });
                }
            }
        }
        life
    }

    /// Marks the staff types of name id `staff` with training bit `mark - 1` (6.7).
    pub fn train(&mut self, cat: &Catalogue, staff: i32, mark: i32) {
        for ty in cat.types.iter().filter(|t| t.name_id == staff) {
            if let Some(bit) = mark.checked_sub(1).and_then(|b| u32::try_from(b).ok()) {
                self.mark(ty, bit);
            }
        }
    }
}

#[cfg(test)]
mod tests;
