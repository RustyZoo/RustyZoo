//! The live state of animals, guests and staff, and the update every unit runs once per step
//! (simulation 6, behaviours 4.6, movement 3).
//!
//! The state is a side table of the world ([`Units`], `World::units`) with one [`Unit`] per
//! animal, guest, keeper, maintenance worker, tour guide and helicopter of the entity list, in
//! list order.
//! The position and the facing (`rotation`) stay in the entity's common part. The table follows the
//! entity list: a unit that is added or removed (a placement, an undo, the removal pass) is added
//! or dropped when the world re-indexes, and a new one starts its status timer at `id % period`.
//!
//! [`update`] is one unit's turn in the original's order: events in; unless suspended, the move;
//! the animation; unless suspended, the status timer (the needs and the checks once it passes the
//! period) and then either the wait counted down or the behaviour and the goals; the events
//! discarded. The move is [`movement`] and the random walk [`walk`]. What the needs, the checks
//! and the goals do is the business of later issues: each is a method of [`Hooks`], empty until
//! then. The animation and the behaviour are [`Standard`]'s: the frames of the unit's animation
//! and the set it runs come from the data ([`crate::behaviour`]).
//!
//! A hook must not add or remove entities (a removal goes through [`Ctx::remove`], which only
//! flags the unit for the next removal pass, as in the original).
//!
//! The log family `unit`: `unit add <id> <kind> <x>,<y>`, `unit remove <id>`,
//! `unit status <id>` for each status tick, `unit tile <id> <x>,<y>` when a unit enters a tile
//! (and `unit depth <id> <z>`, its height in movement units, when it does in a tank),
//! `unit enter <id> <exhibit>` when that is another exhibit, `unit face <id> <direction>` when its
//! facing turns, `unit abort <id>` when its next tile step is refused, `unit gate <id> open|close
//! <gate>` when it opens the gate ahead or closes the one behind, `unit set <id> <name>` when
//! a unit starts a set and `unit anim <id> <name>` when the animation it shows changes.

use std::collections::HashMap;

pub(crate) use crate::animal::Animal;
use crate::animal::Class;
use crate::behaviour::{Animation, Heard, Runner, Target};
use crate::config::Config;
use crate::entity::{Catalogue, EntityType};
use crate::goal::Goals;
pub(crate) use crate::guest::Guest;
use crate::path::{self, Finder, Mover, Settings};
use crate::sim::{Event, Events, KEEPER_CHANNEL, Rng, UNIT_CHANNEL};
use crate::timing::ini_whole;
use crate::world::World;
use crate::zoo::Entity;

/// The colour choices of a new unit: `count` draws in a row, each `rand % palettes of the group`
/// (0 for a group the type does not have), as the creation draws them (save games 11.2).
pub(crate) fn draw_colours(ty: Option<&EntityType>, count: usize, rng: &mut Rng) -> Vec<u8> {
    let palettes = ty.map(EntityType::palettes).unwrap_or_default();
    (0..count)
        .map(|g| {
            let draw = rng.rand();
            palettes
                .get(g)
                .filter(|&&n| n > 0)
                .map_or(0, |&n| u8::try_from(draw as usize % n).unwrap_or(0))
        })
        .collect()
}

pub mod gate;
mod hash;
mod hooks;
pub mod movement;
pub(crate) mod own;
pub mod predator;
pub mod resume;
pub mod swim;
#[cfg(test)]
pub(crate) mod testing;
#[cfg(test)]
mod tests;
pub mod walk;

#[cfg(test)]
pub(crate) use hooks::Hooks;
pub(crate) use hooks::{Standard, update};

/// What a unit is.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) enum Kind {
    #[default]
    Animal,
    Guest,
    Keeper,
    Worker,
    Guide,
    /// The helicopter of the DRT base (staff 7.11.1).
    Heli,
}

impl Kind {
    /// The kind of the units of an entity class; `None` for everything that is not a unit.
    pub(crate) fn of_class(class: &str) -> Option<Kind> {
        Some(match class {
            "animals" => Kind::Animal,
            "guests" => Kind::Guest,
            "keeper" => Kind::Keeper,
            "maint" => Kind::Worker,
            "tour" => Kind::Guide,
            "helicopter" => Kind::Heli,
            _ => return None,
        })
    }

    /// The word the log uses.
    pub(crate) fn name(self) -> &'static str {
        match self {
            Kind::Animal => "animal",
            Kind::Guest => "guest",
            Kind::Keeper => "keeper",
            Kind::Worker => "worker",
            Kind::Guide => "guide",
            Kind::Heli => "heli",
        }
    }

    /// The class that decides the costs of the path finder.
    fn mover(self) -> path::Kind {
        match self {
            Kind::Animal => path::Kind::Animal,
            Kind::Guest => path::Kind::Guest,
            Kind::Keeper | Kind::Heli => path::Kind::Keeper,
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

/// The part of the state of a staff member: its exhibits and job (staff issues).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct Staff {
    /// The colour choices drawn at the creation, one per colour group (save games 11.2).
    pub colours: Vec<u8>,
    /// The counters drawn at the creation (staff 4.2): -1 for a key of 0.
    pub work: i32,
    pub chase: i32,
    /// The exhibits it is assigned to, the oldest first, none twice (staff 6).
    pub assigned: Vec<u32>,
    /// The duty kinds of a maintenance worker that the player switched off (staff 8.1): each kind
    /// the game allows starts on.
    pub off: [bool; 4],
    /// The tiles and the animals a keeper failed to reach; its searches skip them (staff 4.3).
    pub lost_tiles: Vec<usize>,
    pub lost_animals: Vec<u32>,
    /// A keeper's own target tile, apart from the goal record's (staff 7.4): the exhibit counters
    /// and the claimed tiles follow it.
    pub target: Option<usize>,
    /// The `cKeeperFoodType` of the animal a keeper feeds (staff 7.6).
    pub food_kind: i32,
    /// A tour guide's group, in the order the guests joined, its countdown of status ticks to the
    /// next gathering (-1 never) and whether this leg's release and gathering have run (tour
    /// guide 4).
    pub members: Vec<u32>,
    pub crowd: i32,
    pub leg: bool,
    /// A maintenance worker's task in the pool (0 none) and the milliseconds left before its
    /// unreachable lists are emptied (staff 4.3, 8.3).
    pub task: u32,
    pub clear: i32,
    /// A keeper's duties found nothing left to do: the goal's own flag, which the next check done
    /// turns into the record's done (staff 7.5).
    pub duties_done: bool,
    /// The animal the member has taken as his capture job, 0 for none (staff 7.11).
    pub captured: u32,
    /// A helicopter's own state (staff 7.11.1).
    pub heli: crate::staff::heli::Heli,
}

/// The kind-specific part of a unit.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Part {
    Animal(Animal),
    Guest(Guest),
    Staff(Staff),
}

/// The live state of one unit.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Unit {
    pub id: u32,
    pub(crate) kind: Kind,
    pub(crate) part: Part,
    /// Whether the unit is moving, and at which speed in movement units per second.
    pub moving: bool,
    pub speed: u32,
    /// The percentage that scales the step of the animation while the unit moves.
    pub anim_speed: u16,
    /// The point the unit goes to, in movement units; `None` for the original's invalid position.
    pub destination: Option<(i32, i32, i32)>,
    /// The tiles to follow, from the one the unit stands on to its destination (movement 4.1).
    pub path: Vec<usize>,
    /// The level of each tile of `path` when a depth search found it, else empty (tank fauna 7).
    pub levels: Vec<i32>,
    /// The tank mode: 0 dry, 1 ladder, 2 platform, 3 at the surface, 4 under water (tank fauna 4).
    pub tank: u8,
    /// The direction of the vertical move, -1 down, 0 level, +1 up, and the segment of the 3D move.
    pub vertical: i32,
    pub(crate) segment: Option<swim::Segment>,
    /// The 3D target the last move call named (tank fauna 6.2).
    pub target: Target,
    /// The direction of the last tile change, -1 for none (movement 2).
    pub travel: i32,
    /// A move call has started the walk (movement 8.2: the call's started flag).
    pub walking: bool,
    /// The speed band of the move call, for picking the move animation again.
    pub(crate) band: movement::Band,
    /// A swimmer changed between land and water: the move animation is picked again.
    pub flip: bool,
    /// The gate the unit opened and has not left behind (movement 6.2).
    pub gate: Option<u32>,
    /// The unit goes in or out of a building: the tile step test looks at the edge cost only.
    pub through_building: bool,
    /// The next tile step was refused: the goal has to give up (movement 4.7). The goals read
    /// and clear it.
    pub aborted: bool,
    /// The use set of a building to start at the next behaviour step, forced and not interruptible
    /// (guest buildings 4.2, 6.4.1).
    pub use_set: Option<String>,
    /// The tile a change of the terrain mode by the random walk sends the animal to (tank fauna
    /// 5), for its turn to act on once the walk is over.
    pub sent: Option<usize>,
    /// What the costs of the path finder read of the unit's type; its tile is set for each use.
    pub mover: Mover,
    /// The animation the unit holds.
    pub(crate) animation: Animation,
    /// The set the unit runs, which plays animations.
    pub(crate) runner: Runner,
    /// The goal record and the objects built from it (goals).
    pub(crate) goal: Goals,
    /// Milliseconds since the last status tick, started at `id % period`.
    pub status_timer: i32,
    /// While above 0 the unit runs no behaviour and no goal.
    pub wait: i32,
    /// The hold of a heal (staff 7.7): no walk starts and no move, the goals and sets go on. It
    /// has no timer; the heal or the pick-up of the unit lowers it.
    pub held: bool,
    /// The events read at the start of this step, for the hooks; empty between steps.
    pub(crate) events: Vec<Event>,
    /// The player holds the unit.
    pub carried: bool,
    /// The unit is inside a building that hides its users: off the map, it does not move.
    pub hidden: bool,
    /// The program is moving the unit by code (it is never set while a step runs).
    pub repositioning: bool,
    /// The unit has had its first turn, in which the kinds start their state.
    pub started: bool,
    /// The unit has read the characteristics of its type (the costs of the path finder).
    pub typed: bool,
    /// The sex a script gave the animal, which its first turn keeps (the `sim` command).
    pub sex: Option<bool>,
    /// The animal is put in its crate on its first turn (scenario trigger 5).
    pub boxed: bool,
    /// The contagion byte of a guest named by one of the guest-name cheats (cheats 4.2 E); not
    /// saved, not part of the state hash.
    pub spreads: u8,
    /// Frightened: the look for a predator found one, the ladder starts the avoid task
    /// (guests 6.2, staff 4.2; an animal's "afraid", animals 11.7); and the kept predator, 0 for
    /// none (animals 11.8).
    pub frightened: bool,
    pub predator: u32,
    /// Caught: a hunting animal has this unit in its catch; it plays `bCaughtBy` (animals
    /// 20.3 point 6).
    pub caught: bool,
    /// A guest saw a predator (guests 6.2, 6.12): it leaves at a run.
    pub afraid: bool,
    /// The show an animal was started in, never cleared (shows 18.2); 0 for none. Saved as
    /// the second flag field after the trick id, from version 97, and not for guests (shows 26).
    pub show: u16,
    /// The trick an animal was told to do, never cleared (shows 18.2); 0 for none. Saved in the unit
    /// block's first flag.
    pub trick: u16,
    /// An animal called to its show (it swims to the show tank) and one sent home from it (shows
    /// 18.2); both are saved in the unit block.
    pub called: bool,
    pub back: bool,
    /// An animal's own target tile and its position, apart from the goal record's target (shows
    /// 32.2): the tile across a show portal while it swims to cross. Only the position is saved.
    pub own_tile: Option<usize>,
    pub own_pos: [i32; 3],
}

impl Unit {
    /// A new unit, as the original's initialisation leaves it (simulation 8.2); its timer is set
    /// by the table.
    pub(crate) fn new(id: u32, kind: Kind) -> Unit {
        Unit {
            id,
            kind,
            part: match kind {
                Kind::Animal => Part::Animal(Animal::default()),
                Kind::Guest => Part::Guest(Guest::default()),
                _ => Part::Staff(Staff::default()),
            },
            moving: false,
            speed: 0,
            anim_speed: 100,
            destination: None,
            path: Vec::new(),
            levels: Vec::new(),
            tank: 0,
            vertical: 0,
            segment: None,
            target: Target::None,
            travel: -1,
            walking: false,
            band: movement::Band::default(),
            flip: false,
            gate: None,
            through_building: false,
            aborted: false,
            use_set: None,
            sent: None,
            mover: Mover::new(kind.mover(), None),
            animation: Animation::default(),
            runner: Runner::default(),
            goal: Goals::default(),
            status_timer: 0,
            wait: 0,
            held: false,
            events: Vec::new(),
            carried: false,
            hidden: false,
            repositioning: false,
            started: false,
            typed: false,
            sex: None,
            boxed: false,
            spreads: 0,
            frightened: false,
            caught: false,
            predator: 0,
            afraid: false,
            show: 0,
            trick: 0,
            called: false,
            back: false,
            own_tile: None,
            own_pos: own::NONE_POINT,
        }
    }

    /// The unit leaves the first tile of its path (and its level).
    pub(crate) fn pop_node(&mut self) {
        self.path.remove(0);
        if self.levels.len() > self.path.len() {
            self.levels.remove(0);
        }
    }

    /// Whether a flag keeps the unit from moving, from its status tick and from its goals.
    pub(crate) fn suspended(&self) -> bool {
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
    /// The AI clock in whole ticks, which the goals stamp their start with; set by the step.
    pub ai_ticks: u32,
    /// The path finder with its settings and its list of unreachable places.
    pub finder: Finder,
    /// The keys of `animals.ai` that every animal reads.
    pub class: Class,
    /// The `[AI]` settings of the ring of a random walk.
    pub(crate) ring: walk::Ring,
    /// `[AI] exhibitServiceInterval`, in AI ticks (staff 7.2): read as the unsigned number the
    /// original compares with, so a negative value is never reached.
    pub service_interval: u32,
    /// `[AI] useAttractionAttractiveness` is not 0: guests choose an attraction by a score of the
    /// distance, not by its place in the list (guests visit 3.7).
    pub attraction_score: bool,
    /// The session flag `eggs_hatch_fast` (cheats 4.2 C): a laid egg gets a hatch counter of 5.
    pub eggs_hatch_fast: bool,
    /// What the keepers spent during the step, as (category, cost), which the step books.
    pub spent: Vec<(usize, f32)>,
    lines: Vec<String>,
    animal_lines: Vec<String>,
    guest_lines: Vec<String>,
    /// The messages posted and not shown yet: the string number, the unit it is about and the
    /// names for its `%s` (the unit's own name when there are none).
    messages: Vec<(i32, u32, Vec<String>)>,
    staff_lines: Vec<String>,
    /// The sounds the units' animations began, with the unit's id, for the app.
    sounds: Vec<(u32, Heard)>,
    /// The rubble pieces (ids) turn-to-rubble made, whose `explode` the clocks queue.
    exploded: Vec<u32>,
    /// The gates that are open or playing their animation (movement 6.2).
    gates: gate::Gates,
    /// The game milliseconds the calendar has run (negative before 2001): the date a new animal is
    /// fed and rested.
    pub now: i64,
    /// The word before the number in a guest's name (string 9507).
    pub guest_name: String,
    /// The texts of the names of the types, by name id: the names of new entities and of the young.
    pub names: HashMap<i32, String>,
    /// The happiness above which, and the chance in percent with which, a guest that leaves joins
    /// the endowment; set by the step from the economy file.
    pub guest_join: (i32, i32),
    /// `[AI] VariableUnitSpeeds` is not 0: a guest walks at its own pace (movement 7.2); set by
    /// the step.
    pub variable_speeds: bool,
    /// The guests that left and the members that joined during the step, which the step books.
    pub guests_gone: u32,
    pub members_joined: u32,
    /// The viewing areas of the exhibits, built again after the map changed.
    pub(crate) views: crate::guest::view::Views,
    /// What guests donated during the step, which the step books.
    pub donated: Vec<f32>,
    /// `cDonationFactor`, set by the step from the economy file.
    pub donation_factor: f32,
    /// The session flags of the cheats, set by the step from the program's (cheats 2.4). The
    /// breeding flag goes down here when a roll uses it, and the step hands that back.
    pub(crate) cheat: crate::cheat::Flags,
    /// Where the sounds are heard from, set by the program before each step (sound 5).
    pub(crate) hearing: crate::staff::rotor::Hearing,
    /// The live state of the guest buildings and the prices of the economy file (guest buildings).
    pub(crate) buildings: crate::building::Table,
    pub(crate) prices: crate::building::Prices,
    /// The numbers of `shows.cfg` (shows 3.1).
    pub show: crate::show::Params,
    /// The satisfactions the show panel's editor left in the tricks of each type (information
    /// panels 19.9.8), by the type's name, subtype and the trick id; not saved.
    pub(crate) trick_current: std::collections::BTreeMap<crate::show::edit::Current, i32>,
    /// The upgrades of the building types, filled by the research programs (research 6.3).
    pub(crate) upgrades: crate::building::upgrade::Upgrades,
    /// The index of the upgrade each building uses in its type's list, by building id; a building
    /// with none listed uses the first (research 6.3). Kept apart from [`Units::buildings`] so
    /// that the state hash of a game with no chosen upgrade is the same.
    pub upgrade_index: std::collections::BTreeMap<u32, u32>,
    /// What guests paid buildings during the step, which the step books as concessions.
    pub concessions: Vec<f32>,
    /// What guests paid for seats at shows during the step, which the step books as show income.
    pub show_income: Vec<f32>,
    /// What the compost paid for dung during the step, which the step books as a construction refund.
    pub recycled: Vec<f32>,
    building_lines: Vec<String>,
    /// The thoughts of the guests, newest first (guests visit 8), and the megatiles their
    /// surroundings check reads (guests 12).
    pub(crate) thoughts: crate::guest::thought::Thoughts,
    pub(crate) mega: crate::guest::mega::Mega,
    /// The scenery guests gawk at (guests visit 3.1).
    pub(crate) gawk: crate::guest::gawk::Gawk,
    /// The events the units posted to the task pool during the step, which the step posts.
    pub(crate) posts: Vec<Event>,
    /// The fence pieces a worker mended during the step, with the start time each drew
    /// (sprite drawing 3.6 item 5), which the step gives to their animation.
    pub(crate) mended: Vec<(u32, u32)>,
    /// The maintenance task pool (staff 8.2).
    pub(crate) pool: crate::staff::pool::Pool,
    world_lines: Vec<String>,
    /// The animals' reactions to a change of their exhibit (animals 10): the animal, whether it
    /// likes the change, and the milliseconds since.
    pub reactions: Vec<(u32, bool, u32)>,
    /// The exhibit sounds the changes asked for (`smile`, `frown`), which the interface plays.
    cheers: Vec<&'static str>,
    /// The escaped list (animals 11.1): the animals loose at the load in file order, then those
    /// that escaped in play in the order they did; see [`predator::escaped`].
    pub escapes: Vec<u32>,
    /// An escape check found an animal loose this step: the status dates the escape (animals 11.1).
    pub loose: bool,
}

impl Default for Units {
    fn default() -> Self {
        Units {
            list: Vec::new(),
            index: HashMap::new(),
            period: 1000,
            ai_ticks: 0,
            finder: Finder::default(),
            class: Class::default(),
            ring: walk::Ring::default(),
            service_interval: 90,
            attraction_score: false,
            eggs_hatch_fast: false,
            hearing: Default::default(),
            spent: Vec::new(),
            lines: Vec::new(),
            animal_lines: Vec::new(),
            messages: Vec::new(),
            staff_lines: Vec::new(),
            sounds: Vec::new(),
            exploded: Vec::new(),
            gates: gate::Gates::new(),
            now: 0,
            guest_name: "Guest".into(),
            names: HashMap::new(),
            guest_join: (80, 50),
            variable_speeds: true,
            guests_gone: 0,
            members_joined: 0,
            views: Default::default(),
            donated: Vec::new(),
            donation_factor: 2.8e-13,
            cheat: Default::default(),
            buildings: Default::default(),
            prices: Default::default(),
            show: Default::default(),
            trick_current: Default::default(),
            upgrades: Default::default(),
            upgrade_index: Default::default(),
            concessions: Vec::new(),
            show_income: Vec::new(),
            recycled: Vec::new(),
            building_lines: Vec::new(),
            guest_lines: Vec::new(),
            thoughts: Default::default(),
            mega: Default::default(),
            gawk: Default::default(),
            posts: Vec::new(),
            mended: Vec::new(),
            pool: Default::default(),
            world_lines: Vec::new(),
            reactions: Vec::new(),
            cheers: Vec::new(),
            escapes: Vec::new(),
            loose: false,
        }
    }
}

impl Units {
    #[cfg(test)]
    pub(crate) fn len(&self) -> usize {
        self.list.len()
    }

    pub(crate) fn iter(&self) -> impl Iterator<Item = &Unit> {
        self.list.iter()
    }

    pub(crate) fn get(&self, id: u32) -> Option<&Unit> {
        self.index.get(&id).map(|&i| &self.list[i])
    }

    pub(crate) fn get_mut(&mut self, id: u32) -> Option<&mut Unit> {
        let i = *self.index.get(&id)?;
        Some(&mut self.list[i])
    }

    /// Sets the status period and starts every unit's timer again at `id % period`, as at a load.
    pub(crate) fn set_period(&mut self, period: i32) {
        self.period = period;
        for unit in &mut self.list {
            unit.status_timer = first_timer(unit.id, period);
        }
    }

    /// Reads the settings of the path finder and of the random walk from the ini file.
    pub(crate) fn set_ini(&mut self, ini: &[u8]) {
        let config = Config::parse(ini).ok();
        self.finder = Finder::new(
            config
                .as_ref()
                .map_or_else(Settings::default, Settings::from_ini),
        );
        self.ring = (config.as_ref()).map_or_else(walk::Ring::default, walk::Ring::from_ini);
        let interval = config
            .as_ref()
            .and_then(|c| ini_whole(c, "AI", "exhibitServiceInterval"));
        self.service_interval = interval.map_or(90, |v| v as u32);
    }

    /// The `unit` log lines since the last call, without the family word.
    pub(crate) fn take_lines(&mut self) -> Vec<String> {
        std::mem::take(&mut self.lines)
    }

    /// The `animal` log lines since the last call, without the family word.
    pub(crate) fn take_animal_lines(&mut self) -> Vec<String> {
        std::mem::take(&mut self.animal_lines)
    }

    /// The `guest` log lines since the last call, without the family word.
    pub(crate) fn take_guest_lines(&mut self) -> Vec<String> {
        std::mem::take(&mut self.guest_lines)
    }

    /// The `building` log lines since the last call, without the family word.
    pub(crate) fn take_building_lines(&mut self) -> Vec<String> {
        std::mem::take(&mut self.building_lines)
    }

    /// The messages posted since the last call, as (string number, unit id, names); the string
    /// holds a `%s` for each name, for the unit's name when there are none.
    pub(crate) fn take_messages(&mut self) -> Vec<(i32, u32, Vec<String>)> {
        std::mem::take(&mut self.messages)
    }

    /// The `staff` log lines since the last call, without the family word.
    pub(crate) fn take_staff_lines(&mut self) -> Vec<String> {
        std::mem::take(&mut self.staff_lines)
    }

    /// Asks for a sound at the place of entity `id` (nothing when there is none).
    pub(crate) fn push_sound(&mut self, id: u32, heard: Option<Heard>) {
        self.sounds.extend(heard.map(|h| (id, h)));
    }

    /// Notes that rubble piece `id` is to play `explode` once.
    pub(crate) fn push_exploded(&mut self, id: u32) {
        self.exploded.push(id);
    }

    /// The pieces noted since the last call.
    pub(crate) fn take_exploded(&mut self) -> Vec<u32> {
        std::mem::take(&mut self.exploded)
    }

    /// The sounds asked for since the last call: the unit and the sound its `[Sounds]` names.
    pub(crate) fn take_sounds(&mut self) -> Vec<(u32, Heard)> {
        std::mem::take(&mut self.sounds)
    }

    /// Adds a line to the `unit` log.
    pub(crate) fn note(&mut self, line: String) {
        self.lines.push(line);
    }

    pub(crate) fn staff_note(&mut self, line: String) {
        self.staff_lines.push(line);
    }

    /// The `world` lines the units told since the last call (`world fence ID LIFE`).
    pub(crate) fn take_world_lines(&mut self) -> Vec<String> {
        std::mem::take(&mut self.world_lines)
    }

    pub(crate) fn world_note(&mut self, line: String) {
        self.world_lines.push(line);
    }

    pub(crate) fn building_note(&mut self, line: String) {
        self.building_lines.push(line);
    }

    pub(crate) fn guest_note(&mut self, line: String) {
        self.guest_lines.push(line);
    }

    /// Guest `thinker` thinks text `text`, about `object` and the exhibit with the tile inside
    /// its gate (guests visit 8.2).
    pub(crate) fn think(
        &mut self,
        thinker: u32,
        text: i32,
        object: Option<u32>,
        exhibit: Option<(u32, Option<usize>)>,
    ) {
        let (exhibit, gate) = exhibit.map_or((None, None), |(x, g)| (Some(x), g));
        self.thoughts.add(crate::guest::thought::Thought {
            text,
            thinker,
            object,
            exhibit,
            gate,
        });
        self.guest_lines.push(format!("thought {thinker} {text}"));
    }

    /// Animal `id` reacts to a change of its exhibit; `likes` it or not (animals 10).
    pub(crate) fn react(&mut self, id: u32, likes: bool) {
        self.reactions.push((id, likes, 0));
        let word = if likes { "likes" } else { "dislikes" };
        self.tell(format!("{word} {id}"));
    }

    /// Some species liked a change of its exhibit (`smile`) or disliked it (`frown`): the sound
    /// plays once (habitat rating 9.4, sound 7). Logs `sound smile` or `sound frown`.
    pub(crate) fn cheer(&mut self, sound: &'static str) {
        self.cheers.push(sound);
        self.tell(format!("sound {sound}"));
    }

    /// The exhibit sounds asked for since the last call.
    pub(crate) fn take_cheers(&mut self) -> Vec<&'static str> {
        std::mem::take(&mut self.cheers)
    }

    /// The reactions age by the step of `ms` milliseconds and go at 1250 ms, or with their animal.
    pub(crate) fn fade(&mut self, ms: u32) {
        let mut reactions = std::mem::take(&mut self.reactions);
        reactions.retain_mut(|r| {
            r.2 = r.2.saturating_add(ms);
            r.2 < 1250 && self.get(r.0).is_some()
        });
        self.reactions = reactions;
    }

    pub(crate) fn tell(&mut self, line: String) {
        self.animal_lines.push(line);
    }

    pub(crate) fn post(&mut self, message: i32, id: u32) {
        self.messages.push((message, id, Vec::new()));
    }

    /// A message whose `%s` take these names.
    pub(crate) fn post_named(&mut self, message: i32, id: u32, names: Vec<String>) {
        self.messages.push((message, id, names));
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
                self.thoughts.forget(id);
            }
        }
        // A thought about a scenery object or a building that is gone goes with it.
        if self.thoughts.has_objects() {
            let ids: std::collections::HashSet<u32> = (entities.iter())
                .filter_map(|e| e.common.as_ref().map(|c| c.id))
                .collect();
            self.thoughts.keep_objects(|o| ids.contains(&o));
        }
        // A removed unit is no user of a building.
        for live in self.buildings.values_mut() {
            live.users.retain(|u| self.index.contains_key(&u.0));
        }
        if !self.buildings.is_empty() || !self.upgrade_index.is_empty() {
            let ids: std::collections::HashSet<u32> = (entities.iter())
                .filter(|e| matches!(e.class.as_str(), "building" | "objects"))
                .filter_map(|e| e.common.as_ref().map(|c| c.id))
                .collect();
            self.buildings.retain(|id, _| ids.contains(id));
            self.upgrade_index.retain(|id, _| ids.contains(id));
        }
    }
}

/// What the managers hand to a unit's turn.
pub struct Ctx<'a> {
    pub(crate) rng: &'a mut Rng,
    pub(crate) events: &'a Events,
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
    pub(crate) fn remove(&mut self, world: &mut World, id: u32) {
        world.forget_undo(id);
        crate::staff::leaves(world, id);
        self.removals.push(id);
    }
}
