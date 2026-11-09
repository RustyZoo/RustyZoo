//! How an entity takes its place on the map, from its type.

use super::order::Kind;
use super::{habitat_fence, member, tank_kind};
use crate::entity::Catalogue;
use crate::world::Shape;

/// How the entity of type `ty` takes its place: a transient one (a water sparkle or ripple) joins
/// its tile's list and holds no quadrant (map drawing 9.7.10 point 4.5); a transient tank wall
/// piece stays an edge.
pub(crate) fn kind_of(entity: &crate::zoo::Entity, ty: &crate::entity::EntityType) -> Kind {
    match Kind::of_class(&entity.class) {
        Kind::Fixed if tank_kind(ty).transient => Kind::Sparkle,
        kind => kind,
    }
}

/// How an entity takes its place on the map, if its type is known.
pub(crate) fn shape_of(catalogue: &Catalogue, entity: &crate::zoo::Entity) -> Option<Shape> {
    let bytes = |s: &str| s.chars().map(|c| c as u8).collect::<Vec<u8>>();
    let ty = catalogue.find(
        &entity.class,
        &bytes(&entity.type_name),
        &bytes(&entity.subtype),
    )?;
    Some(Shape {
        kind: kind_of(entity, ty),
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

    #[test]
    fn a_transient_type_holds_no_slot_whatever_its_class() {
        let kind = |class, ini| shape(class, ini).map(|s| s.kind);
        assert_eq!(kind("objects", "cIsTransient=1\n"), Some(Kind::Sparkle));
        for ini in [
            "",
            "cIsTransient=0\n",
            "cIsTransient=-1\n",
            "cIsTransient=99999999\n",
        ] {
            assert_eq!(kind("objects", ini), Some(Kind::Fixed), "{ini:?}");
        }
        // The flag moves no unit, no edge piece and no tank wall.
        assert_eq!(kind("animals", "cIsTransient=1\n"), Some(Kind::Unit));
        assert_eq!(kind("fences", "cIsTransient=1\n"), Some(Kind::Edge));
        assert_eq!(kind("tankwall", "cIsTransient=1\n"), Some(Kind::Wall));
    }

    #[test]
    fn a_water_sparkle_on_the_map_holds_no_quadrant_and_stands_in_no_unit_list() {
        let ini = "[Characteristics/Integers]\ncIsTransient=1\n";
        let mut cat = Catalogue::default();
        cat.push(EntityType {
            class: "objects",
            type_name: b"other".to_vec(),
            subtype: b"twaterwv".to_vec(),
            file: None,
            config: Some(Arc::new(Config::parse(ini.as_bytes()).unwrap())),
            name_id: 0,
            footprint: [1, 1, 1],
            height: 0,
        });
        let tile = crate::zoo::Tile {
            elevation: 0,
            shape: 0,
            terrain: 0,
            edges: 0,
        };
        let zoo = crate::zoo::Zoo {
            width: 2,
            height: 2,
            tiles: vec![tile; 4],
            ..Default::default()
        };
        let mut world = crate::world::World::new(zoo, |e| shape_of(&cat, e));
        let sparkle = Entity {
            class: "objects".into(),
            type_name: "other".into(),
            subtype: "twaterwv".into(),
            body_size: None,
            common: Some(Common {
                x: 96,
                y: 96,
                ..Default::default()
            }),
        };
        let shape = shape_of(&cat, &sparkle);
        assert!(world.add_wave(sparkle, shape.unwrap()).is_some());
        let t = world.tile_index(1, 1).unwrap();
        assert!((0..4).all(|q| world.quadrant(t, q).is_none()));
        assert!(world.is_empty(t) && world.units_on(t).next().is_none());
    }
}
