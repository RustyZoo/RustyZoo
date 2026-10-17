//! The guest's use of buildings: the seek of a need ([`Cx::seek`], guests 6.4 to 6.9, 7), the chain
//! of tasks 0x66 to 0x6a (goals 11.3; guest buildings 6 and 7) and eating (goals 11.4, guests 9).
//!
//! A building target is a building id on the guest ([`crate::guest::Guest::building`]) with the
//! slot and entrance it was given; the guest walks to the entrance (0x66, 0x67), to the slot (0x68),
//! is a user for the time inside (0x6a) and is released (0x69). The users, the counts and the
//! income are the building's ([`crate::building::Live`]). The thoughts and the notes left on a
//! tile are the thoughts issue's; the log lines are `building use` and `building buy`.

use super::super::{Cx, ENTER_BUILDING, ENTRANCE, EXIT_BUILDING, GO_BUILDING as GO, USE};
use super::IN_BUILDING;
use crate::building::items::{Item, LITTER};
use crate::building::{self, Kind, Live, Mood, Point};
use crate::guest::{BATHROOM, ENERGY, Guest, HAPPINESS, HUNGER, THIRST};
use crate::unit::Part;
use crate::zoo::Entity;

mod litter;

/// The thought of a guest too crowded to be served by a building (guests 7.2).
const CROWDED: i32 = 10243;
/// The thoughts of a price that is really low and too high (guests 9.4).
const CHEAP: i32 = 10072;
const EXPENSIVE: i32 = 10073;

/// The thoughts of a need: wanting it and finding nothing to serve it (guests 14).
fn wants(need: &str) -> (i32, i32) {
    match need {
        "bathroom" => (10084, 10086),
        "food" => (10079, 10081),
        "drink" => (10082, 10083),
        "energy" => (10092, 10093),
        _ => (10090, 10091),
    }
}

/// The slot of an item by need (guests 9.2).
fn slot_of(need: &str) -> Option<usize> {
    match need {
        "food" | "eat" => Some(0),
        "drink" => Some(1),
        "fungift" => Some(2),
        "gift" => Some(3),
        "trash" => Some(4),
        _ => None,
    }
}

impl Cx<'_, '_> {
    fn guest(&mut self) -> Option<&mut Guest> {
        match &mut self.unit.part {
            Part::Guest(g) => Some(g),
            _ => None,
        }
    }

    /// The building `id`: a copy of its entity and its kind.
    fn building(&self, id: u32) -> Option<(Entity, Kind)> {
        let e = self.world.zoo.entities.get(self.world.find(id)?)?.clone();
        let kind = building::kind_of(self.ctx.cat, &e, self.world.units.prices.default)?;
        Some((e, kind))
    }

    fn tile_at(&self, (x, y): Point) -> Option<usize> {
        self.world.tile_index(x.div_euclid(64), y.div_euclid(64))
    }

    /// Guests 7.1 and 7.2: the building that satisfies `need` nearest to the guest within
    /// `radius` tiles, with the expensive ones farther and the cheap ones nearer; a tie is
    /// drawn. When none is found and `crowded` the draw of the crowded building is made: the
    /// error holds the one drawn among all that satisfy the need, if any.
    pub(super) fn nearest(
        &mut self,
        need: &str,
        radius: i64,
        crowded: bool,
    ) -> Result<u32, Option<u32>> {
        let here = self.tile().ok_or(None)?;
        let width = self.world.zoo.width as usize;
        let area = self.world.exhibits().region(here);
        let prices = self.world.units.prices;
        let (mut all, mut best, mut ties) = (Vec::new(), i64::MAX, Vec::new());
        for e in &self.world.zoo.entities {
            let (Some(kind), Some(c)) = (
                building::kind_of(self.ctx.cat, e, prices.default),
                e.common.as_ref(),
            ) else {
                continue;
            };
            let Some(t) = self.tile_at((c.x, c.y)).filter(|_| kind.satisfies(need)) else {
                continue;
            };
            all.push(c.id);
            let live = self.world.units.buildings.get(&c.id);
            if self.world.exhibits().region(t) != area
                || !building::has_open_slot(self.world, e, &kind, live)
            {
                continue;
            }
            let dx = (t % width) as i64 - (here % width) as i64;
            let dy = (t / width) as i64 - (here / width) as i64;
            let mut d = dx * dx + dy * dy;
            let price = live.map_or(kind.price, |l| l.price);
            let scale = match prices.mood(price, kind.price) {
                _ if !kind.commerce => None,
                Mood::Angry => Some(prices.expensive),
                Mood::Happy => Some(prices.cheap),
                Mood::Fair => None,
            };
            if let Some(f) = scale {
                let far = ((d as f64).sqrt() * f64::from(f)).round_ties_even() as i64;
                d = far * far;
            }
            if d > radius * radius || d > best {
                continue;
            }
            if d < best {
                (best, ties) = (d, Vec::new());
            }
            ties.push(c.id);
        }
        let rng = &mut *self.ctx.rng;
        match ties.len() {
            0 if crowded && !all.is_empty() => Err(Some(all[rng.pick(all.len() as u32) as usize])),
            0 => Err(None),
            n => Ok(ties[rng.pick(n as u32) as usize]),
        }
    }

    /// The seek part of a need check (guests 6.4 to 6.9): a guest that is not heading for a
    /// building looks for one that satisfies `need`. The energy seek has the class radius and,
    /// finding nothing, resets the goal and raises the wandered flag.
    pub(crate) fn seek(&mut self, need: &'static str) {
        let Some(k) = crate::guest::keys(self.world, self.ctx.cat, self.at) else {
            return;
        };
        if self.guest().is_none_or(|g| g.building != 0) {
            return;
        }
        let energy = need == "energy";
        let radius = if energy { k.search_radius } else { 46_340 };
        let (want, none) = wants(need);
        self.think(want, None);
        let found = self.nearest(need, i64::from(radius), !energy);
        if found.is_ok_and(|b| self.head_for(b, need)) {
            return;
        }
        match found {
            Err(Some(b)) => self.think(CROWDED, Some(b)),
            _ => self.think(none, None),
        }
        if energy {
            self.reset();
            self.unit.goal.wandered = true;
        }
    }

    /// The first open slot of building `id` whose entrance the guest can reach, the nearest
    /// first (guest buildings 6.2); the guest then holds the path to the entrance. Without one
    /// its path stays as it was.
    fn open_slot(&mut self, id: u32) -> Option<(usize, Point)> {
        let (e, kind) = self.building(id)?;
        let c = self.world.zoo.entities.get(self.at)?.common.as_ref()?;
        let (from, here) = ((c.x, c.y), self.tile());
        let live = self.world.units.buildings.get(&id);
        let list = building::entrances(self.world, &e, &kind, live, from);
        let saved = (
            self.unit.path.clone(),
            self.unit.levels.clone(),
            self.unit.destination,
        );
        for (_, slot, p) in list {
            if let Some(t) = self.tile_at(p)
                && (Some(t) == here || self.path_to(t, false))
            {
                return Some((slot, p));
            }
        }
        (self.unit.path, self.unit.levels, self.unit.destination) = saved;
        None
    }

    /// Guest buildings 6.2: the guest takes building `b` for `need` and sets out for the entrance
    /// of the nearest open slot: task 0x66, or 0x67 for a type with a direct entrance. Whether it
    /// did; if not, the target is cleared.
    fn head_for(&mut self, b: u32, need: &'static str) -> bool {
        if let Some(g) = self.guest() {
            (g.need, g.building) = (Some(need), b);
        }
        let started = self.set_out(b);
        if !started {
            self.clear_target();
        }
        started
    }

    /// The walk of [`Cx::head_for`] to the entrance of the nearest open slot of building `b`.
    fn set_out(&mut self, b: u32) -> bool {
        let Some((_, kind)) = self.building(b) else {
            return false;
        };
        if IN_BUILDING.contains(&self.record().task) {
            return true;
        }
        let Some((slot, p)) = self.open_slot(b) else {
            return false;
        };
        let Some(t) = self.tile_at(p) else {
            return false;
        };
        if Some(t) != self.tile() && !self.start_walk(false, false, false) {
            return false;
        }
        if let Some(g) = self.guest() {
            (g.slot, g.entrance) = (slot, p);
        }
        self.set_task(if kind.direct { ENTRANCE } else { GO }, 0, b, Some(t));
        true
    }

    /// The guest has no building target and serves no need.
    pub(in crate::goal) fn clear_target(&mut self) {
        if let Some(g) = self.guest() {
            (g.need, g.building) = (None, 0);
        }
    }

    /// 0x66 complete: the guest is near the building; with an open slot it goes to its entrance
    /// (0x67), else it gives up.
    pub(in crate::goal) fn building_arrived(&mut self) {
        let id = self.guest().map_or(0, |g| g.building);
        let slot = self.open_slot(id);
        let tile = slot.and_then(|(_, p)| self.tile_at(p));
        match (slot, tile) {
            (Some((slot, p)), Some(t)) => {
                if let Some(g) = self.guest() {
                    (g.slot, g.entrance) = (slot, p);
                }
                self.set_task(ENTRANCE, 0, id, Some(t));
            }
            _ => self.reset(),
        }
    }

    /// 0x67 complete: a charging building with an expensive price turns the guest away angry;
    /// else it walks into the slot (0x68).
    pub(in crate::goal) fn entrance_arrived(&mut self) {
        let Some(id) = self.guest().map(|g| g.building) else {
            return;
        };
        let Some((e, kind)) = self.building(id) else {
            return self.reset();
        };
        if kind.commerce && self.mood(id, &kind) == Mood::Angry {
            self.react(Mood::Angry, id);
            return self.reset();
        }
        let Some((slot, _)) = self.open_slot(id) else {
            return self.reset();
        };
        let to = self.tile_at(building::place(&e, kind.slots[slot].at));
        if let Some(g) = self.guest() {
            g.slot = slot;
        }
        let Some(t) = to else {
            return self.reset();
        };
        // The walk into the building looks at the edge cost only (pathfinding 4.2).
        if Some(t) != self.tile() {
            self.unit.through_building = true;
            if !(self.path_to(t, false) && self.start_walk(false, false, false)) {
                self.unit.through_building = false;
                return self.reset();
            }
        }
        self.set_task(ENTER_BUILDING, 0, id, Some(t));
    }

    /// 0x68 check done: one time in 15 the slot is looked at again; done at the slot.
    pub(in crate::goal) fn enter_check_done(&mut self) {
        let (id, slot) = self.guest().map_or((0, 0), |g| (g.building, g.slot));
        if self.ctx.rng.pick(15) == 0 {
            let open = self.building(id).is_some_and(|(e, kind)| {
                let live = self.world.units.buildings.get(&id);
                building::can_add(self.world, &e, &kind, live, slot)
            });
            if !open {
                return self.reset();
            }
        }
        if self.reached(false) {
            self.record().done = true;
        }
    }

    /// 0x68 complete: the building takes the guest as a user.
    pub(in crate::goal) fn enter_arrived(&mut self) {
        self.unit.path.clear();
        let (id, slot) = self.guest().map_or((0, 0), |g| (g.building, g.slot));
        match self.add_user(id, slot) {
            true => self.set_task(USE, 0, id, None),
            false => self.reset(),
        }
    }

    /// Guest buildings 6.4: the guest becomes a user of slot `slot` of building `id`. The building
    /// counts it, hides it or puts it at the slot, and sells it an item. Whether it did.
    fn add_user(&mut self, id: u32, slot: usize) -> bool {
        let Some((e, kind)) = self.building(id) else {
            return false;
        };
        let live = self.world.units.buildings.get(&id);
        if !building::can_add(self.world, &e, &kind, live, slot) {
            return false;
        }
        let (unit, now) = (self.unit.id, self.now());
        let live = (self.world.units.buildings.entry(id)).or_insert_with(|| Live::new(&kind));
        live.users.push((unit, slot, now));
        live.guests[0] += 1;
        live.guests[2] += 1;
        let price = live.price;
        self.world.units.building_note(format!("use {id} {unit}"));
        if kind.hide {
            self.unit.hidden = true;
            self.world.lift_unit(self.at);
        } else if !kind.outside {
            let (x, y) = building::place(&e, kind.slots[slot].at);
            let z = e.common.as_ref().map_or(0, |c| c.z);
            let facing = building::facing(&e, kind.slots[slot].facing);
            self.world.place_unit(self.at, (x, y, z), facing);
        }
        // Guest buildings 6.4 step 7: the used thought, the items the building removes, the sale.
        if !kind.used.is_empty() {
            let at = self.ctx.rng.pick(kind.used.len() as u32) as usize;
            self.think(kind.used[at], None);
        }
        let items = &self.ctx.cat.items;
        let takes = (kind.removes.iter()).any(|n| items.get(n).is_some_and(|i| i.id == LITTER));
        self.bin_litter(id, takes);
        self.sell(id, &kind, price);
        true
    }

    /// Guest buildings 7.4: a building that sells nothing consumes the guest's item for its need;
    /// else one of its items is drawn and, unless the price is expensive, bought.
    fn sell(&mut self, id: u32, kind: &Kind, price: f32) {
        let need = self.guest().and_then(|g| g.need);
        if kind.sells.is_empty() {
            let item = need.and_then(slot_of).and_then(|s| self.guest()?.items[s]);
            return item.map_or((), |i| self.consume(i));
        }
        let name = &kind.sells[self.ctx.rng.pick(kind.sells.len() as u32) as usize];
        let Some(item) = self.ctx.cat.items.get(name).copied() else {
            return;
        };
        match self.mood(id, kind) {
            Mood::Angry => return self.react(Mood::Angry, id),
            mood => self.react(mood, id),
        }
        self.world
            .units
            .building_note(format!("buy {id} {name} {price}"));
        let slot = need
            .and_then(slot_of)
            .filter(|&s| s != 4 || self.guest().is_some_and(|g| g.items[4].is_none()));
        if let (Some(s), Some(g)) = (slot, self.guest()) {
            g.items[s] = Some(item.id);
            g.gift &= s != 3;
            if item.now {
                self.consume(item.id);
            }
        }
    }

    /// How the price of building `id` strikes a guest (guest buildings 7.3).
    fn mood(&self, id: u32, kind: &Kind) -> Mood {
        let live = self.world.units.buildings.get(&id);
        let price = live.map_or(kind.price, |l| l.price);
        self.world.units.prices.mood(price, kind.price)
    }

    /// The price reaction (guests 9.4): the happiness changes by the class key, and the thought is
    /// about the building.
    fn react(&mut self, mood: Mood, building: u32) {
        let Some(k) = crate::guest::keys(self.world, self.ctx.cat, self.at) else {
            return;
        };
        let change = match mood {
            Mood::Happy => k.price[0],
            Mood::Angry => k.price[1],
            Mood::Fair => 0,
        };
        match mood {
            Mood::Happy => self.think(CHEAP, Some(building)),
            Mood::Angry => self.think(EXPENSIVE, Some(building)),
            Mood::Fair => {}
        }
        if let Some(g) = self.guest() {
            g.pending[HAPPINESS] = g.pending[HAPPINESS].saturating_add(change);
        }
    }

    /// Guests 5.4: the change record is added to the pending values.
    fn apply(&mut self, record: [i32; 6]) {
        if let Some(g) = self.guest() {
            let happy = if g.child { record[1] } else { record[0] };
            for (v, change) in [
                (HAPPINESS, happy),
                (HUNGER, record[2]),
                (THIRST, record[3]),
                (BATHROOM, record[4]),
                (ENERGY, record[5]),
            ] {
                g.pending[v] = g.pending[v].saturating_add(change);
            }
        }
    }

    /// Guests 9.3: the guest consumes the item of name id `id`.
    fn consume(&mut self, id: i32) {
        let Some(item) = self.ctx.cat.items.values().find(|i| i.id == id).copied() else {
            return;
        };
        let Item {
            record,
            remove,
            litter,
            thought,
            ..
        } = item;
        if thought != 0 {
            self.think(thought, None);
        }
        self.apply(record);
        if let Some(g) = self.guest() {
            if remove {
                let at = [0, 1, 3, 4, 2]
                    .into_iter()
                    .find(|&s| g.items[s] == Some(id));
                at.into_iter().for_each(|s| g.items[s] = None);
            }
            if litter && g.items[4].is_none() {
                g.items[4] = Some(LITTER);
            }
        }
    }

    /// 0x6a check done (goals 11.3, guest buildings 6.5): the time inside is over. A building that
    /// is gone, or that does not list the guest, ends the goal.
    pub(in crate::goal) fn use_check_done(&mut self) {
        let id = self.guest().map_or(0, |g| g.building);
        let unit = self.unit.id;
        let entered = self
            .world
            .units
            .buildings
            .get(&id)
            .and_then(|l| l.users.iter().find(|u| u.0 == unit))
            .map(|u| u.2);
        let kind = self.building(id).map(|b| b.1);
        let (Some(entered), Some(kind)) = (entered, kind) else {
            self.show();
            return self.reset();
        };
        let over = self.now().wrapping_sub(entered) >= kind.time_inside.max(0) as u32;
        self.record().done = over && !kind.holds;
    }

    /// A hidden guest is shown again where it stands.
    fn show(&mut self) {
        if std::mem::take(&mut self.unit.hidden) {
            let c = self.world.zoo.entities[self.at].common.as_ref();
            if let Some((p, r)) = c.map(|c| ((c.x, c.y, c.z), c.rotation)) {
                self.world.place_unit(self.at, p, r);
            }
        }
    }

    /// 0x6a complete: the guest sets out to leave (0x69); the building still lists it.
    pub(in crate::goal) fn use_done(&mut self) {
        self.unit.path.clear();
        let id = self.guest().map_or(0, |g| g.building);
        self.set_task(EXIT_BUILDING, 1, id, None);
    }

    /// 0x69 decide (goals 11.3): a user that uses the exit or stays outside is done where it
    /// stands; any other walks, with the building flag, to one entrance of its slot (one draw).
    /// Without a walk it stays.
    pub(in crate::goal) fn exit_decide(&mut self) {
        if self.record().target.tile.is_some() {
            return;
        }
        let (id, slot) = self.guest().map_or((0, 0), |g| (g.building, g.slot));
        let own = self.tile();
        let entrance = self.building(id).and_then(|(e, k)| {
            let list = &k.slots.get(slot)?.entrances;
            let walks = !(k.uses_exit || k.outside) && !list.is_empty();
            walks.then(|| building::place(&e, list[self.ctx.rng.pick(list.len() as u32) as usize]))
        });
        let tile = entrance.and_then(|p| self.tile_at(p));
        if let Some(t) = tile.filter(|&t| Some(t) != own) {
            self.unit.through_building = true;
            if self.path_to(t, false) && self.start_walk(false, false, false) {
                return self.set_task(EXIT_BUILDING, 1, id, Some(t));
            }
            self.unit.through_building = false;
        }
        self.set_task(EXIT_BUILDING, 1, id, own);
    }

    /// Guest buildings 6.6: the building lets the guest go. A charging building books its price
    /// times its factor as concessions; the type's change record is given to the guest; a type
    /// whose users use the exit puts it there, any other shows a hidden guest where it stands.
    fn remove_user(&mut self) {
        let id = self.guest().map_or(0, |g| g.building);
        let unit = self.unit.id;
        let Some((e, kind)) = self.building(id) else {
            return self.show();
        };
        let Some(live) = self.world.units.buildings.get_mut(&id) else {
            return self.show();
        };
        let Some(i) = live.users.iter().position(|u| u.0 == unit) else {
            return self.show();
        };
        let (_, slot, _) = live.users.remove(i);
        if kind.commerce {
            if kind.sells.is_empty() {
                let mood = self.mood(id, &kind);
                self.react(mood, id);
            }
            let Some(live) = self.world.units.buildings.get_mut(&id) else {
                return;
            };
            // A hostile factor or price books nothing.
            let income = (kind.factor * live.price).max(0.0);
            live.income[0] += income;
            live.income[2] += income;
            self.world.units.concessions.push(income);
        }
        self.apply(kind.change);
        match kind.slots[slot].exit.filter(|_| kind.uses_exit) {
            Some(exit) => {
                let (x, y) = building::place(&e, exit);
                let z = e.common.as_ref().map_or(0, |c| c.z);
                self.unit.hidden = false;
                let facing = building::facing(&e, kind.slots[slot].facing);
                self.world.place_unit(self.at, (x, y, z), facing);
            }
            None => self.show(),
        }
    }

    /// 0x69 check done: one time in 15 the building is looked at again (it is gone: the goal ends
    /// and the guest is shown); done at the target.
    pub(in crate::goal) fn exit_check_done(&mut self) {
        let id = self.guest().map_or(0, |g| g.building);
        if self.ctx.rng.pick(15) == 0 && self.building(id).is_none() {
            self.show();
            return self.reset();
        }
        if self.reached(false) {
            self.record().done = true;
        }
    }

    /// 0x69 complete (goals 11.3): the building lets the guest go; one that holds no food wanders
    /// on after a stand (guests 8).
    pub(in crate::goal) fn exit_done(&mut self) {
        self.unit.through_building = false;
        self.remove_user();
        self.reset();
        let food = self.guest().is_some_and(|g| g.items[0].is_some());
        self.unit.goal.wandered = !food;
    }

    /// 0x6b check done (goals 11.4): the nearest building where the guest can eat; done either way.
    pub(in crate::goal) fn eat_check_done(&mut self) {
        let Some(k) = crate::guest::keys(self.world, self.ctx.cat, self.at) else {
            return;
        };
        let found = self.nearest("eat", i64::from(k.search_radius), false);
        if let Some(g) = self.guest() {
            g.building = found.unwrap_or(0);
        }
        self.record().done = true;
    }

    /// 0x6b complete: with a place to eat the guest goes there; else it stands to eat.
    pub(in crate::goal) fn eat_done(&mut self) {
        let b = self.guest().map_or(0, |g| g.building);
        if b == 0 || !self.head_for(b, "eat") {
            self.reset();
            if let Some(g) = self.guest() {
                g.stand = true;
            }
        }
    }

    /// 0x6d init (goals 11.4): the guest eats the first food item it holds, standing.
    pub(in crate::goal) fn eating_init(&mut self) -> bool {
        let Some(item) = self.guest().and_then(|g| g.items[0]) else {
            return false;
        };
        self.consume(item);
        self.start_set("bEatStanding", false)
    }
}

#[cfg(test)]
mod tests;
