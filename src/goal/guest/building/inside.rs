//! The inside tile of a building (goals 11.3.2): the tile the 0x68 walk heads for, found from the
//! unit's own tile, not from the slot.

use super::Cx;

/// Goals 11.3.2 point 5: the straight directions (0: y-1, 2: x+1, 4: y+1, 6: x-1) from the unit's
/// tile towards the building's, one or two; none for the same tile.
fn directions(dx: i32, dy: i32) -> Vec<(i32, i32)> {
    let (n, e, s, w) = ((0, -1), (1, 0), (0, 1), (-1, 0));
    match (dx.signum(), dy.signum(), dx.abs().cmp(&dy.abs())) {
        (0, 0, _) => vec![],
        (_, _, std::cmp::Ordering::Less) if dy < 0 => vec![n],
        (_, _, std::cmp::Ordering::Less) => vec![s],
        (_, _, std::cmp::Ordering::Greater) if dx > 0 => vec![e],
        (_, _, std::cmp::Ordering::Greater) => vec![w],
        (1, -1, _) => vec![n, e],
        (1, 1, _) => vec![e, s],
        (-1, 1, _) => vec![s, w],
        _ => vec![n, w],
    }
}

/// Goals 11.3.2 points 4 to 7: from tile `from`, towards the tile `to` under the building, the first
/// tile that `holds` the building, else the tile one step past the last run. A run tests `n + 1`
/// tiles, `n` the truncated distance, and the next direction starts from the last tile the run
/// reached.
pub(super) fn walk(
    from: (i32, i32),
    to: (i32, i32),
    holds: impl Fn(i32, i32) -> bool,
) -> (i32, i32) {
    let (dx, dy) = (to.0 - from.0, to.1 - from.1);
    let n = f64::from(dx * dx + dy * dy).sqrt() as i32;
    let (mut k, mut t) = (from, from);
    for (sx, sy) in directions(dx, dy) {
        t = (k.0 + sx, k.1 + sy);
        for _ in 0..n {
            if holds(t.0, t.1) {
                break;
            }
            k = t;
            t = (t.0 + sx, t.1 + sy);
        }
        if holds(t.0, t.1) {
            break;
        }
    }
    t
}

impl Cx<'_, '_> {
    /// Goals 11.3.2: the tile index of the inside tile of the building `id` seen from the unit's
    /// own tile; none when the unit or the result is off the map. A neighbour off the map holds
    /// nothing (open point 1).
    pub(super) fn inside_tile(&self, id: u32) -> Option<usize> {
        let c = |i: usize| self.world.zoo.entities.get(i)?.common.as_ref();
        let (b, u) = (self.world.find(id)?, c(self.at)?);
        let b = (b, c(b)?);
        let (px, py) = (u.x.div_euclid(64), u.y.div_euclid(64));
        self.world.tile_index(px, py)?;
        // The physical quarter of the building's position (the world indexes it `qy * 2 + qx`).
        let q = (b.1.y.rem_euclid(64) / 32 * 2 + b.1.x.rem_euclid(64) / 32) as usize;
        let holds = |x, y| {
            let t = self.world.tile_index(x, y);
            t.is_some_and(|t| self.world.quadrant(t, q) == Some(b.0))
        };
        let (x, y) = walk(
            (px, py),
            (b.1.x.div_euclid(64), b.1.y.div_euclid(64)),
            holds,
        );
        self.world.tile_index(x, y)
    }
}
