//! Mates, births, young animals growing up and eggs (animals-needs 11, 12 and 13).
//!
//! The checks are plain functions of the animal, an [`Env`] and the generator, like the others of
//! the module; the world side is [`candidates`] (who it could pick as a mate), [`born`] (the young,
//! placed in the exhibit), [`pair`], [`grow_up`] and [`hatch`].
//!
//! The log lines of the family `animal`: `mate <id> <id>` when two animals become mates, `birth
//! <father> <young> <subtype> <x>,<y>` and `egg <father> <egg> <x>,<y>` for a young animal or an
//! egg put in the exhibit, `grow <id> <subtype>` when a young animal becomes an adult and `hatch
//! <id>` when an egg hatches.

use super::life;
use super::{Animal, Class, Env, Keys, Out, fire};
use crate::behaviour::Data;
use crate::entity::{Catalogue, EntityType};
use crate::path;
use crate::render::order::Kind;
use crate::sim::Rng;
use crate::unit::{Ctx, Part, Unit};
use crate::world::{Shape, TankKind, World};
use crate::zoo::{Common, Entity};

/// The messages "has given birth" (it names the mother) and "has hatched".
pub const BIRTH: i32 = 10032;
pub const HATCHED: i32 = 10322;

/// The failed attempts to place one young animal that destroy it (11.4 point 3).
const ATTEMPTS: u32 = 10;

/// The reason bits (animals 8): "lost its mate" (0x80) and "no mate" (0x8000).
const LOST_MATE: u32 = 7;
const NO_MATE: u32 = 15;

/// Where an animal's mate is (animals-needs 11.2 point 1).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Mate {
    /// It has none, or it is in the same exhibit or on the zoo grounds.
    #[default]
    Here,
    /// There is no such animal (died, sold).
    Gone,
    /// It is in another exhibit.
    Away,
}

/// What the birth makes of the young, called when the roll succeeded: whether at least one young
/// animal was put in the exhibit.
pub type Birth<'a> = &'a mut dyn FnMut(&mut Animal, &mut Rng) -> bool;

/// The hit points an egg must keep to count its time down: `cMinHatchHits`, or 60 percent of
/// `cMaxHits` when that is 0 (13.1).
fn threshold(k: &Keys) -> i32 {
    match k.min_hatch_hits {
        0 => (i64::from(k.max_hits) * 60 / 100) as i32,
        n => n,
    }
}

impl Animal {
    /// `setEgg` (13.1) for a new animal: the hatch countdown, and the health that gives a keeper
    /// something to heal. `sickly` is the keeper's `cSicklyAnimalPct`.
    pub fn set_egg(&mut self, k: &Keys, sickly: i64) {
        self.egg = true;
        self.hatch = k.time_to_hatch;
        let start = i64::from(threshold(k)) - 1 + i64::from(self.hatch);
        let top = i64::from(k.max_hits);
        let cap = (100 - sickly.clamp(0, 100)) * top / 100;
        self.health = start
            .min(top)
            .min(cap)
            .clamp(i64::from(i32::MIN), i64::from(i32::MAX)) as i32;
    }
}

/// The egg check (13.2), before the egg branch of the order: the time to hatch counts down while
/// the egg keeps its health, and once it is spent an egg in an exhibit hatches.
pub fn egg_check(a: &mut Animal, k: &Keys, env: &Env, out: &mut Out) {
    if !a.egg || (!a.dying && a.health < threshold(k)) {
        return;
    }
    let old = a.hatch;
    a.hatch = old.wrapping_sub(1);
    if old < 1 && env.exhibit {
        if a.dying {
            out.remove = true;
        } else if env.sets.has_hatch && !env.sets.hatch_now {
            out.sets.push("bHatch");
        }
    }
}

/// The check of a young animal growing up (12): the counter runs at every tick, and when it ends a
/// young animal becomes an adult.
pub fn grow(a: &mut Animal, k: &Keys, env: &Env, out: &mut Out) {
    if fire(&mut a.baby, k.baby_to_adult) && env.baby {
        out.grow = true;
    }
}

/// Picks a mate among `env.candidates` (11.3): its id is cleared, then one candidate is drawn and
/// both take each other's id. Only in an exhibit.
fn find(a: &mut Animal, env: &Env, rng: &mut Rng, out: &mut Out) -> bool {
    if !env.exhibit {
        return false;
    }
    a.mate = 0;
    let Some(&mate) = env
        .candidates
        .get(rng.pick(env.candidates.len() as u32) as usize)
    else {
        return false;
    };
    a.mate = mate;
    out.pair = Some(mate);
    a.unmark(LOST_MATE);
    a.unmark(NO_MATE);
    true
}

/// The reproduction check (11) with old age before it: the birth interval counts up at every
/// tick; when the check is due, the life counter runs (10), and an adult that is not dying looks
/// after its mate and, a male in good spirits that is ready, rolls for young.
pub fn reproduction(
    a: &mut Animal,
    k: &Keys,
    class: &Class,
    env: &Env,
    rng: &mut Rng,
    out: &mut Out,
    birth: Birth,
) {
    let ready = a.interval >= k.reproduction_interval;
    if !ready {
        a.interval = a.interval.wrapping_add(1);
    }
    if !life::aging(a, k, class, env, rng, out)
        || a.dying
        || !env.adult
        || env.show_tank
        || a.in_show
    {
        return;
    }
    // 1: the mate must still be in the exhibit or on the grounds.
    let lost = a.mate != 0 && env.mate != Mate::Here;
    if lost && env.mate == Mate::Away {
        (out.unpair, a.mate) = (Some(a.mate), 0);
    }
    let monogamous = k.mating_type == 1;
    let (delay, full) = (class.mate_pick_delay, class.mate_pick_delay.wrapping_sub(1));
    if lost {
        // 3: another mate at once, or after the delay of a monogamous animal that mourns.
        if !monogamous {
            find(a, env, rng, out);
            a.mate_delay = full;
        } else {
            if a.mate_delay == full {
                a.change(k.no_mate_change);
                a.mark(LOST_MATE, true);
            }
            if fire(&mut a.mate_delay, delay) {
                find(a, env, rng, out);
            }
        }
        return;
    }
    // 2: no mate yet.
    if a.mate == 0 && !find(a, env, rng, out) && monogamous {
        a.mark(NO_MATE, true);
    }
    // 4: the birth.
    if a.happiness >= k.happy_reproduce
        && env.male
        && ready
        && (a.mate != 0 || find(a, env, rng, out))
        && rng.chance(k.reproduction_chance)
        && birth(a, rng)
    {
        a.interval = 0;
        a.change(k.baby_born_change);
    }
}

/// The animals the one at position `at` could pick as a mate (11.3), in the order of the
/// exhibit's list (the tiles in the exhibit's order, then each tile's unit list): the ones of its
/// species in its exhibit that are adults (`m` or `f`, not a young animal or its egg) of the other
/// sex, have no mate and are neither its parents nor its young.
pub fn candidates(world: &World, at: usize, x: u32, me: &Animal) -> Vec<u32> {
    let own = &world.zoo.entities[at];
    let mine = own.common.as_ref().map(|c| c.id);
    crate::animal::exhibit_animals(world, x)
        .into_iter()
        .filter_map(|i| {
            let e = &world.zoo.entities[i];
            let c = e.common.as_ref().filter(|_| e.type_name == own.type_name)?;
            let Some(Part::Animal(other)) = world.units.get(c.id).map(|u| &u.part) else {
                return None;
            };
            let fit = matches!(e.subtype.as_str(), "m" | "f")
                && Some(c.id) != mine
                && other.female != me.female
                && other.mate == 0
                && [other.mother, other.father]
                    .iter()
                    .all(|&p| Some(p) != mine)
                && [me.mother, me.father].iter().all(|&p| p != c.id);
            fit.then_some(c.id)
        })
        .collect()
}

/// Where the animal at position `at`'s mate stands, for the check of 11.2 point 1.
pub fn mate_of(world: &World, at: usize, mate: u32) -> Mate {
    let region = |c: &Common| {
        let tile = world.tile_index(c.x.div_euclid(64), c.y.div_euclid(64));
        tile.and_then(|t| world.exhibits().region(t))
    };
    let mine = world.zoo.entities[at].common.as_ref().and_then(region);
    let other = world.find(mate).map(|m| &world.zoo.entities[m]);
    match other
        .filter(|e| e.class == "animals")
        .and_then(|e| e.common.as_ref())
    {
        None => Mate::Gone,
        Some(c) if region(c) == mine || region(c) == Some(crate::exhibit::GROUNDS) => Mate::Here,
        Some(_) => Mate::Away,
    }
}

/// The other animal learns of the pair: it takes the id, and the lines about a missing mate go
/// (11.3). `unpair` is the animal that lost its mate.
pub fn pair(world: &mut World, id: u32, out: &Out) {
    if let Some(mate) = out.pair {
        if let Some(Part::Animal(m)) = world.units.get_mut(mate).map(|u| &mut u.part) {
            m.mate = id;
            m.unmark(LOST_MATE);
            m.unmark(NO_MATE);
        }
        world.units.tell(format!("mate {id} {mate}"));
    }
    if let Some(Part::Animal(m)) = out
        .unpair
        .and_then(|old| world.units.get_mut(old))
        .map(|u| &mut u.part)
        && m.mate == id
    {
        m.mate = 0;
    }
}

/// The keeper's `cSicklyAnimalPct`: one value for the whole game, the last keeper type's (staff 2).
fn sickly(cat: &Catalogue) -> i64 {
    let value = |t: &&EntityType| crate::build::number(t, "cSicklyAnimalPct");
    let keepers = cat.types.iter().filter(|t| t.class == "keeper");
    keepers
        .filter_map(|t| value(&t))
        .next_back()
        .map_or(10, |p| p as i64)
}

/// How an animal of the type takes its place on the map.
pub fn shape(ty: &EntityType) -> Shape {
    Shape {
        kind: Kind::of_class("animals"),
        footprint: (ty.footprint[0], ty.footprint[1]),
        fence: false,
        wall: false,
        gate: false,
        gated: false,
        tank: TankKind::NONE,
        walkable: (false, false),
    }
}

/// The tiles of exhibit `x` and its amphibious neighbours that suit a young animal of type `ty`
/// (11.4 point 3), in the exhibit's order: nothing in the four quarters, no unit listed, and a
/// terrain cost below 20 for the young, which has no tile yet; an egg swims and dives not.
pub(super) fn suitable(world: &World, x: u32, ty: &EntityType, egg: bool) -> Vec<usize> {
    let flag = |key| crate::build::number(ty, key) == Some(1.0);
    let mut young = path::Mover::new(path::Kind::Animal, None);
    young.swims = flag("cSwims") && !egg;
    young.underwater = flag("cUnderwater") && !egg;
    young.only_underwater = flag("cOnlyUnderwater");
    young.climbs = flag("cClimbsCliffs");
    young.avoid_edges = crate::build::number(ty, "cAvoidEdges").map_or(0, |r| r.max(0.0) as u32);
    young.footprint = (ty.footprint[0] * ty.footprint[1]).clamp(1, 4) as u32;
    let near = (world.exhibits().get(x)).map_or(Vec::new(), |e| e.amphibious.clone());
    let tiles = std::iter::once(x)
        .chain(near)
        .flat_map(|r| world.tiles_of(r));
    let suits = |t: &usize| {
        (0..4).all(|q| world.quadrant(*t, q).is_none())
            && world.units_on(*t).next().is_none()
            && path::terrain(world, &young, *t, None) < path::MAX
    };
    tiles.filter(suits).collect()
}

/// `fCheckReproduction` after the roll (11.4 points 2 to 4) for the male `father` standing at
/// position `at`: the young of its species are made one after the other and put on random
/// suitable tiles of its exhibit, until `cOffspring` are there or no tile suits. Every animal of
/// the species in the exhibit takes the happiness change `bonus` (`cBabyBornChange`), the father
/// apart (the caller does it), and the birth is told. Whether a young animal was put there.
pub fn born(
    father: &mut Animal,
    id: u32,
    (world, cat, data): (&mut World, &Catalogue, &dyn Data),
    (at, bonus): (usize, i32),
    rng: &mut Rng,
) -> bool {
    let type_name = world.zoo.entities[at].type_name.clone();
    let Some(tile) = (world.zoo.entities[at].common.as_ref())
        .and_then(|c| world.tile_index(c.x.div_euclid(64), c.y.div_euclid(64)))
    else {
        return false;
    };
    let Some(x) = crate::animal::food::exhibit_of(world, tile) else {
        return false;
    };
    let bytes = type_name.bytes().collect::<Vec<u8>>();
    // The young's type, else an adult of the species.
    let (young, sub) = match cat.find("animals", &bytes, b"y") {
        Some(t) => (t, "y"),
        None => {
            let sub = if rng.rand() & 1 == 0 { "f" } else { "m" };
            match cat.find("animals", &bytes, sub.as_bytes()) {
                Some(t) => (t, sub),
                None => return false,
            }
        }
    };
    let class = world.units.class.clone();
    let k = Keys::of(young, &class);
    let sickly = sickly(cat);
    let mother = father.mate;
    let mut placed = 0u32;
    for _ in 0..k.offspring.max(0) {
        let mut baby = Animal::create(&k, &class, sub, rng);
        if k.lays_eggs == 1 {
            baby.set_egg(&k, sickly);
        }
        (baby.father, baby.mother) = (id, mother);
        (baby.fed, baby.slept) = (world.units.now, world.units.now);
        // Ten attempts for each young: a random tile among the ones that suit it (no draw when
        // none does), which the world may refuse. A young that finds no place is destroyed and
        // no more are made.
        let done = (0..ATTEMPTS).any(|_| {
            let spots = suitable(world, x, young, baby.egg);
            let t = match spots.len() as u32 {
                0 => return false,
                n => spots[rng.pick(n) as usize],
            };
            put(world, data, young, sub, t, baby.clone())
        });
        if !done {
            break;
        }
        placed += 1;
    }
    if placed == 0 {
        return false;
    }
    // The species in the exhibit are glad, and the mother is named.
    for e in crate::panel::exhibit::animals_in(world, x) {
        let other = &world.zoo.entities[e];
        let Some(c) = other
            .common
            .as_ref()
            .filter(|_| other.type_name == type_name)
        else {
            continue;
        };
        if c.id != id
            && let Some(Part::Animal(a)) = world.units.get_mut(c.id).map(|u| &mut u.part)
        {
            a.change(bonus);
        }
    }
    let name = (world.find(father.mate))
        .and_then(|m| {
            world.zoo.entities[m]
                .common
                .as_ref()
                .map(|c| c.name.clone())
        })
        .unwrap_or_default();
    // The message is the mate's own (11.2 point 4), posted even if the mate is gone.
    world.units.post_named(BIRTH, father.mate, vec![name]);
    world.units.tell(format!("message {BIRTH} {}", father.mate));
    true
}

/// Puts the young `baby` of type `ty` and subtype `sub` in the middle of tile `t` and starts it
/// as a loaded animal is started: its state is made, with the draws, and it takes its place.
fn put(
    world: &mut World,
    data: &dyn Data,
    ty: &EntityType,
    sub: &str,
    t: usize,
    baby: Animal,
) -> bool {
    let width = world.zoo.width as usize;
    let (tx, ty_) = ((t % width) as i32, (t / width) as i32);
    let text = |bytes: &[u8]| bytes.iter().map(|&b| char::from(b)).collect::<String>();
    let label = (world.units.names.get(&ty.name_id)).map_or_else(|| "Animal".into(), Clone::clone);
    let name = crate::place::unit::name(world, ty, &label);
    let entity = Entity {
        class: "animals".into(),
        type_name: text(&ty.type_name),
        subtype: sub.into(),
        body_size: None,
        common: Some(Common {
            x: tx * 64 + 32,
            y: ty_ * 64 + 32,
            z: world.zoo.tiles[t].elevation * 16,
            rotation: 0,
            id: 0,
            name,
            colors: Vec::new(),
            stamp: None,
            fence: None,
            food: 0,
            movement_mode: 0,
            animal: None,
            guest: None,
            filter: None,
            building: None,
        }),
    };
    let Some(id) = world.unrecorded(|w| w.add(entity, Some(shape(ty)))) else {
        return false;
    };
    let (Some(at), Some(slot)) = (world.find(id), world.units.get_mut(id)) else {
        return false;
    };
    let egg = baby.egg;
    let father = baby.father;
    let mut unit = std::mem::replace(slot, Unit::new(id, slot.kind));
    unit.part = Part::Animal(baby);
    unit.started = true;
    crate::unit::movement::read_type(&mut unit, data, &world.zoo.entities[at]);
    unit.typed = true;
    crate::unit::swim::add_to_map(&mut unit, world, at);
    if egg && data.timing(&world.zoo.entities[at], "egg_idle").is_some() {
        unit.animation.base_to("egg_idle");
    }
    if let Some(slot) = world.units.get_mut(id) {
        *slot = unit;
    }
    world.mark_rating(t);
    let line = match egg {
        true => format!("egg {father} {id} {tx},{ty_}"),
        false => format!("birth {father} {id} {sub} {tx},{ty_}"),
    };
    world.units.tell(line);
    true
}

/// A young animal becomes an adult (12): its type changes to the species' `m` or `f` by its sex
/// (the `m` one when the species has no `f`), keeping its needs, name, mate and parents; it stops
/// everything it does, and reads its type again.
pub fn grow_up(unit: &mut Unit, world: &mut World, ctx: &mut Ctx, at: usize, data: &dyn Data) {
    let Part::Animal(a) = &unit.part else {
        return;
    };
    let entity = &world.zoo.entities[at];
    let bytes = entity.type_name.bytes().collect::<Vec<u8>>();
    let wanted = if a.female { "f" } else { "m" };
    let adult = [wanted, "m"].into_iter().find_map(|s| {
        ctx.cat
            .find("animals", &bytes, s.as_bytes())
            .map(|t| (s, t))
    });
    let Some((sub, ty)) = adult else {
        return;
    };
    let tile = (entity.common.as_ref())
        .and_then(|c| world.tile_index(c.x.div_euclid(64), c.y.div_euclid(64)));
    world.retype(at, sub, shape(ty));
    unit.runner = crate::behaviour::Runner::default();
    crate::unit::movement::stop(unit);
    crate::animal::food::halt(unit, world, ctx, at, data);
    unit.typed = false;
    if let Some(t) = tile {
        world.mark_rating(t);
    }
    world.units.tell(format!("grow {} {sub}", unit.id));
}

/// `finishHatch` (13.3): the egg is an animal, shows its idle picture and the hatching is told.
pub fn hatch(unit: &mut Unit, world: &mut World, at: usize, data: &dyn Data) {
    let Part::Animal(a) = &mut unit.part else {
        return;
    };
    if !std::mem::take(&mut a.egg) {
        return;
    }
    if data.timing(&world.zoo.entities[at], "idle").is_some() {
        unit.animation.base_to("idle");
    }
    unit.animation.cut();
    world.units.post(HATCHED, unit.id);
    world.units.tell(format!("message {HATCHED} {}", unit.id));
    world.units.tell(format!("hatch {}", unit.id));
}
