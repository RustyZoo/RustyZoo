//! The marine specialist's side of a show (shows 22): when a show needs him, his cues, his place
//! in the unit list after a load and his removal. His job (19) is in the keeper goals.

use super::{cycle, event, has_info, info_slot, units};
use crate::behaviour::Data;
use crate::entity::Catalogue;
use crate::goal::Cx;
use crate::place::bulldozer::type_of;
use crate::sim::{Event, Events};
use crate::unit::{Ctx, Kind, Unit};
use crate::world::World;

/// Whether exhibit `x` holds a show that needs a keeper of type `t` (shows 22.1 point 1): `t` is
/// the keeper type of its scheduled species, its unit list of `t` is empty and the show is ready.
/// The list is a lookup: it makes the record of `t` (shows 32.7 point 1.2).
pub(crate) fn needs_keeper(world: &mut World, cat: &Catalogue, x: u32, t: u32) -> bool {
    let date = crate::timing::file_time(world.units.now);
    let Some(n) = info_slot(world, x) else {
        return false;
    };
    let Some(i) = world.zoo.shows.infos[n]
        .as_mut()
        .filter(|i| cycle::keeper_type(cat, i.scheduled().unwrap_or(0)) == t)
    else {
        return false;
    };
    i.lookup(t, date).units.is_empty() && i.show.ready
}

/// The set the specialist starts for the events he heard (shows 22.2 points 2 to 4): only those
/// sent by his show count; the last cue of a kind wins; the post-trick set comes before the
/// pre-trick one.
fn cue_set(world: &World, show: u16, heard: &[Event]) -> Option<String> {
    let (mut pre, mut post) = (None, None);
    for e in heard.iter().filter(|e| e.sender == u32::from(show)) {
        match e.kind {
            event::NEXT_ITEM => pre = Some(e.extra),
            event::TRICK_DONE => post = Some(e.extra),
            // Posted on no keeper channel: it changes a post that is there, never adds one.
            event::ENDED => post = post.map(|_| e.extra),
            _ => {}
        }
    }
    if pre.is_none() && post.is_none() {
        return None;
    }
    let info = (world.zoo.shows.infos.iter().flatten()).find(|i| i.id == show)?;
    let script = (world.zoo.shows.scripts.iter()).find(|s| s.id == info.show.script)?;
    let name = |item: Option<u16>, k: usize| {
        let item = item.and_then(|n| script.items.get(usize::from(n)));
        item.map_or("", |i| i.names[k].as_str())
    };
    [(post, 3), (pre, 2)]
        .into_iter()
        .map(|(n, k)| name(n, k))
        .find(|s| !s.is_empty())
        .map(str::to_string)
}

/// The start of a keeper's update (shows 22.2 point 1): while his job is 19 he reads the cues of
/// his show and starts the set of the last one, as a behaviour trick does.
pub(crate) fn cue(unit: &mut Unit, world: &mut World, ctx: &mut Ctx, at: usize, data: &dyn Data) {
    if unit.kind != Kind::Keeper || unit.goal.record.job != super::CONDUCT {
        return;
    }
    let Some(set) = cue_set(world, unit.show, &unit.events) else {
        return;
    };
    let mut cx = Cx {
        unit,
        world,
        ctx,
        at,
        data,
    };
    cx.start_with(&set, false, true);
}

/// After a load (shows 22.1 point 7): a keeper whose job is 19 and who stands in an exhibit joins
/// its show again, the unit lists not being saved. The goals are loaded by then.
pub(crate) fn rejoin(world: &mut World, cat: &Catalogue) {
    for at in 0..world.zoo.entities.len() {
        let id = world.zoo.entities[at].common.as_ref().map(|c| c.id);
        let conducting = id
            .and_then(|id| world.units.get(id))
            .is_some_and(|u| u.kind == Kind::Keeper && u.goal.record.job == super::CONDUCT);
        let ty = type_of(cat, &world.zoo.entities[at]);
        if let (true, Some(id), Some(ty), Some(x)) = (conducting, id, ty, world.exhibit_of(at)) {
            units::join(world, cat, x, id, ty);
        }
    }
}

/// A keeper is removed from the map, not picked up (shows 22.1 point 8): when the show info of his
/// show id exists he leaves its list and the show is aborted. The events go to the world's posts.
pub(crate) fn removed(world: &mut World, cat: &Catalogue, id: u32) {
    let Some(show) = (world.units.get(id)).filter(|u| u.kind == Kind::Keeper && u.show != 0) else {
        return;
    };
    let show = show.show;
    let ty = world
        .find(id)
        .and_then(|at| type_of(cat, &world.zoo.entities[at]));
    if let (true, Some(ty)) = (has_info(world, show), ty) {
        units::leave(world, show, id, ty.name_id as u32);
        let mut posted = Events::default();
        cycle::abort(world, cat, &mut posted, show);
        world.units.posts.extend(posted.queue().second);
    }
}

#[cfg(test)]
mod tests;
