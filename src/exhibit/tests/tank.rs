//! Tank exhibits on synthetic maps: which exhibits are tanks, which are show tanks, the tank
//! list the picture reads, and the default names.

use super::*;
use crate::exhibit::Tank;
use crate::world::Step;

type Piece = (&'static str, (i32, i32, u32));

/// A wall piece of a class of `shape`: `tank`, `show`, or an ordinary `fences` piece.
fn wall(class: &str, (x, y, d): (i32, i32, u32)) -> Entity {
    let mut e = entity(class, x, y, d);
    e.subtype = "f".into();
    e
}

/// The map with these pieces already on it, as if loaded.
fn built(pieces: &[Piece]) -> World {
    let mut z = zoo(&[]);
    z.entities
        .extend(pieces.iter().map(|&(class, p)| wall(class, p)));
    unique(&mut z);
    World::new(z, shape)
}

/// The pieces of `class` round the square of tiles `x0..=x1` by `y0..=y1`.
pub(super) fn ring(class: &'static str, x0: i32, y0: i32, x1: i32, y1: i32) -> Vec<Piece> {
    square(x0, y0, x1, y1)
        .into_iter()
        .map(|p| (class, p))
        .collect()
}

pub(super) fn tank(world: &World, x: usize, y: usize) -> Option<Tank> {
    let t = y * SIZE as usize + x;
    world.tank_at(t).and_then(|id| world.tank(id))
}

#[test]
fn an_exhibit_closed_by_tank_walls_only_is_a_tank() {
    let world = built(&ring("tank", 2, 2, 3, 3));
    assert_eq!(sizes(&world), [4]);
    assert_eq!(tank(&world, 2, 3).map(|t| t.show), Some(false));
    assert_eq!(world.tank_at(0), None, "the grounds");
    // One piece of another fence makes it a land exhibit.
    let mut pieces = ring("tank", 2, 2, 3, 3);
    pieces[0].0 = "fences";
    let world = built(&pieces);
    assert_eq!(sizes(&world), [4]);
    assert_eq!(tank(&world, 2, 2), None);
}

#[test]
fn the_wall_of_either_tile_of_a_line_counts() {
    // The east line of the tank holds a tank wall on the slot of the tile outside only.
    let mut pieces = ring("tank", 2, 2, 3, 3);
    pieces.retain(|&(_, p)| p != (3, 2, 2) && p != (3, 3, 2));
    pieces.extend([("tank", (4, 2, 6)), ("tank", (4, 3, 6))]);
    assert!(tank(&built(&pieces), 2, 2).is_some());
    // A fence of another type inside and a tank wall outside: land.
    let mut pieces = ring("tank", 2, 2, 3, 3);
    pieces[2].0 = "fences";
    pieces.push(("tank", (2, 1, 4)));
    assert_eq!(tank(&built(&pieces), 2, 2), None);
}

#[test]
fn a_tank_is_a_show_tank_when_every_line_passes_and_one_has_a_show_wall() {
    assert_eq!(
        tank(&built(&ring("show", 2, 2, 3, 3)), 2, 2).map(|t| t.show),
        Some(true)
    );
    // One normal wall among show walls: not a show tank.
    let mut pieces = ring("show", 2, 2, 3, 3);
    pieces[0].0 = "tank";
    assert_eq!(tank(&built(&pieces), 2, 2).map(|t| t.show), Some(false));
    // The same with a second wall on the line, from the tile outside: the line passes.
    pieces.push(("show", (2, 1, 4)));
    assert_eq!(tank(&built(&pieces), 2, 2).map(|t| t.show), Some(true));
}

#[test]
fn a_show_wall_across_stands_for_the_tank_whose_wall_it_is_on_a_tile_of_another_tank() {
    // A normal tank (1..2) and a show tank (3..4) side by side, each with the wall of its own
    // tile on the shared line: each has the kind of its walls.
    let mut pieces = ring("tank", 1, 2, 2, 3);
    pieces.extend(ring("show", 3, 2, 4, 3));
    pieces.retain(|&(_, p)| !(p.2 == 2 && p.0 == 2) && !(p.2 == 6 && p.0 == 3));
    pieces.extend([("tank", (2, 2, 2)), ("tank", (2, 3, 2))]);
    pieces.extend([("show", (3, 2, 6)), ("show", (3, 3, 6))]);
    let world = built(&pieces);
    assert_eq!(tank(&world, 1, 2).map(|t| t.show), Some(false));
    assert_eq!(tank(&world, 4, 2).map(|t| t.show), Some(true));
    // Only the walls of the show tank on the shared line: the normal tank stands on a line
    // whose only wall is a show wall, but its other lines have normal ones: not a show tank.
    let mut only = pieces.clone();
    only.retain(|&(_, p)| !(p.0 == 2 && p.2 == 2));
    let world = built(&only);
    assert_eq!(tank(&world, 1, 2).map(|t| t.show), Some(false));
    assert_eq!(tank(&world, 4, 2).map(|t| t.show), Some(true));
}

#[test]
fn a_line_with_only_a_wall_across_passes_when_that_stands_on_a_tank_tile() {
    // The show tank (1..2) has show walls on three sides and on its east line only the plain
    // wall of the tank beside it (3..4), a tank tile: the line passes, so it is a show tank. If
    // the neighbour is a land exhibit instead, the line fails.
    let mut pieces = ring("show", 1, 2, 2, 3);
    pieces.retain(|&(_, p)| !(p.0 == 2 && p.2 == 2));
    let mut beside = ring("tank", 3, 2, 4, 3);
    let mut world = {
        let mut all = pieces.clone();
        all.append(&mut beside.clone());
        built(&all)
    };
    assert_eq!(tank(&world, 1, 2).map(|t| t.show), Some(true));
    assert_eq!(tank(&world, 4, 2).map(|t| t.show), Some(false));
    beside[0].0 = "fences";
    pieces.append(&mut beside);
    world = built(&pieces);
    assert_eq!(tank(&world, 1, 2).map(|t| t.show), Some(false));
    assert_eq!(tank(&world, 4, 2), None);
}

#[test]
fn the_tank_records_of_a_file_give_the_tanks_their_water() {
    let mut z = zoo(&[]);
    z.entities.extend(
        ring("show", 2, 2, 3, 3)
            .into_iter()
            .map(|(c, p)| wall(c, p)),
    );
    unique(&mut z);
    z.tanks = vec![crate::zoo::Tank {
        tile: (2, 2),
        gate: (3, 3),
        height: 9,
        level: 6,
        salt: false,
        purity: 80,
    }];
    let mut world = World::new(z, shape);
    let t = tank(&world, 3, 3).unwrap();
    assert_eq!(
        (t.show, t.height, t.level, t.salt, t.purity),
        (true, 9, 6, false, 80)
    );
    // The list the picture reads is the same, and follows the tanks that change.
    assert_eq!(world.zoo.tanks.len(), 1);
    assert_eq!(
        (world.zoo.tanks[0].tile, world.zoo.tanks[0].gate),
        ((2, 2), (3, 3))
    );
    assert!(!world.take_dirty(), "opening a map changes nothing");
    let side = at(&world, 2, 2, 0);
    world.remove(side);
    assert!(world.zoo.tanks.is_empty(), "a land exhibit is no tank");
    assert!(world.take_dirty());
}

#[test]
fn a_tank_closed_by_the_player_has_the_name_of_a_tank_or_a_show_tank() {
    let mut pieces = ring("tank", 2, 2, 3, 3);
    let last = pieces.pop().unwrap();
    let mut world = built(&pieces);
    world.set_exhibit_words(["Exhibit".into(), "Tank".into(), "Show Tank".into()]);
    world.add(wall(last.0, last.1), shape(&wall(last.0, last.1)));
    for id in world.waiting() {
        world.give_gate(id);
    }
    assert_eq!(names(&world), [(1, "Tank 1".to_string(), 4)]);
    assert!(tank(&world, 2, 2).is_some());
    assert_eq!(world.zoo.tanks.len(), 1);
    // A show tank next to it takes the counter on.
    for (c, p) in ring("show", 5, 5, 6, 6) {
        world.add(wall(c, p), shape(&wall(c, p)));
        for id in world.waiting() {
            world.give_gate(id);
        }
    }
    assert_eq!(names(&world)[1], (2, "Show Tank 2".to_string(), 4));
    assert_eq!(tank(&world, 5, 5).map(|t| t.show), Some(true));
    // A land exhibit is still "Exhibit".
    for (c, p) in ring("fences", 0, 5, 0, 6) {
        world.add(wall(c, p), shape(&wall(c, p)));
        for id in world.waiting() {
            world.give_gate(id);
        }
    }
    assert_eq!(names(&world)[2].1, "Exhibit 3");
}

#[test]
fn the_tiles_of_a_tank_are_carried_by_any_tile_of_it() {
    let world = built(&ring("tank", 2, 2, 3, 3));
    let t = |x: usize, y: usize| y * SIZE as usize + x;
    let mut tiles = world.carried(t(3, 2));
    tiles.sort_unstable();
    assert_eq!(tiles, [t(2, 2), t(3, 2), t(2, 3), t(3, 3)]);
    assert!(world.carried(t(0, 0)).is_empty());
}

// ---- Making a tank

use crate::exhibit::TankConfig;
use crate::zoo::Tile;

/// The shapes of `shape`, and: a `fish` (a unit that lives under water), a `dog` (one that does
/// not), a `bench` (scenery that does not), a `toy` (scenery that lives under water), a `ball`
/// (scenery for shows).
pub(super) fn tank_shape(e: &Entity) -> Option<Shape> {
    let thing = |kind, tank| {
        Some(Shape {
            kind,
            footprint: (1, 1),
            fence: false,
            wall: false,
            gate: false,
            gated: false,
            tank,
            walkable: (false, false),
        })
    };
    let none = TankKind::NONE;
    match e.class.as_str() {
        "fish" | "animals" => thing(
            Kind::Unit,
            TankKind {
                underwater: true,
                ..none
            },
        ),
        "dog" => thing(Kind::Unit, none),
        "guests" => thing(Kind::Unit, none),
        "bench" => thing(Kind::Fixed, none),
        "toy" => thing(
            Kind::Fixed,
            TankKind {
                underwater: true,
                ..none
            },
        ),
        "ball" => thing(
            Kind::Fixed,
            TankKind {
                surface: true,
                shows: true,
                ..none
            },
        ),
        // A tank wall of the class the gate code knows.
        "tankwall" => Some(Shape {
            kind: Kind::Wall,
            footprint: (1, 1),
            fence: true,
            wall: false,
            gate: false,
            gated: true,
            walkable: (false, false),
            tank: TankKind {
                wall: true,
                underwater: true,
                ..none
            },
        }),
        _ => shape(e),
    }
}

/// The values a new tank gets: height 5 (9 for a show tank), ground lowered by 4 (8), salt water
/// of purity 90, filled.
pub(super) const CONFIG: TankConfig = TankConfig {
    sink: [4, 8],
    height: [5, 9],
    level: 0,
    filled: true,
    salt: true,
    purity: 90,
    terrain: [2, 17],
    offset: -1,
    limit: 20,
    murky: 60,
};

/// A map of tiles with these elevations by tile (0 elsewhere), and the pieces, with the
/// configuration set.
pub(super) fn tanks(pieces: &[Piece], things: &[(&str, i32, i32)], high: &[(usize, i32)]) -> World {
    let mut z = zoo(&[]);
    z.entities
        .extend(pieces.iter().map(|&(class, p)| wall(class, p)));
    z.entities.extend(things.iter().map(|&(c, x, y)| {
        let mut e = entity(c, 0, 0, 0);
        let at = e.common.as_mut().unwrap();
        (at.x, at.y) = (x, y);
        e
    }));
    unique(&mut z);
    for &(t, e) in high {
        z.tiles[t] = Tile {
            elevation: e,
            ..z.tiles[t]
        };
    }
    let mut world = World::new(z, tank_shape);
    world.set_tank_config(CONFIG);
    world
}

pub(super) fn put_wall(world: &mut World, class: &str, p: (i32, i32, u32)) -> u32 {
    let e = wall(class, p);
    let id = world.add(e.clone(), tank_shape(&e)).unwrap();
    for new in world.waiting() {
        world.give_gate(new);
    }
    id
}

pub(super) fn level(world: &World, x: usize, y: usize) -> (i32, u8) {
    let t = world.zoo.tiles[y * SIZE as usize + x];
    (t.elevation, t.shape)
}

#[test]
fn a_new_tank_is_levelled_at_the_mean_less_the_sink_and_the_ground_comes_back_on_undo() {
    // Tiles of (2..3, 2..3) at levels 2, 3, 4 and 3: the mean is 3, less 4 is -1, flat.
    let mut pieces = ring("tank", 2, 2, 3, 3);
    let last = pieces.pop().unwrap();
    let t = |x: usize, y: usize| y * SIZE as usize + x;
    let mut world = tanks(
        &pieces,
        &[],
        &[(t(2, 2), 2), (t(3, 2), 3), (t(2, 3), 4), (t(3, 3), 3)],
    );
    world.zoo.tiles[t(3, 3)].shape = 0x05;
    world.begin();
    world.take_events();
    put_wall(&mut world, last.0, last.1);
    for (x, y) in [(2, 2), (3, 2), (2, 3), (3, 3)] {
        assert_eq!(level(&world, x, y), (-1, 0), "tile {x},{y}");
    }
    assert_eq!(level(&world, 4, 4), (0, 0), "the ground beside it stays");
    // The numbers of tanks.cfg: height 5, nothing else yet filled, water of purity 90.
    let tank = tank(&world, 2, 2).unwrap();
    assert_eq!(
        (tank.height, tank.level, tank.filled, tank.salt, tank.purity),
        (5, 0, true, true, 90)
    );
    assert!(!world.undo().is_empty());
    assert_eq!(level(&world, 2, 3), (4, 0));
    assert_eq!(level(&world, 3, 3), (3, 0x05), "the shape is back too");
    assert!(world.exhibits().list().is_empty());
}

#[test]
fn a_show_tank_sinks_further_and_has_higher_walls() {
    let mut pieces = ring("show", 2, 2, 3, 3);
    let last = pieces.pop().unwrap();
    let mut world = tanks(&pieces, &[], &[]);
    put_wall(&mut world, last.0, last.1);
    assert_eq!(level(&world, 2, 2), (-8, 0));
    assert_eq!(
        tank(&world, 3, 3).map(|t| (t.show, t.height)),
        Some((true, 9))
    );
}

#[test]
fn the_walls_stand_on_the_tanks_side_after_the_wall_step_and_go_back_on_undo() {
    // The tank (2..3, 2..2) is closed by walls on the tiles outside its two long sides and ends.
    let pieces = [
        ("tank", (2, 1, 4)),
        ("tank", (3, 1, 4)),
        ("tank", (2, 3, 0)),
        ("tank", (3, 3, 0)),
        ("tank", (1, 2, 2)),
        ("tank", (4, 2, 6)),
    ];
    let mut world = tanks(&pieces[..5], &[], &[]);
    world.begin();
    world.take_events();
    let count = world.zoo.entities.len();
    let last = pieces[5];
    put_wall(&mut world, last.0, last.1);
    assert_eq!(world.zoo.entities.len(), count + 1, "nothing is built");
    let width = SIZE as usize;
    let inside = [(2, 2), (3, 2)].map(|(x, y)| y * width + x);
    for t in inside {
        for d in [0, 2, 4, 6] {
            let across = world.neighbour(t, d).unwrap();
            if inside.contains(&across) {
                continue;
            }
            assert!(world.edge(t, d).is_some(), "tile {t} side {d}");
            assert!(world.edge(across, (d + 4) % 8).is_none());
        }
    }
    assert_eq!(world.tank_at(inside[0]).map(|_| ()), Some(()));
    assert!(!world.undo().is_empty());
    let on = world.edge(2 * width + 1, 2);
    assert!(on.is_some(), "the wall on the tile outside is back");
    assert!(world.edge(2 * width + 2, 6).is_none());
}

#[test]
fn a_wall_shared_with_another_tank_is_copied_so_that_each_has_its_own() {
    // The tank (1, 2) is there; the tile (2, 2) beside it has two walls and its west line is
    // closed by the first tank's wall.
    let mut pieces = ring("tank", 1, 2, 1, 2);
    pieces.extend([("tank", (2, 2, 0)), ("tank", (2, 2, 2))]);
    let mut world = tanks(&pieces, &[], &[]);
    assert_eq!(world.exhibits().list().len(), 1);
    let count = world.zoo.entities.len();
    world.begin();
    world.take_events();
    put_wall(&mut world, "tank", (2, 2, 4));
    let width = SIZE as usize;
    assert_eq!(world.exhibits().list().len(), 2);
    assert_eq!(
        world.zoo.entities.len(),
        count + 2,
        "the piece and the copy"
    );
    let (mine, theirs) = (world.edge(2 * width + 2, 6), world.edge(2 * width + 1, 2));
    assert!(mine.is_some() && theirs.is_some() && mine != theirs);
    let copy = &world.zoo.entities[mine.unwrap()];
    assert_eq!((copy.class.as_str(), copy.subtype.as_str()), ("tank", "f"));
    // Undone, the copy is gone.
    assert!(!world.undo().is_empty());
    assert_eq!(world.zoo.entities.len(), count);
}

#[test]
fn what_a_tank_does_not_allow_is_found_and_the_player_is_asked_before_the_name() {
    // A bench, a dog and a fish in the tank, a toy and a guest too: the bench and the dog go,
    // the guest is told too (it stays), the fish and the toy are at home.
    let mut pieces = ring("tank", 2, 2, 3, 3);
    let last = pieces.pop().unwrap();
    let things = [
        ("bench", 2 * 64 + 16, 2 * 64 + 16),
        ("dog", 3 * 64 + 20, 2 * 64 + 20),
        ("fish", 3 * 64 + 30, 3 * 64 + 30),
        ("toy", 2 * 64 + 40, 3 * 64 + 40),
        ("guests", 2 * 64 + 5, 3 * 64 + 5),
        ("bench", 6 * 64 + 5, 6 * 64 + 5),
    ];
    let mut world = tanks(&pieces, &things, &[]);
    world.begin();
    world.take_events();
    put_wall(&mut world, last.0, last.1);
    let events = world.take_events();
    assert!(events.contains(&Event::Objects(1)), "{events:?}");
    assert!(!events.iter().any(|e| matches!(e, Event::New { .. })));
    let class = |id: u32| world.zoo.entities[world.find(id).unwrap()].class.clone();
    let mut refused: Vec<String> = world.refused(1).into_iter().map(class).collect();
    refused.sort();
    assert_eq!(refused, ["bench", "dog", "guests"]);
    // After Yes the objects have been dealt with: the name comes.
    world.accept_tank(1);
    assert_eq!(world.take_events(), [Event::New { id: 1, ask: true }]);
}

#[test]
fn a_show_tank_refuses_what_is_not_for_shows() {
    let mut pieces = ring("show", 2, 2, 3, 3);
    let last = pieces.pop().unwrap();
    let things = [
        ("toy", 2 * 64 + 40, 3 * 64 + 40),
        ("ball", 3 * 64 + 40, 3 * 64 + 40),
    ];
    let mut world = tanks(&pieces, &things, &[]);
    put_wall(&mut world, last.0, last.1);
    let class = |id: u32| world.zoo.entities[world.find(id).unwrap()].class.clone();
    let refused: Vec<String> = world.refused(1).into_iter().map(class).collect();
    assert_eq!(
        refused,
        ["toy"],
        "the ball is for shows, the walls are show walls"
    );
}

#[test]
fn a_tank_that_is_declined_is_undone_and_gives_its_number_back() {
    let mut pieces = ring("tank", 2, 2, 3, 3);
    let last = pieces.pop().unwrap();
    let mut world = tanks(&pieces, &[("bench", 2 * 64 + 16, 2 * 64 + 16)], &[]);
    let count = world.zoo.entities.len();
    world.begin();
    put_wall(&mut world, last.0, last.1);
    assert_eq!(world.exhibits().counter, 2);
    let done = world.decline_tank(1);
    let wall = done.steps.iter().find(|s| matches!(s, Step::Removed(..)));
    assert!(matches!(wall, Some(Step::Removed(_, price)) if *price == 0.0));
    assert!(world.exhibits().list().is_empty());
    assert_eq!(world.exhibits().counter, 1);
    assert_eq!(world.zoo.entities.len(), count);
    assert_eq!(level(&world, 2, 2), (0, 0));
    assert!(world.waiting().is_empty());
    assert!(
        world.zoo.tanks.is_empty(),
        "the picture's tank list follows"
    );
}

#[test]
fn units_that_have_to_leave_are_put_outside_the_tank() {
    let mut pieces = ring("tank", 2, 2, 3, 3);
    let last = pieces.pop().unwrap();
    let mut world = tanks(&pieces, &[("dog", 3 * 64 + 20, 2 * 64 + 20)], &[]);
    put_wall(&mut world, last.0, last.1);
    let outside = world.outside(1).unwrap();
    assert_eq!(world.tank_at(outside), None);
    let dog = world.refused(1)[0];
    world.move_unit(dog, outside);
    assert!(world.refused(1).is_empty());
}

#[test]
fn a_land_exhibit_closed_by_tank_walls_becomes_a_tank_and_back() {
    // A square of plain fences with three tank walls; the fourth is swapped for a tank wall.
    let mut pieces = ring("fences", 2, 2, 3, 3);
    for p in pieces.iter_mut().skip(1) {
        p.0 = "tank";
    }
    let mut world = tanks(&pieces, &[], &[]);
    assert_eq!(names(&world), [(1, String::new(), 4)]);
    assert!(tank(&world, 2, 2).is_none());
    world.begin();
    world.take_events();
    let first = at(&world, 2, 2, 0);
    let e = wall("tank", (2, 2, 0));
    world.swap(first, e.clone(), tank_shape(&e)).unwrap();
    for id in world.waiting() {
        world.give_gate(id);
    }
    // The land exhibit is gone, a tank takes its tiles with a new number and its name.
    assert_eq!(names(&world), [(2, "Tank 1".to_string(), 4)]);
    assert!(tank(&world, 2, 2).is_some());
    let events = world.take_events();
    assert!(events.contains(&Event::Removed(1)), "{events:?}");
    assert_eq!(level(&world, 2, 2).1, 0);
    // The other way: a plain fence in place of a wall makes it land again.
    let id = at(&world, 3, 3, 4);
    let e = wall("fences", (3, 3, 4));
    world.swap(id, e.clone(), tank_shape(&e)).unwrap();
    for id in world.waiting() {
        world.give_gate(id);
    }
    assert_eq!(names(&world), [(3, "Exhibit 2".to_string(), 4)]);
    assert!(tank(&world, 2, 2).is_none());
    assert!(world.zoo.tanks.is_empty());
}

#[test]
fn a_tank_that_joins_a_land_exhibit_goes_into_it() {
    // A tank (2, 2) and a land exhibit (3, 2) with one piece between them, on the tank's slot.
    let mut pieces = ring("tank", 2, 2, 2, 2);
    pieces.extend(
        ring("fences", 3, 2, 3, 2)
            .into_iter()
            .filter(|&(_, p)| p.2 != 6),
    );
    let mut world = tanks(&pieces, &[], &[]);
    assert_eq!(world.exhibits().list().len(), 2);
    let line = at(&world, 2, 2, 2);
    world.begin();
    assert!(world.remove(line));
    assert_eq!(world.exhibits().list().len(), 1);
    assert!(
        world.tank_at(2 * SIZE as usize + 2).is_none()
            && world.tank_at(2 * SIZE as usize + 3).is_none(),
        "the land exhibit is what is left: a tank is closed by tank walls only"
    );
}

#[test]
fn a_tank_cut_in_two_gives_the_new_part_its_water_and_a_wall_of_its_own() {
    // A tank (2..5, 2) with the water of the tank config changed after its making: the line
    // between tiles 3 and 4 is closed by a wall that stands on the tile 3 only.
    let mut pieces = ring("tank", 2, 2, 5, 2);
    let last = pieces.pop().unwrap();
    let mut world = tanks(&pieces[..], &[], &[]);
    put_wall(&mut world, last.0, last.1);
    assert_eq!(sizes(&world), [4]);
    let id = world.exhibits().list()[0].id;
    let level_before = level(&world, 2, 2);
    world.begin();
    world.take_events();
    {
        let e = world.exhibits().get(id).unwrap().clone();
        assert!(e.tank.is_some());
    }
    put_wall(&mut world, "tank", (3, 2, 2));
    assert_eq!(sizes(&world), [2, 2]);
    let events = world.take_events();
    assert!(events.contains(&Event::Split(id)), "{events:?}");
    assert!(
        events.contains(&Event::New { id: 2, ask: true }),
        "{events:?}"
    );
    let names = names(&world);
    assert_eq!(names[1].1, "Tank 2");
    // Each side has a wall of its own on the line: the piece and a copy.
    let width = SIZE as usize;
    assert!(world.edge(2 * width + 3, 2).is_some());
    assert!(world.edge(2 * width + 4, 6).is_some());
    // The new part kept its level (no sink) and the two tanks are the same tank in every other way.
    assert_eq!(level(&world, 5, 2), level_before);
    let (a, b) = (tank(&world, 2, 2).unwrap(), tank(&world, 5, 2).unwrap());
    assert_eq!(
        (a.height, a.level, a.salt, a.purity),
        (b.height, b.level, b.salt, b.purity)
    );
    assert_eq!(a.filled, b.filled);
}

// ---- tanks.cfg

#[test]
fn the_water_offset_and_the_highest_wall_are_read_and_cut() {
    let c = |text: &str| read(&format!("[tank]\n{text}"));
    assert_eq!(
        (c("").offset, c("").limit),
        (0, 30),
        "missing: the defaults"
    );
    assert_eq!(
        (
            c("waterOffset = -1\nmaximumTankHeight = 20").offset,
            c("maximumTankHeight = 20").limit
        ),
        (-1, 20)
    );
    assert_eq!(c("waterOffset = 0").offset, 0);
    assert_eq!(c("waterOffset = -99999").offset, -255);
    assert!((-255..=255).contains(&c("waterOffset = 99999999999").offset));
    assert_eq!(c("waterOffset = abc").offset, 0);
    assert_eq!(
        (
            c("maximumTankHeight = 0").limit,
            c("maximumTankHeight = -4").limit
        ),
        (0, 0)
    );
    assert_eq!(c("maximumTankHeight = 99999").limit, 255);
    // The full level: the height plus the offset, never below 0.
    let cfg = |offset| TankConfig {
        offset,
        ..TankConfig::default()
    };
    assert_eq!(
        (
            cfg(-1).top(5),
            cfg(0).top(5),
            cfg(-9).top(5),
            cfg(255).top(255)
        ),
        (4, 5, 0, 255)
    );
}

fn read(text: &str) -> TankConfig {
    TankConfig::read(&crate::config::Config::parse(text.as_bytes()).unwrap())
}

#[test]
fn the_numbers_of_tanks_cfg_are_read_and_cut_to_what_a_tank_can_hold() {
    let shipped = "[cost]\nsaltWater = 1.5\n[tank]\ninitialSink = 4\ninitialHeight = 5\ninitialSinkShow = 8\n\
        initialHeightShow = 9\ninitialWaterLevel = 0\ninitialFillState = 1\ninitialSalinity = 100\n\
        initialWaterPurity = 100\n\
        tankTerrain = 2\ntankTerrainShow = 17\n";
    assert_eq!(
        read(shipped),
        TankConfig {
            sink: [4, 8],
            height: [5, 9],
            level: 0,
            filled: true,
            salt: true,
            purity: 100,
            terrain: [2, 17],
            offset: 0,
            limit: 30,
            murky: 60,
        }
    );
    // The limit of the clean water: any number, as the original takes it.
    for (text, murky) in [("60", 60), ("0", 0), ("-5", -5), ("99999", 99999), ("", 60)] {
        let cfg = format!("[tank]\nmurkyWaterPurity = {text}\n");
        assert_eq!(read(&cfg).murky, murky, "{text:?}");
    }
    // Missing: what there is without a file, heights of 1 included; an empty file too.
    let none = TankConfig::default();
    assert_eq!(
        (none.height, none.sink, none.filled),
        ([1, 1], [0, 0], false)
    );
    assert_eq!(read(""), none);
    assert_eq!(read("[tank]\n"), none);
    // Zero, negative, huge, not a number, hex: cut or ignored, never a panic.
    let odd = read(
        "[tank]\ninitialHeight = 0\ninitialHeightShow = -3\ninitialSink = -99\ninitialSinkShow = 99999999999\n\
        initialWaterLevel = abc\ninitialFillState = -1\ninitialSalinity = 0\ninitialWaterPurity = 0x7fffffff\n\
        tankTerrain = -5\ntankTerrainShow = 999\n",
    );
    assert_eq!(odd.terrain, [0, 255], "cut to a byte");
    assert_eq!(read("[tank]\ntankTerrain = 5\n").terrain, [5, 0]);
    assert_eq!(none.terrain, [0, 0], "no default: grass");
    assert_eq!(odd.height, [0, 0]);
    assert_eq!(odd.sink[0], -24);
    assert!(
        (-24..=24).contains(&odd.sink[1]),
        "a number that wraps is still cut"
    );
    assert_eq!(
        (odd.level, odd.filled, odd.salt, odd.purity),
        (0, false, false, 100)
    );
}

#[test]
fn the_exhibit_whose_gate_moves_into_a_new_tank_gets_another_gate() {
    // A land exhibit on tile (4, 2) with its gate on the line to tile (3, 2), a tank wall. Walls
    // are put round (3, 2) until it is a tank: the gate moves to the tank's side, so the
    // exhibit needs a new one, on one of its other sides.
    let mut z = zoo(&[]);
    let mut piece = |class: &str, subtype: &str, (x, y, d)| {
        let mut e = entity(class, x, y, d);
        e.subtype = subtype.into();
        z.entities.push(e);
    };
    for d in [0, 2, 4] {
        piece("fences", "f", (4, 2, d));
    }
    piece("tankwall", "g", (4, 2, 6));
    piece("tankwall", "f", (3, 2, 0));
    piece("tankwall", "f", (3, 2, 4));
    unique(&mut z);
    let gate = z.entities.iter().position(|e| e.subtype == "g").unwrap() as u32 + 1;
    let mut world = World::new(z, tank_shape);
    world.set_tank_config(CONFIG);
    let exhibit = world.exhibits().list()[0].id;
    world.set_gate(exhibit, gate);
    world.begin();
    world.take_events();
    let e = wall("tankwall", (3, 2, 6));
    world.add(e.clone(), tank_shape(&e)).unwrap();
    // The tank has the gate piece on its own side of the line.
    let width = SIZE as usize;
    let line = world.edge(2 * width + 3, 2).unwrap();
    assert_eq!(world.zoo.entities[line].common.as_ref().unwrap().id, gate);
    assert!(world.tank_at(2 * width + 3).is_some());
    // The exhibit has another gate and the old one is an ordinary wall.
    let new = world.exhibits().get(exhibit).unwrap().gate.unwrap();
    assert_ne!(new, gate);
    // It is on the first pair of the exhibit's boundary: the north side of its only tile.
    assert_eq!(world.edge(2 * width + 4, 0), world.find(new));
    assert!(world.take_events().contains(&Event::Gate(exhibit)));
    assert_eq!(world.zoo.entities[world.find(gate).unwrap()].subtype, "f");
    assert_eq!(world.zoo.entities[world.find(new).unwrap()].subtype, "g");
}

// ---- The floor and the second wall

fn floor(world: &World, x: usize, y: usize) -> u8 {
    world.zoo.tiles[y * SIZE as usize + x].terrain
}

const TILES: [(usize, usize); 4] = [(2, 2), (3, 2), (2, 3), (3, 3)];

#[test]
fn tank_floor_is_sand_or_gunite_by_the_show_state_and_sand_again_when_the_tank_goes() {
    // A tank closed by the player: sand under it whatever the ground was, and the ground beside
    // it stays. The paint is in the undo list.
    let mut pieces = ring("tank", 2, 2, 3, 3);
    let last = pieces.pop().unwrap();
    let mut world = tanks(&pieces, &[], &[]);
    world.zoo.tiles[3 * SIZE as usize + 3].terrain = 5;
    world.zoo.tiles[4 * SIZE as usize + 4].terrain = 5;
    world.begin();
    put_wall(&mut world, last.0, last.1);
    assert!(TILES.iter().all(|&(x, y)| floor(&world, x, y) == 2));
    assert_eq!(floor(&world, 4, 4), 5);
    assert_eq!(
        level(&world, 2, 2),
        (-4, 0),
        "the ground is levelled all the same"
    );
    assert!(!world.undo().is_empty());
    assert_eq!(floor(&world, 3, 3), 5, "the old type is back");
    assert_eq!(floor(&world, 2, 2), 0);
    // A show tank gets the gunite, the type of tanks.cfg.
    let mut pieces = ring("show", 2, 2, 3, 3);
    let last = pieces.pop().unwrap();
    let mut world = tanks(&pieces, &[], &[]);
    put_wall(&mut world, last.0, last.1);
    assert!(TILES.iter().all(|&(x, y)| floor(&world, x, y) == 17));
    // Opened, it is no tank: sand, also where it was gunite, and the ground stays where it is.
    let at = world.edge(2 * SIZE as usize + 2, 0).unwrap();
    let id = world.zoo.entities[at].common.as_ref().unwrap().id;
    world.begin();
    world.remove(id);
    assert!(world.exhibits().list().is_empty());
    assert!(TILES.iter().all(|&(x, y)| floor(&world, x, y) == 2));
    assert_eq!(level(&world, 2, 2), (-8, 0));
    assert!(
        !world.undo().is_empty(),
        "the gunite is back with the walls"
    );
    assert!(TILES.iter().all(|&(x, y)| floor(&world, x, y) == 17));
    // A saved zoo keeps the ground it has.
    let world = built(&ring("show", 2, 2, 3, 3));
    assert!(TILES.iter().all(|&(x, y)| floor(&world, x, y) == 0));
}

#[test]
fn tank_floor_follows_a_wall_change_that_makes_a_tank_a_show_tank() {
    // A loaded tank keeps its ground. The walls are swapped for show walls one by one: the last
    // one makes it a show tank and paints it.
    let mut world = tanks(&ring("tank", 2, 2, 3, 3), &[], &[]);
    assert!(TILES.iter().all(|&(x, y)| floor(&world, x, y) == 0));
    let ids: Vec<u32> = ring("tank", 2, 2, 3, 3)
        .iter()
        .map(|&(_, (x, y, d))| {
            let t = y as usize * SIZE as usize + x as usize;
            let e = world.edge(t, d as usize).unwrap();
            world.zoo.entities[e].common.as_ref().unwrap().id
        })
        .collect();
    for (n, id) in ids.iter().enumerate() {
        let mut new = wall("show", (0, 0, 0));
        let at = world.find(*id).unwrap();
        new.common = world.zoo.entities[at].common.clone();
        let shape = shape(&new);
        world.swap(*id, new, shape).unwrap();
        let want = if n + 1 == ids.len() { 17 } else { 0 };
        assert!(
            TILES.iter().all(|&(x, y)| floor(&world, x, y) == want),
            "swap {n}"
        );
    }
    assert_eq!(tank(&world, 2, 2).map(|t| t.show), Some(true));
}

#[test]
fn wall_copy_new_is_a_healthy_plain_piece_of_the_type_with_nothing_of_the_wall_kept() {
    // The tank (1, 2) has a worn wall, a gate with a name and colours, on the line the second
    // tank (2, 2) leans on.
    let mut pieces = ring("tank", 1, 2, 1, 2);
    pieces.extend([("tank", (2, 2, 0)), ("tank", (2, 2, 2))]);
    let mut world = tanks(&pieces, &[], &[]);
    let old = world.edge(2 * SIZE as usize + 1, 2).unwrap();
    {
        let e = &mut world.zoo.entities[old];
        e.subtype = "g".into();
        let c = e.common.as_mut().unwrap();
        c.name = "worn".into();
        c.colors = vec![3, 1];
        c.fence = Some(crate::zoo::Fence {
            strength: 9,
            life: 0,
            timers: [5, 6],
            portal: true,
        });
    }
    world.begin();
    put_wall(&mut world, "tank", (2, 2, 4));
    let copy = world.edge(2 * SIZE as usize + 2, 6).unwrap();
    assert_ne!(copy, old);
    let (e, c) = (
        &world.zoo.entities[copy],
        world.zoo.entities[copy].common.as_ref().unwrap(),
    );
    assert_eq!(
        (e.class.as_str(), e.type_name.as_str(), e.subtype.as_str()),
        ("tank", "", "f")
    );
    assert_eq!(
        (c.name.as_str(), c.colors.len(), c.movement_mode),
        ("", 0, 0)
    );
    assert_eq!(
        c.fence,
        Some(crate::zoo::Fence {
            strength: 500,
            life: 10,
            timers: [0; 2],
            portal: false
        })
    );
    assert_eq!(e.body_size, None);
    // The wall it copies is as it was.
    let worn = world.zoo.entities[old].common.as_ref().unwrap();
    assert_eq!((worn.name.as_str(), worn.fence.unwrap().life), ("worn", 0));
}

#[test]
fn gate_tank_refuses_a_tile_with_an_object_and_the_marine_specialist_swims_to_it() {
    let t = |x: usize, y: usize| y * SIZE as usize + x;
    let gate_at = |w: &World, x: usize, y: usize, d: usize| w.gate_for(w.edge(t(x, y), d).unwrap());
    // A tank (2..3, 2..3) with a bench in a quadrant of tile (2, 2) and water under tile (3, 2).
    let things = [("bench", 2 * 64 + 16, 2 * 64 + 16)];
    let mut world = tanks(&ring("tank", 2, 2, 3, 3), &things, &[]);
    world.set_water(vec![false, true]);
    world.zoo.tiles[t(3, 2)].terrain = 1;
    let piece = |w: &World, x: usize, y: usize, d: usize| w.edge(t(x, y), d).unwrap();
    // Item 4: an object in a quadrant of the tank tile; the gate is judged, and refused.
    assert_eq!(gate_at(&world, 2, 2, 0), Some(1));
    assert!(world.tank_holds_object(piece(&world, 2, 2, 0)));
    assert!(world.tank_holds_object(piece(&world, 2, 2, 6)));
    assert!(!world.tank_holds_object(piece(&world, 3, 2, 0)));
    assert!(!world.keeper_passes(piece(&world, 2, 2, 0)), "code 3");
    // Water on the tank tile does not stop the marine specialist, a land keeper would not cross.
    assert!(world.keeper_passes(piece(&world, 3, 2, 0)));
    // The same piece of a land exhibit: water stops the zookeeper, and an object on the tile
    // is no reason to refuse.
    let mut world = tanks(&ring("fences", 2, 2, 3, 3), &things, &[]);
    world.set_water(vec![false, true]);
    world.zoo.tiles[t(3, 2)].terrain = 1;
    assert!(!world.tank_holds_object(piece(&world, 2, 2, 0)));
    assert!(!world.keeper_passes(piece(&world, 3, 2, 0)));
}

#[test]
fn a_new_tank_is_undone_as_one_terrain_step_without_a_price_and_its_wall_moves_back() {
    // The ring of a tank is closed by the last wall; the levelling and the floor are one terrain
    // action, and the undo hands back no price for them (placing 8, kind 3).
    let mut pieces = ring("tank", 2, 2, 3, 3);
    let last = pieces.pop().unwrap();
    let mut world = tanks(&pieces, &[], &[]);
    world.begin();
    let id = put_wall(&mut world, last.0, last.1);
    world.priced(100.0);
    let done = world.undo();
    let kinds: Vec<_> = done
        .steps
        .iter()
        .map(|s| match s {
            Step::Removed(spot, price) => (spot.id == id, *price as i32),
            Step::Tiles(n, price) => (*n == 4, *price as i32 + 1),
            other => panic!("{other:?}"),
        })
        .collect();
    assert_eq!(kinds, [(true, 100), (true, 1)]);
    assert!(world.exhibits().list().is_empty());
    assert_eq!(floor(&world, 2, 2), 0);
    assert_eq!(level(&world, 3, 3), (0, 0));
}

#[test]
fn a_land_exhibit_that_becomes_a_tank_gets_its_name_and_gate_back_on_undo() {
    // The morph records the name and gate of the exhibit it replaces (placing 8, kind 11).
    let mut pieces = ring("fences", 2, 2, 3, 3);
    for p in pieces.iter_mut().skip(1) {
        p.0 = "tank";
    }
    let mut world = tanks(&pieces, &[], &[]);
    let gate = at(&world, 3, 3, 4);
    world.rename(1, "Lions".into());
    world.set_gate(1, gate);
    world.begin();
    let first = at(&world, 2, 2, 0);
    let e = wall("tank", (2, 2, 0));
    world.swap(first, e.clone(), tank_shape(&e)).unwrap();
    for id in world.waiting() {
        world.give_gate(id);
    }
    assert!(tank(&world, 2, 2).is_some());
    assert!(!world.undo().is_empty());
    let region = region(&world, 2, 2);
    let back = world.exhibits().get(region).unwrap();
    assert!(back.tank.is_none());
    assert_eq!((back.name.as_str(), back.gate), ("Lions", Some(gate)));
    assert!(world.zoo.tanks.is_empty());
}

#[test]
fn removing_a_wall_of_a_tank_records_its_values_and_the_undo_gives_them_back() {
    // A loaded tank of height 9, purity 80, fresh and drained: the wall put back by the undo makes
    // a tank again with the values of a new one, then the recorded ones are set (placing 8).
    let mut z = zoo(&[]);
    z.entities.extend(
        ring("tank", 2, 2, 3, 3)
            .into_iter()
            .map(|(c, p)| wall(c, p)),
    );
    unique(&mut z);
    z.tanks = vec![crate::zoo::Tank {
        tile: (2, 2),
        gate: (3, 3),
        height: 9,
        level: 0,
        salt: false,
        purity: 80,
    }];
    let mut world = World::new(z, tank_shape);
    world.set_tank_config(CONFIG);
    let before = tank(&world, 2, 2).unwrap();
    let side = at(&world, 2, 2, 0);
    world.begin();
    world.remove(side);
    assert!(tank(&world, 2, 2).is_none());
    assert!(!world.undo().is_empty());
    let after = tank(&world, 2, 2).unwrap();
    assert_eq!(
        (after.height, after.purity, after.salt, after.filled),
        (before.height, before.purity, before.salt, before.filled)
    );
    assert_eq!(after.height, 9, "not the height of a new tank");
}

#[test]
fn a_wall_that_is_not_the_wall_of_a_tank_records_no_tank_value() {
    // A plain fence of a land exhibit: the undo of its removal sets no tank value.
    let mut world = tanks(&ring("fences", 2, 2, 3, 3), &[], &[]);
    let side = at(&world, 2, 2, 0);
    world.begin();
    world.remove(side);
    let done = world.undo();
    assert!(
        done.steps
            .iter()
            .all(|s| matches!(s, Step::Back(..) | Step::Tiles(..)))
    );
}

#[test]
fn a_wall_copied_by_the_wall_step_is_undone_as_a_placement_of_its_type() {
    let pieces = [("tank", (2, 2, 0)), ("tank", (2, 2, 2))];
    let mut pieces = ring("tank", 1, 2, 1, 2)
        .into_iter()
        .chain(pieces)
        .collect::<Vec<_>>();
    pieces.shrink_to_fit();
    let mut world = tanks(&pieces, &[], &[]);
    world.begin();
    put_wall(&mut world, "tank", (2, 2, 4));
    let done = world.undo();
    let copies = done
        .steps
        .iter()
        .filter(|s| matches!(s, Step::Copied(spot, _) if spot.name == "tank//f"))
        .count();
    assert_eq!(copies, 1, "{:?}", done.steps);
}

#[test]
fn a_morph_without_a_gate_starts_at_the_changed_line() {
    // A land exhibit (2,2)-(3,3) with no gate, closed by tank walls but for the piece on tile
    // (3,3) towards the east. A tank wall in its place makes it a tank, filled from (3,3) and
    // with its gate walk started at the pair ((3,3), (4,3)), not at the first pair of the old
    // boundary list on the top row.
    let mut pieces = ring("tank", 2, 2, 3, 3);
    pieces.retain(|&(_, p)| p != (3, 3, 2));
    pieces.push(("fences", (3, 3, 2)));
    let mut world = tanks(&pieces, &[], &[]);
    assert!(world.exhibits().list()[0].gate.is_none());
    world.begin();
    let id = at(&world, 3, 3, 2);
    let e = wall("tank", (3, 3, 2));
    world.swap(id, e.clone(), tank_shape(&e)).unwrap();
    for id in world.waiting() {
        world.give_gate(id);
    }
    let t = |x: usize, y: usize| y * SIZE as usize + x;
    let new = world.exhibits().list()[0].clone();
    assert!(new.tank.is_some());
    assert_eq!(new.order, [t(2, 2), t(3, 2), t(2, 3), t(3, 3)]);
    assert_eq!(world.gate_slot(new.id), Some((t(3, 3), 2)));
}
