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
use crate::single;

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
    pub id: u32,
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
    /// The animals the exhibit holds as (id, happiness), in list order, and how many stand on the
    /// tiles the area sees.
    pub all: Vec<(u32, i32)>,
    pub animals: usize,
    /// The species in view (name ids, in order) and how many animals in view are sick.
    pub species: Vec<i32>,
    pub sick: usize,
    /// The crowding of the area: ten times the guests per tile, rounded.
    pub crowd: i32,
    /// The average happiness of all the animals of the exhibit.
    pub average: i32,
    /// The tour bonus of the area: `cTourGuideBonus` of the first guide on its tiles (habitat
    /// rating 10.4).
    pub bonus: i32,
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
            id: c.id,
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

/// The tile lists of the areas of exhibit `id`, in the order of its areas, of those that see a
/// tile (goals 11.1).
pub fn viewing_areas(world: &mut World, cat: &Catalogue, id: u32) -> Vec<Vec<usize>> {
    refresh(world, cat);
    let areas = world.units.views.areas.iter().find(|(x, _)| *x == id);
    let kept = areas.into_iter().flat_map(|(_, areas)| areas);
    kept.filter(|a| !a.seen.is_empty())
        .map(|a| a.tiles.clone())
        .collect()
}

/// A random viewing tile of exhibit `id` (goals 11.1): one draw picks one of the areas that see a
/// tile, a second one a tile of that area's own list. None, and no draw, when no area sees a tile.
pub fn viewing_tile(world: &mut World, cat: &Catalogue, id: u32, rng: &mut Rng) -> Option<usize> {
    let areas = viewing_areas(world, cat, id);
    let tiles = areas.get(rng.pick(areas.len() as u32) as usize)?;
    tiles.get(rng.pick(tiles.len() as u32) as usize).copied()
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
    // The exhibit's animal list (6.3): the tiles in their own order, then each tile's unit list.
    let mut held = beasts(world, cat);
    let all: Vec<Beast> = crate::animal::exhibit_animals(world, *exhibit)
        .into_iter()
        .filter_map(|at| {
            let id = world.zoo.entities[at].common.as_ref()?.id;
            let i = held.iter().position(|b| b.id == id)?;
            Some(held.swap_remove(i))
        })
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
    let bonus = (area.tiles.iter()).find_map(|&t| {
        let mut guides = (world.zoo.entities.iter().enumerate())
            .filter(|(at, e)| e.class == "tour" && tile_of(world, *at) == Some(t));
        guides.find_map(|(_, e)| {
            let b = crate::staff::Crowd::of(crate::place::bulldozer::type_of(cat, e)).bonus;
            let carried = e.common.as_ref().is_none_or(|c| world.is_carried(c.id));
            (b != 0 && !carried).then_some(b)
        })
    });
    Some(Sight {
        exhibit: *exhibit,
        all: all.iter().map(|b| (b.id, b.happy)).collect(),
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
        bonus: bonus.unwrap_or(0),
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

/// 3.3 and 3.4: the attractions (the buildings that satisfy `fun`, in the order of the list) with
/// the tile each stands on, and of them those the guest has not seen (3.2): the seen list holds
/// the building, or an entity of the same help id, on one of its tiles.
pub fn attractions(world: &World, cat: &Catalogue, g: &Guest) -> (usize, Vec<(u32, usize)>) {
    let (ids, helps) = gawk::marks(world, cat, g);
    let default = world.units.prices.default;
    let mut all = 0;
    let mut unseen = Vec::new();
    for e in &world.zoo.entities {
        let (Some(kind), Some(c)) = (crate::building::kind_of(cat, e, default), e.common.as_ref())
        else {
            continue;
        };
        let tile = world.tile_index(c.x.div_euclid(64), c.y.div_euclid(64));
        let Some(tile) = tile.filter(|_| kind.satisfies("fun")) else {
            continue;
        };
        all += 1;
        let seen = g.seen.contains(&tile)
            || ids.contains(&c.id)
            || gawk::help_id(cat, e).is_some_and(|h| helps.contains(&h));
        if !seen {
            unseen.push((c.id, tile));
        }
    }
    (all, unseen)
}

/// The attractiveness of an exhibit, rated first when it is marked (habitat rating 13.1).
pub fn attractiveness(world: &mut World, cat: &Catalogue, id: u32, rng: &mut Rng) -> i32 {
    crate::exhibit::rating::refresh(world, cat, id, &mut || rng.rand());
    world
        .exhibits()
        .get(id)
        .map_or(0, |x| x.rating.attractiveness)
}

/// 6.3: the animal of a thought, drawn from `all` for the band `lo..=hi` of happiness: the first
/// in the band at or after the drawn index, else the last in it before. One draw, none for an
/// empty list.
fn pick(rng: &mut Rng, all: &[(u32, i32)], band: std::ops::RangeInclusive<i32>) -> Option<u32> {
    let k = rng.pick(all.len() as u32) as usize;
    let in_band = |a: &&(u32, i32)| band.contains(&a.1);
    let (before, after) = all.split_at(k);
    (after
        .iter()
        .find(in_band)
        .or_else(|| before.iter().rfind(in_band)))
    .map(|a| a.0)
}

/// 6: the viewing check of a guest on `tile`: the happiness changes by what the exhibit shows.
/// The thoughts it gives are returned in order, as the string id and the animal they are about.
pub fn check(
    g: &mut Guest,
    k: &Keys,
    world: &mut World,
    cat: &Catalogue,
    rng: &mut Rng,
    tile: Option<usize>,
) -> Vec<(i32, Option<u32>)> {
    g.va = lookup(world, cat, tile, rng);
    let Some(s) = tile
        .zip(g.va)
        .and_then(|(t, dir)| sight(world, cat, t, dir))
    else {
        return Vec::new();
    };
    if s.animals == 0 {
        return Vec::new();
    }
    let (v, mut change, mut thoughts) = (&k.view, 0i32, Vec::new());
    if s.sick > 0 {
        change += v.sick;
        if s.sick > 1 {
            thoughts.push((10096, None));
        }
        let sick = pick(rng, &s.all, -100..=100);
        thoughts.extend(sick.map(|a| (10545, Some(a))));
    }
    let (band, thought);
    (change, band, thought) = match s.average {
        81.. => (change + v.happy[0], 81..=100, 10098),
        61..=80 => (change + v.happy[1], 61..=80, 10281),
        0..=60 => (change + v.angry[0], 0..=60, 10282),
        -60..=-1 => (change + v.angry[0], -61..=-1, 10284),
        -80..=-61 => (change + v.angry[1], -80..=-61, 10283),
        _ => (change + v.angry[2], -100..=-81, 10099),
    };
    let mood = pick(rng, &s.all, band);
    thoughts.extend(mood.map(|a| (thought, Some(a))));
    if s.species.len() as i32 >= v.species.0 {
        change += v.species.1;
        thoughts.push((10100, None));
    }
    if s.crowd >= v.crowd.0 {
        change += v.crowd.1;
        let hidden = pick(rng, &s.all, -100..=100);
        thoughts.extend(hidden.map(|a| (10102, Some(a))));
    }
    g.pending[HAPPINESS] = g.pending[HAPPINESS].saturating_add(change);
    thoughts
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
    // The calendar's milliseconds (it runs 7,200 times faster than the play) in units of 100 ns.
    let t = (world.units.now.saturating_sub(g.viewed_at).max(0) as u64).saturating_mul(10_000);
    let factor = f64::from(world.units.donation_factor);
    // Each operation rounded to 24 bits: h + b first, then the products in the order written.
    let happy = single(f64::from(s.average) + f64::from(s.bonus));
    let raw = single(single(single(t as f64 * factor) * happy) * 0.01);
    let d = if raw.is_nan() { 0.0 } else { raw.min(10.0) } as f32;
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
