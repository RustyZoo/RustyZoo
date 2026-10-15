//! The viewing areas on pictures of a map, the line of sight on flat ground with one obstacle,
//! the pressure of the guests and the wear of the fences on a synthetic world.

use super::*;
use crate::exhibit::viewing::{Area, Ground, Plan, Wear, areas, wear};
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
    g.corner[3] = [6; 4];
    assert!(!g.sees(0, 7) && !g.sees(7, 0));
    assert!(
        g.sees(0, 2) && g.sees(0, 3),
        "the tile itself is the end of the line"
    );
    // A rise of one step is below the line, which is one step above the tile.
    g.corner[3] = [2; 4];
    assert!(g.sees(0, 7));
}

#[test]
fn the_line_is_a_slope_from_one_tile_to_the_other() {
    // From a tile at elevation 2 down to one at 0: the line is at 5 near the start and at 3
    // near the end (in half steps), so a rise of 4 blocks only near the end.
    let mut g = flat(7);
    g.base[0] = 2;
    g.corner[0] = [4; 4];
    g.corner[1] = [4; 4];
    assert!(g.sees(0, 6));
    g.corner[1] = [0; 4];
    g.corner[5] = [4; 4];
    assert!(!g.sees(0, 6));
}

#[test]
fn scenery_blocks_only_above_the_line() {
    let mut g = flat(8);
    g.scenery[3] = [4; 4];
    assert!(!g.sees(0, 7));
    g.scenery[3] = [2; 4];
    assert!(g.sees(0, 7));
    g.scenery[3] = [i32::MIN; 4];
    assert!(g.sees(0, 7));
}

#[test]
fn a_fence_blocks_from_either_tile_and_only_above_the_line() {
    for (tile, side) in [(2, 1), (3, 3)] {
        let mut g = flat(8);
        g.fence[tile][side] = 6;
        assert!(!g.sees(0, 7) && !g.sees(7, 0), "tile {tile} side {side}");
        g.fence[tile][side] = 2;
        assert!(g.sees(0, 7));
    }
    // A fence on a side the line does not cross.
    let mut g = flat(8);
    g.fence[3][0] = 6;
    assert!(g.sees(0, 7));
}

#[test]
fn a_diagonal_line_crosses_the_x_edge_then_the_y_edge() {
    let mut g = Ground::new(5, 5);
    assert!(g.sees(0, 24));
    g.corner[12] = [6; 4];
    assert!(!g.sees(0, 24));
    g.corner[12] = [0; 4];
    // The fence on the +x side of the first tile.
    g.fence[0][1] = 6;
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
    hill.corner[5] = [8; 4];
    hill.corner[19] = [8; 4];
    a.observe(&p, &hill);
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
    // Far away the guests thin out to a level and stay there; none, none.
    assert_eq!(area.pressure(10, 10, 99, &mut first)[0], 5);
    assert_eq!(area.pressure(10, 0, 7, &mut first), [0; 3]);
    // The animal on the tile: direction 0.
    assert_eq!(area.pressure(10, 4, 5, &mut first), [4, 4, -4]);
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

fn worn(world: &mut World, x: usize, y: usize, d: usize, life: u32) {
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
    let mut world = World::new(zoo(&square(2, 2, 4, 4)), shape);
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
    let mut world = World::new(zoo(&pieces), shape);
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
    let mut world = World::new(zoo(&pieces), shape);
    worn(&mut world, 3, 3, 0, 0);
    assert_eq!(states(&world), [(1, Wear::Sound), (2, Wear::Broken)]);
}
