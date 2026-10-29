//! The upgrades of the building types (research 6.3; guest buildings 6.6, 9, 10.3): the list a
//! type holds, filled by the programs that are complete, and the index of the upgrade each
//! building uses ([`Units::upgrade_index`]).

use crate::unit::Units;
use std::collections::BTreeMap;

/// The most upgrade buttons the Programs tab has.
pub const BUTTONS: usize = 5;

/// One upgrade: its program's name id (the tooltip), the animation of its button, the monthly
/// upkeep and the happiness change of an adult and of a child who leaves.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Upgrade {
    pub name: i32,
    pub icon: String,
    pub upkeep: i32,
    pub adult: i32,
    pub child: i32,
}

/// The upgrades of each building type, by its name id, in the order they were added.
pub type Upgrades = BTreeMap<i32, Vec<Upgrade>>;

/// Adds `upgrade` to the type named `target`, unless it holds one of the same program name id
/// already.
pub fn add(list: &mut Upgrades, target: i32, upgrade: Upgrade) {
    let held = list.entry(target).or_default();
    if held.iter().all(|u| u.name != upgrade.name) {
        held.push(upgrade);
    }
}

/// The upgrade building `id` of the type named `name` uses: the list entry at its index, none
/// when the index is out of range or the list is empty.
pub fn current(units: &Units, name: i32, id: u32) -> Option<&Upgrade> {
    let index = units.upgrade_index.get(&id).copied().unwrap_or(0);
    units.upgrades.get(&name)?.get(usize::try_from(index).ok()?)
}

/// The player picked the button at position `n`: the index of building `id` of the type named
/// `name` becomes `n`, when the type has such an upgrade. Whether it did.
pub fn choose(units: &mut Units, name: i32, id: u32, n: usize) -> bool {
    if n >= units.upgrades.get(&name).map_or(0, Vec::len) {
        return false;
    }
    match n {
        0 => units.upgrade_index.remove(&id),
        _ => units.upgrade_index.insert(id, n as u32),
    };
    true
}

#[cfg(test)]
mod tests;
