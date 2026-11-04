//! The drown task of a guest (tank fauna 13.1, 13.2) on a 10 by 10 map with a strip of water from
//! (4, 7) to (6, 7) and guest 1 in its middle.

use super::*;
use crate::guest::Guest;
use crate::unit::testing::world_with;

fn pond(guest: Guest) -> World {
    crowded(guest, Vec::new())
}

fn crowded(guest: Guest, others: Vec<crate::zoo::Entity>) -> World {
    let mut w = world_with((10, 10), |zoo| {
        zoo.entrance = (8, 5);
        for x in 4..=6 {
            zoo.tiles[7 * 10 + x].terrain = 1;
        }
        let mut man = entity("guests", "man", 1, centre((5, 7)), 0);
        man.common.as_mut().unwrap().name = "Sam".into();
        zoo.entities.push(man);
        zoo.entities.extend(others);
    });
    w.set_water(vec![false, true]);
    let u = w.units.get_mut(1).unwrap();
    u.part = Part::Guest(Guest {
        countdowns: [1000; 11],
        saw_animal: 1000,
        ..guest
    });
    u.goal.record.task = DROWN;
    w
}

fn man() -> Guest {
    Guest::default()
}

fn kinds() -> [Guest; 3] {
    let flags = |female, child| Guest {
        female,
        child,
        ..Guest::default()
    };
    [flags(true, false), flags(false, true), flags(true, true)]
}

fn at(w: &World, id: u32) -> (i32, i32) {
    let c = w.zoo.entities[w.find(id).unwrap()].common.as_ref().unwrap();
    (c.x.div_euclid(64), c.y.div_euclid(64))
}

fn init(w: &mut World) -> bool {
    with_cx(w, &mut Rng::default(), |cx| cx.drown_init())
}

/// The drown sets end at once; a guest's ambient table then starts `bIdle`, which replaces the
/// finished set as the top one.
struct Drowns(Sets);

impl Data for Drowns {
    fn set(&self, e: &crate::zoo::Entity, name: &str) -> Option<&[Call]> {
        self.0.set(e, name)
    }
    fn ambient(&self, _: &crate::zoo::Entity, _: usize) -> &[Row] {
        &self.0.1
    }
    fn timing(&self, e: &crate::zoo::Entity, name: &str) -> Option<Timing> {
        self.0.timing(e, name)
    }
    fn number(&self, e: &crate::zoo::Entity, number: &Number) -> i32 {
        self.0.number(e, number)
    }
    fn sound(&self, e: &crate::zoo::Entity, key: &str) -> Option<Heard> {
        self.0.sound(e, key)
    }
}

fn drowns() -> Drowns {
    let mut sets = Sets::new(&[
        ("bSurfDrown", &["fFaceTowardTarget()"]),
        ("bSuitDrown", &["fFaceTowardTarget()"]),
    ]);
    let idle = Row {
        lo: i32::MIN,
        hi: i32::MAX,
        set: "bIdle".into(),
    };
    sets.1.push(idle);
    Drowns(sets)
}

#[test]
fn a_man_posts_thinks_drowns_and_is_removed_with_the_forced_message_when_the_set_ends() {
    let (mut w, mut s) = (pond(man()), sim());
    let log = run_with(&mut w, &mut s, &drowns(), 12);
    assert!(log.contains(&"set 1 bSurfDrown".to_string()), "{log:?}");
    assert!(
        w.units
            .take_guest_lines()
            .contains(&"thought 1 10566".to_string())
    );
    let told = w.units.take_messages();
    let first = told.iter().position(|m| m.0 == 10528).expect("warning");
    let gone = told.iter().position(|m| m.0 == 10251).expect("gone");
    assert!(first < gone);
    assert_eq!(told[gone], (10251, 1, vec!["Sam".to_string()]));
    assert!(w.find(1).is_none(), "removed");
}

#[test]
fn a_man_is_not_removed_while_the_drown_set_plays() {
    let (mut w, mut s) = (pond(man()), sim());
    let data = Sets::new(&[("bSurfDrown", &["fKeepControl()"])]);
    run_with(&mut w, &mut s, &data, 12);
    assert!(w.find(1).is_some());
    assert_eq!(unit(&w).runner.top(), Some("bSurfDrown"));
    assert_eq!(unit(&w).goal.record.task, DROWN);
}

#[test]
fn a_man_that_came_out_of_a_changing_room_drowns_in_the_suit_and_forgets_the_return_building() {
    for (set, expected) in [
        (None, "bSurfDrown"),
        (Some("bReturnchroom0"), "bSuitDrown"),
        (Some("bReturnchroom1"), "bSurfDrown"),
    ] {
        let mut w = pond(Guest {
            return_set: set.map(String::from),
            returning: Some(7),
            ..man()
        });
        assert!(init(&mut w));
        let Part::Guest(g) = &unit(&w).part else {
            panic!()
        };
        assert_eq!(
            (g.return_set.as_deref(), g.returning),
            (Some(expected), None),
            "{set:?}"
        );
    }
}

#[test]
fn a_woman_or_a_child_is_put_on_the_nearest_dry_tile_and_never_removed() {
    for guest in kinds() {
        let mut w = pond(guest);
        assert!(init(&mut w));
        // Round 1 starts with the north-west tile.
        assert_eq!(at(&w, 1), (4, 6));
        assert_eq!(unit(&w).goal.record.task, NONE);
        assert!(w.units.take_messages().is_empty());
        assert!(w.units.thoughts.of(1).is_empty());
        let mut s = sim();
        run_with(&mut w, &mut s, &drowns(), 12);
        assert!(w.find(1).is_some());
    }
}

#[test]
fn the_rescue_skips_water_and_the_tiles_where_an_object_or_a_unit_stands() {
    let others = vec![
        entity("bush", "", 2, centre((4, 6)), 0),
        entity("bush", "", 4, centre((3, 5)), 0),
        entity("keeper", "keeper", 3, centre((5, 6)), 0),
    ];
    let mut w = crowded(kinds()[0].clone(), others);
    assert!(init(&mut w));
    // Round 1 fails whole: (4, 6) holds a bush, (4, 7) is water, (5, 6) a keeper, (5, 7) is
    // water. Round 2 adds the west column first, from the north: (3, 5) holds a bush.
    assert_eq!(at(&w, 1), (3, 6));
}

#[test]
fn a_rescue_with_no_tile_leaves_the_guest_where_it_is_with_no_task() {
    let mut w = pond(kinds()[0].clone());
    for t in &mut w.zoo.tiles {
        t.terrain = 1;
    }
    assert!(init(&mut w));
    assert_eq!(at(&w, 1), (5, 7));
    assert_eq!(unit(&w).goal.record.task, NONE);
    assert!(w.units.thoughts.of(1).is_empty());
}

#[test]
fn only_a_guest_can_drown() {
    assert_eq!(Goal::build(DROWN, Kind::Guest), Some(Goal::Drown));
    let mut w = map("keeper");
    assert!(!with_cx(&mut w, &mut Rng::default(), |cx| cx.drown_init()));
}

#[test]
fn a_drowned_man_is_hidden_when_he_is_removed() {
    let mut w = pond(man());
    let at = w.find(1).unwrap();
    assert!(w.is_drawn(at));
    with_cx(&mut w, &mut Rng::default(), |cx| cx.drown_complete());
    assert!(!w.is_drawn(at));
}

#[test]
fn the_rescue_does_not_count_the_guest_herself() {
    // She stands off the centre of a dry tile whose three tiles before hers in the scan are taken.
    let others = vec![
        entity("bush", "", 2, centre((4, 4)), 0),
        entity("bush", "", 3, centre((4, 5)), 0),
        entity("bush", "", 4, centre((5, 4)), 0),
    ];
    let mut w = crowded(kinds()[0].clone(), others);
    let at = w.find(1).unwrap();
    w.place_unit(at, (5 * 64 + 10, 5 * 64 + 50, 0), 0);
    assert!(init(&mut w));
    let c = w.zoo.entities[at].common.as_ref().unwrap();
    assert_eq!((c.x, c.y), centre((5, 5)));
}
