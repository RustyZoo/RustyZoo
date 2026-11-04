//! The sprite player of a portal wall (shows 32.6): the opening and closing animation. Only the
//! open byte is saved; the player is not, so a loaded portal shows the rest picture of its byte.
//!
//! Setting the base sequence clears the player at once (point 2), so a new direction shows its
//! first step before any advance; a flagged portal ends closed on frame 0 (point 7).

use super::World;
use crate::behaviour::Timing;

/// One wall's player. A wall with none is closed at rest (frame 0).
#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct Player {
    /// The base sequence plays reversed: `frame` counts from the last frame.
    reverse: bool,
    frame: u32,
    elapsed: u32,
    /// The frames of the animation, as the last advance saw them (0: not seen yet).
    count: u32,
    /// The run that follows an open or close, not finished yet.
    animating: bool,
}

impl Player {
    /// The frame to draw; `u32::MAX` stands for the last frame while the count is not known.
    fn shown(&self) -> u32 {
        match (self.reverse, self.count) {
            (false, _) => self.frame,
            (true, 0) => u32::MAX,
            (true, n) => n.saturating_sub(1).saturating_sub(self.frame),
        }
    }

    /// `set_portal_anim` (shows 32.6 point 2): the base sequence takes `reverse` and the player is
    /// cleared at once, back to the first step of the base.
    fn base(&mut self, reverse: bool) {
        (self.reverse, self.frame, self.elapsed) = (reverse, 0, 0);
    }

    /// One step of `step` ms (shows 32.6 point 4): a player that animates runs; at the end of the
    /// run it stops and takes the base of the rest picture.
    fn advance(&mut self, timing: Timing, open: bool, step: u32) {
        self.count = timing.count;
        if !self.animating {
            return;
        }
        self.elapsed = self.elapsed.saturating_add(step);
        let Timing { count, delay } = timing;
        let mut ended = delay == 0 || count == 0;
        while !ended && self.elapsed >= delay {
            self.elapsed -= delay;
            ended = self.frame + 1 >= count;
            self.frame = (self.frame + 1).min(count - 1);
        }
        if ended {
            self.animating = false;
            self.base(open);
        }
    }
}

impl World {
    /// `set_open` (shows 32.6 point 3): the wall `id` opens or closes, its player animates with
    /// the base of the new direction, which shows at once.
    pub(crate) fn portal_play(&mut self, id: u32, open: bool) {
        let p = self.portal_players.entry(id).or_default();
        p.animating = true;
        p.base(!open);
    }

    /// The flag setter's reset (point 7): the run stops and the player is cleared on the forward
    /// base, frame 0, whatever the wall showed.
    pub(crate) fn portal_reset(&mut self, id: u32) {
        if let Some(p) = self.portal_players.get_mut(&id) {
            p.animating = false;
            p.base(false);
        }
    }

    /// The loaded portals show the rest picture of their open byte (point 8): an open one rests
    /// on its last frame.
    pub(super) fn portals_at_rest(&mut self) {
        let open = (self.zoo.entities.iter())
            .filter_map(|e| e.common.as_ref())
            .filter(|c| c.fence.is_some_and(|f| f.portal && f.open))
            .map(|c| c.id);
        for id in open.collect::<Vec<_>>() {
            let rest = Player {
                reverse: true,
                ..Player::default()
            };
            self.portal_players.insert(id, rest);
        }
    }

    /// The step of the wall at `at` (point 4): its player advances by `step` ms with the timing of
    /// the type's `portal` animation.
    pub(crate) fn portal_advance(&mut self, at: usize, timing: Option<Timing>, step: u32) {
        let Some(c) = self.zoo.entities[at].common.as_ref() else {
            return;
        };
        let open = c.fence.is_some_and(|f| f.open);
        let timing = timing.unwrap_or(Timing { count: 0, delay: 0 });
        if let Some(p) = self.portal_players.get_mut(&c.id) {
            p.advance(timing, open, step);
        }
    }

    /// Whether any wall has a player.
    pub(crate) fn portal_players_any(&self) -> bool {
        !self.portal_players.is_empty()
    }

    /// The frame each portal wall shows, by the id of the piece.
    pub(crate) fn portal_frames(&self) -> impl Iterator<Item = (u32, u32)> + '_ {
        self.portal_players.iter().map(|(&id, p)| (id, p.shown()))
    }
}
