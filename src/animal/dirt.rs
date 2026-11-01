//! The dung an animal puts down, its death and the keeper's heal (animals-needs 8.1, 9.7, 10).
//!
//! The dung meter is tested by the hunger check ([`super::food::hunger`]); this puts the pile
//! down. A pile is a scenery object of the type `objects/other/poo` (`dinopoo` for a dinosaur)
//! whose food counter holds its size: 0 normal, 1 small, 2 giant. A pile on the zoo grounds calls a
//! maintenance worker (staff 8.2); the keepers rake the piles of an exhibit.

use super::{DIED, Keys};
use crate::behaviour::Data;
use crate::build::number;
use crate::entity::Catalogue;
use crate::place::bulldozer::type_of;
use crate::sim::Rng;

/// The event a pile of dung on the zoo grounds posts for the maintenance task pool (the same
/// number as the one an exhibit posts for what an emptied exhibit leaves).
pub const DUNG: u16 = 10032;
use crate::entity::EntityType;
use crate::render::order::Kind;
use crate::unit::{Ctx, Part, Unit};
use crate::world::{Shape, World};
use crate::zoo::{Common, Entity};

/// Puts a scenery object of type `ty`, named `(class, type, subtype)`, in the middle of tile
/// `(x, y)`, facing `rotation`, with `food` in its counter; whether it was added.
pub fn put(
    world: &mut World,
    ty: &EntityType,
    names: (&str, &str, &str),
    (x, y): (i32, i32),
    rotation: u32,
    food: i32,
) -> bool {
    let at = (
        x.saturating_mul(64).saturating_add(32),
        y.saturating_mul(64).saturating_add(32),
    );
    put_at(world, ty, names, at, rotation, food)
}

/// The same at the point `(x, y)` in world units (64 to a tile).
pub fn put_at(
    world: &mut World,
    ty: &EntityType,
    names: (&str, &str, &str),
    at: (i32, i32),
    rotation: u32,
    food: i32,
) -> bool {
    put_z(world, ty, names, (at.0, at.1, None), rotation, food)
}

/// The same at the height `z` when it is given (food in the water), else on the ground.
pub fn put_z(
    world: &mut World,
    ty: &EntityType,
    names: (&str, &str, &str),
    (x, y, z): (i32, i32, Option<i32>),
    rotation: u32,
    food: i32,
) -> bool {
    let Some(tile) = world.tile_index(x.div_euclid(64), y.div_euclid(64)) else {
        return false;
    };
    let common = Common {
        x,
        y,
        z: z.unwrap_or(world.zoo.tiles[tile].elevation * 16),
        rotation,
        id: 0,
        name: String::new(),
        colors: Vec::new(),
        fence: None,
        food,
        ..Default::default()
    };
    let entity = Entity {
        class: names.0.into(),
        type_name: names.1.into(),
        subtype: names.2.into(),
        body_size: None,
        common: Some(common),
    };
    let shape = Shape {
        kind: Kind::Fixed,
        footprint: (ty.footprint[0], ty.footprint[1]),
        fence: false,
        wall: false,
        gate: false,
        gated: false,
        tank: crate::render::tank_kind(ty),
        walkable: (false, false),
    };
    world.add(entity, Some(shape)).is_some()
}

/// `putDownDirt` (8.1) for the animal at position `at`: in a tank the water loses purity; on land
/// a pile of its size goes in the middle of the tile, if the quarter there is free; in the water
/// nothing happens. When it is done the dung meter restarts at 0.
pub fn put_down(unit: &mut Unit, world: &mut World, ctx: &mut Ctx, at: usize, k: &Keys) {
    let Some(c) = world.zoo.entities[at].common.as_ref() else {
        return;
    };
    let (x, y) = (c.x.div_euclid(64), c.y.div_euclid(64));
    let Some(tile) = world.tile_index(x, y) else {
        return;
    };
    let done = if let Some(tank) = world.tank_at(tile) {
        world.pollute(tank, k.poo_impact.max(0) as u32);
        true
    } else if !world.is_water(tile) {
        let name = if k.dino_dung == 1 { "dinopoo" } else { "poo" };
        let size = if k.giant_dung == 1 {
            2
        } else {
            i32::from(k.small_dung == 1)
        };
        let ty = ctx.cat.find("objects", b"other", name.as_bytes());
        // The facing is drawn before the pile is added.
        let facing = ctx.rng.rand() & 7;
        let free = world.quadrant(tile, 3).is_none();
        let made = ty
            .filter(|_| free)
            .is_some_and(|ty| put(world, ty, ("objects", "other", name), (x, y), facing, size));
        world.mark_rating(tile);
        // A pile on the zoo grounds calls a maintenance worker (8.1 point 3).
        if made && world.exhibits().region(tile) == Some(crate::exhibit::GROUNDS) {
            let event = crate::world::piece_event(unit.id, DUNG, tile as u32);
            world.units.posts.push(event);
        }
        made
    } else {
        false
    };
    if let (true, Part::Animal(a)) = (done, &mut unit.part) {
        a.dung = 0;
        world.units.tell(format!("dung {} {x},{y}", unit.id));
    }
}

/// Every animal of exhibit `x` mourns at its next tick (animals-needs 10.4): the exhibit's stored
/// list, recomputed first when it is dirty.
pub fn mourn(world: &mut World, ctx: &mut Ctx, x: u32) {
    crate::exhibit::rating::refresh(world, ctx.cat, x, &mut || ctx.rng.rand());
    let ids: Vec<u32> = (crate::animal::exhibit_animals(world, x).into_iter())
        .filter_map(|at| world.zoo.entities[at].common.as_ref().map(|c| c.id))
        .collect();
    for id in ids {
        if let Some(Part::Animal(a)) = world.units.get_mut(id).map(|u| &mut u.part) {
            a.mourning = true;
        }
    }
}

/// `fDie` (10.2): the animal is removed, "has died of old age" is posted and its exhibit mourns.
pub fn die(unit: &Unit, world: &mut World, ctx: &mut Ctx, at: usize) {
    ctx.remove(world, unit.id);
    world.units.post(DIED, unit.id);
    world.units.tell(format!("died {}", unit.id));
    let tile = world.zoo.entities[at]
        .common
        .as_ref()
        .and_then(|c| world.tile_index(c.x.div_euclid(64), c.y.div_euclid(64)));
    if let Some(x) = tile.and_then(|t| super::food::exhibit_of(world, t)) {
        mourn(world, ctx, x);
    }
}

impl super::Animal {
    /// A keeper's heal (9.7): full health, no longer sick.
    pub fn heal(&mut self, k: &Keys) {
        (self.health, self.at_minimum_health) = (k.max_hits, false);
        self.unmark(super::SICK);
    }
}

/// The rename cheat of a heal (staff 7.7 point 2), with the flag `heal_changes_species` up: one
/// draw, `% 10`, at every heal; on 0 a second draw picks, among the egg-laying animal types of the
/// same subtype and another name id (catalogue order), the type the animal at `id` turns into in
/// place. It gets a fresh default name, an idle picture and the type's values; everything else
/// stays. An empty list changes nothing and draws nothing more.
pub fn change_species(world: &mut World, cat: &Catalogue, rng: &mut Rng, data: &dyn Data, id: u32) {
    if !world.units.cheat.heal_changes_species || rng.pick(10) != 0 {
        return;
    }
    let Some(at) = world.find(id) else {
        return;
    };
    let Some(old) = type_of(cat, &world.zoo.entities[at]) else {
        return;
    };
    let same = |t: &&EntityType| {
        t.class == "animals"
            && number(t, "cLaysEggs") == Some(1.0)
            && t.subtype == old.subtype
            && t.name_id != old.name_id
    };
    let list: Vec<&EntityType> = cat.types.iter().filter(same).collect();
    let Some(&new) = list.get(rng.pick(list.len() as u32) as usize) else {
        return;
    };
    let name = crate::place::unit::named(world, new);
    let latin1 = |b: &[u8]| b.iter().map(|&c| char::from(c)).collect::<String>();
    world.zoo.entities[at].type_name = latin1(&new.type_name);
    let subtype = latin1(&new.subtype);
    world.retype(at, &subtype, super::breed::shape(new));
    if let Some(c) = world.zoo.entities[at].common.as_mut() {
        c.name = name;
    }
    let idle = data.timing(&world.zoo.entities[at], "idle").is_some();
    if let Some(u) = world.units.get_mut(id) {
        u.typed = false;
        if idle {
            u.animation.base_to("idle");
        }
    }
    world
        .units
        .tell(format!("species {id} {}", latin1(&new.type_name)));
}
