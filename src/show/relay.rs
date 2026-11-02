//! How the events of a show reach the grandstands and the guests seated there (shows 6.2, 6.3):
//! the world manager's reading of the show events and the grandstand's reading of its own.

use super::{KEEPER, Sight, event, grandstands};
use crate::building::kind_of;
use crate::entity::Catalogue;
use crate::sim::{BUILDING_CHANNEL, Event, Events, Rng, TANK_CHANNEL, UNIT_CHANNEL, WORLD_CHANNEL};
use crate::world::World;

/// The steps before the cheer or the boo of a show's end is heard.
const LATER: u16 = 135;

/// The events the world manager relays to the tank and the grandstands (shows 6.2).
const RELAYED: [u16; 9] = [
    event::ANNOUNCE,
    event::START_NOTE,
    event::STARTED,
    event::ENDED,
    event::ABORTED,
    event::BAD,
    event::NEUTRAL,
    event::GOOD,
    event::GREAT,
];

/// The events a grandstand shows its audience: the end and the abort, then the reactions.
const ENDS: [u16; 3] = [event::ENDED, event::ABORTED, event::OVER];
const REACTIONS: [u16; 4] = [event::BAD, event::NEUTRAL, event::GOOD, event::GREAT];

/// An event of the world manager (sender 0), delivered after `delay` steps.
fn world_event(kind: u16, target: u32, channel: u8, data: u32, extra: u16, delay: u16) -> Event {
    Event {
        sender: 0,
        target,
        data,
        extra,
        kind,
        delay,
        sender_channel: WORLD_CHANNEL,
        channel,
    }
}

/// Whether show `id` has a marine specialist.
fn has_keeper(world: &World, id: u32) -> bool {
    let infos = world.zoo.shows.infos.iter().flatten();
    infos
        .into_iter()
        .any(|i| u32::from(i.id) == id && !i.units(KEEPER).is_empty())
}

/// The sound of a reaction or of an end (shows 6.2 `show_sound`): the grandstands that face the
/// show now are counted and one draw picks the one that plays. What plays, by how many guests
/// are there, waits for the sound specification; the draw is made.
fn show_sound(world: &World, cat: &Catalogue, rng: &mut Rng, show: u32, kind: u16) {
    if ![
        event::BAD,
        event::GOOD,
        event::GREAT,
        event::CHEER,
        event::BOO,
    ]
    .contains(&kind)
    {
        return;
    }
    let sight = Sight::of(world, cat);
    let facing = (sight.stands.iter())
        .filter(|&&(_, at)| u32::from(sight.seen(world, at)) == show)
        .count();
    if facing > 0 {
        rng.pick(facing as u32);
    }
}

/// The world manager's listen (shows 6.2), after the AI manager of the same step: the show events
/// of the step go to the tank and to the grandstands that belong to the show, and the cheer or the
/// boo that follows an end is posted for later.
pub fn listen_world(world: &World, cat: &Catalogue, events: &mut Events, rng: &mut Rng) {
    let heard: Vec<Event> = events.listen_all(WORLD_CHANNEL).cloned().collect();
    let stands = grandstands(world, cat);
    // Only the first abort of the step counts.
    let aborted = (heard.iter())
        .find(|e| e.kind == event::ABORTED)
        .map(|e| e.sender);
    for ev in heard {
        let show = ev.sender;
        if !RELAYED.contains(&ev.kind) {
            if [event::CHEER, event::BOO].contains(&ev.kind) {
                show_sound(world, cat, rng, ev.data, ev.kind);
            }
            continue;
        }
        if ev.kind == event::ANNOUNCE && !has_keeper(world, show) {
            continue;
        }
        let data = ev.data & 0xffff;
        events.post(world_event(ev.kind, show, TANK_CHANNEL, data, ev.extra, 0));
        let kind = match ev.kind {
            event::STARTED => event::START_NOTE,
            k => k,
        };
        for &(id, _) in &stands {
            if (world.units.buildings.get(&id)).is_some_and(|l| u32::from(l.show_id) == show) {
                events.post(world_event(kind, id, BUILDING_CHANNEL, data, ev.extra, 0));
            }
        }
        let kept = aborted != Some(show);
        if ev.kind == event::ENDED && kept {
            let cheer = world_event(event::CHEER, 0, WORLD_CHANNEL, show, ev.data as u16, LATER);
            events.post(cheer);
        } else if kept && ev.kind != event::ABORTED && ev.kind != event::ENDED {
            show_sound(world, cat, rng, show, ev.kind);
        } else {
            events.post(world_event(event::BOO, 0, WORLD_CHANNEL, show, 0, LATER));
        }
    }
}

/// The grandstand at position `at` of the entity list reads its events (shows 6.3), in its turn
/// of the entity loop: the announce raises its "show soon" flag; an end, an abort and the
/// reactions go to every guest seated on a draw place, after a delay of 0 to 5 steps (90 to 129
/// more for an end or an abort). A grandstand that does not face the show it belongs to turns
/// every event into an abort.
pub fn listen_grandstand(
    world: &mut World,
    cat: &Catalogue,
    events: &mut Events,
    rng: &mut Rng,
    at: usize,
) {
    let Some(id) = (world.zoo.entities[at].common.as_ref())
        .filter(|_| world.zoo.entities[at].class == "building")
        .map(|c| c.id)
    else {
        return;
    };
    let heard: Vec<Event> = events.listen(id, BUILDING_CHANNEL).cloned().collect();
    if heard.is_empty() {
        return;
    }
    let default = world.units.prices.default;
    let Some(kind) =
        kind_of(cat, &world.zoo.entities[at], default).filter(|k| k.satisfies("grandstand"))
    else {
        return;
    };
    if !world.units.buildings.contains_key(&id) {
        return;
    }
    let seen = Sight::of(world, cat).seen(world, at);
    for ev in heard {
        let own = world.units.buildings.get(&id).map_or(0, |l| l.show_id);
        let t = if seen == 0 || seen != own {
            event::ABORTED
        } else {
            ev.kind
        };
        let Some(live) = world.units.buildings.get_mut(&id) else {
            return;
        };
        if t == event::ANNOUNCE {
            live.fill = 1;
        } else if ENDS.contains(&t) || REACTIONS.contains(&t) {
            let over = ENDS.contains(&t);
            for (k, slot) in kind.slots.iter().enumerate() {
                if slot.places.is_none() {
                    continue;
                }
                let mut seated: Vec<_> = live.seats.iter().filter(|s| s.slot == k).collect();
                seated.sort_by_key(|s| s.place);
                for s in seated {
                    if !live.users.iter().any(|u| u.0 == s.unit) {
                        continue;
                    }
                    let mut delay = rng.pick(6) as u16;
                    if over {
                        delay += 90 + rng.pick(35) as u16;
                    }
                    events.post(Event {
                        sender: id,
                        target: s.unit,
                        data: ev.data,
                        extra: ev.extra,
                        kind: t,
                        delay,
                        sender_channel: BUILDING_CHANNEL,
                        channel: UNIT_CHANNEL,
                    });
                }
            }
            if over {
                live.fill = 0;
            }
        }
    }
}
