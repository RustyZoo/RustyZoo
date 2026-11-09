//! The hooks that run the checks for an animal of the world: they read the exhibit and the company,
//! run the checks, apply the outcome and load an animal of a saved map.

use super::class::Keys;
use super::*;
use crate::goal::Cx;

pub(super) fn find<'c>(cat: &'c Catalogue, e: &Entity) -> Option<&'c EntityType> {
    let bytes = |s: &str| s.chars().map(|c| c as u8).collect::<Vec<u8>>();
    cat.find(&e.class, &bytes(&e.type_name), &bytes(&e.subtype))
}

/// The records of exhibit `x`; a dirty exhibit is rated first.
pub(super) fn records(world: &mut World, cat: &Catalogue, x: u32, rng: &mut Rng) -> Vec<Record> {
    rating::refresh(world, cat, x, &mut || rng.rand());
    world
        .exhibits()
        .get(x)
        .map_or_else(Vec::new, |e| e.rating.records.clone())
}

/// Paints the tile `(tx, ty)` as trampled ground of type `paint` for the animal `id`; the bare
/// cell paint marks no exhibit (animals 4.5).
pub(super) fn trample(world: &mut World, (tx, ty): (i32, i32), paint: u8, id: u32) {
    world.paint_cell(tx, ty, paint);
    world.units.tell(format!("trample {id} {tx},{ty}"));
}

/// An exhibit and its amphibious neighbours.
pub(super) fn region_and_neighbours(world: &World, x: u32) -> Vec<u32> {
    let neighbours = world.exhibits().get(x).map(|e| e.amphibious.clone());
    std::iter::once(x)
        .chain(neighbours.into_iter().flatten())
        .collect()
}

/// A check's read of exhibit `x` for the animal `a` at `at` (habitat rating 4): the bite of the
/// tick so far has marked the exhibit, which is recomputed when dirty, and `env` takes what the
/// check reads.
pub(super) fn read_exhibit(
    world: &mut World,
    cat: &Catalogue,
    (read, x, at): (Read, u32, usize),
    (a, meal, key): (&Animal, &Meal, &str),
    env: &mut Env,
    rng: &mut Rng,
) {
    if read != Read::Mates {
        food::settle(world, a, meal);
    }
    let regions = match read {
        Read::Mates | Read::Keeper | Read::Emptied => vec![x],
        Read::Pile | Read::Trample => Vec::new(),
        Read::Food => food::near(world, x, &keys(world, cat, at)),
        Read::Sick | Read::Exhibit => region_and_neighbours(world, x),
    };
    for &n in &regions {
        rating::refresh(world, cat, n, &mut || rng.rand());
    }
    match read {
        Read::Mates => env.candidates = breed::candidates(world, at, x, a),
        Read::Food => {
            // The food of its kind at the last rating pass, the tick's bites taken (4.1).
            let kind = usize::try_from(keys(world, cat, at).food_kind).ok();
            let sums = regions.iter().filter_map(|&n| world.exhibits().get(n));
            env.food_total = (sums.filter_map(|e| e.rating.food.get(kind?).copied()))
                .fold(0, i32::saturating_add);
        }
        Read::Emptied | Read::Pile | Read::Trample => {}
        Read::Keeper => env.keeper_working = life::keeper_at_work(world, x),
        Read::Sick => {
            // The counters of the last rating pass, not the animals now (animals needs 9.5).
            let sick: u32 = (regions.iter().filter_map(|&n| world.exhibits().get(n)))
                .fold(0, |sum, e| sum.saturating_add(e.rating.sick));
            env.sick_near = sick.saturating_sub(u32::from(a.at_minimum_health));
        }
        Read::Exhibit => {
            env.census = census(world, x, &regions, key);
            // The counters of the exhibit and its amphibious neighbours at the last rating pass,
            // capped at their animal counts (animals 5.2).
            let seen = regions.iter().filter_map(|&n| world.exhibits().get(n));
            let (unhappy, animals) = seen.fold((0u32, 0usize), |(u, n), e| {
                (
                    u.saturating_add(e.rating.unhappy),
                    n + e.rating.animals.len(),
                )
            });
            let capped = unhappy.min(u32::try_from(animals).unwrap_or(u32::MAX));
            env.census.unhappy = capped.saturating_sub(u32::from(a.very_unhappy));
            let own = records(world, cat, x, rng)
                .into_iter()
                .find(|r| r.key == key);
            if let Some(r) = own {
                let near = regions[1..].to_vec();
                let read = &mut |n| records(world, cat, n, rng);
                let seen = rating::rating(x, &near, key, read).unwrap_or_default();
                env.species = r.animals as i32;
                env.rating = rating::shown(seen) as i32;
                env.other = r.other.round() as i32;
            }
        }
    }
}

/// Counts the animals of the regions `near` (an exhibit and its amphibious neighbours) for an
/// animal of species `key`, from their stored animal lists (habitat rating 5.1; a reader
/// refreshes a dirty exhibit first); the species only in the exhibit `x`.
pub(super) fn census(world: &World, x: u32, near: &[u32], key: &str) -> Census {
    let mut c = Census::default();
    let listed = near
        .iter()
        .flat_map(|&r| exhibit_animals(world, r).into_iter().map(move |at| (r, at)));
    for (region, at) in listed {
        let e = &world.zoo.entities[at];
        if e.common.is_none() {
            continue;
        }
        let adult = matches!(e.subtype.as_str(), "m" | "f");
        let same = e.type_name == key;
        c.adults += u32::from(same && adult);
        c.all_adults += u32::from(adult);
        if region == x {
            c.species += u32::from(same);
        }
    }
    c
}

/// The animals (positions in the entity list) of exhibit `x` in the order of the exhibit's
/// stored animal list (habitat rating 5.1, guests visit 6.3). The list is rebuilt only at a
/// recomputation, so a reader refreshes the exhibit first ([`rating::refresh`]).
pub(crate) fn exhibit_animals(world: &World, x: u32) -> Vec<usize> {
    let list = world.exhibits().get(x).map(|e| &e.rating.animals);
    (list.into_iter().flatten())
        .filter_map(|&id| world.find(id))
        .collect()
}

/// The shelters for the check of an animal on tile `tile` that aims at building `target` (animals
/// 6): the nearest buildings that serve `animalrest`, none when the target still does.
pub(crate) fn shelters(
    world: &World,
    cat: &Catalogue,
    target: u32,
    tile: Option<usize>,
    who: Option<&crate::building::Species>,
) -> Shelter {
    let serves = |id: u32| {
        let e = world.find(id).map(|at| &world.zoo.entities[at]);
        let kind = e.and_then(|e| crate::building::kind_of(cat, e, world.units.prices.default));
        kind.is_some_and(|k| k.satisfies(NEED))
    };
    let held = target != 0 && serves(target);
    let nearest = match tile {
        Some(t) if !held => crate::building::satisfiers(world, cat, t, NEED, SEARCH, who).1,
        _ => Vec::new(),
    };
    Shelter { held, nearest }
}

/// The land, water and tank tiles of the exhibits `near` (animals-needs 7.2).
pub(super) fn kinds(world: &World, near: &[u32]) -> [usize; 3] {
    let mut count = [0; 3];
    for t in near.iter().flat_map(|&x| tank::tiles(world, x)) {
        count[usize::from(tank::kind(world, t))] += 1;
    }
    count
}

/// The keys of the animal at position `at` of the entity list.
pub(crate) fn keys(world: &World, cat: &Catalogue, at: usize) -> Keys {
    keys_over(world, cat, at, &world.research)
}

/// The same with the changes of research `over`.
fn keys_over(world: &World, cat: &Catalogue, at: usize, over: &crate::research::Overlay) -> Keys {
    let class = &world.units.class;
    match find(cat, &world.zoo.entities[at]) {
        Some(ty) => Keys::of(ty, class).researched(ty, over),
        None => Keys::read(&|_, fallback| fallback, class),
    }
}

/// A research change of the life counter reaches the live animals of its type (research 6.4).
pub(crate) fn lengthen(world: &mut World, changes: &[crate::research::Life]) {
    for change in changes {
        let ids = (world.zoo.entities.iter())
            .filter(|e| e.class == "animals" && change.reaches(e))
            .filter_map(|e| Some(e.common.as_ref()?.id));
        let ids: Vec<u32> = ids.collect();
        for id in ids {
            if let Some(Part::Animal(a)) = world.units.get_mut(id).map(|u| &mut u.part) {
                a.life = change.of(a.life);
            }
        }
    }
}

/// Creates the animal at position `at` of a loaded map as the loader does (animals 3.5): it gets
/// its creation, with its draws, and then the values its record stores. The caller goes in file
/// order before the first step; the first turn of the animal draws nothing.
pub(crate) fn load(world: &mut World, cat: &Catalogue, rng: &mut Rng, at: usize) {
    let entity = &world.zoo.entities[at];
    let Some((id, saved)) = (entity.common.as_ref())
        .and_then(|c| Some((c.id, c.animal.clone()?)))
        .filter(|&(id, _)| world.find(id) == Some(at))
    else {
        return;
    };
    // The life is cut by the type as the file leaves it: the research of the file comes after the
    // animals and lengthens them ([`lengthen`], save games 15.5).
    let (k, subtype) = (
        keys_over(world, cat, at, &Default::default()),
        entity.subtype.clone(),
    );
    let mut animal = Animal::create(&k, &world.units.class, &subtype, rng);
    animal.restore(&saved, &k, &world.units.class);
    // The chase stamp is the entity base's: a stored 0 becomes the current tick (animals 20.8).
    animal.stamp = (entity.common.as_ref())
        .and_then(|c| c.stamp)
        .unwrap_or(world.units.ai_ticks);
    // What the saved unit was doing: the active goal (its set is taken by `unit::resume`).
    let block = entity.common.as_ref().and_then(|c| c.unit.as_ref());
    // The drink task is the wanted goal's (behaviours 4.7); a drinking flag saved outside it stays
    // up until the hunger check that finds food lowers it (animals-needs 6.1).
    let drink = block.is_some_and(|b| b.target.goal == i32::from(crate::goal::DRINK));
    let now = world.units.ai_ticks;
    if let Some(unit) = world.units.get_mut(id) {
        if animal.drinking && drink {
            let task = crate::goal::Record {
                task: crate::goal::DRINK,
                mode: 1,
                start: now,
                ..Default::default()
            };
            unit.goal.record = task;
        }
        // An egg shows its own picture (animals-needs 13.1), a crate its box (unit placing 9.1).
        if animal.egg {
            unit.animation.base_to("egg_idle");
        } else if animal.crated {
            unit.animation.base_to("box_idle");
        }
        unit.part = Part::Animal(animal);
        unit.started = true;
    }
    // A loose animal joins the escaped list in file order (animals 11.1).
    if world
        .units
        .get(id)
        .is_some_and(|u| matches!(&u.part, Part::Animal(a) if a.escaped))
    {
        world.units.escapes.push(id);
    }
}

/// The first turn of an animal that was not on the map at its load (adopted, born): its values
/// start and its counters are drawn.
pub(crate) fn start(unit: &mut Unit, world: &mut World, ctx: &mut Ctx, at: usize) {
    let Part::Animal(a) = &mut unit.part else {
        return;
    };
    let k = keys(world, ctx.cat, at);
    let subtype = world.zoo.entities[at].subtype.clone();
    *a = Animal::create(&k, &world.units.class, &subtype, ctx.rng);
    (a.fed, a.slept) = (world.units.now, world.units.now);
    if let Some(female) = unit.sex.take() {
        a.female = female;
    }
    a.crated = std::mem::take(&mut unit.boxed);
}

/// The update of the values at a status tick (animals 3.3).
pub(crate) fn needs(unit: &mut Unit, world: &mut World, ctx: &mut Ctx, at: usize) {
    let k = keys(world, ctx.cat, at);
    let Part::Animal(a) = &mut unit.part else {
        return;
    };
    a.update(&k);
    let line = format!(
        "status {} {} {} {} {}",
        unit.id, a.happiness, a.hunger, a.energy, a.health
    );
    world.units.tell(line);
}

/// A caught state, a set or a task occupies the animal (animals 9.3).
fn busy(unit: &Unit) -> bool {
    let sets = matches!(
        unit.runner.top(),
        Some("bSick" | "bSleep" | "bEat" | "bTestFence")
    );
    let tasks = matches!(
        unit.goal.record.task,
        102..=106 | 114 | 115 | 122 | 124 | 128 | 129 | 133 | 134
    );
    unit.caught || sets || tasks
}

/// What the bored check sees round the animal when its counter is due (animals 21.1).
fn sight(unit: &mut Unit, world: &mut World, ctx: &mut Ctx, at: usize, data: &dyn Data) -> Sight {
    match &unit.part {
        Part::Animal(a) if a.counters[BORED] == 0 => Cx {
            unit,
            world,
            ctx,
            at,
            data,
        }
        .sight(),
        _ => Sight::default(),
    }
}

/// What the bored check asked of the unit: the afraid flag, or the reset that ends a lonely chase.
fn bored_after(
    unit: &mut Unit,
    world: &mut World,
    ctx: &mut Ctx,
    at: usize,
    data: &dyn Data,
    out: &Out,
) {
    unit.frightened |= out.afraid;
    if out.lonely {
        Cx {
            unit,
            world,
            ctx,
            at,
            data,
        }
        .reset();
    }
}

/// The conditional checks of a status tick (animals 4.2).
pub(crate) fn checks(
    unit: &mut Unit,
    world: &mut World,
    ctx: &mut Ctx,
    at: usize,
    data: &dyn Data,
) {
    boxed::check(unit, world, ctx, at);
    let task = unit.goal.record.task;
    let sight = sight(unit, world, ctx, at, data);
    let meal = food::meal(unit, world, ctx.cat, data, at);
    let sets = life::sets(unit, world, data, at);
    let cannot_act = !life::can_act(unit, world, ctx.cat, at);
    let busy = busy(unit);
    let Part::Animal(a) = &mut unit.part else {
        return;
    };
    let entity = &world.zoo.entities[at];
    let Some(com) = entity.common.as_ref() else {
        return;
    };
    let key = entity.type_name.clone();
    let young = !matches!(entity.subtype.as_str(), "m" | "f" | "p");
    let (tx, ty_) = (com.x.div_euclid(64), com.y.div_euclid(64));
    let tile = world.tile_index(tx, ty_);
    let region = tile
        .and_then(|t| world.exhibits().region(t))
        .unwrap_or(GROUNDS);
    let x = Some(region).filter(|&r| r != GROUNDS && r != EXTERIOR);
    let k = keys(world, ctx.cat, at);
    let class = world.units.class.clone();
    let mut env = Env {
        exhibit: x.is_some(),
        grounds: x.is_none(),
        young,
        grass: tile.is_some_and(|t| {
            matches!(world.zoo.tiles[t].terrain, 0 | 1 | 4 | 11 | 13)
                && (0..4).all(|q| world.quadrant(t, q).is_none())
        }),
        wet: tile.is_some_and(|t| world.is_water(t)),
        cannot_act,
        now: world.units.now,
        busy,
        sight,
        tank_mode: unit.tank,
        food_task: matches!(task, 114 | 115),
        meal,
        sets,
        boost: world.units.cheat.breeding_boost
            && crate::place::bulldozer::type_of(ctx.cat, entity)
                .is_some_and(|t| crate::cheat::BREEDERS.contains(&t.name_id)),
        adult: matches!(entity.subtype.as_str(), "m" | "f"),
        baby: entity.subtype == "y",
        male: entity.subtype == "m",
        mate: match a.mate {
            0 => breed::Mate::Here,
            mate => breed::mate_of(world, at, mate),
        },
        ..Env::default()
    };
    if let Some(x) = x {
        let exhibit = world.exhibits().get(x);
        let near = region_and_neighbours(world, x);
        env.wet |= exhibit.is_some_and(|e| e.tank.is_some());
        env.show_tank = exhibit.is_some_and(|e| e.tank.is_some_and(|t| t.show));
        env.tank = exhibit.and_then(|e| e.tank);
        if a.counters[WATER] == 0 && (k.swims || k.underwater) {
            env.kinds = kinds(world, &near);
            env.stands = tile.map_or(0, |t| tank::kind(world, t));
        }
        env.size = (near.iter())
            .filter_map(|&n| world.exhibits().get(n))
            .map(|e| e.tiles)
            .sum();
    } else if a.counters[ESCAPE] == 0 {
        // The escape check sends an animal off the water or the tank to a tile of the grounds.
        env.stands = tile.map_or(0, |t| tank::kind(world, t));
        if env.stands != 0 {
            env.kinds = kinds(world, &[GROUNDS]);
        }
    }
    if a.counters[SHELTER] == 0 && k.building_use > 0 {
        let who = crate::building::Species::of(ctx.cat, &world.zoo.entities[at], a.egg);
        env.shelter = shelters(world, ctx.cat, a.building, tile, who.as_ref());
    }
    let id = unit.id;
    let cat = ctx.cat;
    let shared = std::cell::RefCell::new(&mut *world);
    let mut birth = |father: &mut Animal, rng: &mut Rng| {
        let mut world = shared.borrow_mut();
        breed::born(
            father,
            id,
            (&mut **world, cat, data),
            (at, k.baby_born_change),
            rng,
        )
    };
    let mut read = |r: Read, a: &Animal, meal: &Meal, env: &mut Env, rng: &mut Rng| {
        let mut world = shared.borrow_mut();
        match r {
            Read::Pile => env.piled = dirt::put_down(&mut world, cat, rng, id, at, &k),
            Read::Trample => {
                trample(&mut world, (tx, ty_), class.trample, id);
            }
            _ => {
                if let Some(x) = x {
                    read_exhibit(&mut world, cat, (r, x, at), (a, meal, &key), env, rng);
                }
            }
        }
    };
    let breath = a.breath;
    let mut out = run_read(a, &k, &class, &mut env, ctx.rng, &mut birth, &mut read);
    if a.breath != breath {
        world.units.tell(format!("breath {id} {}", a.breath));
    }
    let meal = std::mem::take(&mut out.meal);
    if let Some(b) = out.shelter {
        world.units.tell(format!("shelter {id} {b}"));
    }
    breed::pair(world, id, &out);
    if out.boost_used {
        world.units.cheat.breeding_boost = false;
    }
    for word in &out.log {
        if *word == "escape" {
            world.units.escapes.retain(|&e| e != id);
            world.units.escapes.push(id);
            world.units.post(ESCAPED, id);
        }
        world.units.tell(format!("{word} {id}"));
    }
    let bored = |unit: &mut Unit, world: &mut World, ctx: &mut Ctx| {
        bored_after(unit, world, ctx, at, data, &out);
    };
    escape::after(unit, world, ctx, at, data, (&out, &k, task), bored);
    for m in out.messages {
        world.units.post(m, unit.id);
        world.units.tell(format!("message {m} {}", unit.id));
    }
    for set in &out.sets {
        life::start_or_queue(unit, world, ctx, at, data, set);
    }
    if out.remove {
        ctx.remove(world, id);
    }
    if let (true, Some(x)) = (out.mourn, x) {
        dirt::mourn(world, ctx, x);
    }
    food::apply(unit, world, ctx, at, data, meal);
    if out.grow && !out.remove {
        breed::grow_up(unit, world, ctx, at, data);
    }
}
