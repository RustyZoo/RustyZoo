//! The edge states of the tiles, as the file packs them.

use crate::render::corner_elevation;
use crate::zoo::Zoo;

/// The four edge states of tile `t` as the file packs them, computed with map drawing 3.5.
pub(super) fn edge_states(zoo: &Zoo, t: usize) -> u8 {
    let width = zoo.width as usize;
    let (tx, ty) = ((t % width) as i32, (t / width) as i32);
    let mut edges = 0;
    // For the edges 0, 2, 4, 6: the step to the neighbour and the two corners on the edge, of
    // the tile and of the neighbour, in the order of increasing coordinate.
    for (d, (dx, dy), own, other) in [
        (0, (0, -1), (7, 1), (5, 3)),
        (2, (1, 0), (1, 3), (7, 5)),
        (4, (0, 1), (5, 3), (7, 1)),
        (6, (-1, 0), (7, 5), (1, 3)),
    ] {
        let beside = u32::try_from(tx + dx)
            .ok()
            .zip(u32::try_from(ty + dy).ok())
            .and_then(|(x, y)| zoo.tile(x, y));
        let state = match beside {
            Some(n) => {
                let tile = &zoo.tiles[t];
                let (t0, t1) = (corner_elevation(tile, own.0), corner_elevation(tile, own.1));
                let (n0, n1) = (corner_elevation(n, other.0), corner_elevation(n, other.1));
                if t1 - t0 == n1 - n0 {
                    match (t0 == n0, t1 == t0) {
                        (true, true) => 0,
                        (true, false) => 1,
                        (false, _) if n0 > t0 => 2,
                        _ => 3,
                    }
                // The slopes differ: the edges 2 and 4 take the state 2 also when they meet at
                // the first corner, as the tile later in row order sees them (3.5).
                } else if t0 < n0 || (t0 == n0 && (d == 2 || d == 4)) {
                    2
                } else {
                    3
                }
            }
            _ => 0,
        };
        edges |= state << (2 * (3 - d / 2));
    }
    edges
}
