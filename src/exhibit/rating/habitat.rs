//! The habitat rating (rules 5.1 to 5.6): `rate`, one function per rule.

use super::*;

/// The records of the exhibit (9.2), one per name id: the first type of each (an index into `all`,
/// the species, then the amphibious neighbours, then the show neighbours) and the record of each
/// type of the species list.
fn group(input: &Input, all: &[&Likes]) -> (Vec<Record>, Vec<usize>, Vec<usize>) {
    let site = &input.site;
    let (tank, show) = (site.water.is_some(), site.water.is_some_and(|w| w.show));
    let mut records: Vec<Record> = Vec::new();
    // The first type of each record, and the record of each type.
    let mut first: Vec<usize> = Vec::new();
    let mut of: Vec<usize> = Vec::new();
    let own = input.species.len();
    for (i, l) in all.iter().enumerate() {
        // What the pair rates: the exhibit's own species, a land exhibit the amphibious types of
        // its tank, a tank the types of its show neighbours (9.2).
        let rated = i < own || i >= own + input.near.len() || (l.amphibious && !tank && !l.via);
        let at = records.iter().position(|r| r.key == l.key);
        if let Some(r) = at {
            records[r].rated |= rated;
        } else {
            records.push(Record {
                key: l.key.clone(),
                tank,
                show,
                show_linked: input.show_linked,
                amphibious: l.amphibious,
                rated,
                ..Record::default()
            });
            first.push(i);
        }
        of.push(at.unwrap_or(records.len() - 1));
    }
    of.truncate(own);
    (records, first, of)
}

/// The other animals (5.5.7): the types of another name id, by name id, genus and family.
fn other_animals(rec: &mut Record, l: &Likes, around: &[&Likes]) {
    let sum: i64 = (around.iter())
        .filter(|o| o.ids[0] != l.ids[0])
        .flat_map(|o| o.ids)
        .map(|id| lookup(&l.compatible, id))
        .sum();
    rec.other = sum as f32;
}

fn value(l: &Likes, it: &Item) -> i64 {
    l.value(it.habitat) + l.value(it.name)
}

fn per_slot(l: &Likes, it: &Item) -> i64 {
    100 * value(l, it) / i64::from(it.slots.max(1))
}

fn count(n: usize) -> i64 {
    n.min(u32::MAX as usize) as i64
}

/// What the tile pass and the claims add up for each record (5.2 to 5.4): the object sums, the
/// rocks and the tiles with foliage.
struct Scores {
    sums: Vec<i64>,
    rock_count: Vec<u32>,
    foliage_tiles: Vec<u32>,
}

impl Scores {
    /// The scenery of site `s` for the species of record `r`: each slot's share of the value,
    /// the rocks and the tiles with foliage, those of a negative value only when `any`.
    fn scenery(&mut self, r: usize, l: &Likes, s: &Site, any: bool) {
        for tile in &s.slots {
            let mut leafy = false;
            for it in tile.iter().filter_map(|&k| s.items.get(k)) {
                self.sums[r] += per_slot(l, it);
                if any || value(l, it) >= 0 {
                    self.rock_count[r] += u32::from(it.rock);
                    leafy |= it.foliage;
                }
            }
            self.foliage_tiles[r] += u32::from(leafy);
        }
    }
}

/// The tile pass (5.2): the paths, then the scenery.
fn tile_pass(input: &Input, all: &[&Likes], first: &[usize]) -> Scores {
    let site = &input.site;
    let mut scores = Scores {
        sums: vec![0; first.len()],
        rock_count: vec![0; first.len()],
        foliage_tiles: vec![0; first.len()],
    };
    for (r, &i) in first.iter().enumerate() {
        for &(habitat, name) in &site.paths {
            scores.sums[r] += all[i].value(habitat) + 25 * all[i].value(name);
        }
    }
    for (r, &i) in first.iter().enumerate() {
        scores.scenery(r, all[i], site, false);
    }
    scores
}

/// The water of the neighbours (5.3): the filled tiles by water, all their tiles, and their murk.
struct Neighbours {
    fresh: i64,
    salt: i64,
    size: i64,
    murk: f32,
}

/// The neighbours (5.3): of a tank its tiles, the filled ones by water, and its murk, and in a
/// land exhibit its scenery for each amphibious species, whatever its value.
fn neighbours(input: &Input, all: &[&Likes], first: &[usize], scores: &mut Scores) -> Neighbours {
    let tank = input.site.water.is_some();
    let mut nb = Neighbours {
        fresh: 0,
        salt: 0,
        size: 0,
        murk: 0.0,
    };
    for s in &input.neighbours {
        let Some(w) = s.water else { continue };
        let tiles = count(s.tiles);
        nb.size += tiles;
        match (w.filled, w.salt) {
            (true, true) => nb.salt += tiles,
            (true, false) => nb.fresh += tiles,
            _ => {}
        }
        nb.murk += (100 - i64::from(w.purity)).saturating_mul(tiles) as f32 * 0.01f32;
        for (r, &i) in first.iter().enumerate() {
            if !tank && all[i].amphibious {
                scores.scenery(r, all[i], s, true);
            }
        }
    }
    nb
}

/// The claims of shelters and toys (5.4): the room left in each building and the animals that
/// found none, by record.
struct Claims {
    shelter_room: Vec<i32>,
    toy_room: Vec<i32>,
    shelterless: Vec<i32>,
    toyless: Vec<i32>,
}

/// Each animal claims a shelter and a toy (5.4); each walk adds the building's value again (an
/// original bug, kept).
fn claims(input: &Input, of: &[usize], listed: &[&Item], scores: &mut Scores) -> Claims {
    let (shelter_room, toy_room): (Vec<i32>, Vec<i32>) = (listed.iter())
        .map(|it| it.rooms.unwrap_or_default())
        .unzip();
    let n = scores.sums.len();
    let mut c = Claims {
        shelter_room,
        toy_room,
        shelterless: vec![0; n],
        toyless: vec![0; n],
    };
    for &a in &input.animals {
        let Some(&r) = of.get(a) else { continue };
        let l = &input.species[a];
        for toys in [false, true] {
            let (need, room) = if toys {
                (l.toys, &mut c.toy_room)
            } else {
                (l.shelter, &mut c.shelter_room)
            };
            let mut claimed = false;
            for (k, &it) in listed.iter().enumerate() {
                scores.sums[r] += per_slot(l, it);
                if need && value(l, it) >= 0 && room[k] != 0 {
                    room[k] -= 1;
                    claimed = true;
                    break;
                }
            }
            if need && !claimed {
                [&mut c.shelterless, &mut c.toyless][usize::from(toys)][r] += 1;
            }
        }
    }
    c
}

/// The buildings of the claims, the exhibit's then the neighbours' (5.3).
fn listed(input: &Input) -> Vec<&Item> {
    (std::iter::once(&input.site).chain(&input.neighbours))
        .flat_map(|s| s.listed.iter().map(|&b| &s.items[b]))
        .collect()
}

/// How many buildings whose room is all left (`left == full`) the species values (5.5.5, 5.5.6).
fn unused(listed: &[&Item], l: &Likes, full: &[i32], left: &[i32]) -> i32 {
    (listed.iter().enumerate())
        .filter(|&(k, &it)| full[k] > 0 && left[k] == full[k] && value(l, it) >= 0)
        .count() as i32
}

/// Terrain (5.5.1) and its bits (5.6), land only; terrain 17's pair is bits 55 and 56. An
/// amphibious species counts (5.5.1) the neighbour water that would bring the share of fresh or
/// salt water to the one it wants, as far as the neighbours have.
fn terrain_share(rec: &mut Record, l: &Likes, site: &Site, nb: &Neighbours) {
    let tank = site.water.is_some();
    let mut extra = [0i64; TERRAINS];
    for (k, available) in [(FRESH, nb.fresh), (SALT, nb.salt)] {
        let w = i64::from(l.terrain[k]);
        if l.amphibious && (1..=99).contains(&w) {
            let x = (w * count(site.tiles) - 100 * i64::from(site.terrain[k])) / (100 - w);
            extra[k] = x.min(available).max(0);
        }
    }
    let d = (count(site.tiles) + extra[FRESH] + extra[SALT]).max(1) as f32;
    let mut sum = 0.0f32;
    for (k, more_water) in extra.into_iter().enumerate() {
        let (w, count) = (l.terrain[k], site.terrain[k]);
        let (wanted, share) = (w as f32, (count as i64 + more_water) as f32 * 100.0 / d);
        sum += if w < 0 {
            if count > 0 { wanted } else { 0.0 }
        } else {
            wanted.min(share)
        };
        let (more, less) = if w < 0 {
            (count > 0, false)
        } else {
            (wanted < share - 3.0, share + 3.0 < wanted)
        };
        if !tank && (more || less) {
            rec.set(
                bit::terrain(k) + u32::from(more),
                w >= 0 && (share - wanted).abs() > 6.0,
            );
        }
    }
    rec.terrain = if tank { 100.0 } else { sum.min(100.0) };
}

/// Foliage, rocks and elevation (5.5.3, 5.5.4): an amphibious species counts all the
/// neighbours' water, not its extras.
fn scenery_scores(rec: &mut Record, l: &Likes, site: &Site, foliage: u32, rocks: u32, water: i64) {
    let n = site.tiles.max(1) as f32;
    let water = if l.amphibious { water } else { 0 };
    let d = (count(site.tiles) + water).max(1) as f32;
    let tree = foliage as f32 * 100.0 / d;
    let rock = rocks as f32 * 100.0 / (4.0 * d);
    let hill = site.hills as f32 * 50.0 / n;
    for (m, p, weight, bits) in [
        (tree, l.trees as f32, 8.0, (bit::FEW_TREES, bit::MANY_TREES)),
        (rock, l.rocks as f32, 6.0, (bit::FEW_ROCKS, bit::MANY_ROCKS)),
    ] {
        let few = m < p - 4.0 || (m == 0.0 && p > 0.0);
        if few || m > p + 4.0 {
            rec.set(if few { bits.0 } else { bits.1 }, (m - p).abs() > 8.0);
        }
        let score = 100.0 - weight * (m - p).abs().clamp(0.0, 50.0);
        if bits.0 == bit::FEW_TREES {
            rec.foliage = score
        } else {
            rec.rocks = score
        }
    }
    let p = l.elevation as f32;
    if hill < p - 10.0 || hill > p + 10.0 {
        rec.set(
            if hill < p {
                bit::TOO_FLAT
            } else {
                bit::TOO_STEEP
            },
            (hill - p).abs() > 20.0,
        );
    }
    rec.elevation = 100.0 - (hill - p).abs().clamp(0.0, 50.0);
}

/// An amphibious species in a tank with no land beside it (5.5.4, 5.6).
fn no_land(rec: &mut Record) {
    for b in [
        bit::FEW_TREES,
        bit::MANY_TREES,
        bit::TOO_FLAT,
        bit::TOO_STEEP,
        bit::FEW_ROCKS,
        bit::MANY_ROCKS,
    ] {
        rec.clear(b);
    }
    rec.elevation = 100.0;
    rec.set(bit::NO_LAND, true);
}

/// Shelters (5.5.5) and toys (5.5.6): `(s, w)` the animals without a shelter and without a toy,
/// `(u, t)` the buildings of the species left unused, `total` the animals without a shelter in all.
fn shelters_toys(rec: &mut Record, (s, w): (i32, i32), (u, t): (i32, i32), total: i32) {
    let penalty = match (s, u) {
        (1..=3, _) => 0,
        (4..=6, _) => 15 * s,
        (7.., _) => 100 * total - 600,
        (_, 3..) => (50 * u + 100) * (u - 3).max(1),
        _ => 0,
    };
    rec.shelters = (100 - penalty).min(100) as f32;
    for (flag, on) in [(bit::NEEDS_SHELTER, s > 0), (bit::UNUSED_SHELTER, u > 0)] {
        if on {
            rec.set(flag, u > 3 || s > 7);
        }
    }
    let penalty = match (w, t) {
        (4.., _) => 15 * w,
        (1..=3, _) => 0,
        (_, 3..) => (50 * t + 100) * (t - 2),
        _ => 0,
    };
    rec.toys = (100 - penalty).min(100) as f32;
    for (flag, on) in [(bit::NEEDS_TOYS, w > 0), (bit::UNUSED_TOYS, t > 1)] {
        if on {
            rec.set(flag, t > 2);
        }
    }
}

/// Depth, water quality and salinity (5.5.8), then the water quality bit, written again after
/// the terrain pairs.
fn water_quality(rec: &mut Record, l: &Likes, site: &Site, nb: &Neighbours) {
    (rec.depth, rec.water, rec.salinity) = (100.0, 100.0, 100.0);
    if let Some(water) = site.water {
        let (a, b) = (i64::from(l.depth.0), i64::from(l.depth.1));
        let level = i64::from(water.level);
        let (d, flag) = match (a != b, level < a, level > b) {
            (true, true, _) => (a - level, bit::TOO_SHALLOW),
            (true, _, true) => (level - b, bit::TOO_DEEP),
            _ => (0, 0),
        };
        let p = 20 * d.clamp(0, 30);
        if d > 0 {
            rec.set(flag, p >= 50);
        }
        rec.depth = (100 - p).min(100) as f32;
        rec.water = water.purity as f32;
        if water.salt != l.salt() {
            rec.salinity = -400.0;
            rec.set(bit::SALINITY, true);
        }
    } else if l.amphibious && nb.size > 0 {
        rec.water = 100.0 - nb.murk / nb.size as f32 * 100.0;
    }
    rec.clear(bit::DIRTY_WATER);
    if rec.water < 50.0 {
        rec.set(bit::DIRTY_WATER, rec.water < 25.0);
    }
}

/// Rates the exhibit for each species in it, in the order the species first appear, then for
/// those of its amphibious neighbours (rules 5.1 to 5.6). The fields that depend on the animals'
/// own state (happiness sums, hunger counts) are not part of the rating and not kept.
pub fn rate(input: &Input) -> Vec<Record> {
    let site = &input.site;
    let tank = site.water.is_some();
    let all: Vec<&Likes> = (input.species.iter())
        .chain(&input.near)
        .chain(&input.show)
        .collect();
    let (mut records, first, of) = group(input, &all);
    for &a in &input.animals {
        if let Some(&r) = of.get(a) {
            records[r].animals += 1;
        }
    }
    let around = surrounding(input);
    for (rec, &i) in records.iter_mut().zip(&first) {
        other_animals(rec, all[i], &around);
    }
    let n = site.tiles.max(1) as f32;
    let mut scores = tile_pass(input, &all, &first);
    let nb = neighbours(input, &all, &first, &mut scores);
    let listed = listed(input);
    let claims = claims(input, &of, &listed, &mut scores);
    let rooms: Vec<(i32, i32)> = (listed.iter())
        .map(|it| it.rooms.unwrap_or_default())
        .collect();
    let (full_shelter, full_toy): (Vec<i32>, Vec<i32>) = rooms.into_iter().unzip();
    let total_shelterless: i32 = claims.shelterless.iter().sum();
    let denominator = 100.0f32 * (0.025f32 * n).max(1.0);
    for (r, &i) in first.iter().enumerate() {
        let l = all[i];
        let rec = &mut records[r];
        let u = unused(&listed, l, &full_shelter, &claims.shelter_room);
        let t = if site.water.is_some_and(|w| w.show) {
            0
        } else {
            unused(&listed, l, &full_toy, &claims.toy_room)
        };
        terrain_share(rec, l, site, &nb);
        // Objects (5.5.2).
        let mut objects = (scores.sums[r] as f32 / denominator).min(100.0);
        if objects < 0.0 {
            objects *= 50.0;
        }
        rec.objects = objects;
        scenery_scores(
            rec,
            l,
            site,
            scores.foliage_tiles[r],
            scores.rock_count[r],
            nb.fresh + nb.salt,
        );
        if tank && l.amphibious && !input.linked {
            no_land(rec);
        }
        let (s, w) = (claims.shelterless[r], claims.toyless[r]);
        shelters_toys(rec, (s, w), (u, t), total_shelterless);
        water_quality(rec, l, site, &nb);
        // Stink (5.2).
        let stink = site.stinky as f32 / n;
        if stink > 0.05 {
            rec.set(bit::STINK, stink > 0.1);
        }
    }
    records
}
