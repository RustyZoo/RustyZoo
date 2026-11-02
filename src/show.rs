//! Shows (shows 3, 4, 10): the numbers of `shows.cfg`, which show a grandstand belongs to, and the
//! two routines that empty the grandstands of a show.
//!
//! The state itself, the scripts and the show info of each tank, is [`crate::zoo::Shows`], loaded
//! and saved with the zoo. The show cycle, the audience and the figures are not here yet: a show
//! never starts, so the abort that `empty_grandstands` asks of a running show (shows 5.6) is left
//! to the cycle.

use crate::building::{kind_of, leave::push_out};
use crate::config::{Config, to_float, to_int};
use crate::entity::Catalogue;
use crate::exhibit::viewing::{self, Ground};
use crate::vfs::Vfs;
use crate::world::World;
use crate::zoo::ShowInfo;

pub mod trick;
mod units;
pub use units::{Move, counted, settle};

/// The numbers of `shows.cfg` that the cycle reads (shows 3.1). The built-in values hold without
/// the file, which only Marine Mania has.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Params {
    /// The satisfaction of a trick at or below which it is bad, at or above which it is good and great.
    pub trick: [i32; 3],
    /// The tricks a show should have, at least and at most.
    pub ideal: [i32; 2],
    /// The percent of a show at or below which it is bad, at or above which it is good and great.
    pub show: [i32; 3],
    /// The complexity a default script adds up to before it offers the food trick (shows 19.2).
    pub threshold: i32,
    /// The admission a type record takes when a script gives it its first (`defaultShowAdmission`).
    pub admission: f32,
}

impl Default for Params {
    fn default() -> Params {
        Params {
            trick: [0, 3, 6],
            ideal: [6, 6],
            show: [25, 50, 75],
            threshold: 5,
            admission: 0.0,
        }
    }
}

impl Params {
    /// The `shows.cfg` of the game, or the built-in numbers without one (or one that cannot be read).
    pub fn load(vfs: &Vfs) -> Params {
        let config = vfs.read("shows.cfg").ok();
        config
            .and_then(|bytes| Config::parse(&bytes).ok())
            .map_or_else(Params::default, |c| Params::read(&c))
    }

    /// Reads a `shows.cfg`; a key that is missing keeps the built-in number.
    pub fn read(config: &Config) -> Params {
        let number = |section: &str, key: &str, fallback: i32| {
            let value = config.section(section).and_then(|s| s.get(key));
            value.and_then(to_int).unwrap_or(fallback)
        };
        let found = Params::default();
        let (trick, ideal, show) = (
            ["badTrick", "goodTrick", "greatTrick"],
            ["minIdealLength", "maxIdealLength"],
            ["badShow", "goodShow", "greatShow"],
        );
        Params {
            trick: std::array::from_fn(|i| {
                number("trickSatisfactionThresholds", trick[i], found.trick[i])
            }),
            ideal: std::array::from_fn(|i| number("show", ideal[i], found.ideal[i])),
            show: std::array::from_fn(|i| {
                number("showSatisfactionThresholds", show[i], found.show[i])
            }),
            threshold: number("behavior", "returnToKeeperThreshold", found.threshold),
            admission: config
                .section("cost")
                .and_then(|s| s.get("defaultShowAdmission"))
                .map_or(found.admission, to_float),
        }
    }
}

/// The direction a rotation faces, as a step in x and y; an odd rotation faces no way.
fn step(rotation: u32) -> Option<(i32, i32)> {
    match rotation % 8 {
        0 => Some((0, -1)),
        2 => Some((1, 0)),
        4 => Some((0, 1)),
        6 => Some((-1, 0)),
        _ => None,
    }
}

/// The show tanks of a world, in the exhibit manager's order: the exhibit and the show id of each
/// show info with a show id (shows 4.1).
fn show_tanks(world: &World) -> Vec<(u32, u16)> {
    (world.exhibits().list().iter())
        .filter_map(|x| Some((x.id, info_of(world, x.id)?.id)))
        .filter(|&(_, id)| id != 0)
        .collect()
}

/// The show info of the habitat record whose first tile lies in exhibit `id`.
pub fn info_of(world: &World, id: u32) -> Option<&ShowInfo> {
    world.zoo.shows.info(info_slot(world, id)?)
}

/// The position in the shows section of the show info of exhibit `id`, if it has one.
fn info_slot(world: &World, id: u32) -> Option<usize> {
    let tiles = world.zoo.habitat_tiles.iter();
    let n = tiles
        .map(|&(x, y)| world.tile_index(x, y))
        .position(|t| t.is_some_and(|t| world.exhibits().region(t) == Some(id)))?;
    world.zoo.shows.info(n).map(|_| n)
}

/// The exhibit of the show info whose show id is `id`.
fn exhibit_of(world: &World, id: u16) -> Option<u32> {
    let n = world
        .zoo
        .shows
        .infos
        .iter()
        .position(|i| i.as_ref().is_some_and(|i| i.id == id))?;
    let &(x, y) = world.zoo.habitat_tiles.get(n)?;
    world
        .exhibits()
        .region(world.tile_index(x, y)?)
        .filter(|&r| world.exhibits().get(r).is_some())
}

/// The grandstands of a world in the order of the building list, as (id, position in the entity
/// list).
fn grandstands(world: &World, cat: &Catalogue) -> Vec<(u32, usize)> {
    let default = world.units.prices.default;
    (world.zoo.entities.iter().enumerate())
        .filter(|(_, e)| kind_of(cat, e, default).is_some_and(|k| k.satisfies("grandstand")))
        .filter_map(|(at, e)| Some((e.common.as_ref()?.id, at)))
        .collect()
}

/// Whether grandstand `at` faces exhibit `exhibit` (shows 4.1): the tile four tiles in front of it
/// is in the exhibit and the line of sight from three to five tiles in front is clear. A tile off
/// the map is a no (the original reads a null tile there).
fn faces(world: &World, ground: &Ground, exhibit: u32, at: usize) -> bool {
    let Some(c) = world.zoo.entities[at].common.as_ref() else {
        return false;
    };
    let Some((dx, dy)) = step(c.rotation) else {
        return false;
    };
    let (x, y) = (c.x.div_euclid(64), c.y.div_euclid(64));
    let ahead = |n: i32| world.tile_index(x + n * dx, y + n * dy);
    let (Some(t3), Some(t4), Some(t5)) = (ahead(3), ahead(4), ahead(5)) else {
        return false;
    };
    world.exhibits().region(t4) == Some(exhibit) && ground.sees(t3, t5)
}

/// The pass for the building `id` just added to the map (placed, set down, put back): it runs only
/// when the building is a grandstand (shows 4.2); any other building runs none.
pub fn added(world: &mut World, cat: &Catalogue, id: u32) {
    let default = world.units.prices.default;
    let stand = world.find(id).is_some_and(|at| {
        kind_of(cat, &world.zoo.entities[at], default).is_some_and(|k| k.satisfies("grandstand"))
    });
    if stand {
        associate(world, cat);
    }
}

/// The pass of a zoo being loaded, for the entity `at` just read (shows 4.2): it runs only when
/// that entity is a grandstand, and sees only the entities read so far.
pub fn loaded(world: &mut World, cat: &Catalogue, at: usize) {
    let default = world.units.prices.default;
    if kind_of(cat, &world.zoo.entities[at], default).is_some_and(|k| k.satisfies("grandstand")) {
        associate_upto(world, cat, at);
    }
}

/// The association pass (shows 4.2): every grandstand that faces a show tank takes the show id of the
/// first one it faces. A grandstand that faces none keeps the id it has. Run once after a zoo is
/// loaded and when a grandstand is added to the map.
pub fn associate(world: &mut World, cat: &Catalogue) {
    associate_upto(world, cat, usize::MAX);
}

/// The pass over the entities up to index `last` only.
fn associate_upto(world: &mut World, cat: &Catalogue, last: usize) {
    let tanks = show_tanks(world);
    let stands: Vec<_> = grandstands(world, cat)
        .into_iter()
        .filter(|&(_, at)| at <= last)
        .collect();
    if tanks.is_empty() || stands.is_empty() {
        return;
    }
    let ground = Ground::until(world, viewing::bodies(cat), last);
    for (id, at) in stands {
        let Some(&(_, show)) = tanks.iter().find(|&&(x, _)| faces(world, &ground, x, at)) else {
            continue;
        };
        if let Some(live) = world.units.buildings.get_mut(&id) {
            live.show_id = show;
        }
    }
}

/// Shows 10.1: the grandstand `id` is emptied when it belongs to a show whose info exists. The
/// show is aborted first (shows 5.6; left to the cycle), the grandstand's "show soon" flag goes down
/// and the grandstands that face the show's tank lose their users.
pub fn empty_grandstands(world: &mut World, cat: &Catalogue, id: u32) {
    let Some(at) = world.find(id) else { return };
    let default = world.units.prices.default;
    if !kind_of(cat, &world.zoo.entities[at], default).is_some_and(|k| k.satisfies("grandstand")) {
        return;
    }
    let Some(show) = world.units.buildings.get(&id).map(|l| l.show_id) else {
        return;
    };
    if show == 0 || !has_info(world, show) {
        return;
    }
    if let Some(live) = world.units.buildings.get_mut(&id) {
        live.fill = 0;
    }
    empty_show_grandstands(world, cat, show);
}

/// Whether a show info with this show id exists.
fn has_info(world: &World, id: u16) -> bool {
    world.zoo.shows.infos.iter().flatten().any(|i| i.id == id)
}

/// Shows 10.2: every grandstand that faces the tank of show `id` loses all its users at once (a
/// forced leave) and its "show soon" flag goes down. Nothing happens for a show with no info or
/// whose tank is gone.
pub fn empty_show_grandstands(world: &mut World, cat: &Catalogue, id: u16) {
    let Some(exhibit) = exhibit_of(world, id) else {
        return;
    };
    let stands = grandstands(world, cat);
    if stands.is_empty() {
        return;
    }
    let ground = Ground::of(world, viewing::bodies(cat));
    let facing: Vec<u32> = (stands.iter())
        .filter(|&&(_, at)| faces(world, &ground, exhibit, at))
        .map(|&(id, _)| id)
        .collect();
    for id in facing {
        push_out(world, cat, id);
        if let Some(live) = world.units.buildings.get_mut(&id) {
            live.fill = 0;
        }
    }
}

#[cfg(test)]
mod tests;
