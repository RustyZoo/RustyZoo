//! The show portals (shows 21.5, exhibits and money 3.11): the wall piece of a tank that lets the
//! animals through to a show neighbour, and the open byte that follows the animals crossing it.

use crate::behaviour::Heard;
use crate::config::to_int;
use crate::entity::Catalogue;
use crate::entity::type_of;
use crate::unit::Part;
use crate::world::World;

/// The portal piece of exhibit `from` towards show neighbour `to`: the first boundary pair, in the
/// order of the exhibit's boundary, whose wall piece on the side of `from` is a portal. Gives the
/// piece, its tile and the tile across the wall. An exhibit that is not a tank asks its amphibious
/// neighbours in order and takes the first answer (shows 34.3).
fn piece(world: &World, from: u32, to: u32) -> Option<(usize, usize, usize)> {
    if world.tank(from).is_none() {
        let near = world.exhibits().get(from)?.amphibious.clone();
        return near.into_iter().find_map(|a| piece(world, a, to));
    }
    for (a, b) in world.exhibits().boundary(world, from) {
        let Some(b) = b.filter(|&b| world.exhibits().region(b) == Some(to)) else {
            continue;
        };
        let side = (0..4)
            .map(|d| d * 2)
            .find(|&d| world.neighbour(a, d) == Some(b));
        let wall = side.and_then(|d| world.edge(a, d));
        let portal = |p: usize| {
            let e = &world.zoo.entities[p];
            e.class == "tankwall"
                && e.common
                    .as_ref()
                    .and_then(|c| c.fence)
                    .is_some_and(|f| f.portal)
        };
        if let Some(p) = wall.filter(|&p| portal(p)) {
            return Some((p, a, b));
        }
    }
    None
}

/// The tile of exhibit `from`'s portal towards `to` and the tile across it (shows 21.3).
pub(crate) fn portal(world: &World, from: u32, to: u32) -> Option<(usize, usize)> {
    piece(world, from, to).map(|(_, a, b)| (a, b))
}

/// Whether an animal of exhibit `from` is on its way through to exhibit `to` (task 133 or 134
/// with its own target tile in `to`).
fn crossing(world: &World, from: u32, to: u32) -> bool {
    let region = |t: Option<usize>| t.and_then(|t| crate::animal::food::exhibit_of(world, t));
    // The cheap tests first: only an animal on its way across looks for its place in the list.
    world.units.iter().any(|u| {
        let task = u.goal.record.task;
        matches!(u.part, Part::Animal(_))
            && (crate::goal::GO_TO_SHOW..=crate::goal::RETURN_FROM_SHOW).contains(&task)
            && region(u.own_tile) == Some(to)
            && world.find(u.id).and_then(|at| world.exhibit_of(at)) == Some(from)
    })
}

/// The end of the update of exhibit `s` (shows 21.5): a show tank opens the portals to its show
/// neighbours while an animal crosses and closes them after. The sound is made once per pair.
pub(crate) fn open_portals(world: &mut World, cat: &Catalogue, s: u32) {
    let Some(near) = (world.exhibits().get(s)).filter(|e| e.tank.is_some_and(|t| t.show)) else {
        return;
    };
    for n in near.show_near.clone() {
        let open = crossing(world, s, n) || crossing(world, n, s);
        let (mine, theirs) = (piece(world, s, n), piece(world, n, s));
        set_open(world, cat, mine.map(|p| p.0), open, true);
        set_open(world, cat, theirs.map(|p| p.0), open, mine.is_none());
    }
}

/// The step of the entity at `at` for the portals (shows 32.6 point 4): a portal wall's player
/// advances by `step` milliseconds.
pub(crate) fn advance(world: &mut World, data: &dyn crate::behaviour::Data, at: usize, step: u32) {
    if !world.portal_players_any() {
        return;
    }
    let timing = data.timing(&world.zoo.entities[at], "portal");
    world.portal_advance(at, timing, step);
}

/// `set_open` (shows 21.5 point 3): a wall whose open byte is not `open` takes it, is drawn again
/// and, with `sound`, makes the sound of its type.
pub(crate) fn set_open(
    world: &mut World,
    cat: &Catalogue,
    wall: Option<usize>,
    open: bool,
    sound: bool,
) {
    let Some(at) = wall.filter(|&at| world.set_portal(at, open)) else {
        return;
    };
    let id = world.zoo.entities[at].common.as_ref().map_or(0, |c| c.id);
    world.portal_play(id, open);
    let heard = sound.then(|| heard(world, cat, at, open)).flatten();
    world.units.push_sound(id, heard);
}

/// The sound the type of the wall at `at` makes when its portal opens or closes (`[PortalSounds]`),
/// with its attenuation.
fn heard(world: &World, cat: &Catalogue, at: usize, open: bool) -> Option<Heard> {
    let ty = type_of(cat, &world.zoo.entities[at])?;
    let section = ty.config.as_deref()?.section("PortalSounds")?;
    let (key, atten) = match open {
        true => ("cPortalOpenSound", "cPortalOpenSoundAtten"),
        false => ("cPortalCloseSound", "cPortalCloseSoundAtten"),
    };
    let file = section.get(key).filter(|v| !v.is_empty())?;
    Some(Heard {
        file: ty.sound_member(file),
        base: section.get(atten).and_then(to_int).unwrap_or(0),
        voice: format!(
            "{}/{}/{key}",
            ty.class,
            String::from_utf8_lossy(&ty.type_name)
        ),
    })
}
