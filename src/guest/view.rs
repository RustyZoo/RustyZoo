//! What a guest sees: the viewing area it stands on (guests visit 4), the sites it can still
//! visit (3), the viewing check (6) and the donation when it walks away (7).
//!
//! The areas of every exhibit and the slots of the tiles are built once after each change of the
//! map ([`Views`], kept in `World::units`). A guest keeps only the direction in which it found
//! its area ([`Guest::va`]) and looks again from the tile it stands on at each use, as the
//! original does.

use std::collections::HashMap;

use super::*;
use crate::exhibit::viewing::{self, Area, Ground, Plan};
use crate::exhibit::{EXTERIOR, GROUNDS};

/// The viewing areas of the exhibits and, per tile, the area each of the eight directions looks
/// through, as (position of the exhibit, position of the area).
#[derive(Debug, Default)]
pub struct Views {
    edits: Option<u64>,
    areas: Vec<(u32, Vec<Area>)>,
    slots: HashMap<usize, [Option<(usize, usize)>; 8]>,
}

/// An animal of an exhibit as the guests judge it.
pub struct Beast {
    pub exhibit: u32,
    pub tile: usize,
    pub name: i32,
    pub happy: i32,
    pub sick: bool,
}

/// What a viewing area shows of its exhibit (4.3).
#[derive(Debug, PartialEq)]
pub struct Sight {
    pub exhibit: u32,
    /// The animals of the exhibit, and those in view with their species in order and how many
    /// are sick.
    pub total: u32,
    pub animals: usize,
    pub species: Vec<i32>,
    pub sick: usize,
    /// The crowding of the area: ten times the guests per tile, rounded.
    pub crowd: i32,
    /// The average happiness of all the animals of the exhibit.
    pub average: i32,
}

/// The tile a unit stands on.
pub fn tile_of(world: &World, at: usize) -> Option<usize> {
    let c = world.zoo.entities.get(at)?.common.as_ref()?;
    world.tile_index(c.x.div_euclid(64), c.y.div_euclid(64))
}

/// The animals that stand in exhibits, in entity order.
pub fn beasts(world: &World, cat: &Catalogue) -> Vec<Beast> {
    let bytes = |s: &str| s.chars().map(|c| c as u8).collect::<Vec<u8>>();
    let mut list = Vec::new();
    for (at, e) in world.zoo.entities.iter().enumerate() {
        let (Some(c), "animals") = (&e.common, e.class.as_str()) else {
            continue;
        };
        let (Some(tile), false) = (tile_of(world, at), world.is_carried(c.id)) else {
            continue;
        };
        let exhibit = world
            .exhibits()
            .region(tile)
            .filter(|&r| r != GROUNDS && r != EXTERIOR);
        let part = world.units.get(c.id).map(|u| &u.part);
        let (Some(exhibit), Some(Part::Animal(a))) = (exhibit, part) else {
            continue;
        };
        let ty = cat.find(&e.class, &bytes(&e.type_name), &bytes(&e.subtype));
        let name = ty.map_or(0, |t| t.name_id);
        list.push(Beast {
            exhibit,
            tile,
            name,
            happy: a.happiness,
            sick: a.dying,
        });
    }
    list
}

/// Builds the areas again when the map changed since they were.
fn refresh(world: &mut World, cat: &Catalogue) {
    if world.units.views.edits == Some(world.edits) {
        return;
    }
    let ground = Ground::of(world, viewing::bodies(cat));
    let mut views = Views {
        edits: Some(world.edits),
        ..Views::default()
    };
    for (e, x) in world.exhibits().list().iter().enumerate() {
        let plan = Plan::of(world, x.id);
        let mut areas = viewing::areas(&plan);
        for (a, area) in areas.iter_mut().enumerate() {
            area.observe(&plan, &ground);
            for (&t, &looks) in area.tiles.iter().zip(&area.looks) {
                for k in (0..8).filter(|k| looks >> k & 1 == 1) {
                    views.slots.entry(t).or_default()[k] = Some((e, a));
                }
            }
        }
        views.areas.push((x.id, areas));
    }
    world.units.views = views;
}

/// The tiles from which guests look at exhibit `id`, in the order of its areas (3.7).
pub fn viewing_tiles(world: &mut World, cat: &Catalogue, id: u32) -> Vec<usize> {
    refresh(world, cat);
    let areas = world.units.views.areas.iter().find(|(x, _)| *x == id);
    let tiles = areas
        .into_iter()
        .flat_map(|(_, areas)| areas)
        .flat_map(|a| a.tiles.iter().zip(&a.looks));
    tiles
        .filter(|(_, looks)| **looks != 0)
        .map(|(t, _)| *t)
        .collect()
}

/// 4.1: one draw gives the direction to start from; the first of the eight that holds an area
/// the guest on `tile` looks through is kept.
pub fn lookup(
    world: &mut World,
    cat: &Catalogue,
    tile: Option<usize>,
    rng: &mut Rng,
) -> Option<u8> {
    let first = (rng.rand() & 7) as u8;
    refresh(world, cat);
    let tile = tile.filter(|&t| world.exhibits().region(t) == Some(GROUNDS))?;
    (0..8)
        .map(|i| (first + i) & 7)
        .find(|&k| through(world, tile, k).is_some())
}

/// The area the tile looks through in direction `k`, if it sees any tile (4.1 point 3).
fn through(world: &World, tile: usize, k: u8) -> Option<(usize, usize)> {
    let (e, a) = world.units.views.slots.get(&tile)?[usize::from(k)]?;
    (!world.units.views.areas[e].1[a].seen.is_empty()).then_some((e, a))
}

/// 4.2 and 4.3: what the guest on `tile` sees looking in direction `k`.
pub fn sight(world: &mut World, cat: &Catalogue, tile: usize, k: u8) -> Option<Sight> {
    refresh(world, cat);
    if world.exhibits().region(tile) != Some(GROUNDS) {
        return None;
    }
    let (e, a) = through(world, tile, k)?;
    let (exhibit, areas) = &world.units.views.areas[e];
    let area = &areas[a];
    let all: Vec<Beast> = beasts(world, cat)
        .into_iter()
        .filter(|b| b.exhibit == *exhibit)
        .collect();
    let seen: Vec<&Beast> = area
        .seen
        .iter()
        .flat_map(|&t| all.iter().filter(move |b| b.tile == t))
        .collect();
    let mut species: Vec<i32> = Vec::new();
    for b in &seen {
        if !species.contains(&b.name) {
            species.push(b.name);
        }
    }
    let guests = (world.zoo.entities.iter().enumerate())
        .filter(|(_, e)| e.class == "guests")
        .filter(|(at, _)| tile_of(world, *at).is_some_and(|t| area.tiles.contains(&t)))
        .count();
    Some(Sight {
        exhibit: *exhibit,
        total: all.len() as u32,
        animals: seen.len(),
        sick: seen.iter().filter(|b| b.sick).count(),
        species,
        crowd: (10.0 * guests as f32 / area.tiles.len() as f32).round_ties_even() as i32,
        average: all
            .iter()
            .map(|b| b.happy)
            .sum::<i32>()
            .checked_div(all.len() as i32)
            .unwrap_or(0),
    })
}

/// Whether the guest has seen exhibit `id` (3.2): a tile of its list belongs to it.
pub fn seen(world: &World, g: &Guest, id: u32) -> bool {
    g.seen
        .iter()
        .any(|&t| world.exhibits().region(t) == Some(id))
}

/// 3.5: the tile of a site joins the list, which keeps 10, and the count.
pub fn see(g: &mut Guest, tile: usize) {
    g.seen.push(tile);
    g.seen_count += 1;
    if g.seen.len() > 10 {
        g.seen.remove(0);
    }
}

/// 3.3 and 3.4: the exhibits that hold an animal, and of them those the guest has not seen and
/// that are not show tanks.
pub fn sites(world: &World, cat: &Catalogue, g: &Guest) -> (usize, Vec<u32>) {
    let held = beasts(world, cat);
    let with = |x: &&crate::exhibit::Exhibit| held.iter().any(|b| b.exhibit == x.id);
    let all: Vec<_> = world.exhibits().list().iter().filter(with).collect();
    let show = |x: &crate::exhibit::Exhibit| x.tank.is_some_and(|t| t.show);
    let unseen = all.iter().filter(|x| !show(x) && !seen(world, g, x.id));
    (all.len(), unseen.map(|x| x.id).collect())
}

/// The attractiveness of an exhibit, rated first when it is marked (habitat rating 13.1).
pub fn attractiveness(world: &mut World, cat: &Catalogue, id: u32) -> i32 {
    if world.exhibits().get(id).is_some_and(|x| x.rating.dirty) {
        let input = crate::exhibit::rating::input(world, cat, id);
        world
            .exhibits_mut()
            .get_mut(id)
            .map(|x| x.rating.compute(&input));
    }
    world
        .exhibits()
        .get(id)
        .map_or(0, |x| x.rating.attractiveness)
}

/// 6: the viewing check of a guest on `tile`: the happiness changes by what the exhibit shows.
/// The draws of the thoughts' animals are made; the thoughts are not given yet.
pub fn check(
    g: &mut Guest,
    k: &Keys,
    world: &mut World,
    cat: &Catalogue,
    rng: &mut Rng,
    tile: Option<usize>,
) {
    g.va = lookup(world, cat, tile, rng);
    let Some(s) = tile
        .zip(g.va)
        .and_then(|(t, dir)| sight(world, cat, t, dir))
    else {
        return;
    };
    if s.animals == 0 {
        return;
    }
    let (v, mut change) = (&k.view, 0i32);
    if s.sick > 0 {
        change += v.sick;
        rng.pick(s.total);
    }
    change += match s.average {
        81.. => v.happy[0],
        61..=80 => v.happy[1],
        -60..=60 => v.angry[0],
        -80..=-61 => v.angry[1],
        _ => v.angry[2],
    };
    rng.pick(s.total);
    if s.species.len() as i32 >= v.species.0 {
        change += v.species.1;
    }
    if s.crowd >= v.crowd.0 {
        change += v.crowd.1;
        rng.pick(s.total);
    }
    g.pending[HAPPINESS] = g.pending[HAPPINESS].saturating_add(change);
}

/// 7: the guest on `tile` stops viewing: the exhibit gets the time it watched and, when it saw
/// animals, none sick and no crowd, a donation.
pub fn leave_area(g: &mut Guest, id: u32, world: &mut World, cat: &Catalogue, tile: Option<usize>) {
    let sight = tile
        .zip(g.va.take())
        .and_then(|(t, dir)| sight(world, cat, t, dir));
    let Some(s) = sight else {
        return;
    };
    // The calendar runs 7,200 seconds for each simulated second; the unit is 100 ns.
    let t = world
        .units
        .now
        .saturating_sub(g.viewed_at)
        .saturating_mul(72_000_000);
    let factor = f64::from(world.units.donation_factor);
    let d = (t as f64 * factor * f64::from(s.average) * 0.01).min(10.0) as f32;
    let give = s.animals > 0 && s.sick == 0 && s.crowd < 50 && d > 0.0;
    let Some(x) = world.exhibits_mut().get_mut(s.exhibit) else {
        return;
    };
    x.stats.viewing = x.stats.viewing.saturating_add(t);
    if give {
        x.stats.donations[1] += d;
        x.stats.donations[2] += d;
        world.units.donated.push(d);
        world
            .units
            .guest_note(format!("donate {id} {} {d:.2}", s.exhibit));
    }
}

#[cfg(test)]
mod tests;
