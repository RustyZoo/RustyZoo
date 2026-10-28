use super::*;
use crate::config::Config;
use crate::entity::EntityType;
use crate::unit::Part;
use crate::unit::testing::{amphibious_world, centre, entity, world_with};
use std::sync::Arc;

fn kind(class: &'static str, name: &str, sub: &str, name_id: i32, text: &str) -> EntityType {
    EntityType {
        class,
        type_name: name.into(),
        subtype: sub.into(),
        file: None,
        config: Some(Arc::new(Config::parse(text.as_bytes()).unwrap())),
        name_id,
        footprint: [1, 1, 1],
        height: 0,
    }
}

/// Polar bears (name id 5001, family 1, genus 10, location 3, habitat 7, era 2, Digs, attractive
/// 5), sharks (5002, family 2), goats (5003, family 1).
fn zoo_types() -> Catalogue {
    let mut cat = Catalogue::default();
    let ints = |f: i32, g: i32, l: i32, h: i32, extra: &str| {
        format!(
            "[Characteristics/Integers]\ncFamily={f}\ncGenus={g}\ncLocation={l}\ncHabitat={h}\n\
             cEra=2\n{extra}[cCompatibleTerrain]\nv=0\nv=100\n"
        )
    };
    let polar = ints(
        1,
        10,
        3,
        7,
        "cUnderwater=1\ncAttractiveness=5\ncExpansionID=1\n",
    );
    cat.push(kind("animals", "polar", "", 5001, &polar));
    let shark = ints(2, 20, 4, 8, "cUnderwater=1\ncOnlyUnderwater=1\n");
    cat.push(kind("animals", "shark", "", 5002, &shark));
    cat.push(kind(
        "animals",
        "goat",
        "",
        5003,
        &ints(1, 30, 3, 7, "cTreePref=2\ncRockPref=2\n"),
    ));
    for t in ["fences", "tankwall"] {
        cat.push(kind(t, "iron", "", 0, ""));
    }
    cat
}

/// Bears 1 in the tank and 3 on the land, shark 2 in the tank, goat 4 on the land.
fn world() -> World {
    let animals = [
        (1, "polar", (3, 3)),
        (2, "shark", (4, 3)),
        (3, "polar", (7, 3)),
        (4, "goat", (8, 3)),
    ];
    crate::unit::testing::gated(amphibious_world(2, &animals))
}

struct Parts {
    world: World,
    cat: Catalogue,
    money: Money,
    status: Status,
}

impl Parts {
    fn new(world: World, cat: Catalogue) -> Parts {
        let (money, status) = (Money::default(), Status::default());
        Parts {
            world,
            cat,
            money,
            status,
        }
    }

    /// The value of a rule with the goal's `value` 0, in January of the first year.
    fn read(&self, rule: [i32; 4]) -> i32 {
        self.value(rule, 0)
    }

    fn value(&self, rule: [i32; 4], value: i32) -> i32 {
        let game = Game {
            world: &self.world,
            cat: &self.cat,
            money: &self.money,
            status: &self.status,
        };
        eval(
            &rule,
            value,
            &Env::new(1, 0, Some(game)),
            &[],
            &Gui::default(),
        )
    }

    fn animal(&mut self, id: u32) -> &mut crate::animal::Animal {
        match &mut self.world.units.get_mut(id).unwrap().part {
            Part::Animal(a) => a,
            _ => panic!("not an animal"),
        }
    }
}

#[test]
fn rule_0_reads_the_accounts_truncated_and_an_unknown_category_or_table_reads_0() {
    let mut p = Parts::new(world(), zoo_types());
    p.money.month = 3;
    p.money.year = 1;
    p.money.monthly[3][2] = -1.7;
    p.money.yearly[3][1] = 250.9;
    p.money.total[3] = 9e12;
    p.money.monthly[14][2] = f32::NAN;
    assert_eq!([0, 1, 2].map(|t| p.read([0, 3, t, 0])), [-1, 250, i32::MAX]);
    assert_eq!(p.read([0, 14, 0, 0]), 0, "no number reads 0");
    for (b, t) in [
        (-1, 0),
        (31, 0),
        (i32::MAX, 0),
        (3, 3),
        (3, -1),
        (3, i32::MIN),
    ] {
        assert_eq!(p.read([0, b, t, 0]), 0, "category {b} table {t}");
    }
    // A game that ran for years: the month and year columns are cut to the tables.
    p.money.month = 99;
    p.money.year = 99;
    p.money.monthly[3][11] = 5.0;
    p.money.yearly[3][19] = 6.0;
    assert_eq!([0, 1].map(|t| p.read([0, 3, t, 0])), [5, 6]);
}

#[test]
fn rule_1_reads_the_zoo_status_after_month_arga_and_cash_rounded() {
    let mut p = Parts::new(world(), zoo_types());
    p.status.rating = 61;
    p.status.sums.animals = 4;
    p.status.sums.animal_happiness = 200;
    p.status.sums.guests = 2;
    p.status.sums.guest_happiness = -200;
    (p.status.sums.sick, p.status.sums.species) = (1, 7);
    p.money.cash = 2.5;
    let read = |p: &Parts, arga| (0..=7).map(|b| p.read([1, b, arga, 0])).collect::<Vec<_>>();
    assert_eq!(read(&p, 0), [75, 0, 61, 2, 4, 1, 7, 3]);
    assert_eq!(read(&p, 1), [75, 0, 61, 2, 4, 1, 7, 3], "month 1 is now");
    assert_eq!(read(&p, 2), [0; 8], "month 2 is later");
    assert_eq!(read(&p, i32::MAX), [0; 8]);
    assert_eq!(read(&p, i32::MIN)[2], 61);
    for b in [8, 9, 10, 11, 12, -1, i32::MAX] {
        assert_eq!(p.read([1, b, 0, 0]), 0, "ruleb {b} reads 0");
    }
    for (cash, want) in [
        (-2.5, -3),
        (f32::NAN, 0),
        (1e20, i32::MAX),
        (-1e20, i32::MIN),
    ] {
        p.money.cash = cash;
        assert_eq!(p.read([1, 7, 0, 0]), want, "cash {cash}");
    }
}

#[test]
fn rule_2_counts_every_entity_by_name_id_and_subtype() {
    let mut cat = Catalogue::default();
    for (name, sub, id) in [
        ("polar", "m", 5001),
        ("polar", "f", 5001),
        ("zebra", "y", 5002),
    ] {
        cat.push(kind("animals", name, sub, id, ""));
    }
    cat.push(kind("bush", "tree", "", 7, ""));
    let world = world_with((4, 4), |z| {
        for (i, (class, name, sub)) in [
            ("animals", "polar", "m"),
            ("animals", "polar", "m"),
            ("animals", "polar", "f"),
            ("animals", "zebra", "y"),
            ("bush", "tree", ""),
            ("bush", "unknown", ""),
        ]
        .into_iter()
        .enumerate()
        {
            let mut e = entity(class, name, i as u32 + 1, centre((i as i32 % 4, 1)), 0);
            e.subtype = sub.into();
            z.entities.push(e);
        }
    });
    let p = Parts::new(world, cat);
    // ruleb 0 the name id, 1 the subtype (m f p y = 0 to 3), 2 both, 3 either.
    let counts = |b, id, sub| p.read([2, b, id, sub]);
    assert_eq!(counts(0, 5001, 9), 3);
    assert_eq!(counts(0, 7, 0), 1, "every class counts");
    assert_eq!(counts(1, 0, 0), 2);
    assert_eq!(counts(1, 0, 3), 1);
    assert_eq!(counts(2, 5001, 0), 2);
    assert_eq!(counts(2, 5001, 3), 0);
    assert_eq!(counts(3, 5001, 3), 4);
    for (b, id, sub) in [
        (4, 5001, 0),
        (-1, 5001, 0),
        (1, 0, 4),
        (1, 0, -1),
        (0, i32::MAX, 0),
    ] {
        assert_eq!(counts(b, id, sub), 0, "{b} {id} {sub}");
    }
    assert_eq!(
        Parts::new(world_with((2, 2), |_| {}), Catalogue::default()).read([2, 0, 0, 0]),
        0
    );
}

#[test]
fn rule_3_counts_the_exhibits_with_the_grounds_in_the_list() {
    let p = Parts::new(world(), zoo_types());
    // The tank and the land exhibit, and the grounds.
    assert_eq!(p.read([3, 0, 0, 0]), 3);
    assert_eq!(p.read([3, 1, 0, 0]), 2);
    // Species: bear, shark, goat; families 1 and 2. Attractiveness: 5 + 1 and 5 + 1 over two.
    assert_eq!([2, 3, 4].map(|b| p.read([3, b, 0, 0])), [3, 2, 6]);
    for b in [5, -1, i32::MAX] {
        assert_eq!(p.read([3, b, 0, 0]), 0);
    }
    let empty = Parts::new(world_with((4, 4), |_| {}), zoo_types());
    assert_eq!(
        [0, 1, 2, 3, 4].map(|b| empty.read([3, b, 0, 0])),
        [1, 0, 0, 0, 0]
    );
}

#[test]
fn rule_4_counts_animals_by_type_state_and_expansion() {
    let mut p = Parts::new(world(), zoo_types());
    let n = |p: &Parts, b, x, y| p.read([4, b, x, y]);
    // By family, genus, species, location and habitat type.
    let single = [(0, 1), (0, 2), (1, 10), (2, 5001), (3, 3), (4, 7), (4, 9)];
    assert_eq!(single.map(|(b, x)| n(&p, b, x, 0)), [3, 1, 2, 2, 3, 3, 0]);
    // The pairs: family and location, genus and location, family and habitat, genus and habitat,
    // habitat and location.
    let pairs = [
        (5, 1, 3),
        (5, 1, 4),
        (6, 10, 3),
        (7, 1, 7),
        (8, 30, 7),
        (9, 7, 3),
        (9, 8, 4),
    ];
    assert_eq!(pairs.map(|(b, x, y)| n(&p, b, x, y)), [3, 0, 2, 3, 1, 3, 1]);
    // Crated and eggs, from the units; escapes are not modelled.
    p.animal(3).crated = true;
    p.animal(4).egg = true;
    let state = [
        (10, 0),
        (11, 1),
        (11, 2),
        (12, 10),
        (13, 0),
        (14, 1),
        (15, 30),
        (15, 10),
    ];
    assert_eq!(state.map(|(b, x)| n(&p, b, x, 0)), [1, 1, 0, 1, 1, 1, 1, 0]);
    assert_eq!([16, 17, 18].map(|b| n(&p, b, 1, 0)), [0; 3]);
    // The expansion of the type: bears 1, the rest 0.
    assert_eq!(
        [n(&p, 19, 1, 0), n(&p, 19, 0, 0), n(&p, 19, 2, 0)],
        [2, 2, 0]
    );
    for b in [20, -1, i32::MAX, i32::MIN] {
        assert_eq!(n(&p, b, 1, 0), 0, "ruleb {b}");
    }
}

#[test]
fn rule_16_counts_the_tanks_by_wall_height() {
    let p = Parts::new(world(), zoo_types());
    // One tank, 2 steps high; the land exhibit is not a tank. `ruleb` 0 is at most, 1 at least,
    // 2 exactly.
    let n = |b, h| p.read([16, b, h, 0]);
    assert_eq!([n(0, 2), n(1, 2), n(2, 2)], [1, 1, 1]);
    assert_eq!([n(0, 1), n(1, 1), n(2, 1)], [0, 1, 0]);
    assert_eq!([n(0, 3), n(1, 3), n(2, 3)], [1, 0, 0]);
    assert_eq!(
        [n(0, -5), n(1, -5), n(0, i32::MAX), n(1, i32::MIN)],
        [0, 1, 1, 1]
    );
    assert_eq!(n(9, 2), 0, "another ruleb compares for equality");
}

#[test]
fn rules_5_and_6_average_the_suitability_of_the_animals_of_each_exhibit() {
    let p = Parts::new(world(), zoo_types());
    let shown = |p: &Parts, id| {
        let at = p.world.find(id).unwrap();
        i64::from(crate::panel::animal::suitability(&p.world, &p.cat, at))
    };
    let (bear_tank, shark, bear_land, goat) =
        (shown(&p, 1), shown(&p, 2), shown(&p, 3), shown(&p, 4));
    let (tank, land) = ((bear_tank + shark) / 2, (bear_land + goat) / 2);
    assert!(
        bear_tank != shark && bear_land != goat,
        "{bear_tank} {shark} {bear_land} {goat}"
    );
    let value = |p: &Parts, rule, v| p.value(rule, i32::try_from(v).unwrap());
    // Rule 5: `value` while every exhibit with a matching animal averages it or more.
    let low = tank.min(land);
    assert_eq!(value(&p, [5, 0, 0, 0], low), low as i32);
    assert_eq!(value(&p, [5, 0, 0, 0], low + 1), 0);
    // By family: the bears only (family 1) and the goat; by species name id.
    let polar = bear_tank.min(bear_land);
    assert_eq!(value(&p, [5, 3, 5001, 0], polar), polar as i32);
    assert_eq!(value(&p, [5, 3, 5001, 0], polar + 1), 0);
    assert_eq!(
        value(&p, [5, 1, 1, 0], 0),
        0,
        "a value of 0 reads 0 as the rule does"
    );
    // No exhibit with a matching animal, an unknown filter: 0.
    assert_eq!(value(&p, [5, 3, 9999, 0], -1000), 0);
    // A kind outside 1 to 6 filters nothing, as 0 does.
    for b in [7, -1, i32::MAX, i32::MIN] {
        assert_eq!(value(&p, [5, b, 1, 0], low), low as i32, "ruleb {b}");
        assert_eq!(value(&p, [5, b, 1, 0], low + 1), 0, "ruleb {b}");
    }
    // Rule 6: `arga` exhibits average `value` or more, the filter value is `argb`. The tank
    // keeps its shark only; the land keeps the bears (the tank's too) and the goat, all rated
    // against it.
    let (t, l) = exhibit_ids(&p);
    let rated = |id: u32, key: &str| {
        i64::from(crate::panel::animal::suitability_in(
            &p.world, &p.cat, id, key,
        ))
    };
    let tank = rated(t, "shark");
    let land = (2 * rated(l, "polar") + rated(l, "goat")) / 3;
    let (best, worst) = (tank.max(land), tank.min(land));
    assert_eq!(value(&p, [6, 0, 1, 0], best), best as i32);
    assert_eq!(value(&p, [6, 0, 1, 0], best + 1), 0);
    assert_eq!(value(&p, [6, 0, 2, 0], worst), worst as i32);
    assert_eq!(value(&p, [6, 0, 2, 0], worst + 1), 0);
    assert_eq!(value(&p, [6, 0, 3, 0], -1000), 0, "only two exhibits");
    let bears = rated(l, "polar");
    assert_eq!(value(&p, [6, 3, 1, 5001], bears), bears as i32);
    assert_eq!(value(&p, [6, 3, 1, 5001], bears + 1), 0);
    assert_eq!(value(&p, [6, 3, 1, 9999], -1000), 0);
}

#[test]
fn rules_5_and_6_average_eggs_and_crated_animals() {
    let mut p = Parts::new(world(), zoo_types());
    let shown = |p: &Parts, id| {
        let at = p.world.find(id).unwrap();
        i64::from(crate::panel::animal::suitability(&p.world, &p.cat, at))
    };
    let (tank, land) = (
        (shown(&p, 1) + shown(&p, 2)) / 2,
        (shown(&p, 3) + shown(&p, 4)) / 2,
    );
    let low = tank.min(land) as i32;
    // The bears are crated and the goat is an egg: no state is tested, all of them stay.
    for id in [1, 3] {
        p.animal(id).crated = true;
    }
    p.animal(4).egg = true;
    assert_eq!(p.value([5, 0, 0, 0], low), low);
    assert_eq!(p.value([5, 0, 0, 0], low + 1), 0);
    assert_eq!(
        p.value([5, 3, 5003, 0], low.min(shown(&p, 4) as i32)),
        shown(&p, 4).min(low as i64) as i32
    );
    assert_eq!(p.value([6, 0, 2, 0], low), low);
    assert_eq!(p.value([6, 0, 2, 0], low + 1), 0);
    // Only crated bears and an egg stand there: a filter on them still finds the exhibits.
    assert_eq!(p.value([5, 3, 5001, 0], -1000), -1000);
    assert_eq!(p.value([6, 3, 1, 5003], -1000), -1000);
}

/// The ids of the tank and the land exhibit of [`world`].
fn exhibit_ids(p: &Parts) -> (u32, u32) {
    let list = p.world.exhibits().list();
    let tank = list.iter().find(|x| x.tank.is_some()).unwrap().id;
    (tank, list.iter().find(|x| x.tank.is_none()).unwrap().id)
}

#[test]
fn rule_6_keeps_the_animals_whose_home_is_the_exhibit() {
    let mut p = Parts::new(world(), zoo_types());
    // The bears are amphibious and the land beside their tank is their home; the shark and the
    // goat have their own. `arga` 2 asks for both exhibits, the value -1000 only counts them.
    let n = |p: &Parts, b, x, arga| p.value([6, b, arga, x], -1000);
    // The shark is kept by the tank only, the goat by the land only: one exhibit each.
    assert_eq!([n(&p, 3, 5002, 1), n(&p, 3, 5002, 2)], [-1000, 0]);
    assert_eq!([n(&p, 3, 5003, 1), n(&p, 3, 5003, 2)], [-1000, 0]);
    // The bears are the land's, even those that swim in the tank.
    assert_eq!([n(&p, 3, 5001, 1), n(&p, 3, 5001, 2)], [-1000, 0]);
    // With no gate on the land the bears have no home: the tank keeps its own.
    let (_, land) = exhibit_ids(&p);
    p.world.exhibits_mut().get_mut(land).unwrap().gate = None;
    assert_eq!(n(&p, 3, 5001, 2), -1000);
    // The land gathers the tank's animals too: the shark has its home in the tank, so the land
    // still ignores it.
    assert_eq!(n(&p, 3, 5002, 2), 0);
}

#[test]
fn rule_6_needs_one_counted_exhibit() {
    let p = Parts::new(world(), zoo_types());
    // No animal passes the filter: no exhibit has an average, whatever `arga`.
    for arga in [0, -1, i32::MIN, 1, i32::MAX] {
        assert_eq!(p.value([6, 3, arga, 9999], -1000), 0, "arga {arga}");
    }
    // One exhibit counted: `arga` of 0 or less asks for that one only.
    for arga in [0, -1, i32::MIN, 1] {
        assert_eq!(p.value([6, 3, arga, 5003], -1000), -1000, "arga {arga}");
    }
    assert_eq!(p.value([6, 3, i32::MAX, 5003], -1000), 0);
    // Counted, but below the value.
    assert_eq!(p.value([6, 3, 0, 5003], i32::MAX), 0);
}

#[test]
fn rule_10_answers_a_press_for_every_non_zero_ruleb() {
    let env = Env::new(1, 0, None);
    let gui = Gui::default();
    let ask = |b| eval(&[10, b, 5, 0], 7, &env, &[], &gui);
    assert_eq!(ask(2), 0, "the first question makes the entry, not a press");
    gui.pressed(5, None);
    assert_eq!(ask(2), 7, "ruleb 2 answers the press");
    assert_eq!(ask(2), 0, "the press is consumed");
    gui.pressed(5, None);
    assert_eq!(ask(-1), 7, "a negative ruleb is a press too");
    gui.pressed(5, None);
    assert_eq!(ask(0), 0, "ruleb 0 reads the shown state, not a press");
    gui.sync(&|id| id == 5, &|_| Vec::new());
    assert_eq!(ask(0), 7, "ruleb 0 reads shown");
    assert_eq!(ask(1), 7, "ruleb 0 did not consume the press");
}

#[test]
fn rules_that_read_the_game_read_0_without_one() {
    let env = Env::new(1, 0, None);
    for rule in [0, 1, 2, 3, 4, 5, 6, 16] {
        assert_eq!(
            eval(&[rule, 0, 0, 0], 0, &env, &[], &Gui::default()),
            0,
            "rule {rule}"
        );
    }
}
