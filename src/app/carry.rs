//! Picking up, carrying and putting down animals, guests and staff by hand (unit placing 8). The
//! rules are in [`crate::place::unit`], the update that stops while a unit is carried in
//! [`crate::unit`].

use super::*;
use crate::entity::EntityType;
use crate::exhibit::{EXTERIOR, GROUNDS};
use crate::unit::Kind as UnitKind;

/// Tells the player why a place was refused: the stored message in the message line
/// (`tool message N`), or `tool refused` without one.
pub(super) fn refusal(ui: &mut Interface, lines: &mut Vec<String>, message: Option<i32>) {
    match message {
        Some(id) => {
            ui.message(&to_cp1252(&ui.string(id)));
            lines.push(format!("tool message {id}"));
        }
        None => lines.push("tool refused".into()),
    }
}

impl App<'_> {
    /// A release on the map with no tool (8.1): a unit under the pointer that is not the selected
    /// one becomes the selected one (`tool select ID`); the selected unit again is picked up
    /// (`world carry ID`), and follows the pointer as a pending unit, free of charge. Anything
    /// else under the pointer, or nothing, clears the selection. Returns whether a unit was
    /// under the pointer; the other things (a gate) are [`App::select`]'s.
    pub(super) fn pick_or_select(&mut self) -> bool {
        let (Some(game), Some(ui)) = (&mut self.game, &self.ui) else {
            return false;
        };
        let world = &game.world;
        let hit = (game.staged)
            .pick(game.camera.view_point(ui.pointer()), 0, |id| {
                world.find(id).is_some()
            })
            .filter(|&id| world.units.get(id).is_some());
        let Some(id) = hit else {
            game.selected = None;
            return false;
        };
        if game.selected != Some(id) {
            game.selected = Some(id);
            self.tool_lines.push(format!("tool select {id}"));
            self.ui_sounds.push("place");
            return true;
        }
        self.carry_up(id);
        true
    }

    /// Picks the unit up (8.2): it follows the pointer as a pending unit, free of charge, and
    /// nothing is selected (`world carry ID`).
    pub(super) fn carry_up(&mut self, id: u32) {
        let Some(game) = &mut self.game else { return };
        let cat = self.renderer.catalogue();
        if let Some(carry) = place::unit::pick_up(&mut game.world, cat, id, game.camera.rotation) {
            game.selected = None;
            game.carry = Some(carry);
            self.world_lines.push(format!("carry {id}"));
            self.ui_sounds.push("place");
            self.aim(false);
        }
    }

    /// A release on the map with a unit carried (8.4): a valid place puts it down there, facing
    /// as the pending unit does, selected again, and logs `world drop ID X,Y`, with `home N` for an
    /// animal that stands in an exhibit; a place that is not valid keeps it carried and shows its
    /// message, as a placement does.
    pub(super) fn drop_carried(&mut self) {
        let (Some(game), Some(ui)) = (&mut self.game, &mut self.ui) else {
            return;
        };
        let (Some(carry), Some(p)) = (game.carry.clone(), game.pending.clone()) else {
            return;
        };
        if p.verdict == place::Verdict::Invalid {
            self.ui_sounds.push("noplace");
            refusal(ui, &mut self.tool_lines, p.message);
            return;
        }
        let ty = &self.renderer.catalogue().types[carry.ty];
        let done = place::unit::put_down(&mut game.world, carry.id, (p.x, p.y, p.z), p.facing);
        game.carry = None;
        if done {
            game.selected = Some(carry.id);
            let line = Self::landing("drop", &game.world, ty, carry.id, p.tile);
            self.world_lines.push(line);
            self.ui_sounds.push("place");
        }
        self.aim(false);
    }

    /// Ends the move without any check (8.6): the unit is put back where it stood, facing as it
    /// did, and selected again, `world putback ID X,Y`.
    pub(super) fn put_back(&mut self) {
        let Some(game) = &mut self.game else { return };
        let Some(carry) = game.carry.take() else {
            return;
        };
        let ty = &self.renderer.catalogue().types[carry.ty];
        if place::unit::put_down(&mut game.world, carry.id, carry.from, carry.facing) {
            game.selected = Some(carry.id);
            let tile = (carry.from.0.div_euclid(64), carry.from.1.div_euclid(64));
            let line = Self::landing("putback", &game.world, ty, carry.id, tile);
            self.world_lines.push(line);
        }
        self.aim(false);
    }

    /// The log line of a unit put down on `tile`: `WORD ID X,Y`, and for an animal in an exhibit
    /// that is not a show tank `home N`, the exhibit it takes as its home (7.3).
    fn landing(word: &str, world: &World, ty: &EntityType, id: u32, tile: (i32, i32)) -> String {
        let mut line = format!("{word} {id} {},{}", tile.0, tile.1);
        let region = world
            .tile_index(tile.0, tile.1)
            .and_then(|t| world.exhibits().region(t))
            .filter(|&r| r != GROUNDS && r != EXTERIOR);
        let home = region.filter(|&r| {
            UnitKind::of_class(ty.class) == Some(UnitKind::Animal)
                && !world.tank(r).is_some_and(|t| t.show)
        });
        if let Some(r) = home {
            line.push_str(&format!(" home {r}"));
        }
        line
    }
}
