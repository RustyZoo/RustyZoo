//! An animal's trick (shows 23.2 to 23.5): the start, the behaviour trick, the toy trick and the
//! food trick's goal, task 0x7d.

use super::Cx;
use crate::animal::food;
use crate::behaviour::Data;
use crate::entity::Catalogue;
use crate::show::{event, in_show_now, toy, trick_item};
use crate::sim::Rng;
use crate::sim::{Event, KEEPER_CHANNEL, SHOW_CHANNEL};
use crate::staff::service;
use crate::unit::{Part, movement};
use crate::world::World;
use crate::zoo::Item;

/// The task of the food trick.
pub(crate) const TRICK_FOOD: u8 = 125;

/// 23.2: animal `id` is asked to start trick `it` of show `show` by the show manager, outside the
/// unit's turn; 0 when it did, else the code that aborts the item.
pub(crate) fn start_trick(
    world: &mut World,
    cat: &Catalogue,
    rng: &mut Rng,
    data: &dyn Data,
    id: u32,
    (it, show): (&Item, u16),
) -> i32 {
    let mut code = 3;
    super::dropped::run(world, cat, rng, data, id, |cx| code = cx.start(it, show));
    code
}

impl Cx<'_, '_> {
    /// 23.2: the animal is asked to start trick `it` of show `show`; 0 when it did, else the code
    /// that aborts the item. A food trick is never refused.
    fn start(&mut self, it: &Item, show: u16) -> i32 {
        if it.marker != 3 && !self.will_do() {
            return 8;
        }
        match it.marker {
            0 => self.behaviour_trick(it, show),
            1 => self.toy_trick(it, show),
            3 => {
                self.set_task(TRICK_FOOD, 0, 0, None);
                (self.unit.trick, self.unit.show) = (it.id, show);
                0
            }
            _ => 3,
        }
    }

    /// 23.2 `will_do`: an animal at or below `cSkipTrickHappiness` refuses with the chance
    /// `cSkipTrickChance` (one draw); the happiness is the current value.
    fn will_do(&mut self) -> bool {
        let k = crate::animal::keys(self.world, self.ctx.cat, self.at);
        let happy = match &self.unit.part {
            Part::Animal(a) => a.happiness,
            _ => return true,
        };
        happy > k.skip_happiness || !self.ctx.rng.chance(k.skip_chance)
    }

    /// 23.3: the set `b<name>` starts, forced and not interruptible; whatever the set, the trick
    /// started.
    fn behaviour_trick(&mut self, it: &Item, show: u16) -> i32 {
        self.start_with(&format!("b{}", it.names[0]), false, true);
        (self.unit.trick, self.unit.show) = (it.id, show);
        0
    }

    /// 23.4: the animal aims at a toy of the item's kind and goes through the building chain to it;
    /// 3 when none is free.
    fn toy_trick(&mut self, it: &Item, show: u16) -> i32 {
        let (who, from) = (self.species(), self.tile());
        let kind = i64::from(it.building);
        let found = from.and_then(|from| {
            let (world, cat) = (&*self.world, self.ctx.cat);
            toy::find_toy(world, cat, self.ctx.rng, who.as_ref(), from, kind)
        });
        let Some(toy) = found else {
            return 3;
        };
        if let Part::Animal(a) = &mut self.unit.part {
            (a.building, a.need) = (toy, crate::guest::TOY_NEED);
        }
        (self.unit.trick, self.unit.show) = (it.id, show);
        0
    }

    /// An event from the unit to itself: a trick done or aborted.
    pub(super) fn tell_trick(&mut self, kind: u16) {
        crate::building::leave::post_own(self.world, self.unit.id, kind);
    }

    /// 12.2 with shows 24 point 3: the clear goal that ends the building chain of an animal. One in
    /// a show tells its item aborted before the reset, so a toy trick never waits for ever.
    pub(super) fn clear_goal(&mut self) {
        if in_show_now(self.world, self.ctx.cat, self.unit) {
            self.tell_trick(event::FAILED);
        }
        self.reset();
    }

    /// Whether the animal is on its way to a toy for its trick (the need served is `showtrick`).
    pub(super) fn toy_need(&self) -> bool {
        matches!(&self.unit.part, Part::Animal(a) if a.need == crate::guest::TOY_NEED && a.building != 0)
    }

    /// The name of the trick the unit is doing, which its use set carries (guest buildings 6.4.1).
    pub(super) fn trick_name(&self) -> Option<String> {
        let it = trick_item(self.world, self.unit.show, self.unit.trick)?;
        Some(it.names[0].clone())
    }

    /// 23.5 decide: with no target, the animal walks to a tile round the inside of its tank's gate
    /// (the keeper's draw), at the water level; no way there clears the goal and aborts the trick.
    pub(super) fn trick_food_decide(&mut self) {
        if self.record().target.tile.is_some() {
            return;
        }
        let Some(x) = self.tile().and_then(|t| food::exhibit_of(self.world, t)) else {
            return;
        };
        let mover = movement::mover(self.unit, self.world, self.at, self.tile().unwrap_or(0));
        let gate = service::gate_tiles(self.world, x).map(|g| g.0);
        let spot = gate.map(|inside| self.gate_side(x, inside, &mover));
        let water = self.world.tank(x).map_or(0, |t| t.level as i32);
        let walking = spot.is_some_and(|t| {
            let level = self.world.zoo.tiles[t].elevation + water;
            self.path_to_level(t, false, Some(level)) && self.start_walk(false, false, false)
        });
        if !walking {
            self.reset();
            return self.tell_trick(event::FAILED);
        }
        // The food tile is the one it stands on: `fFaceTowardFood` turns it to nothing.
        if let Part::Animal(a) = &mut self.unit.part {
            a.food = 0;
        }
    }

    /// 23.5 complete: the set of the trick, the specialist's cue, the treat's changes, the goal
    /// cleared.
    pub(super) fn trick_food_complete(&mut self) {
        let (trick, show) = (self.unit.trick, self.unit.show);
        if let Some(it) = trick_item(self.world, show, trick) {
            self.behaviour_trick(&it, show);
        }
        self.cue_post_trick(show);
        // The change record {adult +10, young +10, hunger -10, energy 0}: an adult or a young
        // animal gains the happiness (animals 6).
        let happy = match self.world.zoo.entities[self.at].subtype.as_str() {
            "m" | "f" | "y" => 10,
            _ => 0,
        };
        if let Part::Animal(a) = &mut self.unit.part {
            a.happiness_change = a.happiness_change.saturating_add(happy);
            a.hunger_change = a.hunger_change.saturating_sub(10);
        }
        self.reset();
    }

    /// The show tells the specialist the trick is done, unless the animal has no state there or
    /// its state is aborted.
    fn cue_post_trick(&mut self, show: u16) {
        let infos = self.world.zoo.shows.infos.iter().flatten();
        let mut states = infos.filter(|i| i.id == show).flat_map(|i| &i.show.items);
        let state = states.find(|s| s.animal == self.unit.id);
        let Some(item) = state.filter(|s| !s.aborted).map(|s| s.item as u16) else {
            return;
        };
        self.world.units.posts.push(Event {
            sender: show.into(),
            target: 0,
            data: 0,
            extra: item,
            kind: event::TRICK_DONE,
            delay: 1,
            sender_channel: SHOW_CHANNEL,
            channel: KEEPER_CHANNEL,
        });
    }
}
