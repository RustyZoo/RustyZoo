//! The portal wall pair of the show neighbours (shows 31.3 points 5 to 8, 31.5, 31.6 point 3) on
//! flat synthetic ground: a plain tank on the left, a show tank built against its east side.

use super::decision::{build, id_at};
use super::tank::{put_wall, ring, tank_shape, tanks};
use super::*;

/// Plain tanks from column 1 (and from column 5 when `both`), then a show tank on columns 3 and 4,
/// all on rows 2 to `last`, built against them: the shared lines hold the plain tank's wall and the
/// show tank's copy.
fn trio(last: i32, both: bool) -> World {
    let mut world = tanks(&[], &[], &[]);
    build(&mut world, "tank", (1, 2, 2, last));
    if both {
        build(&mut world, "tank", (5, 2, 6, last));
    }
    world.begin();
    let mut pieces = ring("show", 3, 2, 4, last);
    pieces.retain(|&(_, p)| !(p.0 == 3 && p.2 == 6) && !(both && p.0 == 4 && p.2 == 2));
    for (class, p) in pieces {
        put_wall(&mut world, class, p);
    }
    world
}

/// The position of the piece on side `d` of tile (`x`, `y`).
fn piece(world: &World, x: i32, y: i32, d: usize) -> usize {
    world.edge(world.tile_index(x, y).unwrap(), d).unwrap()
}

fn flag(world: &World, x: i32, y: i32, d: usize) -> bool {
    let fence = world.zoo.entities[piece(world, x, y, d)].common.as_ref();
    fence.and_then(|c| c.fence).is_some_and(|f| f.portal)
}

/// The flags of the lines between columns 2 and 3, row by row from row 2, on both sides.
fn west_line(world: &World, rows: i32) -> Vec<(bool, bool)> {
    (2..=rows)
        .map(|y| (flag(world, 3, y, 6), flag(world, 2, y, 2)))
        .collect()
}

/// The links are made again after an edit that touches none: a piece far from the tanks.
fn touch(world: &mut World) {
    world.begin();
    put_wall(world, "fences", (7, 0, 0));
}

/// The links are made again, as a plain tank drained and filled again does.
fn again(world: &mut World, id: u32) {
    world.drain(id);
    world.fill(id);
}

#[test]
fn the_pieces_of_every_qualifying_pair_lose_their_flag_and_the_other_pairs_keep_theirs() {
    // 31.3 points 5 and 7: a flag set by hand on the line goes at the recompute; the north wall of
    // the show tank, a pair with the zoo grounds, is not touched.
    let mut world = trio(4, false);
    let (plain, show) = (id_at(&world, 1, 2), id_at(&world, 3, 2));
    assert_eq!(
        west_line(&world, 4),
        [(false, false), (true, true), (false, false)]
    );
    for (x, y, d) in [(3, 2, 6), (2, 4, 2), (3, 2, 0)] {
        let at = piece(&world, x, y, d);
        world.set_flag(at, true);
    }
    again(&mut world, plain);
    assert!(
        world
            .exhibits()
            .get(show)
            .unwrap()
            .show_near
            .contains(&plain)
    );
    assert_eq!(
        west_line(&world, 4),
        [(false, false), (true, true), (false, false)]
    );
    assert!(
        flag(&world, 3, 2, 0),
        "a line to the zoo grounds is not a pair of the link"
    );
}

#[test]
fn only_a_pair_with_one_piece_on_each_tile_and_no_gate_is_straight() {
    // 31.3 point 6: the corner tiles hold two pieces; a two by two tank has only corners; a gate
    // on the line is no portal.
    let world = trio(3, false);
    assert!(west_line(&world, 3).iter().all(|f| *f == (false, false)));
    let world = trio(4, false);
    let show = world.exhibits().get(id_at(&world, 3, 2)).unwrap();
    assert_eq!(show.portals.len(), 1);
    assert_eq!(west_line(&world, 4)[1], (true, true));
    let mut world = trio(4, false);
    let at = piece(&world, 3, 3, 6);
    (world.zoo.entities[at].class, world.zoo.entities[at].subtype) =
        ("tankwall".into(), "g".into());
    touch(&mut world);
    assert_eq!(west_line(&world, 4), [(false, false); 3]);
}

#[test]
fn the_flag_setter_closes_the_portal_when_it_changes_in_play_and_not_when_a_zoo_is_read() {
    // 31.3 point 7 and 31.6 point 3.
    let mut world = trio(4, false);
    let open = |w: &mut World, x: i32, y: i32, d: usize| {
        let at = piece(w, x, y, d);
        w.set_portal(at, true);
    };
    let is_open = |w: &World, x: i32, y: i32, d: usize| {
        let c = w.zoo.entities[piece(w, x, y, d)].common.as_ref();
        c.and_then(|c| c.fence).is_some_and(|f| f.open)
    };
    open(&mut world, 3, 3, 6);
    // The same pair again: the flag does not change, the open byte stays.
    touch(&mut world);
    assert!(is_open(&world, 3, 3, 6));
    // A flag set by hand on the line goes at the recompute, and the byte of that piece with it
    // in play; a zoo read keeps it.
    let at = piece(&world, 3, 4, 6);
    world.set_flag(at, true);
    open(&mut world, 3, 4, 6);
    // The tanks are filled in the file.
    let mut saved = world.saved(|_| true);
    saved.tanks.iter_mut().for_each(|t| t.level = 3);
    touch(&mut world);
    assert!(!flag(&world, 3, 4, 6) && !is_open(&world, 3, 4, 6));
    let loaded = World::new(saved, tank_shape);
    assert!(!flag(&loaded, 3, 4, 6) && is_open(&loaded, 3, 4, 6));
    assert!(flag(&loaded, 3, 3, 6) && is_open(&loaded, 3, 3, 6));
}

#[test]
fn the_portal_is_the_first_straight_pair_of_the_show_tanks_boundary() {
    // 31.3 point 8: rows 3 and 4 both qualify; the one met first is the portal, and the other
    // pair's flags are cleared.
    let mut world = trio(5, false);
    let (plain, show) = (id_at(&world, 1, 2), id_at(&world, 3, 2));
    assert_eq!(
        west_line(&world, 5),
        [(false, false), (true, true), (false, false), (false, false)]
    );
    let first = world.exhibits().get(show).unwrap().portals[0];
    let at = |id: u32| world.find(id).unwrap();
    assert_eq!(first.to, plain);
    assert_eq!(at(first.mine), piece(&world, 3, 3, 6));
    assert_eq!(at(first.theirs), piece(&world, 2, 3, 2));
    let spare = piece(&world, 3, 4, 6);
    world.set_flag(spare, true);
    touch(&mut world);
    assert!(!flag(&world, 3, 4, 6) && flag(&world, 3, 3, 6));
}

#[test]
fn draining_a_plain_tank_drops_its_link_and_flags_and_filling_it_again_brings_them_back() {
    // 31.5 point 2 and the test of point 6.
    let mut world = trio(4, true);
    let (n1, show, n2) = (
        id_at(&world, 1, 2),
        id_at(&world, 3, 2),
        id_at(&world, 5, 2),
    );
    let east = |w: &World| (flag(w, 4, 3, 2), flag(w, 5, 3, 6));
    assert_eq!(world.exhibits().get(show).unwrap().show_near, [n1, n2]);
    assert!(flag(&world, 3, 3, 6) && east(&world) == (true, true));
    world.drain(n1);
    assert_eq!(world.exhibits().get(show).unwrap().show_near, [n2]);
    assert!(
        !flag(&world, 3, 3, 6) && !flag(&world, 2, 3, 2),
        "the S-N1 flags go"
    );
    assert_eq!(east(&world), (true, true), "the S-N2 flags stay");
    world.fill(n1);
    assert!(flag(&world, 3, 3, 6) && flag(&world, 2, 3, 2));
    // Lower the walls of N2 to 3: its link and flags go.
    world.set_height(n2, 3);
    assert_eq!(world.exhibits().get(show).unwrap().show_near, [n1]);
    assert_eq!(east(&world), (false, false));
}

#[test]
fn deleting_an_exhibit_clears_the_flags_of_its_portals_on_both_sides() {
    // 31.5 point 1: the plain side is opened to the grounds, then the show tank.
    let mut world = trio(4, true);
    let id = |w: &World, x: usize| w.tank_at(3 * 8 + x);
    let (n2, show) = (id(&world, 5).unwrap(), id(&world, 3).unwrap());
    let wall = |w: &World, x, y, d| {
        w.zoo.entities[piece(w, x, y, d)]
            .common
            .as_ref()
            .unwrap()
            .id
    };
    let out = wall(&world, 6, 3, 2);
    assert!(world.remove(out));
    assert!(world.exhibits().get(n2).is_none());
    assert!(
        !flag(&world, 4, 3, 2) && !flag(&world, 5, 3, 6),
        "the east flags are gone"
    );
    assert!(flag(&world, 3, 3, 6), "the west link stays");
    let out = wall(&world, 3, 2, 0);
    assert!(world.remove(out));
    assert!(world.exhibits().get(show).is_none());
    assert!(
        !flag(&world, 3, 3, 6) && !flag(&world, 2, 3, 2),
        "the west flags are gone"
    );
}

#[test]
fn the_halves_of_a_split_show_tank_get_their_portals_again() {
    // 31.5 point 4: rows 2 to 6 cut between rows 2 and 3; the old portal (row 3) is now a corner.
    let mut world = trio(6, false);
    assert_eq!(west_line(&world, 6)[1], (true, true));
    world.begin();
    put_wall(&mut world, "show", (3, 2, 4));
    put_wall(&mut world, "show", (4, 2, 4));
    // One straight row of the lower half is flagged (not the corners), the old one is cleared.
    let rows = west_line(&world, 6);
    assert_eq!(
        rows.iter().filter(|f| **f == (true, true)).count(),
        1,
        "{rows:?}"
    );
    assert!(rows[2..4].contains(&(true, true)) && !rows[1].0, "{rows:?}");
    let bottom = world.exhibits().get(id_at(&world, 3, 3)).unwrap();
    assert_eq!(bottom.portals.len(), 1);
}

/// A play world's zoo read again, as a saved game: the info of the show tank is dropped from it.
fn reloaded(world: &World, clock: u32, keep: bool) -> World {
    let mut zoo = world.saved(|_| true);
    zoo.clocks.ai_ticks = clock;
    if !keep {
        zoo.shows.infos.iter_mut().for_each(|i| *i = None);
    }
    World::new(zoo, tank_shape)
}

#[test]
fn a_loaded_show_tank_whose_record_holds_no_info_keeps_a_fresh_one_and_a_stored_info_is_kept() {
    // 31.6 points 1, 2 and 5, O25.
    let mut world = trio(4, false);
    let n = crate::show::info_slot(&world, id_at(&world, 3, 2)).unwrap();
    world.zoo.shows.infos[n].as_mut().unwrap().frequency = 45;
    let counter = world.zoo.shows.next_show;
    let stored = reloaded(&world, 500, true);
    let info = stored.zoo.shows.infos.iter().flatten().next().unwrap();
    assert_eq!(
        (info.id, info.frequency),
        (1002, 45),
        "the stored info stays"
    );
    assert_eq!(stored.zoo.shows.next_show, counter + 2);
    let fresh = reloaded(&world, 500, false);
    let info = fresh.zoo.shows.infos.iter().flatten().next().unwrap();
    assert_eq!((info.id, info.frequency, info.next), (counter + 2, 90, 590));
    assert_eq!(fresh.zoo.shows.next_show, counter + 2);
}
