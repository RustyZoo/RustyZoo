//! Turning to rubble (cheats 4.6): an entity is taken off the map and leaves small rubble pieces,
//! cell by cell. No cash is paid, nothing is asked or said.

use crate::build::number;
use crate::building::lift;
use crate::entity::{Catalogue, EntityType};
use crate::place::bulldozer::type_of;
use crate::render::order::ground_height;
use crate::sim::Rng;
use crate::unit::gate::heard_of;
use crate::world::{Shape, World};
use crate::zoo::{Common, Entity, Rubble};

/// The name id of the restroom, which leaves its own rubble.
const RESTROOM: i32 = 8008;
/// The string that names a piece, with `%s` for the name of the entity.
pub const PIECE_NAME: i32 = 10512;
/// The rubble types a building of at least 2 x 2 quadrants leaves, in the order of the pick.
const BIG: [&str; 4] = ["brubble1", "brubble2", "brubble3", "brubble4"];

/// What the routine needs besides the world.
pub struct Kit<'a> {
    pub cat: &'a Catalogue,
    pub rng: &'a mut Rng,
    /// The text of [`PIECE_NAME`].
    pub template: &'a str,
    /// How an entity takes its place on the map.
    pub shape: &'a dyn Fn(&Entity) -> Option<Shape>,
}

/// The first rubble type of that subtype.
fn rubble<'a>(cat: &'a Catalogue, subtype: &str) -> Option<&'a EntityType> {
    cat.types
        .iter()
        .find(|t| t.class == "rubble" && t.type_name == b"other" && t.subtype == subtype.as_bytes())
}

/// Takes entity `id` off the map and leaves its rubble (4.6): a building lets its users go first
/// (guest buildings 11.1). Whether there was such an entity.
pub fn turn_to_rubble(world: &mut World, kit: &mut Kit, id: u32) -> bool {
    let Some(at) = world.find(id) else {
        return false;
    };
    let entity = world.zoo.entities[at].clone();
    let Some(c) = entity.common.clone() else {
        return false;
    };
    world.forget_undo(id);
    let ty = type_of(kit.cat, &entity);
    if entity.class == "building" {
        lift(world, kit.cat, id, kit.rng);
    } else {
        world.unrecorded(|w| w.remove(id));
    }
    let Some(ty) = ty else {
        return true;
    };
    let flag = |key| number(ty, key) == Some(1.0);
    // A building leaves its own pieces, one to a tile unless it is one quadrant wide.
    let building = entity.class == "building" && !flag("cForcesSceneryRubble");
    let (fx, fy) = (ty.footprint[0], ty.footprint[1]);
    let by_tile = building && (ty.name_id == RESTROOM || (fx >= 2 && fy >= 2));
    let names: Vec<&str> = match (building, ty.name_id == RESTROOM, by_tile) {
        (false, ..) if flag("cUsesTreeRubble") => vec!["bstump1"],
        (true, true, _) => vec!["bathrubl"],
        (true, _, true) => BIG.to_vec(),
        _ => vec!["brubls1"],
    };
    let kinds: Vec<&EntityType> = names.iter().filter_map(|s| rubble(kit.cat, s)).collect();
    if kinds.is_empty() {
        return true;
    }
    let step = if by_tile { 2 } else { 1 };
    let mut made = Vec::new();
    for i in (0..fx).step_by(step) {
        for j in (0..fy).step_by(step) {
            let (cx, cy) = (c.x - fx * 16 + 16 + 32 * i, c.y - fy * 16 + 16 + 32 * j);
            let tile = world.tile_index(cx.div_euclid(64), cy.div_euclid(64));
            let kind = if by_tile {
                // A draw for every cell whose tile exists.
                let Some(_) = tile else { continue };
                kinds[kit.rng.pick(kinds.len() as u32) as usize]
            } else {
                // Scenery skips one cell in a hundred; a building's quadrant draws its pick.
                if kit.rng.pick(if building { 1 } else { 100 }) == 0 && !building {
                    continue;
                }
                kinds[0]
            };
            let Some(tile) = tile else { continue };
            // The piece stands on the ground at its point.
            let (x, y) = match by_tile {
                true => (cx.div_euclid(64) * 64 + 32, cy.div_euclid(64) * 64 + 32),
                false => (cx, cy),
            };
            let z = ground_height(&world.zoo.tiles[tile], x.rem_euclid(64), y.rem_euclid(64));
            let name = kit.template.replacen("%s", &c.name, 1);
            let text = |bytes: &[u8]| bytes.iter().map(|&b| char::from(b)).collect::<String>();
            let piece = Entity {
                class: "rubble".into(),
                type_name: text(&kind.type_name),
                subtype: text(&kind.subtype),
                body_size: None,
                common: Some(Common {
                    x,
                    y,
                    z,
                    rotation: kit.rng.rand() & 7,
                    name,
                    ..Default::default()
                }),
            };
            let shape = (kit.shape)(&piece);
            let id = world.unrecorded(|w| w.add(piece, shape));
            made.extend(id);
            // The first piece of each column explodes with the sound of its own type.
            if let (0, Some(id)) = (j, id) {
                let voice = format!("{id}/cExplosionSound");
                let heard = heard_of(kind, "cExplosionSound", "cExplosionSoundAtten", &voice);
                world.units.push_sound(id, heard);
            }
        }
    }
    // The pieces of one call share a group and keep the name id of the type they replace.
    for &piece in &made {
        let at = world.find(piece);
        if let Some(common) = at.and_then(|at| world.zoo.entities[at].common.as_mut()) {
            common.rubble = Some(Rubble {
                group: made.clone(),
                name_id: ty.name_id,
            });
        }
    }
    true
}

#[cfg(test)]
mod tests;
