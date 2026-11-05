//! The admission of a show and the price of its grandstands (shows 33): the show panel edits the
//! admission of one species of a show tank and writes it into the price of every grandstand that
//! faces the show. The panel itself is not built yet; these are the steps it will call.

use super::{Sight, info_slot};
use crate::building::{Live, kind_of};
use crate::entity::Catalogue;
use crate::timing::file_time;
use crate::world::World;

/// `update_admission()` (shows 33.6) for show tank `exhibit` and the selected species `species`:
/// the admission shown (0 with no species) is written into the price of the grandstands that face
/// the show, clamped as a building price is (guest buildings 7.2). The lookup of the species makes
/// a missing record. An exhibit with no show info changes nothing. Returns the admission shown.
pub fn update_admission(
    world: &mut World,
    cat: &Catalogue,
    exhibit: u32,
    species: Option<u32>,
) -> f32 {
    let Some(n) = info_slot(world, exhibit) else {
        return 0.0;
    };
    let date = file_time(world.units.now);
    let Some(info) = world.zoo.shows.infos[n].as_mut() else {
        return 0.0;
    };
    let shown = species.map_or(0.0, |s| f32::from_bits(info.lookup(s, date).admission));
    let id = info.id;
    let (max, default) = (world.units.prices.max, world.units.prices.default);
    let sight = Sight::of(world, cat);
    for &(stand, at) in &sight.stands {
        if sight.seen(world, at) != id {
            continue;
        }
        let Some(kind) = kind_of(cat, &world.zoo.entities[at], default) else {
            continue;
        };
        let price = shown.max(0.0).min(max).max(kind.low).min(kind.high);
        (world.units.buildings.entry(stand))
            .or_insert_with(|| Live::new(&kind))
            .price = price;
    }
    shown
}

/// The record's admission := `new(increment, max, min, current)`, then [`update_admission`]. Only
/// with a species selected. Returns the admission shown.
fn store(
    world: &mut World,
    cat: &Catalogue,
    exhibit: u32,
    species: Option<u32>,
    new: impl FnOnce(f32, f32, f32, f32) -> f32,
) -> f32 {
    let (inc, max, min) = {
        let p = &world.units.show;
        (p.increment, p.max_admission, p.min_admission)
    };
    let date = file_time(world.units.now);
    let record = info_slot(world, exhibit)
        .zip(species)
        .and_then(|(n, s)| Some(world.zoo.shows.infos[n].as_mut()?.lookup(s, date)));
    if let Some(record) = record {
        record.admission = new(inc, max, min, f32::from_bits(record.admission)).to_bits();
    }
    update_admission(world, cat, exhibit, species)
}

/// The up or down button (shows 33.4): the admission moves by the increment and stops at the
/// maximum (up) or minimum (down) once it reaches it.
pub fn press_admission(
    world: &mut World,
    cat: &Catalogue,
    exhibit: u32,
    species: Option<u32>,
    up: bool,
) -> f32 {
    store(world, cat, exhibit, species, |inc, max, min, a| {
        match up {
            true if a + inc >= max => max,
            true => a + inc,
            false if a - inc <= min => min,
            false => a - inc,
        }
    })
}

/// A number typed into the admission field (shows 33.5): at or above the maximum it is the
/// maximum, at or below the minimum the minimum.
pub fn type_admission(
    world: &mut World,
    cat: &Catalogue,
    exhibit: u32,
    species: Option<u32>,
    typed: f32,
) -> f32 {
    store(world, cat, exhibit, species, |_, max, min, _| {
        if typed >= max {
            max
        } else if typed > min {
            typed
        } else {
            min
        }
    })
}
