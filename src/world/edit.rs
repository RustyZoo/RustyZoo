//! The edit primitives: adding, retyping, removing and swapping entities, and setting or painting
//! tiles.

use super::undo::Action;
use super::*;

impl World {
    /// The next press starts a new undo list: the first operation after it empties the old one.
    pub(crate) fn begin(&mut self) {
        self.fresh = true;
    }

    /// Adds an entity with a fresh id and returns the id; `None` for one without a body. The
    /// price paid is told with [`World::priced`].
    pub(crate) fn add(&mut self, entity: Entity, shape: Option<Shape>) -> Option<u32> {
        self.add_rated(entity, shape, true)
    }

    /// The same; `rated` is whether the exhibits it touches are marked for a new rating (the
    /// plain entity add of an animal's dung pile marks none).
    pub(crate) fn add_rated(
        &mut self,
        mut entity: Entity,
        shape: Option<Shape>,
        rated: bool,
    ) -> Option<u32> {
        let common = entity.common.as_mut()?;
        let id = self.zoo.id_counter.checked_add(1)?;
        (self.zoo.id_counter, common.id) = (id, id);
        self.zoo.entities.push(entity);
        self.shapes.push(shape);
        self.changed(None);
        let mut notes = Vec::new();
        let tiles = self.rated_tiles(self.zoo.entities.len() - 1);
        if shape.is_some_and(|s| s.fence) {
            let slot = self.slot_of(self.zoo.entities.len() - 1);
            let seen = self.events.len();
            let before = self.keeping(slot);
            self.settle(slot, false);
            notes = self.changed_by(before, seen);
        }
        if rated {
            tiles.into_iter().for_each(|t| self.mark_rating(t));
        }
        notes.into_iter().for_each(|note| self.record(note));
        self.record(Action::Placed { id, price: 0.0 });
        self.filter_added(self.zoo.entities.len() - 1);
        self.show_moved(self.zoo.entities.len() - 1, true);
        Some(id)
    }

    /// The exhibits that a fence piece cut in two or replaced by one of the other kind (a split
    /// or a morph: the events since `seen` tell it), with their names and gates as they were
    /// `before`: the actions that give them back, to record before the piece itself, so that the
    /// undo takes the piece away first (placing 8, kind 11).
    pub(super) fn changed_by(&self, before: Vec<(u32, Action)>, seen: usize) -> Vec<Action> {
        let hit = |id: u32| {
            self.events[seen..]
                .iter()
                .any(|e| matches!(e, Event::Split(s) | Event::Removed(s) if *s == id))
        };
        (before.into_iter())
            .filter(|(id, _)| hit(*id))
            .map(|(_, note)| note)
            .collect()
    }

    /// The unit at position `at` of the list takes another subtype of its type and the shape that
    /// goes with it (a young animal grows up). It does not join the show lists (shows 20).
    pub(crate) fn retype(&mut self, at: usize, subtype: &str, shape: Shape) {
        self.zoo.entities[at].subtype = subtype.into();
        self.shapes[at] = Some(shape);
        self.changed(None);
    }

    /// Removes the entity with this id; whether there was one. A fence piece that opens its line
    /// records the exhibits it opens, with their names and gates, first. A rubble piece takes the
    /// rest of its group with it, destroyed with no undo record, before itself so that a price told
    /// next belongs to it; it is recorded alone, its group emptied (cheats 4.6).
    pub(crate) fn remove(&mut self, id: u32) -> bool {
        let group = (self.find(id))
            .and_then(|at| self.zoo.entities[at].common.as_mut()?.rubble.as_mut())
            .map_or_else(Vec::new, |r| std::mem::take(&mut r.group));
        for g in group.into_iter().filter(|&g| g != id) {
            // A saved group may name anything: only rubble goes.
            if self
                .find(g)
                .is_some_and(|at| self.zoo.entities[at].class == "rubble")
            {
                self.remove_one(g, false);
            }
        }
        self.remove_one(id, true)
    }

    /// Removes the entity with this id alone; `undoable` records it for the undo.
    pub(super) fn remove_one(&mut self, id: u32, undoable: bool) -> bool {
        let Some(at) = self.find(id) else {
            return false;
        };
        let slot = self.slot_of(at);
        let fence = self.shapes[at].is_some_and(|s| s.fence);
        if fence {
            self.note_tank(at);
            self.note_opening(slot, false);
        }
        let rated = self.rated_tiles(at);
        if !self.is_carried(id) {
            self.show_moved(at, false);
        }
        // A fence piece removed sets the escape check flag on both sides (habitat rating 5.8).
        if fence {
            rated.iter().for_each(|&t| self.escape_check(t));
        }
        let entity = self.zoo.entities.remove(at);
        let shape = self.shapes.remove(at);
        if let Some(c) = &entity.common {
            self.filter_view.remove(&c.id);
        }
        self.changed(Some(Shift::Removed(at)));
        if fence {
            self.settle(slot, false);
        }
        rated.into_iter().for_each(|t| self.mark_rating(t));
        if undoable {
            self.record(Action::Removed {
                at,
                entity,
                shape,
                price: 0.0,
            });
        }
        true
    }

    /// Puts back an entity that was taken out for the player to hold (guest buildings 11.1): it
    /// keeps its id and goes to position `at` of the list again, with nothing recorded for the
    /// undo. The exhibit it stands in is marked for a new rating.
    pub(crate) fn put_back(&mut self, at: usize, entity: Entity, shape: Option<Shape>) {
        let at = at.min(self.zoo.entities.len());
        self.zoo.entities.insert(at, entity);
        self.shapes.insert(at, shape);
        self.changed(Some(Shift::Inserted(at)));
        self.rated_tiles(at)
            .into_iter()
            .for_each(|t| self.mark_rating(t));
    }

    /// Whether `entity` has the type of the one at `at` in the list.
    pub(crate) fn same_type(&self, at: usize, entity: &Entity) -> bool {
        let old = &self.zoo.entities[at];
        (&old.class, &old.type_name, &old.subtype)
            == (&entity.class, &entity.type_name, &entity.subtype)
    }

    /// Puts a new entity in place of the one with this id, as one change, in the same place of
    /// the entity list. An entity of the same type, and a path of another type (placing 5.3,
    /// 6.3), keeps the old one's id; a fence piece of another type gets a fresh id. An exhibit
    /// fence replaced by another stays closed and keeps its exhibit, and a gate stays the gate
    /// when the new type has one. Returns the id of the new entity.
    pub(crate) fn swap(
        &mut self,
        id: u32,
        mut entity: Entity,
        shape: Option<Shape>,
    ) -> Option<u32> {
        let at = self.find(id)?;
        let slot = self.slot_of(at);
        let was = self.shapes[at];
        let door = self.is_door(at);
        let same = self.same_type(at, &entity);
        let path = was.is_some_and(|s| s.kind == Kind::Path);
        let keep = same || path;
        let new = match keep {
            true => id,
            false => self.zoo.id_counter.checked_add(1)?,
        };
        let common = entity.common.as_mut()?;
        (self.zoo.id_counter, common.id) = (self.zoo.id_counter.max(new), new);
        let gate = door && shape.is_some_and(|s| s.gated && !s.wall);
        if gate {
            entity.subtype = "g".into();
        }
        let fence = (was.is_some_and(|s| s.fence), shape.is_some_and(|s| s.fence));
        if fence == (true, false) {
            self.note_opening(slot, true);
        }
        let before = self
            .keeping(slot)
            .into_iter()
            .filter(|_| fence == (true, true));
        let before: Vec<_> = before.collect();
        let seen = self.events.len();
        let exhibit = (self.exhibits.list().iter())
            .find(|e| e.gate == Some(id))
            .map(|e| e.id);
        let old = std::mem::replace(&mut self.zoo.entities[at], entity);
        self.shapes[at] = shape;
        if let Some(e) = exhibit.and_then(|x| self.exhibits.get_mut(x)) {
            e.gate = gate.then_some(new);
        }
        self.changed(None);
        if fence.0 || fence.1 {
            self.settle(slot, false);
        }
        let rated = self.rated_tiles(at);
        rated.into_iter().for_each(|t| self.mark_rating(t));
        // A fence replaced by one of another kind (a morph) records the exhibits it changes
        // (kind 11); a path laid on a path of the same type records nothing (placing 8).
        self.changed_by(before, seen)
            .into_iter()
            .for_each(|note| self.record(note));
        if !(same && path) {
            self.record(Action::Replaced {
                id: new,
                old,
                shape: was,
                exhibit,
                price: 0.0,
            });
        }
        Some(new)
    }

    /// Sets the elevation, shape and terrain type of tile (`tx`, `ty`) and writes the edge states
    /// of it and of its four neighbours again (map drawing 3.5); whether the tile is on the map.
    pub(crate) fn set_tile(
        &mut self,
        tx: i32,
        ty: i32,
        elevation: i32,
        shape: u8,
        terrain: u8,
    ) -> bool {
        let Some(t) = self.tile_index(tx, ty) else {
            return false;
        };
        self.put_tile(t, elevation, shape, terrain, true);
        true
    }

    /// Paints tile `(tx, ty)` with terrain `terrain` as the bare cell paint of trampled ground
    /// (animals 4.5): no undo entry, no terrain change pair, so no exhibit is marked.
    pub(crate) fn paint_cell(&mut self, tx: i32, ty: i32, terrain: u8) {
        if let Some(t) = self.tile_index(tx, ty) {
            self.zoo.tiles[t].terrain = terrain;
            self.edges_around(t);
            self.changed(None);
        }
    }

    /// The last two actions, when both are tiles with no price, become one: the levelling and the
    /// floor of a tank are one terrain action (placing 8, kind 3). A tile in both keeps its
    /// first state.
    pub(super) fn join_tiles(&mut self) {
        let free = |a: &Action| matches!(a, Action::Tiles { price, .. } if *price == 0.0);
        let n = self.undo.len();
        if n < 2 || !free(&self.undo[n - 1]) || !free(&self.undo[n - 2]) {
            return;
        }
        let Some(Action::Tiles { tiles, .. }) = self.undo.pop() else {
            return;
        };
        if let Some(Action::Tiles { tiles: first, .. }) = self.undo.last_mut() {
            for tile in tiles {
                if first.iter().all(|f| f.0 != tile.0) {
                    first.push(tile);
                }
            }
        }
    }

    /// Sets tile `t`; `record` puts it in the undo list.
    pub(super) fn put_tile(
        &mut self,
        t: usize,
        elevation: i32,
        shape: u8,
        terrain: u8,
        record: bool,
    ) {
        let old = self.zoo.tiles[t];
        if record {
            self.record(Action::Tiles {
                tiles: vec![(t, old)],
                price: 0.0,
            });
        }
        let tile = &mut self.zoo.tiles[t];
        (tile.elevation, tile.shape, tile.terrain) = (elevation, shape, terrain);
        if (old.elevation, old.shape) != (elevation, shape) {
            self.reheight(t);
        }
        self.edges_around(t);
        self.changed(None);
        self.mark_rating(t);
        self.escape_check(t);
    }
}
