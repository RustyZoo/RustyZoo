//! The checks of a pending object: the ones every kind shares, those of fences and the smart
//! fences test (placing 5.1 to 6.3).

use super::*;

/// The checks every object shares (placing 5.1, 5.2, 5.3): the first failure keeps its message.
#[allow(clippy::too_many_arguments)]
pub(super) fn check(
    p: &mut Pending,
    world: &World,
    cat: &Catalogue,
    water: &[bool],
    dirt: &dyn Fn(usize) -> Option<(u8, f32)>,
    smart: bool,
    class: Class,
    fp: (i32, i32),
) {
    let t = &cat.types[p.ty];
    let (tx, ty) = p.tile;
    let Some(tile) = world.tile_index(tx, ty) else {
        return fail(p, Some(NOT_IN_THE_ZOO));
    };
    if !world.in_zoo(tile) {
        return fail(p, Some(NOT_IN_THE_ZOO));
    }
    // A tank filter goes next to a tank that no filter serves, facing it (placing 5.1).
    if t.class == "filter" {
        match world.filter_facing(tile, p.facing) {
            Some(d) => p.facing = d,
            None => return fail(p, Some(NO_TANK)),
        }
    }
    let same = |old: usize| {
        let same = world.zoo.entities.get(old).is_some_and(|e| is_of(e, t));
        (Verdict::Overwrite, if same { 0.0 } else { cost(t) })
    };
    if class == Class::Fence {
        return check_fence(p, world, cat, water, smart, tile);
    }
    if class == Class::Path
        && let Some(old) = world.path(tile)
    {
        (p.verdict, p.price) = same(old);
    }
    if class == Class::Path {
        // The only checks of a path (placing 5.3): flat or one side raised, and on water only
        // where dirt could be painted first. No footprint, nothing in the way: it goes under
        // objects.
        if !path_shape(world.zoo.tiles[tile].shape) {
            fail(p, None);
        }
        let wet = water
            .get(usize::from(world.zoo.tiles[tile].terrain))
            .copied()
            .unwrap_or(false);
        if wet {
            match dirt(tile) {
                Some((kind, paint)) => {
                    p.paint = Some(kind);
                    p.price += paint;
                }
                None => fail(p, None),
            }
        }
        return;
    }
    // The quadrants of the footprint (map drawing 9.3.1): all on the map, in the zoo and free.
    if !(1..=MAX_FOOTPRINT).contains(&fp.0) || !(1..=MAX_FOOTPRINT).contains(&fp.1) {
        return fail(p, None);
    }
    let wet = |at: usize| {
        water
            .get(usize::from(world.zoo.tiles[at].terrain))
            .copied()
            .unwrap_or(false)
    };
    let (x0, y0) = (p.x - 16 * (fp.0 - 1), p.y - 16 * (fp.1 - 1));
    // The quadrants on the map and in the zoo; the others fail without a message (only the
    // object's own tile has one, above).
    let mut quads = Vec::new();
    for j in 0..fp.1 {
        for i in 0..fp.0 {
            let qx = (x0 + 32 * i).div_euclid(32);
            let qy = (y0 + 32 * j).div_euclid(32);
            match world.tile_index(qx.div_euclid(2), qy.div_euclid(2)) {
                Some(at) if world.in_zoo(at) => {
                    quads.push((at, (qy.rem_euclid(2) * 2 + qx.rem_euclid(2)) as usize));
                }
                _ => fail(p, None),
            }
        }
    }
    // Land or water (placing 5.4, 5.2 item 2). A type that is not a land type stands on
    // water only, which says so (10564) before anything else of the footprint; a type that
    // only swims fails on land, one that does not swim in water, both without a message.
    let flag = |key: &str, default: bool| number(t, key).map_or(default, |n| n == 1.0);
    let swims = flag("cSwims", false);
    for &(at, _) in &quads {
        // The tiles of a tank count as water for the types that live in it (placing 5.4).
        let tank = world.tank_at(at).is_some();
        if !flag("cLand", true) && !wet(at) && !tank {
            fail(p, Some(NOT_ON_WATER));
        }
        if (only_swims(t) && !wet(at) && !tank) || (!only_swims(t) && !swims && wet(at)) {
            fail(p, None);
        }
    }
    for &(at, quadrant) in &quads {
        if let Some(held) = world.quadrant(at, quadrant) {
            fail(p, Some(IN_THE_WAY));
            if !p.blocking.contains(&held) {
                p.blocking.push(held);
            }
        }
    }
    let tiles: Vec<usize> = quads.iter().map(|&(at, _)| at).collect();
    // Units on a tile of the footprint whose rectangle meets the object's are in the way (item 4);
    // none is, for a walkable type. A unit's rectangle is its footprint centred on it, both axes
    // the larger one at an odd rotation, the axes swapped at the other quarter turn. The helicopter
    // lets the object through (it moves in the air); recommended: nobody else does.
    let units: Vec<(usize, i32, i32, (i32, i32))> = world
        .zoo
        .entities
        .iter()
        .enumerate()
        .filter_map(|(i, e)| {
            let (c, shape) = (e.common.as_ref()?, world.shape(i)?);
            let on = world.tile_index(c.x.div_euclid(64), c.y.div_euclid(64))?;
            let (fx, fy) = world.unit_footprint(i)?;
            let rect = match c.rotation % 4 {
                0 => (fx, fy),
                _ if c.rotation % 2 != 0 => (fx.max(fy), fx.max(fy)),
                _ => (fy, fx),
            };
            let unit = shape.kind == Kind::Unit && tiles.contains(&on);
            unit.then_some((i, c.x, c.y, rect))
        })
        .collect();
    let meets = |centre: i32, own: i32, other: i32, reach: i32| {
        i64::from((centre - other).abs()) < i64::from(own) + i64::from(reach)
    };
    for &(i, x, y, (fx, fy)) in &units {
        if !flag("walkable", false)
            && !unit::lets_through_object(world, i, t)
            && meets(p.x, 16 * fp.0, x, 16 * fx)
            && meets(p.y, 16 * fp.1, y, 16 * fy)
        {
            fail(p, Some(IN_THE_WAY));
            p.blocking.push(i);
        }
    }
    // More than a quarter tile needs flat ground at one height (item 5), more than a tile no
    // fence and no cliff on the sides inside it (item 6).
    if ((fp.0 > 1 || fp.1 > 1) && uneven(world, &tiles))
        || ((fp.0 > 2 || fp.1 > 2) && fenced(world, x0, y0, fp))
    {
        fail(p, None);
    }
    // A place refused for any reason collects every unit on its tiles, whether its rectangle meets
    // the object's or not (10555 unless a message was stored before).
    if p.verdict == Verdict::Invalid {
        for &(i, ..) in &units {
            if !p.blocking.contains(&i) {
                fail(p, Some(IN_THE_WAY));
                p.blocking.push(i);
            }
        }
    }
}

/// Whether a tile of `tiles` is not flat or not at the height of the first (placing 5.2 item 5).
pub(crate) fn uneven(world: &World, tiles: &[usize]) -> bool {
    let tile = |t: usize| world.zoo.tiles[t];
    tiles.first().is_some_and(|&first| {
        tiles
            .iter()
            .any(|&t| tile(t).shape != 0 || tile(t).elevation != tile(first).elevation)
    })
}

/// Whether a tile side inside the footprint of `fp` quarters whose first quarter is at
/// `(x0, y0)` holds a fence or is a cliff (placing 5.2 item 6).
pub(crate) fn fenced(world: &World, x0: i32, y0: i32, fp: (i32, i32)) -> bool {
    let (tx0, ty0) = (x0.div_euclid(64), y0.div_euclid(64));
    let tx1 = (x0 + 32 * (fp.0 - 1)).div_euclid(64);
    let ty1 = (y0 + 32 * (fp.1 - 1)).div_euclid(64);
    (ty0..=ty1).any(|ty| {
        (tx0..=tx1).any(|tx| {
            [(2, 1, 0), (4, 0, 1)].into_iter().any(|(d, dx, dy)| {
                let inside = tx + dx <= tx1 && ty + dy <= ty1;
                let both = world
                    .tile_index(tx, ty)
                    .zip(world.tile_index(tx + dx, ty + dy));
                matches!((inside, both), (true, Some((a, b)))
                    if world.edge(a, d).is_some()
                        || world.edge(b, (d + 4) % 8).is_some()
                        || world.cliff(a, d)
                        || world.cliff(b, (d + 4) % 8))
            })
        })
    })
}

/// The fence check (placing 6.3): the zoo wall stays, nothing crosses the side, the smart fences
/// test and the entrance quadrant refuse a piece without a message, a fence that only swims is
/// refused on dry land with 10564; the slot of a piece of the same type above its decayed life takes a
/// free replacement, any other the full price.
fn check_fence(
    p: &mut Pending,
    world: &World,
    cat: &Catalogue,
    water: &[bool],
    smart: bool,
    tile: usize,
) {
    let t = &cat.types[p.ty];
    let d = p.facing as usize;
    if world.wall(tile, d) || world.gate(tile) {
        return fail(p, None);
    }
    // In a tank only a tank wall stands, a show wall in a show tank and the other in the others
    // (placing 6.3 rule 3).
    if let Some(tank) = world.tank_at(tile).and_then(|id| world.tank(id)) {
        let wall = t
            .config
            .as_deref()
            .is_some_and(|c| c.section("TankFence").is_some());
        if !wall || (number(t, "cIsShowFence") == Some(1.0)) != tank.show {
            return fail(p, None);
        }
    }
    // No object holds a quadrant of this tile and one of the tile across the side.
    let held = |t: usize| (0..4).filter_map(move |q| world.quadrant(t, q));
    if let Some(across) = world.neighbour(tile, d)
        && held(tile).any(|h| held(across).any(|o| o == h))
    {
        return fail(p, None);
    }
    if smart && smarter(p, world, cat) {
        return fail(p, None);
    }
    if let Some(old) = world.edge(tile, d).and_then(|o| world.zoo.entities.get(o)) {
        let life = old
            .common
            .as_ref()
            .and_then(|c| c.fence)
            .map_or_else(|| number(t, "cLife").unwrap_or(LIFE), |f| f.life as f32);
        let free = is_of(old, t) && life > number(t, "cDecayedLife").unwrap_or(0.0);
        (p.verdict, p.price) = (Verdict::Overwrite, if free { 0.0 } else { cost(t) });
    }
    // A fence that only swims stands on water or in a tank alone: 10564.
    let wet = world.tank_at(tile).is_some()
        || water
            .get(usize::from(world.zoo.tiles[tile].terrain))
            .copied()
            .unwrap_or(false);
    if number(t, "cLand") == Some(0.0) && only_swims(t) && !wet {
        fail(p, Some(NOT_ON_WATER));
    }
}

/// Whether the smart fences test refuses the piece (placing 6.3 rule 5): within four tiles of its
/// tile, a unit with a body over two half tiles that covers it, or the object holding the last
/// quarter of a tile whose placement footprint differs from its footprint and whose rectangle
/// holds the piece.
fn smarter(p: &Pending, world: &World, cat: &Catalogue) -> bool {
    let (tx, ty) = p.tile;
    let near = |x: i32, y: i32| (x - tx).abs() <= 4 && (y - ty).abs() <= 4;
    let bytes = |s: &str| s.chars().map(|c| c as u8).collect::<Vec<u8>>();
    let of = |e: &Entity| cat.find(&e.class, &bytes(&e.type_name), &bytes(&e.subtype));
    let counted =
        |x: i32, y: i32| near(x, y) && world.tile_index(x, y).is_some_and(|u| world.in_zoo(u));
    let units = world.zoo.entities.iter().enumerate().any(|(i, e)| {
        let Some(c) = e
            .common
            .as_ref()
            .filter(|_| Kind::of_class(&e.class) == Kind::Unit)
        else {
            return false;
        };
        let (ux, uy) = (c.x.div_euclid(64), c.y.div_euclid(64));
        // The unit's own footprint, egg or crate included, turned with its facing (placing 5.2).
        let (mut fx, mut fy) = world.unit_footprint(i).unwrap_or_default();
        if c.rotation % 4 != 0 {
            (fx, fy) = (fy, fx);
        }
        counted(ux, uy)
            && (fx > 2 || fy > 2)
            && fx / 2 >= 2 * (ux - tx).abs()
            && fy / 2 >= 2 * (uy - ty).abs()
    });
    units
        || (tx - 4..=tx + 4).any(|ux| {
            (ty - 4..=ty + 4).any(|uy| {
                let Some(u) = world.tile_index(ux, uy).filter(|_| counted(ux, uy)) else {
                    return false;
                };
                let Some(e) = world.quadrant(u, 3).and_then(|o| world.zoo.entities.get(o)) else {
                    return false;
                };
                let (Some(c), Some(t)) = (e.common.as_ref(), of(e)) else {
                    return false;
                };
                let own = (t.footprint[0], t.footprint[1]);
                let mut at = placement_footprint(t);
                if c.rotation % 4 != 0 {
                    at = (at.1, at.0);
                }
                (c.x.div_euclid(64), c.y.div_euclid(64)) == (ux, uy)
                    && placement_footprint(t) != own
                    && i64::from((p.x - c.x).abs()) < 16 * i64::from(at.0)
                    && i64::from((p.y - c.y).abs()) < 16 * i64::from(at.1)
            })
        })
}
