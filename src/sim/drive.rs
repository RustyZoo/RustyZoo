//! A run without an interface (the `sim` command): the managers of [`Sim`] stepping a world by a
//! fixed length, the commands of a script applied before the step they name, and the state hash.

use super::{Sim, hash};
use crate::animal::Class;
use crate::behaviour::{Library, NoData, Timings};
use crate::entity::Catalogue;
use crate::money::Money;
use crate::unit::Part;
use crate::vfs::Vfs;
use crate::world::World;

/// A command a script gives at a step. Each is one the player's tools would give the world, as
/// far as a run without an interface can.
#[derive(Debug, Clone, PartialEq)]
pub enum Command {
    /// `remove ID`: the entity goes at the next step's removal pass.
    Remove(u32),
    /// `bulldoze ID`: the bulldozer deletes the entity at once, with its refund (a building pushes
    /// its users out, a trash can spills its litter).
    Bulldoze(u32),
    /// `tile TX TY ELEVATION SHAPE TERRAIN`: sets a tile.
    Tile(i32, i32, i32, u8, u8),
    /// `fill ID` and `drain ID`: the water of a tank.
    Fill(u32),
    Drain(u32),
    /// `cash N`: adds to the cash (a negative number takes from it).
    Cash(f32),
    /// `food X,Y TYPE [UNITS]`: a pile of food of the type (`herbchow`) on a tile, as a keeper puts
    /// it down: full, or with this many units.
    Food(i32, i32, String, Option<i32>),
    /// `guide X,Y`: a tour guide stands on the tile, hired without the hiring cost.
    Guide(i32, i32),
    /// `keeper X,Y` and `worker X,Y`: a marine specialist or a maintenance worker, the same way.
    Keeper(i32, i32),
    Worker(i32, i32),
    /// `animal TYPE X,Y [male|female]`: an animal of the type (`mantaray`) stands on the tile; of
    /// the sex when one is given (the subtype of that sex when the species has one).
    Animal(String, i32, i32, Option<bool>),
    /// `filter X,Y DIRECTION`: a tank filter on the tile, facing a direction 0 to 7, as placed
    /// during play.
    Filter(i32, i32, u32),
    /// `leave ID`: the guest decides to leave the zoo (the flag its checks raise).
    Leave(u32),
    /// `heal ID`: the animal is healed as a keeper's heal does (animals-needs 9.7), without the
    /// keeper and without the booking.
    Heal(u32),
    /// `mood ID N`: the animal's happiness is N, from -100 to 100.
    Mood(u32, i32),
    /// `save FILE` and `load FILE`: the zoo is written to a file on disk, or a saved game is opened
    /// from one as Continue does. The caller does these; [`Run::apply`] does nothing for them.
    Save(String),
    Load(String),
}

/// The highest elevation a script sets.
const MAX_ELEVATION: i32 = 255;

impl std::str::FromStr for Command {
    type Err = String;

    fn from_str(text: &str) -> Result<Command, String> {
        let words: Vec<&str> = text.split_whitespace().collect();
        let n = |i: usize| -> Result<i64, String> {
            words
                .get(i)
                .and_then(|w| w.parse().ok())
                .ok_or_else(|| format!("bad command: {text:?}"))
        };
        let id = |i: usize| u32::try_from(n(i)?).map_err(|_| format!("bad id: {text:?}"));
        let tile = |word: &str| -> Result<(i32, i32), String> {
            let at = word.split_once(',');
            let at = at.and_then(|(x, y)| Some((x.parse().ok()?, y.parse().ok()?)));
            at.ok_or_else(|| format!("bad tile: {text:?}"))
        };
        let command = match words.first().copied() {
            Some("remove") if words.len() == 2 => Command::Remove(id(1)?),
            Some("bulldoze") if words.len() == 2 => Command::Bulldoze(id(1)?),
            Some("heal") if words.len() == 2 => Command::Heal(id(1)?),
            Some("mood") if words.len() == 3 => {
                let mood = n(2).ok().filter(|m| (-100..=100).contains(m));
                Command::Mood(
                    id(1)?,
                    mood.ok_or_else(|| format!("bad mood: {text:?}"))? as i32,
                )
            }
            Some("fill") if words.len() == 2 => Command::Fill(id(1)?),
            Some("drain") if words.len() == 2 => Command::Drain(id(1)?),
            Some("leave") if words.len() == 2 => Command::Leave(id(1)?),
            Some("cash") if words.len() == 2 => {
                let cash = words[1].parse::<f32>().ok().filter(|c| c.is_finite());
                Command::Cash(cash.ok_or_else(|| format!("bad cash: {text:?}"))?)
            }
            Some(which @ ("save" | "load")) => {
                let file = text.trim_start().strip_prefix(which).unwrap_or("").trim();
                match (which, file.is_empty()) {
                    (_, true) => return Err(format!("bad command: {text:?}")),
                    ("save", _) => Command::Save(file.into()),
                    _ => Command::Load(file.into()),
                }
            }
            Some(who @ ("guide" | "keeper" | "worker")) if words.len() == 2 => {
                let (x, y) = tile(words[1])?;
                match who {
                    "guide" => Command::Guide(x, y),
                    "keeper" => Command::Keeper(x, y),
                    _ => Command::Worker(x, y),
                }
            }
            Some("animal") if (3..=4).contains(&words.len()) => {
                let (x, y) = tile(words[2])?;
                let female = match words.get(3) {
                    None => None,
                    Some(&"male") => Some(false),
                    Some(&"female") => Some(true),
                    Some(_) => return Err(format!("bad sex: {text:?}")),
                };
                Command::Animal(words[1].into(), x, y, female)
            }
            Some("filter") if words.len() == 3 => {
                let (x, y) = tile(words[1])?;
                let facing = words[2].parse().ok().filter(|d| *d < 8);
                Command::Filter(x, y, facing.ok_or_else(|| format!("bad facing: {text:?}"))?)
            }
            Some("food") if (3..=4).contains(&words.len()) => {
                let at = words[1].split_once(',');
                let at = at.and_then(|(x, y)| Some((x.parse().ok()?, y.parse().ok()?)));
                let (x, y) = at.ok_or_else(|| format!("bad tile: {text:?}"))?;
                let units = words.get(3).map(|w| {
                    (w.parse().ok().filter(|&n| n >= 0))
                        .ok_or_else(|| format!("bad units: {text:?}"))
                });
                Command::Food(x, y, words[2].into(), units.transpose()?)
            }
            Some("tile") if words.len() == 6 => {
                let part = |i: usize, max: i64| {
                    n(i).ok().filter(|v| (0..=max).contains(v)).ok_or_else(|| {
                        format!("tile: elevation, shape and terrain are 0 to {max}: {text:?}")
                    })
                };
                let coordinate =
                    |i: usize| i32::try_from(n(i)?).map_err(|_| format!("bad tile: {text:?}"));
                Command::Tile(
                    coordinate(1)?,
                    coordinate(2)?,
                    part(3, MAX_ELEVATION.into())? as i32,
                    part(4, 255)? as u8,
                    part(5, 255)? as u8,
                )
            }
            _ => return Err(format!("unknown command: {text:?}")),
        };
        Ok(command)
    }
}

/// Reads a script: one `<step> <command>` per line, `#` lines and empty lines left out. The
/// commands are sorted by step, those of one step keeping their order.
pub fn parse(text: &str) -> Result<Vec<(u64, Command)>, String> {
    let mut commands = Vec::new();
    for (n, line) in text.lines().enumerate() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let at = |why: String| format!("script line {}: {why}", n + 1);
        let (step, command) = line.split_once(char::is_whitespace).unwrap_or((line, ""));
        let step = step
            .parse()
            .map_err(|_| at(format!("not a step: {step:?}")))?;
        commands.push((step, command.parse().map_err(at)?));
    }
    commands.sort_by_key(|&(step, _)| step);
    Ok(commands)
}

/// A game being run: the managers, the world and the cash.
pub struct Run {
    /// The types of the game.
    pub cat: Catalogue,
    pub sim: Sim,
    pub world: World,
    pub money: Money,
    timings: Timings,
}

impl Run {
    /// Starts the game as the window's does, from a generator seeded with `seed` (a test run),
    /// with the types `cat`, the keys `class` of the animals' class and the economy file. It is
    /// a new game: the set-up rating check runs, and the fence pieces stay as the file stores them.
    pub fn new(
        files: (&[u8], &[u8]),
        types: (Catalogue, Class),
        world: World,
        money: Money,
        seed: u32,
    ) -> Run {
        Run::start(files, types, world, money, seed, false)
    }

    /// The same, a start whose map file has `resetfences` above 0 when `fences` is set: every fence
    /// piece is made healthy first (placing 7.4).
    pub fn start(
        files: (&[u8], &[u8]),
        types: (Catalogue, Class),
        world: World,
        money: Money,
        seed: u32,
        fences: bool,
    ) -> Run {
        Run::open(files, types, world, money, seed, Some(fences))
    }

    /// A saved game opened as Continue does: the calendar and the zoo status are the file's, and
    /// the load ends with the rating check.
    pub fn resume(
        files: (&[u8], &[u8]),
        types: (Catalogue, Class),
        world: World,
        money: Money,
        seed: u32,
    ) -> Run {
        Run::open(files, types, world, money, seed, None)
    }

    /// `fences` is `None` for a saved game, else the new game's reset of the fence pieces.
    fn open(
        (ini, economy): (&[u8], &[u8]),
        (cat, class): (Catalogue, Class),
        mut world: World,
        mut money: Money,
        seed: u32,
        fences: Option<bool>,
    ) -> Run {
        let mut sim = Sim::new(ini);
        sim.set_seed(seed);
        sim.set_economy(economy);
        sim.start_game(world.zoo.tiles.len(), world.zoo.clocks.clone());
        if fences.is_some() {
            sim.restart_calendar();
        }
        world.units.set_period(sim.ai.status_period);
        world.units.class = Class {
            reset_interval: sim.ai.animal_reset_interval,
            ..class
        };
        sim.load(&mut world, &cat);
        sim.fences_loaded(&world, &cat);
        match fences {
            Some(fences) => sim.new_game(&mut world, &cat, &mut money, fences),
            None => sim.loaded(&world, &cat, &mut money),
        }
        Run {
            cat,
            sim,
            world,
            money,
            timings: Timings::default(),
        }
    }

    /// Applies a command; whether it did anything (a tile off the map, a tank that is none, do
    /// not).
    pub fn apply(&mut self, command: &Command) -> bool {
        match *command {
            Command::Remove(id) => {
                let known = self.world.find(id).is_some();
                self.sim.remove(id);
                known
            }
            Command::Bulldoze(id) => {
                let (world, money, cat) = (&mut self.world, &mut self.money, &self.cat);
                crate::place::bulldozer::remove(world, money, cat, id, &mut self.sim.rng).is_some()
            }
            Command::Tile(x, y, elevation, shape, terrain) => {
                let rand = &mut || self.sim.rng.rand();
                crate::exhibit::rating::terrain(&mut self.world, &self.cat, rand, |w| {
                    w.set_tile(x, y, elevation, shape, terrain)
                })
            }
            Command::Fill(id) | Command::Drain(id) => {
                let tank = self
                    .world
                    .exhibits()
                    .get(id)
                    .is_some_and(|x| x.tank.is_some());
                match *command {
                    Command::Fill(id) => self.world.fill(id),
                    _ => self.world.drain(id),
                }
                tank
            }
            Command::Leave(id) => match self.world.units.get_mut(id).map(|u| &mut u.part) {
                Some(Part::Guest(g)) => {
                    g.leave = true;
                    true
                }
                _ => false,
            },
            Command::Cash(amount) => {
                self.money.add_cash(amount);
                true
            }
            Command::Food(x, y, ref name, units) => self.put_food(x, y, name, units),
            Command::Heal(id) => self.heal(id),
            Command::Mood(id, mood) => match self.world.units.get_mut(id).map(|u| &mut u.part) {
                Some(Part::Animal(a)) => {
                    a.happiness = mood;
                    true
                }
                _ => false,
            },
            Command::Guide(x, y) => self.add_unit("tour", None, x, y, None),
            Command::Keeper(x, y) => self.add_unit("keeper", Some("trainer"), x, y, None),
            Command::Worker(x, y) => self.add_unit("maint", None, x, y, None),
            Command::Animal(ref name, x, y, sex) => self.add_unit("animals", Some(name), x, y, sex),
            Command::Filter(x, y, facing) => self.add_filter(x, y, facing),
            Command::Save(_) | Command::Load(_) => false,
        }
    }

    /// A unit of the class (and the type, when it is given; the first sex of the data, or the
    /// subtype of the sex asked for, which the animal then has even when the species has no such
    /// subtype) on tile `(x, y)`, hired without the hiring cost; whether there is such a tile and
    /// such a type.
    fn add_unit(
        &mut self,
        class: &str,
        name: Option<&str>,
        x: i32,
        y: i32,
        sex: Option<bool>,
    ) -> bool {
        let named =
            |t: &&crate::entity::EntityType| name.is_none_or(|n| t.type_name == n.as_bytes());
        let mut found = self
            .cat
            .types
            .iter()
            .filter(named)
            .filter(|t| t.class == class);
        let wanted = sex.map(|female| if female { b"f" } else { b"m" });
        let first = found.clone().next();
        let Some(ty) = found
            .find(|t| wanted.is_some_and(|w| t.subtype == w))
            .or(first)
        else {
            return false;
        };
        let Some(tile) = self.world.tile_index(x, y) else {
            return false;
        };
        let text = |bytes: &[u8]| bytes.iter().map(|&b| char::from(b)).collect::<String>();
        let label = match class {
            "tour" => "Guide",
            "keeper" => "Keeper",
            "maint" => "Worker",
            _ => "Animal",
        };
        let name = crate::place::unit::name(&mut self.world, ty, label);
        let entity = crate::zoo::Entity {
            class: ty.class.into(),
            type_name: text(&ty.type_name),
            subtype: text(&ty.subtype),
            body_size: None,
            common: Some(crate::zoo::Common {
                x: x * 64 + 32,
                y: y * 64 + 32,
                z: self.world.zoo.tiles[tile].elevation * 16,
                rotation: 0,
                id: 0,
                name,
                colors: Vec::new(),
                fence: None,
                ..Default::default()
            }),
        };
        let shape = crate::world::Shape {
            kind: crate::render::order::Kind::of_class("tour"),
            footprint: (ty.footprint[0], ty.footprint[1]),
            fence: false,
            wall: false,
            gate: false,
            gated: false,
            tank: crate::world::TankKind::NONE,
            walkable: (false, false),
        };
        let Some(id) = self.world.add(entity, Some(shape)) else {
            return false;
        };
        if let (Some(female), Some(unit)) = (sex, self.world.units.get_mut(id)) {
            unit.sex = Some(female);
        }
        if let Some(at) = self.world.find(id) {
            crate::staff::make(&mut self.world, &self.cat, &mut self.sim.rng, at);
        }
        true
    }

    /// A tank filter facing `facing` on tile `(x, y)`; whether there is such a tile and such a
    /// type.
    fn add_filter(&mut self, x: i32, y: i32, facing: u32) -> bool {
        let tile = self.world.tile_index(x, y);
        let ty = self.cat.types.iter().find(|t| t.class == "filter");
        match (tile, ty) {
            (Some(tile), Some(ty)) => self.world.add_filter(ty, tile, facing),
            _ => false,
        }
    }

    /// A pile of food of the type `name` in the middle of tile `(x, y)`, full unless `units` says
    /// otherwise; whether there is such a tile and such a type.
    fn put_food(&mut self, x: i32, y: i32, name: &str, units: Option<i32>) -> bool {
        let Some(ty) = (self.cat).find("food", b"food", name.as_bytes()) else {
            return false;
        };
        let full = crate::build::number(ty, "cMaxFoodUnits").unwrap_or(0.0) as i32;
        let names = ("food", "food", name);
        let units = units.unwrap_or(full);
        crate::animal::dirt::put(&mut self.world, ty, names, (x, y), 0, units)
    }

    /// Heals the animal `id`; whether there is one.
    fn heal(&mut self, id: u32) -> bool {
        let Some(at) = self.world.find(id) else {
            return false;
        };
        let k = crate::animal::keys(&self.world, &self.cat, at);
        let unit = self.world.units.get_mut(id);
        let Some(Part::Animal(a)) = unit.map(|u| &mut u.part) else {
            return false;
        };
        a.heal(&k);
        self.world.units.tell(format!("heal {id}"));
        true
    }

    /// One step of `ms` milliseconds; the `sim`, `unit`, `animal` and `guest` log lines it made. The units
    /// run no behaviour set.
    pub fn step(&mut self, ms: u32) -> Vec<String> {
        self.step_with(ms, None)
    }

    /// [`Run::step`] whose units run their sets and animations, from the files of `vfs`.
    pub fn step_with(&mut self, ms: u32, vfs: Option<&Vfs>) -> Vec<String> {
        let money = &mut self.money;
        let update = |_: &mut World, _, _| {};
        match vfs {
            None => (self.sim).step_game(&NoData, &self.cat, &mut self.world, money, ms, update),
            Some(vfs) => {
                let data = Library {
                    catalogue: &self.cat,
                    vfs,
                    timings: &self.timings,
                };
                (self.sim).step_game(&data, &self.cat, &mut self.world, money, ms, update)
            }
        };
        let mut lines: Vec<String> = (self.sim.take_lines().into_iter())
            .map(|l| format!("sim {l}"))
            .collect();
        lines.extend((self.world.units.take_lines().into_iter()).map(|l| format!("unit {l}")));
        lines.extend(
            (self.world.units.take_sounds().into_iter())
                .map(|(id, s)| format!("unit sound {id} {}", s.file)),
        );
        lines.extend(
            (self.world.units.take_animal_lines().into_iter()).map(|l| format!("animal {l}")),
        );
        // No interface plays the exhibit sounds here; the animal log tells them.
        self.world.units.take_cheers();
        lines.extend(
            (self.world.units.take_guest_lines().into_iter()).map(|l| format!("guest {l}")),
        );
        lines.extend(
            (self.world.units.take_staff_lines().into_iter()).map(|l| format!("staff {l}")),
        );
        lines.extend(
            (self.world.units.take_building_lines().into_iter()).map(|l| format!("building {l}")),
        );
        lines.extend(
            (self.world.units.take_world_lines().into_iter()).map(|l| format!("world {l}")),
        );
        lines
    }

    /// The file of the zoo as it is now (see [`Zoo::write`]).
    pub fn save(&self) -> Result<Vec<u8>, String> {
        let mut zoo = self.world.zoo.clone();
        zoo.clocks = self.sim.clocks();
        zoo.write(&self.money)
    }

    pub fn hash(&self) -> u64 {
        hash::state(&self.sim, &self.world, &self.money)
    }
}
