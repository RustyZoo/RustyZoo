//! The trick list of an animal type (shows 19.1) and the default script built from it (19.2).

use super::Params;
use crate::building::kind_of;
use crate::config::{Section, to_int};
use crate::entity::{Catalogue, EntityType, unit_prefixes};
use crate::place::bulldozer::type_of;
use crate::world::World;
use crate::zoo::Item;

/// The trick that is the treat: every list ends with it and a default script offers it.
pub(crate) const FOOD: u16 = 11299;

/// The tricks of an animal type in order (shows 19.1): for the default subtype's prefix, then the
/// type's own when it differs, the names of `trick` in the section `<prefix>tricks`, all in one
/// list. `empty` empties the list so far; a name reads `<prefix>tricks/<name>` as one trick, a
/// fresh item when the section is missing, unless the list holds one with the same id already. A
/// type that is no animal has none.
pub(crate) fn tricks(ty: &EntityType) -> Vec<Item> {
    let Some(config) = ty.config.as_deref().filter(|_| ty.class == "animals") else {
        return Vec::new();
    };
    let mut out: Vec<Item> = Vec::new();
    for prefix in unit_prefixes(ty, config) {
        let section = |rest: &[u8]| config.section([&prefix[..], b"tricks", rest].concat());
        let Some(list) = section(b"") else {
            continue;
        };
        for name in list.get_all("trick") {
            if name == b"empty" {
                out.clear();
                continue;
            }
            let item = match section(&[b"/", &name[..]].concat()) {
                Some(one) => item(one, ty.name_id as u32),
                None => item(&Section::default(), ty.name_id as u32),
            };
            if !out.iter().any(|t| t.id == item.id) {
                out.push(item);
            }
        }
    }
    out
}

/// Whether the animal type performs: its list is not empty (shows 18.1).
pub(crate) fn performs(ty: &EntityType) -> bool {
    !tricks(ty).is_empty()
}

/// One trick from its section; a fresh item has a satisfaction, a delta, a depth and a complexity
/// of 1, is visible and has the type -1, and a key that is missing or cannot be read leaves the
/// fresh value; the three flags are stored as 1 when the number is not 0.
fn item(section: &Section, species: u32) -> Item {
    let int = |key: &str, fresh: i32| section.get(key).and_then(to_int).unwrap_or(fresh);
    let flag = |key: &str, fresh: i32| u8::from(int(key, fresh) != 0);
    let text = |key: &str| {
        let bytes = section.get(key).unwrap_or_default();
        bytes.iter().map(|&b| b as char).collect::<String>()
    };
    let satisfaction = int("satisfaction", 1);
    Item {
        available: flag("defaultavailable", 0),
        visible: flag("visible", 1),
        id: int("id", 0) as u16,
        kind: species,
        marker: int("type", -1) as u32,
        names: [
            text("name"),
            text("anim"),
            text("keeperpretrick"),
            text("keeperposttrick"),
        ],
        building: int("building", 0) as u32,
        complexity: int("complexity", 1) as u32,
        returns: flag("returntokeeper", 0),
        satisfaction,
        delta: int("satisfactiondelta", 1),
        current: satisfaction,
        depth: int("minimumdepth", 1),
        help: [int("normalHelpID", 0) as u32, int("grayedHelpID", 0) as u32],
        icons: [text("normalIcon"), text("grayedIcon")],
    }
}

/// The items of the default script of animal type `ty` for the show tank `tank` (shows 19.2): the
/// available tricks in order, with the food trick after each run of tricks that adds up to the
/// threshold. `None` for a type with no tricks; a type whose tricks are all unavailable gets a
/// script with no item.
pub(crate) fn default_items(
    world: &World,
    cat: &Catalogue,
    params: &Params,
    tank: u32,
    ty: &EntityType,
) -> Option<Vec<Item>> {
    let list = super::edit::own_items(world, ty);
    if list.is_empty() {
        return None;
    }
    let mut out = Vec::new();
    let mut sum = 0i64;
    for t in list.iter().filter(|t| available(world, cat, tank, ty, t)) {
        out.push(Item {
            available: 1,
            ..t.clone()
        });
        sum += i64::from(t.complexity);
        if sum >= i64::from(params.threshold) {
            out.extend(list.iter().find(|f| f.id == FOOD).cloned());
            sum = 0;
        }
    }
    Some(out)
}

/// Whether the default script takes trick `t` of `ty` for the show tank `tank`: available (by its
/// file or by research) and visible, the water deep enough, and its toy standing in the tank.
pub(crate) fn available(
    world: &World,
    cat: &Catalogue,
    tank: u32,
    ty: &EntityType,
    t: &Item,
) -> bool {
    let research = world
        .research
        .trick_available(ty, i32::from(t.id), t.available != 0);
    let deep = world
        .exhibits()
        .get(tank)
        .and_then(|e| e.tank)
        .is_none_or(|k| i64::from(k.level) >= i64::from(t.depth));
    research && t.visible != 0 && deep && (t.building == 0 || toy_in(world, cat, tank, t.building))
}

/// Whether a building of the `showtrick` kind whose type id is `id` stands on a tile of exhibit
/// `tank`.
fn toy_in(world: &World, cat: &Catalogue, tank: u32, id: u32) -> bool {
    let default = world.units.prices.default;
    world.zoo.entities.iter().any(|e| {
        let Some(c) = e.common.as_ref() else {
            return false;
        };
        let toy = kind_of(cat, e, default).is_some_and(|k| k.satisfies("showtrick"));
        let named = type_of(cat, e).is_some_and(|t| t.name_id as u32 == id);
        let tile = world.tile_index(c.x.div_euclid(64), c.y.div_euclid(64));
        toy && named && tile.is_some_and(|t| world.exhibits().region(t) == Some(tank))
    })
}
