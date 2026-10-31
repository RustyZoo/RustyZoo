//! The contagion cheats (cheats 4.2 E): a guest with bits of the contagion byte acts, at each of
//! its goal updates, on the other guests on its tile. A guest that is entering, using or leaving
//! a building is busy (cheats 10 O2): it neither acts nor is acted on. The panic flag is not
//! modelled: a scared guest just decides to leave, unless it stands on water.

use super::IN_BUILDING;
use crate::cheat::spread;
use crate::goal::{Cx, stop_use};
use crate::unit::Part;

impl Cx<'_, '_> {
    /// Ladder start: the actions of the contagion byte on the guests that share the tile.
    pub(in crate::goal) fn spread(&mut self) {
        let bits = self.unit.spreads;
        if bits == 0 || IN_BUILDING.contains(&self.record().task) {
            return;
        }
        let Some(tile) = self.tile() else {
            return;
        };
        let water = self.world.is_water(tile);
        let me = self.unit.id;
        let mut others = Vec::new();
        for (at, e) in self.world.zoo.entities.iter().enumerate() {
            let id = e.common.as_ref().map(|c| c.id);
            if let (true, Some(id)) = (e.class == "guests", id)
                && id != me
                && crate::guest::view::tile_of(self.world, at) == Some(tile)
            {
                others.push((at, id, e.type_name.bytes().next().unwrap_or(0)));
            }
        }
        for (at, id, letter) in others {
            let Some(unit) = self.world.units.get_mut(id) else {
                continue;
            };
            let busy = IN_BUILDING.contains(&unit.goal.record.task);
            let scared = |mask: u8, kinds: &[u8]| {
                bits & mask != 0 && kinds.contains(&letter) && !busy && !water
            };
            if bits & spread::SICKNESS != 0
                && !busy
                && matches!(&unit.part, Part::Guest(g) if !g.sick)
            {
                stop_use(unit);
                if let Part::Guest(g) = &mut unit.part {
                    g.sick = true;
                }
            }
            if let Part::Guest(g) = &mut unit.part
                && (scared(spread::SCARE_WOMEN, b"w") || scared(spread::SCARE_CHILDREN, b"bg"))
            {
                g.leave = true;
            }
            let shirt = match (bits & spread::YELLOW != 0, bits & spread::RED != 0) {
                (true, _) => Some(spread::YELLOW_SHIRT),
                (_, true) => Some(spread::RED_SHIRT),
                _ => None,
            };
            if let Some(colour) = shirt
                && let Some(c) = self.world.zoo.entities[at].common.as_mut()
                && let Some(shirt) = c.colors.first_mut()
            {
                *shirt = colour;
            }
        }
    }
}
