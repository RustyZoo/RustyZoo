//! The guest-name cheats other than the research ones (cheats 4.2): the effects, run by the name
//! hook. The named guest is the one the guest panel has selected.

use super::App;
use crate::build::number;
use crate::cheat::{Cheat, spread};
use crate::entity::Catalogue;
use crate::goal::{IN_BUILDING, stop_use};
use crate::guest::Guest;
use crate::money::Money;
use crate::place::bulldozer::type_of;
use crate::rubble::{Kit, PIECE_NAME, turn_to_rubble};
use crate::unit::Part;
use crate::world::World;
use crate::zoo::Entity;

/// The scenery of the cars and the lab jeep: name ids (cheats 4.2 D).
const CARS: [i32; 6] = [6138, 6139, 6140, 6142, 6145, 6167];
/// What `cars_to_rubble` gives to the donations and the cash.
const CAR_GIFT: f32 = 5500.0;
/// The palettes of `guests_sick_recolour`: shirt and trousers.
const SICK_PALETTE: [u8; 2] = [5, 6];

/// The ids of the guests, in list order.
fn guest_ids(world: &World) -> Vec<u32> {
    (world.zoo.entities.iter())
        .filter(|e| e.class == "guests")
        .filter_map(|e| Some(e.common.as_ref()?.id))
        .collect()
}

/// Applies `f` to the live state of every guest.
fn each_guest(world: &mut World, mut f: impl FnMut(&mut Guest)) {
    let ids = guest_ids(world);
    for id in ids {
        if let Some(Part::Guest(g)) = world.units.get_mut(id).map(|u| &mut u.part) {
            f(g);
        }
    }
}

/// Sets the first two palette bytes of the entity at `at`, if it has that many.
fn recolour(world: &mut World, at: usize, [first, second]: [u8; 2]) {
    if let Some(c) = world.zoo.entities[at].common.as_mut()
        && let [a, b, ..] = &mut c.colors[..]
    {
        (*a, *b) = (first, second);
    }
}

/// `palette_set_N` (4.2 A): the shirt and trousers or skirt of every guest, then the first two
/// palettes of every recolourable building. Only what exists now.
pub(super) fn palette_set(world: &mut World, cat: &Catalogue, guests: [u8; 3], buildings: [u8; 2]) {
    for at in 0..world.zoo.entities.len() {
        let e = &world.zoo.entities[at];
        let Some(id) = e.common.as_ref().map(|c| c.id) else {
            continue;
        };
        if e.class == "guests" {
            let female = matches!(world.units.get(id).map(|u| &u.part),
                Some(Part::Guest(g)) if g.female);
            recolour(world, at, [guests[0], guests[if female { 2 } else { 1 }]]);
        } else if e.class == "building"
            && type_of(cat, e)
                .is_some_and(|t| number(t, "cIsColorReplaced").is_some_and(|n| n != 0.0))
        {
            recolour(world, at, buildings);
        }
    }
}

/// `guests_sick_recolour` (4.2 C): every guest that is not carried gets the palettes 5 and 6; one
/// that is not busy in a building (cheats 10 O2) also stops its goals and is sick.
pub(super) fn guests_sick(world: &mut World) {
    let ids = guest_ids(world);
    for id in ids {
        let Some(unit) = world.units.get_mut(id).filter(|u| !u.suspended()) else {
            continue;
        };
        if !IN_BUILDING.contains(&unit.goal.record.task) {
            stop_use(unit);
            if let Part::Guest(g) = &mut unit.part {
                g.sick = true;
            }
        }
        if let Some(at) = world.find(id) {
            recolour(world, at, SICK_PALETTE);
        }
    }
}

/// `cars_to_rubble` (4.2 D): every car turns to rubble (4.6), and when at least one did the
/// donations and the cash gain 5500. `before` is told each id just before. The number of cars.
pub(super) fn remove_cars(
    (world, money): (&mut World, &mut Money),
    kit: &mut Kit,
    mut before: impl FnMut(u32),
) -> usize {
    let car = |e: &&Entity| type_of(kit.cat, e).is_some_and(|t| CARS.contains(&t.name_id));
    let ids: Vec<u32> = (world.zoo.entities.iter())
        .filter(car)
        .filter_map(|e| Some(e.common.as_ref()?.id))
        .collect();
    for &id in &ids {
        before(id);
        turn_to_rubble(world, kit, id);
    }
    if !ids.is_empty() {
        money.refund(crate::money::DONATIONS, CAR_GIFT);
    }
    ids.len()
}

/// The contagion cheats (4.2 E): the guest at `at` takes the bit. Sickness needs any guest, the
/// others a man (the type name starts with `m`); the painting ones also paint its own shirt.
pub(super) fn spreads(world: &mut World, at: usize, bit: u8) {
    let e = &world.zoo.entities[at];
    let Some(id) = e.common.as_ref().map(|c| c.id) else {
        return;
    };
    if e.class != "guests" || (bit != spread::SICKNESS && !e.type_name.starts_with('m')) {
        return;
    }
    if let Some(unit) = world.units.get_mut(id) {
        unit.spreads |= bit;
    }
    let shirt = match bit {
        spread::YELLOW => spread::YELLOW_SHIRT,
        spread::RED => spread::RED_SHIRT,
        _ => return,
    };
    if let Some(c) = world.zoo.entities[at].common.as_mut()
        && let Some(first) = c.colors.first_mut()
    {
        *first = shirt;
    }
}

impl App<'_> {
    /// Runs one of the guest-name cheats; the other cheats are the name hook's.
    pub(super) fn guest_cheat(&mut self, cheat: Cheat) {
        let cat = self.renderer.catalogue();
        let Some(game) = &mut self.game else { return };
        match cheat {
            Cheat::Palette { guests, buildings } => {
                palette_set(&mut game.world, cat, guests, buildings)
            }
            Cheat::GuestsLeave => each_guest(&mut game.world, |g| g.leave = true),
            Cheat::FencesBreak => {
                let mut lines = Vec::new();
                let events = game.world.break_fences(cat, &mut lines);
                lines.into_iter().for_each(|l| self.sim.note(l));
                events.into_iter().for_each(|e| self.sim.post(e));
            }
            Cheat::GuestsSick => guests_sick(&mut game.world),
            Cheat::UnlockScenarios => {
                if let Some(menu) = &mut self.menu {
                    self.menu_events.extend(menu.unlock_all_scenarios());
                }
            }
            Cheat::EggsHatchFast => self.cheats.eggs_hatch_fast = true,
            Cheat::CarsToRubble => {
                let (carry, selected, pending) =
                    (&mut game.carry, &mut game.selected, &mut game.pending);
                let held = &mut game.held;
                let template = (self.ui.as_ref()).map_or(String::new(), |u| u.string(PIECE_NAME));
                let renderer = &self.renderer;
                let shape = |e: &Entity| renderer.shape(e);
                let mut kit = Kit {
                    cat,
                    rng: &mut self.sim.rng,
                    template: &template,
                    shape: &shape,
                };
                remove_cars((&mut game.world, &mut game.money), &mut kit, |id| {
                    // The map view lets it go (scenarios 7.5).
                    if carry.as_ref().is_some_and(|c| c.id == id) {
                        (*carry, *held, *pending) = (None, None, None);
                    }
                    if *selected == Some(id) {
                        *selected = None;
                    }
                });
            }
            Cheat::Spread(bit) => {
                let at = game.guest.selected.and_then(|id| game.world.find(id));
                if let Some(at) = at {
                    spreads(&mut game.world, at, bit);
                }
            }
            _ => {}
        }
    }
}
