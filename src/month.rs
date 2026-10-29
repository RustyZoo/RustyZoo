//! The charges of the month-end finance check (zoo status 9 items 6 and 7): the monthly wage of
//! every staff member and the upkeep of the guest buildings, taken whatever the cash.

use crate::building::{Kind, Live, upgrade};
use crate::config::to_float;
use crate::entity::{Catalogue, EntityType};
use crate::money::{BUILDING_UPKEEP, Money};
use crate::place::bulldozer::type_of;
use crate::staff;
use crate::unit::Units;
use crate::world::World;

/// The name id of the helicopter base, which costs nothing (guest buildings 9).
const HELICOPTER_BASE: i32 = 8031;

/// Whether the type is a guest building: a building that satisfies the need `building` (guest
/// buildings 3.4). The compost does not, and pays nothing.
pub fn is_guest_building(ty: &EntityType) -> bool {
    ty.class == "building"
        && ty
            .config
            .as_deref()
            .and_then(|c| c.section("Satisfies"))
            .is_some_and(|s| s.has("building"))
}

/// The monthly upkeep of a building type: `cUpkeep`, 0 when the file has none or for the
/// helicopter base (guest buildings 9).
pub fn upkeep(ty: &EntityType) -> f32 {
    let key = ty
        .config
        .as_deref()
        .and_then(|c| c.section("Characteristics/Floats")?.get("cUpkeep"))
        .map(to_float)
        .filter(|v| v.is_finite());
    match ty.name_id {
        HELICOPTER_BASE => 0.0,
        _ => key.unwrap_or(0.0),
    }
}

/// The monthly upkeep of building `id` of type `ty`: its current upgrade's, else the type's
/// (research 6.3, guest buildings 9); the helicopter base costs nothing whatever it holds.
pub fn upkeep_of(units: &Units, ty: &EntityType, id: u32) -> f32 {
    match (ty.name_id, upgrade::current(units, ty.name_id, id)) {
        (HELICOPTER_BASE, _) => 0.0,
        (_, Some(u)) => u.upkeep as f32,
        _ => upkeep(ty),
    }
}

/// The wages and then the upkeep of the month that ended, booked in the month that begins. One
/// booking per staff member in list order, one for all the buildings, even when it is 0. The log
/// gets `wages ID AMOUNT` for each member and `upkeep AMOUNT`.
pub fn charge(world: &mut World, cat: &Catalogue, money: &mut Money, log: &mut Vec<String>) {
    let mut total = 0.0_f32;
    for e in &world.zoo.entities {
        let Some(ty) = type_of(cat, e) else { continue };
        if let Some(category) = staff::monthly_wages(ty) {
            let wage = staff::monthly(ty);
            money.spend(category, wage);
            log.push(format!(
                "wages {} {wage}",
                e.common.as_ref().map_or(0, |c| c.id)
            ));
        } else if is_guest_building(ty) {
            let id = e.common.as_ref().map_or(0, |c| c.id);
            let u = upkeep_of(&world.units, ty, id);
            total += u;
            // Each building rolls its monthly figures in the same loop (guest buildings 9).
            if let Some(c) = e.common.as_ref() {
                let default = world.units.prices.default;
                (world.units.buildings.entry(c.id))
                    .or_insert_with(|| Live::new(&Kind::of(ty, default)))
                    .roll(u);
            }
        }
    }
    money.spend(BUILDING_UPKEEP, total);
    log.push(format!("upkeep {total}"));
}

#[cfg(test)]
mod tests;
