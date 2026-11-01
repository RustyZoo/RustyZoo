//! The guest-name cheats (cheats 4.2) other than the research ones, on synthetic worlds and
//! through the name hook with invented texts.

use super::cheat_guest::*;
use super::developer_tests::with_game;
use super::*;
use crate::cheat::{Cheat, Subject, spread};
use crate::config::Config;
use crate::entity::{Catalogue, EntityType};
use crate::money::Money;
use crate::unit::Part;
use crate::unit::testing::{centre, entity, keeper_shape, world_with};
use crate::world::World;

/// Guests 1 (a man), 2 (a woman) and 3 (a boy), each with the palette 0, 0, 0, 0, and the
/// buildings 10 (`tent`, recoloured), 11 (`shed`, not), 12 (`plain`, no key) and 13 (a tent with
/// one palette entry), with the palette 1, 2, 3.
fn zoo() -> World {
    let mut w = world_with((10, 10), |zoo| {
        for (id, kind) in [(1, "man"), (2, "woman"), (3, "boy")] {
            zoo.entities
                .push(entity("guests", kind, id, centre((id as i32, 1)), 0));
        }
        for (id, kind) in [(10, "tent"), (11, "shed"), (12, "plain"), (13, "tent")] {
            zoo.entities
                .push(entity("building", kind, id, centre((id as i32 - 8, 3)), 0));
        }
        for e in &mut zoo.entities {
            let c = e.common.as_mut().unwrap();
            c.colors = match (e.class.as_str(), c.id) {
                ("guests", _) => vec![0; 4],
                (_, 13) => vec![1],
                _ => vec![1, 2, 3],
            };
        }
    });
    if let Some(Part::Guest(g)) = w.units.get_mut(2).map(|u| &mut u.part) {
        g.female = true;
    }
    w
}

fn buildings() -> Catalogue {
    let mut cat = Catalogue::default();
    for (kind, keys) in [
        ("tent", "cIsColorReplaced=1"),
        ("shed", "cIsColorReplaced=0"),
        ("plain", "cHeight=1"),
    ] {
        let text = format!("[Characteristics/Integers]\n{keys}\n");
        cat.push(EntityType {
            class: "building",
            type_name: kind.as_bytes().to_vec(),
            subtype: Vec::new(),
            file: None,
            config: Some(std::sync::Arc::new(Config::parse(text.as_bytes()).unwrap())),
            name_id: 0,
            footprint: [1, 1, 1],
            height: 0,
        });
    }
    cat
}

fn colours(w: &World, id: u32) -> Vec<u8> {
    let at = w.find(id).unwrap();
    w.zoo.entities[at].common.as_ref().unwrap().colors.clone()
}

fn guest(w: &World, id: u32) -> &crate::guest::Guest {
    match &w.units.get(id).unwrap().part {
        Part::Guest(g) => g,
        _ => panic!("not a guest"),
    }
}

#[test]
fn a_palette_set_recolours_the_guests_by_sex_and_the_recolourable_buildings_only() {
    let mut w = zoo();
    palette_set(&mut w, &buildings(), [8, 4, 5], [21, 16]);
    assert_eq!(colours(&w, 1), [8, 4, 0, 0], "a man: shirt, trousers");
    assert_eq!(colours(&w, 2), [8, 5, 0, 0], "a woman: shirt, skirt");
    assert_eq!(colours(&w, 3), [8, 4, 0, 0]);
    assert_eq!(colours(&w, 10), [21, 16, 3]);
    // No flag, a zero flag and a building with one palette entry stay as they are.
    for id in [11, 12] {
        assert_eq!(colours(&w, id), [1, 2, 3], "{id}");
    }
    assert_eq!(colours(&w, 13), [1]);
    // An empty zoo and an unknown catalogue do nothing.
    palette_set(
        &mut world_with((2, 2), |_| {}),
        &buildings(),
        [1, 2, 3],
        [4, 5],
    );
    let mut w = zoo();
    palette_set(&mut w, &Catalogue::default(), [8, 4, 5], [21, 16]);
    assert_eq!(colours(&w, 10), [1, 2, 3]);
    assert_eq!(colours(&w, 1), [8, 4, 0, 0]);
}

#[test]
fn the_sick_recolour_spares_a_carried_guest_and_the_goals_of_a_busy_one() {
    let mut w = zoo();
    for (id, task, carried) in [(1, 100, false), (2, 105, false), (3, 100, true)] {
        let u = w.units.get_mut(id).unwrap();
        (u.goal.record.task, u.carried) = (task, carried);
    }
    guests_sick(&mut w);
    // Not busy: recoloured, goals stopped, sick.
    assert_eq!(colours(&w, 1)[..2], [5, 6]);
    assert!(guest(&w, 1).sick);
    assert_eq!(w.units.get(1).unwrap().goal.record.task, 0);
    // In a building: recoloured, but neither stopped nor sick.
    assert_eq!(colours(&w, 2)[..2], [5, 6]);
    assert!(!guest(&w, 2).sick);
    assert_eq!(w.units.get(2).unwrap().goal.record.task, 105);
    // Carried: untouched.
    assert_eq!(colours(&w, 3), [0; 4]);
    assert!(!guest(&w, 3).sick);
}

#[test]
fn only_a_man_takes_the_scare_and_paint_bits_and_any_guest_the_sickness_one() {
    for (bit, men_only) in [
        (spread::SICKNESS, false),
        (spread::SCARE_WOMEN, true),
        (spread::SCARE_CHILDREN, true),
        (spread::YELLOW, true),
        (spread::RED, true),
        (0, true),
        (u8::MAX, true),
    ] {
        let mut w = zoo();
        for at in 0..3 {
            spreads(&mut w, at, bit);
        }
        let bits: Vec<u8> = (1..=3).map(|id| w.units.get(id).unwrap().spreads).collect();
        let want = if men_only { [bit, 0, 0] } else { [bit; 3] };
        assert_eq!(bits, want, "{bit}");
    }
    // Painting also paints the named man's own shirt at once; sickness paints nothing.
    for (bit, shirt) in [(spread::YELLOW, 9), (spread::RED, 1), (spread::SICKNESS, 0)] {
        let mut w = zoo();
        spreads(&mut w, 0, bit);
        assert_eq!(colours(&w, 1)[0], shirt, "{bit}");
    }
    // A building is no guest, and a position out of the list is no guest.
    let mut w = zoo();
    spreads(&mut w, 3, spread::SICKNESS);
    assert!(
        w.units
            .get(10)
            .is_none_or(|u| !matches!(&u.part, Part::Guest(_)))
    );
}

/// Scenery of the type `car` with these name ids, and the cat that knows them.
fn cars(ids: &[i32]) -> (World, Catalogue) {
    let mut cat = Catalogue::default();
    cat.push(EntityType {
        class: "rubble",
        type_name: b"other".to_vec(),
        subtype: b"brubls1".to_vec(),
        file: None,
        config: None,
        name_id: 0,
        footprint: [1, 1, 1],
        height: 0,
    });
    let w = world_with((10, 10), |zoo| {
        for (n, &name) in ids.iter().enumerate() {
            let kind = format!("car{n}");
            zoo.entities.push(entity(
                "objects",
                &kind,
                100 + n as u32,
                centre((n as i32, 1)),
                0,
            ));
            cat.push(EntityType {
                class: "objects",
                type_name: kind.into_bytes(),
                subtype: Vec::new(),
                file: None,
                config: None,
                name_id: name,
                footprint: [1, 1, 1],
                height: 0,
            });
        }
    });
    (w, cat)
}

/// Runs the car cheat on the world with the generator `rng`.
fn cheat_cars(
    w: &mut World,
    cat: &Catalogue,
    money: &mut Money,
    told: &mut Vec<u32>,
    rng: &mut crate::sim::Rng,
) -> usize {
    let shape = |_: &crate::zoo::Entity| None;
    let mut kit = crate::rubble::Kit {
        cat,
        rng,
        template: "R %s",
        shape: &shape,
    };
    remove_cars((w, money), &mut kit, |id| told.push(id))
}

#[test]
fn the_cars_turn_to_rubble_by_name_id_and_pay_the_gift_once_when_any_went() {
    let (mut w, cat) = cars(&[6138, 6140, 6167, 7000, 0, -1, i32::MAX]);
    let (mut money, mut told, mut rng) = (Money::default(), Vec::new(), crate::sim::Rng::default());
    assert_eq!(cheat_cars(&mut w, &cat, &mut money, &mut told, &mut rng), 3);
    assert_eq!(told, [100, 101, 102]);
    let left = |w: &World, class: &str| -> Vec<u32> {
        (w.zoo.entities.iter())
            .filter(|e| e.class == class)
            .map(|e| e.common.as_ref().unwrap().id)
            .collect()
    };
    assert_eq!(left(&w, "objects"), [103, 104, 105, 106]);
    // A one-quadrant car leaves at most one piece, and the generator was used.
    assert!(left(&w, "rubble").len() <= 3);
    assert_ne!(rng, crate::sim::Rng::default());
    assert_eq!(
        (money.cash, money.total[crate::money::DONATIONS]),
        (5500.0, 5500.0)
    );
    // None to take: nothing is told, nothing is drawn and nothing is paid.
    let before = rng.clone();
    let none = cheat_cars(&mut w, &cat, &mut money, &mut Vec::new(), &mut rng);
    assert_eq!((none, &rng), (0, &before));
    let (mut w, cat) = cars(&[]);
    assert_eq!(cheat_cars(&mut w, &cat, &mut money, &mut told, &mut rng), 0);
    assert_eq!(money.cash, 5500.0);
}

fn arm(app: &mut App, text: &str, cheat: Cheat) {
    app.cheats.push(Subject::Guest, text, cheat);
}

/// Adds guest `id` (a `kind`) to the game and selects it.
fn select(app: &mut App, id: u32, kind: &str) {
    let game = app.game.as_mut().unwrap();
    let e = entity("guests", kind, id, centre((3, 3)), 0);
    game.world.add(e, Some(keeper_shape())).unwrap();
    let id = game
        .world
        .zoo
        .entities
        .last()
        .unwrap()
        .common
        .as_ref()
        .unwrap()
        .id;
    app.open_guest(id);
}

fn type_name(app: &mut App, text: &str) {
    app.interface().unwrap().set_text(3402, text.as_bytes());
    app.guest_rename();
}

#[test]
fn a_guest_name_runs_the_cheat_on_the_flag_the_guests_the_money_and_the_menu() {
    with_game("cheat-guest-names", false, false, |app, _| {
        select(app, 0, "man");
        arm(app, "Zed One", Cheat::EggsHatchFast);
        arm(app, "Zed Two", Cheat::GuestsLeave);
        arm(app, "Zed Three", Cheat::Spread(spread::RED));
        arm(app, "Zed Four", Cheat::CarsToRubble);
        arm(app, "Zed Five", Cheat::UnlockScenarios);
        arm(app, "Zed Six", Cheat::FencesBreak);
        let id = app.game.as_ref().unwrap().guest.selected.unwrap();
        let game = |app: &App| {
            app.game
                .as_ref()
                .unwrap()
                .world
                .units
                .get(id)
                .unwrap()
                .clone()
        };
        // A wrong text runs none of them.
        for text in ["zed one", "Zed One ", "Zed", ""] {
            type_name(app, text);
        }
        assert!(!app.cheats.eggs_hatch_fast);
        type_name(app, "Zed One");
        assert!(app.cheats.eggs_hatch_fast);
        assert!(matches!(&game(app).part, Part::Guest(g) if !g.leave));
        type_name(app, "Zed Two");
        assert!(matches!(&game(app).part, Part::Guest(g) if g.leave));
        type_name(app, "Zed Three");
        assert_eq!(game(app).spreads, spread::RED);
        // No cars, no menu scenarios and no fence: nothing happens, and no gift is paid.
        let cash = app.game.as_ref().unwrap().money.cash;
        for text in ["Zed Four", "Zed Five", "Zed Six"] {
            type_name(app, text);
        }
        assert_eq!(app.game.as_ref().unwrap().money.cash, cash);
        // The next step hands the flag to the world.
        assert!(!app.game.as_ref().unwrap().world.units.eggs_hatch_fast);
        app.pass(1000, &[]);
        assert!(app.game.as_ref().unwrap().world.units.eggs_hatch_fast);
        // Without a game, none of them runs.
        app.game = None;
        for cheat in [Cheat::GuestsLeave, Cheat::CarsToRubble, Cheat::Spread(2)] {
            app.guest_cheat(cheat);
        }
    });
}

#[test]
fn the_named_guest_is_the_selected_one_and_without_one_no_bit_is_taken() {
    with_game("cheat-guest-none", false, false, |app, _| {
        arm(app, "Zed Three", Cheat::Spread(spread::YELLOW));
        // Nobody selected.
        type_name(app, "Zed Three");
        select(app, 0, "woman");
        let id = app.game.as_ref().unwrap().guest.selected.unwrap();
        // A woman is not a man.
        type_name(app, "Zed Three");
        let unit = app.game.as_ref().unwrap().world.units.get(id).unwrap();
        assert_eq!(unit.spreads, 0);
    });
}
