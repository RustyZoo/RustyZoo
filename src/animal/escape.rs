//! The escape walk of an animal (animals 11.3): how it crosses the fence on its way out, what it
//! does on the tiles of the walk, and the set of an escaped animal (behaviours 6.4 point 1). The
//! search that starts the walk is the random walk's pick (`crate::unit::walk`).
//!
//! Not here yet: the rampage sets and the rampage's effects, the break sound.

use crate::behaviour::{Data, Env as Run};
use crate::entity::Catalogue;
use crate::path::Mover;
use crate::sim::Rng;
use crate::unit::movement::DIR;
use crate::unit::{Part, Unit};
use crate::world::World;

/// How an animal crosses the line that holds the fence pieces `(FA, FB)` (11.3 `choose_kind`):
/// out of a tank 4; else, for each intact piece in turn (life not 0), 1 when it jumps it, 2 when
/// it climbs it, 3 when it bashes (the piece's strength below the bash) or crushes it; 0 walks
/// through.
pub fn kind(w: &World, m: &Mover, tank: bool, (fa, fb): (Option<usize>, Option<usize>)) -> u32 {
    if tank {
        return 4;
    }
    for p in [fa, fb].into_iter().flatten() {
        let (Some(s), Some(c)) = (w.shape(p), w.zoo.entities[p].common.as_ref()) else {
            continue;
        };
        let k = s.tank;
        let (life, strength) = c
            .fence
            .map_or((k.life, k.strength), |f| (f.life, f.strength));
        if life == 0 {
            continue;
        }
        if m.jumper && k.jumpable {
            return 1;
        }
        if m.climber && k.climbable {
            return 2;
        }
        if i64::from(m.bash) > i64::from(strength) || m.crush != 0 && m.crush >= k.height {
            return 3;
        }
    }
    0
}

/// The animation a crossing of this kind plays on the crossing tile.
pub fn crossing(kind: u32) -> Option<&'static str> {
    match kind {
        1 => Some("jump_high"),
        2 => Some("climb_up"),
        4 => Some("exit_tank"),
        _ => None,
    }
}

/// The animal on an escape walk entered tile `new` (11.3 `intoNewTile`): the crossing tile `A`
/// plays the crossing animation; leaving it, or reaching the last tile `B` without having entered
/// it, breaks the fence of a bash, or of a jump by a species that bashes; `B` plays `run` after a
/// jump or a climb.
pub fn entered(unit: &mut Unit, world: &mut World, cat: &Catalogue, new: usize) {
    let bash = unit.mover.bash > 0;
    let Part::Animal(a) = &mut unit.part else {
        return;
    };
    let Some(w) = a.walk.as_mut() else {
        return;
    };
    let tile = |p: [i32; 2]| world.tile_index(p[0], p[1]);
    let (ta, tb) = (tile(w.a), tile(w.b));
    let breaks = w.broken == 0 && (w.kind == 3 || w.kind == 1 && bash);
    let mut smash = false;
    if Some(new) == ta {
        w.reached = 1;
        a.force = crossing(w.kind);
    } else if w.reached == 1 && (Some(new) != tb || w.across == 1) {
        smash = breaks;
    }
    if Some(new) == tb {
        if matches!(w.kind, 1 | 2) {
            a.force = Some("run");
        }
        smash |= w.reached == 0 && breaks;
    }
    if smash {
        w.broken = 1;
        let line = (w.a, w.b);
        break_line(world, cat, line, new);
    }
}

/// Breaks the fence behind tile `t` on the way from `A` to `B` (the sign direction `d`): the piece
/// on `t`'s side `d + 4` and the one on side `d` of the neighbour that way, each only when it is a
/// piece that can break.
fn break_line(world: &mut World, cat: &Catalogue, (a, b): ([i32; 2], [i32; 2]), t: usize) {
    let step = ((b[0] - a[0]).signum(), (b[1] - a[1]).signum());
    let Some(d) = DIR.iter().position(|&s| s == step) else {
        return;
    };
    let back = (d + 4) % 8;
    let behind = world.neighbour(t, back).and_then(|n| world.edge(n, d));
    let mut lines = Vec::new();
    for p in [world.edge(t, back), behind].into_iter().flatten() {
        if !world.wears(cat, p) {
            continue;
        }
        let fence = (world.zoo.entities[p].common.as_mut()).and_then(|c| c.fence.as_mut());
        if let Some(f) = fence.filter(|f| f.life > 1) {
            f.life = 1;
        }
        let events = world.wear(cat, p, u32::MAX, &mut lines);
        world.units.posts.extend(events);
    }
    lines.into_iter().for_each(|l| world.units.world_note(l));
}

/// The walk ends on its last tile (11.3, the check done of the wander): with the animal on tile
/// `here`, the record is dropped. Whether the animal is on an escape walk, which leaves the end of
/// the wander undecided.
pub fn walk_done(unit: &mut Unit, world: &World, here: Option<usize>) -> bool {
    let Part::Animal(a) = &mut unit.part else {
        return false;
    };
    let Some(w) = a.walk else {
        return false;
    };
    if here.is_some() && here == world.tile_index(w.b[0], w.b[1]) {
        a.walk = None;
    }
    true
}

/// Behaviours 6.4 point 1, the escaped branch: an escaped animal that stands on dry land, is not
/// moving and is not rampaging starts `bEscaped` unless it runs already (interruptible, forced,
/// the queued sequences kept; a pending `bEscaped` is cleared).
pub fn escaped_set(unit: &mut Unit, world: &World, data: &dyn Data, rng: &mut Rng, at: usize) {
    let Part::Animal(a) = &unit.part else {
        return;
    };
    let tile = (world.zoo.entities[at].common.as_ref())
        .and_then(|c| world.tile_index(c.x.div_euclid(64), c.y.div_euclid(64)));
    let dry = tile.is_some_and(|t| !world.is_water(t) && world.tank_at(t).is_none());
    let held = a.dying || a.crated || a.egg || a.in_show;
    if held || !a.escaped || !dry || a.rampaging || unit.moving {
        return;
    }
    const SET: &str = "bEscaped";
    if unit.runner.running() && unit.runner.top() == Some(SET) {
        return;
    }
    let env = Run {
        data,
        unit: &world.zoo.entities[at],
        rng,
    };
    unit.runner
        .start(&mut unit.animation, &env, SET, true, true, true);
    if unit.runner.queued() == Some(SET) {
        unit.runner.forget();
    }
}
