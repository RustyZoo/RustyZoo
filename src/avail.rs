//! Which entity types can be bought: one flag per type, set at the start of a program from
//! `[Global] AvailableAtStartup`, changed by name id during a game and raised by the rewards of
//! the ini file (entity types 10.4; research 6.2, 7.3).

use crate::config::{Config, to_int};
use crate::entity::{Catalogue, EntityType};
use crate::timing::ini_whole;
use std::collections::HashMap;

#[cfg(test)]
mod tests;

/// The most rewards read from `[user]`: more than any game gives, and a bound on a wrong count.
const MAX_UNLOCKS: i32 = 10_000;

/// The flags changed since the start-up values (a type not in the map keeps its start-up flag).
#[derive(Debug, Default)]
pub struct Availability(HashMap<usize, bool>);

/// Whether the type is available at the start of a program: `[Global] AvailableAtStartup` is 1
/// or missing.
fn at_startup(ty: &EntityType) -> bool {
    ty.config
        .as_deref()
        .and_then(|c| c.section("Global")?.get("AvailableAtStartup"))
        .is_none_or(|v| to_int(v) == Some(1))
}

impl Availability {
    /// Whether the type at `index` of the catalogue is available. A user type always is.
    pub fn is(&self, cat: &Catalogue, index: usize) -> bool {
        let Some(ty) = cat.types.get(index) else {
            return false;
        };
        crate::strings::is_user_id(ty.name_id)
            || self
                .0
                .get(&index)
                .copied()
                .unwrap_or_else(|| at_startup(ty))
    }

    /// Makes every type with this name id, whatever its subtype, available or not. A name id of
    /// 0 sets the nameless types; a negative one matches no type. Whether a flag changed.
    pub fn set(&mut self, cat: &Catalogue, name_id: i32, on: bool) -> bool {
        self.set_where(cat, |ty| ty.name_id == name_id, on)
    }

    /// Makes every type `keep` accepts available or not (a user type never changes). Whether a
    /// flag changed.
    pub fn set_where(
        &mut self,
        cat: &Catalogue,
        keep: impl Fn(&EntityType) -> bool,
        on: bool,
    ) -> bool {
        let mut changed = false;
        for (i, ty) in cat.types.iter().enumerate() {
            if keep(ty) && !crate::strings::is_user_id(ty.name_id) && self.is(cat, i) != on {
                self.0.insert(i, on);
                changed = true;
            }
        }
        changed
    }

    /// The rewards of the ini file: `[user] unlockCount` and `unlockEntity0` ... each name id
    /// makes available every type of the class and type name of the first type with that name id
    /// and that name id. A missing or invalid entry is skipped. Whether a flag changed.
    pub fn unlock(&mut self, cat: &Catalogue, ini: &Config) -> bool {
        let count = ini_whole(ini, "user", "unlockCount").unwrap_or(0);
        let mut changed = false;
        for n in 0..count.clamp(0, MAX_UNLOCKS) {
            let Some(id) = ini_whole(ini, "user", &format!("unlockEntity{n}")).filter(|&id| id > 0)
            else {
                continue;
            };
            let Some(first) = cat.types.iter().find(|t| t.name_id == id) else {
                continue;
            };
            for (i, ty) in cat.types.iter().enumerate() {
                if ty.name_id == id
                    && ty.class == first.class
                    && ty.type_name == first.type_name
                    && !self.is(cat, i)
                {
                    self.0.insert(i, true);
                    changed = true;
                }
            }
        }
        changed
    }
}
