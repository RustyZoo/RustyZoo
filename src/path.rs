//! The path finder of the original (pathfinding): a search over tiles with the costs of the unit
//! that asks, and the list of destinations found unreachable.
//!
//! The search is over tiles, or over (tile, level) when the unit is in a tank or either end is a
//! tank tile (the depth search, 3.8 and tank fauna 7). What a
//! cost reads of the live unit (its destination, facing, tasks and water states) is a field of
//! [`Mover`] that the unit code sets; the other units on a tile come from the world's map index.

use crate::config::Config;
use crate::timing::ini_whole;
use crate::world::World;
use std::collections::HashMap;

mod cost;
mod search;
#[cfg(test)]
mod tests;

pub use cost::{in_reach, step, terrain};
use search::open_deep;
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
    /// The unit's own id, to leave it out of the units on a tile.
    pub id: Option<u32>,
    /// The unit is in a tank (move kind 3), `cSurface` and `cDepth` of its type, and the levels
    /// the depth search starts and ends at (the unit's level and the destination's).
    pub deep: bool,
    pub surface: bool,
    pub depth: i32,
    pub levels: (i32, i32),
    /// The direction and the vertical direction the start node of a depth search is entered with:
    /// the unit's facing and its vertical move (3.6).
    pub start: (usize, i32),
    /// `cAvoidEdges` of the animal's type: the radius, in tiles, of the fences it keeps off.
    pub avoid_edges: u32,
    /// The tile the unit is going to now (before a new request sets it).
    pub dest: Option<usize>,
    /// An animal going to or from a show tank (tasks 0x85 and 0x86).
    pub show: bool,
    /// The id of the unit an animal chases (task 0x74).
    pub chasing: Option<u32>,
    /// The facing of a unit in a random walk (`fRandomWalk`), else none.
    pub turn: Option<usize>,
    /// The unit goes in or out of a building: only the edge cost decides a step (4.2).
    pub building: bool,
    /// The id of the building the unit is going to, with the flag up (4.3 point 1).
    pub target_building: Option<u32>,
    /// A guest travelling in water, one recovering from a fall into water or entering a building,
    /// and one avoiding a predator (task 0x75), with the `cNonPathCostEmergency` it then pays.
    pub in_water: bool,
    pub recovering: bool,
    pub avoiding: bool,
    pub emergency_cost: u32,
    /// The ids of the units that are predators for this guest (M7 decides who they are).
    pub predators: Vec<u32>,
}

impl Mover {
    /// A unit of this kind on tile `at` with plain values.
    pub fn new(kind: Kind, at: Option<usize>) -> Mover {
        Mover {
            kind,
            at,
            footprint: 1,
            non_path_cost: 7,
            emergency_cost: 1,
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
/// direction `d`. With `building` (or the unit's own flag: it goes in or out of a building) only
/// the edge cost is looked at, not the terrain cost of `b`.
pub fn allowed(
    w: &World,
    m: &Mover,
    s: &Settings,
    (a, b, d): (usize, usize, usize),
    building: bool,
) -> bool {
    search::open(w, m, s, (a, b, d), None, building || m.building).is_some()
}

/// 4.2 in a depth search: the same for the step from level `za` of `a` to level `zb` of `b`.
pub fn allowed_deep(
    w: &World,
    m: &Mover,
    s: &Settings,
    ((a, za), (b, zb), d): ((usize, i32), (usize, i32), usize),
    building: bool,
) -> bool {
    open_deep(w, m, s, ((a, za), (b, zb), d), None, building || m.building).is_some()
}

/// The move cost of a path (3.6): each step's cost times its length factor, with the last tile as
/// the goal; the product and each add are rounded to single precision (3.3).
pub fn path_cost(w: &World, m: &Mover, path: &[usize]) -> f64 {
    let goal = path.last().copied();
    let step = |p: &[usize]| {
        let d = search::direction(w, p[0], p[1]);
        let length = [1.0, search::DIAGONAL][d % 2];
        search::single(length * f64::from(cost::step(w, m, p[0], p[1], d, goal)))
    };
    path.windows(2)
        .map(step)
        .fold(0.0, |a, b| search::single(a + b))
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
    /// The unit's new path, when the request made one, and the level of each of its tiles when
    /// the search was in depth (else empty).
    pub path: Option<Vec<usize>>,
    pub levels: Vec<i32>,
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
    /// The diagonal switch the last search left (3.1 item 8); none before the first one.
    switch: Option<bool>,
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

    /// The clearing time, the width, the switch and the unreachable list in tile order, as words
    /// for the hash of a run.
    pub fn state(&self) -> Vec<u64> {
        let mut list: Vec<_> = self.unreachable.iter().collect();
        list.sort();
        let head = [self.clear_at, self.width.map_or(0, |w| w as u64 + 1)];
        let switch = self.switch.map_or(2, u64::from);
        let list = list.into_iter().flat_map(|(&t, &c)| [t as u64, c as u64]);
        head.into_iter().chain([switch]).chain(list).collect()
    }

    /// The settings with the diagonal switch as the last search of any unit left it: the gate
    /// choice of the exhibits searches with these (pathfinding 8).
    pub fn last(&self) -> Settings {
        Settings {
            diagonal: self.switch.unwrap_or(self.settings.diagonal),
            ..self.settings.clone()
        }
    }

    /// The search of an exhibit's reachability test (habitat rating 5.8) from the unit's tile to
    /// tile `to`: the search alone, with no partial path and no unreachable list, after the
    /// diagonal switch takes the unit's rule. Whether it reaches `to`.
    pub fn test(&mut self, w: &World, m: &Mover, to: usize) -> bool {
        let Some(from) =
            m.at.filter(|&s| s < w.zoo.tiles.len() && to < w.zoo.tiles.len())
        else {
            return false;
        };
        self.switch = Some(search::diagonal(m, &self.settings));
        search(w, m, &self.settings, from, to, false).reached
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
        let reply = |ok, path, dest| Reply {
            ok,
            path,
            levels: Vec::new(),
            dest,
        };
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
        // The search is in depth for a unit in a tank or an end in one (3.1 item 2).
        let deep = m.deep || w.tank_at(from).is_some() || w.tank_at(to).is_some();
        let (z0, z1) = m.levels;
        if from == to && (!deep || z0 == z1) {
            let levels = if deep { vec![z0] } else { Vec::new() };
            return Reply {
                levels,
                ..reply(true, Some(vec![from]), Dest::Set(to))
            };
        }
        // Items 5 to 7 see no goal tile (3.1); in depth each neighbour is tried at the levels
        // -1, 0 and +1.
        let s = &self.settings;
        let around = if deep { -1..=1 } else { 0..=0 };
        let ok = |(a, za): (usize, i32), (b, zb): (usize, i32), d: usize| {
            match deep {
                true => open_deep(w, m, s, ((a, za), (b, zb), d), None, m.building),
                false => search::open(w, m, s, (a, b, d), None, m.building),
            }
            .is_some()
        };
        let enter = (0..4).any(|d| {
            let a = w.neighbour(to, d * 2);
            a.is_some_and(|a| {
                let d = search::direction(w, a, to);
                around
                    .clone()
                    .any(|dz| ok((a, z1.saturating_add(dz)), (to, z1), d))
            })
        });
        let leave = (0..8)
            .step_by(if search::diagonal(m, s) { 1 } else { 2 })
            .any(|d| {
                let n = w.neighbour(from, d);
                n.is_some_and(|n| {
                    around
                        .clone()
                        .any(|dz| ok((from, z0), (n, z0.saturating_add(dz)), d))
                })
            });
        if terrain(w, m, to, None) >= MAX || !enter || !leave {
            return reply(false, None, Dest::Keep);
        }
        // Only a request that gets to its search sets the diagonal switch (3.1 item 8).
        self.switch = Some(search::diagonal(m, s));
        let found = search(w, m, s, from, to, partial);
        if found.reached {
            self.ease(to);
            return Reply {
                levels: found.levels,
                ..reply(true, Some(found.path), Dest::Set(to))
            };
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
                Reply {
                    levels: found.levels,
                    ..reply(true, Some(found.path), Dest::Set(end))
                }
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
