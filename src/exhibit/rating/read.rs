//! Reading an exhibit of the world into the plain description the rules work on.

use super::*;

/// Whether the pair of an entity of type `ty` reads the neighbours of a tank ([`entity`]): an
/// animal or a scenery piece (an object, a fence or tank wall piece, a path, a building), not a
/// staff member or a guest.
pub(crate) fn reads_neighbours(ty: &EntityType) -> bool {
    use crate::render::order::Kind;
    ty.class == "animals" || Kind::of_class(ty.class) != Kind::Unit
}

/// The habitat number and name id of an object type (habitat rating 2).
pub(crate) fn object(ty: &EntityType) -> (i32, i32) {
    let it = item(Some(ty), 1);
    (it.habitat, it.name)
}

/// The item an object type makes; a type that is not found makes the default one.
pub(super) fn item(ty: Option<&EntityType>, slots: i32) -> Item {
    let int = |key: &str| ty.and_then(|ty| number(ty, key)).map_or(0, |v| v as i32);
    let (shelter, toy) = (int("cShelter") == 1, int("cToySatisfaction").max(0));
    Item {
        habitat: ty
            .and_then(|ty| number(ty, "cHabitat"))
            .map_or(NO_HABITAT, |v| v as i32),
        name: ty.map_or(0, |ty| ty.name_id),
        kind: ty.map_or_else(String::new, |ty| {
            let text = |b: &[u8]| String::from_utf8_lossy(b).into_owned();
            format!("{}/{}/{}", ty.class, text(&ty.type_name), text(&ty.subtype))
        }),
        slots: slots.max(1),
        foliage: int("cFoliage") == 1,
        rock: int("cRock") == 1,
        stink: int("cStink") > 0,
        rooms: (shelter || toy > 0).then(|| (if shelter { int("cCapacity") } else { 0 }, toy)),
    }
}

/// Reads exhibit `id` of a world into the description the rules work on (rule 5.1 to 5.4).
pub fn input(world: &World, catalogue: &Catalogue, id: u32) -> Input {
    let Some(exhibit) = world.exhibits().get(id) else {
        return Input::default();
    };
    let (species, animals, tally) = animals_on(world, catalogue, &exhibit.order);
    let keeper = (world.units.iter())
        .filter(|u| u.kind == Kind::Keeper)
        .find(|u| matches!(&u.part, Part::Staff(s) if s.assigned.contains(&id)))
        .map(|u| u.id);
    // The types and the tiles of the amphibious neighbours (5.1 point 4, 5.3).
    let (mut near, mut neighbours): (Vec<Likes>, Vec<Site>) = (Vec::new(), Vec::new());
    for n in (exhibit.amphibious.iter()).filter_map(|&n| world.exhibits().get(n)) {
        // Its species, then those of its show neighbours.
        let beside = (n.show_near.iter()).filter_map(|&s| world.exhibits().get(s));
        let listed = [(n, false)].into_iter().chain(beside.map(|s| (s, true)));
        for (from, via) in listed {
            for l in animals_on(world, catalogue, &from.order).0 {
                let seen = (near.iter_mut()).find(|o| (&o.key, &o.subtype) == (&l.key, &l.subtype));
                match seen {
                    // Met again on the tiles of a neighbour: no longer only a show neighbour's.
                    Some(o) => o.via &= via,
                    None => near.push(Likes { via, ..l }),
                }
            }
        }
        neighbours.push(site(world, catalogue, &n.order, n.tank.map(water)));
    }
    let mut show: Vec<Likes> = Vec::new();
    for s in (exhibit.show_near.iter()).filter_map(|&s| world.exhibits().get(s)) {
        for l in animals_on(world, catalogue, &s.order).0 {
            if !show
                .iter()
                .any(|o| (&o.key, &o.subtype) == (&l.key, &l.subtype))
            {
                show.push(l);
            }
        }
    }
    Input {
        site: site(world, catalogue, &exhibit.order, exhibit.tank.map(water)),
        species,
        animals,
        happy: tally.happy,
        sick: tally.sick,
        unhappy: tally.unhappy,
        food: food_on(world, catalogue, &exhibit.order),
        keeper,
        linked: !exhibit.amphibious.is_empty(),
        near,
        neighbours,
        show_linked: !exhibit.show_near.is_empty(),
        show,
    }
}

/// The tiles `order` of an exhibit as the tile pass reads them (5.2).
pub(super) fn site(
    world: &World,
    catalogue: &Catalogue,
    order: &[usize],
    water: Option<Water>,
) -> Site {
    let bytes = |s: &str| s.chars().map(|c| c as u8).collect::<Vec<u8>>();
    let kind = |e: usize| {
        let e = &world.zoo.entities[e];
        catalogue.find(&e.class, &bytes(&e.type_name), &bytes(&e.subtype))
    };
    let mut site = Site {
        tiles: order.len(),
        water,
        ..Site::default()
    };
    let mut items: HashMap<usize, usize> = HashMap::new();
    let mut inside = vec![false; world.zoo.tiles.len()];
    order.iter().for_each(|&t| inside[t] = true);
    for &t in order {
        let tile = &world.zoo.tiles[t];
        match world.path(t) {
            Some(p) => {
                let it = item(kind(p), 1);
                site.paths.push((it.habitat, it.name));
            }
            None => site.terrain[usize::from(tile.terrain) % TERRAINS] += 1,
        }
        let mut held = Vec::new();
        let mut stinks = false;
        for q in 0..4 {
            let Some(e) = world.quadrant(t, q) else {
                continue;
            };
            let at = match items.get(&e) {
                Some(&at) => at,
                None => {
                    let slots = world
                        .shape(e)
                        .map_or(1, |s| s.footprint.0.saturating_mul(s.footprint.1));
                    let item = item(kind(e), slots);
                    site.items.push(item);
                    items.insert(e, site.items.len() - 1);
                    site.items.len() - 1
                }
            };
            let item = &site.items[at];
            stinks |= item.stink;
            if item.rooms.is_some() && !site.listed.contains(&at) {
                site.listed.push(at);
            }
            held.push(at);
        }
        site.slots.push(held);
        site.stinky += u32::from(stinks);
        // Hilliness (5.2): the corner spread, and the cliffs on a side that stays in the exhibit.
        let corners = (0..4).map(|c| i32::from(tile.shape >> (2 * c) & 3));
        site.hills += corners.clone().max().unwrap_or(0) - corners.min().unwrap_or(0);
        site.hills += [0, 2, 4, 6]
            .into_iter()
            .filter(|&d| world.cliff(t, d) && world.neighbour(t, d).is_some_and(|n| inside[n]))
            .count() as i32;
    }
    site
}

pub(super) fn water(t: Tank) -> Water {
    Water {
        level: t.level as i32,
        purity: t.purity as i32,
        show: t.show,
        salt: t.salt,
        filled: t.filled,
    }
}

/// What a recomputation counts of the animals of an exhibit (5.1 point 3).
#[derive(Default)]
pub(super) struct Tally {
    happy: i32,
    sick: u32,
    unhappy: u32,
}

/// The food amounts on the tiles `order` summed by food kind (5.1 point 3); a kind outside the
/// 16 is not summed.
pub(super) fn food_on(world: &World, catalogue: &Catalogue, order: &[usize]) -> [i32; FOODS] {
    let mut sums = [0i32; FOODS];
    let mut seen = HashSet::new();
    for &t in order {
        for e in (0..4).filter_map(|q| world.quadrant(t, q)) {
            let entity = &world.zoo.entities[e];
            let amount = entity.common.as_ref().map(|c| c.food);
            let Some(amount) = amount.filter(|_| entity.class == "food" && seen.insert(e)) else {
                continue;
            };
            let kind = crate::animal::food::kind(catalogue, entity);
            if let Some(sum) = usize::try_from(kind).ok().and_then(|k| sums.get_mut(k)) {
                *sum = sum.saturating_add(amount);
            }
        }
    }
    sums
}

/// The animal types on the tiles `order`, each once in the order they first appear tile by
/// tile, the type of each animal and the counts of the animals.
pub(super) fn animals_on(
    world: &World,
    catalogue: &Catalogue,
    order: &[usize],
) -> (Vec<Likes>, Vec<usize>, Tally) {
    let (mut species, mut animals): (Vec<Likes>, Vec<usize>) = (Vec::new(), Vec::new());
    let mut tally = Tally::default();
    for (at, ty) in animals_at(world, catalogue, order) {
        let unit = (world.zoo.entities[at].common.as_ref()).and_then(|c| world.units.get(c.id));
        if let Some(Part::Animal(a)) = unit.map(|u| &u.part) {
            tally.happy += a.happiness;
            tally.sick += u32::from(a.at_minimum_health);
            tally.unhappy += u32::from(a.very_unhappy);
        }
        let mut likes = Likes::of(ty);
        // The bears' cheat gives the grizzly a bash strength (cheats 4.1).
        likes.bash |= world.units.cheat.bears_bash && likes.ids[0] == crate::cheat::GRIZZLY;
        let at = (species.iter())
            .position(|s| (&s.key, &s.subtype) == (&likes.key, &likes.subtype))
            .unwrap_or_else(|| {
                species.push(likes);
                species.len() - 1
            });
        animals.push(at);
    }
    (species, animals, tally)
}

/// The animals whose own tile is one of `order` (5.1 point 3), tile by tile in that order: the
/// place of each in the entity list and its type.
pub(crate) fn animals_at<'c>(
    world: &World,
    catalogue: &'c Catalogue,
    order: &[usize],
) -> Vec<(usize, &'c EntityType)> {
    let bytes = |s: &str| s.chars().map(|c| c as u8).collect::<Vec<u8>>();
    let mut inside = vec![false; world.zoo.tiles.len()];
    order.iter().for_each(|&t| inside[t] = true);
    let width = world.zoo.width as i64;
    let mut on: HashMap<usize, Vec<(usize, &EntityType)>> = HashMap::new();
    for (at, entity) in world.zoo.entities.iter().enumerate() {
        let (Some(c), "animals") = (&entity.common, entity.class.as_str()) else {
            continue;
        };
        // A carried animal has left the tiles' lists, so it counts in no exhibit (8.2).
        if world.is_carried(c.id) {
            continue;
        }
        let (x, y) = (i64::from(c.x.div_euclid(64)), i64::from(c.y.div_euclid(64)));
        let t = (y >= 0 && x >= 0 && x < width).then(|| (y * width + x) as usize);
        let ty = catalogue.find(
            &entity.class,
            &bytes(&entity.type_name),
            &bytes(&entity.subtype),
        );
        if let Some((t, ty)) = t.filter(|&t| inside.get(t) == Some(&true)).zip(ty) {
            on.entry(t).or_default().push((at, ty));
        }
    }
    (order.iter())
        .flat_map(|t| on.remove(t).unwrap_or_default())
        .collect()
}
