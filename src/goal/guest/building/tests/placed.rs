//! A guest put down with a pressing need takes the nearest building that serves a need (unit
//! placing 8.5 point 3), on the map of the parent module: guest 1 at (2, 6), a stand 7 at (5, 5)
//! and a restroom 8 at (5, 8).

use super::*;

fn drop_guest(cat: &Catalogue, w: &mut World, rng: &mut Rng) {
    crate::goal::dropped(w, cat, rng, &walks(), 1);
}

/// Moves building `id` to `tile`.
fn put(w: &mut World, id: u32, tile: (i32, i32)) {
    let at = w.find(id).unwrap();
    let c = w.zoo.entities[at].common.as_mut().unwrap();
    (c.x, c.y) = centre(tile);
}

fn wandered(w: &World) -> bool {
    w.units.get(1).unwrap().goal.wandered
}

/// A hungry guest at `guest`, the stand at `stand` and the restroom at `restroom`, put down.
fn dropped(guest: (i32, i32), stand: (i32, i32), restroom: (i32, i32)) -> (World, Rng) {
    let (cat, mut w, mut rng) = (types(), zoo(), Rng::default());
    hungry(&mut w);
    for (id, tile) in [(1, guest), (7, stand), (8, restroom)] {
        put(&mut w, id, tile);
    }
    drop_guest(&cat, &mut w, &mut rng);
    (w, rng)
}

#[test]
fn a_guest_dropped_with_a_pressing_need_takes_the_nearest_satisfier_within_10_tiles() {
    // The restroom is nearer: it is taken, for its need, though the guest is hungry.
    let (w, _) = dropped((0, 0), (3, 3), (2, 2));
    assert_eq!((guest(&w).building, guest(&w).need), (8, Some("bathroom")));
    assert!(!wandered(&w));
    // The stand is out of reach (162 squared tiles), the restroom at 100 is not.
    let (w, _) = dropped((0, 0), (9, 9), (6, 8));
    assert_eq!(guest(&w).building, 8);
    let (w, _) = dropped((0, 0), (6, 8), (9, 9));
    assert_eq!((guest(&w).building, guest(&w).need), (7, Some("food")));
}

#[test]
fn a_guest_dropped_with_a_pressing_need_draws_one_number_among_equally_near_satisfiers() {
    // The stand and the restroom are both 10 squared tiles away.
    let mut expect = Rng::default();
    let n = expect.pick(2) as usize;
    let (w, rng) = dropped((2, 6), (5, 5), (5, 7));
    assert_eq!(guest(&w).building, [7, 8][n]);
    assert_eq!(rng, expect);
}

#[test]
fn a_guest_dropped_with_a_pressing_need_and_no_satisfier_is_marked_wandered() {
    let (w, rng) = dropped((0, 0), (9, 9), (9, 8));
    assert_eq!((guest(&w).building, wandered(&w)), (0, true));
    assert_eq!(rng, Rng::default());
}
