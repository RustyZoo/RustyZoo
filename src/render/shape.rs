//! How an entity takes its place on the map, from its type.

use super::order::Kind;
use super::{habitat_fence, member, tank_kind};
use crate::entity::Catalogue;
use crate::world::Shape;

/// How an entity takes its place on the map, if its type is known.
pub fn shape_of(catalogue: &Catalogue, entity: &crate::zoo::Entity) -> Option<Shape> {
    let bytes = |s: &str| s.chars().map(|c| c as u8).collect::<Vec<u8>>();
    let ty = catalogue.find(
        &entity.class,
        &bytes(&entity.type_name),
        &bytes(&entity.subtype),
    )?;
    Some(Shape {
        kind: Kind::of_class(&entity.class),
        footprint: (ty.footprint[0], ty.footprint[1]),
        fence: habitat_fence(ty),
        wall: member(ty, "zoowall"),
        gate: member(ty, "zoogate"),
        gated: matches!(entity.class.as_str(), "fences" | "tankwall")
            && catalogue
                .find(&entity.class, &bytes(&entity.type_name), b"g")
                .is_some(),
        tank: tank_kind(ty),
        walkable: (
            crate::build::number(ty, "walkable") == Some(1.0),
            crate::build::number(ty, "walkableByTall") == Some(1.0),
        ),
        forms: (entity.class == "animals").then(|| ty.forms()),
        rubbleable: entity.class == "scenery"
            && crate::build::number(ty, "cRubbleable").unwrap_or(1.0) == 1.0,
    })
}
