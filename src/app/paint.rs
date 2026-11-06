//! The picture of a pass: the map staged again after an edit, and the window painted when its
//! look changed.

use super::*;

impl App<'_> {
    /// Paints the window of the view if it differs from the last frame: the camera moved or the
    /// interface changed.
    pub(super) fn paint(&mut self) -> bool {
        let look = self.look();
        let moved = self.painted != look;
        let stale = std::mem::take(&mut self.stale);
        if !moved && stale.is_none() && !self.ui.as_ref().is_some_and(Interface::dirty) {
            return false;
        }
        // The one place that draws the map; the zoom of the camera applies here.
        let target = match self.ui {
            Some(_) => &mut self.map,
            None => &mut self.frame,
        };
        if let Some(game) = &self.game
            && (moved || stale.is_some())
        {
            let (x, y) = game.camera.scroll;
            match moved {
                true => game.staged.draw(target, x, y, game.camera.zoom, game.grid),
                false => game.staged.redraw(
                    target,
                    x,
                    y,
                    game.camera.zoom,
                    game.grid,
                    &stale.unwrap_or_default(),
                ),
            }
            self.painted = look;
        }
        if let Some(ui) = &mut self.ui {
            let map = &self.map;
            let game = self.game.is_some();
            ui.paint(&mut self.frame, &mut |frame, rect| {
                if game {
                    copy_rect(frame, map, rect);
                }
            });
            // The cost of the pending object or of the brush, next to the pointer (placing 4.5,
            // terrain editing 7.3).
            if let Some(amount) = self.game.as_ref().and_then(Game::cost) {
                let text = place::cost_text(ui.string(SYMBOL), amount);
                ui.pointer_text(&mut self.frame, &to_cp1252(&text), COST_OFFSET);
            }
            if self.timers.speed.developer() {
                ui.corner_text(&mut self.frame, b"developer mode");
            }
        }
        true
    }

    /// Stages the map again when an edit changed the world since the last draw: the picture is
    /// the final state at once. A map that cannot be staged keeps the old picture.
    pub(super) fn restage(&mut self) {
        let dirty = match &mut self.game {
            Some(game) => game.world.take_dirty(),
            None => return,
        };
        // An edit may change what the place under the pointer is worth.
        if dirty {
            self.aim(false);
        }
        playing!(self, game);
        // The pending object and the terrain highlights are transient items: the picture is made
        // again when they changed.
        let mut want = match game.placing.run.is_empty() {
            true => game
                .placing
                .pending
                .as_ref()
                .map_or_else(Extra::default, |p| p.extra(&game.placing.tints)),
            false => place::extra_of(&game.placing.run, &game.placing.tints),
        };
        want.marks.clone_from(&game.marks);
        want.shown = game.shown();
        want.hidden.extend(game.carry.as_ref().map(|c| c.id));
        want.selected = game.selected;
        let animal = game
            .selected
            .and_then(|id| match &game.world.units.get(id)?.part {
                crate::unit::Part::Animal(a) => Some((a.egg, a.crated)),
                _ => None,
            });
        want.form = match animal {
            Some((true, _)) => Form::Egg,
            Some((_, true)) => Form::Crate,
            _ => Form::Plain,
        };
        want.faces.clone_from(&game.world.units.reactions);
        want.filters = game.world.filter_looks();
        want.idle = Frames(self.sim.idle_frames());
        want.marks.extend(game.selection);
        game.dozer_extra(&mut want);
        want.marks.extend(game.assign_marks());
        if !dirty && want == game.extra {
            // Only the frames of the idle animations moved: no new staging.
            let size = (self.frame.width, self.frame.height);
            let at = (game.camera.scroll, game.camera.zoom);
            let rects = game.staged.reframe(&want.idle, size, at.0, at.1);
            let drawn = self.painted.is_some()
                && self.painted == Some((at.0, at.1, game.camera.rotation, game.grid));
            match (drawn, rects) {
                (true, Some(rects)) => self.stale.get_or_insert_default().extend(rects),
                (true, None) => (self.painted, self.stale) = (None, None),
                _ => {}
            }
            game.extra = want;
            return;
        }
        match self
            .renderer
            .stage_with(&game.world.zoo, game.camera.rotation, false, &want)
        {
            Ok(staged) => {
                let old = std::mem::replace(&mut game.staged, staged);
                // The map drawn for the old staging is mended where the new one differs.
                let (x, y) = game.camera.scroll;
                let size = (self.frame.width, self.frame.height);
                let drawn = self.painted.is_some()
                    && self.painted
                        == Some((
                            game.camera.scroll,
                            game.camera.zoom,
                            game.camera.rotation,
                            game.grid,
                        ));
                let changes = drawn
                    .then(|| {
                        game.staged
                            .changes(&old, size, (x, y), game.camera.zoom, game.grid)
                    })
                    .flatten();
                match changes {
                    Some(rects) => self.stale.get_or_insert_default().extend(rects),
                    None => (self.painted, self.stale) = (None, None),
                }
                if dirty {
                    self.lines.world.push("stage".to_string());
                }
            }
            Err(why) => self.lines.world.push(format!("stage failed: {why}")),
        }
        game.extra = want;
    }

    /// The camera as it looks now; `None` without a game.
    pub(super) fn look(&self) -> Option<Look> {
        self.game
            .as_ref()
            .map(|g| (g.camera.scroll, g.camera.zoom, g.camera.rotation, g.grid))
    }
}

/// Copies the rectangle (left, top, width, height) of `from` to the same place of `to`, clipped.
fn copy_rect(to: &mut Framebuffer, from: &Framebuffer, (x, y, w, h): (i32, i32, i32, i32)) {
    let left = x.max(0) as usize;
    let right =
        (i64::from(x) + i64::from(w)).clamp(0, i64::from(to.width.min(from.width))) as usize;
    for row in
        y.max(0)..(i64::from(y) + i64::from(h)).min(i64::from(to.height.min(from.height))) as i32
    {
        let range = (row as usize * to.width as usize + left) * 4
            ..(row as usize * to.width as usize + right) * 4;
        if range.start < range.end {
            to.pixels[range.clone()].copy_from_slice(&from.pixels[range]);
        }
    }
}

/// FNV-1a over the pixels of a frame.
pub fn frame_hash(frame: &Framebuffer) -> u64 {
    frame.pixels.iter().fold(0xcbf2_9ce4_8422_2325, |hash, &b| {
        (hash ^ u64::from(b)).wrapping_mul(0x0100_0000_01b3)
    })
}

/// The FNV-1a hashes of up to four frames at once, each as [`frame_hash`] gives it: the four sums
/// are independent, so the processor works on them side by side, about four times as fast as one
/// after the other. Frames of different sizes are hashed one by one.
pub fn frame_hashes(frames: &[&Framebuffer]) -> Vec<u64> {
    let size = frames.first().map_or(0, |f| f.pixels.len());
    if frames.len() > 4 || frames.iter().any(|f| f.pixels.len() != size) {
        return frames.iter().map(|f| frame_hash(f)).collect();
    }
    // Missing lanes repeat the first frame.
    let lane = |k: usize| {
        frames
            .get(k)
            .or(frames.first())
            .map_or(&[][..], |f| &f.pixels)
    };
    let data = [lane(0), lane(1), lane(2), lane(3)];
    let mut hash = [0xcbf2_9ce4_8422_2325u64; 4];
    for i in 0..size {
        for (h, d) in hash.iter_mut().zip(data) {
            *h = (*h ^ u64::from(d[i])).wrapping_mul(0x0100_0000_01b3);
        }
    }
    hash[..frames.len()].to_vec()
}
