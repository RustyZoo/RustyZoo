//! The water of a tank (exhibits and money 3.10): the level moves by one elevation step per
//! simulation step towards the full level while the tank is filled and towards 0 while it is not;
//! filling and draining while the game is paused are instant; the wall height changes with the
//! walls, a split and a merge.

use super::World;
use crate::exhibit::Tank;

/// A draw owed at its place among the others (map drawing 9.7.8 point 3).
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum Draw {
    /// The tank got clean water with some level: its ripples are set up.
    Ripples(u32),
    /// The wall information of the tank is updated: its ripples are set up again.
    Walls(u32),
    /// The fence piece draws its start time now.
    Start(u32),
    /// The fence piece is made healthy: its wear interval, then its start time.
    Heal(u32),
    /// The fence piece is made healthy without a start time: its wear interval only (a copy of a
    /// gate, whose constructor draws no start time).
    Interval(u32),
    /// A set-up queued while the tanks of a split had each other's tiles: the index in
    /// [`World::take_over`].
    Over(usize),
    /// The wall piece is deleted (a merge's walls between its two tanks, after the update).
    Delete(u32),
}

impl World {
    /// Tells the world whether the game is paused: a fill or a drain is then instant.
    pub(crate) fn set_paused(&mut self, paused: bool) {
        self.paused = paused;
    }

    /// The ids of the tank exhibits.
    pub(crate) fn tank_ids(&self) -> Vec<u32> {
        let tanks = self.exhibits.list().iter();
        tanks.filter(|e| e.tank.is_some()).map(|e| e.id).collect()
    }

    /// Changes the values of tank `id` and tells it when they differ. A level that comes back to 0
    /// from above resets the purity to `initialWaterPurity`.
    pub(super) fn edit_tank(
        &mut self,
        id: u32,
        change: impl FnOnce(&mut Tank, &crate::exhibit::TankConfig),
    ) {
        let config = self.tank_config;
        let Some(tank) = self.exhibits.get_mut(id).and_then(|e| e.tank.as_mut()) else {
            return;
        };
        let before = *tank;
        change(tank, &config);
        // The level coming back to 0 from above gives clean water again (3.10).
        let reset = before.level > 0 && tank.level == 0 && tank.purity != config.purity;
        if reset {
            tank.purity = config.purity;
        }
        let (now, changed) = (*tank, *tank != before);
        if changed {
            // Told only when the level or the purity differ from the last line of the tank (the
            // values before the first change when none was told): a change of the flag alone is not.
            let (level, purity) = (now.level, now.purity);
            let told = self.told_water.get(&id).copied();
            if told.unwrap_or((before.level, before.purity)) != (level, purity) {
                self.told_water.insert(id, (level, purity));
                self.units
                    .world_note(format!("water {id} level {level} purity {purity}"));
            }
            // Clean water with some level, where there was none: the ripples are set up now.
            if super::tank::clean(&now, config.murky) && !super::tank::clean(&before, config.murky)
            {
                self.push_set_up(Draw::Ripples(id));
            }
            self.events.push(super::Event::Tank(id));
            // The level back to 0 removes the tank's transients (map drawing 9.7.10 point 6).
            if now.level == 0 && before.level > 0 {
                self.drop_sparkles(id);
            }
            if (now.level, now.height) != (before.level, before.height) {
                self.stir(id);
                if !self.wet.contains(&id) {
                    self.wet.push(id);
                }
            }
            self.sync_tanks();
            // The top of the walls moved or the fill state changed: the links (tank fauna 12.2,
            // exhibits and money 3.11). A new height sets the escape check flag;
            // a filled tank with a gate is rated again, and its animals react (habitat rating 4,
            // 5.8). The water level and the purity mark nothing.
            if now.height != before.height || now.filled != before.filled {
                self.relink();
            }
            if now.height != before.height
                && let Some(e) = self.exhibits.get_mut(id)
            {
                e.rating.escape = true;
                if now.filled && e.gate.is_some() {
                    self.exhibits.mark(id);
                    let (from, to) = (config.top(before.height), config.top(now.height));
                    self.events.push(super::Event::Depth { id, from, to });
                }
            }
        }
    }

    /// Tank `id` had its water level, base level, wall height or ground changed: the guests in it
    /// and at its gate get the AI reset (guest buildings 12.2 point 22). Not a tank: nothing.
    pub(crate) fn stir(&mut self, id: u32) {
        if self.tank(id).is_some() && !self.stirred.contains(&id) {
            self.stirred.push(id);
        }
    }

    /// The ground of tile `t` changed: its tank, if any, is stirred.
    pub(super) fn stir_tile(&mut self, t: usize) {
        if let Some(x) = self.exhibits.region(t) {
            self.stir(x);
        }
    }

    /// The tanks stirred since the last call.
    pub(crate) fn take_stirred(&mut self) -> Vec<u32> {
        std::mem::take(&mut self.stirred)
    }

    /// The tanks whose water level or wall height moved since the last call.
    pub(crate) fn take_wet(&mut self) -> Vec<u32> {
        std::mem::take(&mut self.wet)
    }

    /// The draws owed, for a test.
    #[cfg(test)]
    pub(crate) fn draws(&self) -> &[Draw] {
        &self.draws
    }

    /// The draws owed written short, for a test: `s7` a start time, `h7` a healthy piece, `i7` its
    /// interval only, `w3` and `r3` the set-ups of tank 3, a queued one with the number of walls
    /// it kept (`w3:6`), `d7` a delete.
    #[cfg(test)]
    pub(crate) fn drawn(&self) -> Vec<String> {
        let one = |d: &Draw| match *d {
            Draw::Ripples(t) => format!("r{t}"),
            Draw::Walls(t) => format!("w{t}"),
            Draw::Start(p) => format!("s{p}"),
            Draw::Heal(p) => format!("h{p}"),
            Draw::Interval(p) => format!("i{p}"),
            Draw::Delete(p) => format!("d{p}"),
            Draw::Over(k) => match &self.overs[k] {
                Some((t, walls, held)) => {
                    format!("{}{t}:{}", if *walls { 'w' } else { 'r' }, held.len())
                }
                None => "taken".into(),
            },
        };
        self.draws.iter().map(one).collect()
    }

    /// Queues a draw behind the ones owed, for a test.
    #[cfg(test)]
    pub(crate) fn queue_draw(&mut self, draw: Draw) {
        self.draws.push(draw);
    }

    /// The next draw owed, in order. With `waiting` a ripple draw of a tank whose units wait for
    /// their reset stays queued (the ripples come after the reset, map drawing 9.7.8 point 4), and
    /// so does everything behind it.
    pub(crate) fn next_draw(&mut self, waiting: bool) -> Option<Draw> {
        let first = *self.draws.first()?;
        if waiting
            && self
                .draw_tank(first)
                .is_some_and(|t| self.stirred.contains(&t))
        {
            return None;
        }
        Some(self.draws.remove(0))
    }

    /// The tank a draw sets up the ripples of, if it is a set-up.
    pub(crate) fn draw_tank(&self, draw: Draw) -> Option<u32> {
        match draw {
            Draw::Ripples(t) | Draw::Walls(t) => Some(t),
            Draw::Over(k) => self.overs.get(k)?.as_ref().map(|o| o.0),
            _ => None,
        }
    }

    /// Queues a ripple set-up. While the tanks of a split have each other's tiles it keeps the
    /// walls the tank holds now, for the set-up runs after they have gone back.
    pub(super) fn push_set_up(&mut self, draw: Draw) {
        let (tank, walls) = match draw {
            Draw::Ripples(t) => (t, false),
            Draw::Walls(t) => (t, true),
            _ => return self.draws.push(draw),
        };
        if !self.swapped {
            return self.draws.push(draw);
        }
        let held = self.exhibits.get(tank).map(|e| self.walls_over(&e.order));
        self.overs
            .push(Some((tank, walls, held.unwrap_or_default())));
        self.draws.push(Draw::Over(self.overs.len() - 1));
    }

    /// The set-up queued as draw `k`: the tank, whether it removes its ripples first, its walls.
    pub(crate) fn take_over(&mut self, k: usize) -> Option<(u32, bool, Vec<u32>)> {
        self.overs.get_mut(k)?.take()
    }

    /// The tanks stirred up to and including `tank`, in order: their units are reset now.
    pub(crate) fn take_stirred_through(&mut self, tank: u32) -> Vec<u32> {
        let Some(n) = self.stirred.iter().position(|&t| t == tank) else {
            return Vec::new();
        };
        self.stirred.drain(..=n).collect()
    }

    /// Whether tank `id` holds no ripples though its water is clean, until its next set-up.
    pub(crate) fn is_bare(&self, id: u32) -> bool {
        self.bare.contains(&id)
    }

    /// A save puts every clean tank's ripples back, a bare tank's too.
    pub(crate) fn set_all_up(&mut self) {
        self.bare.clear();
    }

    /// Tank `id` is set up: it is not bare any more.
    pub(crate) fn set_up(&mut self, id: u32) {
        self.bare.retain(|&t| t != id);
    }

    /// The wall information of the tank that tile `t` belongs to is updated (map drawing 9.7.8
    /// point 3.2): its ripples are set up again whether or not its walls changed.
    pub(super) fn update_walls(&mut self, t: usize) {
        if let Some(id) = self.tank_at(t) {
            self.push_set_up(Draw::Walls(id));
        }
    }

    /// The wall information of the tank of the tile that holds fence piece `id` is updated.
    pub(crate) fn update_walls_of(&mut self, id: u32) {
        let tile = self.find(id).and_then(|at| self.slot_of(at));
        if let Some((t, _)) = tile {
            self.update_walls(t);
        }
    }

    /// A fence piece bought on a slot that holds one of the same type (it keeps its id): on an
    /// active tank wall its tank updates its wall information, then the piece's validation
    /// draws its start time once more (map drawing 9.7.8 point 3.2.2 and 3.4). Whether it did.
    pub(crate) fn retyped_wall(&mut self, id: u32) -> bool {
        let active = self.active_wall_tank(id).is_some();
        if active {
            self.update_walls_of(id);
            self.draws.push(Draw::Start(id));
        }
        active
    }

    /// The tank of the wall piece `id` when the wall is active: a tank wall whose tank has a
    /// wall height (map drawing 9.7.8 point 3.2.2).
    pub(crate) fn active_wall_tank(&self, id: u32) -> Option<u32> {
        let at = self.find(id)?;
        self.shapes[at].filter(|s| s.tank.wall)?;
        let (t, _) = self.slot_of(at)?;
        let tank = self.tank_at(t)?;
        self.tank(tank).filter(|k| k.height > 0).map(|_| tank)
    }

    /// Whether a tank was made or went, or the ground of a tile changed, since the last call.
    pub(crate) fn take_reshaped(&mut self) -> bool {
        std::mem::take(&mut self.reshaped)
    }

    /// An animal's dung lowers the purity of the water of tank `id` (animals-needs 8.1).
    pub(crate) fn pollute(&mut self, id: u32, by: u32) {
        let purity = self.tank(id).map_or(0, |t| i64::from(t.purity));
        self.set_purity(id, purity - i64::from(by));
    }

    /// One simulation step of the water of every tank (3.10): one elevation step up while filled
    /// and below the full level, one down while not filled and above 0; a tank that comes empty
    /// with another target salinity takes it and is filled again.
    /// Whether a level moved.
    pub(crate) fn step_water(&mut self) -> bool {
        let before = self.events.len();
        for id in self.tank_ids() {
            let mut swapped = false;
            self.edit_tank(id, |t, config| {
                let top = config.top(t.height);
                if t.filled && t.level < top {
                    t.level += 1;
                } else if !t.filled && t.level > 0 {
                    t.level -= 1;
                    if t.level == 0 && t.target != t.salt {
                        (t.salt, swapped) = (t.target, true);
                    }
                }
            });
            if swapped {
                self.fill(id);
            }
        }
        self.events.len() > before
    }

    /// Fills tank `id` (3.10): refused, silently, while something in it may not be in water; the
    /// first such object (an entity id) is returned. The level then rises with the steps, or at
    /// once while the game is paused.
    pub(crate) fn fill(&mut self, id: u32) -> Option<u32> {
        if let Some(&object) = self.unfit(id, false).first() {
            return Some(object);
        }
        let paused = self.paused;
        self.edit_tank(id, |t, config| {
            (t.filled, t.target) = (true, t.salt);
            if paused {
                t.level = config.top(t.height);
            }
        });
        None
    }

    /// Drains tank `id`: refused (false) while an animal is in it. The level falls with the steps,
    /// or to 0 at once while the game is paused.
    pub(crate) fn drain(&mut self, id: u32) -> bool {
        let drained = self.drain_quietly(id);
        if drained {
            crate::show::drained(self, id);
        }
        drained
    }

    /// [`World::drain`] without the abort event of the show (a load posts it by its own rule).
    pub(super) fn drain_quietly(&mut self, id: u32) -> bool {
        let animal = self.tiles_of(id).into_iter().any(|t| {
            let units = self.scene.units_on(t);
            (units.iter()).any(|&o| self.zoo.entities[self.owner[o]].class == "animals")
        });
        if animal {
            return false;
        }
        let paused = self.paused;
        self.edit_tank(id, |t, _| {
            t.filled = false;
            if paused {
                t.level = 0;
            }
        });
        true
    }

    /// Gives tank `id` the salinity `salt` (4.7 fill tools): another salinity drains it first and
    /// is taken as soon as it is empty, at once when it already is. False when the drain was
    /// refused (an animal is in the tank); only a tank that is still filled then keeps its
    /// salinity. The caller fills it.
    pub(crate) fn set_salinity(&mut self, id: u32, salt: bool) -> bool {
        let Some(tank) = self.tank(id) else {
            return false;
        };
        if tank.salt == salt {
            return true;
        }
        let drained = self.drain(id);
        if self.tank(id).is_some_and(|t| t.filled) {
            return false;
        }
        self.edit_tank(id, |t, _| {
            t.target = salt;
            if t.level == 0 {
                t.salt = salt;
            }
        });
        drained
    }

    /// The lowest wall height tank `id` takes (4.7): 2, or the deepest need less the water offset.
    /// The need of an object in a quarter of a tile is the `cDepth` of its type; that of a unit
    /// (but a transient one) the larger of its `cDepth` and the `cHeight` of each quarter object
    /// it cannot walk through.
    pub(crate) fn wall_floor(&self, id: u32) -> u32 {
        let mut need = i32::MIN;
        for t in self.tiles_of(id) {
            let held: Vec<_> = (0..4).filter_map(|q| self.quadrant(t, q)).collect();
            let shape = |e: usize| self.shapes[e];
            need = (held.iter().filter_map(|&e| shape(e)))
                .map(|s| s.tank.depth)
                .fold(need, i32::max);
            for e in self
                .units_on(t)
                .filter_map(shape)
                .filter(|s| !s.tank.transient)
            {
                let blocks = held.iter().filter_map(|&o| shape(o)).filter(|o| {
                    !(if e.tank.tall {
                        o.walkable.1
                    } else {
                        o.walkable.0
                    })
                });
                need = blocks
                    .map(|o| o.tank.height)
                    .fold(need.max(e.tank.depth), i32::max);
            }
        }
        // The water offset is taken off the deepest need (the shipped -1 adds one).
        need.saturating_sub(self.tank_config.offset).max(2) as u32
    }

    /// Gives tank `id` a wall height, cut to the lowest it takes and the highest a tank has (4.7);
    /// when the water is then above the full level it is cut at once.
    pub(crate) fn set_height(&mut self, id: u32, height: u32) {
        let height = height.max(self.wall_floor(id)).min(self.tank_config.limit);
        self.edit_tank(id, |t, config| {
            t.height = height;
            t.level = t.level.min(config.top(height));
        });
    }

    /// The water values of tank `kept` after tank `gone` joined it (3.10 merge), `a` and `b`
    /// being the values and the number of tiles of each before. Done only when the merged tank
    /// has a gate.
    pub(super) fn merge_water(&mut self, kept: u32, a: (Tank, usize), b: (Tank, usize)) {
        let weight = |t: &(Tank, usize)| t.0.height as usize * t.1;
        let (va, vb) = (weight(&a), weight(&b));
        let tiles = (a.1 + b.1).max(1);
        let height = (va + vb) / tiles;
        // The tank with the larger volume sets the flag and the salt; a tie goes to the filled one,
        // then the salt one, then the tank that stays.
        let rank = |t: &(Tank, usize)| (weight(t), t.0.filled, t.0.salt);
        let lead = if rank(&b) > rank(&a) { b.0 } else { a.0 };
        let filled: Vec<_> = [a, b].into_iter().filter(|t| t.0.filled).collect();
        let sum: usize = filled.iter().map(weight).sum();
        let purity = filled
            .iter()
            .map(|t| t.0.purity as usize * weight(t))
            .sum::<usize>()
            .checked_div(sum);
        let reset = self.tank_config.purity;
        self.edit_tank(kept, |t, _| {
            t.filled = lead.filled;
            t.salt = lead.salt;
            t.purity = purity.map_or(reset, |p| p as u32);
        });
        self.set_height(kept, u32::try_from(height).unwrap_or(u32::MAX));
        match lead.filled {
            true => _ = self.fill(kept),
            false => {
                self.drain(kept);
                self.clear_refused(kept);
                self.edit_tank(kept, |t, _| t.purity = reset);
            }
        }
    }

    /// The wall height the pending gate over the fence piece at `at` takes (placing 6.4): the
    /// height of the tank that piece is a wall of, 0 for a piece of another type or of a wall no
    /// tank records.
    pub(crate) fn wall_height(&self, at: usize) -> u32 {
        let wall = self
            .shapes
            .get(at)
            .copied()
            .flatten()
            .is_some_and(|s| s.tank.wall);
        let tile = self.slot_of(at).filter(|_| wall);
        let tank = tile.and_then(|(t, d)| {
            let sides = [Some(t), self.neighbour(t, d)];
            sides.into_iter().flatten().find_map(|i| self.tank_at(i))
        });
        tank.and_then(|id| self.tank(id)).map_or(0, |t| t.height)
    }
}
