//! The live state of animals, guests and staff, and the update every unit runs once per step
//! (simulation 6, behaviours 4.6, movement 3).
//!
//! The state is a side table of the world ([`Units`], `World::units`) with one [`Unit`] per
//! animal, guest, keeper, maintenance worker and tour guide of the entity list, in list order.
//! The position and the facing (`rotation`) stay in the entity's common part. The table follows the
//! entity list: a unit that is added or removed (a placement, an undo, the removal pass) is added
//! or dropped when the world re-indexes, and a new one starts its status timer at `id % period`.
//!
//! [`update`] is one unit's turn in the original's order: events in; unless suspended, the move;
//! the animation; unless suspended, the status timer (the needs and the checks once it passes the
//! period) and then either the wait counted down or the behaviour and the goals; the events
//! discarded. The move is [`movement`]. What the animation frames,
//! the needs, the checks, the behaviour and the goals do is the business of later issues: each is
//! a method of [`Hooks`], empty until then.
//! A hook must not add or remove entities (a removal goes through [`Ctx::remove`], which only
//! flags the unit for the next removal pass, as in the original).
//!
//! The log family `unit`: `unit add <id> <kind> <x>,<y>`, `unit remove <id>`,
//! `unit status <id>` for each status tick, `unit tile <id> <x>,<y>` when a unit enters a tile,
//! `unit enter <id> <exhibit>` when that is another exhibit, `unit face <id> <direction>` when its
//! facing turns and `unit abort <id>` when its next tile step is refused.

use std::collections::HashMap;

pub use crate::animal::Animal;
use crate::animal::Class;
use crate::config::Config;
use crate::entity::Catalogue;
use crate::path::{self, Finder, Mover, Settings};
use crate::sim::{Event, Events, KEEPER_CHANNEL, Rng, UNIT_CHANNEL};
use crate::world::World;
use crate::zoo::Entity;

pub mod movement;
#[cfg(test)]
pub(crate) mod testing;
#[cfg(test)]
mod tests;

/// What a unit is.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Kind {
    #[default]
    Animal,
    Guest,
    Keeper,
    Worker,
    Guide,
}

impl Kind {
    /// The kind of the units of an entity class; `None` for everything that is not a unit (and
    /// for the helicopter, which comes with the scenarios).
    pub fn of_class(class: &str) -> Option<Kind> {
        Some(match class {
            "animals" => Kind::Animal,
            "guests" => Kind::Guest,
            "keeper" => Kind::Keeper,
            "maint" => Kind::Worker,
            "tour" => Kind::Guide,
            _ => return None,
        })
    }

    /// The word the log uses.
    pub fn name(self) -> &'static str {
        match self {
            Kind::Animal => "animal",
            Kind::Guest => "guest",
            Kind::Keeper => "keeper",
            Kind::Worker => "worker",
            Kind::Guide => "guide",
        }
    }

    /// The class that decides the costs of the path finder.
    fn mover(self) -> path::Kind {
        match self {
            Kind::Animal => path::Kind::Animal,
            Kind::Guest => path::Kind::Guest,
            Kind::Keeper => path::Kind::Keeper,
            Kind::Worker => path::Kind::Worker,
            Kind::Guide => path::Kind::Guide,
        }
    }

    /// The event channel the kind listens on (simulation 5.2).
    fn channel(self) -> u8 {
        match self {
            Kind::Keeper => KEEPER_CHANNEL,
            _ => UNIT_CHANNEL,
        }
    }
}

/// The part of the state of a guest: its needs, items and lists (guest issues).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Guest {}

/// The part of the state of a staff member: its exhibits and job (staff issues).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Staff {
    /// The counters drawn when the first turn starts the state (staff 4.2): -1 for a key of 0.
    pub work: i32,
    pub chase: i32,
}

/// The kind-specific part of a unit.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Part {
    Animal(Animal),
    Guest(Guest),
    Staff(Staff),
}

/// The animation a unit shows: the behaviour calls set the name and restart it; the frame comes
/// from the elapsed time and the animation's own frame times (animation issue).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Animation {
    pub name: String,
    pub frame: u32,
    /// Milliseconds the animation has run.
    pub elapsed: u32,
}

/// The live state of one unit.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Unit {
    pub id: u32,
    pub kind: Kind,
    pub part: Part,
    /// Whether the unit is moving, and at which speed in movement units per second.
    pub moving: bool,
    pub speed: u32,
    /// The percentage that scales the step of the animation while the unit moves.
    pub anim_speed: u16,
    /// The point the unit goes to, in movement units; `None` for the original's invalid position.
    pub destination: Option<(i32, i32, i32)>,
    /// The tiles to follow, from the one the unit stands on to its destination (movement 4.1).
    pub path: Vec<usize>,
    /// The direction of the last tile change, -1 for none (movement 2).
    pub travel: i32,
    /// The unit goes in or out of a building: the tile step test looks at the edge cost only.
    pub through_building: bool,
    /// The next tile step was refused: the goal has to give up (movement 4.7). The goals read
    /// and clear it.
    pub aborted: bool,
    /// What the costs of the path finder read of the unit's type; its tile is set for each use.
    pub mover: Mover,
    pub animation: Animation,
    /// Milliseconds since the last status tick, started at `id % period`.
    pub status_timer: i32,
    /// While above 0 the unit runs no behaviour and no goal.
    pub wait: i32,
    /// The events read at the start of this step, for the hooks; empty between steps.
    pub events: Vec<Event>,
    /// The player holds the unit.
    pub carried: bool,
    /// The program is moving the unit by code (it is never set while a step runs).
    pub repositioning: bool,
    /// The unit has had its first turn, in which the kinds start their state.
    pub started: bool,
}

impl Unit {
    /// A new unit, as the original's initialisation leaves it (simulation 8.2); its timer is set
    /// by the table.
    pub fn new(id: u32, kind: Kind) -> Unit {
        Unit {
            id,
            kind,
            part: match kind {
                Kind::Animal => Part::Animal(Animal::default()),
                Kind::Guest => Part::Guest(Guest {}),
                _ => Part::Staff(Staff::default()),
            },
            moving: false,
            speed: 0,
            anim_speed: 100,
            destination: None,
            path: Vec::new(),
            travel: -1,
            through_building: false,
            aborted: false,
            mover: Mover::new(kind.mover(), None),
            animation: Animation::default(),
            status_timer: 0,
            wait: 0,
            events: Vec::new(),
            carried: false,
            repositioning: false,
            started: false,
        }
    }

    /// Whether a flag keeps the unit from moving, from its status tick and from its goals.
    pub fn suspended(&self) -> bool {
        self.carried || self.repositioning
    }
}

/// The status timer of a new unit (simulation 6.3).
fn first_timer(id: u32, period: i32) -> i32 {
    match period < 1 {
        true => 0,
        false => (id % period as u32) as i32,
    }
}

/// The table of the units of a world.
#[derive(Debug)]
pub struct Units {
    /// In entity list order.
    list: Vec<Unit>,
    /// The position in `list` of each id; only looked up, never walked.
    index: HashMap<u32, usize>,
    /// The status period (`[AI] mStatusPeriod`) new units start their timers from.
    period: i32,
    /// The path finder with its settings and its list of unreachable places.
    pub finder: Finder,
    /// The keys of `animals.ai` that every animal reads.
    pub class: Class,
    lines: Vec<String>,
    animal_lines: Vec<String>,
    /// The messages posted and not shown yet: the string number and the unit it is about.
    messages: Vec<(i32, u32)>,
}

impl Default for Units {
    fn default() -> Self {
        Units {
            list: Vec::new(),
            index: HashMap::new(),
            period: 1000,
            finder: Finder::default(),
            class: Class::default(),
            lines: Vec::new(),
            animal_lines: Vec::new(),
            messages: Vec::new(),
        }
    }
}

impl Units {
    pub fn len(&self) -> usize {
        self.list.len()
    }

    pub fn is_empty(&self) -> bool {
        self.list.is_empty()
    }

    pub fn iter(&self) -> impl Iterator<Item = &Unit> {
        self.list.iter()
    }

    pub fn get(&self, id: u32) -> Option<&Unit> {
        self.index.get(&id).map(|&i| &self.list[i])
    }

    pub fn get_mut(&mut self, id: u32) -> Option<&mut Unit> {
        let i = *self.index.get(&id)?;
        Some(&mut self.list[i])
    }

    /// Sets the status period and starts every unit's timer again at `id % period`, as at a load.
    pub fn set_period(&mut self, period: i32) {
        self.period = period;
        for unit in &mut self.list {
            unit.status_timer = first_timer(unit.id, period);
        }
    }

    /// Reads the settings of the path finder from the ini file.
    pub fn set_ini(&mut self, ini: &[u8]) {
        let config = Config::parse(ini).ok();
        self.finder = Finder::new(
            config
                .as_ref()
                .map_or_else(Settings::default, Settings::from_ini),
        );
    }

    /// The `unit` log lines since the last call, without the family word.
    pub fn take_lines(&mut self) -> Vec<String> {
        std::mem::take(&mut self.lines)
    }

    /// The `animal` log lines since the last call, without the family word.
    pub fn take_animal_lines(&mut self) -> Vec<String> {
        std::mem::take(&mut self.animal_lines)
    }

    /// The messages posted since the last call, as (string number, unit id); the string holds a
    /// `%s` for the unit's name.
    pub fn take_messages(&mut self) -> Vec<(i32, u32)> {
        std::mem::take(&mut self.messages)
    }

    pub(crate) fn tell(&mut self, line: String) {
        self.animal_lines.push(line);
    }

    pub(crate) fn post(&mut self, message: i32, id: u32) {
        self.messages.push((message, id));
    }

    /// Makes the table follow the entity list: units that stand in it keep their state, new ones
    /// start, those that left are dropped. An entity whose id an earlier one has is not a unit.
    pub(crate) fn sync(&mut self, entities: &[Entity]) {
        let before: Vec<u32> = self.list.iter().map(|u| u.id).collect();
        let mut old: HashMap<u32, Unit> = std::mem::take(&mut self.list)
            .into_iter()
            .map(|u| (u.id, u))
            .collect();
        self.index.clear();
        for e in entities {
            let (Some(kind), Some(c)) = (Kind::of_class(&e.class), e.common.as_ref()) else {
                continue;
            };
            if self.index.contains_key(&c.id) {
                continue;
            }
            let unit = old
                .remove(&c.id)
                .filter(|u| u.kind == kind)
                .unwrap_or_else(|| {
                    self.lines.push(format!(
                        "add {} {} {},{}",
                        c.id,
                        kind.name(),
                        c.x.div_euclid(64),
                        c.y.div_euclid(64)
                    ));
                    let mut unit = Unit::new(c.id, kind);
                    unit.status_timer = first_timer(c.id, self.period);
                    unit
                });
            self.index.insert(c.id, self.list.len());
            self.list.push(unit);
        }
        for id in before {
            if !self.index.contains_key(&id) {
                self.lines.push(format!("remove {id}"));
            }
        }
    }

    /// A hash of the whole table (FNV-1a over the fields, in list order) for the state hash of a
    /// run.
    pub fn hash(&self) -> u64 {
        let mut h = Fnv(0xcbf2_9ce4_8422_2325);
        h.word(self.period as u32);
        for u in &self.list {
            h.word(u.id);
            h.word(u.kind as u32);
            h.word(u32::from(u.moving));
            h.word(u.speed);
            h.word(u32::from(u.anim_speed));
            for v in u.destination.iter().flat_map(|&(x, y, z)| [x, y, z]) {
                h.word(v as u32);
            }
            h.word(u32::from(u.destination.is_some()));
            h.word(u.path.len() as u32);
            for &t in &u.path {
                h.word(t as u32);
            }
            h.word(u.travel as u32);
            h.word(u32::from(u.through_building) | u32::from(u.aborted) << 1);
            h.bytes(u.animation.name.as_bytes());
            h.word(u.animation.frame);
            h.word(u.animation.elapsed);
            h.word(u.status_timer as u32);
            h.word(u.wait as u32);
            h.word(
                u32::from(u.carried) | u32::from(u.repositioning) << 1 | u32::from(u.started) << 2,
            );
            match &u.part {
                Part::Animal(a) => h.bytes(format!("{a:?}").as_bytes()),
                Part::Staff(s) => {
                    h.word(s.work as u32);
                    h.word(s.chase as u32);
                }
                Part::Guest(_) => {}
            }
            h.word(u.events.len() as u32);
            for e in &u.events {
                for v in [e.sender, e.target, e.data] {
                    h.word(v);
                }
                for v in [
                    e.extra,
                    e.kind,
                    e.delay,
                    e.sender_channel.into(),
                    e.channel.into(),
                ] {
                    h.word(v.into());
                }
            }
        }
        h.0
    }
}

struct Fnv(u64);

impl Fnv {
    fn bytes(&mut self, bytes: &[u8]) {
        for &b in bytes {
            self.0 = (self.0 ^ u64::from(b)).wrapping_mul(0x0000_0100_0000_01b3);
        }
        self.bytes_end();
    }

    /// A separator, so that two names do not run into each other.
    fn bytes_end(&mut self) {
        self.0 = (self.0 ^ 0xff).wrapping_mul(0x0000_0100_0000_01b3);
    }

    fn word(&mut self, v: u32) {
        for b in v.to_le_bytes() {
            self.0 = (self.0 ^ u64::from(b)).wrapping_mul(0x0000_0100_0000_01b3);
        }
    }
}

/// What the managers hand to a unit's turn.
pub struct Ctx<'a> {
    pub rng: &'a mut Rng,
    pub events: &'a Events,
    /// The ids that asked to be removed at the next step's removal pass.
    pub removals: &'a mut Vec<u32>,
    /// The step in milliseconds.
    pub step: u32,
    /// The types of the game.
    pub cat: &'a Catalogue,
}

impl Ctx<'_> {
    /// The unit's `setIsRemoved`: it goes at the next removal pass, and the undo list forgets it
    /// at once (an undo must neither bring back nor remove a destroyed entity).
    pub fn remove(&mut self, world: &mut World, id: u32) {
        world.forget_undo(id);
        self.removals.push(id);
    }
}

/// The work of a unit's turn that later issues fill. Each gets the unit (taken out of the table
/// for the turn), the world, the managers' context and the unit's position in the entity list.
pub trait Hooks {
    /// The unit's first turn, before anything else (simulation 8.2: the creation of its state).
    fn start(&mut self, _: &mut Unit, _: &mut World, _: &mut Ctx, _at: usize) {}
    /// The move by the step (movement 3); not run while suspended.
    fn movement(&mut self, _: &mut Unit, _: &mut World, _: &mut Ctx, _at: usize) {}
    /// The status variables (the needs); at a status tick, not run while suspended.
    fn needs(&mut self, _: &mut Unit, _: &mut World, _: &mut Ctx, _at: usize) {}
    /// The conditional checks; at a status tick, right after the needs.
    fn checks(&mut self, _: &mut Unit, _: &mut World, _: &mut Ctx, _at: usize) {}
    /// The behaviour set (behaviours 4.6); not run while suspended or waiting.
    fn behaviour(&mut self, _: &mut Unit, _: &mut World, _: &mut Ctx, _at: usize) {}
    /// The goals (simulation 6.6); not run while suspended or waiting.
    fn goals(&mut self, _: &mut Unit, _: &mut World, _: &mut Ctx, _at: usize) {}
}

/// The hooks of the game: the move, and the common part of the goal update.
pub struct Standard;

impl Hooks for Standard {
    fn movement(&mut self, unit: &mut Unit, world: &mut World, ctx: &mut Ctx, at: usize) {
        movement::step(unit, world, ctx.step, at);
    }

    fn behaviour(&mut self, unit: &mut Unit, world: &mut World, _: &mut Ctx, at: usize) {
        // The move call of a behaviour set ends the walk here, after the move (movement 8.2).
        movement::finish(unit, world, at);
    }

    fn start(&mut self, unit: &mut Unit, world: &mut World, ctx: &mut Ctx, at: usize) {
        crate::animal::start(unit, world, ctx, at);
        crate::staff::start(unit, world, ctx, at);
    }
    fn needs(&mut self, unit: &mut Unit, world: &mut World, ctx: &mut Ctx, at: usize) {
        crate::animal::needs(unit, world, ctx, at);
    }
    fn checks(&mut self, unit: &mut Unit, world: &mut World, ctx: &mut Ctx, at: usize) {
        crate::animal::checks(unit, world, ctx, at);
    }
    fn goals(&mut self, unit: &mut Unit, world: &mut World, ctx: &mut Ctx, at: usize) {
        // The kind's own goal work comes first, then this, then the current goal's update and
        // the sub-goals.
        off_zoo(unit, world, ctx, at);
    }
}

/// The common part of every unit's goal update (simulation 6.6): one draw, and one time in 15 a
/// unit that stands outside the zoo is removed. (The unit standing on the footprint of a building
/// is sent out of it by the building issue.)
fn off_zoo(unit: &Unit, world: &mut World, ctx: &mut Ctx, at: usize) {
    if ctx.rng.pick(15) != 0 {
        return;
    }
    let inside = world.zoo.entities[at]
        .common
        .as_ref()
        .and_then(|c| world.tile_index(c.x.div_euclid(64), c.y.div_euclid(64)))
        .is_some_and(|t| world.in_zoo(t));
    if !inside {
        ctx.remove(world, unit.id);
    }
}

/// One turn of the unit at position `at` of the entity list (simulation 6.1); nothing for an
/// entity that is not a unit.
pub fn update(hooks: &mut impl Hooks, world: &mut World, ctx: &mut Ctx, at: usize) {
    let Some(id) = (world.zoo.entities.get(at)).and_then(|e| e.common.as_ref().map(|c| c.id))
    else {
        return;
    };
    let Some(slot) = world.units.get_mut(id) else {
        return;
    };
    // The unit leaves the table for its turn, so that the hooks can use the world freely.
    let mut unit = std::mem::replace(slot, Unit::new(id, slot.kind));
    turn(hooks, &mut unit, world, ctx, at);
    if let Some(slot) = world.units.get_mut(id) {
        *slot = unit;
    }
}

fn turn(hooks: &mut impl Hooks, unit: &mut Unit, world: &mut World, ctx: &mut Ctx, at: usize) {
    let step = ctx.step;
    if !unit.started {
        unit.started = true;
        hooks.start(unit, world, ctx, at);
    }
    unit.events = ctx
        .events
        .listen(unit.id, unit.kind.channel())
        .cloned()
        .collect();
    if !unit.suspended() {
        hooks.movement(unit, world, ctx, at);
    }
    // A moving unit's animation runs at its speed percentage; it runs even when suspended.
    let advance = match unit.moving {
        true => step.wrapping_mul(unit.anim_speed.into()) / 100,
        false => step,
    };
    unit.animation.elapsed = unit.animation.elapsed.wrapping_add(advance);
    if !unit.suspended() {
        let period = world.units.period;
        unit.status_timer = unit.status_timer.wrapping_add(step as i32);
        if unit.status_timer > period {
            unit.status_timer = if period < 1 {
                0
            } else {
                unit.status_timer % period
            };
            world.units.lines.push(format!("status {}", unit.id));
            hooks.needs(unit, world, ctx, at);
            hooks.checks(unit, world, ctx, at);
        }
        if unit.wait > 0 {
            unit.wait = unit.wait.wrapping_sub(step as i32);
        } else {
            hooks.behaviour(unit, world, ctx, at);
            hooks.goals(unit, world, ctx, at);
        }
    }
    unit.events.clear();
}
