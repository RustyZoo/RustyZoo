use super::*;
use crate::entity::EntityType;
use crate::fixtures::ty;
use crate::unit::testing::{centre, entity, world_with};

/// A map with the zoo entrance at (9, 5) and the entities of `kinds`, one for each, in a row.
fn zoo(kinds: &[(&str, &str)]) -> World {
    world_with((10, 10), |zoo| {
        zoo.entrance = (9, 5);
        for (n, (class, name)) in kinds.iter().enumerate() {
            let at = centre((n as i32 % 8, n as i32 / 8 * 2));
            zoo.entities.push(entity(class, name, n as u32 + 1, at, 0));
        }
    })
}

fn catalogue() -> Catalogue {
    let mut cat = Catalogue::default();
    let fun: [(&'static str, &str, &str); 5] = [
        ("building", "ride", "[Satisfies]\nfun\nbuilding\n"),
        ("objects", "fountain", "[Satisfies]\nfun\n"),
        ("building", "shop", "[Satisfies]\nbuilding\n"),
        ("building", "loud", "[Satisfies]\nFun\n"),
        ("building", "plain", ""),
    ];
    for (class, name, ini) in fun {
        cat.push(ty(class, name, "", ini));
    }
    cat.push(EntityType {
        config: None,
        ..ty("building", "bare", "", "")
    });
    cat
}

#[test]
fn the_counts_go_by_the_class_of_the_type_and_a_subclass_counts() {
    let world = zoo(&[
        ("animals", "lion"),
        ("animals", "lion"),
        ("guests", "man"),
        ("guests", "man"),
        ("guests", "man"),
        ("keeper", "keeper"),
        ("maint", "worker"),
        ("tour", "guide"),
        ("helicopter", "heli"),
        ("objects", "bench"),
    ]);
    let counts = Counts::of(&world, &catalogue());
    assert_eq!(counts.list(), [2, 0, 0, 3, 4]);
}

#[test]
fn an_attraction_is_a_type_whose_own_satisfies_holds_the_key_fun_byte_for_byte() {
    let world = zoo(&[
        ("building", "ride"),
        ("objects", "fountain"),
        ("building", "shop"),
        ("building", "loud"),
        ("building", "plain"),
        ("building", "bare"),
        ("building", "missing"),
    ]);
    // The ride satisfies fun and a building and counts once; Fun in capitals does not count.
    assert_eq!(Counts::of(&world, &catalogue()).attractions, 2);
    // Two of the same type count twice.
    let twice = zoo(&[("building", "ride"), ("building", "ride")]);
    assert_eq!(Counts::of(&twice, &catalogue()).attractions, 2);
}

#[test]
fn an_empty_zoo_counts_nothing() {
    let world = zoo(&[]);
    assert_eq!(Counts::of(&world, &Catalogue::default()), Counts::default());
}

/// A pen of 3 by 3 tiles round (2, 5) with a gate fence, which makes one exhibit.
fn pen(show: Option<bool>) -> World {
    let mut w = world_with((10, 10), |zoo| {
        zoo.entrance = (9, 5);
        for i in 0..3 {
            for (at, d) in [
                ((1 + i, 4), 0),
                ((1 + i, 6), 4),
                ((1, 4 + i), 6),
                ((3, 4 + i), 2),
            ] {
                let id = 100 + zoo.entities.len() as u32;
                zoo.entities.push(entity("fences", "", id, centre(at), d));
            }
        }
    });
    if let Some(show) = show {
        let x = w.exhibits_mut().list_mut();
        x[0].tank = Some(crate::exhibit::Tank {
            show,
            ..Default::default()
        });
    }
    w
}

#[test]
fn the_exhibits_are_the_habitat_list_less_the_show_tanks() {
    let plain = pen(None);
    let n = plain.exhibits().list().len();
    assert!(n >= 1, "the pen makes an exhibit");
    assert_eq!(Counts::of(&plain, &Catalogue::default()).exhibits, n);
    // A tank that is not a show tank counts.
    let tank = pen(Some(false));
    assert_eq!(Counts::of(&tank, &Catalogue::default()).exhibits, n);
    // A show tank does not.
    let show = pen(Some(true));
    assert_eq!(Counts::of(&show, &Catalogue::default()).exhibits, n - 1);
}

#[test]
fn a_name_with_a_character_other_than_a_space_is_kept() {
    for kept_text in ["a", " a ", "\t", "  \t", "\u{a0}", "\0"] {
        assert!(kept(kept_text.as_bytes()), "{kept_text:?}");
    }
    for blank in ["", " ", "     "] {
        assert!(!kept(blank.as_bytes()), "{blank:?}");
    }
}

#[test]
fn the_members_are_cut_toward_zero() {
    assert_eq!([members(12.9), members(-3.9), members(0.0)], [12, -3, 0]);
    assert_eq!(members(f32::NAN), 0);
    assert_eq!(members(f32::INFINITY), i64::MAX);
}

#[test]
fn the_name_is_the_building_on_the_entrance_tile() {
    let with = world_with((10, 10), |zoo| {
        zoo.entrance = (9, 5);
        zoo.entities
            .push(entity("objects", "tree", 1, centre((2, 2)), 0));
        zoo.entities
            .push(entity("building", "hall", 2, centre((9, 5)), 0));
    });
    let at = entrance(&with).unwrap();
    assert_eq!(with.zoo.entities[at].type_name, "hall");
    // No building there, or an entrance outside the map.
    let without = world_with((10, 10), |zoo| zoo.entrance = (9, 5));
    assert_eq!(entrance(&without), None);
    let outside = world_with((10, 10), |zoo| zoo.entrance = (u32::MAX, 5));
    assert_eq!(entrance(&outside), None);
}

#[test]
fn the_entrance_building_is_the_one_that_holds_the_last_quarter_of_the_tile() {
    let with = |dx: i32, dy: i32| {
        world_with((10, 10), |zoo| {
            zoo.entrance = (9, 5);
            zoo.entities
                .push(entity("building", "shed", 1, (9 * 64 + dx, 5 * 64 + dy), 0));
        })
    };
    assert_eq!(entrance(&with(48, 48)), Some(0));
    for (dx, dy) in [(16, 16), (48, 16), (16, 48)] {
        assert_eq!(entrance(&with(dx, dy)), None, "{dx} {dy}");
    }
}
