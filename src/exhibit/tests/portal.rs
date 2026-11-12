//! The portal wall pair of the show neighbours (shows 31.3 points 5 to 8, 31.5, 31.6 point 3) on
//! flat synthetic ground: a plain tank on the left, a show tank built against its east side.

use super::decision::{build, id_at};
use crate::show::Move;
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

/// The links of the show tank are made again by an edit that is a trigger of shows 34.4: the plain
/// tank at column 1 is filled, filled already.
fn touch(world: &mut World) {
    world.begin();
    let plain = id_at(world, 1, 2);
    world.fill(plain);
}

/// An edit that is no trigger: a piece far from the tanks.
fn far(world: &mut World) {
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
    // A far edit recomputes nothing: the byte stays. A trigger makes the same pair again: the flag
    // goes 0 then 1 and the byte is 0 (34.5 point 3).
    far(&mut world);
    assert!(is_open(&world, 3, 3, 6));
    touch(&mut world);
    assert!(flag(&world, 3, 3, 6) && !is_open(&world, 3, 3, 6));
    open(&mut world, 3, 3, 6);
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

/// Plain tank on columns 1 and 2 down to row `plain`, show tank on columns 3 and 4 down to `show`.
fn uneven(plain: i32, show: i32) -> World {
    let mut world = tanks(&[], &[], &[]);
    build(&mut world, "tank", (1, 2, 2, plain));
    world.begin();
    for (class, p) in ring("show", 3, 2, 4, show) {
        put_wall(&mut world, class, p);
    }
    world
}

#[test]
fn a_pair_is_not_straight_when_only_one_of_its_two_tiles_holds_a_single_piece() {
    for (plain, show, rows) in [(3, 5, 3), (5, 3, 3)] {
        let w = uneven(plain, show);
        let s = w.exhibits().get(id_at(&w, 3, 2)).unwrap();
        assert_eq!(s.show_near, [id_at(&w, 1, 2)], "linked ({plain}, {show})");
        assert!(s.portals.is_empty(), "({plain}, {show})");
        assert!((2..=rows).all(|y| !flag(&w, 3, y, 6) && !flag(&w, 2, y, 2)));
    }
}

/// The portal animation of the tests: four frames of 100 ms.
const FRAMES: crate::behaviour::Timing = crate::behaviour::Timing {
    count: 4,
    delay: 100,
};

/// The position and the id of the portal piece on the west line of the show tank.
fn portal_piece(world: &World) -> (usize, u32) {
    let at = piece(world, 3, 3, 6);
    (at, world.zoo.entities[at].common.as_ref().unwrap().id)
}

/// The frame the wall `id` shows, among four; a wall without a player shows frame 0.
fn shown(world: &World, id: u32) -> u32 {
    let found = world.portal_frames().find(|f| f.0 == id);
    found.map_or(0, |f| f.1.min(3))
}

/// `set_open` of shows 21.5 on the portal piece.
fn open_it(world: &mut World, at: usize, open: bool) {
    let cat = crate::entity::Catalogue::default();
    crate::show::portals::set_open(world, &cat, Some(at), open, false);
}

/// Steps of `ms` milliseconds: the frame shown after each.
fn run(world: &mut World, at: usize, ms: u32, n: usize) -> Vec<u32> {
    let id = portal_piece(world).1;
    (0..n)
        .map(|_| {
            world.portal_advance(at, Some(FRAMES), ms);
            shown(world, id)
        })
        .collect()
}

#[test]
fn a_portal_opens_through_its_frames_closes_back_down_and_rests_on_the_last_or_the_first() {
    // 32.6 points 2 to 5: each frame lasts its delay, the open portal rests on the last frame,
    // the closed one on frame 0.
    let mut world = trio(4, false);
    let (at, id) = portal_piece(&world);
    assert_eq!(shown(&world, id), 0, "closed at rest");
    open_it(&mut world, at, true);
    assert_eq!(run(&mut world, at, 100, 6), [1, 2, 3, 3, 3, 3]);
    open_it(&mut world, at, false);
    assert_eq!(run(&mut world, at, 100, 6), [2, 1, 0, 0, 0, 0]);
}

#[test]
fn a_change_before_the_end_restarts_from_the_other_end_with_a_jump() {
    // 32.6 point 6.
    let mut world = trio(4, false);
    let (at, _) = portal_piece(&world);
    open_it(&mut world, at, true);
    assert_eq!(run(&mut world, at, 50, 4), [0, 1, 1, 2]);
    open_it(&mut world, at, false);
    assert_eq!(
        shown(&world, portal_piece(&world).1),
        3,
        "the other end at once"
    );
    assert_eq!(run(&mut world, at, 50, 3), [3, 2, 2]);
}

#[test]
fn a_portal_takes_its_new_direction_at_once() {
    // 32.6 points 2 and 6: the call that sets the base clears the player, before any advance:
    // opening shows frame 0, closing the last frame, also when it interrupts a run.
    let mut world = trio(4, false);
    let (at, id) = portal_piece(&world);
    open_it(&mut world, at, true);
    assert_eq!(shown(&world, id), 0);
    run(&mut world, at, 100, 6);
    open_it(&mut world, at, false);
    assert_eq!(shown(&world, id), 3);
    run(&mut world, at, 100, 1);
    open_it(&mut world, at, true);
    assert_eq!(shown(&world, id), 0, "a reopening interrupts the closing");
}

#[test]
fn a_flagged_portal_shows_its_closed_picture_at_once_whatever_it_showed() {
    // 32.6 point 7: the flag setter clears open and the player; the portal ends closed, byte 0
    // and frame 0, from a run or from the open rest frame.
    for (run_for, closed_first) in [(2, false), (6, false), (6, true)] {
        let mut world = trio(4, false);
        let (at, id) = portal_piece(&world);
        open_it(&mut world, at, true);
        run(&mut world, at, 100, run_for);
        if closed_first {
            open_it(&mut world, at, false);
            run(&mut world, at, 100, 1);
        }
        assert_ne!(shown(&world, id), 0);
        world.set_flag(at, false);
        world.set_flag(at, true);
        let fence = |w: &World| w.zoo.entities[at].common.as_ref().unwrap().fence.unwrap();
        assert!(
            !fence(&world).open && fence(&world).portal,
            "byte 0, flag set"
        );
        assert_eq!(shown(&world, id), 0, "frame 0 at once");
        assert_eq!(run(&mut world, at, 100, 5), [0; 5], "and it stays");
        // The next opening plays from the first frame.
        open_it(&mut world, at, true);
        assert_eq!(run(&mut world, at, 100, 4), [1, 2, 3, 3]);
    }
}

#[test]
fn an_unrelated_edit_leaves_a_staying_portal_open() {
    // 32.6 point 7: an entity added far from the tanks (a guest arriving) does not touch a
    // portal: byte and frame stay.
    let mut world = trio(4, false);
    let (at, id) = portal_piece(&world);
    open_it(&mut world, at, true);
    run(&mut world, at, 100, 6);
    far(&mut world);
    let fence = world.zoo.entities[at]
        .common
        .as_ref()
        .unwrap()
        .fence
        .unwrap();
    assert!(fence.open && fence.portal);
    assert_eq!(shown(&world, id), 3);
}

#[test]
fn a_loaded_portal_shows_the_rest_picture_of_its_saved_byte() {
    // 32.6 point 8 and O29: the open byte is all that is saved; a flag the recompute leaves
    // alone shows its picture too.
    let mut world = trio(4, false);
    let (at, id) = portal_piece(&world);
    open_it(&mut world, at, true);
    run(&mut world, at, 50, 1);
    let mut saved = reloaded(&world, 0, true);
    let (at2, id2) = portal_piece(&saved);
    assert_eq!(id2, id);
    assert!(
        saved.zoo.entities[at2]
            .common
            .as_ref()
            .unwrap()
            .fence
            .unwrap()
            .open
    );
    assert_eq!(shown(&saved, id), 3, "open: the last frame");
    assert_eq!(run(&mut saved, at2, 500, 3), [3, 3, 3], "nothing animates");
    open_it(&mut world, at, false);
    run(&mut world, at, 500, 1);
    let closed = reloaded(&world, 0, true);
    assert_eq!(shown(&closed, id), 0, "closed: frame 0");
    // A piece of a line to the grounds, flagged and open in the file, is not a pair of the link.
    let (north, flagged) = (piece(&world, 3, 2, 0), reloaded_id(&world, 3, 2, 0));
    let mut zoo = world.saved(|_| true);
    let fence = zoo.entities[north]
        .common
        .as_mut()
        .unwrap()
        .fence
        .as_mut()
        .unwrap();
    (fence.portal, fence.open) = (true, true);
    let loaded = World::new(zoo, tank_shape);
    assert_eq!(shown(&loaded, flagged), 3);
}

/// The id of the piece on side `d` of tile (`x`, `y`).
fn reloaded_id(world: &World, x: i32, y: i32, d: usize) -> u32 {
    world.zoo.entities[piece(world, x, y, d)]
        .common
        .as_ref()
        .unwrap()
        .id
}

/// The abort events (kind, target) the load of the world saved from `world` posts, every tank
/// stored empty, as version `version`; the infos are dropped from the file when `keep` is false.
fn load_posts(world: &World, version: u32, keep: bool) -> (World, Vec<(u16, u32)>) {
    let mut zoo = world.saved(|_| true);
    zoo.version = version;
    zoo.tanks.iter_mut().for_each(|t| t.level = 0);
    if !keep {
        zoo.shows.infos.iter_mut().for_each(|i| *i = None);
    }
    let loaded = World::new(zoo, tank_shape);
    let sent = loaded.units.posts.iter();
    let sent = sent.map(|e| (e.kind, e.target)).collect();
    (loaded, sent)
}

#[test]
fn the_drains_of_a_load_post_the_id_the_tank_holds_then_before_version_97_and_nothing_after() {
    // 30.4 point 6: the id is read live, so a show tank with no stored info posts the fresh
    // info's id (never 0), and a plain tank posts 0; from 97 the event queue load empties them.
    let world = trio(4, false);
    let abort = crate::show::event::ABORT;
    let (loaded, sent) = load_posts(&world, 90, false);
    let fresh = loaded.zoo.shows.infos.iter().flatten().next().unwrap().id;
    assert_ne!(fresh, 0);
    assert_eq!(sent, [(abort, 0), (abort, u32::from(fresh))]);
    let stored = world.zoo.shows.infos.iter().flatten().next().unwrap().id;
    let (_, sent) = load_posts(&world, 90, true);
    assert_eq!(sent, [(abort, 0), (abort, u32::from(stored))]);
    for keep in [false, true] {
        assert_eq!(load_posts(&world, 97, keep).1, [], "kept {keep}");
        assert_eq!(load_posts(&world, 106, keep).1, [], "kept {keep}");
    }
}

/// The show tank between two plain tanks and a far plain tank, a crossing at the west portal
/// (open, frame 3): the piece's position and id.
fn crossing() -> (World, (usize, u32)) {
    let mut world = trio(4, true);
    build(&mut world, "tank", (1, 6, 2, 7));
    let portal = portal_piece(&world);
    open_it(&mut world, portal.0, true);
    run(&mut world, portal.0, 100, 6);
    (world, portal)
}

fn is_open(world: &World, at: usize) -> bool {
    let fence = world.zoo.entities[at].common.as_ref().and_then(|c| c.fence);
    fence.is_some_and(|f| f.open)
}

#[test]
fn a_drain_far_from_the_show_tank_leaves_its_portal_open_and_a_neighbours_closes_it() {
    // 34.4 points 2, 3 and 13, 34.5 points 3 and 5.
    let (mut world, (at, id)) = crossing();
    let far = id_at(&world, 1, 6);
    let n2 = id_at(&world, 5, 2);
    world.drain(far);
    assert!(is_open(&world, at) && shown(&world, id) == 3);
    world.fill(far);
    assert!(is_open(&world, at) && shown(&world, id) == 3);
    world.drain(n2);
    assert!(flag(&world, 3, 3, 6));
    assert!(!is_open(&world, at) && shown(&world, id) == 0);
}

#[test]
fn a_recompute_drops_and_makes_again_the_link_of_a_neighbour_that_stays() {
    // 34.5 points 1 and 3.2: the performers of N1 leave the show and join it again, behind the
    // show tank's own.
    let (mut world, _) = crossing();
    let (n1, n2, far) = (id_at(&world, 1, 2), id_at(&world, 5, 2), id_at(&world, 1, 6));
    let links = |world: &mut World, plain: u32| -> Vec<bool> {
        let moves = world.take_moves();
        let on = |m: &Move| match m {
            Move::Link { plain: p, on, .. } if *p == plain => Some(*on),
            _ => None,
        };
        moves.iter().filter_map(on).collect()
    };
    world.take_moves();
    world.drain(far);
    assert_eq!(links(&mut world, n1), []);
    world.drain(n2);
    assert_eq!(links(&mut world, n1), [false, true]);
}

#[test]
fn a_terrain_edit_recomputes_only_when_the_game_is_not_paused() {
    // 34.4 point 7: a tile of a plain tank repainted.
    let (mut world, (at, id)) = crossing();
    let tile = world.zoo.tiles[world.tile_index(1, 3).unwrap()];
    let paint = |w: &mut World| {
        w.set_tile(1, 3, tile.elevation, tile.shape, tile.terrain ^ 1);
    };
    world.set_paused(true);
    paint(&mut world);
    assert!(is_open(&world, at) && shown(&world, id) == 3);
    world.set_paused(false);
    paint(&mut world);
    assert!(!is_open(&world, at) && shown(&world, id) == 0);
}
