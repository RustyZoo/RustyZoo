//! A unit's own target tile and its position (shows 32.2), apart from the goal record's target.

use super::Unit;
use crate::render::order::ground_height;
use crate::world::World;

/// The position of a unit with no own target tile.
pub(crate) const NONE_POINT: [i32; 3] = [-1; 3];

/// The tile under a position (shows 32.2 point 3): none for the none point, else the tile of
/// (x / 64, y / 64) cut towards zero, none when off the map.
pub(crate) fn tile_under(world: &World, p: [i32; 3]) -> Option<usize> {
    if p == NONE_POINT {
        return None;
    }
    world.tile_index(p[0] / 64, p[1] / 64)
}

impl Unit {
    /// The setter with flag 1 (shows 32.2 point 2): the tile `t`, and its centre as the position
    /// (the ground's height there), or the none point for none.
    pub(crate) fn aim(&mut self, world: &World, t: Option<usize>) {
        self.own_tile = t;
        self.own_loaded = false;
        self.own_pos = t.map_or(NONE_POINT, |t| {
            let width = world.zoo.width as usize;
            let tile = &world.zoo.tiles[t];
            // The ground's height counts 16 x the elevation already.
            let z = ground_height(tile, 32, 32);
            [
                (t % width) as i32 * 64 + 32,
                (t / width) as i32 * 64 + 32,
                z,
            ]
        });
    }

    /// The tile and the position as a goal reset or a pick-up leaves them: none.
    pub(crate) fn clear_own(&mut self) {
        (self.own_tile, self.own_pos) = (None, NONE_POINT);
        self.own_loaded = false;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::unit::Kind;
    use crate::unit::testing::world;

    #[test]
    fn a_new_unit_has_no_own_target_tile_and_the_none_point() {
        // Shows 32.2 point 1.
        let u = Unit::new(1, Kind::Animal);
        assert_eq!((u.own_tile, u.own_pos), (None, [-1; 3]));
    }

    #[test]
    fn the_setter_with_flag_1_takes_the_tile_and_its_centre_at_the_ground_height() {
        // Shows 32.2 point 2: x and y at 64 a tile and 32, z the ground's height there.
        let w = world((6, 4), |tiles, _| {
            tiles[2 * 6 + 3].elevation = 3;
            (tiles[6 + 1].elevation, tiles[6 + 1].shape) = (1, 0x54);
        });
        let mut u = Unit::new(1, Kind::Animal);
        let (flat, sloped) = (w.tile_index(3, 2), w.tile_index(1, 1));
        u.aim(&w, flat);
        assert_eq!(
            (u.own_tile, u.own_pos),
            (flat, [3 * 64 + 32, 2 * 64 + 32, 48])
        );
        u.aim(&w, sloped);
        let ground = ground_height(&w.zoo.tiles[sloped.unwrap()], 32, 32);
        assert!(ground != 16);
        assert_eq!(u.own_pos, [64 + 32, 64 + 32, ground]);
        // None gives the none point.
        u.aim(&w, None);
        assert_eq!((u.own_tile, u.own_pos), (None, NONE_POINT));
    }

    #[test]
    fn the_tile_under_a_position_is_cut_towards_zero_and_none_off_the_map_or_for_the_none_point() {
        // Shows 32.2 point 3.
        let w = world((6, 4), |_, _| {});
        let under = |x, y| tile_under(&w, [x, y, 0]);
        assert_eq!(under(3 * 64 + 63, 2 * 64), w.tile_index(3, 2));
        assert_eq!(tile_under(&w, NONE_POINT), None);
        // Truncation: -1 is column 0, -64 is off the map; so are the far sides.
        assert_eq!(under(-1, 100), w.tile_index(0, 1));
        assert_eq!(under(-64, 0), None);
        assert_eq!(under(6 * 64, 0), None);
        assert_eq!(under(0, i32::MAX), None);
        assert_eq!(under(i32::MIN, 0), None);
    }
}
