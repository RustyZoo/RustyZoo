//! Making a tank (exhibits and money 3.9): the ground levelled, the walls brought to the tank's
//! side, the water values, the objects a tank refuses, the split of a tank and what the player
//! answers when something has to go.

use super::{Action, Draw, Event, Undone, World};
use crate::exhibit::{EXTERIOR, GROUNDS, Tank, TankConfig};
use crate::render::order::{Kind, ground_height};
use crate::terrain::shape::LIMIT;
use crate::zoo::{Common, Entity, Fence};

/// The offset of the middle of side `d` (0, 2, 4, 6) inside its tile, where a fence piece stands.
fn middle(d: usize) -> (i32, i32) {
    [(32, 0), (63, 32), (32, 63), (0, 32)][d / 2]
}

/// Whether a tank holds clean water for ripples: some level, some wall height and a purity of at
/// least `murky` (map drawing 9.7.8).
pub(super) fn clean(t: &Tank, murky: i32) -> bool {
    t.level > 0 && t.height > 0 && i64::from(t.purity) >= i64::from(murky)
}

impl World {
    /// The numbers that make a new tank.
    pub(crate) fn tank_config(&self) -> &TankConfig {
        &self.tank_config
    }

    /// Sets the numbers that make a new tank.
    pub fn set_tank_config(&mut self, config: TankConfig) {
        self.tank_config = config;
        // The tanks of a loaded map are cut to the highest wall and the full level of these numbers.
        for id in self.tank_ids() {
            self.edit_tank(id, |t, c| {
                t.height = t.height.min(c.limit);
                t.level = t.level.min(c.top(t.height));
            });
        }
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
        self.flatten(id, mean.saturating_sub(sink), record);
    }

    /// The ground level of tank `id` (4.7): the level of its first tile.
    pub(crate) fn ground(&self, id: u32) -> Option<i32> {
        let first = *self.exhibits.get(id)?.order.first()?;
        Some(self.zoo.tiles[first].elevation)
    }

    /// Makes the tiles of exhibit `id` flat at `level`, cut to the limits of the ground.
    pub(crate) fn flatten(&mut self, id: u32, level: i32, record: bool) {
        let level = level.clamp(-LIMIT, LIMIT);
        for t in self.tiles_of(id) {
            let tile = self.zoo.tiles[t];
            if (tile.elevation, tile.shape) != (level, 0) {
                self.put_tile(t, level, 0, tile.terrain, record);
                if record {
                    self.join_tiles();
                }
            }
        }
        // The base level setter updates the tank at once (shows 34.4 point 5).
        self.reach(id);
        self.relink(false);
    }

    /// Paints the tiles with the terrain type of a show tank or of a tank, free and recorded for
    /// undo; a tile that has the type is left.
    pub(super) fn paint(&mut self, tiles: &[usize], show: bool) {
        if self.replaying {
            return;
        }
        let terrain = self.tank_config.terrain[usize::from(show)];
        for &t in tiles {
            let tile = self.zoo.tiles[t];
            if tile.terrain != terrain {
                self.put_tile(t, tile.elevation, tile.shape, terrain, true);
                self.join_tiles();
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
    /// tanks side by side end with one wall each. The gates that moved in are replaced. Each
    /// pair with a tile across validates the pieces of its two slots, inside then outside, which
    /// draw a start time each unless they are gates (map drawing 9.7.8 point 3.4).
    pub(super) fn wall_step(&mut self, id: u32) {
        let mut moved = Vec::new();
        for (t, n) in self.exhibits.boundary(self, id) {
            let Some((n, d)) = n.zip(n.and_then(|n| self.side_to(t, n))) else {
                continue;
            };
            let (inside, outside) = (self.edge(t, d), self.edge(n, (d + 4) % 8));
            let wall = outside.filter(|&w| self.shapes[w].is_some_and(|s| s.tank.wall));
            let (Some(wall), None) = (wall, inside) else {
                inside
                    .into_iter()
                    .chain(outside)
                    .for_each(|at| self.validate(Some(at)));
                continue;
            };
            let piece = match self.tank_at(n).is_some() {
                // A copy is made healthy (a gate's copy draws its wear interval only), then
                // validated, then the piece it was made from.
                true => {
                    let gate = self.is_door(wall);
                    let copy = self.copy_wall(wall, t, d);
                    if let Some(c) = copy {
                        self.draws.push(if gate {
                            Draw::Interval(c)
                        } else {
                            Draw::Heal(c)
                        });
                        self.draws.push(Draw::Start(c));
                        self.validate(outside);
                    }
                    copy
                }
                false => {
                    self.move_wall(wall, t, d);
                    self.validate(Some(wall));
                    self.zoo.entities[wall].common.as_ref().map(|c| c.id)
                }
            };
            moved.push((piece, n));
        }
        // The exhibit whose gate was moved in has none that its keepers can use.
        for (piece, n) in moved {
            let Some(region) = self
                .exhibits
                .region(n)
                .filter(|&r| r != GROUNDS && r != EXTERIOR)
            else {
                continue;
            };
            if piece.is_some() && self.exhibits.get(region).is_some_and(|e| e.gate == piece) {
                self.replace_gate(region);
            }
        }
    }

    /// The piece at `at` is validated: it draws a start time unless it is a gate.
    fn validate(&mut self, at: Option<usize>) {
        let Some(at) = at.filter(|&at| !self.is_door(at)) else {
            return;
        };
        let id = self.zoo.entities[at].common.as_ref().map(|c| c.id);
        self.draws.extend(id.map(Draw::Start));
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
        self.changed(None);
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
                    open: false,
                }),
                ..Default::default()
            }),
        };
        self.zoo.id_counter = id;
        self.zoo.entities.push(copy);
        self.shapes.push(self.shapes[wall]);
        self.record(Action::Copied { id });
        self.changed(None);
        Some(id)
    }

    /// Gives exhibit `e` a new gate after its old one moved to a tank (3.9): the first pair of its
    /// boundary, in its boundary order, whose tile outside is in the zoo and not in a tank nor in
    /// an exhibit that opens into `e`, and whose line holds one piece that is not a zoo wall.
    /// Without such a pair the old record stays.
    fn replace_gate(&mut self, e: u32) {
        for (i, out) in self.exhibits.boundary(self, e) {
            let Some(out) = out else { continue };
            let line: Vec<usize> = self.line((i, out)).into_iter().flatten().collect();
            let free =
                (self.exhibits.region(out)).is_some_and(|r| !self.opens_into(&self.exhibits, r, e));
            if self.in_zoo(out)
                && self.tank_at(out).is_none()
                && free
                && let [piece] = line[..]
                && self.shapes[piece].is_some_and(|s| s.fence && !s.wall)
                && self.make_door(piece)
            {
                if let Some(old) = (self.exhibits.get(e))
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
        }
    }

    /// The water values of a new tank `id` from `tanks.cfg` (3.9 creation step 3), its wall height
    /// by the kind it turned out to be. A new tank starts at level 0 and is filled when the file
    /// says so; the water rises with the simulation steps (3.10).
    fn fill_values(&mut self, id: u32, inherited: Option<Tank>) {
        let c = self.tank_config;
        let Some(tank) = self.exhibits.get_mut(id).and_then(|e| e.tank.as_mut()) else {
            return;
        };
        let k = usize::from(tank.show);
        let new = Tank {
            height: c.height[k],
            level: c.level,
            filled: false,
            salt: c.salt,
            target: c.salt,
            purity: c.purity,
            ..*tank
        };
        *tank = new;
        // Both kinds start from the creation's values: filled when the file says so and nothing in
        // the tank refuses it. A part cut off a tank then gets the water of that tank: its height
        // (through the wall rules), salinity, purity and level cut to the full level, and it is
        // drained when the tank was not filled (an animal in it refuses that).
        if c.filled {
            self.fill(id);
        }
        if let Some(old) = inherited {
            self.set_height(id, old.height);
            self.edit_tank(id, |t, c| {
                (t.salt, t.target, t.purity) = (old.salt, old.target, old.purity);
                t.level = old.level.min(c.top(t.height));
            });
            if !old.filled {
                self.drain(id);
            }
            self.edit_tank(id, |t, c| t.level = old.level.min(c.top(t.height)));
        }
        self.events.push(Event::Tank(id));
        self.sync_tanks();
    }

    /// Makes tank `id` (3.9 creation): the ground, the walls, the values, and then what the tank
    /// does not allow: the player is asked first when something stands in it. Not while an undo
    /// is replayed or a map is read, which keep the ground and the walls as they are.
    pub(super) fn make_tank(&mut self, id: u32, inherited: Option<Tank>, physical: bool) {
        let owed = self.draws.len();
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
            self.decide(&changed, true);
        }
        self.fill_values(id, inherited);
        // The part cut off a tank updates its wall information once its water values are set
        // (map drawing 9.7.8 point 3.4); a new tank draws nothing (its level is 0).
        if physical && inherited.is_some() {
            self.push_set_up(Draw::Walls(id));
        }
        let asks = physical && inherited.is_none() && !self.refused(id).is_empty();
        // Nothing to ask about: the tank is filled again, whatever the file says (3.10).
        if physical && inherited.is_none() && !asks {
            self.fill(id);
        }
        // A new tank filled at once (the game is paused) sets no ripples up: its purity is not
        // written yet when the level is (map drawing 9.7.8 open point a).
        if physical && inherited.is_none() {
            let made = self.draws.split_off(owed);
            let before = made.len();
            self.draws
                .extend(made.into_iter().filter(|&d| d != Draw::Ripples(id)));
            if self.draws.len() - owed != before {
                self.bare.push(id);
            }
        }
        let event = match asks {
            true => Event::Objects(id),
            false => Event::New { id, ask: physical },
        };
        self.events.push(event);
    }

    /// The entities (ids) in tank `id` that it does not allow (3.9): a unit whose type is not at
    /// home under water, a scenery object that neither lives under water nor on the surface,
    /// and in a show tank also anything that is not for shows; what is transient is left out.
    pub(crate) fn refused(&self, id: u32) -> Vec<u32> {
        self.unfit(id, self.tank(id).is_some_and(|t| t.show))
    }

    /// The same without the rule of show tanks when `show` is false: what the water of tank `id`
    /// does not allow (3.10).
    pub(super) fn unfit(&self, id: u32, show: bool) -> Vec<u32> {
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

    /// The wall pieces (entity indexes) of tank `id`, each once (4.7: the pieces on the lines of
    /// its boundary, on either slot).
    pub(crate) fn wall_pieces(&self, id: u32) -> Vec<usize> {
        let mut found: Vec<usize> = Vec::new();
        for (t, n) in self.exhibits.boundary(self, id) {
            for piece in n
                .map_or([None; 2], |n| self.line((t, n)))
                .into_iter()
                .flatten()
            {
                let wall = self.shapes[piece].is_some_and(|s| s.tank.wall);
                if wall && !found.contains(&piece) {
                    found.push(piece);
                }
            }
        }
        found
    }

    /// The tanks that show ripples against their walls (map drawing 9.7.8): some water and a clean
    /// purity. Each with the ids of the tank walls held in an edge slot of a tile of the tank, the
    /// tiles in list order and the slots 0, 2, 4, 6: one ripple each, whose tank the wall is
    /// does not matter.
    pub(crate) fn rippled_tanks(&self) -> Vec<(u32, Vec<u32>)> {
        let murky = self.tank_config.murky;
        (self.exhibits.list().iter())
            .filter(|e| e.tank.as_ref().is_some_and(|t| clean(t, murky)))
            .map(|e| (e.id, self.walls_over(&e.order)))
            .collect()
    }

    /// The ids of the tank walls held in an edge slot of the `tiles`, in list then slot order.
    pub(super) fn walls_over(&self, tiles: &[usize]) -> Vec<u32> {
        let slots = tiles.iter().flat_map(|&t| [0, 2, 4, 6].map(|d| (t, d)));
        let walls = slots.filter_map(|(t, d)| self.edge(t, d));
        let walls = walls.filter(|&p| self.shape(p).is_some_and(|s| s.tank.wall));
        let ids = walls.filter_map(|p| self.zoo.entities[p].common.as_ref().map(|c| c.id));
        ids.collect()
    }

    /// The tile outside the first pair of the boundary of exhibit `id`, where an animal or a
    /// staff member that has to leave is put.
    pub(crate) fn outside(&self, id: u32) -> Option<usize> {
        self.exhibits.boundary(self, id).first()?.1
    }

    /// Clears tank `id` of what its water does not allow, without a question (3.10 merge): a
    /// unit that moves goes to the tile outside, a guest stays, anything else is removed.
    pub(super) fn clear_refused(&mut self, id: u32) {
        let outside = self.outside(id);
        for e in self.unfit(id, false) {
            let Some(at) = self.find(e) else { continue };
            let unit = self.shapes[at].is_some_and(|s| s.kind == Kind::Unit);
            match (unit, outside) {
                (true, Some(t)) if self.zoo.entities[at].class != "guests" => self.move_unit(e, t),
                (true, _) => {}
                _ => _ = self.remove(e),
            }
        }
    }

    /// Puts the unit with this id in the middle of tile `t`.
    pub(crate) fn move_unit(&mut self, id: u32, t: usize) {
        let width = self.zoo.width as usize;
        let (x, y) = ((t % width) as i32 * 64 + 32, (t / width) as i32 * 64 + 32);
        let z = ground_height(&self.zoo.tiles[t], 32, 32);
        if let Some(c) = self
            .find(id)
            .and_then(|at| self.zoo.entities[at].common.as_mut())
        {
            (c.x, c.y, c.z) = (x, y, z);
            self.changed(None);
        }
    }

    /// The player said Yes to the question about the objects of tank `id`, which have been
    /// dealt with: it goes on to its name.
    pub(crate) fn accept_tank(&mut self, id: u32) {
        let owed: Vec<u32> = (self.owed.iter())
            .filter(|o| o.0 == id)
            .map(|o| o.1)
            .collect();
        self.owed.retain(|o| o.0 != id);
        self.resize_areas(&[], &owed);
        self.fill(id);
        self.events.push(Event::New { id, ask: true });
    }

    /// The player said No: the operation that made tank `id` is undone, its number given back.
    pub(crate) fn decline_tank(&mut self, id: u32) -> Undone {
        self.waiting.retain(|w| w.0 != id);
        self.owed.retain(|o| o.0 != id);
        let done = self.undo();
        self.exhibits.counter = self.exhibits.counter.saturating_sub(1).max(1);
        done
    }
}
