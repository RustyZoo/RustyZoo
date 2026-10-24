//! The water of a tank (exhibits and money 3.10): the level moves by one elevation step per
//! simulation step towards the full level while the tank is filled and towards 0 while it is not;
//! filling and draining while the game is paused are instant; the wall height changes with the
//! walls, a split and a merge.

use super::World;
use crate::exhibit::Tank;

impl World {
    /// Tells the world whether the game is paused: a fill or a drain is then instant.
    pub fn set_paused(&mut self, paused: bool) {
        self.paused = paused;
    }

    /// The ids of the tank exhibits.
    pub(super) fn tank_ids(&self) -> Vec<u32> {
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
            self.events.push(super::Event::Tank(id));
            self.sync_tanks();
            // The top of the walls moved: the links (tank fauna 12.2) and the escape check flag;
            // a filled tank with a gate is rated again, and its animals react (habitat rating 4,
            // 5.8). The water level and the purity mark nothing.
            if now.height != before.height {
                self.relink();
                if let Some(e) = self.exhibits.get_mut(id) {
                    e.rating.escape = true;
                    if now.filled && e.gate.is_some() {
                        self.exhibits.mark(id);
                        let (from, to) = (config.top(before.height), config.top(now.height));
                        self.events.push(super::Event::Depth { id, from, to });
                    }
                }
            }
        }
    }

    /// An animal's dung lowers the purity of the water of tank `id` (animals-needs 8.1).
    pub fn pollute(&mut self, id: u32, by: u32) {
        let purity = self.tank(id).map_or(0, |t| i64::from(t.purity));
        self.set_purity(id, purity - i64::from(by));
    }

    /// One simulation step of the water of every tank (3.10): one elevation step up while filled
    /// and below the full level, one down while not filled and above 0.
    /// Whether a level moved.
    pub fn step_water(&mut self) -> bool {
        let before = self.events.len();
        for id in self.tank_ids() {
            self.edit_tank(id, |t, config| {
                let top = config.top(t.height);
                if t.filled && t.level < top {
                    t.level += 1;
                } else if !t.filled && t.level > 0 {
                    t.level -= 1;
                }
            });
        }
        self.events.len() > before
    }

    /// Fills tank `id` (3.10): refused, silently, while something in it may not be in water. The
    /// level then rises with the steps, or at once while the game is paused.
    pub fn fill(&mut self, id: u32) {
        if !self.unfit(id, false).is_empty() {
            return;
        }
        let paused = self.paused;
        self.edit_tank(id, |t, config| {
            t.filled = true;
            if paused {
                t.level = config.top(t.height);
            }
        });
    }

    /// Drains tank `id`: refused while an animal is in it. The level falls with the steps, or to
    /// 0 at once while the game is paused.
    pub fn drain(&mut self, id: u32) {
        let animal = self.tiles_of(id).into_iter().any(|t| {
            let units = self.scene.units_on(t);
            (units.iter()).any(|&o| self.zoo.entities[self.owner[o]].class == "animals")
        });
        if animal {
            return;
        }
        let paused = self.paused;
        self.edit_tank(id, |t, _| {
            t.filled = false;
            if paused {
                t.level = 0;
            }
        });
    }

    /// Gives tank `id` a wall height, cut to 2 and the highest a tank has (4.7); when the water
    /// is then above the full level it is cut at once.
    pub fn set_height(&mut self, id: u32, height: u32) {
        let height = height.max(2).min(self.tank_config.limit);
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
            true => self.fill(kept),
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
    pub fn wall_height(&self, at: usize) -> u32 {
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
