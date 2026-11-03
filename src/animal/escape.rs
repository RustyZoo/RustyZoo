//! The escape walk of an animal (animals 11.3): how it crosses the fence on its way out, what it
//! does on the tiles of the walk, the sets of an escaped or rampaging animal (behaviours 6.4
//! points 1 and 2) and what a rampage does to the scenery (animals 11). The search that starts
//! the walk is the random walk's pick (`crate::unit::walk`).

use super::{Animal, Class, ESCAPE, Env, Keys, Out, fire};
use crate::behaviour::{Data, Env as Run};
use crate::entity::Catalogue;
use crate::exhibit::{EXTERIOR, GROUNDS};
use crate::path::Mover;
use crate::rubble::{Kit, PIECE_NAME, leave_rubble};
use crate::sim::Rng;
use crate::unit::movement::DIR;
use crate::unit::{Ctx, Part, Unit};
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

/// A rampaging animal that is not on an escape walk entered tile `t` (animals 11.9 point 4): each
/// object of its four quadrants whose type has `cRubbleable` 1, one that spans several quadrants
/// once, turns to rubble with the routine of its kind (cheats 4.6). The object leaves the list at
/// the next removal pass.
pub fn trample(unit: &Unit, world: &mut World, ctx: &mut Ctx, t: usize) {
    if !matches!(&unit.part, Part::Animal(a) if a.rampaging && a.walk.is_none()) {
        return;
    }
    let mut ids = Vec::new();
    for q in 0..4 {
        let Some(e) = world
            .quadrant(t, q)
            .filter(|&e| world.shape(e).is_some_and(|s| s.rubbleable))
        else {
            continue;
        };
        let id = world.zoo.entities[e].common.as_ref().map(|c| c.id);
        if let Some(id) = id.filter(|id| !ids.contains(id) && !ctx.removals.contains(id)) {
            ids.push(id);
        }
    }
    let (cat, template) = (
        ctx.cat,
        world
            .units
            .names
            .get(&PIECE_NAME)
            .cloned()
            .unwrap_or_default(),
    );
    let shape = |e: &crate::zoo::Entity| crate::render::shape_of(cat, e);
    let mut kit = Kit {
        cat,
        rng: &mut *ctx.rng,
        template: &template,
        shape: &shape,
    };
    for id in ids {
        if leave_rubble(world, &mut kit, id) {
            ctx.removals.push(id);
        }
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

/// The unit's tile and whether it stands in an exhibit that is not the zoo grounds.
fn place(world: &World, at: usize) -> (Option<usize>, bool) {
    let tile = (world.zoo.entities[at].common.as_ref())
        .and_then(|c| world.tile_index(c.x.div_euclid(64), c.y.div_euclid(64)));
    let region = (tile.and_then(|t| world.exhibits().region(t))).unwrap_or(GROUNDS);
    (tile, region != GROUNDS && region != EXTERIOR)
}

/// `start_new` of a set (behaviours 4.2): nothing when it runs already; else it starts forced with
/// the queued sequences kept, and a pending set of that name is forgotten. Whether it succeeded.
pub(super) fn start_new(
    unit: &mut Unit,
    world: &World,
    data: &dyn Data,
    rng: &mut Rng,
    at: usize,
    set: (&str, bool),
) -> bool {
    let (name, interruptible) = set;
    if unit.runner.running() && unit.runner.top() == Some(name) {
        return true;
    }
    let env = Run {
        data,
        unit: &world.zoo.entities[at],
        rng,
    };
    let started = unit
        .runner
        .start(&mut unit.animation, &env, name, interruptible, true, true);
    if unit.runner.queued() == Some(name) {
        unit.runner.forget();
    }
    started
}

/// Behaviours 6.4 points 0 and 1, the escaped and rampage sets. An escaped animal on dry land
/// that is not moving and not rampaging starts `bEscaped` (interruptible); a rampaging one that is
/// loose runs `bEscapedRampage`, one in an exhibit `bHabitatRampage`, neither interruptible. A
/// set that runs already is queued behind itself. With `bSleep`, `bSick` or `bHatch` as the top or
/// pending set the rampage is dropped and nothing starts.
pub fn ambient_sets(unit: &mut Unit, world: &World, data: &dyn Data, rng: &mut Rng, at: usize) {
    let ending = [unit.runner.top(), unit.runner.queued()]
        .iter()
        .any(|n| matches!(n, Some("bSleep" | "bSick" | "bHatch")));
    let Part::Animal(a) = &mut unit.part else {
        return;
    };
    if ending {
        (a.rampaging, a.rampage) = (false, 0);
        return;
    }
    let (tile, in_exhibit) = place(world, at);
    let dry = tile.is_some_and(|t| !world.is_water(t) && world.tank_at(t).is_none());
    let held = a.dying || a.crated || a.egg || a.in_show || a.tranq;
    let (loose, rampaging) = (a.escaped && dry, a.rampaging);
    if held {
        return;
    }
    let (name, ignore_moving) = match (loose, rampaging) {
        (false, true) if in_exhibit => (HABITAT_RAMPAGE, true),
        (true, true) => (LOOSE_RAMPAGE, false),
        (true, false) if !unit.moving => {
            start_new(unit, world, data, rng, at, (ESCAPED, true));
            return;
        }
        _ => return,
    };
    if unit.runner.top() == Some(name) {
        if unit.runner.queued() != Some(name) {
            unit.runner.queue(name, false, true);
        }
    } else if ignore_moving || !unit.moving {
        start_new(unit, world, data, rng, at, (name, false));
    }
}

/// Behaviours 6.4 point 2, before the table: the sleep check takes the choice (2.1), else a
/// rampage in an exhibit starts its set (2.2). Whether either did, which ends the choice.
pub fn chose(
    unit: &mut Unit,
    world: &mut World,
    ctx: &mut Ctx,
    at: usize,
    data: &dyn Data,
) -> bool {
    super::life::ambient_sleep(unit, world, ctx, at, data)
        || habitat_rampage(unit, world, data, ctx.rng, at)
}

/// Behaviours 6.4 point 2.2: an idle animal rampaging in an exhibit starts `bHabitatRampage`,
/// which ends the choice. Whether it did.
pub fn habitat_rampage(
    unit: &mut Unit,
    world: &World,
    data: &dyn Data,
    rng: &mut Rng,
    at: usize,
) -> bool {
    let idle = !unit.runner.running() && unit.runner.queued().is_none();
    let Part::Animal(a) = &unit.part else {
        return false;
    };
    let held = a.dying || a.crated || a.egg || a.tranq;
    if !idle || held || !a.rampaging || !place(world, at).1 {
        return false;
    }
    start_new(unit, world, data, rng, at, (HABITAT_RAMPAGE, false));
    true
}

/// What a tick of the checks asks after its escape check (animals 11): the status dates a loose
/// animal's escape, the fence test starts when the zap draw succeeded (11.10), a rampage that
/// ended is cleared, and an animal on water is sent off it.
pub fn after(
    unit: &mut Unit,
    world: &mut World,
    ctx: &mut Ctx,
    at: usize,
    data: &dyn Data,
    (out, k): (&Out, &Keys),
) {
    world.units.loose |= out.loose;
    if out.calm {
        calm(unit, world, data, at);
    }
    if let Some(pick) = out.sent {
        let mode = if unit.mover.only_underwater { 2 } else { 0 };
        super::tank::send(unit, world, ctx, at, data, (mode, Some(pick)));
    }
    if out.fence {
        super::fence::test(unit, world, ctx, at, data, k);
    }
}

/// A rampage ended (animals 11): when its set runs, the pending call and the set are cleared, the
/// unit stops with its idle animation and the animal's status resets, which drops its escape
/// record and its need flags.
pub fn calm(unit: &mut Unit, world: &World, data: &dyn Data, at: usize) {
    if !matches!(unit.runner.top(), Some(LOOSE_RAMPAGE | HABITAT_RAMPAGE)) {
        return;
    }
    unit.runner = Default::default();
    crate::unit::movement::stop(unit);
    crate::unit::movement::idle(unit, world, data, at);
    if let Part::Animal(a) = &mut unit.part {
        (a.drinking, a.wants_food, a.thirsty, a.walk) = (false, false, false, None);
    }
}

const ESCAPED: &str = "bEscaped";
const LOOSE_RAMPAGE: &str = "bEscapedRampage";
const HABITAT_RAMPAGE: &str = "bHabitatRampage";

/// The escape check (animals 11.1): the rampage, then the counter; when it fires, an animal that
/// stands on the zoo grounds is escaped and takes the land mode, sent off the water (11.5), one in
/// an exhibit is not escaped any more.
pub(super) fn check(
    a: &mut Animal,
    k: &Keys,
    class: &Class,
    env: &Env,
    rng: &mut Rng,
    out: &mut Out,
) {
    if a.in_show {
        return;
    }
    if a.rampaging {
        a.rampage = a.rampage.wrapping_add(1);
        if a.happiness > k.rampage_threshold || a.rampage > k.rampage_max {
            (a.rampaging, a.rampage) = (false, 0);
            out.calm = true;
        }
    } else {
        a.rampage = 0;
        if k.rampage_chance >= 1
            && k.rampage_min >= 1
            && a.happiness <= k.rampage_threshold
            && rng.chance(k.rampage_chance)
        {
            let len = k.rampage_max.wrapping_sub(k.rampage_min);
            if len >= 0 {
                a.rampaging = true;
                a.rampage = len - rng.rand() as i32 % len.wrapping_add(1);
            }
        }
    }
    if !fire(&mut a.counters[ESCAPE], class.periods[ESCAPE]) {
        return;
    }
    if env.grounds {
        let was = std::mem::replace(&mut a.escaped, true);
        let mode = if k.only_underwater { 2 } else { 0 };
        if a.terrain != mode && env.stands != mode && env.kinds[usize::from(mode)] > 0 {
            out.sent = Some(rng.pick(env.kinds[usize::from(mode)] as u32) as usize);
        }
        a.terrain = mode;
        out.loose = true;
        if !was {
            (a.thirsty, a.drinking) = (false, false);
            out.log.push("escape");
        }
    } else {
        (a.escaped, a.claim) = (false, 0);
    }
}
