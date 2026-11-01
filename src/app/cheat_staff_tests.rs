//! The exhibit, animal, maintenance worker and keeper cheats, driven with invented texts.

use super::developer_tests::{available, catalogue, with_game, with_world};
use super::*;
use crate::cheat::{Cheat, Cond, Flags, GRIZZLY, SCIENTIST, Subject};
use crate::config::Config;
use crate::entity::{Catalogue, EntityType};

/// The gnu of the fixture (name id 7) starts unavailable and is no program's target.
const GNU: i32 = 7;

/// Arms an invented text for the cheat and types it as a name.
fn run(app: &mut App, subject: Subject, cheat: Cheat) {
    let text = format!("{subject:?} {cheat:?}");
    app.cheats.push(subject, &text, cheat);
    app.name_hook(subject, &text);
}

fn flags(app: &App) -> Flags {
    app.cheats.flags
}

/// What the other flags look like with only `set` up.
fn only(set: impl FnOnce(&mut Flags)) -> Flags {
    let mut f = Flags::default();
    set(&mut f);
    f
}

#[test]
fn each_flag_cheat_raises_its_flag_alone_and_the_world_sees_it_at_once() {
    type Set = fn(&mut Flags);
    let cases: [(Cheat, Set); 7] = [
        (Cheat::RatingBonus, |f| f.rating_bonus = true),
        (Cheat::DonationDouble, |f| f.donation_double = true),
        (Cheat::ShopIncomeDouble, |f| f.shop_income_double = true),
        (Cheat::BreedingBoost, |f| f.breeding_boost = true),
        (Cheat::MarketingDouble, |f| f.marketing_double = true),
        (Cheat::HealChangesSpecies, |f| f.heal_changes_species = true),
        (Cheat::KeeperSpeedDouble, |f| f.keeper_speed_double = true),
    ];
    for (cheat, set) in cases {
        with_game("cheat-flag", false, false, |app, _| {
            assert_eq!(flags(app), Flags::default());
            run(app, Subject::Exhibit, cheat);
            assert_eq!(flags(app), only(set), "{cheat:?}");
            let world = &app.game.as_ref().unwrap().world;
            assert_eq!(world.units.cheat, only(set), "{cheat:?}");
            // Again, and another text: nothing is undone.
            app.name_hook(Subject::Exhibit, &format!("Exhibit {cheat:?}"));
            app.name_hook(Subject::Exhibit, "other");
            assert_eq!(flags(app), only(set), "{cheat:?}");
        });
    }
}

#[test]
fn the_flags_last_through_a_new_game_and_come_back_from_the_step() {
    with_game("cheat-flag-new", false, false, |app, _| {
        run(app, Subject::Exhibit, Cheat::DonationDouble);
        app.open_game(crate::render::tests::tile_with(&[]), Money::default())
            .unwrap();
        assert!(flags(app).donation_double);
        // The step hands the flags to the new world, and takes back what a rule lowered.
        app.pass(1000, &[]);
        app.game.as_mut().unwrap().world.units.cheat.donation_double = false;
        app.pass(2000, &[]);
        assert!(app.game.as_ref().unwrap().world.units.cheat.donation_double);
    });
}

#[test]
fn an_unlock_cheat_makes_the_types_of_the_name_available() {
    with_game("cheat-unlock", false, false, |app, cat| {
        assert!(!available(app, cat)[2]);
        // A name id no type has changes nothing.
        for name in [0, -1, i32::MAX, 5] {
            run(app, Subject::Exhibit, Cheat::Unlock(name));
        }
        assert_eq!(available(app, cat), [true, true, false]);
        run(app, Subject::Exhibit, Cheat::Unlock(GNU));
        assert_eq!(available(app, cat), [true, true, true]);
        // Again: still available.
        app.name_hook(
            Subject::Exhibit,
            &format!("Exhibit {:?}", Cheat::Unlock(GNU)),
        );
        assert_eq!(available(app, cat), [true, true, true]);
    });
    // Without a game nothing happens.
    with_game("cheat-unlock-none", false, false, |app, cat| {
        app.cheats.push(Subject::Exhibit, "Zed", Cheat::Unlock(GNU));
        app.game = None;
        app.name_hook(Subject::Exhibit, "Zed");
        assert!(app.game.is_none() && cat.types.len() == 3);
    });
}

/// The fixture's types, and a Dinosaur Digs animal, an animal of the base game, a bush and a stone.
fn world_types() -> Catalogue {
    let mut cat = catalogue();
    let text = |class, name: &str, keys: &str| EntityType {
        class,
        type_name: name.as_bytes().to_vec(),
        subtype: if class == "animals" {
            b"m".to_vec()
        } else {
            Vec::new()
        },
        file: None,
        config: Some(std::sync::Arc::new(Config::parse(keys.as_bytes()).unwrap())),
        name_id: 0,
        footprint: [1, 1, 1],
        height: 0,
    };
    let ints = |keys: &str| format!("[Characteristics/Integers]\n{keys}\n");
    cat.push(text("animals", "dino", &ints("cExpansionID=1")));
    cat.push(text("animals", "moose", &ints("cExpansionID=0")));
    cat.push(text("objects", "bush", &ints("cFoliage=1")));
    cat.push(text("objects", "stone", &ints("cFoliage=0")));
    cat
}

fn names(app: &App) -> Vec<String> {
    let world = &app.game.as_ref().unwrap().world;
    world
        .zoo
        .entities
        .iter()
        .map(|e| e.type_name.clone())
        .collect()
}

#[test]
fn the_dinosaur_cheat_removes_the_animals_of_dinosaur_digs_and_nothing_else() {
    let entities = [
        ("animals", "dino", "m"),
        ("animals", "moose", "m"),
        ("objects", "bush", ""),
        ("animals", "dino", "m"),
        ("animals", "lion", "m"),
    ];
    with_world(
        "cheat-dino",
        (false, false),
        (world_types(), &entities),
        |app, _| {
            let cash = app.game.as_ref().unwrap().money.cash;
            run(app, Subject::Animal, Cheat::DinosaursVanish);
            assert_eq!(names(app), ["moose", "bush", "lion"]);
            assert_eq!(app.game.as_ref().unwrap().money.cash, cash, "no refund");
            // Again: nothing left to remove.
            let text = format!("Animal {:?}", Cheat::DinosaursVanish);
            app.name_hook(Subject::Animal, &text);
            assert_eq!(names(app), ["moose", "bush", "lion"]);
        },
    );
    with_world(
        "cheat-dino-none",
        (false, false),
        (world_types(), &[]),
        |app, _| {
            run(app, Subject::Animal, Cheat::DinosaursVanish);
            assert!(names(app).is_empty());
        },
    );
}

#[test]
fn the_foliage_cheat_removes_the_foliage_and_pays_the_bonus_even_for_none() {
    let entities = [
        ("objects", "bush", ""),
        ("objects", "stone", ""),
        ("animals", "moose", "m"),
        ("objects", "bush", ""),
    ];
    with_world(
        "cheat-foliage",
        (false, false),
        (world_types(), &entities),
        |app, _| {
            let cash = app.game.as_ref().unwrap().money.cash;
            run(app, Subject::Worker, Cheat::FoliageClearBonus);
            assert_eq!(names(app), ["stone", "moose"]);
            let money = &app.game.as_ref().unwrap().money;
            assert_eq!(money.cash, cash + 300.0);
            // No foliage left: the bonus is paid all the same, once per use.
            let text = format!("Worker {:?}", Cheat::FoliageClearBonus);
            app.name_hook(Subject::Worker, &text);
            assert_eq!(names(app), ["stone", "moose"]);
            assert_eq!(app.game.as_ref().unwrap().money.cash, cash + 600.0);
        },
    );
}

#[test]
fn an_entity_hook_looks_at_the_class_the_subtype_and_the_name_id_of_the_named_one() {
    // A male and a female keeper of the base game, a scientist, a worker, a bear.
    let named_types = || {
        let mut cat = catalogue();
        let mut add = |class, name: &str, sub: &str, name_id| {
            cat.push(EntityType {
                class,
                type_name: name.as_bytes().to_vec(),
                subtype: sub.as_bytes().to_vec(),
                file: None,
                config: None,
                name_id,
                footprint: [1, 1, 1],
                height: 0,
            });
        };
        add("keeper", "keeper", "m", 100);
        add("keeper", "keeper", "f", 100);
        add("keeper", "scient", "f", SCIENTIST);
        add("maint", "maint", "m", 200);
        add("maint", "maint", "f", 200);
        add("animals", "bear", "m", GRIZZLY);
        cat
    };
    let entities = [
        ("keeper", "keeper", "m"),
        ("keeper", "keeper", "f"),
        ("keeper", "scient", "f"),
        ("maint", "maint", "m"),
        ("maint", "maint", "f"),
        ("animals", "bear", "m"),
    ];
    // Row: the class and condition armed, and the entities (by id) the text works on.
    let rows: [(Subject, Cond, Vec<u32>); 6] = [
        (Subject::Keeper, Cond::Any, vec![1, 2, 3]),
        (Subject::Keeper, Cond::Subtype(b'm'), vec![1]),
        (Subject::Keeper, Cond::Subtype(b'f'), vec![2, 3]),
        (Subject::Keeper, Cond::NameId(SCIENTIST), vec![3]),
        (Subject::Worker, Cond::Subtype(b'm'), vec![4]),
        (Subject::Animal, Cond::NameId(GRIZZLY), vec![6]),
    ];
    for (class, cond, works) in rows {
        with_world(
            "cheat-named",
            (false, false),
            (named_types(), &entities),
            |app, _| {
                app.cheats
                    .push_when(class, cond, "Zed", Cheat::BreedingBoost);
                for id in 1..=6 {
                    app.cheats.flags = Flags::default();
                    app.interface().unwrap().set_text(3402, b"Zed");
                    app.entity_hook(Subject::Guest, 3402, Some(id));
                    let done = flags(app).breeding_boost;
                    assert_eq!(done, works.contains(&id), "{class:?} {cond:?} entity {id}");
                }
                // No selection, an id that is nobody's: the fallback class, no condition holds.
                for selected in [None, Some(0), Some(99), Some(u32::MAX)] {
                    app.cheats.flags = Flags::default();
                    app.entity_hook(Subject::Keeper, 3402, selected);
                    let done = flags(app).breeding_boost;
                    assert_eq!(done, class == Subject::Keeper && cond == Cond::Any);
                }
            },
        );
    }
}

#[test]
fn the_speed_cheat_doubles_the_speed_of_the_named_unit_at_once() {
    with_world(
        "cheat-speed",
        (false, false),
        (world_types(), &[("animals", "moose", "m")]),
        |app, _| {
            let world = &mut app.game.as_mut().unwrap().world;
            world.units.get_mut(1).unwrap().speed = 21;
            run(app, Subject::Keeper, Cheat::KeeperSpeedDouble);
            // No unit was named: nothing moved.
            let speed = |app: &mut App, id| {
                let world = &app.game.as_ref().unwrap().world;
                world.units.get(id).map(|u| u.speed)
            };
            assert_eq!(speed(app, 1), Some(21));
            app.exhibit_and_staff_cheat(Cheat::KeeperSpeedDouble, Some(1));
            assert_eq!(speed(app, 1), Some(42));
            // Unknown ids and a speed at the top do not panic.
            app.exhibit_and_staff_cheat(Cheat::KeeperSpeedDouble, Some(0));
            app.exhibit_and_staff_cheat(Cheat::KeeperSpeedDouble, Some(u32::MAX));
            app.game
                .as_mut()
                .unwrap()
                .world
                .units
                .get_mut(1)
                .unwrap()
                .speed = u32::MAX;
            app.exhibit_and_staff_cheat(Cheat::KeeperSpeedDouble, Some(1));
            assert_eq!(speed(app, 1), Some(u32::MAX));
        },
    );
}

#[test]
fn the_bear_cheat_rates_again_the_exhibits_of_grizzlies_and_flags_them_for_the_escape_check() {
    use crate::unit::testing::amphibious_world;
    for name_id in [GRIZZLY, GRIZZLY - 1, 0, i32::MIN] {
        let mut cat = Catalogue::default();
        cat.push(EntityType {
            class: "animals",
            type_name: b"bear".to_vec(),
            subtype: Vec::new(),
            file: None,
            config: None,
            name_id,
            footprint: [1, 1, 1],
            height: 0,
        });
        let mut world = amphibious_world(2, &[(5, "bear", (3, 3))]);
        let tank = world.exhibits().region(2 * 12 + 2).unwrap();
        world.exhibits_mut().list_mut().iter_mut().for_each(|e| {
            (e.rating.escape, e.rating.dirty) = (false, false);
        });
        super::cheat_staff::bears_look(&mut world, &cat);
        for e in world.exhibits().list() {
            let hit = e.id == tank && name_id == GRIZZLY;
            assert_eq!(e.rating.escape, hit, "{name_id}");
            // Its neighbours are rated again with it; nothing is when no bear is there.
            assert!(!hit || e.rating.dirty);
            assert!(hit || name_id == GRIZZLY || !e.rating.dirty);
        }
    }
}

#[test]
fn the_buildings_cheat_rubbles_each_rubbleable_building_once_and_nothing_else() {
    use crate::app::cheat_staff::rubble_buildings;
    use crate::unit::testing::{centre, entity, world_with};
    let typed = |class, kind: &str, sub: &str, keys: &str| {
        let text = format!("[Characteristics/Integers]\ncFootprintX=2\ncFootprintY=2\n{keys}\n");
        EntityType {
            class,
            type_name: kind.into(),
            subtype: sub.into(),
            file: None,
            config: Some(std::sync::Arc::new(Config::parse(text.as_bytes()).unwrap())),
            name_id: 0,
            footprint: [2, 2, 1],
            height: 0,
        }
    };
    let mut cat = Catalogue::default();
    cat.push(typed("rubble", "other", "brubble1", ""));
    cat.push(typed("building", "hall", "", ""));
    cat.push(typed("building", "bunker", "", "cRubbleable=0"));
    cat.push(typed("objects", "bush", "", ""));
    let mut w = world_with((10, 10), |zoo| {
        for (id, class, kind, at) in [
            (1, "building", "hall", (2, 2)),
            (2, "building", "bunker", (5, 2)),
            (3, "objects", "bush", (2, 5)),
            (4, "building", "hall", (5, 5)),
            (5, "building", "ghost", (7, 7)),
        ] {
            zoo.entities.push(entity(class, kind, id, centre(at), 0));
        }
    });
    let shape = |_: &Entity| None;
    let mut rng = crate::sim::Rng::default();
    let mut kit = crate::rubble::Kit {
        cat: &cat,
        rng: &mut rng,
        template: "",
        shape: &shape,
    };
    rubble_buildings(&mut w, &mut kit);
    let of = |class: &str| -> Vec<u32> {
        (w.zoo.entities.iter())
            .filter(|e| e.class == class)
            .map(|e| e.common.as_ref().unwrap().id)
            .collect()
    };
    // The bunker (not rubbleable), the bush and the building of an unknown type stay.
    assert_eq!(of("building"), [2, 5]);
    assert_eq!(of("objects"), [3]);
    // A 2 x 2 hall leaves one piece, which is neither in the undo list nor on the old ids.
    assert_eq!(of("rubble").len(), 2);
    assert!(w.undo().is_empty());
}
