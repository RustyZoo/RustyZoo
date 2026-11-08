//! How an entity takes its place on the map, from its type.

use super::order::Kind;
use super::{habitat_fence, member, tank_kind};
use crate::entity::Catalogue;
use crate::world::Shape;

/// How an entity takes its place on the map, if its type is known.
pub(crate) fn shape_of(catalogue: &Catalogue, entity: &crate::zoo::Entity) -> Option<Shape> {
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
        // Scenery types are stored under the class `objects`.
        rubbleable: matches!(
            entity.class.as_str(),
            "objects" | "building" | "food" | "filter" | "rubble"
        ) && crate::build::number(ty, "cRubbleable").unwrap_or(1.0) == 1.0,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Config;
    use crate::entity::EntityType;
    use crate::zoo::{Common, Entity};
    use std::sync::Arc;

    /// A `thing` of each class, with the integers `ini`; the shape the map gives the entity.
    fn shape(class: &'static str, ini: &str) -> Option<Shape> {
        let ty = EntityType {
            class,
            type_name: b"thing".to_vec(),
            subtype: Vec::new(),
            file: None,
            config: Some(Arc::new(
                Config::parse(format!("[Characteristics/Integers]\n{ini}").as_bytes()).unwrap(),
            )),
            name_id: 0,
            footprint: [1, 1, 1],
            height: 0,
        };
        let mut cat = Catalogue::default();
        cat.push(ty);
        let entity = Entity {
            class: class.into(),
            type_name: "thing".into(),
            subtype: String::new(),
            body_size: None,
            common: Some(Common::default()),
        };
        shape_of(&cat, &entity)
    }

    #[test]
    fn objects_buildings_food_filters_and_rubble_are_rubbleable_when_the_key_is_1_or_absent() {
        for class in ["objects", "building", "food", "filter", "rubble"] {
            let rubbleable = |ini| shape(class, ini).is_some_and(|s| s.rubbleable);
            assert!(rubbleable(""), "{class} by default");
            assert!(rubbleable("cRubbleable=1\n"), "{class}");
            assert!(!rubbleable("cRubbleable=0\n"), "{class} with 0");
            assert!(!rubbleable("cRubbleable=2\n"), "{class} with 2");
        }
        // Fences, paths, units and the rest are not.
        for class in ["fences", "paths", "animals", "tankwall", "ambient"] {
            assert!(shape(class, "").is_none_or(|s| !s.rubbleable), "{class}");
        }
    }
}
