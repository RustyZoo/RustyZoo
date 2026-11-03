//! A guest's reactions to the events of a show (shows 6.4): the thought, the change of its
//! status and the set it starts for an end, an abort and a trick.

use super::checks::update;
use crate::behaviour::{Data, Env};
use crate::building::leave::apply;
use crate::show::event;
use crate::unit::{Ctx, Part, Unit};
use crate::world::World;

/// The sets of the exits: bad, neutral, good, great.
const EXITS: [&str; 4] = [
    "bExitShowBad",
    "bExitShow",
    "bExitShowGood",
    "bExitShowGreat",
];
/// The thoughts of an end, by the same four steps.
const END_THOUGHTS: [i32; 4] = [10287, 10288, 10289, 10290];

/// The guest `unit` reads the show events of its turn (shows 6.4).
pub(crate) fn react(unit: &mut Unit, world: &mut World, ctx: &mut Ctx, at: usize, data: &dyn Data) {
    let Some(max_energy) = (matches!(unit.part, Part::Guest(_)))
        .then(|| super::keys(world, ctx.cat, at))
        .flatten()
        .map(|k| k.max_energy)
    else {
        return;
    };
    let heard = std::mem::take(&mut unit.events);
    let ended = heard.iter().any(|e| e.kind == event::ENDED);
    let mut aborted = heard.iter().any(|e| e.kind == event::ABORTED);
    for ev in &heard {
        aborted |= unit.runner.top() == Some(EXITS[0]);
        // The percent tests compare unsigned 32-bit numbers (shows 6.4 point 3).
        let [bad, good, great] = world.units.show.show.map(|n| n as u32);
        let p = ev.data;
        // The great limit is tested before the good one (shows 6.4 point 3): the set and the
        // thought are the same band.
        let band = match p {
            _ if p <= bad => 0,
            _ if p >= great => 3,
            _ if p >= good => 2,
            _ => 1,
        };
        let mut name = None;
        // An abort changes nothing: no record, no status update.
        let mut changes = true;
        match ev.kind {
            event::ENDED => {
                name = (!aborted).then(|| EXITS[band]);
                world.units.think(unit.id, END_THOUGHTS[band], None, None);
                apply(unit, [0, 0, 0, 0, 0, -i32::from(ev.extra)]);
            }
            event::ABORTED => {
                name = Some(EXITS[0]);
                changes = false;
            }
            event::BAD | event::GOOD | event::GREAT => {
                if !aborted && !ended {
                    name = Some(match ev.kind {
                        event::BAD => "bBadTrick",
                        event::GOOD => "bGoodTrick",
                        _ => "bGreatTrick",
                    });
                }
                let gain = ev.data as i32;
                apply(unit, [gain, gain, 0, 0, 0, 0]);
            }
            _ => continue,
        }
        if let Part::Guest(g) = &mut unit.part
            && changes
        {
            update(g, max_energy);
        }
        if let Some(text) = match ev.kind {
            event::BAD => Some(10610),
            event::GOOD => Some(10612),
            event::GREAT => Some(10613),
            _ => None,
        } {
            world.units.think(unit.id, text, None, None);
        }
        if let Some(name) = name {
            let env = Env {
                data,
                unit: &world.zoo.entities[at],
                rng: &mut *ctx.rng,
            };
            unit.runner
                .start(&mut unit.animation, &env, name, true, true, true);
        }
    }
    unit.events = heard;
}
