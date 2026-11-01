//! The units a building makes when it is placed (unit placing 7.4): a slot with a `birthUser`
//! gets a new unit of that type, which waits there. Only the DRT base has one in the shipped data,
//! its helicopter.

use super::{Kind, facing, place};
use crate::entity::Catalogue;
use crate::goal::USE;
use crate::place::bulldozer::type_of;
use crate::sim::Rng;
use crate::world::{Shape, World};
use crate::zoo::{Common, Entity, fresh};

/// Makes the units of the slots of building `base`, just placed, in slot order, and returns their
/// ids. Each is a helicopter of the type with the slot's name id (the first match), with its two
/// draws: at the slot's position and facing, landed, waiting at the base (task 0x6a), a user of the
/// slot and a member of the base. A name id no type has, or a type that is no helicopter, makes
/// nothing. `built` is the base's construction date (a file time) for its record; a unit made
/// leaves the undo list empty.
pub fn births(
    world: &mut World,
    cat: &Catalogue,
    base: u32,
    rng: &mut Rng,
    built: u64,
    shape: impl Fn(&Entity) -> Option<Shape>,
) -> Vec<u32> {
    let Some(at) = world.find(base) else {
        return Vec::new();
    };
    let Some(ty) = type_of(cat, &world.zoo.entities[at]) else {
        return Vec::new();
    };
    let kind = Kind::of(ty, world.units.prices.default);
    let mut made = Vec::new();
    for (s, slot) in kind.slots.iter().enumerate().filter(|(_, s)| s.birth != 0) {
        let Some(t) = cat.types.iter().find(|t| t.name_id == slot.birth) else {
            continue;
        };
        if t.class != "helicopter" {
            continue;
        }
        let record = crate::staff::helicopter(t, base, rng);
        let e = &world.zoo.entities[at];
        let Some(b) = e.common.as_ref() else {
            return made;
        };
        let (x, y) = place(e, slot.at);
        // An odd rotation ignores the offset, the height too (guest buildings 3.2).
        let z = b.z + if b.rotation % 2 == 0 { slot.height } else { 0 };
        let mut unit = fresh::unit_new(None);
        (unit.slot, unit.target.goal) = (s as u8, i32::from(USE));
        unit.target.extra[0] = world.units.ai_ticks;
        let mut heli = Entity {
            class: t.class.into(),
            type_name: String::from_utf8_lossy(&t.type_name).into(),
            subtype: String::from_utf8_lossy(&t.subtype).into(),
            body_size: None,
            common: Some(Common {
                x,
                y,
                z,
                rotation: facing(e, slot.facing),
                movement_mode: 1,
                staff: Some(Box::new(record)),
                unit: Some(Box::new(unit)),
                ..Default::default()
            }),
        };
        let name = crate::place::unit::named(world, t);
        if let Some(c) = heli.common.as_mut() {
            c.name = name;
        }
        let shape = shape(&heli);
        let Some(id) = world.add(heli, shape) else {
            continue;
        };
        let price = kind.price;
        let c = world.zoo.entities[at].common.as_mut();
        if let Some(r) = c.map(|c| {
            c.building
                .get_or_insert_with(|| fresh::building_new(price, built))
        }) {
            r.users.push((id, s as u32));
            r.users_now += 1;
            r.members.push(id);
        }
        made.push(id);
    }
    if !made.is_empty() {
        world.clear_undo();
    }
    made
}
