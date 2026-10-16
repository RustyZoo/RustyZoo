//! The creation of a guest, the arrival table and ten minutes at the entrance.

use super::*;
use crate::config::Config;
use crate::sim::drive::Run;
use crate::status::zoo::{Settings, Status};
use crate::unit::testing::{centre, entity, world_with};
use crate::zoo::StatusRecord;
use std::sync::Arc;

fn config(text: &str) -> Arc<Config> {
    Arc::new(Config::parse(text.as_bytes()).unwrap())
}

fn kind(class: &'static str, name: &str, text: &str, name_id: i32) -> EntityType {
    EntityType {
        class,
        type_name: name.as_bytes().to_vec(),
        subtype: Vec::new(),
        file: None,
        config: Some(config(text)),
        name_id,
        footprint: [1, 1, 1],
        height: 0,
    }
}

/// The keys of the shipped guest file that matter here, with a start stat for every second guest.
const GUESTS: &str = "[Global]\nDefaultSubtype=man\n\
    [Characteristics/Integers]\ncRandomStatNth=2\ncRandomHungerMin=41\ncRandomHungerMax=50\n\
    cRandomThirstMin=41\ncRandomThirstMax=50\ncRandomEnergyMin=0\ncRandomEnergyMax=0\n\
    [man/Characteristics/Integers]\ncInitialHappiness=50\ncHungerCheck=30\ncThirstyCheck=30\n\
    cBathroomCheck=30\ncLeaveZooCheck=20\ncBuySouvenirCheck=40\ncEnergyCheck=30\ncChaseCheck=5\n\
    cTrashCheck=30\ncLikeAnimalsCheck=20\ncViewingAreaCheck=20\ncEnvironmentEffectCheck=20\n\
    cSawAnimalReset=20\n\
    [woman/Characteristics/Integers]\ncInitialHappiness=55\n";

fn man() -> EntityType {
    kind("guests", "man", GUESTS, 9503)
}

/// The four guest types and a zebra of name id 5.
fn types() -> Catalogue {
    let mut cat = Catalogue::default();
    for (name, id) in [
        ("boy", 9505),
        ("girl", 9506),
        ("man", 9503),
        ("woman", 9504),
    ] {
        cat.push(kind("guests", name, GUESTS, id));
    }
    cat.push(kind(
        "animals",
        "zebra",
        "[Characteristics/Integers]\ncInitialHappiness=0\n",
        5,
    ));
    cat
}

fn date(month: u32, day: u32) -> Date {
    Date {
        month,
        day,
        ..crate::timing::Calendar::new().date()
    }
}

/// How many numbers the generator `after` has drawn since `before`.
fn drawn(before: &Rng, after: &Rng) -> usize {
    let mut r = before.clone();
    let mut n = 0;
    while r != *after && n < 100 {
        r.rand();
        n += 1;
    }
    n
}

fn seeded(seed: u32) -> Rng {
    let mut r = Rng::default();
    r.seed(seed);
    r
}

#[test]
fn the_keys_come_from_the_type_then_the_default_type_then_the_plain_section() {
    let k = Keys::of(&man());
    assert_eq!(
        (k.initial_happiness, k.max_energy, k.saw_animal_reset),
        (50, 100, 20)
    );
    assert_eq!(k.periods, [30, 30, 30, 20, 40, 30, 5, 30, 20, 20, 20]);
    assert_eq!(k.random_nth, 2);
    assert_eq!(k.random, [(41, 50), (41, 50), (0, 0), (0, 0)]);
    let woman = Keys::of(&kind("guests", "woman", GUESTS, 9504));
    assert_eq!((woman.initial_happiness, woman.periods[0]), (55, 30));
    // A type without a file has the program's values.
    let mut bare = man();
    bare.config = None;
    let k = Keys::of(&bare);
    assert_eq!((k.initial_happiness, k.random_nth), (40, 0));
    assert_eq!(k.periods, [5, 5, 10, 20, 20, 10, 2, 20, 10, 10, 10]);
}

#[test]
fn a_guest_draws_colours_the_start_stat_sex_countdowns_favourite_and_speed_in_order() {
    let ty = man();
    let k = Keys::of(&ty);
    // 4 colours, 1 for the count, a stat and its value when the count says so and the stat has a
    // valid range, 2 for sex and age, 11 countdowns, the favourite and the speed.
    let mut starts = Vec::new();
    for seed in 0..60 {
        let (before, mut rng) = (seeded(seed), seeded(seed));
        let g = Guest::create(&k, &ty, &[7, 9], date(3, 4), 0, &mut rng);
        let n = drawn(&before, &rng);
        let started = g.values[1..].iter().any(|&v| v != 0);
        assert!(
            n == 22 && started || n == 21 || n == 20 && !started,
            "{seed}: {n}"
        );
        starts.push((g.values[HUNGER] != 0, g.values[THIRST] != 0));
        let range = |v: i32| v == 0 || (41..50).contains(&v);
        assert!(range(g.values[HUNGER]) && range(g.values[THIRST]));
        assert_eq!(
            (g.values[ENERGY], g.values[BATHROOM], g.values[HAPPINESS]),
            (0, 0, 50)
        );
        assert!(g.speed_offset < 11 && g.favourite.is_some_and(|f| f == 7 || f == 9));
    }
    assert!(
        [(true, false), (false, true), (false, false)]
            .iter()
            .all(|s| starts.contains(s))
    );
    // Each countdown is its draw reduced by its period, in the order of the checks.
    let (mut rng, mut check) = (seeded(5), seeded(5));
    let one = Keys { random_nth: 0, ..k };
    let g = Guest::create(&one, &ty, &[], date(3, 4), 0, &mut rng);
    (0..6).for_each(|_| {
        check.rand();
    });
    let expected: Vec<i32> = one
        .periods
        .iter()
        .map(|&p| check.rand() as i32 % p)
        .collect();
    assert_eq!(g.countdowns.to_vec(), expected);
    assert_eq!((g.favourite, drawn(&seeded(5), &rng)), (None, 18));
    // A period of 0 never fires and draws nothing; a negative one draws.
    let zero = Keys {
        periods: [0, -7, 5, 5, 5, 5, 5, 5, 5, 5, 5],
        ..one
    };
    let mut rng = seeded(5);
    let g = Guest::create(&zero, &ty, &[], date(3, 4), 0, &mut rng);
    assert_eq!(
        (g.countdowns[0], (0..7).contains(&g.countdowns[1])),
        (-1, true)
    );
    assert_eq!(drawn(&seeded(5), &rng), 17);
}

#[test]
fn the_type_name_sets_sex_and_age_and_new_year_makes_a_guest_sick() {
    let k = Keys::of(&man());
    let make = |name: &str, d: Date| {
        let ty = kind("guests", name, GUESTS, 1);
        Guest::create(&k, &ty, &[], d, 0, &mut seeded(3))
    };
    let flags = |name| {
        let g = make(name, date(6, 1));
        (g.female, g.child)
    };
    assert_eq!(
        [flags("man"), flags("woman"), flags("boy"), flags("girl")],
        [(false, false), (true, false), (false, true), (true, true)]
    );
    // Another name keeps the draws.
    let other = make("other", date(6, 1));
    assert_eq!((other.female, other.child), (flags("other")));
    assert!(make("man", date(1, 1)).sick && !make("man", date(1, 2)).sick);
    assert!(!make("man", date(12, 1)).sick);
    let g = make("man", date(2, 3));
    assert_eq!(
        (g.reset, g.saw_animal, g.arrived),
        (0, 20, Some(date(2, 3)))
    );
}

#[test]
fn the_favourite_is_drawn_among_the_animal_types_that_are_not_special_by_name_id() {
    let mut cat = Catalogue::default();
    for (name, id, special) in [("a", 30, 0), ("b", 10, 1), ("c", 20, 0), ("d", 20, 0)] {
        let text = format!("[Characteristics/Integers]\ncIsSpecialAnimal={special}\n");
        cat.push(kind("animals", name, &text, id));
    }
    cat.push(kind("guests", "man", GUESTS, 3));
    assert_eq!(favourites(&cat), [20, 20, 30]);
    assert!(favourites(&Catalogue::default()).is_empty());
}

#[test]
fn every_cell_of_the_arrival_table_has_its_chance_and_its_advice() {
    // Rows: 81 and above, 60 to 80, 30 to 59, below 30; columns: the price against 49, 29, 19, 9.
    #[rustfmt::skip]
    let table = [
        [Some((0, false)), Some((2, false)), Some((3, false)), Some((3, true)), Some((4, true))],
        [None, Some((1, false)), Some((2, false)), Some((3, false)), Some((4, true))],
        [None, Some((0, false)), Some((2, false)), Some((2, false)), Some((3, true))],
        [None, None, Some((0, false)), Some((1, false)), Some((2, false))],
    ];
    #[rustfmt::skip]
    let rows = [(i32::MAX, 0), (81, 0), (80, 1), (60, 1), (59, 2), (30, 2), (29, 3), (-5, 3), (i32::MIN, 3)];
    #[rustfmt::skip]
    let prices = [(49.01, 0), (49.0, 1), (29.01, 1), (29.0, 2), (19.01, 2), (19.0, 3), (9.01, 3), (9.0, 4), (0.0, 4), (-1.0, 4), (f32::MAX, 0), (f32::NAN, 4)];
    for (rating, r) in rows {
        for (price, column) in prices {
            for key in 0..5 {
                let mut s = Status::start(&StatusRecord::default(), None);
                s.settings.limits = [49.0, 29.0, 19.0, 9.0];
                s.settings.create_chance = [0; 5];
                s.settings.create_chance[key] = 100;
                (s.rating, s.price) = (rating, price);
                let mut rng = seeded(1);
                let got = s.new_guest_check(true, &mut rng);
                let want = table[r][column]
                    .filter(|&(k, _)| k == key)
                    .map(|(_, advice)| advice);
                assert_eq!(got, want, "rating {rating} price {price} key {key}");
                assert_eq!(s.column, column as i32);
                // A chance of 0 draws nothing; any other chance one number.
                assert_eq!(
                    drawn(&seeded(1), &rng),
                    usize::from(table[r][column].is_some_and(|(k, _)| k == key))
                );
            }
        }
    }
    let mut s = Status::start(&StatusRecord::default(), None);
    assert_eq!(s.column, 2, "the column starts at 2");
    s.timers[2] = 5;
    assert_eq!(s.new_guest_check(false, &mut seeded(1)), None);
    assert_eq!(
        (s.column, s.timers[2]),
        (2, 0),
        "no occupied exhibit: nothing, no column"
    );
}

#[test]
fn the_economy_file_gives_the_chances_the_multiple_and_the_limits() {
    let knobs = |s: &Settings| (s.limits, s.create_chance, s.admission_multiple);
    let mut s = Status::start(&StatusRecord::default(), None).settings;
    assert_eq!(knobs(&s), ([50.0, 20.0, 10.0, 5.0], [1; 5], 1.0));
    let mut file =
        "[Characteristics]\ncadmissionmultiple=3.5\ncCreateGuestChanceVeryLow=3\n".to_string();
    file += "cCreateGuestChanceVeryHigh=40\n";
    file += "CADULTADMISSION = 49\nCADULTADMISSION = 29\nCADULTADMISSION = 19\n\
              CADULTADMISSION = 9\nCADULTADMISSION = 0\n";
    s.apply(&Config::parse(file.as_bytes()).unwrap());
    assert_eq!(knobs(&s), ([49.0, 29.0, 19.0, 9.0], [3, 1, 1, 1, 40], 3.5));
    // Four values, six values or none keep the limits.
    for n in [0, 4, 6] {
        let mut s = Status::start(&StatusRecord::default(), None).settings;
        let file = "cAdultAdmission = 7\n".repeat(n);
        s.apply(&Config::parse(format!("[characteristics]\n{file}").as_bytes()).unwrap());
        assert_eq!(s.limits, [50.0, 20.0, 10.0, 5.0], "{n} values");
    }
}

/// A pen of 3 by 3 tiles from (1, 1) with a zebra of id 500 in it, and a zoo entrance at (7, 7);
/// the zebra is left out of an empty one.
fn zoo(zebra: bool) -> World {
    world_with((8, 8), |zoo| {
        zoo.entrance = (7, 7);
        for i in 1..=3 {
            for (at, d) in [((i, 1), 0), ((i, 3), 4), ((1, i), 6), ((3, i), 2)] {
                let id = 100 + zoo.entities.len() as u32;
                zoo.entities.push(entity("fences", "", id, centre(at), d));
            }
        }
        if zebra {
            zoo.entities
                .push(entity("animals", "zebra", 500, centre((2, 2)), 0));
        }
    })
}

const ECONOMY: &[u8] = b"[checks]\nrating=100000000\nnewguest=2000\n[characteristics]\n\
    cAdultAdmission=49\ncAdultAdmission=29\ncAdultAdmission=19\ncAdultAdmission=9\n\
    cAdultAdmission=0\ncAdmissionMultiple=3.5\ncCreateGuestChanceVeryLow=3\n\
    cCreateGuestChanceLow=5\ncCreateGuestChanceMed=25\ncCreateGuestChanceHigh=30\n\
    cCreateGuestChanceVeryHigh=40\n";

/// A game on the pen from the ini text `ini`, with the rating and the price given.
fn game(ini: &str, zebra: bool) -> Run {
    let cat = (types(), Default::default());
    let run = Run::new(
        (ini.as_bytes(), ECONOMY),
        cat,
        zoo(zebra),
        Money::default(),
        1,
    );
    assert_eq!(run.world.exhibits().list().len(), 1);
    run
}

/// The same run for `ms` milliseconds of 67 ms steps, with the rating kept (the month change
/// checks it out of turn).
fn run_for(ms: u32, zebra: bool, (rating, price): (i32, f32)) -> (Run, Vec<String>) {
    let mut run = game("", zebra);
    run.sim.status.price = price;
    let mut log = Vec::new();
    for _ in 0..ms / 67 {
        run.sim.status.rating = rating;
        log.extend(run.step(67));
    }
    (run, log)
}

fn added(log: &[String]) -> Vec<&str> {
    log.iter()
        .filter_map(|l| l.strip_prefix("guest add "))
        .collect()
}

#[test]
fn ten_minutes_at_the_entrance_create_guests_at_the_rate_of_the_rating_and_the_price() {
    // Rating 85 and price 9 is the last column: 40 percent at each of the 298 checks.
    let (run, log) = run_for(600_000, true, (85, 9.0));
    let made = added(&log);
    assert!((90..150).contains(&made.len()), "{}", made.len());
    let adults = made.iter().filter(|l| l.ends_with(" adult")).count();
    let children = made.len() - adults;
    assert!(adults > 0 && children > 0);
    // The admissions are booked: a child pays half, the multiple is 3.5, and it is counted.
    let m = &run.money;
    let income = (adults as f32 + children as f32 / 2.0) * 9.0 * 3.5;
    assert!((m.total[money::ADMISSIONS_INCOME] - income).abs() < 0.5);
    assert!((m.cash - income).abs() < 0.5, "the cash is the income");
    assert_eq!(m.total[money::ADMISSIONS] as usize, made.len());
    assert_eq!(m.total[money::GUESTS_CREATED] as usize, made.len());
    assert_eq!(m.total[money::NET], m.total[money::ADMISSIONS_INCOME]);
    let months: f32 = m.monthly[money::ADMISSIONS].iter().sum();
    assert_eq!(
        months,
        m.total[money::ADMISSIONS],
        "a month changed in ten minutes"
    );
    assert!(m.monthly[money::ADMISSIONS][0] > 0.0 && m.monthly[money::ADMISSIONS][1] > 0.0);
    // Each guest stands in the entities with its values, a name and the entrance tile.
    let guests: Vec<_> = run
        .world
        .zoo
        .entities
        .iter()
        .filter(|e| e.class == "guests")
        .collect();
    assert_eq!(guests.len(), made.len());
    assert!(guests[0].common.as_ref().unwrap().name.ends_with(" 1"));
    assert_eq!(guests[1].common.as_ref().unwrap().name, "Guest 2");
    assert_eq!(run.sim.status.column, 4);
    // Two runs agree.
    let (again, log2) = run_for(600_000, true, (85, 9.0));
    assert!(log == log2 && run.hash() == again.hash());
}

#[test]
fn no_guest_comes_while_no_exhibit_holds_an_animal() {
    let (run, log) = run_for(60_000, false, (85, 9.0));
    assert!(added(&log).is_empty());
    assert_eq!((run.money.cash, run.sim.status.column), (0.0, 2));
}

#[test]
fn the_last_sums_limit_the_guests_and_ten_guests_are_too_few_for_the_advice() {
    let run_with = |guests: i32, max: i32| {
        let mut run = game(&format!("[AI]\nmaxGuests={max}\n"), true);
        run.sim.status.settings.create_chance = [100; 5];
        (run.sim.status.rating, run.sim.status.price) = (85, 9.0);
        run.sim.status.sums.guests = guests;
        let log: Vec<String> = (0..31).flat_map(|_| run.step(67)).collect();
        (run.sim.take_messages(), log)
    };
    let (said, log) = run_with(11, 1000);
    assert_eq!(added(&log).len(), 1);
    assert_eq!(said, [(10017, None)]);
    assert!(log.contains(&"sim message 10017".to_string()));
    let (said, log) = run_with(10, 1000);
    assert!(
        added(&log).len() == 1 && said.is_empty(),
        "10 guests: no advice"
    );
    let (_, log) = run_with(5, 5);
    assert!(added(&log).is_empty(), "at the limit nobody comes");
    let (_, log) = run_with(4, 5);
    assert_eq!(added(&log).len(), 1);
    // A limit of 0 or less lets nobody in; a huge one every one.
    assert!(added(&run_with(0, 0).1).is_empty() && added(&run_with(0, -1).1).is_empty());
    assert_eq!(added(&run_with(999, i32::MAX).1).len(), 1);
}

#[test]
fn without_a_guest_type_or_an_entrance_tile_nothing_is_made_and_nothing_is_drawn() {
    let make = |cat: &Catalogue, entrance: (u32, u32)| {
        let mut world = zoo(true);
        world.zoo.entrance = entrance;
        let (mut rng, mut money) = (seeded(1), Money::default());
        let a = Arrival {
            price: 9.0,
            multiple: 3.5,
            guests: 0,
            max_guests: 1000,
            date: date(3, 4),
            reset: 0,
        };
        let made = arrive(&mut world, cat, &mut rng, &mut money, &a);
        (made, drawn(&seeded(1), &rng), money.cash)
    };
    assert!(make(&types(), (7, 7)).0);
    assert_eq!(make(&Catalogue::default(), (7, 7)), (false, 0, 0.0));
    assert_eq!(make(&types(), (0, 0)), (false, 0, 0.0));
    assert_eq!(make(&types(), (8, 0)), (false, 0, 0.0), "off the map");
    assert_eq!(make(&types(), (u32::MAX, u32::MAX)), (false, 0, 0.0));
    // One creation: the type, 21 or 22 for the guest, the tile.
    let n = make(&types(), (7, 7)).1;
    assert!((1 + 20 + 1..=1 + 22 + 1).contains(&n), "{n}");
}
