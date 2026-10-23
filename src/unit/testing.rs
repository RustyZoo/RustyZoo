//! Synthetic maps for the tests of the move and of the random walk.

use crate::render::order::Kind as Class;
use crate::world::{Shape, TankKind, World};
use crate::zoo::{Common, Entity, Fence, Tile, View, Zoo};

/// An entity standing at the point `(x, y)`.
pub fn entity(class: &str, kind: &str, id: u32, (x, y): (i32, i32), rotation: u32) -> Entity {
    let fence = matches!(class, "fences" | "tankwall").then_some(Fence {
        strength: 5,
        life: 5,
        timers: [0; 2],
        portal: false,
    });
    Entity {
        class: class.into(),
        type_name: kind.into(),
        subtype: String::new(),
        body_size: None,
        common: Some(Common {
            x,
            y,
            z: 0,
            rotation,
            id,
            name: String::new(),
            colors: Vec::new(),
            fence,
            ..Default::default()
        }),
    }
}

/// The centre of tile `(tx, ty)`.
pub fn centre((tx, ty): (i32, i32)) -> (i32, i32) {
    (tx * 64 + 32, ty * 64 + 32)
}

/// Units take their place by their position, `paths` are path pieces, `fences` exhibit fences,
/// `bush` and `building` a one-quarter object (a `bush` of type `grass` is walkable, a `building`
/// of type `hall` takes a whole tile).
fn shape(e: &Entity) -> Option<Shape> {
    let thing = |kind, footprint, fence| Shape {
        kind,
        footprint,
        fence,
        wall: false,
        gate: false,
        gated: fence,
        tank: TankKind::NONE,
        walkable: (false, false),
    };
    match e.class.as_str() {
        "fences" => Some(thing(Class::Edge, (1, 1), true)),
        "paths" => Some(thing(Class::Path, (2, 2), false)),
        "bush" if e.type_name == "grass" => Some(Shape {
            walkable: (true, false),
            ..thing(Class::Fixed, (1, 1), false)
        }),
        "bush" => Some(thing(Class::Fixed, (1, 1), false)),
        "building" if e.type_name == "hall" => Some(thing(Class::Fixed, (2, 2), false)),
        "building" => Some(thing(Class::Fixed, (1, 1), false)),
        "tankwall" => Some(Shape {
            tank: TankKind {
                wall: true,
                ..TankKind::NONE
            },
            ..thing(Class::Wall, (1, 1), true)
        }),
        "animals" if e.type_name == "big" => Some(thing(Class::Unit, (3, 2), false)),
        "animals" | "guests" | "keeper" | "maint" | "tour" => {
            Some(thing(Class::Unit, (1, 1), false))
        }
        _ => None,
    }
}

/// The shape of a keeper or any other unit.
pub fn keeper_shape() -> Shape {
    shape(&entity("keeper", "", 0, (0, 0), 0)).expect("a unit has a shape")
}

/// A flat `w` by `h` map, all of it inside the zoo, with the entities `edit` makes.
pub fn world(size: (i32, i32), edit: impl FnOnce(&mut Vec<Tile>, &mut Vec<Entity>)) -> World {
    world_with(size, |zoo| edit(&mut zoo.tiles, &mut zoo.entities))
}

/// The same, with the whole zoo to edit.
pub fn world_with((w, h): (i32, i32), edit: impl FnOnce(&mut Zoo)) -> World {
    let mut zoo = Zoo {
        version: 0,
        locale: 0,
        expansion: 0,
        width: w as u32,
        height: h as u32,
        entrance: (0, 0),
        habitat_tiles: Vec::new(),
        habitat_gates: Vec::new(),
        habitat_info: Vec::new(),
        tanks: Vec::new(),
        id_counter: 1_000_000,
        clocks: Default::default(),
        tiles: vec![
            Tile {
                elevation: 0,
                shape: 0,
                terrain: 0,
                edges: 0
            };
            (w * h) as usize
        ],
        entities: Vec::new(),
        view: View::default(),
        names: Default::default(),
    };
    edit(&mut zoo);
    World::new(zoo, shape)
}

/// A type made of a walk: the set `bWalk` of one move call, the set `bZoo` of a call that never ends, the animations `idle`, `walk`, `trot`,
/// `run`, `swim` and the tank animations, the rates 20, 46 and 76, and the animation speeds 50 (slow) and 80 (slow, in water).
pub struct Walker(Vec<crate::behaviour::Call>, Vec<crate::behaviour::Call>);

impl Walker {
    pub fn new(call: &str) -> Walker {
        let parse = |c: &str| crate::behaviour::parse_call(c.as_bytes()).unwrap().0;
        Walker(vec![parse(call)], vec![parse("fKeepControl()")])
    }

    /// Starts the set `bWalk` on unit `id`, forced, as the goals of a guest or staff member do.
    pub fn start(&self, w: &mut World, id: u32) {
        self.start_set(w, id, "bWalk", true);
    }

    /// Starts the set `name` (`bWalk`, or `bZoo`, which does nothing for ever).
    pub fn start_set(&self, w: &mut World, id: u32, name: &str, force: bool) {
        let at = w.find(id).unwrap();
        let mut rng = crate::sim::Rng::default();
        let env = crate::behaviour::Env {
            data: self,
            unit: &w.zoo.entities[at],
            rng: &mut rng,
        };
        let unit = w.units.get_mut(id).unwrap();
        unit.runner
            .start(&mut unit.animation, &env, name, true, force, true);
    }
}

impl crate::behaviour::Data for Walker {
    fn set(&self, _: &Entity, name: &str) -> Option<&[crate::behaviour::Call]> {
        match name {
            "bWalk" => Some(&self.0),
            "bZoo" => Some(&self.1),
            _ => None,
        }
    }
    fn ambient(&self, _: &Entity, _: usize) -> &[crate::behaviour::Row] {
        &[]
    }
    fn timing(&self, _: &Entity, name: &str) -> Option<crate::behaviour::Timing> {
        [
            "idle", "walk", "trot", "run", "swim", "subswim", "surfswim", "dive", "rise",
        ]
        .contains(&name)
        .then_some(crate::behaviour::Timing {
            count: 1,
            delay: 100,
        })
    }
    fn number(&self, _: &Entity, number: &crate::behaviour::Number) -> i32 {
        rates(number)
    }
    fn sound(&self, _: &Entity, _: &str) -> Option<crate::behaviour::Heard> {
        None
    }
}

/// A 12 by 8 map with a tank of 4 by 3 tiles from (2, 2): the floor at level 0, the walls
/// `height` high and the water at `level`; its first tile is (2, 2). The animals are the given
/// ids at a tile and a height.
pub fn tank_world(height: u32, level: u32, animals: &[(u32, (i32, i32), i32)]) -> World {
    let wall = |(x, y): (i32, i32), d: u32| {
        let side = [(32, 0), (63, 32), (32, 63), (0, 32)][d as usize / 2];
        entity(
            "tankwall",
            "iron",
            1000 + x as u32 * 10 + y as u32 + d,
            (x * 64 + side.0, y * 64 + side.1),
            d,
        )
    };
    let mut walls = Vec::new();
    for t in 0..4 {
        walls.push(wall((2 + t, 2), 0));
        walls.push(wall((2 + t, 4), 4));
    }
    for t in 0..3 {
        walls.push(wall((2, 2 + t), 6));
        walls.push(wall((5, 2 + t), 2));
    }
    let mut w = world_with((12, 8), |z| {
        z.entities.extend(walls);
        for &(id, tile, height) in animals {
            let mut fish = entity("animals", "fish", id, centre(tile), 0);
            fish.common.as_mut().unwrap().z = height;
            z.entities.push(fish);
        }
        z.habitat_tiles = vec![(2, 2)];
        z.habitat_gates = vec![((2, 2), 0)];
        z.habitat_info = vec![crate::zoo::HabitatInfo {
            name: String::new(),
            stats: [0; 9],
            created: 0,
            serviced: 0,
        }];
    });
    let id = w.tank_at(2 * 12 + 2).unwrap();
    let tank = w.exhibits_mut().get_mut(id).unwrap().tank.as_mut().unwrap();
    (tank.height, tank.level, tank.filled) = (height, level, true);
    w
}

/// The numbers of a walking type: the rates 20, 46 and 76, the animation speeds 50 (slow) and 80
/// (slow, in water), and a swimmer.
pub fn rates(number: &crate::behaviour::Number) -> i32 {
    match number {
        crate::behaviour::Number::Int(n) => *n,
        crate::behaviour::Number::Characteristic(key) => match key.as_str() {
            "cSlowRate" => 20,
            "cMediumRate" => 46,
            "cFastRate" => 76,
            "cSlowAnimSpeed" => 50,
            "cSlowAnimSpeedWater" => 80,
            "cSwims" => 1,
            _ => 0,
        },
    }
}

/// The tank of `tank_world` (2 high, full, so its surface is at the wall tops) and a land exhibit of 4 by 3 tiles from (6, 2) beside it
/// that shares the tank's right wall, the ground raised to level `land`: the wall tops are level
/// with it at 2. The land exhibit's other three sides are exhibit fences; `animals` stand at
/// the given tiles.
pub fn amphibious_world(land: i32, animals: &[(u32, &str, (i32, i32))]) -> World {
    amphibious_world_with(land, animals, false)
}

/// [`amphibious_world`] with a second land exhibit of 2 by 3 tiles from (0, 2) on the other side
/// of the tank (its other three sides are exhibit fences, the tank's left wall is shared).
pub fn amphibious_world_between(land: i32, animals: &[(u32, &str, (i32, i32))]) -> World {
    amphibious_world_with(land, animals, true)
}

/// Gives every exhibit a gate, an id no piece has, for what needs a gate (tank fauna 12.3).
pub fn gated(mut w: World) -> World {
    w.exhibits_mut()
        .list_mut()
        .iter_mut()
        .for_each(|e| e.gate = Some(u32::MAX));
    w
}

fn amphibious_world_with(land: i32, animals: &[(u32, &str, (i32, i32))], west: bool) -> World {
    let piece = |class: &str, (x, y): (i32, i32), d: u32| {
        let side = [(32, 0), (63, 32), (32, 63), (0, 32)][d as usize / 2];
        let id = 1000 + x as u32 * 10 + y as u32 + d;
        entity(class, "iron", id, (x * 64 + side.0, y * 64 + side.1), d)
    };
    // The ids of the west land's pieces are kept apart from the others'.
    let far = |mut e: Entity| {
        e.common.as_mut().unwrap().id += 5000;
        e
    };
    let mut pieces = Vec::new();
    for t in 0..4 {
        pieces.push(piece("tankwall", (2 + t, 2), 0));
        pieces.push(piece("tankwall", (2 + t, 4), 4));
        pieces.push(piece("fences", (6 + t, 2), 0));
        pieces.push(piece("fences", (6 + t, 4), 4));
    }
    for t in 0..3 {
        pieces.push(piece("tankwall", (2, 2 + t), 6));
        pieces.push(piece("tankwall", (5, 2 + t), 2));
        pieces.push(piece("fences", (9, 2 + t), 2));
        if west {
            pieces.push(far(piece("fences", (0, 2 + t), 6)));
        }
    }
    for t in 0..2 {
        if west {
            pieces.push(far(piece("fences", (t, 2), 0)));
            pieces.push(far(piece("fences", (t, 4), 4)));
        }
    }
    let mut w = world_with((12, 8), |z| {
        z.entities.extend(pieces);
        for &(id, kind, tile) in animals {
            z.entities
                .push(entity("animals", kind, id, centre(tile), 0));
        }
        let west = if west { 0..2 } else { 0..0 };
        let lands = (0..4).map(|x| 6 + x).chain(west);
        for t in lands.flat_map(|x| (0..3).map(move |y| (x, 2 + y))) {
            z.tiles[t.1 * 12 + t.0].elevation = land;
        }
    });
    let id = w.tank_at(2 * 12 + 2).unwrap();
    w.set_height(id, 2);
    let tank = w.exhibits_mut().get_mut(id).unwrap().tank.as_mut().unwrap();
    (tank.level, tank.filled) = (2, true);
    w
}
