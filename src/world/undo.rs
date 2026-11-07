//! The undo list of the last operation: what an operation did, where an entity stood, and taking
//! the operations back (placing 8).

use super::*;

/// A value of a tank that the removal of one of its walls records.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) enum TankValue {
    Purity(u32),
    Salt(bool),
    Filled(bool),
    Height(u32),
    /// The elevation of the tank's ground.
    Level(i32),
}

/// What an operation did, to undo it. The prices are what the action moved in the cash: an undo
/// gives them back (placing 8).
#[derive(Debug, Clone, PartialEq)]
pub(super) enum Action {
    /// An entity was added: its id and the price paid for it.
    Placed { id: u32, price: f32 },
    /// A wall the wall step built for nothing: recorded as a placement, so the undo gives back
    /// the full price of its type (placing 8, kind 1).
    Copied { id: u32 },
    /// A value of the tank on this tile before one of its walls was removed (placing 8, kinds 4
    /// to 8).
    Tank { tile: usize, value: TankValue },
    /// An entity was removed from this position of the list, and the refund it gave.
    Removed {
        at: usize,
        entity: Entity,
        shape: Option<Shape>,
        price: f32,
    },
    /// The entity `id` took the place of `old` in the list (a path of another type, a fence piece
    /// laid on a held slot) at this price; `exhibit` is the exhibit that had `old` as its gate.
    Replaced {
        id: u32,
        old: Entity,
        shape: Option<Shape>,
        exhibit: Option<u32>,
        price: f32,
    },
    /// Tiles had this state (their index first), and the price of the change.
    Tiles {
        tiles: Vec<(usize, Tile)>,
        price: f32,
    },
    /// A fence piece stood here: its id, position `(x, y, z)` and rotation.
    Moved {
        id: u32,
        at: (i32, i32, i32),
        rotation: u32,
    },
    /// The exhibit on this tile had this name and this gate (the tile and side of its piece)
    /// before a fence cut it in two or opened it (placing 8, kind 11).
    Exhibit {
        tile: usize,
        name: String,
        gate: Option<(usize, usize)>,
    },
}

/// Where an entity stands, as the log tells it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Spot {
    /// `class/type/subtype`.
    pub name: String,
    pub tile: (i32, i32),
    /// The side of a fence.
    pub side: Option<u32>,
    pub id: u32,
}

/// One action of the undo list that was taken back, with the price it moved.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum Step {
    /// A placed object went; its price comes back (the construction spending goes down).
    Removed(Spot, f32),
    /// A wall the wall step built went: the full price of its type comes back, the entity tells
    /// the type.
    Copied(Spot, Box<Entity>),
    /// A removed object came back; its refund is taken back.
    Back(Spot, f32),
    /// A piece got its old type back; the price of the change comes back.
    Retyped(Spot, f32),
    /// This many tiles got their old state back; the price of the change comes back.
    Tiles(usize, f32),
    /// A removed object that could not be put back stays sold with its refund (one of several).
    Skipped(Spot),
}

/// What an undo did.
#[derive(Debug, Default, Clone, PartialEq)]
pub(crate) struct Undone {
    /// The actions taken back, the last recorded first.
    pub steps: Vec<Step>,
    /// A removed object that cannot be put back because something stands in its place: the undo
    /// stopped there and the rest of the list is kept.
    pub blocked: Option<Entity>,
    /// The names an undo gave back to exhibits, for the name hook (cheats 2.2).
    pub names: Vec<String>,
}

impl Undone {
    /// Whether nothing was taken back.
    pub(crate) fn is_empty(&self) -> bool {
        self.steps.is_empty() && self.blocked.is_none() && self.names.is_empty()
    }
}

impl World {
    /// Tells the price of the action recorded last: what an undo gives back (a purchase) or takes
    /// back (a refund). A call after an edit that records nothing changes nothing.
    pub(crate) fn priced(&mut self, amount: f32) {
        if let Some(
            Action::Placed { price, .. }
            | Action::Removed { price, .. }
            | Action::Replaced { price, .. }
            | Action::Tiles { price, .. },
        ) = self.undo.last_mut()
        {
            *price = amount;
        }
    }

    /// Runs an edit that the undo list does not record: the simulation's own removals, and a test
    /// that needs something in the way.
    pub(crate) fn unrecorded<T>(&mut self, edit: impl FnOnce(&mut World) -> T) -> T {
        let (undo, fresh) = (std::mem::take(&mut self.undo), self.fresh);
        let result = edit(self);
        (self.undo, self.fresh) = (undo, fresh);
        result
    }

    /// Whether the undo list holds the placement of the entity `id`.
    pub(crate) fn undoes(&self, id: u32) -> bool {
        self.undo
            .iter()
            .any(|a| matches!(a, Action::Placed { id: i, .. } if *i == id))
    }

    /// Drops the actions of the undo list that concern the entity `id`, which the simulation
    /// removed: an undo must neither bring it back nor remove it.
    pub(crate) fn forget_undo(&mut self, id: u32) {
        self.undo.retain(|a| match a {
            Action::Placed { id: i, .. }
            | Action::Copied { id: i }
            | Action::Replaced { id: i, .. }
            | Action::Moved { id: i, .. } => *i != id,
            _ => true,
        });
    }

    /// Whether an undo would do anything: the Undo button is enabled.
    pub(crate) fn can_undo(&self) -> bool {
        !self.undo.is_empty()
    }

    /// Forgets the undo list.
    pub(crate) fn clear_undo(&mut self) {
        self.undo.clear();
    }

    /// An accepted terrain session: the undo list holds only this one action, which puts the
    /// tiles back as they were and gives the price back (terrain editing 8.4, 8.7).
    pub(crate) fn accepted(&mut self, tiles: Vec<(usize, Tile)>, price: f32) {
        self.undo.clear();
        self.undo.push(Action::Tiles { tiles, price });
    }

    /// Takes back the operations of the last press, last first (placing 8): a placed object goes,
    /// a removed one comes back in its place of the list, a piece laid on another gets its old
    /// type back, tiles get their old state, an exhibit its name and gate. The exhibits change as
    /// for a normal edit, quietly (no name is asked, no gate chosen, the counter stays).
    ///
    /// A removed object that cannot be put back is skipped when the list holds several; else the
    /// undo stops there and the actions not undone yet stay in the list (`blocked`). What was
    /// undone is told in `steps`, with the prices for the caller to move in the cash.
    pub(crate) fn undo(&mut self) -> Undone {
        let mut actions = std::mem::take(&mut self.undo);
        let several = actions
            .iter()
            .filter(|a| matches!(a, Action::Removed { .. }))
            .count()
            > 1;
        let mut done = Undone::default();
        let (mut stale, mut doors) = (false, Vec::new());
        while let Some(action) = actions.pop() {
            match action {
                Action::Placed { id, price } => {
                    if let Some(at) = self.find(id) {
                        let spot = self.spot(at);
                        self.zoo.entities.remove(at);
                        self.shapes.remove(at);
                        self.changed(Some(Shift::Removed(at)));
                        stale = true;
                        done.steps.push(Step::Removed(spot, price));
                    }
                }
                Action::Copied { id } => {
                    if let Some(at) = self.find(id) {
                        let spot = self.spot(at);
                        let entity = self.zoo.entities.remove(at);
                        self.shapes.remove(at);
                        self.changed(Some(Shift::Removed(at)));
                        stale = true;
                        done.steps.push(Step::Copied(spot, Box::new(entity)));
                    }
                }
                Action::Tank { tile, value } => {
                    if std::mem::take(&mut stale) {
                        self.settle(None, true);
                    }
                    self.restore_tank(tile, value);
                }
                Action::Removed {
                    at,
                    entity,
                    shape,
                    price,
                } => {
                    // It must find its place with the others standing: it is tried last, then
                    // moved to its old position in the list.
                    self.zoo.entities.push(entity.clone());
                    self.shapes.push(shape);
                    self.changed(None);
                    if !self.fits(self.zoo.entities.len() - 1) {
                        self.zoo.entities.pop();
                        self.shapes.pop();
                        self.changed(None);
                        if several {
                            done.steps.push(Step::Skipped(self.spot_of(&entity)));
                            continue;
                        }
                        done.blocked = Some(entity.clone());
                        actions.push(Action::Removed {
                            at,
                            entity,
                            shape,
                            price,
                        });
                        break;
                    }
                    let (entity, shape) = (self.zoo.entities.pop(), self.shapes.pop());
                    let at = at.min(self.zoo.entities.len());
                    if let (Some(entity), Some(shape)) = (entity, shape) {
                        self.zoo.entities.insert(at, entity);
                        self.shapes.insert(at, shape);
                    }
                    self.changed(Some(Shift::Inserted(at)));
                    stale = true;
                    if self.is_door(at) {
                        doors.extend(self.zoo.entities[at].common.as_ref().map(|c| c.id));
                    }
                    done.steps.push(Step::Back(self.spot(at), price));
                }
                Action::Replaced {
                    id,
                    old,
                    shape,
                    exhibit,
                    price,
                } => {
                    if let Some(at) = self.find(id) {
                        let old_id = old.common.as_ref().map_or(id, |c| c.id);
                        self.zoo.entities[at] = old;
                        self.shapes[at] = shape;
                        if let Some(e) = exhibit.and_then(|x| self.exhibits.get_mut(x)) {
                            e.gate = Some(old_id);
                        }
                        self.changed(None);
                        stale = true;
                        done.steps.push(Step::Retyped(self.spot(at), price));
                    }
                }
                Action::Moved { id, at, rotation } => {
                    let c = self
                        .find(id)
                        .and_then(|i| self.zoo.entities[i].common.as_mut());
                    if let Some(c) = c {
                        (c.x, c.y, c.z, c.rotation) = (at.0, at.1, at.2, rotation);
                        self.changed(None);
                        stale = true;
                    }
                }
                Action::Tiles { tiles, price } => {
                    for &(t, tile) in &tiles {
                        let moved = (tile.elevation, tile.shape)
                            != (self.zoo.tiles[t].elevation, self.zoo.tiles[t].shape);
                        self.zoo.tiles[t] = tile;
                        self.reshaped = true;
                        if moved {
                            self.reheight(t);
                        }
                        self.edges_around(t);
                    }
                    self.changed(None);
                    stale = true;
                    done.steps.push(Step::Tiles(tiles.len(), price));
                }
                Action::Exhibit { tile, name, gate } => {
                    if std::mem::take(&mut stale) {
                        self.settle(None, true);
                    }
                    done.names.extend(self.restore_exhibit(tile, name, gate));
                }
            }
        }
        if stale {
            self.settle(None, true);
        }
        // A piece that was a gate and is not the gate of any exhibit now is a fence again.
        for id in doors {
            let Some(at) = self.find(id) else { continue };
            if !self.exhibits.list().iter().any(|e| e.gate == Some(id)) {
                self.make_fence(at);
            }
        }
        self.undo = actions;
        self.fresh = false;
        self.dirty = true;
        done
    }

    /// Whether the entity at `at` found its place on the map (or has none to find).
    pub(super) fn fits(&self, at: usize) -> bool {
        (self.owner.iter().position(|&e| e == at)).is_none_or(|o| self.scene.is_placed(o))
    }

    /// Where the entity at `at` stands.
    pub(super) fn spot(&self, at: usize) -> Spot {
        let mut spot = self.spot_of(&self.zoo.entities[at]);
        if let Some((t, d)) = self.slot(at) {
            let width = self.zoo.width as usize;
            spot.tile = ((t % width) as i32, (t / width) as i32);
            spot.side = Some(d as u32);
        }
        spot
    }

    /// Where an entity stands by its own position (a fence by its side).
    pub(crate) fn spot_of(&self, e: &Entity) -> Spot {
        let c = e.common.as_ref();
        let fence = matches!(e.class.as_str(), "fences" | "tankwall");
        Spot {
            name: format!("{}/{}/{}", e.class, e.type_name, e.subtype),
            tile: c.map_or((0, 0), |c| (c.x.div_euclid(64), c.y.div_euclid(64))),
            side: c.filter(|_| fence).map(|c| c.rotation % 8),
            id: c.map_or(0, |c| c.id),
        }
    }

    /// The exhibits on the two tiles of a slot, with the id of each: the action that gives one
    /// back its name and gate (placing 8, kind 11).
    pub(super) fn keeping(&self, slot: Option<(usize, usize)>) -> Vec<(u32, Action)> {
        let Some((a, d)) = slot else {
            return Vec::new();
        };
        let mut found: Vec<(u32, Action)> = Vec::new();
        for t in [Some(a), self.neighbour(a, d)].into_iter().flatten() {
            let Some(e) = (self.exhibits.region(t)).and_then(|r| self.exhibits.get(r)) else {
                continue;
            };
            if found.iter().all(|(id, _)| *id != e.id) {
                found.push((
                    e.id,
                    Action::Exhibit {
                        tile: t,
                        name: e.name.clone(),
                        gate: self.gate_slot(e.id),
                    },
                ));
            }
        }
        found
    }

    /// Records the exhibits on the two sides of the fence piece on this slot, with their names and
    /// gates, before it goes, when its two tiles are in different regions. A removal records
    /// whether the line stays closed or opens; a replacement (`replaced`) only when no piece of
    /// the other tile holds the line too (placing 8, kind 11).
    pub(super) fn note_opening(&mut self, slot: Option<(usize, usize)>, replaced: bool) {
        let Some((a, d)) = slot else { return };
        let Some(b) = self.neighbour(a, d) else {
            return;
        };
        let held = self
            .edge(b, (d + 4) % 8)
            .is_some_and(|o| self.shapes[o].is_some_and(|s| s.fence));
        if (replaced && held) || self.exhibits.region(a) == self.exhibits.region(b) {
            return;
        }
        for (_, note) in self.keeping(slot) {
            self.record(note);
        }
    }

    /// Records the values of the tank whose own wall at position `at` is about to go: the purity,
    /// salinity, filled flag, wall height and base level, in this order (placing 8, kinds 4 to 8).
    pub(super) fn note_tank(&mut self, at: usize) {
        let Some((t, _)) = self.slot_of(at) else {
            return;
        };
        if !self.shapes[at].is_some_and(|s| s.tank.wall) {
            return;
        }
        let Some(id) = self.tank_at(t) else { return };
        let (Some(tank), Some(&first)) = (self.tank(id), self.tiles_of(id).first()) else {
            return;
        };
        let level = self.zoo.tiles[first].elevation;
        for value in [
            TankValue::Purity(tank.purity),
            TankValue::Salt(tank.salt),
            TankValue::Filled(tank.filled),
            TankValue::Height(tank.height),
            TankValue::Level(level),
        ] {
            self.record(Action::Tank { tile: t, value });
        }
    }

    /// Sets a value of the tank now on tile `tile`; none there, nothing happens.
    pub(super) fn restore_tank(&mut self, tile: usize, value: TankValue) {
        let Some(id) = self.tank_at(tile) else { return };
        if let TankValue::Level(level) = value {
            for t in self.tiles_of(id) {
                let at = self.zoo.tiles[t];
                if at.elevation != level {
                    self.put_tile(t, level, at.shape, at.terrain, false);
                }
            }
        }
        if let Some(tank) = self.exhibits.get_mut(id).and_then(|e| e.tank.as_mut()) {
            match value {
                TankValue::Purity(v) => tank.purity = v,
                TankValue::Salt(v) => tank.salt = v,
                TankValue::Height(_) | TankValue::Level(_) | TankValue::Filled(_) => {}
            }
        }
        // The filled state goes through the fill change, which makes the show links again.
        if let TankValue::Filled(v) = value {
            self.edit_tank(id, |t, _| t.filled = v);
        }
        // The height goes through the wall height change: the water is cut to the new full level.
        if let TankValue::Height(v) = value {
            self.set_height(id, v);
        }
        self.events.push(Event::Tank(id));
        self.sync_tanks();
    }

    /// Gives the exhibit on tile `tile` its name, and its gate on the line of the recorded slot
    /// (the piece there, on either tile's slot); the gate it had becomes a fence again. Gives back
    /// the name it set.
    pub(super) fn restore_exhibit(
        &mut self,
        tile: usize,
        name: String,
        gate: Option<(usize, usize)>,
    ) -> Option<String> {
        let id = self
            .exhibits
            .region(tile)
            .filter(|&r| r != GROUNDS && r != EXTERIOR)?;
        let piece = gate.and_then(|(t, d)| {
            let across = self.neighbour(t, d).and_then(|n| self.edge(n, (d + 4) % 8));
            let at = self.edge(t, d).or(across)?;
            self.shapes[at].is_some_and(|s| s.fence).then_some(at)
        });
        let current = self.exhibits.get(id).and_then(|e| e.gate);
        let new = piece
            .filter(|&at| self.make_door(at))
            .and_then(|at| self.zoo.entities[at].common.as_ref().map(|c| c.id));
        if let Some(old) = current
            .filter(|&g| Some(g) != new)
            .and_then(|g| self.find(g))
        {
            self.make_fence(old);
        }
        let named = self.exhibits.get_mut(id).map(|e| {
            e.name = name.clone();
            e.gate = new;
            name
        });
        self.dirty = true;
        if new.is_some() {
            self.events.push(Event::Gate(id));
        }
        named
    }
}
