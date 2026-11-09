//! The helicopter of the DRT base (staff 7.11.1): its own words and its rotor sound, the base it
//! lifts from (the slot point, the exit point, its users and its dart count), its idle sets and
//! the flight, one step of the free move kept in the height band of its type.

use crate::behaviour::{Data, Env, Number};
use crate::building::{self, place};
use super::rotor::Rotor;
use crate::entity::Catalogue;
use crate::render::order::ground_height;
use crate::unit::{Part, Unit};
use crate::world::World;
use crate::zoo::Entity;

/// The three own words of a helicopter (zoo format 6.13), the band it flies in and its sound.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Heli {
    /// The entity id of its base.
    pub base: u32,
    pub flying: bool,
    /// The nearest direction towards the destination, -1 for none.
    pub dir: i32,
    /// `cMinHeight` and `cMaxHeight` of the type.
    pub band: (i32, i32),
    /// Not saved: a flying helicopter loaded starts it again at full volume (staff 7.11.2).
    pub rotor: Option<Rotor>,
}

impl Default for Heli {
    fn default() -> Self {
        Heli {
            base: 0,
            flying: false,
            dir: -1,
            band: (0, 0),
            rotor: None,
        }
    }
}

impl Heli {
    /// The helicopter a record's own words stand for (zoo format 6.13); a short record is a
    /// helicopter with no base.
    pub(crate) fn from_own(own: &[u8]) -> Heli {
        let word = |i: usize| {
            own.get(4 * i..4 * i + 4)
                .and_then(|b| <[u8; 4]>::try_from(b).ok())
                .map_or(0, u32::from_le_bytes)
        };
        let flying = word(1) != 0;
        Heli {
            base: word(0),
            flying,
            dir: if own.len() < 12 { -1 } else { word(2) as i32 },
            // A flying helicopter restarts its sound at its first update.
            rotor: flying.then(Rotor::loaded_in_air),
            ..Heli::default()
        }
    }

    /// The own words as the record stores them.
    pub(crate) fn own(&self) -> Vec<u8> {
        [self.base, u32::from(self.flying), self.dir as u32]
            .into_iter()
            .flat_map(u32::to_le_bytes)
            .collect()
    }

    /// Point 8.2: setting the word to what it is does nothing. Landing raises the fade flag of the
    /// sound; taking off needs a new sound when there is none (the answer is true; staff 7.11.2
    /// point 3), and does nothing to one that exists.
    pub(crate) fn set_flying(&mut self, on: bool) -> bool {
        if self.flying == on {
            return false;
        }
        self.flying = on;
        match (&mut self.rotor, on) {
            (Some(r), false) => r.raise(),
            (None, true) => return true,
            _ => {}
        }
        false
    }

    /// The fade term of the sound (0 is full volume), when there is one.
    pub(crate) fn level(&self) -> Option<i32> {
        self.rotor.as_ref().map(Rotor::level)
    }
}

/// The helicopter's destination setter (point 8.1): the point, and the flight direction towards
/// it, -1 when it is where it stands.
pub(crate) fn set_destination(unit: &mut Unit, world: &World, at: usize, to: (i32, i32, i32)) {
    unit.destination = Some(to);
    let from = world.zoo.entities.get(at).and_then(|e| e.common.as_ref());
    let dir = from.and_then(|c| {
        crate::animal::food::nearest_direction(to.0.wrapping_sub(c.x), to.1.wrapping_sub(c.y))
    });
    if let Part::Staff(s) = &mut unit.part {
        s.heli.dir = dir.map_or(-1, |d| d as i32);
    }
}

/// The first turn: the band of the type's heights.
pub(crate) fn read_type(unit: &mut Unit, data: &dyn Data, entity: &Entity) {
    let n = |key: &str| data.number(entity, &Number::Characteristic(key.into()));
    if let Part::Staff(s) = &mut unit.part {
        s.heli.band = (n("cMinHeight"), n("cMaxHeight"));
    }
}

/// Point 8.4: with no set running and none pending, a helicopter idles on the ground or in the
/// air; the set is interruptible and not forced.
pub(crate) fn idle_set(unit: &mut Unit, env: &Env) {
    let Part::Staff(s) = &unit.part else { return };
    let name = if s.heli.flying {
        "bIdleAir"
    } else {
        "bIdleGround"
    };
    if !unit.runner.running() && unit.runner.queued().is_none() {
        unit.runner
            .start(&mut unit.animation, env, name, true, false, true);
    }
}

/// The helicopter's position and the sound it has, for the sound rules.
fn sounding(unit: &Unit, world: &World, at: usize) -> Option<((i32, i32, i32), bool)> {
    let c = world.zoo.entities[at].common.as_ref()?;
    let Part::Staff(s) = &unit.part else {
        return None;
    };
    Some(((c.x, c.y, c.z), s.heli.flying))
}

/// Point 7 of staff 7.11.2: a helicopter loaded in the air starts its sound before anything else
/// happens to it.
pub(crate) fn settle(unit: &mut Unit, world: &World, data: &dyn Data, at: usize) {
    let Some((point, _)) = sounding(unit, world, at) else {
        return;
    };
    if let Part::Staff(s) = &mut unit.part
        && s.heli.rotor.as_ref().is_some_and(Rotor::pending)
    {
        let loaded = data.loop_sound(&world.zoo.entities[at]);
        let mut r = Rotor::start(loaded, point, &world.units.hearing);
        r.settle();
        s.heli.rotor = Some(r);
    }
}

/// Setting the flying word (point 8.2): the sound starts when the helicopter has none (staff
/// 7.11.2 point 3).
pub(crate) fn set_flying(unit: &mut Unit, world: &World, data: &dyn Data, at: usize, on: bool) {
    settle(unit, world, data, at);
    let Some((point, _)) = sounding(unit, world, at) else {
        return;
    };
    if let Part::Staff(s) = &mut unit.part
        && s.heli.set_flying(on)
    {
        let loaded = data.loop_sound(&world.zoo.entities[at]);
        s.heli.rotor = Some(Rotor::start(loaded, point, &world.units.hearing));
    }
}

/// Staff 7.11.2 point 4: the sound after an update of `ms` milliseconds.
pub(crate) fn rotor(unit: &mut Unit, world: &World, data: &dyn Data, at: usize, ms: u32) {
    settle(unit, world, data, at);
    let Some((point, flying)) = sounding(unit, world, at) else {
        return;
    };
    if let Part::Staff(s) = &mut unit.part
        && let Some(r) = &mut s.heli.rotor
        && !r.update(flying, ms, point, &world.units.hearing)
    {
        s.heli.rotor = None;
    }
}

/// The base `id`, when it exists and is a building.
pub(crate) fn base_at(world: &World, id: u32) -> Option<usize> {
    let at = world.find(id)?;
    (world.zoo.entities[at].class == "building").then_some(at)
}

/// A point in world units, the height last.
pub(crate) type Point3 = (i32, i32, i32);

/// The slot point and the exit point of slot `slot` of the base at position `base` (guest buildings
/// 3.2): turned by the rotation, the heights ignored at an odd one.
pub(crate) fn points(
    world: &World,
    cat: &Catalogue,
    base: usize,
    slot: usize,
) -> Option<(Point3, Point3)> {
    let e = &world.zoo.entities[base];
    let slot = building::kind_of(cat, e, world.units.prices.default)?
        .slots
        .into_iter()
        .nth(slot)?;
    let c = e.common.as_ref()?;
    let lift = |up: i32| c.z + if c.rotation % 2 == 0 { up } else { 0 };
    let (sx, sy) = place(e, slot.at);
    let (ex, ey) = place(e, slot.exit.unwrap_or(slot.at));
    Some((
        (sx, sy, lift(slot.height)),
        (ex, ey, lift(slot.exit_height)),
    ))
}

/// `add_user` of slot 0 for the helicopter (guest buildings 6.4): refused when the slot is full,
/// else the users now and the list grow. Whether it was added.
pub(crate) fn add_user(world: &mut World, cat: &Catalogue, base: usize, unit: u32) -> bool {
    let e = &world.zoo.entities[base];
    let kind = building::kind_of(cat, e, world.units.prices.default);
    let room = kind
        .as_ref()
        .and_then(|k| k.slots.first())
        .map(|s| s.capacity);
    let Some(b) = world.zoo.entities[base]
        .common
        .as_mut()
        .and_then(|c| c.building.as_mut())
    else {
        return false;
    };
    let taken = b.users.iter().filter(|u| u.1 == 0).count();
    if room.is_none_or(|n| i32::try_from(taken).unwrap_or(i32::MAX) >= n) {
        return false;
    }
    b.users.push((unit, 0));
    b.users_now = b.users_now.saturating_add(1);
    true
}

/// `remove_user`, not forced: the helicopter leaves the list and the users now go down. It stays
/// where it is (the base has `cUserUsesExit` 0).
pub(crate) fn remove_user(world: &mut World, base: usize, unit: u32) {
    let b = world.zoo.entities[base]
        .common
        .as_mut()
        .and_then(|c| c.building.as_mut());
    if let Some(b) = b.filter(|b| b.users.iter().any(|u| u.0 == unit)) {
        b.users.retain(|u| u.0 != unit);
        b.users_now = b.users_now.saturating_sub(1);
    }
}

/// The dart the helicopter fired: the base's fill word, which the original calls its user-data
/// refresh after (nothing to see for this building).
pub(crate) fn add_dart(world: &mut World, base: usize) {
    let b = world.zoo.entities[base]
        .common
        .as_mut()
        .and_then(|c| c.building.as_mut());
    if let Some(b) = b {
        b.fill = b.fill.wrapping_add(1);
    }
}

/// The length of the vector, cut towards zero.
fn length(parts: &[i32]) -> i32 {
    let squares: i64 = parts.iter().map(|&v| i64::from(v).pow(2)).sum();
    (squares as f64).sqrt() as i32
}

/// One step of `d` units of the helicopter's flight (point 7): the next point and the facing it
/// is given, `rotation` when the flight direction is -1.
fn next_point(
    unit: &Unit,
    world: &World,
    d: i32,
    (p, rotation): ((i32, i32, i32), u32),
) -> ((i32, i32, i32), u32) {
    let (dir, band) = match &unit.part {
        Part::Staff(s) => (s.heli.dir, s.heli.band),
        _ => (-1, (0, 0)),
    };
    let facing = u32::try_from(dir).unwrap_or(rotation);
    let Some(to) = unit.destination else {
        return (p, facing);
    };
    let delta = [
        to.0.wrapping_sub(p.0),
        to.1.wrapping_sub(p.1),
        to.2.wrapping_sub(p.2),
    ];
    let vertical = matches!(unit.runner.top(), Some("bAscend" | "bDescend"));
    let go = |v: i32, len: i32| v.wrapping_mul(d) / len;
    if vertical {
        let len = length(&delta);
        if d >= len {
            return (to, facing);
        }
        let next = (
            p.0 + go(delta[0], len),
            p.1 + go(delta[1], len),
            p.2 + go(delta[2], len),
        );
        return (next, facing);
    }
    let len = length(&delta[..2]);
    let (x, y) = match d < len {
        true => (p.0 + go(delta[0], len), p.1 + go(delta[1], len)),
        false => (to.0, to.1),
    };
    // The height above the terrain, less the walls of a tank under it.
    let tile = world.tile_index(x.div_euclid(64), y.div_euclid(64));
    let ground = tile.map_or(0, |t| {
        ground_height(&world.zoo.tiles[t], x.rem_euclid(64), y.rem_euclid(64))
    });
    let wall = tile
        .and_then(|t| world.tank_at(t))
        .and_then(|x| world.tank(x))
        .map_or(0, |t| t.steps().0);
    let h = p.2.wrapping_sub(ground).wrapping_sub(16 * wall);
    let (min, max, half) = (band.0, band.1, d >> 1);
    let z = match () {
        _ if h < 0 => return ((p.0, p.1, p.2.wrapping_add(d)), facing),
        _ if min.wrapping_sub(h) >= 1 => p.2 + (min - h).min(half),
        _ if h.wrapping_sub(max) > 0 => p.2 - (h - max).min(half),
        _ => p.2,
    };
    ((x, y, z), facing)
}

/// The free move of a helicopter by the step (movement 11): true when it did not move and the
/// facing did not change.
pub(crate) fn fly(unit: &Unit, world: &mut World, distance: u32, at: usize) -> bool {
    let Some(c) = world.zoo.entities[at].common.as_ref() else {
        return true;
    };
    let from = ((c.x, c.y, c.z), c.rotation);
    let d = i32::try_from(distance).unwrap_or(i32::MAX);
    let (to, facing) = next_point(unit, world, d, from);
    world.place_unit(at, to, facing);
    (to, facing) == from
}

/// Point 8.5: the landed helicopter at position `at` goes to the slot point of its stored slot of
/// its base, facing as it did. A stored slot the base's type lacks (0xff of an old file) gives the
/// base's own position (staff 7.11.1 point 13.4).
pub(crate) fn land(world: &mut World, cat: &Catalogue, at: usize) {
    let Some(c) = world.zoo.entities[at].common.as_ref() else {
        return;
    };
    let slot = c.unit.as_ref().map_or(0, |u| usize::from(u.slot));
    let rotation = c.rotation;
    let base = match world.units.get(c.id).map(|u| &u.part) {
        Some(Part::Staff(s)) => base_at(world, s.heli.base),
        _ => None,
    };
    let Some(base) = base else {
        return;
    };
    let own = world.zoo.entities[base]
        .common
        .as_ref()
        .map(|b| (b.x, b.y, b.z));
    if let Some((x, y, z)) = points(world, cat, base, slot).map(|p| p.0).or(own) {
        world.place_unit(at, (x, y, z), rotation);
    }
}
