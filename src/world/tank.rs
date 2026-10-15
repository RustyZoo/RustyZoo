//! Making a tank (exhibits and money 3.9): the ground levelled, the walls brought to the tank's
//! side, the water values, the objects a tank refuses, the split of a tank and what the player
//! answers when something has to go.

use super::exhibit::Pair;
use super::{Action, Event, Undone, World};
use crate::exhibit::{EXTERIOR, GROUNDS, Tank, TankConfig};
use crate::render::order::{Kind, ground_height};
use crate::terrain::shape::LIMIT;
use crate::zoo::{Common, Entity, Fence};
use std::collections::HashSet;

/// The offset of the middle of side `d` (0, 2, 4, 6) inside its tile, where a fence piece stands.
fn middle(d: usize) -> (i32, i32) {
    [(32, 0), (63, 32), (32, 63), (0, 32)][d / 2]
}

impl World {
    /// Sets the numbers that make a new tank.
    pub fn set_tank_config(&mut self, config: TankConfig) {
        self.tank_config = config;
    }

    /// Levels the tiles of exhibit `id` flat at the mean of their levels (rounded toward 0) less
    /// `sink`, within the limits of the ground; `record` puts the change in the undo list.
    fn level(&mut self, id: u32, sink: i32, record: bool) {
        let tiles = self.tiles_of(id);
        let sum: i64 = tiles
            .iter()
            .map(|&t| i64::from(self.zoo.tiles[t].elevation))
            .sum();
        let mean = i32::try_from(sum / tiles.len().max(1) as i64).unwrap_or(0);
        let level = mean.saturating_sub(sink).clamp(-LIMIT, LIMIT);
        for t in tiles {
            let tile = self.zoo.tiles[t];
            if (tile.elevation, tile.shape) != (level, 0) {
                self.put_tile(t, level, 0, tile.terrain, record);
            }
        }
    }

    /// Paints the tiles with the terrain type of a show tank or of a tank, free and recorded for
    /// undo; a tile that has the type is left.
    pub(super) fn paint(&mut self, tiles: &[usize], show: bool) {
        let terrain = self.tank_config.terrain[usize::from(show)];
        for &t in tiles {
            let tile = self.zoo.tiles[t];
            if tile.terrain != terrain {
                self.put_tile(t, tile.elevation, tile.shape, terrain, true);
            }
        }
    }

    /// The floors of these tanks after their show state was decided (3.9 "The floor").
    pub(super) fn floors(&mut self, ids: &[u32]) {
        let mut done = Vec::new();
        for &id in ids {
            if done.contains(&id) {
                continue;
            }
            done.push(id);
            if let Some(tank) = self.tank(id) {
                self.paint(&self.tiles_of(id), tank.show);
            }
        }
    }

    /// Brings the walls of the boundary of tank `id` to the tank's side of their lines (the wall
    /// step of 3.9): a line whose slot inside is empty and whose wall stands across gets that wall
    /// moved, or, when the tile across is a tank too, a copy of it (a plain piece), so that two
    /// tanks side by side end with one wall each. The gates that moved in are replaced.
    fn wall_step(&mut self, id: u32) {
        let mut moved = Vec::new();
        for t in self.tiles_of(id) {
            for d in [0, 2, 4, 6] {
                let Some(n) = self.neighbour(t, d) else {
                    continue;
                };
                if self.exhibits.region(n) == Some(id) || self.edge(t, d).is_some() {
                    continue;
                }
                let Some(wall) = self
                    .edge(n, (d + 4) % 8)
                    .filter(|&w| self.shapes[w].is_some_and(|s| s.tank.wall))
                else {
                    continue;
                };
                let piece = match self.tank_at(n).is_some() {
                    true => self.copy_wall(wall, t, d),
                    false => {
                        self.move_wall(wall, t, d);
                        self.zoo.entities[wall].common.as_ref().map(|c| c.id)
                    }
                };
                moved.push((piece, t, d, n));
            }
        }
        // The exhibit whose gate was moved in has none that its keepers can use.
        for (piece, t, _, n) in moved {
            let Some(region) = self
                .exhibits
                .region(n)
                .filter(|&r| r != GROUNDS && r != EXTERIOR)
            else {
                continue;
            };
            if piece.is_some() && self.exhibits.get(region).is_some_and(|e| e.gate == piece) {
                self.replace_gate(region, (n, t));
            }
        }
    }

    /// Puts the wall at position `wall` of the entity list on side `d` of tile `t`.
    fn move_wall(&mut self, wall: usize, t: usize, d: usize) {
        let width = self.zoo.width as usize;
        let (x, y) = middle(d);
        let (x, y) = ((t % width) as i32 * 64 + x, (t / width) as i32 * 64 + y);
        let z = ground_height(&self.zoo.tiles[t], x.rem_euclid(64), y.rem_euclid(64));
        let Some(c) = self.zoo.entities[wall].common.as_mut() else {
            return;
        };
        let old = (c.id, (c.x, c.y, c.z), c.rotation);
        (c.x, c.y, c.z, c.rotation) = (x, y, z, d as u32);
        self.record(Action::Moved {
            id: old.0,
            at: old.1,
            rotation: old.2,
        });
        self.changed();
    }

    /// Builds a new piece of the type of the wall at position `wall` on side `d` of tile `t`, as a
    /// bought piece is (healthy, a plain fence, nothing of the wall's state kept), free, and
    /// returns its id.
    fn copy_wall(&mut self, wall: usize, t: usize, d: usize) -> Option<u32> {
        let id = self.zoo.id_counter.checked_add(1)?;
        let old = &self.zoo.entities[wall];
        let kind = self.shapes[wall]?.tank;
        let width = self.zoo.width as usize;
        let (x, y) = middle(d);
        let (x, y) = ((t % width) as i32 * 64 + x, (t / width) as i32 * 64 + y);
        let copy = Entity {
            class: old.class.clone(),
            type_name: old.type_name.clone(),
            subtype: "f".into(),
            body_size: None,
            common: Some(Common {
                x,
                y,
                z: ground_height(&self.zoo.tiles[t], x.rem_euclid(64), y.rem_euclid(64)),
                rotation: d as u32,
                id,
                name: String::new(),
                colors: Vec::new(),
                fence: Some(Fence {
                    strength: kind.strength,
                    life: kind.life,
                    timers: [0; 2],
                    portal: false,
                }),
                movement_mode: 0,
            }),
        };
        self.zoo.id_counter = id;
        self.zoo.entities.push(copy);
        self.shapes.push(self.shapes[wall]);
        self.record(Action::Placed { id, price: 0.0 });
        self.changed();
        Some(id)
    }

    /// Gives exhibit `e` a new gate after its old one moved to a tank (3.9): the first pair of its
    /// boundary, from `start`, whose tile outside is in the zoo and not in a tank nor in an
    /// exhibit that opens into `e`, and whose line holds one piece that is not a zoo wall. Without
    /// such a pair the old record stays.
    fn replace_gate(&mut self, e: u32, start: Pair) {
        let (mut seen, mut cur) = (HashSet::new(), start);
        while seen.insert(cur) {
            let (_, out) = cur;
            let region = self.exhibits.region(out);
            let line: Vec<usize> = self.line(cur).into_iter().flatten().collect();
            let free = region.is_some_and(|r| !self.opens_into(&self.exhibits, r, e));
            if self.in_zoo(out)
                && self.tank_at(out).is_none()
                && free
                && let [piece] = line[..]
                && self.shapes[piece].is_some_and(|s| s.fence && !s.wall)
                && self.make_door(piece)
            {
                if let Some(old) = self
                    .exhibits
                    .get(e)
                    .and_then(|x| x.gate)
                    .and_then(|g| self.find(g))
                {
                    self.make_fence(old);
                }
                let gate = self.zoo.entities[piece].common.as_ref().map(|c| c.id);
                if let Some(x) = self.exhibits.get_mut(e) {
                    x.gate = gate;
                }
                self.events.push(Event::Gate(e));
                return;
            }
            match self.step(e, cur) {
                Some(next) => cur = next,
                None => return,
            }
        }
    }

    /// The water values of a new tank `id` from `tanks.cfg` (3.9 creation step 3), its wall height
    /// by the kind it turned out to be.
    fn fill_values(&mut self, id: u32, inherited: Option<Tank>) {
        let c = self.tank_config;
        let Some(tank) = self.exhibits.get_mut(id).and_then(|e| e.tank.as_mut()) else {
            return;
        };
        let k = usize::from(tank.show);
        let new = Tank {
            height: c.height[k],
            level: c.level,
            filled: c.filled,
            salt: c.salt,
            purity: c.purity,
            ..*tank
        };
        // The new part of a split tank has the water of the tank it was cut from.
        *tank = match inherited {
            Some(old) => Tank {
                height: old.height,
                level: old.level,
                filled: old.filled,
                salt: old.salt,
                purity: old.purity,
                ..*tank
            },
            None => new,
        };
        self.events.push(Event::Tank(id));
        self.sync_tanks();
    }

    /// Makes tank `id` (3.9 creation): the ground, the walls, the values, and then what the tank
    /// does not allow: the player is asked first when something stands in it. Not while an undo
    /// is replayed or a map is read, which keep the ground and the walls as they are.
    pub(super) fn make_tank(&mut self, id: u32, inherited: Option<Tank>, physical: bool) {
        if physical {
            match inherited {
                // A part cut off a tank keeps its level.
                Some(_) => self.level(id, 0, false),
                None => {
                    let show = self.tank(id).is_some_and(|t| t.show);
                    let sink = self.tank_config.sink[usize::from(show)];
                    self.level(id, sink, true);
                }
            }
            self.wall_step(id);
            let mut changed = self.retank(false);
            changed.push(id);
            self.floors(&changed);
        }
        self.fill_values(id, inherited);
        let asks = physical && inherited.is_none() && !self.refused(id).is_empty();
        let event = match asks {
            true => Event::Objects(id),
            false => Event::New { id, ask: physical },
        };
        self.events.push(event);
    }

    /// The tank cut in two keeps its side: its walls are brought to its side again.
    pub(super) fn rewall(&mut self, id: u32) {
        self.wall_step(id);
    }

    /// The entities (ids) in tank `id` that it does not allow (3.9): a unit whose type is not at
    /// home under water, a scenery object that neither lives under water nor on the surface,
    /// and in a show tank also anything that is not for shows; what is transient is left out.
    pub fn refused(&self, id: u32) -> Vec<u32> {
        let show = self.tank(id).is_some_and(|t| t.show);
        let mut found: Vec<usize> = Vec::new();
        for t in self.tiles_of(id) {
            let units = self.scene.units_on(t).iter().map(|&o| self.owner[o]);
            let slots = (0..4).filter_map(|q| self.quadrant(t, q));
            let edges = [0, 2, 4, 6].into_iter().filter_map(|d| self.edge(t, d));
            for e in slots.chain(self.path(t)).chain(units).chain(edges) {
                let no = self.shapes[e].is_some_and(|s| {
                    let fits = match s.kind {
                        Kind::Unit => s.tank.underwater,
                        _ => s.tank.underwater || s.tank.surface,
                    };
                    !s.tank.transient && (!fits || (show && !s.tank.shows))
                });
                if no && !found.contains(&e) {
                    found.push(e);
                }
            }
        }
        found
            .iter()
            .filter_map(|&e| self.zoo.entities[e].common.as_ref().map(|c| c.id))
            .collect()
    }

    /// The tile outside the first pair of the boundary of exhibit `id`, where an animal or a
    /// staff member that has to leave is put.
    pub fn outside(&self, id: u32) -> Option<usize> {
        self.tiles_of(id).into_iter().find_map(|t| {
            [0, 2, 4, 6]
                .into_iter()
                .filter_map(|d| self.neighbour(t, d))
                .find(|&n| self.exhibits.region(n) != Some(id))
        })
    }

    /// Puts the unit with this id in the middle of tile `t`.
    pub fn move_unit(&mut self, id: u32, t: usize) {
        let width = self.zoo.width as usize;
        let (x, y) = ((t % width) as i32 * 64 + 32, (t / width) as i32 * 64 + 32);
        let z = ground_height(&self.zoo.tiles[t], 32, 32);
        if let Some(c) = self
            .find(id)
            .and_then(|at| self.zoo.entities[at].common.as_mut())
        {
            (c.x, c.y, c.z) = (x, y, z);
            self.changed();
        }
    }

    /// The player said Yes to the question about the objects of tank `id`, which have been
    /// dealt with: it goes on to its name.
    pub fn accept_tank(&mut self, id: u32) {
        self.events.push(Event::New { id, ask: true });
    }

    /// The player said No: the operation that made tank `id` is undone, its number given back.
    pub fn decline_tank(&mut self, id: u32) -> Undone {
        self.waiting.retain(|w| w.0 != id);
        let done = self.undo();
        self.exhibits.counter = self.exhibits.counter.saturating_sub(1).max(1);
        done
    }
}
