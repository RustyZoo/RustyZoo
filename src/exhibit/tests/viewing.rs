//! The viewing areas on pictures of a map, the line of sight on flat ground with one obstacle,
//! the pressure of the guests and the wear of the fences on a synthetic world.

use super::*;
use crate::exhibit::viewing::{Area, Body, Ground, Plan, Wear, areas, wear};
use crate::zoo::Fence;

/// A map from rows of text: `E` a fenced exhibit tile, `e` an unfenced one, `P` a path (inside
/// the zoo), `.` nothing.
fn plan(rows: &[&str]) -> Plan {
    let width = rows[0].len();
    let mut plan = Plan {
        width,
        height: rows.len(),
        ..Plan::default()
    };
    for (y, row) in rows.iter().enumerate() {
        for (x, c) in row.chars().enumerate() {
            plan.inside.push(matches!(c, 'E' | 'e'));
            plan.fenced.push(c == 'E');
            plan.path.push(c == 'P');
            plan.in_zoo.push(true);
            if matches!(c, 'E' | 'e') {
                plan.order.push(y * width + x);
            }
        }
    }
    plan
}

fn xy(plan: &Plan, a: &Area) -> Vec<(usize, usize)> {
    (a.tiles.iter())
        .map(|&t| (t % plan.width, t / plan.width))
        .collect()
}

/// The columns (axis 0) or rows (axis 1) the area covers, counted inclusive.
fn span(plan: &Plan, a: &Area, axis: usize) -> usize {
    let c: Vec<usize> = (a.tiles.iter())
        .map(|&t| [t % plan.width, t / plan.width][axis])
        .collect();
    c.iter().max().unwrap() - c.iter().min().unwrap() + 1
}

#[test]
fn a_long_path_beside_an_exhibit_is_cut_into_areas_of_four_tiles() {
    let p = plan(&["PPPPPPPPPPPP", "............", "EEEEEEEEEEEE"]);
    let a = areas(&p);
    assert_eq!(
        a.iter().map(|a| a.tiles.len()).collect::<Vec<_>>(),
        [4, 4, 4]
    );
    assert_eq!(xy(&p, &a[1]), [(4, 0), (5, 0), (6, 0), (7, 0)]);
    // A tile in the middle looks straight down (4) and along both diagonals down (3 and 5).
    assert_eq!(a[1].looks[0], 0b0011_1000);
    // The first tile has no diagonal to the west: the map ends.
    assert_eq!(a[0].looks[0], 0b0001_1000);
}

#[test]
fn an_area_covers_at_most_four_columns_and_four_rows() {
    let p = plan(&[
        "..........",
        ".PPPPPPPP.",
        ".PEEEEEEP.",
        ".PEEEEEEP.",
        ".PEEEEEEP.",
        ".PEEEEEEP.",
        ".PEEEEEEP.",
        ".PEEEEEEP.",
        ".PPPPPPPP.",
        "..........",
    ]);
    let a = areas(&p);
    assert_eq!(
        a.iter().map(|a| a.tiles.len()).sum::<usize>(),
        28,
        "every path tile"
    );
    for a in &a {
        assert!(
            span(&p, a, 0) <= 4 && span(&p, a, 1) <= 4,
            "{:?}",
            xy(&p, a)
        );
    }
    // The corner area starts at (1, 1) and takes the column down before the row along.
    assert_eq!(
        xy(&p, &a[0]),
        [(1, 1), (1, 2), (1, 3), (1, 4), (2, 1), (3, 1), (4, 1)]
    );
}

#[test]
fn only_path_tiles_within_two_tiles_of_a_fenced_tile_count() {
    let far = plan(&["P....", ".....", ".....", "..E..", "....."]);
    assert!(areas(&far).is_empty(), "3 rows away");
    let near = plan(&["..P..", ".....", "..E..", "....."]);
    assert_eq!(areas(&near).len(), 1);
    // An exhibit tile without a fence starts nothing.
    assert!(areas(&plan(&["..P..", ".....", "..e.."])).is_empty());
    // A path outside the zoo is left out.
    let mut outside = near.clone();
    outside.in_zoo[2] = false;
    assert!(areas(&outside).is_empty());
}

#[test]
fn a_tile_that_sees_the_exhibit_in_no_line_is_in_no_direction_and_joins_nobody() {
    // (5, 1) is two across and one down from the exhibit tile (3, 0): no straight or diagonal
    // line. (5, 2) sees it along the diagonal (direction 7), but its neighbour holds no
    // direction, so it starts an area of its own.
    let p = plan(&["...E..", ".....P", ".....P"]);
    let a = areas(&p);
    assert_eq!(a.len(), 2);
    assert_eq!(
        (a[0].looks.clone(), a[1].looks.clone()),
        (vec![0], vec![0b1000_0000])
    );
}

fn flat(width: usize) -> Ground {
    Ground::new(width, 2)
}

#[test]
fn flat_ground_is_clear_and_a_hill_between_blocks() {
    let mut g = flat(8);
    assert!(g.sees(0, 7) && g.sees(7, 0) && g.sees(3, 3));
    // The line is one step above the viewer: a hill of 1 step is below it, 2 steps above.
    g.base[3] = 1;
    assert!(g.sees(0, 7) && g.sees(7, 0));
    g.base[3] = 2;
    assert!(!g.sees(0, 7) && !g.sees(7, 0));
    // The eye of each end is above its own base, so a hill under it is seen.
    let mut g = flat(8);
    g.base[7] = 2;
    assert!(g.sees(0, 7) && g.sees(7, 0));
    g.base[6] = 5;
    assert!(!g.sees(0, 7) && !g.sees(7, 0));
}

#[test]
fn the_line_is_a_slope_from_one_eye_to_the_other() {
    // From a tile at elevation 2 down to one at 0 the line is at 6 near the start and at 2 near
    // the end (half steps), so a base of 2 (4 half steps) blocks near the end only.
    let mut g = flat(7);
    g.base[0] = 2;
    g.base[1] = 2;
    assert!(g.sees(0, 6));
    g.base[1] = 0;
    g.base[5] = 2;
    assert!(!g.sees(0, 6));
}

#[test]
fn scenery_blocks_while_its_top_is_above_the_line_in_half_steps() {
    let mut g = flat(8);
    // The line is at 2 half steps; the top is in steps and compared as it is.
    g.scenery[3] = [3; 4];
    assert!(!g.sees(0, 7));
    g.scenery[3] = [2; 4];
    assert!(g.sees(0, 7));
    g.scenery[3] = [i32::MIN; 4];
    assert!(g.sees(0, 7));
    // Only the quadrant the line goes through: the line runs along the top half of the row.
    g.scenery[3] = [i32::MIN, i32::MIN, 9, 9];
    assert!(g.sees(0, 7));
    g.scenery[3] = [9, 9, i32::MIN, i32::MIN];
    assert!(!g.sees(0, 7));
}

#[test]
fn a_fence_blocks_from_either_tile_when_its_top_is_above_the_eye() {
    for (tile, side) in [(2, 1), (3, 3)] {
        let mut g = flat(8);
        g.fence[tile][side] = 2;
        assert!(!g.sees(0, 7) && !g.sees(7, 0), "tile {tile} side {side}");
        // Not above the eye (one step above the viewer's base).
        g.fence[tile][side] = 1;
        assert!(g.sees(0, 7));
    }
    // A fence on a side the line does not cross.
    let mut g = flat(8);
    g.fence[3][0] = 6;
    assert!(g.sees(0, 7));
    // The eye is the viewer's: a viewer on a step sees over a fence the other would not.
    let mut g = flat(8);
    g.base[0] = 3;
    g.fence[3][1] = 3;
    assert!(g.sees(0, 7));
}

#[test]
fn a_diagonal_step_is_blocked_only_when_both_routes_round_the_corner_are_closed() {
    let mut g = Ground::new(5, 5);
    assert!(g.sees(0, 24));
    g.base[12] = 3;
    assert!(!g.sees(0, 24));
    g.base[12] = 0;
    // The first step goes from tile 0 to tile 6 across the corner: the east side of tile 0 closes
    // the route through tile 1, the south side the route through tile 5.
    g.fence[0][1] = 6;
    assert!(g.sees(0, 24));
    g.fence[0][2] = 6;
    assert!(!g.sees(0, 24));
    // The fence of the neighbour's slot counts on the far edge of a route too.
    g.fence[0][2] = i32::MIN;
    g.fence[5][1] = 6;
    assert!(!g.sees(0, 24));
}

#[test]
fn an_area_sees_the_exhibit_tiles_within_ten_tiles_and_in_line_of_sight() {
    let p = plan(&["P.............", "EEEEEEEEEEEEEE"]);
    let mut a = areas(&p).remove(0);
    let g = Ground::new(14, 2);
    a.observe(&p, &g);
    assert_eq!(
        a.seen,
        (14..24).collect::<Vec<_>>(),
        "up to 9 tiles along, 1 down"
    );
    // A hill in the way hides what is behind it, and observing again starts over.
    let mut hill = g.clone();
    hill.base[5] = 8;
    hill.base[19] = 8;
    a.observe(&p, &hill);
    // The tile on the hill is seen (the eye is above its base), what is behind it is not.
    assert_eq!(a.seen, (14..20).collect::<Vec<_>>());
}

#[test]
fn guests_press_on_an_animal_less_with_each_tile_of_distance() {
    let area = Area {
        tiles: vec![5, 6],
        looks: vec![0; 2],
        seen: Vec::new(),
    };
    let mut first = || 0;
    // Ten guests, the tile 2 away to the west of the animal: 10, 9, 8; west is neither of the
    // directions that gain.
    assert_eq!(area.pressure(10, 10, 7, &mut first), [8, -8, -8]);
    // The tile 2 away to the north (direction 0): the y part gains.
    assert_eq!(area.pressure(10, 10, 25, &mut first), [8, 8, -8]);
    // The second tile is drawn by `rand() % 2`: 1 away to the west.
    assert_eq!(area.pressure(10, 10, 7, &mut || 3), [9, -9, -9]);
    // Far away the guests thin out to a level and stay there (0.9 x 5 is 4.5, rounded to even).
    assert_eq!(area.pressure(10, 10, 99, &mut first)[0], 4);
    assert_eq!(area.pressure(10, 0, 7, &mut first), [0; 3]);
    // The animal on the tile has no direction: both parts lose.
    assert_eq!(area.pressure(10, 4, 5, &mut first), [4, -4, -4]);
}

#[test]
fn the_plan_of_an_exhibit_of_a_world_has_its_tiles_and_fenced_ones() {
    let world = World::new(zoo(&square(2, 2, 4, 4)), shape);
    let p = Plan::of(&world, 1);
    assert_eq!((p.width, p.height, p.order.len()), (8, 8, 9));
    assert_eq!(
        p.fenced.iter().filter(|&&f| f).count(),
        8,
        "not the middle tile"
    );
    assert!(p.path.iter().all(|&x| !x) && areas(&p).is_empty());
    assert_eq!(Plan::of(&world, 99).order, [], "no such exhibit");
}

/// The pieces as ordinary fences (a piece without a subtype is a zoo wall in these tests), or
/// as walls of a tank.
fn made(class: &str, mut zoo: Zoo) -> Zoo {
    for e in zoo.entities.iter_mut().filter(|e| e.class == "fences") {
        e.class = class.into();
        e.subtype = "f".into();
    }
    zoo
}

fn worn(world: &mut World, x: usize, y: usize, d: usize, life: i32) {
    let e = world.edge(y * 8 + x, d).unwrap();
    world.zoo.entities[e].common.as_mut().unwrap().fence = Some(Fence {
        strength: 500,
        life,
        timers: [0; 2],
        portal: false,
    });
}

fn states(world: &World) -> Vec<(u32, Wear)> {
    let mut all: Vec<_> = wear(world, |_| Some(4)).into_iter().collect();
    all.sort();
    all
}

#[test]
fn a_decayed_fence_wears_its_exhibit_and_a_broken_one_more() {
    let mut world = World::new(made("fences", zoo(&square(2, 2, 4, 4))), shape);
    assert_eq!(states(&world), [(1, Wear::Sound)]);
    worn(&mut world, 3, 2, 0, 5);
    assert_eq!(states(&world), [(1, Wear::Sound)], "above its decayed life");
    worn(&mut world, 3, 2, 0, 4);
    assert_eq!(states(&world), [(1, Wear::Decayed)]);
    worn(&mut world, 4, 4, 4, 0);
    worn(&mut world, 2, 3, 6, 2);
    assert_eq!(
        states(&world),
        [(1, Wear::Broken)],
        "never lowered by a decayed one"
    );
    // A type without a decayed life never decays.
    assert_eq!(wear(&world, |_| None)[&1], Wear::Sound);
}

#[test]
fn the_wear_goes_to_the_exhibit_across_unless_its_slot_is_taken_or_it_is_the_grounds() {
    let mut pieces = square(1, 1, 6, 6);
    pieces.extend(square(3, 3, 4, 4));
    let mut world = World::new(made("fences", zoo(&pieces)), shape);
    // The ring is 1, the inner square 2. The outer fence has the grounds across.
    worn(&mut world, 3, 1, 0, 3);
    assert_eq!(states(&world), [(1, Wear::Decayed), (2, Wear::Sound)]);
    // The inner fence wears the inner square and the ring across it.
    worn(&mut world, 3, 3, 0, 0);
    assert_eq!(states(&world), [(1, Wear::Broken), (2, Wear::Broken)]);
    // A second piece in the ring's own slot of the line keeps the ring out of it.
    let mut pieces = square(1, 1, 6, 6);
    pieces.extend(square(3, 3, 4, 4));
    pieces.push((3, 2, 4));
    let mut world = World::new(made("fences", zoo(&pieces)), shape);
    worn(&mut world, 3, 3, 0, 0);
    assert_eq!(states(&world), [(1, Wear::Sound), (2, Wear::Broken)]);
}

#[test]
fn a_tank_wall_never_breaks_and_the_ground_reads_the_fences_of_a_world() {
    let mut world = World::new(made("tank", zoo(&square(2, 2, 4, 4))), shape);
    worn(&mut world, 3, 2, 0, 0);
    assert_eq!(states(&world), [(1, Wear::Decayed)], "state 1 only");
    let world = World::new(made("fences", zoo(&square(2, 2, 4, 4))), shape);
    let body = |_: &crate::zoo::Entity| Body {
        height: 3,
        blocks: true,
        see_through: false,
    };
    let g = Ground::of(&world, body);
    assert_eq!(g.fence[2 * 8 + 3], [3, i32::MIN, i32::MIN, i32::MIN]);
    assert_eq!(g.fence[3 * 8 + 3], [i32::MIN; 4], "the middle tile");
    let clear = |_: &crate::zoo::Entity| Body {
        height: 3,
        blocks: true,
        see_through: true,
    };
    assert_eq!(
        Ground::of(&world, clear).fence[2 * 8 + 3][0],
        i32::MIN,
        "a see-through fence that is not a tank wall never blocks"
    );
}
