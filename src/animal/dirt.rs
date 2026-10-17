//! The dung an animal puts down, its death and the keeper's heal (animals-needs 8.1, 9.7, 10).
//!
//! The dung meter is tested by the hunger check ([`super::food::hunger`]); this puts the pile
//! down. A pile is a scenery object of the type `objects/other/poo` (`dinopoo` for a dinosaur)
//! whose food counter holds its size: 0 normal, 1 small, 2 giant. The keepers that rake it and the
//! workers that hear of it are the staff issues'.

use super::{DIED, Keys};
use crate::entity::EntityType;
use crate::render::order::Kind;
use crate::unit::{Ctx, Part, Unit};
use crate::world::{Shape, TankKind, World};
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
    let Some(tile) = world.tile_index(x, y) else {
        return false;
    };
    let common = Common {
        x: x * 64 + 32,
        y: y * 64 + 32,
        z: world.zoo.tiles[tile].elevation * 16,
        rotation,
        id: 0,
        name: String::new(),
        colors: Vec::new(),
        fence: None,
        movement_mode: 0,
        food,
        animal: None,
        guest: None,
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
        tank: TankKind::NONE,
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
        made
    } else {
        false
    };
    if let (true, Part::Animal(a)) = (done, &mut unit.part) {
        a.dung = 0;
        world.units.tell(format!("dung {} {x},{y}", unit.id));
    }
}

/// Every animal of exhibit `x` mourns at its next tick (animals-needs 10.4).
pub fn mourn(world: &mut World, x: u32) {
    let ids: Vec<u32> = (world.zoo.entities.iter())
        .filter(|e| e.class == "animals")
        .filter_map(|e| e.common.as_ref())
        .filter(|c| {
            let tile = world.tile_index(c.x.div_euclid(64), c.y.div_euclid(64));
            tile.and_then(|t| world.exhibits().region(t)) == Some(x)
        })
        .map(|c| c.id)
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
        mourn(world, x);
    }
}

impl super::Animal {
    /// A keeper's heal (9.7): full health, no longer sick.
    pub fn heal(&mut self, k: &Keys) {
        (self.health, self.at_minimum_health) = (k.max_hits, false);
        self.unmark(super::SICK);
    }
}
