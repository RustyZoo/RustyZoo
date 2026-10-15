//! The path finder of the original (pathfinding): a search over tiles with the costs of the unit
//! that asks, and the list of destinations found unreachable.
//!
//! The finder works on tiles on land. The depth search of a tank (3.8) and what needs the live
//! units (other animals on a tile, the tasks that change a cost, the unit's facing in a random
//! walk, food under a keeper) are not here yet: a unit table does not exist.

use crate::config::Config;
use crate::timing::ini_whole;
use crate::world::World;
use std::collections::HashMap;

mod cost;
mod search;
#[cfg(test)]
mod tests;

pub use cost::{step, terrain};
pub use search::search;

/// The cost of a closed step: 20 or more is closed.
pub const MAX: u32 = 20;

/// The class that decides the costs of a unit.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    /// A zookeeper or a marine specialist.
    #[default]
    Keeper,
    /// A maintenance worker or a scientist.
    Worker,
    Guide,
    Guest,
    Animal,
}

/// The unit that asks: its kind, its tile and the type values the costs read.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Mover {
    pub kind: Kind,
    /// The unit's own tile; none for the temporary keeper of the exhibits' tests.
    pub at: Option<usize>,
    /// `cSwims`, `cUnderwater`, `cOnlyUnderwater`, `cTall`, `cClimbsCliffs` of the type.
    pub swims: bool,
    pub underwater: bool,
    pub only_underwater: bool,
    pub tall: bool,
    pub climbs: bool,
    /// `cFootprintX * cFootprintY`, at most 4.
    pub footprint: u32,
    /// `cNonPathCost` of a guest.
    pub non_path_cost: u32,
    /// A keeper in relaxed mode (6.2).
    pub relaxed: bool,
    /// A guest on its way out of the zoo (task 0x71).
    pub leaving: bool,
    /// An animal in the exhibit's reachability test, and one in water mode (terrain mode 1).
    pub test: bool,
    pub water_mode: bool,
}

impl Mover {
    /// A unit of this kind on tile `at` with plain values.
    pub fn new(kind: Kind, at: Option<usize>) -> Mover {
        Mover {
            kind,
            at,
            footprint: 1,
            non_path_cost: 7,
            ..Mover::default()
        }
    }

    /// The temporary keeper of the exhibits' tests: a marine specialist for a tank (`diver`).
    pub fn keeper(diver: bool, relaxed: bool) -> Mover {
        let mut keeper = Mover::new(Kind::Keeper, None);
        (keeper.swims, keeper.underwater, keeper.relaxed) = (diver, diver, relaxed);
        keeper
    }
}

/// The `[AI]` settings of the ini file (9).
#[derive(Debug, Clone, PartialEq)]
pub struct Settings {
    pub diagonal: bool,
    pub node_max: usize,
    pub min_cost: i32,
    pub estimate: i32,
    pub min_unreachable: i8,
    pub clear_every: u64,
}

impl Default for Settings {
    fn default() -> Settings {
        Settings {
            diagonal: true,
            node_max: 500,
            min_cost: 2,
            estimate: 1,
            min_unreachable: 5,
            clear_every: 5,
        }
    }
}

impl Settings {
    /// The settings of an ini file; a missing key keeps its fallback.
    pub fn from_ini(ini: &Config) -> Settings {
        let d = Settings::default();
        let whole = |key: &str| ini_whole(ini, "AI", key);
        Settings {
            diagonal: whole("AllowDiagonalMoves").is_none_or(|v| v == 1),
            node_max: whole("NodeMax").map_or(d.node_max, |v| v.max(0) as usize),
            min_cost: whole("MinCost").unwrap_or(d.min_cost),
            estimate: whole("EstimateFunc").unwrap_or(d.estimate),
            // Kept in a signed byte.
            min_unreachable: whole("MinUnreachableCount").map_or(d.min_unreachable, |v| v as i8),
            clear_every: whole("ClearUnreachableFreq").map_or(d.clear_every, |v| v.max(0) as u64),
        }
    }
}

/// 4.2: whether a unit may take the tile step `(a, b, d)` from tile `a` to its neighbour `b` in
/// direction `d`. With `building` (the unit goes in or out of a building) only the edge cost is
/// looked at, not the terrain cost of `b`.
pub fn allowed(
    w: &World,
    m: &Mover,
    s: &Settings,
    (a, b, d): (usize, usize, usize),
    building: bool,
) -> bool {
    if !building {
        return search::open(w, m, s, (a, b, d), None).is_some();
    }
    cost::clear(w, m, a, b, d, search::diagonal(m, s))
        && w.tank_at(b).is_none()
        && cost::edge(w, m, a, d) < MAX
}

/// What to do with the unit's destination tile after a request.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Dest {
    Keep,
    Set(usize),
    Clear,
}

/// The answer to a request (3.1).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Reply {
    pub ok: bool,
    /// The unit's new path, when the request made one.
    pub path: Option<Vec<usize>>,
    pub dest: Dest,
}

/// The finder: the settings, the width of the first map that asked and the unreachable list.
#[derive(Debug, Default)]
pub struct Finder {
    pub settings: Settings,
    width: Option<i32>,
    /// Failures by destination tile (7).
    unreachable: HashMap<usize, i8>,
    /// The AI tick at which the list is emptied next; the first update empties it.
    clear_at: u64,
}

impl Finder {
    pub fn new(settings: Settings) -> Finder {
        Finder {
            settings,
            ..Finder::default()
        }
    }

    /// The AI update at AI tick `now` (the whole ticks of the AI clock, which the step driver owns):
    /// once the clearing time is reached the unreachable list is emptied.
    pub fn tick(&mut self, now: u64) {
        if now >= self.clear_at {
            self.unreachable.clear();
            self.clear_at = now.saturating_add(self.settings.clear_every);
        }
    }

    /// The failures held for destination tile `t`.
    pub fn failures(&self, t: usize) -> Option<i8> {
        self.unreachable.get(&t).copied()
    }

    /// A success or a refusal moves the counter of `t` one step towards 0 (removed at 0).
    fn ease(&mut self, t: usize) {
        if let Some(c) = self.unreachable.get_mut(&t) {
            *c = c.saturating_sub(c.signum());
            if *c == 0 {
                self.unreachable.remove(&t);
            }
        }
    }

    /// Records a failure: 1 for a new entry, else one more, and `-M` on reaching `M`.
    fn record(&mut self, t: usize) {
        let m = self.settings.min_unreachable;
        match self.unreachable.get_mut(&t) {
            None => drop(self.unreachable.insert(t, 1)),
            Some(c) => {
                *c = c.saturating_add(1);
                if *c == m {
                    *c = m.wrapping_neg();
                }
            }
        }
    }

    /// A unit asks for a path to tile `to` (3.1). `partial` accepts a path to the node nearest
    /// the goal.
    pub fn request(&mut self, w: &World, m: &Mover, to: usize, partial: bool) -> Reply {
        let reply = |ok, path, dest| Reply { ok, path, dest };
        let size = w.zoo.tiles.len();
        let Some(from) = m.at.filter(|&s| s < size && to < size) else {
            return reply(false, None, Dest::Keep);
        };
        // The threshold keeps the width of the first map that asked (O5); the distance itself is
        // measured on the current map.
        let threshold = *self.width.get_or_insert(w.zoo.width as i32) as usize;
        let width = (w.zoo.width as usize).max(1);
        let near = |t: usize| (t % width, t / width);
        let ((fx, fy), (tx, ty)) = (near(from), near(to));
        let far = fx.abs_diff(tx) + fy.abs_diff(ty);
        if self.unreachable.get(&to).is_some_and(|&c| c < 0) {
            self.ease(to);
            return reply(false, None, Dest::Keep);
        }
        if from == to {
            return reply(true, Some(vec![from]), Dest::Set(to));
        }
        // Items 5 to 7 see no goal tile (3.1).
        let s = &self.settings;
        let enter = (0..4).any(|d| {
            let a = w.neighbour(to, d * 2);
            a.is_some_and(|a| {
                search::open(w, m, s, (a, to, search::direction(w, a, to)), None).is_some()
            })
        });
        let leave = (0..8)
            .step_by(if search::diagonal(m, s) { 1 } else { 2 })
            .any(|d| {
                let n = w.neighbour(from, d);
                n.is_some_and(|n| search::open(w, m, s, (from, n, d), None).is_some())
            });
        if terrain(w, m, to, None) >= MAX || !enter || !leave {
            return reply(false, None, Dest::Keep);
        }
        let found = search(w, m, s, from, to, partial);
        if found.reached {
            self.ease(to);
            return reply(true, Some(found.path), Dest::Set(to));
        }
        match found.path.last().copied() {
            None => {
                if far <= threshold {
                    self.record(to);
                }
                reply(false, Some(found.path), Dest::Clear)
            }
            Some(end) => {
                if far <= threshold / 2 {
                    self.record(to);
                }
                reply(true, Some(found.path), Dest::Set(end))
            }
        }
    }
}

/// The cost of the pair of tiles `(a, b)` of an exhibit's boundary for a keeper with the line's
/// pieces taken off (8): the edge costs 1, so it is one more than the larger terrain cost of the
/// two, 20 at most.
pub fn pair_cost(w: &World, m: &Mover, a: usize, b: usize) -> u32 {
    (1 + terrain(w, m, a, None).max(terrain(w, m, b, None))).min(MAX)
}
