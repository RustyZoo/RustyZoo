//! What the rules of a simple goal read from the game (scenarios 6): the accounts, the zoo
//! status, the exhibits and the entity list.
//!
//! The figures that take a walk over the entities or the exhibits are made once per pass, the
//! first time a rule asks, and kept in the [`Env`].
//!
//! Not read yet, they give 0: rule 8 (research makes types available), the interface rules 10
//! and 11, rule 13 (research), rule 15 (shows), rule 1 with `ruleb` 11 (awards), the escaped
//! animals of rule 4, and the rules and `ruleb` values no shipped file uses.

use std::cell::OnceCell;
use std::collections::{BTreeMap, BTreeSet, HashMap};

use super::Env;
use crate::build::{number, plain};
use crate::entity::Catalogue;
use crate::exhibit::rating;
use crate::money::{CATEGORIES, MONTHS, Money, YEARS};
use crate::panel::animal::suitability;
use crate::panel::exhibit::animals_exhibit;
use crate::status::zoo::Status;
use crate::world::World;

/// The parts of the game the rules read.
#[derive(Clone, Copy)]
pub struct Game<'a> {
    pub world: &'a World,
    pub cat: &'a Catalogue,
    pub money: &'a Money,
    pub status: &'a Status,
}

/// What an animal's type says, for the rules that count and average animals.
struct Animal {
    family: i32,
    genus: i32,
    /// The type's name id: the species key.
    species: i32,
    location: i32,
    habitat: i32,
    era: i32,
    expansion: i32,
    crated: bool,
    egg: bool,
}

/// One entity of the list with a known type.
struct Fact {
    at: usize,
    name_id: i32,
    /// 0 to 3 for the subtypes `m`, `f`, `p` and `y`.
    subtype: Option<i32>,
    animal: Option<Animal>,
}

/// An exhibit as the goals count it.
struct Summary {
    animals: usize,
    keys: Vec<String>,
    families: Vec<i32>,
    attractiveness: i64,
}

/// The animals that stand in an exhibit, with the suitability the animal panel shows: an index
/// into the facts and the figure.
type Homes = BTreeMap<u32, Vec<(usize, i32)>>;

/// The figures made once per pass.
#[derive(Default)]
pub(super) struct Cache {
    facts: OnceCell<Vec<Fact>>,
    exhibits: OnceCell<Vec<Summary>>,
    homes: OnceCell<Homes>,
}

fn whole(v: i64) -> i32 {
    v.clamp(i64::from(i32::MIN), i64::from(i32::MAX)) as i32
}

fn bytes(s: &str) -> Vec<u8> {
    s.chars().map(|c| c as u8).collect()
}

fn facts<'e>(env: &'e Env, g: &Game) -> &'e [Fact] {
    env.cache.facts.get_or_init(|| {
        let int = |ty, key| number(ty, key).unwrap_or(0.0) as i32;
        let entities = g.world.zoo.entities.iter().enumerate();
        entities
            .filter_map(|(at, e)| {
                let ty = g
                    .cat
                    .find(&e.class, &bytes(&e.type_name), &bytes(&e.subtype))?;
                let unit = e.common.as_ref().and_then(|c| g.world.units.get(c.id));
                let state = match unit.map(|u| &u.part) {
                    Some(crate::unit::Part::Animal(a)) => (a.crated, a.egg),
                    _ => (false, false),
                };
                Some(Fact {
                    at,
                    name_id: ty.name_id,
                    subtype: match &ty.subtype[..] {
                        [c] => b"mfpy".iter().position(|s| s == c).map(|p| p as i32),
                        _ => None,
                    },
                    animal: (e.class == "animals").then(|| Animal {
                        family: int(ty, "cFamily"),
                        genus: int(ty, "cGenus"),
                        species: ty.name_id,
                        location: int(ty, "cLocation"),
                        habitat: int(ty, "cHabitat"),
                        era: int(ty, "cEra"),
                        expansion: plain(ty, "cExpansionID").unwrap_or(0.0) as i32,
                        crated: state.0,
                        egg: state.1,
                    }),
                })
            })
            .collect()
    })
}

fn exhibits<'e>(env: &'e Env, g: &Game) -> &'e [Summary] {
    env.cache.exhibits.get_or_init(|| {
        let list = g.world.exhibits().list();
        let inputs = list.iter().map(|x| rating::input(g.world, g.cat, x.id));
        inputs
            .map(|i| Summary {
                animals: i.animals.len(),
                keys: i.species.iter().map(|l| l.key.clone()).collect(),
                families: i.species.iter().map(|l| l.ids[2]).collect(),
                attractiveness: (i.animals.iter())
                    .filter_map(|&a| i.species.get(a))
                    .map(|l| i64::from(l.attractiveness))
                    .sum(),
            })
            .collect()
    })
}

fn homes<'e>(env: &'e Env, g: &Game) -> &'e Homes {
    env.cache.homes.get_or_init(|| {
        let mut homes = Homes::new();
        let mut seen: HashMap<(u32, &str), i32> = HashMap::new();
        for (i, f) in facts(env, g).iter().enumerate() {
            let alive = f.animal.as_ref().is_some_and(|a| !a.egg && !a.crated);
            let Some(x) = animals_exhibit(g.world, f.at).filter(|_| alive) else {
                continue;
            };
            let kind = g.world.zoo.entities[f.at].type_name.as_str();
            let shown = *seen
                .entry((x, kind))
                .or_insert_with(|| suitability(g.world, g.cat, f.at));
            homes.entry(x).or_default().push((i, shown));
        }
        homes
    })
}

/// The values of an animal that rules 4 to 6 compare: family, genus, species, location,
/// habitat type and era.
fn keys(a: &Animal) -> [i32; 6] {
    [a.family, a.genus, a.species, a.location, a.habitat, a.era]
}

/// Rule 4 (animal counts by type, state and expansion).
fn animals(env: &Env, g: &Game, rule: &[i32; 4]) -> i32 {
    let (b, x, y) = (rule[1], rule[2], rule[3]);
    // The pairs of rules 5 to 9: the key compared with `arga` and the one with `argb`.
    let pair = |a: usize, b: usize| move |k: [i32; 6]| k[a] == x && k[b] == y;
    let count = |keep: &dyn Fn(&Animal) -> bool| {
        let animals = facts(env, g).iter().filter_map(|f| f.animal.as_ref());
        animals.filter(|a| keep(a)).count() as i32
    };
    match b {
        0..=4 => count(&|a| keys(a)[b as usize] == x),
        5 => count(&|a| pair(0, 3)(keys(a))),
        6 => count(&|a| pair(1, 3)(keys(a))),
        7 => count(&|a| pair(0, 4)(keys(a))),
        8 => count(&|a| pair(1, 4)(keys(a))),
        9 => count(&|a| pair(4, 3)(keys(a))),
        10 => count(&|a| a.crated),
        11 => count(&|a| a.crated && a.family == x),
        12 => count(&|a| a.crated && a.genus == x),
        13 => count(&|a| a.egg),
        14 => count(&|a| a.egg && a.family == x),
        15 => count(&|a| a.egg && a.genus == x),
        19 => count(&|a| a.expansion == x),
        _ => 0,
    }
}

/// Rule 3 (the exhibits).
fn exhibit_count(env: &Env, g: &Game, b: i32) -> i32 {
    let all = exhibits(env, g);
    let distinct = |f: &dyn Fn(&Summary) -> Vec<String>| {
        all.iter().flat_map(f).collect::<BTreeSet<_>>().len() as i32
    };
    match b {
        // The list holds the zoo grounds besides the exhibits (scenarios O2).
        0 => all.len() as i32 + 1,
        1 => all.iter().filter(|x| x.animals > 0).count() as i32,
        2 => distinct(&|x| x.keys.clone()),
        3 => distinct(&|x| x.families.iter().map(i32::to_string).collect()),
        4 => match all.len() {
            0 => 0,
            n => whole(all.iter().map(|x| x.attractiveness).sum::<i64>() / n as i64),
        },
        _ => 0,
    }
}

/// The average of the suitability of the animals of `x` that pass the filter of rule 5 or 6
/// (`b`, `v`), or none when there is no such animal.
fn average(env: &Env, g: &Game, ids: &[u32], b: i32, v: i32) -> Option<i64> {
    let facts = facts(env, g);
    let keep = |&&(i, _): &&(usize, i32)| {
        let a = facts[i].animal.as_ref();
        let k = a.map(keys);
        match b {
            0 => true,
            1..=4 => k.is_some_and(|k| k[b as usize - 1] == v),
            5 => k.is_some_and(|k| k[5] == v),
            6 => k.is_some_and(|k| k[4] == v),
            _ => false,
        }
    };
    let homes = homes(env, g);
    let shown: Vec<i64> = (ids.iter().filter_map(|x| homes.get(x)))
        .flatten()
        .filter(keep)
        .map(|&(_, s)| i64::from(s))
        .collect();
    (!shown.is_empty()).then(|| shown.iter().sum::<i64>() / shown.len() as i64)
}

/// Rule 5: `value` when every exhibit with a matching animal averages `value` or more.
fn all_suit(env: &Env, g: &Game, rule: &[i32; 4], value: i32) -> i32 {
    let ids = homes(env, g).keys().copied();
    let averages: Vec<i64> = ids
        .filter_map(|x| average(env, g, &[x], rule[1], rule[2]))
        .collect();
    let good = !averages.is_empty() && averages.iter().all(|&a| a >= i64::from(value));
    if good { value } else { 0 }
}

/// Rule 6: `value` when `arga` exhibits, each with its show neighbours' animals, average `value`
/// or more.
fn some_suit(env: &Env, g: &Game, rule: &[i32; 4], value: i32) -> i32 {
    let exhibits = g.world.exhibits();
    let good = exhibits.list().iter().filter(|x| {
        let ids: Vec<u32> = std::iter::once(x.id)
            .chain(x.show_near.iter().copied())
            .collect();
        average(env, g, &ids, rule[1], rule[3]).is_some_and(|a| a >= i64::from(value))
    });
    if good.count() as i64 >= i64::from(rule[2]) {
        value
    } else {
        0
    }
}

/// Rule 16: the tanks whose wall height compares with `arga` as `ruleb` says.
fn tanks(g: &Game, rule: &[i32; 4]) -> i32 {
    let heights = (g.world.exhibits().list().iter()).filter_map(|x| x.tank.as_ref());
    let test = |h: i32| match rule[1] {
        0 => h <= rule[2],
        1 => h >= rule[2],
        2 => h == rule[2],
        _ => false,
    };
    heights.filter(|t| test(t.steps().0)).count() as i32
}

/// Rule 0: a figure of the accounts, truncated.
fn account(m: &Money, rule: &[i32; 4]) -> i32 {
    let Some(c) = usize::try_from(rule[1]).ok().filter(|&c| c < CATEGORIES) else {
        return 0;
    };
    let month = (m.month as usize).clamp(1, MONTHS) - 1;
    let year = (m.year as usize).min(YEARS - 1);
    let figure = match rule[2] {
        0 => m.monthly[c][month],
        1 => m.yearly[c][year],
        2 => m.total[c],
        _ => 0.0,
    };
    figure as i32
}

/// Rule 1: a figure of the zoo status, 0 before month `arga`.
fn status(env: &Env, g: &Game, rule: &[i32; 4]) -> i32 {
    if rule[2] > env.total_months() {
        return 0;
    }
    let (s, sums) = (g.status, &g.status.sums);
    let rating = |average: i32| whole((i64::from(average) + 100) * 100 / 200);
    match rule[1] {
        0 => rating(sums.animal_average()),
        1 => rating(sums.guest_average()),
        2 => s.rating,
        3 => sums.guests,
        4 => sums.animals,
        5 => sums.sick,
        6 => sums.species,
        7 => g.money.cash.round() as i32,
        _ => 0,
    }
}

/// The value a rule reads: `rule` is `rulea`, `ruleb`, `arga`, `argb`; `value` the goal's own;
/// `states` the reported state of each goal of the list.
pub(super) fn eval(rule: &[i32; 4], value: i32, env: &Env, states: &[u8]) -> i32 {
    match rule[0] {
        7 => match rule[1] {
            0 => env.month,
            1 => env.year,
            2 => env.total_months(),
            _ => 0,
        },
        9 => states.iter().filter(|&&s| i32::from(s) == rule[1]).count() as i32,
        99 => rule[2],
        _ => {
            let Some(g) = env.game else { return 0 };
            match rule[0] {
                0 => account(g.money, rule),
                1 => status(env, &g, rule),
                2 => {
                    let (id, sub) = (rule[2], rule[3]);
                    let by_name = |f: &Fact| f.name_id == id;
                    let by_sub = |f: &Fact| f.subtype == Some(sub);
                    let keep: &dyn Fn(&Fact) -> bool = match rule[1] {
                        0 => &by_name,
                        1 => &by_sub,
                        2 => &|f| by_name(f) && by_sub(f),
                        3 => &|f| by_name(f) || by_sub(f),
                        _ => &|_| false,
                    };
                    facts(env, &g).iter().filter(|f| keep(f)).count() as i32
                }
                3 => exhibit_count(env, &g, rule[1]),
                4 => animals(env, &g, rule),
                5 => all_suit(env, &g, rule, value),
                6 => some_suit(env, &g, rule, value),
                16 => tanks(&g, rule),
                _ => 0,
            }
        }
    }
}

#[cfg(test)]
mod tests;
