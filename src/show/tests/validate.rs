//! Whether the first animal can do an item of the script: the start removes the ones it cannot,
//! the cue aborts them, and a toy item draws once each time (shows 23.6, 27 points 1 and 2).

use super::cycle::{info_mut, info_of_show, item, record, show_tank, staged, tank_mut, tick};
use super::*;
use crate::behaviour::NoData;
use crate::sim::Rng;
use crate::zoo::Item;

const TOY: u32 = 9100;

/// A toy that holds one seal, a toy item for it and a world with the toy in the tank.
fn toy_world(mut items: Vec<Item>) -> (World, Catalogue) {
    let toy = "[Characteristics/Integers]\ncCapacity=1\ncTimeInside=9\ncUserStaysOutside=0\n\
        [Satisfies]\nbuilding\nshowtrick\n[slots]\nname=slot\n[slot]\nslotpos=0\nslotpos=0\n\
        entrpos=0\nentrpos=48\nexitpos=0\nexitpos=48\ncapacity=1\n";
    for i in items.iter_mut().filter(|i| i.marker == 1) {
        i.building = TOY;
    }
    let (mut w, mut cat) = staged(items);
    cat.push(EntityType {
        name_id: TOY as i32,
        footprint: [1, 1, 1],
        ..kind("building", "toy", toy)
    });
    w.zoo
        .entities
        .push(entity("building", "toy", 40, centre((6, 2)), 4));
    let all = w.zoo.entities.clone();
    w.units.sync(&all);
    (w, cat)
}

/// Puts animal `id` on tile `tile`.
fn place(w: &mut World, id: u32, tile: (i32, i32)) {
    let at = w.find(id).unwrap();
    let (x, y) = centre(tile);
    let c = w.zoo.entities[at].common.as_mut().unwrap();
    (c.x, c.y) = (x, y);
}

/// Starts the show at its time with the generator `rng`; whether it runs.
fn start(w: &mut World, cat: &Catalogue, rng: &mut Rng) -> bool {
    let mut events = Events::default();
    update(w, cat, (&mut events, rng, &NoData), 90);
    info_of_show(w).show.running
}

fn seeded() -> Rng {
    let mut rng = Rng::default();
    rng.seed(7);
    rng
}

/// The generator after `n` draws.
fn drawn(n: usize) -> Rng {
    let mut rng = seeded();
    (0..n).for_each(|_| {
        rng.rand();
    });
    rng
}

#[test]
fn the_start_asks_the_first_animal_of_the_list_and_keeps_the_items_it_can_do() {
    // The toy is in the show tank: a seal in the tank can reach it, one on the grounds cannot.
    for (first, last, runs) in [(21, 22, false), (22, 21, true)] {
        let (mut w, cat) = toy_world(vec![item(1, 3, 5)]);
        place(&mut w, 21, (1, 3));
        info_mut(&mut w).records[0].units = vec![first, last];
        assert_eq!(start(&mut w, &cat, &mut seeded()), runs, "first {first}");
        assert_eq!(w.zoo.shows.scripts[0].items.len(), usize::from(runs));
    }
}

#[test]
fn a_toy_item_draws_one_number_at_the_start_and_one_more_for_each_animal_at_its_cue() {
    let (mut w, cat) = toy_world(vec![item(1, 3, 5)]);
    let mut rng = seeded();
    assert!(start(&mut w, &cat, &mut rng));
    assert_eq!(rng.rand(), drawn(1).rand());
    // Both animals are at item 0 with no cue sent: each cue asks again (the trick start is the next
    // pass's).
    let mut rng = seeded();
    let mut events = Events::default();
    for st in &mut info_mut(&mut w).show.items {
        (st.item, st.ready) = (0, false);
    }
    update(&mut w, &cat, (&mut events, &mut rng, &NoData), 91);
    assert_eq!(rng.rand(), drawn(2).rand());
}

#[test]
fn an_item_of_another_kind_and_the_item_before_the_first_draw_nothing() {
    let (mut w, cat) = toy_world(vec![item(0, 3, 5), item(3, 3, 5)]);
    let mut rng = seeded();
    assert!(start(&mut w, &cat, &mut rng));
    assert_eq!(rng.rand(), seeded().rand());
}

#[test]
fn the_depth_is_that_of_the_show_tank_not_of_the_tank_the_animal_stands_in() {
    for (level, runs) in [(2, false), (3, true), (4, true), (0, false)] {
        let (mut w, cat) = staged(vec![Item {
            depth: 3,
            ..item(0, 3, 5)
        }]);
        // Both seals stand on the zoo grounds.
        place(&mut w, 21, (1, 3));
        place(&mut w, 22, (1, 3));
        tank_mut(&mut w).tank = Some(show_tank(level));
        // A dry tank refuses the start whatever the depth: the level 0 row tests the depth alone.
        let ok = start(&mut w, &cat, &mut seeded());
        assert_eq!(ok, runs && level > 0, "level {level}");
    }
}

#[test]
fn a_keeper_in_another_show_tank_fails_the_validation_with_code_9() {
    // Seal 21 stands in the second tank, whose show has a keeper or not (shows 23.6 point 4).
    for (keepers, runs) in [(&[][..], false), (&[30][..], true)] {
        let (mut w, cat) = staged(vec![item(0, 3, 5)]);
        place(&mut w, 21, (7, 1));
        let mut other = info_of_show(&w).clone();
        other.id = 1002;
        other.records = vec![record(KEEPER, 0, keepers)];
        w.zoo.shows.infos.push(Some(other));
        assert_eq!(start(&mut w, &cat, &mut seeded()), runs, "{keepers:?}");
        assert_eq!(w.zoo.shows.scripts[0].items.len(), usize::from(runs));
    }
}

#[test]
fn a_toy_item_with_no_toy_for_the_animal_is_removed_and_the_others_stay() {
    let (mut w, cat) = toy_world(vec![item(1, 3, 5), item(0, 3, 5)]);
    w.zoo.shows.scripts[0].items[1].id = 11201;
    // The toy is gone: the first item fails (3), the second stays.
    w.zoo.entities.retain(|e| e.type_name != "toy");
    assert!(start(&mut w, &cat, &mut seeded()));
    let ids: Vec<u16> = w.zoo.shows.scripts[0].items.iter().map(|i| i.id).collect();
    assert_eq!(ids, [11201]);
}

#[test]
fn a_cue_aborts_an_item_the_animal_can_no_longer_do_and_the_pass_aborts_it_again() {
    let (mut w, cat) = toy_world(vec![item(0, 3, 5)]);
    let mut events = Events::default();
    tick(&mut w, &cat, &mut events, 90);
    assert!(info_of_show(&w).show.running);
    // The water drops under the trick's depth after the start.
    w.zoo.shows.scripts[0].items[0].depth = 9;
    for st in &mut info_mut(&mut w).show.items {
        (st.item, st.ready) = (0, true);
    }
    let mut events = Events::default();
    tick(&mut w, &cat, &mut events, 91);
    let aborted = (super::cycle::heard(&events).into_iter())
        .filter(|e| e.0 == event::ITEM_ABORTED && e.2 == 21)
        .count();
    // The cue aborts it, then the pass: two pairs for each animal.
    assert_eq!(aborted, 2);
    assert!(info_mut(&mut w).show.items.iter().all(|s| s.aborted));
}
