use super::*;
use crate::config::Config;
use crate::entity::EntityType;
use crate::unit::testing::{entity, world_with};
use crate::zoo::StatusRecord;
use std::sync::Arc;

fn config(text: &str) -> Config {
    Config::parse(text.as_bytes()).unwrap()
}

fn status(economy: &str) -> Status {
    Status::start(&StatusRecord::default(), Some(&config(economy)))
}

/// The running zoo of the specification's example 17.2 (esthetics 1.27).
fn running() -> Sums {
    Sums {
        animals: 8,
        sick: 1,
        animal_happiness: 480,
        guests: 40,
        guest_happiness: 1700,
        species: 6,
        zoo_value: 12_500,
        esthetics: 1.27,
        research: 25,
        ..Sums::default()
    }
}

#[test]
fn the_built_in_settings_then_the_files_key_by_key() {
    let mut s = Settings::builtin();
    assert_eq!(
        (s.rating, s.species_available, s.percent[1]),
        (10_000, 44, 0.5)
    );
    s.apply(&config(
        "[CHECKS]\nRating = 4000\nnewguest=2000\n[Characteristics]\ncspeciesavailable=20\n\
         cPctSick=0.3\ncDonationChance=0x10\ncDonationLow=x\n",
    ));
    // Names ignore case, a missing key keeps its value, hexadecimal is read, text is 0.
    assert_eq!((s.rating, s.message, s.new_guest), (4000, 10_000, 2000));
    assert_eq!(
        (s.species_available, s.percent[1], s.percent[0]),
        (20, 0.3, 0.5)
    );
    assert_eq!((s.donation_chance, s.donation_low), (16, 0));
    let before = s.clone();
    s.apply(&config(""));
    assert_eq!(s, before, "a file without a section changes nothing");
}

#[test]
fn the_economy_file_gives_the_happiness_and_chance_of_joining_the_endowment() {
    let join = |file: &str| {
        let mut s = Settings::builtin();
        s.apply(&config(file));
        (s.join_happiness, s.join_chance)
    };
    assert_eq!(join(""), (80, 50), "missing keys keep the built-in values");
    assert_eq!(
        join("[CHARACTERISTICS]\ncmembershipjoinchance=15\n"),
        (80, 15)
    );
    assert_eq!(
        join("[characteristics]\ncMembershipJoinHappiness=-5\ncMembershipJoinChance=0\n"),
        (-5, 0)
    );
    assert_eq!(
        join("[characteristics]\ncMembershipJoinHappiness=99999999\ncMembershipJoinChance=-3\n"),
        (99_999_999, -3)
    );
    assert_eq!(
        join("[characteristics]\ncMembershipJoinChance=x\n"),
        (80, 0)
    );
}

#[test]
fn the_rating_adds_seven_parts_as_the_specification_works_it_out() {
    let s = status("[characteristics]\ncSpeciesAvailable=20");
    // 17.2: healthy 13, species 3, animals 20, guests 17, value 4, research 1, esthetics 1.
    assert_eq!(s.compute(&running(), 0), 59);
    // An empty zoo is 12 and 12 for the happiness and 10 for a value of 30000; the esthetics
    // part is trunc(clamp(10 * esthetics, 0, 100) * 0.1).
    let empty = |esthetics, value| Sums {
        esthetics,
        zoo_value: value,
        ..Sums::default()
    };
    let rated = |e, v| s.compute(&empty(e, v), 0);
    assert_eq!(
        [
            rated(3.415, 560_000),
            rated(0.3, 50_000),
            rated(11.0, 50_000)
        ],
        [37, 34, 44]
    );
    // Limits: huge, negative and not a number.
    assert_eq!(
        [rated(1e9, 3000), rated(-3.0, -50), rated(f32::NAN, 0)],
        [35, 24, 24]
    );
    let best = Sums {
        animals: 1,
        animal_happiness: 100,
        guests: 1,
        guest_happiness: 100,
        species: 99,
        zoo_value: i64::MAX,
        esthetics: 50.0,
        research: 100,
        ..Sums::default()
    };
    assert_eq!(s.compute(&best, 0), 100);
    // A species limit of 0 or less skips its part.
    for limit in [0, -3] {
        let s = status(&format!("[characteristics]\ncSpeciesAvailable={limit}"));
        assert_eq!(s.compute(&running(), 0), 56);
    }
}

#[test]
fn the_rating_check_sets_the_statistics_and_restarts_its_timer() {
    let (mut s, mut money, mut log) = (status(""), Money::default(), Vec::new());
    s.timers = [5000, 6, 7];
    s.rating_check(running(), 0, &mut money, &mut log);
    // With the built-in 44 species: 13 + 1 + 20 + 17 + 4 + 1 + 1.
    assert_eq!((s.rating, s.timers), (57, [0, 6, 7]));
    let stat = |c: usize| (money.monthly[c][0], money.yearly[c][0], money.total[c]);
    assert_eq!(stat(money::RATING), (57.0, 57.0, 57.0));
    assert_eq!(
        (
            stat(money::ANIMAL_HAPPINESS).0,
            stat(money::GUEST_HAPPINESS).0
        ),
        (60.0, 42.0)
    );
    assert_eq!(
        (stat(money::ZOO_VALUE).0, stat(money::GUESTS).0),
        (12_500.0, 40.0)
    );
    assert_eq!(log, ["rating 57"]);
}

#[test]
fn the_message_check_posts_the_advice_in_order() {
    let mut s = status("[characteristics]\ncPctProtestors=0.3\ncPctSick=0.3\ncPctHungry=0.2");
    s.settings.high_guest_happiness = 80;
    s.sums = Sums {
        animals: 8,
        sick: 3,
        guests: 40,
        hungry: 9,
        guest_happiness: 40 * 85,
        ..Sums::default()
    };
    let (mut say, mut log) = (Vec::new(), Vec::new());
    s.timers[1] = 10_001;
    s.message_check(500.0, &mut say, &mut log);
    // 8 * 0.3 = 2.4 < 3 sick (protestors and guests), 40 * 0.2 = 8 < 9 hungry, 85 is happy.
    assert_eq!(
        say,
        [
            (10009, None),
            (10008, None),
            (10011, None),
            (10021, None),
            (10018, None)
        ]
    );
    assert_eq!((log.len(), s.timers[1]), (5, 0));
    let ids = |s: &mut Status, cash: f32| {
        let mut say = Vec::new();
        s.message_check(cash, &mut say, &mut Vec::new());
        say.iter().map(|m| m.0).collect::<Vec<_>>()
    };
    // At the limit nothing is posted: 2.4 < 2 is false, and ten guests are not many.
    s.sums = Sums {
        animals: 8,
        sick: 2,
        guests: 10,
        hungry: 10,
        ..Sums::default()
    };
    // The cash is truncated: 1000.9 is still low, -0.5 has run out but is not below 0.
    assert_eq!(ids(&mut s, 1001.0), []);
    assert_eq!([ids(&mut s, 1000.9), ids(&mut s, -0.5)], [[10018], [10019]]);
    assert_eq!(ids(&mut s, -1.0), [10019, 10020]);
    s.settings.low_guest_happiness = 0;
    s.sums = Sums {
        guests: 11,
        ..Sums::default()
    };
    assert_eq!(ids(&mut s, 5000.0), [10022]);
}

#[test]
fn a_check_is_due_when_its_timer_passes_the_period_and_the_month_flag_forces_the_rating() {
    let due = |r, m, n| Due {
        rating: r,
        message: m,
        new_guest: n,
    };
    let mut s = status("[checks]\nrating=4000\nmessage=10000\nnewguest=2000");
    assert_eq!(s.due(4000, false), due(false, false, true));
    assert_eq!(s.due(1, false), due(true, false, true));
    s.timers = [0; 3];
    assert_eq!(s.due(1, true), due(true, false, false));
    // A period of 0 or less is always passed; a huge step does not overflow.
    let mut s = status("[checks]\nrating=0\nmessage=-5\nnewguest=2147483647");
    assert_eq!(s.due(0, false), due(false, true, false));
    assert_eq!(s.due(u32::MAX, false), due(true, true, true));
}

#[test]
fn the_donation_draws_only_with_a_chance_and_low_cash_and_the_foundations_run_out() {
    let rng = |n: usize| {
        let mut rng = Rng::default();
        rng.seed(1);
        (0..n).for_each(|_| _ = rng.rand());
        rng
    };
    let economy = "[characteristics]\ncDonationChance=100\ncDonationLow=15000\n\
                   cDonationHigh=20000\ncDonationsAvail=2";
    let (mut s, mut say, mut log) = (status(economy), Vec::new(), Vec::new());
    let (mut r, mut money) = (rng(0), Money::default());
    // Cash of 1000 or more: nothing is drawn.
    money.cash = 1000.0;
    s.donation(&mut r, &mut money, &mut say, &mut log);
    assert_eq!((r.state(), s.given), (rng(0).state(), 0.0));
    // Below: two draws, the chance (41) and the amount (15000 + 18467 % 5000).
    money.cash = 900.0;
    s.donation(&mut r, &mut money, &mut say, &mut log);
    assert_eq!(r.state(), rng(2).state());
    assert_eq!(
        (money.cash, money.total[money::DONATIONS]),
        (18_467.0 + 900.0, 18_467.0)
    );
    assert_eq!(say, [(15002, Some(18_467))]);
    assert_eq!(log, ["donation 18467"]);
    // The second, then "no foundations", then only the count grows.
    for _ in 0..3 {
        money.cash = 0.0;
        s.donation(&mut r, &mut money, &mut say, &mut log);
    }
    assert_eq!(
        say.iter().map(|m| m.0).collect::<Vec<_>>(),
        [15002, 15002, 15003]
    );
    assert_eq!((s.given, money.cash), (4.0, 0.0));
    // A chance of 0 draws nothing, and a range that overflows does not panic.
    let mut r = rng(0);
    money.cash = 0.0;
    status("[characteristics]\ncDonationChance=0").donation(&mut r, &mut money, &mut say, &mut log);
    assert_eq!(r.state(), rng(0).state());
    let huge = "[characteristics]\ncDonationChance=100\ncDonationLow=2000000000\n\
                cDonationHigh=-2000000000";
    status(huge).donation(&mut r, &mut Money::default(), &mut say, &mut log);
}

#[test]
fn the_bars_are_written_once_the_counter_passes_1001() {
    let mut s = status("");
    s.rating = 37;
    s.sums = Sums {
        animals: 2,
        animal_happiness: 120,
        ..Sums::default()
    };
    assert_eq!(s.bars(1001), None);
    // The remainder is kept; the animals show (60 + 100) * 100 / 200.
    assert_eq!((s.bars(1), s.bar_ms), (Some([80, 0, 37]), 1));
    assert_eq!(s.bars(u32::MAX).map(|b| b[2]), Some(37));
    // An empty zoo shows 0, not the 12 its rating counts.
    s.sums = Sums::default();
    assert_eq!(s.bars(2000), Some([0, 0, 37]));
}

/// A type of a synthetic game with the file text `text`.
fn kind(class: &'static str, name: &str, text: &str) -> EntityType {
    EntityType {
        class,
        type_name: name.as_bytes().to_vec(),
        subtype: Vec::new(),
        file: None,
        config: Some(Arc::new(config(text))),
        name_id: 0,
        footprint: [1, 1, 1],
        height: 0,
    }
}

#[test]
fn the_sums_count_the_animals_the_value_and_the_esthetics_of_pieces() {
    let mut cat = Catalogue::default();
    cat.push(kind(
        "animals",
        "a",
        "[Characteristics/Integers]\ncPurchaseCost=100.9\n",
    ));
    let fence = "[Characteristics/Integers]\ncPurchaseCost=40\ncDecayedLife=2\n\
                 [EstheticBonus]\nv=9503\nv=4\nv=9504\nv=5\n";
    cat.push(kind("fences", "f", fence));
    let mut world = world_with((4, 4), |zoo| {
        zoo.entities.push(entity("animals", "a", 1, (32, 32), 0));
        zoo.entities.push(entity("animals", "a", 2, (96, 32), 0));
        zoo.entities.push(entity("fences", "f", 3, (32, 0), 0));
        zoo.entities.push(entity("guests", "g", 4, (32, 32), 0));
    });
    for (id, happiness, sick) in [(1, 60, false), (2, 30, true)] {
        let Some(Part::Animal(a)) = world.units.get_mut(id).map(|u| &mut u.part) else {
            panic!("no animal {id}");
        };
        (a.happiness, a.at_minimum_health) = (happiness, sick);
    }
    let Some(Part::Guest(g)) = world.units.get_mut(4).map(|u| &mut u.part) else {
        panic!("no guest");
    };
    g.values = [-40, 51, 50, 0, 100];
    let sums = Sums::of(&world, &cat, 250.7, 0);
    // The cash and each cost are cut to whole units: 250 + 100 + 100 + 40.
    assert_eq!(
        (
            sums.zoo_value,
            sums.animals,
            sums.sick,
            sums.animal_happiness
        ),
        (490, 2, 1, 90)
    );
    // The guest counts with its happiness, and not for the value of the zoo.
    assert_eq!(
        (sums.guests, sums.guest_happiness, sums.guest_average()),
        (1, -40, -40)
    );
    // A need counts when strictly above the limit, 50 without a guest type; the gift and litter
    // counts wait for the items.
    let needs = |s: &Sums| (s.hungry, s.thirsty, s.restroom, s.tired, s.gift, s.litter);
    assert_eq!(needs(&sums), (1, 0, 1, 0, 0, 0));
    // The guest file's own limit replaces it.
    let mut typed = Catalogue::default();
    typed.push(kind(
        "guests",
        "g",
        "[Characteristics/Integers]\ncUIThreshold=99\n",
    ));
    assert_eq!(
        needs(&Sums::of(&world, &typed, 250.7, 0)),
        (0, 0, 1, 0, 0, 0)
    );
    assert_eq!((sums.species, sums.esthetics), (0, 9.0));
    // A decayed piece (life 2 is at its limit) loses 11 for each of the four guest types, a
    // broken one 45 more.
    let esthetics = |world: &mut World, life| {
        let c = world.zoo.entities[2].common.as_mut().unwrap();
        c.fence.as_mut().unwrap().life = life;
        Sums::of(world, &cat, 0.0, 0).esthetics
    };
    assert_eq!(esthetics(&mut world, 2), 9.0 - 44.0);
    assert_eq!(esthetics(&mut world, 0), 9.0 - 4.0 * 56.0);
    assert_eq!(esthetics(&mut world, 3), 9.0);
}

#[test]
#[ignore = "needs the game: set ZT_DATA_DIR to the install folder"]
fn the_first_check_of_a_shipped_freeform_map_gives_the_observed_rating() {
    use crate::render::MapRenderer;
    use crate::sim::drive::Run;
    use crate::vfs::Vfs;
    use crate::zoo::Zoo;
    let dir = std::env::var_os("ZT_DATA_DIR").expect("ZT_DATA_DIR is not set");
    let vfs = Vfs::open(std::path::Path::new(&dir)).unwrap();
    let economy = vfs.read("economy.cfg").unwrap();
    // Zoo status 17.5: the set-up check and the first one, 4 s later. `med_kids` counts its 51
    // stored guests in the set-up check (6.4). At its first check the original has removed them
    // (the scenario resets) and has its research complete (5 points); the engine has neither
    // yet, and the two differences cancel (guests 17 against 12, research 0 against 5).
    // The stored guests keep their check countdowns, so the trash check cuts the happiness of
    // about one guest in five in these 4 s, not of all at once.
    let table = [
        ("dinosmal", 45, 42),
        ("deathmtn", 42, 37),
        ("nile2", 42, 37),
        ("dinomed", 42, 44),
        ("lavaland", 41, 36),
        ("airport", 40, 35),
        ("med_bch", 49, 44),
        ("lg_resrt", 47, 42),
        ("beach", 49, 44),
        ("tundra", 42, 37),
        ("med_kids", 81, 78),
    ];
    for (map, setup, first) in table {
        let catalogue = Catalogue::load(&vfs).unwrap();
        let renderer = MapRenderer::new(&vfs, &catalogue);
        let bytes = vfs.read(&format!("maps/{map}.zoo")).unwrap();
        let (zoo, money) = Zoo::parse_money(&bytes).unwrap();
        let world = World::new(zoo, |e| renderer.shape(e));
        let class = crate::animal::Class::load(&vfs);
        let mut run = Run::new((vfs.ini(), &economy), (catalogue, class), world, money, 1);
        assert_eq!(run.sim.status.rating, setup, "{map}: the set-up check");
        if map == "med_kids" {
            // The ratings do not depend on the limit of a need (`cUIThreshold`, above every
            // stored need here): a lookup that fell back to the built-in 50 would count some.
            let sums = Sums::of(&run.world, &run.cat, 0.0, 0);
            assert_eq!(sums.guests, 51, "{map}: the stored guests");
            let needs = [sums.hungry, sums.thirsty, sums.restroom, sums.tired];
            assert_eq!(needs, [0; 4], "{map}: the guests with a need");
        }
        (0..61).for_each(|_| _ = run.step(67));
        assert_eq!(run.sim.status.rating, first, "{map}: the first check");
    }
}

#[test]
fn the_economy_file_sets_what_the_compost_pays_for_dung() {
    let paid = |text: &str| status(text).settings.prices.recycling;
    assert_eq!(paid(""), 50.0);
    assert_eq!(paid("[Characteristics]\ncOtherKey=9\n"), 50.0);
    assert_eq!(paid("[characteristics]\nczoodoorecyclingamount=75\n"), 75.0);
    assert_eq!(paid("[characteristics]\ncZooDooRecyclingAmount=0\n"), 0.0);
    assert_eq!(paid("[characteristics]\ncZooDooRecyclingAmount=-5\n"), -5.0);
    assert_eq!(
        paid("[characteristics]\ncZooDooRecyclingAmount=1e30\n"),
        1e30
    );
}

#[test]
fn the_price_is_kept_in_the_range_at_the_start_and_after_the_economy_file() {
    let start = |price: f32, economy: Option<&str>| {
        let record = StatusRecord {
            price,
            ..StatusRecord::default()
        };
        Status::start(&record, economy.map(config).as_ref()).price
    };
    // The built-in range 0 to 100: huge, negative and not a number.
    assert_eq!(
        [22.0, 150.0, 1e30, -5.0, f32::NAN, 100.0].map(|p| start(p, None)),
        [22.0, 100.0, 100.0, 0.0, 100.0, 100.0]
    );
    // The file's range holds after it; a missing key keeps the built-in one, and the two are
    // swapped when the maximum is below the minimum.
    let range = "[characteristics]\ncMinAdultAdmissionPrice=10\ncMaxAdultAdmissionPrice=40\n";
    assert_eq!(
        [(5.0, range), (25.0, range), (70.0, range), (70.0, "")].map(|(p, f)| start(p, Some(f))),
        [10.0, 25.0, 40.0, 70.0]
    );
    let swapped = "[characteristics]\ncMinAdultAdmissionPrice=40\ncMaxAdultAdmissionPrice=10\n";
    assert_eq!(start(70.0, Some(swapped)), 40.0);
    assert_eq!(start(2.0, Some(swapped)), 10.0);
    let text = "[characteristics]\ncMinAdultAdmissionPrice=x\ncMaxAdultAdmissionPrice=-3\n";
    assert_eq!(start(70.0, Some(text)), 0.0, "text is 0");
    // A price equal to the minimum stays; a price at the maximum stays.
    assert_eq!([10.0, 40.0].map(|p| start(p, Some(range))), [10.0, 40.0]);
}

#[test]
fn the_month_end_endowment_draws_for_each_member_then_once_for_the_gift() {
    let rng = |n: usize| {
        let mut rng = Rng::default();
        rng.seed(1);
        (0..n).for_each(|_| _ = rng.rand());
        rng
    };
    let mut first = rng(0);
    let draws: Vec<i32> = (0..4).map(|_| (first.rand() % 101) as i32).collect();
    let economy = "[characteristics]\ncEndowmentGiftLow=100\ncEndowmentGiftHigh=300\n";
    let run = |members: f32, rating: i32, economy: &str| {
        let (mut s, mut money, mut log) = (status(economy), Money::default(), Vec::new());
        s.rating = rating;
        money.total[money::MEMBERS] = members;
        let mut r = rng(0);
        s.endowment(&mut r, &mut money, &mut log);
        (money, log, r)
    };
    // Three members: a draw each, one for the gift; those whose draw less 10 is above the rating
    // leave.
    let rating = 55;
    let (money, log, r) = run(3.0, rating, economy);
    let left = draws[..3].iter().filter(|&&n| rating < n - 10).count();
    assert!(left > 0 && left < 3, "{draws:?}");
    assert_eq!(r.state(), rng(4).state());
    assert_eq!(money.total[money::MEMBERS], (3 - left) as f32);
    assert_eq!(money.total[money::MEMBERS_LEFT], left as f32);
    let gift = (100.0 + draws[3] as f32 * 200.0 * 0.01) * (3 - left) as f32;
    assert_eq!((money.cash, money.total[money::ENDOWMENT]), (gift, gift));
    assert_eq!(money.total[money::NET], gift, "the gift is income");
    assert_eq!(log.len(), 4);
    assert_eq!(log[3], format!("endowment {} {gift}", draws[3]));
    // Without members nothing is drawn or booked: none, negative, not a number.
    for members in [0.0, -2.0, f32::NAN] {
        let (money, log, r) = run(members, rating, economy);
        assert_eq!((r.state(), log.len(), money.cash), (rng(0).state(), 0, 0.0));
    }
    // A rating of 100 keeps everyone, a rating below zero loses those with a draw above the factor;
    // a gift range below zero pays the low amount.
    let (money, _, _) = run(
        3.0,
        100,
        "[characteristics]\ncEndowmentGiftLow=5\ncEndowmentGiftHigh=1\n",
    );
    assert_eq!((money.total[money::MEMBERS], money.cash), (3.0, 15.0));
    // When every member leaves there is no gift draw and no payment.
    let (money, log, r) = run(3.0, -1000, "");
    assert_eq!(money.total[money::MEMBERS], 0.0);
    assert_eq!((r.state(), log.len(), money.cash), (rng(3).state(), 3, 0.0));
    assert_eq!(money.total[money::ENDOWMENT], 0.0);
    // A count a file stores out of all measure draws a bounded number of times.
    let (money, log, _) = run(1e30, 100, "[characteristics]\ncMembershipJoinFactor=0\n");
    assert_eq!(log.len(), MAX_MEMBERS as usize + 1);
    assert_eq!(money.total[money::MEMBERS_LEFT], 0.0);
}

#[test]
fn the_rating_loses_the_escape_change_less_the_time_per_day_since_the_last_escape() {
    use crate::timing::file_time;
    const DAY: i64 = 86_400_000;
    let mut s = status("");
    let sums = running();
    let base = s.compute(&sums, 0);
    s.settings.escaped_change = 30;
    s.settings.escaped_time = 2;
    // An escape on the day of the check takes the full change; after 5 whole days, 20; after 15
    // days or more, nothing.
    let at = |s: &mut Status, escape: i64, now: i64| {
        s.escape = file_time(escape);
        s.compute(&sums, now)
    };
    assert_eq!(at(&mut s, 0, 0), base - 30);
    assert_eq!(at(&mut s, 0, 5 * DAY + 23 * 3_600_000), base - 20);
    assert_eq!(
        at(&mut s, 0, 6 * DAY - 1),
        base - 20,
        "whole hours, then days"
    );
    assert_eq!(at(&mut s, 0, 15 * DAY), base);
    assert_eq!(at(&mut s, 0, 400 * DAY), base);
    // An escape that lies in the future has not happened: no penalty. A time of 0 or less and a
    // huge change are bounded, the rating stays 0 to 100.
    assert_eq!(at(&mut s, 3 * DAY, 0), base);
    assert_eq!(at(&mut s, 3 * DAY, 3 * DAY - 1), base);
    s.settings.escaped_time = 0;
    assert_eq!(at(&mut s, 0, 400 * DAY), base - 30);
    s.settings.escaped_time = -5;
    assert_eq!(
        at(&mut s, 0, 10 * DAY),
        (base - 80).max(0),
        "a negative time adds to the penalty"
    );
    s.settings.escaped_change = i32::MAX;
    assert_eq!(at(&mut s, 0, 0), 0);
    s.settings.escaped_change = i32::MIN;
    assert_eq!(at(&mut s, 0, 0), base);
    // A zoo that never had one stores 1 January 2000: a year before the first day, no penalty.
    let mut fresh = status("");
    assert_eq!(fresh.compute(&sums, 0), base);
    fresh.settings.escaped_change = 50;
    assert_eq!(fresh.compute(&sums, 0), base);
    // The date is what a file stores.
    let record = StatusRecord {
        escape: 7,
        ..StatusRecord::default()
    };
    assert_eq!(Status::start(&record, None).record().escape, 7);
}
