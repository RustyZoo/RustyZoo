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

/// The sums of a running zoo of the specification's example 17.2, without esthetics.
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
    // Names ignore case; a key that is missing keeps its value; hexadecimal is read; text is 0.
    assert_eq!((s.rating, s.message, s.new_guest), (4000, 10_000, 2000));
    assert_eq!(
        (s.species_available, s.percent[1], s.percent[0]),
        (20, 0.3, 0.5)
    );
    assert_eq!((s.donation_chance, s.donation_low), (16, 0));
    // The prices are swapped when the maximum is below the minimum.
    s.apply(&config(
        "[characteristics]\ncMinAdultAdmissionPrice=30\ncMaxAdultAdmissionPrice=10",
    ));
    assert_eq!((s.price_min, s.price_max), (10.0, 30.0));
    // A file without a section changes nothing.
    let before = s.clone();
    s.apply(&config(""));
    assert_eq!(s, before);
}

#[test]
fn the_adult_price_is_held_between_the_limits() {
    let s = status("[characteristics]\ncMinAdultAdmissionPrice=2\ncMaxAdultAdmissionPrice=60");
    assert_eq!(
        [-5.0, 2.0, 22.0, 60.0, 1e9].map(|p| s.limit(p)),
        [2.0, 2.0, 22.0, 60.0, 60.0]
    );
    assert!(s.limit(f32::NAN).is_finite());
    // Built in: 0 to 100; the price a file stores is limited when the game starts.
    let record = StatusRecord {
        price: 250.0,
        ..StatusRecord::default()
    };
    assert_eq!(Status::start(&record, None).price, 100.0);
}

#[test]
fn the_rating_adds_seven_parts_as_the_specification_works_it_out() {
    let s = status("[characteristics]\ncSpeciesAvailable=20");
    // 17.2: healthy 13, species 3, animals 20, guests 17, value 4, research 1, esthetics 1.
    assert_eq!(s.compute(&running(), 0), 59);
    // An empty zoo: 12 and 12 for the happiness, 10 for the value, the esthetics part
    // trunc(clamp(10 * esthetics) / 10): 0.3 gives 0, 3.415 gives 3, 11 and more give 10.
    let empty = |esthetics, value| Sums {
        esthetics,
        zoo_value: value,
        ..Sums::default()
    };
    assert_eq!(s.compute(&empty(3.415, 560_000), 0), 37);
    assert_eq!(s.compute(&empty(0.3, 50_000), 0), 34);
    assert_eq!(s.compute(&empty(11.0, 50_000), 0), 44);
    assert_eq!(s.compute(&empty(1e9, 3000), 0), 12 + 12 + 1 + 10);
    // The limits: a negative zoo value, negative esthetics, a huge penalty and 100.
    assert_eq!(s.compute(&empty(-3.0, -50), 0), 24);
    assert_eq!(s.compute(&running(), 1000), 0);
    assert_eq!(
        s.compute(&running(), -5),
        59,
        "a penalty of 0 or less takes nothing"
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
    // Without a species limit the part is skipped, as is a negative limit.
    let none = status("[characteristics]\ncSpeciesAvailable=0");
    assert_eq!(none.compute(&running(), 0), 56);
    assert_eq!(
        status("[characteristics]\ncSpeciesAvailable=-3").compute(&running(), 0),
        56
    );
}

#[test]
fn the_rating_check_sets_the_statistics_and_restarts_its_timer() {
    let mut s = status("");
    let mut money = Money::default();
    let mut log = Vec::new();
    s.timers = [5000, 6, 7];
    s.rating_check(running(), &mut money, (0, false), (50, 2), &mut log);
    // Species count for 44: 13 + 1 + 20 + 17 + 4 + 1 + 1; the timer of the check restarts.
    assert_eq!((s.rating, s.timers), (57, [0, 6, 7]));
    let value = |c: usize| (money.monthly[c][0], money.yearly[c][0], money.total[c]);
    assert_eq!(
        value(money::RATING),
        (s.rating as f32, s.rating as f32, s.rating as f32)
    );
    assert_eq!(value(money::ANIMAL_HAPPINESS).0, 60.0);
    assert_eq!(value(money::GUEST_HAPPINESS).0, 42.0);
    assert_eq!(
        (value(money::ZOO_VALUE).0, value(money::GUESTS).0),
        (12_500.0, 40.0)
    );
    assert_eq!(log, [format!("rating {}", s.rating)]);
}

#[test]
fn an_escape_costs_the_ini_change_and_fades_by_the_time_for_each_game_day() {
    let mut s = status("");
    let check = |s: &mut Status, now, loose| {
        let (mut money, mut log) = (Money::default(), Vec::new());
        s.rating_check(running(), &mut money, (now, loose), (50, 2), &mut log);
        s.rating
    };
    // No escape: the date of the last one is a year before the game, no penalty.
    let free = check(&mut s, 0, false);
    assert_eq!(free, 57);
    // An animal loose: the whole change; caught, 2 points back for each day, 25 days to none.
    assert_eq!(check(&mut s, 1000, true), free - 50);
    assert_eq!(check(&mut s, 1000 + 3 * GAME_DAY_MS, false), free - 44);
    assert_eq!(check(&mut s, 1000 + 25 * GAME_DAY_MS, false), free);
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
    let ids: Vec<u32> = say.iter().map(|m| m.0).collect();
    // 8 * 0.3 = 2.4 < 3 protestors and sick; 40 * 0.2 = 8 < 9 hungry; 85 is very happy; 500 is low.
    assert_eq!(ids, [10009, 10008, 10011, 10021, 10018]);
    assert_eq!(
        log,
        [
            "message 10009",
            "message 10008",
            "message 10011",
            "message 10021",
            "message 10018"
        ]
    );
    assert_eq!(s.timers[1], 0);
    // At the fractions' limit nothing is posted: 2.4 < 2 is false, and ten guests are not many.
    s.sums = Sums {
        animals: 8,
        sick: 2,
        guests: 10,
        hungry: 10,
        ..Sums::default()
    };
    let ids = |cash: f32, s: &mut Status| {
        let mut say = Vec::new();
        s.message_check(cash, &mut say, &mut Vec::new());
        say.iter().map(|m| m.0).collect::<Vec<_>>()
    };
    assert_eq!(ids(5000.0, &mut s), []);
    // The cash is truncated: 1000.9 is still low, 1001 is not, -0.5 has run out but is not below 0.
    assert_eq!(ids(1001.0, &mut s), []);
    assert_eq!(ids(1000.9, &mut s), [10018]);
    assert_eq!(ids(1000.0, &mut s), [10018]);
    assert_eq!(ids(-0.5, &mut s), [10019]);
    assert_eq!(ids(-1.0, &mut s), [10019, 10020]);
    // Unhappy guests: 0 against the shipped low threshold of -20 when set so.
    s.settings.low_guest_happiness = 0;
    s.sums = Sums {
        guests: 11,
        ..Sums::default()
    };
    assert_eq!(ids(5000.0, &mut s), [10022]);
}

#[test]
fn the_checks_are_due_when_their_timer_passes_the_period_and_the_month_flag_forces_the_rating() {
    let mut s = status("[checks]\nrating=4000\nmessage=10000\nnewguest=2000");
    assert_eq!(
        s.due(4000, false),
        Due {
            rating: false,
            message: false,
            new_guest: true
        }
    );
    assert_eq!(
        s.due(1, false),
        Due {
            rating: true,
            message: false,
            new_guest: true
        }
    );
    s.timers = [0, 0, 0];
    assert_eq!(
        s.due(1, true),
        Due {
            rating: true,
            message: false,
            new_guest: false
        }
    );
    // A period of 0 or less is always passed, a huge step does not overflow.
    let mut s = status("[checks]\nrating=0\nmessage=-5\nnewguest=2147483647");
    assert_eq!(
        s.due(0, false),
        Due {
            rating: false,
            message: true,
            new_guest: false
        }
    );
    assert_eq!(
        s.due(1, false),
        Due {
            rating: true,
            message: true,
            new_guest: false
        }
    );
    assert_eq!(
        s.due(u32::MAX, false),
        Due {
            rating: true,
            message: true,
            new_guest: true
        }
    );
}

#[test]
fn the_donation_chance_draws_only_with_a_chance_and_low_cash() {
    let rng = |n: usize| {
        let mut rng = Rng::default();
        rng.seed(1);
        (0..n).for_each(|_| _ = rng.rand());
        rng
    };
    let mut s = status(
        "[characteristics]\ncDonationChance=100\ncDonationLow=15000\ncDonationHigh=20000\ncDonationsAvail=2",
    );
    let (mut say, mut log) = (Vec::new(), Vec::new());
    // Cash of 1000 or more: nothing is drawn.
    let (mut r, mut money) = (
        rng(0),
        Money {
            cash: 1000.0,
            ..Money::default()
        },
    );
    s.donation(&mut r, &mut money, &mut say, &mut log);
    assert_eq!((r.state(), s.given), (rng(0).state(), 0.0));
    // Below: two draws, the chance and the amount (41, then 18467 % 5000 = 3467).
    money.cash = 900.0;
    s.donation(&mut r, &mut money, &mut say, &mut log);
    assert_eq!(r.state(), rng(2).state());
    assert_eq!(
        (money.cash, money.total[money::DONATIONS]),
        (900.0 + 18_467.0, 18_467.0)
    );
    assert_eq!(
        (say.clone(), log.clone()),
        (
            vec![(15002, Some(18_467))],
            vec!["donation 18467".to_string()]
        )
    );
    // The second and then "no foundations", then nothing but the count.
    for _ in 0..3 {
        money.cash = 0.0;
        s.donation(&mut r, &mut money, &mut say, &mut log);
    }
    assert_eq!(
        say.iter().map(|m| m.0).collect::<Vec<_>>(),
        [15002, 15002, 15003]
    );
    assert_eq!((s.given, money.cash), (4.0, 0.0), "the fourth gave nothing");
    // A chance of 0 draws nothing at all; one of 100 and a range of 0 gives the low amount.
    let mut s = status("[characteristics]\ncDonationChance=0");
    let mut r = rng(0);
    money.cash = 0.0;
    s.donation(&mut r, &mut money, &mut say, &mut log);
    assert_eq!(r.state(), rng(0).state());
    let mut s =
        status("[characteristics]\ncDonationChance=100\ncDonationLow=700\ncDonationHigh=700");
    s.donation(&mut r, &mut money, &mut say, &mut log);
    assert_eq!(money.cash, 700.0);
    // A range above 2^31 does not overflow.
    let mut s = status(
        "[characteristics]\ncDonationChance=100\ncDonationLow=-2000000000\ncDonationHigh=2000000000",
    );
    s.donation(&mut r, &mut money, &mut Vec::new(), &mut Vec::new());
}

#[test]
fn the_endowment_pays_its_members_and_some_leave_at_the_month_end() {
    let mut s = status(
        "[characteristics]\ncMembershipJoinFactor=10\ncEndowmentGiftLow=100\ncEndowmentGiftHigh=300",
    );
    s.rating = 50;
    let mut rng = Rng::default();
    rng.seed(1);
    let mut money = Money::default();
    // No member: nothing is drawn, only the accounts move on.
    s.finance_check(&mut rng, &mut money);
    assert_eq!(
        (money.month, rng.state()),
        (2, {
            let mut r = Rng::default();
            r.seed(1);
            r.state()
        })
    );
    // Three members; draws 41, 18467 and 6334 are 41, 85 and 72 modulo 101: a member leaves when
    // 50 < n - 10, so the second and third go; the gift draw is 26500 % 101 = 38 and the one
    // member that stays gets 100 + 38 * 200 * 0.01 = 176.
    money.add_count(money::MEMBERS, 3.0);
    let mut rng = Rng::default();
    rng.seed(1);
    s.finance_check(&mut rng, &mut money);
    assert!((money.cash - 176.0).abs() < 0.01 && money.total[money::ENDOWMENT] == money.cash);
    assert_eq!(
        (
            money.total[money::MEMBERS],
            money.total[money::MEMBERS_LEFT]
        ),
        (1.0, 2.0)
    );
    assert_eq!(money.month, 3);
    // At a rating of 100 nobody leaves; a high gift below the low one pays the low one.
    s.rating = 100;
    s.settings.endowment_high = 50;
    let cash = money.cash;
    s.finance_check(&mut rng, &mut money);
    assert_eq!(money.cash, cash + 100.0);
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
    // The remainder is kept: 1001 + 1 is 1002 > 1001, left 1; the animals show 60 + 100 over 2.
    assert_eq!(s.bars(1), Some([80, 0, 37]));
    assert_eq!(s.bar_ms, 1);
    assert_eq!(s.bars(1000), None);
    assert_eq!(s.bars(u32::MAX).map(|b| b[2]), Some(37));
    // An empty zoo shows 0, never the 12 of the rating.
    s.sums = Sums::default();
    assert_eq!(s.bars(2000), Some([0, 0, 37]));
}

/// A type of a synthetic game: its purchase cost and the esthetic bonus of the man and woman.
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
fn the_sums_count_animals_value_and_the_esthetics_of_pieces() {
    let mut cat = Catalogue::default();
    cat.push(kind(
        "animals",
        "a",
        "[Characteristics/Integers]\ncPurchaseCost=100.9\n",
    ));
    cat.push(kind(
        "fences",
        "f",
        "[Characteristics/Integers]\ncPurchaseCost=40\ncDecayedLife=2\n\
         [EstheticBonus]\nv=9503\nv=4\nv=9504\nv=5\n",
    ));
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
    let sums = Sums::of(&world, &cat, 250.7, 0);
    // The cash and each purchase cost are cut to whole units: 250 + 100 + 100 + 40.
    assert_eq!(
        (
            sums.zoo_value,
            sums.animals,
            sums.sick,
            sums.animal_happiness
        ),
        (490, 2, 1, 90)
    );
    assert_eq!((sums.guests, sums.species, sums.esthetics), (0, 0, 9.0));
    // A decayed piece (life 2 is at its limit) loses 11 for each of the four guest types, a
    // broken one 56 more.
    let life = |world: &mut World, life| {
        let c = world.zoo.entities[2].common.as_mut().unwrap();
        c.fence.as_mut().unwrap().life = life;
        Sums::of(world, &cat, 0.0, 0).esthetics
    };
    assert_eq!(life(&mut world, 2), 9.0 - 44.0);
    assert_eq!(life(&mut world, 0), 9.0 - 4.0 * 56.0);
    assert_eq!(life(&mut world, 3), 9.0);
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
    // Zoo status 17.5: the rating after the first check, and the set-up check before it;
    // `med_kids` keeps its research complete (5 points) in the original, M7 here.
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
        ("med_kids", 76, 73),
    ];
    for (map, setup, first) in table {
        let catalogue = crate::entity::Catalogue::load(&vfs).unwrap();
        let renderer = MapRenderer::new(&vfs, &catalogue);
        let (zoo, money) =
            Zoo::parse_money(&vfs.read(&format!("maps/{map}.zoo")).unwrap()).unwrap();
        let world = crate::world::World::new(zoo, |e| renderer.shape(e));
        let class = crate::animal::Class::load(&vfs);
        let mut run = Run::new((vfs.ini(), &economy), (catalogue, class), world, money, 1);
        assert_eq!(run.sim.status.rating, setup, "{map}: the set-up check");
        (0..61).for_each(|_| _ = run.step(67));
        assert_eq!(run.sim.status.rating, first, "{map}: the first check");
    }
}
