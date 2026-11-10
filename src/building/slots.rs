//! The place and the room of a building's slots: where a user leaves, and whether one more fits.

use super::{Kind, Live, Point, SHELTER_OF_ANY_EXHIBIT, Slot, Species, place, ret};
use crate::place::path_shape;
use crate::world::World;
use crate::zoo::Entity;

/// The place a user of slot `slot` of the building is put at when it leaves by force or by the
/// exit (guest buildings 6.3, 6.7): the slot's exit for a type whose users use it, else the first
/// entrance, whose place passes [`fits`]. `who` is the animal that leaves, which its own place
/// rules apply to too (6.3 point 5), its exhibit being the one of the tile it stands on; `None`
/// for a guest. `None` when there is none, and the slot cannot be used.
pub(crate) fn exit_of(
    world: &World,
    e: &Entity,
    kind: &Kind,
    slot: usize,
    who: Option<&Species>,
) -> Option<Point> {
    let s = kind.slots.get(slot)?;
    let home = who
        .and_then(|w| w.tile)
        .and_then(|(x, y)| world.tile_index(x, y))
        .and_then(|t| world.exhibits().region(t));
    let exempt = kind.name_id == SHELTER_OF_ANY_EXHIBIT;
    let animal = |t: usize| who.is_none_or(|w| animal_place(world, t, w, home, exempt));
    match kind.uses_exit {
        true => (s.exit.map(|p| place(e, p))).filter(|&p| fits(world, p, Some(s), &animal)),
        false => {
            (s.entrances.iter().map(|&p| place(e, p))).find(|&p| fits(world, p, None, &animal))
        }
    }
}

/// Guest buildings 6.3, the place test for a guest: the tile of `p` is on the map, in the zoo and
/// has a quarter free or held by a walkable object; an exit place (`exit` given) must also be on
/// the terrain the slot allows, water or land. `animal` is the animal's part of the test (point 5),
/// which passes a guest.
fn fits(world: &World, (x, y): Point, exit: Option<&Slot>, animal: &dyn Fn(usize) -> bool) -> bool {
    let Some(t) = world.tile_index(x.div_euclid(64), y.div_euclid(64)) else {
        return false;
    };
    let walkable = |e: usize| world.shape(e).is_some_and(|s| s.walkable.0);
    let open = (0..4).any(|q| world.quadrant(t, q).is_none_or(walkable));
    let terrain = exit.is_none_or(|s| match world.is_water(t) {
        true => s.exit_water,
        false => s.exit_land,
    });
    world.in_zoo(t) && open && terrain && animal(t)
}

/// Guest buildings 6.3 point 5, the place of an animal on tile `t`: in its own exhibit, which is
/// the exhibit of the tile it stands on (`home`) unless the type is `exempt`; not a large animal on
/// a tile that is not flat or raised along one side (the path shapes of placing 5.3); and not water
/// for a non-swimmer.
fn animal_place(world: &World, t: usize, who: &Species, home: Option<u32>, exempt: bool) -> bool {
    if !who.animal {
        return true;
    }
    let own = exempt || world.exhibits().region(t) == home;
    own && !(who.large && !path_shape(world.zoo.tiles[t].shape))
        && (who.swims || !world.is_water(t))
}

/// Whether the unit (`who`) may use slot `slot` of a building (guest buildings 6.1):
/// the building and the slot have room, the slot's species and filter let it in, and an exit exists
/// (6.3: an entrance, or the slot's exit for a type that uses it, that passes the place test).
pub(crate) fn can_add(
    world: &World,
    e: &Entity,
    kind: &Kind,
    live: Option<&Live>,
    slot: usize,
    who: Option<&Species>,
) -> bool {
    if live.is_some_and(|l| l.dead) {
        return false;
    }
    let users = live.map_or(0, |l| l.users.len() + l.extra as usize);
    let Some(s) = kind.slots.get(slot) else {
        return false;
    };
    let in_slot = live.map_or(0, |l| l.users.iter().filter(|u| u.1 == slot).count());
    let no_show = who.is_some_and(|w| w.grandstand) && live.is_none_or(|l| l.fill == 0);
    users < kind.capacity.max(0) as usize
        && in_slot < s.capacity.max(0) as usize
        && !no_show
        && s.allows(who)
        && ret::may_enter(world, e, kind, live, who)
        && exit_of(world, e, kind, slot, who).is_some()
}

/// Whether some slot of the building is open (guest buildings 6.1).
pub(crate) fn has_open_slot(
    world: &World,
    e: &Entity,
    kind: &Kind,
    live: Option<&Live>,
    who: Option<&Species>,
) -> bool {
    (0..kind.slots.len()).any(|s| can_add(world, e, kind, live, s, who))
}

/// The entrances of the open slots with their squared distance from `from`, the nearest first and
/// in slot order among equals (guest buildings 6.2).
pub(crate) fn entrances(
    world: &World,
    e: &Entity,
    kind: &Kind,
    live: Option<&Live>,
    from: Point,
    who: Option<&Species>,
) -> Vec<(i64, usize, Point)> {
    let mut found = Vec::new();
    for (i, s) in kind.slots.iter().enumerate() {
        if !can_add(world, e, kind, live, i, who) {
            continue;
        }
        for &rel in &s.entrances {
            let p = place(e, rel);
            let (dx, dy) = (i64::from(from.0 - p.0), i64::from(from.1 - p.1));
            found.push((dx * dx + dy * dy, i, p));
        }
    }
    found.sort_by_key(|f| f.0);
    found
}
