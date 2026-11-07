//! Shows (shows 3 to 6, 10): the numbers of `shows.cfg`, which show a grandstand belongs to, the
//! show cycle with the events that carry it to the grandstands, and the two routines that empty
//! the grandstands of a show.
//!
//! The state itself, the scripts and the show info of each tank, is [`crate::zoo::Shows`], loaded
//! and saved with the zoo. [`update`] is the show manager's step (the cycle is `cycle`), [`relay`]
//! the world manager's and the grandstands' reading of the events. The animal side of an item is
//! `performer`, its calls and the events it reads, and `crate::goal::start_trick`, the trick it
//! starts; the cycle's validation of an item is `cycle/validate.rs`. The marine specialist is `keeper`. The audience is
//! `audience`: the search of a guest for a show and the price of a seat.

use crate::behaviour::Data;
use crate::building::kind_of;
use crate::building::leave::push_out;
use crate::config::{Config, to_float, to_int};
use crate::entity::Catalogue;
use crate::exhibit::viewing::{self, Ground};
use crate::place::bulldozer::type_of;
use crate::sim::{Event, Events, Rng, SHOW_CHANNEL};
use crate::unit::{Part, Unit};
use crate::vfs::Vfs;
use crate::world::World;
use crate::zoo::{Item, ShowInfo};

pub mod admission;
mod audience;
mod cycle;
pub(crate) mod edit;
mod keeper;
mod performer;
pub(crate) mod portals;
mod relay;
pub(crate) mod toy;
pub mod trick;
mod units;

pub(crate) use audience::{find, pay};
pub(crate) use keeper::{cue, needs_keeper, rejoin, removed};
pub(crate) use portals::{advance as advance_portals, open_portals, portal};
pub(crate) use relay::{listen_grandstand, listen_world};
pub(crate) use units::{Move, counted, join, leave, settle};

/// The type id of the marine specialist, who conducts a show (shows 3.2), and the job he does
/// while it lasts (shows 22.1).
pub(crate) const KEEPER: u32 = 9552;
pub(crate) const CONDUCT: u8 = 19;

/// The event types of a show (shows 6.1).
pub(crate) mod event {
    pub(crate) const ANNOUNCE: u16 = 0x2711;
    pub(crate) const START_NOTE: u16 = 0x2712;
    pub(crate) const STARTED: u16 = 0x2713;
    pub(crate) const STOP: u16 = 0x2714;
    pub(crate) const ENDED: u16 = 0x2715;
    pub(crate) const ABORT: u16 = 0x2716;
    pub(crate) const ABORTED: u16 = 0x2717;
    /// Relayed by a grandstand like an end, but never posted by the world manager (shows 6.3).
    pub(crate) const OVER: u16 = 0x2719;
    pub(crate) const NEXT_ITEM: u16 = 0x271d;
    /// An animal reports its trick done, or failed (shows 24).
    pub(crate) const FINISHED: u16 = 0x271e;
    pub(crate) const TRICK_DONE: u16 = 0x271f;
    pub(crate) const FAILED: u16 = 0x2722;
    pub(crate) const CALL: u16 = 0x271a;
    pub(crate) const ARRIVED: u16 = 0x271b;
    pub(crate) const ITEM_ABORTED: u16 = 0x2723;
    pub(crate) const PREPARE: u16 = 0x2724;
    pub(crate) const CUE: u16 = 0x2725;
    pub(crate) const BAD: u16 = 0x272a;
    pub(crate) const NEUTRAL: u16 = 0x272b;
    pub(crate) const GOOD: u16 = 0x272c;
    pub(crate) const GREAT: u16 = 0x272d;
    pub(crate) const CHEER: u16 = 0x2735;
    pub(crate) const BOO: u16 = 0x2736;
}

/// The numbers of `shows.cfg` that the cycle reads (shows 3.1). The built-in values hold without
/// the file, which only Marine Mania has.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Params {
    /// The satisfaction of a trick at or below which it is bad, at or above which it is good and great.
    pub trick: [i32; 3],
    /// The tricks a show should have, at least and at most.
    pub ideal: [i32; 2],
    /// The most tricks the show panel's editor lets a show hold (`[show] maxLength`, 0: no limit;
    /// information panels 19.9.6).
    pub length: i32,
    /// The percent of a show at or below which it is bad, at or above which it is good and great.
    pub show: [i32; 3],
    /// The complexity a default script adds up to before it offers the food trick (shows 19.2).
    pub threshold: i32,
    /// The admission a type record takes when a script gives it its first (`defaultShowAdmission`).
    pub admission: f32,
    /// The show panel's step and bounds of the admission (`showAdmissionIncrement`,
    /// `maxShowAdmission`, `minShowAdmission`; shows 33.4).
    pub increment: f32,
    pub max_admission: f32,
    pub min_admission: f32,
    /// The show panel's frequencies: the AI ticks between two shows for Infrequent, Frequent and
    /// Continuous (`[timing]` of `shows.cfg`; shows 33.12).
    pub intervals: [i32; 3],
    /// `[AI] maxDistToGoToShow` of the ini file: how far a guest goes to a show (shows 7.1).
    pub distance: i32,
}

impl Default for Params {
    fn default() -> Params {
        Params {
            trick: [0, 3, 6],
            ideal: [6, 6],
            length: 0,
            show: [25, 50, 75],
            threshold: 5,
            admission: 0.0,
            increment: 0.25,
            max_admission: 10.0,
            min_admission: 0.0,
            intervals: [120, 90, 65],
            distance: 150,
        }
    }
}

impl Params {
    /// The `shows.cfg` of the game, or the built-in numbers without one (or one that cannot be read).
    pub fn load(vfs: &Vfs) -> Params {
        let config = vfs.read("shows.cfg").ok();
        config
            .and_then(|bytes| Config::parse(&bytes).ok())
            .map_or_else(Params::default, |c| Params::read(&c))
    }

    /// Reads a `shows.cfg`; a key that is missing keeps the built-in number.
    pub(crate) fn read(config: &Config) -> Params {
        let number = |section: &str, key: &str, fallback: i32| {
            let value = config.section(section).and_then(|s| s.get(key));
            value.and_then(to_int).unwrap_or(fallback)
        };
        let found = Params::default();
        let (trick, ideal, show) = (
            ["badTrick", "goodTrick", "greatTrick"],
            ["minIdealLength", "maxIdealLength"],
            ["badShow", "goodShow", "greatShow"],
        );
        let cost = |key: &str, fallback: f32| {
            let value = config.section("cost").and_then(|s| s.get(key));
            value.map_or(fallback, to_float)
        };
        Params {
            trick: std::array::from_fn(|i| {
                number("trickSatisfactionThresholds", trick[i], found.trick[i])
            }),
            ideal: std::array::from_fn(|i| number("show", ideal[i], found.ideal[i])),
            length: number("show", "maxLength", found.length),
            show: std::array::from_fn(|i| {
                number("showSatisfactionThresholds", show[i], found.show[i])
            }),
            threshold: number("behavior", "returnToKeeperThreshold", found.threshold),
            admission: cost("defaultShowAdmission", found.admission),
            increment: cost("showAdmissionIncrement", found.increment),
            max_admission: cost("maxShowAdmission", found.max_admission),
            min_admission: cost("minShowAdmission", found.min_admission),
            intervals: std::array::from_fn(|i| {
                let keys = [
                    "infrequentShowInterval",
                    "frequentShowInterval",
                    "continuousShowInterval",
                ];
                number("timing", keys[i], found.intervals[i])
            }),
            distance: found.distance,
        }
    }
}

/// The direction a rotation faces, as a step in x and y; an odd rotation faces no way.
fn step(rotation: u32) -> Option<(i32, i32)> {
    match rotation % 8 {
        0 => Some((0, -1)),
        2 => Some((1, 0)),
        4 => Some((0, 1)),
        6 => Some((-1, 0)),
        _ => None,
    }
}

/// The show tanks of a world, in the exhibit manager's order: the exhibit and the show id of each
/// show info with a show id (shows 4.1).
fn show_tanks(world: &World) -> Vec<(u32, u16)> {
    (world.exhibits().list().iter())
        .filter_map(|x| Some((x.id, info_of(world, x.id)?.id)))
        .filter(|&(_, id)| id != 0)
        .collect()
}

/// The show info of the habitat record whose first tile lies in exhibit `id`.
pub(crate) fn info_of(world: &World, id: u32) -> Option<&ShowInfo> {
    world.zoo.shows.info(info_slot(world, id)?)
}

/// The position in the shows section of the show info of exhibit `id`, if it has one.
pub(crate) fn info_slot(world: &World, id: u32) -> Option<usize> {
    world.info_slot_in(world.exhibits(), id)
}

/// The show tank that grandstand `stand` faces (shows 4.1), if it faces one.
pub(crate) fn faced_exhibit(world: &World, cat: &Catalogue, stand: u32) -> Option<u32> {
    let sight = Sight::of(world, cat);
    let &(_, at) = sight.stands.iter().find(|&&(id, _)| id == stand)?;
    match sight.seen(world, at) {
        0 => None,
        show => exhibit_of(world, show),
    }
}

/// The first item with trick id `trick` in the current script of the show info `show` (shows 23.2).
pub(crate) fn trick_item(world: &World, show: u16, trick: u16) -> Option<Item> {
    let info = (world.zoo.shows.infos.iter().flatten()).find(|i| i.id == show)?;
    let script = (world.zoo.shows.scripts.iter()).find(|s| s.id == info.show.script)?;
    script.items.iter().find(|i| i.id == trick).cloned()
}

/// The exhibit of habitat record `n`, from its first tile.
fn tank_of(world: &World, n: usize) -> Option<u32> {
    let &(x, y) = world.zoo.habitat_tiles.get(n)?;
    world
        .exhibits()
        .region(world.tile_index(x, y)?)
        .filter(|&r| world.exhibits().get(r).is_some())
}

/// The habitat record of the show info whose show id is `id`.
fn habitat_of(world: &World, id: u16) -> Option<usize> {
    (world.zoo.shows.infos.iter()).position(|i| i.as_ref().is_some_and(|i| i.id == id))
}

/// The show info whose show id is `id`.
pub(crate) fn info_by_id(world: &World, id: u16) -> Option<&ShowInfo> {
    world.zoo.shows.info(habitat_of(world, id)?)
}

/// The exhibit of the show info whose show id is `id`.
pub(crate) fn exhibit_of(world: &World, id: u16) -> Option<u32> {
    tank_of(world, habitat_of(world, id)?)
}

/// Whether `unit` is in a show (shows 18.2): it is an animal that performs, and it is on its way
/// home or was started in a show whose info holds an item state for it (`held` tells that of the
/// show id it is given). `at` is the unit's place in the entity list; only an animal looks at its
/// type (shows 32.5 point 3).
pub(crate) fn in_show(
    world: &World,
    cat: &Catalogue,
    unit: &Unit,
    at: usize,
    held: impl FnOnce(u16) -> bool,
) -> bool {
    // The answer needs all three; the cheap tests come first, the type's trick list last.
    let animal = matches!(unit.part, Part::Animal(_));
    let ty = || type_of(cat, world.zoo.entities.get(at)?);
    animal
        && (unit.back || (unit.show != 0 && held(unit.show)))
        && ty().is_some_and(trick::performs)
}

/// [`in_show`] when no show info is out of the world: computed at each reader, nothing keeps a
/// copy (shows 32.3 point 1).
pub(crate) fn in_show_now(world: &World, cat: &Catalogue, unit: &Unit, at: usize) -> bool {
    let held = |show: u16| {
        let infos = world.zoo.shows.infos.iter().flatten();
        let mut infos = infos.filter(|i| i.id == show);
        infos.any(|i| i.show.items.iter().any(|s| s.animal == unit.id))
    };
    in_show(world, cat, unit, at, held)
}

/// `abort_show` (shows 21.6): the show of the show id `id` is aborted when it has an info. The
/// events it posts go out with the next step's.
pub(crate) fn abort_show(world: &mut World, cat: &Catalogue, id: u16) {
    let mut events = Events::default();
    cycle::abort(world, cat, &mut events, id);
    world.units.posts.extend(events.take_posted());
}

/// The frequency of show `id` set from the show panel (shows 33.12): nothing when it is the
/// info's; else the schedule is recalculated, and for -1 the show is aborted when it is not
/// started. The events it posts go out with the next step's.
pub(crate) fn set_frequency(world: &mut World, cat: &Catalogue, id: u16, frequency: i32) {
    let mut events = Events::default();
    cycle::set_frequency(world, cat, &mut events, id, frequency);
    world.units.posts.extend(events.take_posted());
}

/// [`abort_show`] for `unit`, which is out of the world's table for its turn: it goes back in
/// while the show lets go of its animals, itself among them.
pub(crate) fn abort_show_for(world: &mut World, cat: &Catalogue, unit: &mut Unit, id: u16) {
    let at = unit.id;
    let Some(slot) = world.units.get_mut(at) else {
        return;
    };
    std::mem::swap(unit, slot);
    abort_show(world, cat, id);
    if let Some(slot) = world.units.get_mut(at) {
        std::mem::swap(unit, slot);
    }
}

/// [`empty_grandstands`] with the events it posts going out with the next step's, as
/// [`abort_show`] does.
pub(crate) fn empty_grandstands_of(world: &mut World, cat: &Catalogue, id: u32) {
    let mut events = Events::default();
    empty_grandstands(world, cat, &mut events, id);
    world.units.posts.extend(events.take_posted());
}

/// [`empty_grandstands_of`] for `unit`, which is out of the world's table for its turn: it goes
/// back in while the audience is put out, itself among it when it is seated.
pub(crate) fn empty_grandstands_for(world: &mut World, cat: &Catalogue, unit: &mut Unit, id: u32) {
    let at = unit.id;
    let Some(slot) = world.units.get_mut(at) else {
        return;
    };
    std::mem::swap(unit, slot);
    empty_grandstands_of(world, cat, id);
    if let Some(slot) = world.units.get_mut(at) {
        std::mem::swap(unit, slot);
    }
}

/// The drain of tank `exhibit` (shows 30.4 points 3 and 5) posts the abort event, whatever the
/// tank was: its show info reads it at the next step; a tank with no info posts it for show 0,
/// which no info reads.
pub(crate) fn drained(world: &mut World, exhibit: u32) {
    post_abort(world, info_of(world, exhibit).map_or(0, |i| i.id));
}

/// The exhibit of show `show` is gone and held `tiles` (shows 30.4 point 1): the abort event is
/// posted for every removal, for show 0 when the exhibit had no info; a show's info is already
/// deleted when it comes, so nobody reads it (O22). The grandstands that faced the exhibit lose
/// their users (none for a removal with no info). The show is not aborted.
pub(crate) fn exhibit_gone(world: &mut World, cat: &Catalogue, show: u16, tiles: &[usize]) {
    post_abort(world, show);
    empty_facing(world, cat, &|t| tiles.contains(&t));
}

fn post_abort(world: &mut World, show: u16) {
    let id = u32::from(show);
    world.units.posts.push(Event {
        sender: id,
        target: id,
        data: 0,
        extra: 0,
        kind: event::ABORT,
        delay: 1,
        sender_channel: SHOW_CHANNEL,
        channel: SHOW_CHANNEL,
    });
}

/// The grandstands of a world in the order of the `grandstand` need list (shows 9.2: appended at
/// each add, so a moved one goes last), as (id, position in the entity list).
fn grandstands(world: &World, cat: &Catalogue) -> Vec<(u32, usize)> {
    let default = world.units.prices.default;
    let mut stands: Vec<_> = (world.zoo.entities.iter().enumerate())
        .filter(|(_, e)| kind_of(cat, e, default).is_some_and(|k| k.satisfies("grandstand")))
        .filter_map(|(at, e)| Some((e.common.as_ref()?.id, at)))
        .collect();
    world.in_need_order(&mut stands, |s| s.0);
    stands
}

/// What the show of a grandstand is read from (shows 4.1): the show tanks, the grandstands and the
/// ground, made once for a number of questions.
struct Sight {
    tanks: Vec<(u32, u16)>,
    stands: Vec<(u32, usize)>,
    ground: Option<Ground>,
}

impl Sight {
    fn of(world: &World, cat: &Catalogue) -> Sight {
        let (tanks, stands) = (show_tanks(world), grandstands(world, cat));
        let ground = (!tanks.is_empty() && !stands.is_empty())
            .then(|| Ground::of(world, viewing::bodies(cat)));
        Sight {
            tanks,
            stands,
            ground,
        }
    }

    /// `show_seen_from` of grandstand `at`: the show id of the first show tank it faces, 0 for none.
    fn seen(&self, world: &World, at: usize) -> u16 {
        let Some(ground) = &self.ground else { return 0 };
        (self.tanks.iter())
            .find(|&&(x, _)| faces(world, ground, x, at))
            .map_or(0, |&(_, show)| show)
    }
}

/// Whether grandstand `at` faces exhibit `exhibit` (shows 4.1).
fn faces(world: &World, ground: &Ground, exhibit: u32, at: usize) -> bool {
    faces_in(
        world,
        ground,
        &|t| world.exhibits().region(t) == Some(exhibit),
        at,
    )
}

/// [`faces`] for an exhibit, `inside` telling the tiles it holds: the
/// tile four tiles in front of it is in the exhibit and the line of sight from three to five tiles
/// in front is clear. A tile off
/// the map is a no (the original reads a null tile there).
fn faces_in(world: &World, ground: &Ground, inside: &dyn Fn(usize) -> bool, at: usize) -> bool {
    let Some(c) = world.zoo.entities[at].common.as_ref() else {
        return false;
    };
    let Some((dx, dy)) = step(c.rotation) else {
        return false;
    };
    let (x, y) = (c.x.div_euclid(64), c.y.div_euclid(64));
    let ahead = |n: i32| world.tile_index(x + n * dx, y + n * dy);
    let (Some(t3), Some(t4), Some(t5)) = (ahead(3), ahead(4), ahead(5)) else {
        return false;
    };
    inside(t4) && ground.sees(t3, t5)
}

/// The pass for the building `id` just added to the map (placed, set down, put back): it runs only
/// when the building is a grandstand (shows 4.2); any other building runs none.
pub(crate) fn added(world: &mut World, cat: &Catalogue, id: u32) {
    world.note_added(id);
    let default = world.units.prices.default;
    let stand = world.find(id).is_some_and(|at| {
        kind_of(cat, &world.zoo.entities[at], default).is_some_and(|k| k.satisfies("grandstand"))
    });
    if stand {
        associate(world, cat);
    }
}

/// The pass of a zoo being loaded, for the entity `at` just read (shows 4.2): it runs only when
/// that entity is a grandstand, and sees only the entities read so far.
pub(crate) fn loaded(world: &mut World, cat: &Catalogue, at: usize) {
    let e = &world.zoo.entities[at];
    if let (true, Some(id)) = (e.class == "building", e.common.as_ref().map(|c| c.id)) {
        world.note_added(id);
    }
    let default = world.units.prices.default;
    if kind_of(cat, &world.zoo.entities[at], default).is_some_and(|k| k.satisfies("grandstand")) {
        associate_upto(world, cat, at);
    }
}

/// The association pass (shows 4.2): every grandstand that faces a show tank takes the show id of the
/// first one it faces. A grandstand that faces none keeps the id it has. Run once after a zoo is
/// loaded and when a grandstand is added to the map.
pub(crate) fn associate(world: &mut World, cat: &Catalogue) {
    associate_upto(world, cat, usize::MAX);
}

/// The pass over the entities up to index `last` only.
fn associate_upto(world: &mut World, cat: &Catalogue, last: usize) {
    let tanks = show_tanks(world);
    let stands: Vec<_> = grandstands(world, cat)
        .into_iter()
        .filter(|&(_, at)| at <= last)
        .collect();
    if tanks.is_empty() || stands.is_empty() {
        return;
    }
    let ground = Ground::until(world, viewing::bodies(cat), last);
    for (id, at) in stands {
        let Some(&(_, show)) = tanks.iter().find(|&&(x, _)| faces(world, &ground, x, at)) else {
            continue;
        };
        if let Some(live) = world.units.buildings.get_mut(&id) {
            live.show_id = show;
        }
    }
}

/// The show manager's step (shows 5.1), first in the AI manager's update, at AI tick `now`: every
/// show info in show id order listens, updates and refreshes its upkeep figure. Nothing happens
/// in a world without a show info.
pub(crate) fn update(
    world: &mut World,
    cat: &Catalogue,
    (events, rng, data): (&mut Events, &mut Rng, &dyn Data),
    now: u32,
) {
    let mut infos: Vec<(u16, usize)> = (world.zoo.shows.infos.iter().enumerate())
        .filter_map(|(n, i)| Some((i.as_ref()?.id, n)))
        .collect();
    if infos.is_empty() {
        return;
    }
    infos.sort_by_key(|&(id, _)| id);
    let upkeep = upkeeps(world, cat);
    for (id, n) in infos {
        let turn = (&mut *events, &mut *rng, data);
        cycle::update(world, cat, turn, now, n, grandstand_upkeep(&upkeep, id));
    }
}

/// The upkeep of each grandstand in list order, with the show it faces now (shows 9.2).
fn upkeeps(world: &World, cat: &Catalogue) -> Vec<(u16, f32)> {
    let sight = Sight::of(world, cat);
    (sight.stands.iter())
        .map(|&(id, at)| {
            let cost = crate::place::bulldozer::type_of(cat, &world.zoo.entities[at])
                .map_or(0.0, |ty| crate::month::upkeep_of(&world.units, ty, id));
            (sight.seen(world, at), cost)
        })
        .collect()
}

/// `grandstand_upkeep(id)` (shows 9.2): the sum in single precision of the upkeeps facing show `id`.
fn grandstand_upkeep(upkeep: &[(u16, f32)], id: u16) -> f32 {
    (upkeep.iter().filter(|u| u.0 == id)).fold(0.0, |a, u| a + u.1)
}

/// The month change of the show infos (shows 9.3), after the exhibits' and before the wages: the
/// figures of the month that ended move to last month and the upkeep takes the current rate.
pub(crate) fn new_month(world: &mut World, cat: &Catalogue) {
    if world.zoo.shows.infos.iter().all(Option::is_none) {
        return;
    }
    let upkeep = upkeeps(world, cat);
    for info in world.zoo.shows.infos.iter_mut().flatten() {
        info.new_month(grandstand_upkeep(&upkeep, info.id));
    }
}

/// `empty_grandstands` (shows 30.3 point 1): the grandstand `id` is emptied when it belongs to a
/// show whose info exists. The show is aborted first (shows 5.6, any state), the grandstand's
/// "show soon" flag goes down and the grandstands that face the show's tank lose their users.
pub(crate) fn empty_grandstands(world: &mut World, cat: &Catalogue, events: &mut Events, id: u32) {
    if !is_grandstand(world, cat, id) {
        return;
    }
    let Some(show) = world.units.buildings.get(&id).map(|l| l.show_id) else {
        return;
    };
    if show == 0 || !has_info(world, show) {
        return;
    }
    cycle::abort(world, cat, events, show);
    if let Some(live) = world.units.buildings.get_mut(&id) {
        live.fill = 0;
    }
    empty_show_grandstands(world, cat, show);
}

/// Whether the entity `id` is a grandstand.
pub(crate) fn is_grandstand(world: &World, cat: &Catalogue, id: u32) -> bool {
    let default = world.units.prices.default;
    (world.find(id)).is_some_and(|at| {
        kind_of(cat, &world.zoo.entities[at], default).is_some_and(|k| k.satisfies("grandstand"))
    })
}

/// Whether a show info with this show id exists.
fn has_info(world: &World, id: u16) -> bool {
    world.zoo.shows.infos.iter().flatten().any(|i| i.id == id)
}

/// Shows 30.3 point 1: every grandstand that faces the tank of show `id` loses all its users at
/// once (a forced leave) and its "show soon" flag goes down. Nothing happens for a show with no
/// info or whose tank is gone.
pub(crate) fn empty_show_grandstands(world: &mut World, cat: &Catalogue, id: u16) {
    let Some(exhibit) = exhibit_of(world, id) else {
        return;
    };
    let tiles = world.tiles_of(exhibit);
    empty_facing(world, cat, &|t| tiles.contains(&t));
}

/// Every grandstand that faces the exhibit whose tiles `inside` tells loses its users.
fn empty_facing(world: &mut World, cat: &Catalogue, inside: &dyn Fn(usize) -> bool) {
    let stands = grandstands(world, cat);
    if stands.is_empty() {
        return;
    }
    let ground = Ground::of(world, viewing::bodies(cat));
    let facing: Vec<u32> = (stands.iter())
        .filter(|&&(_, at)| faces_in(world, &ground, inside, at))
        .map(|&(id, _)| id)
        .collect();
    for id in facing {
        push_out(world, cat, id);
        if let Some(live) = world.units.buildings.get_mut(&id) {
            live.fill = 0;
        }
    }
}

#[cfg(test)]
pub(crate) mod tests;
