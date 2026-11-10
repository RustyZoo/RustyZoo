//! Picking a unit up and putting it down (unit placing 8.2 to 8.6).

use super::*;

/// A unit the player holds (unit placing 8.2): which one, its type, and where it stood, which way
/// it faced and how the view was turned, for the facing it keeps and for cancelling.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Carry {
    pub id: u32,
    /// The type, a position in the catalogue.
    pub ty: usize,
    pub from: (i32, i32, i32),
    /// The last point the pending thing had (unit placing 9.1.2 point 7): the pick-up point until
    /// the pointer gives one on the map. A save writes it.
    pub at: (i32, i32, i32),
    pub facing: u32,
    pub view: u8,
}

/// Picks the unit `id` up (unit placing 8.2): it is taken off the map, stops everything (its move,
/// its behaviour set and its animation, the goals being the goal issues') and is marked carried,
/// so its update is suspended; the exhibit it left is marked for a new rating. An escaped animal
/// is crated (9.1). `None` when `id` is
/// no unit, is carried already, or its type is not moveable (`cMoveable` 0).
pub(crate) fn pick_up(world: &mut World, cat: &Catalogue, id: u32, view: u8) -> Option<Carry> {
    let at = world.find(id)?;
    let e = &world.zoo.entities[at];
    e.common.as_ref()?;
    let ty = cat.position(&e.class, e.type_name.as_bytes(), e.subtype.as_bytes())?;
    if number(&cat.types[ty], "cMoveable") == Some(0.0) || world.units.get(id)?.carried {
        return None;
    }
    // A guest in the middle of a building visit leaves the building first (guest buildings 11.2).
    crate::building::leave::pick_up(world, cat, id);
    let e = &world.zoo.entities[at];
    let c = e.common.as_ref()?;
    let carry = Carry {
        id,
        ty,
        from: (c.x, c.y, c.z),
        at: (c.x, c.y, c.z),
        facing: c.rotation,
        view,
    };
    let tile = world.tile_index(c.x.div_euclid(64), c.y.div_euclid(64));
    world.show_moved(at, false);
    world.lift_unit(at);
    if let Some(t) = tile {
        world.mark_rating(t);
    }
    let c = world.zoo.entities[at].common.as_mut()?;
    c.movement_mode = 0;
    // The state clear writes "no slot" in the unit block of every kind (goals 11.3.1 point 2); a
    // guest's and an animal's live slot is written over it at save, a staff member's is this byte.
    if let Some(u) = c.unit.as_mut() {
        u.slot = u8::MAX;
    }
    // A keeper told the exhibit he is no longer coming: he closes the gate he remembers.
    crate::staff::leaves(world, id);
    let unit = world.units.get_mut(id)?;
    unit.tank = 0;
    unit.carried = true;
    // The pick-up of the animal itself lowers the hold of an aborted heal (staff 7.7).
    unit.held = false;
    match &mut unit.part {
        // An escaped animal is crated (8.2 point 4); any other one's escape walk ends.
        crate::unit::Part::Animal(a) => {
            // The state clear of a pick-up writes "no need" (shows 32.1 point 1) and "no slot"
            // (goals 11.3.1 point 2).
            (a.need, a.slot) = (crate::guest::NO_NEED, crate::guest::NO_SLOT);
            match a.escaped {
                true => crate::animal::boxed::crate_up(a),
                false => a.walk = None,
            }
        }
        // A guest forgets the building it meant to return to (unit placing 8.2 point 5) and its
        // slot (goals 11.3.1 point 2).
        crate::unit::Part::Guest(g) => {
            g.returning = None;
            g.slot = crate::guest::NO_SLOT;
        }
        _ => {}
    }
    if unit.kind == crate::unit::Kind::Animal {
        // The state clear writes the own target tile and its position directly (shows 32.2 point 4).
        unit.clear_own();
    }
    unit.moving = false;
    unit.destination = None;
    unit.path.clear();
    unit.travel = -1;
    (unit.through_building, unit.aborted) = (false, false);
    unit.runner = Runner::default();
    // An egg and a crate keep their base, the others get the plain idle (behaviours 5.10.1 point 4;
    // a crated escapee shows the crate its crating stop installs).
    let base = match &unit.part {
        Part::Animal(a) if a.egg => None,
        Part::Animal(a) if a.crated => Some("box_idle"),
        _ => Some("idle"),
    };
    if let Some(base) = base {
        unit.animation = Animation::default();
        unit.animation.base_to(base);
        unit.animation.cut();
    }
    unit.events.clear();
    Some(carry)
}

/// Puts the carried unit `id` down at `point` facing `facing` (unit placing 8.4 and 8.6; the
/// caller has judged the place): it is added to the map again, takes the tank mode and the terrain
/// mode of its tile (tank fauna 4, 5), is no longer carried, and the exhibit of the tile is marked for a new rating. Its home is the
/// exhibit it stands in; the goals then start anew at its next update. `false` when the unit is
/// gone or not carried.
pub(crate) fn put_down(
    world: &mut World,
    id: u32,
    point: (i32, i32, i32),
    facing: u32,
    data: &dyn Data,
) -> bool {
    let (Some(at), true) = (
        world.find(id),
        world.units.get(id).is_some_and(|u| u.carried),
    ) else {
        return false;
    };
    let Some(tile) = world.tile_index(point.0.div_euclid(64), point.1.div_euclid(64)) else {
        return false;
    };
    world.place_unit(at, point, facing);
    world.show_moved(at, true);
    if let Some(unit) = world.units.get_mut(id) {
        unit.carried = false;
        // A guest put down has lost its wish to leave (guests 8) and its return building
        // (unit placing 8.5, pathfinding 6.5).
        if let crate::unit::Part::Guest(g) = &mut unit.part {
            g.leave = false;
            g.returning = None;
            g.return_set = None;
        }
    }
    // The member lists of the return buildings let it go (guest buildings 12.2 point 3).
    crate::building::ret::forget(world, id);
    crate::unit::swim::put(world, id, data);
    world.mark_rating(tile);
    true
}
