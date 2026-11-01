//! What a keeper looks at in an exhibit: his animals, his dung, whether the exhibit needs him
//! and which one he serves next (staff 7.2, 7.3, 7.6 to 7.8), land exhibits and tanks.

use std::cmp::Reverse;

use crate::animal::{self, food};
use crate::build::number;
use crate::entity::{Catalogue, EntityType};
use crate::exhibit::GROUNDS;
use crate::goal::DUTIES;
use crate::panel::exhibit::animals_in;
use crate::path::{self, Mover};
use crate::place::bulldozer::type_of;
use crate::unit::{Kind, Part};
use crate::world::World;

/// More AI ticks since the last service than this make a second keeper welcome (7.3).
const LONG: u32 = 1000;

/// An animal of the keeper's kind in an exhibit.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Seen {
    pub id: u32,
    pub tile: usize,
    /// Health at most `100 - cSicklyAnimalPct` percent of its maximum (staff 2).
    pub sick: bool,
    pub dying: bool,
    pub egg: bool,
    /// Hungry at least as much as its species' threshold, and no food of its kind left in its
    /// exhibit (neighbours included for an amphibious species).
    pub hungry: bool,
    pub bare: bool,
    /// `cKeeperFoodType` and `cNeededFood`, and whether the species is amphibious.
    pub food: i32,
    pub needed: i32,
    pub amphibious: bool,
}

impl Seen {
    /// Hungry and unfed: what sends a keeper to the exhibit (staff 2).
    pub fn unfed(&self) -> bool {
        self.hungry && self.bare
    }
}

/// The values of the key in the first section (own subtype, default subtype, plain) that has it.
fn list(ty: &EntityType, key: &str) -> Vec<i32> {
    let Some(config) = ty.config.as_deref() else {
        return Vec::new();
    };
    let default = config
        .section("Global")
        .and_then(|g| g.get("DefaultSubtype"));
    let named = |p: &[u8]| match p.is_empty() {
        true => b"Characteristics/Integers".to_vec(),
        false => [p, b"/Characteristics/Integers"].concat(),
    };
    [&ty.subtype[..], default.unwrap_or_default(), b""]
        .into_iter()
        .filter_map(|p| config.section(named(p)))
        .map(|s| s.get_all(key))
        .find(|v| !v.is_empty())
        .map_or_else(Vec::new, |v| {
            v.iter().filter_map(|x| crate::config::to_int(x)).collect()
        })
}

/// The object types the keeper rakes (`cDirt`, staff 3).
pub fn dirt_ids(who: &EntityType) -> Vec<i32> {
    list(who, "cDirt")
}

/// `x`, `y` of a tile.
fn xy(world: &World, t: usize) -> (i64, i64) {
    let w = world.zoo.width as usize;
    ((t % w) as i64, (t / w) as i64)
}

/// The squared distance in tiles.
pub fn distance(world: &World, a: usize, b: usize) -> i64 {
    let (a, b) = (xy(world, a), xy(world, b));
    (a.0 - b.0).pow(2) + (a.1 - b.1).pow(2)
}

/// The animals of exhibit `x` that are of the keeper's kind (`cKeeper`, default 9500).
pub fn animals(world: &World, cat: &Catalogue, x: u32, who: &EntityType) -> Vec<Seen> {
    seen(world, cat, x, who, animals_in(world, x))
}

/// The same animals in the order of the exhibit's stored animal list (habitat rating 5.1); the
/// caller recomputes a dirty exhibit first.
pub fn listed(world: &World, cat: &Catalogue, x: u32, who: &EntityType) -> Vec<Seen> {
    seen(world, cat, x, who, crate::animal::exhibit_animals(world, x))
}

fn seen(world: &World, cat: &Catalogue, x: u32, who: &EntityType, ats: Vec<usize>) -> Vec<Seen> {
    let percent = number(who, "cSicklyAnimalPct").map_or(10, |p| p as i64);
    let percent = percent.clamp(0, 100);
    let mut found = Vec::new();
    for at in ats {
        let e = &world.zoo.entities[at];
        let (Some(ty), Some(c)) = (type_of(cat, e), e.common.as_ref()) else {
            continue;
        };
        let kind = number(ty, "cKeeper").map_or(9500, |v| v as i32);
        let (Some(Part::Animal(a)), Some(tile)) = (
            world.units.get(c.id).map(|u| &u.part),
            world.tile_index(c.x.div_euclid(64), c.y.div_euclid(64)),
        ) else {
            continue;
        };
        if kind != who.name_id {
            continue;
        }
        let k = animal::keys(world, cat, at);
        let regions = food::near(world, x, &k);
        let left: i32 = (food::pieces(world, cat, &regions, k.food_kind).iter())
            .map(|p| p.left)
            .fold(0, i32::saturating_add);
        found.push(Seen {
            id: c.id,
            tile,
            sick: i64::from(a.health) * 100 <= i64::from(k.max_hits) * (100 - percent),
            dying: a.dying,
            egg: a.egg,
            hungry: a.hunger >= k.hunger_threshold,
            bare: left < 1,
            food: k.food_kind,
            needed: number(ty, "cNeededFood").map_or(100, |v| v as i32),
            amphibious: k.amphibious,
        });
    }
    found
}

/// Whether the centre slot of tile `t` holds the keeper's dung (staff 2).
pub fn is_dung(world: &World, cat: &Catalogue, who: &EntityType, t: usize) -> bool {
    let entity = world.quadrant(t, 3).map(|e| &world.zoo.entities[e]);
    entity.is_some_and(|e| {
        e.class == "objects"
            && type_of(cat, e).is_some_and(|ty| dirt_ids(who).contains(&ty.name_id))
    })
}

/// The tiles of exhibit `x` whose centre slot holds the keeper's dung.
pub fn dung(world: &World, cat: &Catalogue, x: u32, who: &EntityType) -> Vec<usize> {
    let tiles = world.tiles_of(x).into_iter();
    tiles.filter(|&t| is_dung(world, cat, who, t)).collect()
}

/// The units of the food of the kind `food` that the animals of `x` still need (staff 7.6).
pub fn food_needed(world: &World, cat: &Catalogue, x: u32, who: &EntityType, food: i32) -> i32 {
    let want: i32 = (animals(world, cat, x, who).iter())
        .filter(|a| a.food == food)
        .map(|a| a.needed)
        .fold(0, i32::saturating_add);
    let held = food::pieces(world, cat, &[x], food)
        .into_iter()
        .map(|p| p.left);
    want.saturating_sub(held.fold(0, i32::saturating_add))
}

/// The keepers at work (task 0x6e) in exhibit `x`.
pub fn present(world: &World, x: u32) -> usize {
    let at = |u: &&crate::unit::Unit| {
        let c = (world.find(u.id)).and_then(|e| world.zoo.entities[e].common.as_ref());
        let tile = c.and_then(|c| world.tile_index(c.x.div_euclid(64), c.y.div_euclid(64)));
        tile.and_then(|t| world.exhibits().region(t)) == Some(x)
    };
    let working = |u: &&crate::unit::Unit| u.kind == Kind::Keeper && u.goal.record.task == DUTIES;
    world.units.iter().filter(working).filter(at).count()
}

/// The own target tiles of the keepers (staff 4.3: the claimed tiles), on their way or at work.
pub fn claimed(world: &World) -> Vec<usize> {
    let own = |u: &crate::unit::Unit| match &u.part {
        Part::Staff(s) if u.kind == Kind::Keeper => s.target,
        _ => None,
    };
    world.units.iter().filter_map(own).collect()
}

/// Whether exhibit `x` needs the keeper (7.2), now being the AI clock: a tank a marine keeper and
/// a land exhibit another one (point 1); in a tank the amphibious animals are the land keepers'
/// (point 2, tank staff 5.2), no animal of a show tank is hungry for him, and water below
/// `murkyWaterPurity` calls the marine keeper (point 4). The amphibious neighbours and the nested
/// tanks wait for their issues.
pub fn needs(world: &World, cat: &Catalogue, x: u32, who: &EntityType) -> bool {
    let Some(exhibit) = world.exhibits().get(x) else {
        return false;
    };
    let marine = number(who, "cUnderwater") == Some(1.0);
    if exhibit.tank.is_some() != marine {
        return false;
    }
    let (tank, show) = (exhibit.tank.is_some(), exhibit.tank.is_some_and(|t| t.show));
    let mut mine = animals(world, cat, x, who);
    mine.retain(|a| !(tank && a.amphibious));
    let hungry = |a: &Seen| !a.egg && !show && a.unfed();
    if mine.iter().any(|a| a.sick || hungry(a)) || !dung(world, cat, x, who).is_empty() {
        return true;
    }
    let murky = world.tank_config().murky;
    if exhibit
        .tank
        .is_some_and(|t| t.level > 0 && i64::from(t.purity) < i64::from(murky))
    {
        return true;
    }
    let (now, serviced) = (world.units.ai_ticks, exhibit.rating.serviced as u32);
    let due = serviced == 0 || now.wrapping_sub(serviced) > world.units.service_interval;
    !mine.is_empty() && due
}

/// The exhibit the keeper `me` serves next (7.3), `here` being the exhibit he stands in. His
/// assigned exhibits, else every exhibit; the zoo grounds never (open point 4); the stable sort
/// keeps the list order (open point 5).
pub fn choose(
    world: &World,
    cat: &Catalogue,
    who: &EntityType,
    assigned: &[u32],
    here: Option<u32>,
    lost: &[usize],
) -> Option<u32> {
    let all: Vec<u32> = world.exhibits().list().iter().map(|e| e.id).collect();
    let mut list: Vec<u32> = if assigned.is_empty() {
        all
    } else {
        assigned.to_vec()
    };
    list.retain(|&x| x != GROUNDS && world.exhibits().get(x).is_some());
    let claimed = claimed(world);
    let ticks = |x: u32| {
        let serviced = world
            .exhibits()
            .get(x)
            .map_or(0, |e| e.rating.serviced as u32);
        world.units.ai_ticks.wrapping_sub(serviced)
    };
    let key = |x: u32| {
        let mine = animals(world, cat, x, who);
        let dung = dung(world, cat, x, who).into_iter();
        let free = dung.filter(|t| !claimed.contains(t) && !lost.contains(t));
        let count = |f: &dyn Fn(&Seen) -> bool| mine.iter().filter(|a| f(a)).count();
        (
            count(&|a| a.dying),
            count(&|a| !a.egg && a.unfed()),
            free.count(),
            ticks(x),
        )
    };
    let mut keyed: Vec<_> = list.into_iter().map(|x| (Reverse(key(x)), x)).collect();
    keyed.sort_by_key(|k| k.0);
    let open = |x: u32| Some(x) != here && needs(world, cat, x, who);
    let coming = |x: u32| world.exhibits().get(x).map_or(0, |e| e.rating.coming) as usize;
    let first = keyed
        .iter()
        .map(|k| k.1)
        .find(|&x| open(x) && present(world, x) == 0 && coming(x) == 0);
    first.or_else(|| {
        (keyed.iter().map(|k| k.1))
            .find(|&x| open(x) && present(world, x) + coming(x) < 2 && ticks(x) > LONG)
    })
}

/// The tiles inside and outside the gate of exhibit `x`.
pub fn gate_tiles(world: &World, x: u32) -> Option<(usize, usize)> {
    let (tile, side) = world.gate_slot(x)?;
    let across = world.neighbour(tile, side)?;
    Some(match world.exhibits().region(tile) == Some(x) {
        true => (tile, across),
        false => (across, tile),
    })
}

/// The entrance test of a failed way to an exhibit with the gate tiles `(inside, outside)` (staff
/// 7.4): a fence piece on both sides of the gate edge, or the larger crossing cost of the keeper
/// 19 or more, or an object in the centre slot of either tile.
pub fn entrance_blocked(
    world: &World,
    keeper: &Mover,
    (inside, outside): (usize, usize),
    tank: bool,
) -> bool {
    let d = (0..8).find(|&d| world.neighbour(inside, d) == Some(outside));
    let fenced = d.is_some_and(|d| {
        world.edge(inside, d).is_some() && world.edge(outside, (d + 4) % 8).is_some()
    });
    let cost = |a, b| match (0..8).find(|&d| world.neighbour(a, d) == Some(b)) {
        Some(d) => path::step(world, keeper, a, b, d, None),
        None => path::MAX,
    };
    // For a tank it is the terrain costs of the two tiles (tank staff 5).
    let crossing = match tank {
        true => path::terrain(world, keeper, inside, None)
            .max(path::terrain(world, keeper, outside, None)),
        false => cost(inside, outside).max(cost(outside, inside)),
    };
    fenced
        || crossing >= 19
        || [inside, outside]
            .iter()
            .any(|&t| world.quadrant(t, 3).is_some())
}
