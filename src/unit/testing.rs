//! Synthetic maps for the tests of the move and of the random walk.

use crate::render::order::Kind as Class;
use crate::world::{Shape, TankKind, World};
use crate::zoo::{Common, Entity, Fence, Tile, View, Zoo};

/// An entity standing at the point `(x, y)`.
pub fn entity(class: &str, kind: &str, id: u32, (x, y): (i32, i32), rotation: u32) -> Entity {
    let fence = (class == "fences").then_some(Fence {
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
            movement_mode: 0,
        }),
    }
}

/// The centre of tile `(tx, ty)`.
pub fn centre((tx, ty): (i32, i32)) -> (i32, i32) {
    (tx * 64 + 32, ty * 64 + 32)
}

/// Units take their place by their position, `paths` are path pieces, `fences` exhibit fences,
/// `bush` a one-quarter object.
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
        "bush" => Some(thing(Class::Fixed, (1, 1), false)),
        "animals" | "guests" | "keeper" | "maint" | "tour" => {
            Some(thing(Class::Unit, (1, 1), false))
        }
        _ => None,
    }
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
    };
    edit(&mut zoo);
    World::new(zoo, shape)
}
