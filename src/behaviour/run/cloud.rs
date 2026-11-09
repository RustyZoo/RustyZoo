//! The cloud of `fDustBall` (behaviours 5.11).

use super::{Animation, Entry, Env, Runner};

impl Runner {
    /// `fDustBall` (behaviours 5.11): the cloud of the unit's tank mode or tile plays on the unit's
    /// own player, a play with the sound of the same key. It is done when the player has taken the
    /// sequence, and the caught prey (`fCaughtPrey`) runs in that same run.
    pub(super) fn dust_ball(&mut self, k: usize, anim: &mut Animation, env: &Env) -> bool {
        if !self.ready {
            if self.playing && anim.queue.is_empty() {
                (self.ready, self.playing) = (true, false);
                (self.chain[k].done, self.meal[0]) = (true, true);
            }
            return true;
        }
        let key = match (self.tank_mode, self.wet) {
            (3, _) => "surfaceball",
            (4, _) => "submergedball",
            (_, true) => "waterball",
            _ => "dustball",
        };
        let timing = |name: &str| env.data.timing(env.unit, name);
        let heard = |k: &&str| env.data.sound(env.unit, k).is_some();
        let sound = [key, "dustball"].into_iter().find(heard);
        // A sound without an animation plays on the manager's empty default animation.
        let name = match [key, "dustball"].into_iter().find(|n| timing(n).is_some()) {
            Some(name) => name,
            None if sound.is_some() => "",
            None => return false,
        };
        let Some(idle) = self.idle(env) else {
            return false;
        };
        let entry = Entry {
            sound: sound.map(Into::into),
            ..Entry::new(name, false)
        };
        let base = anim.shows_base();
        anim.queue(vec![entry], idle);
        if self.fresh && base {
            anim.finish(&timing);
        }
        (self.playing, self.ready) = (true, false);
        true
    }
}
