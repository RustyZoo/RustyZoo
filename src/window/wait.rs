//! The wait at the end of a pass: sleep until the next timer is due instead of spinning.

use rustyzoo::app::{App, Command};
use sdl3::EventPump;

use super::events::State;

/// The most the loop spins at the end of a wait, in milliseconds, however late a wait was.
const MAX_SLACK_MS: f64 = 12.0;

/// How far the waits overshot lately, in milliseconds.
#[derive(Default)]
pub struct Wait {
    slack: f64,
}

impl Wait {
    /// Waits for an event until shortly before a timer is due, then polls until it is: a timed
    /// wait alone overshoots by milliseconds on some systems. The margin follows how far the
    /// waits overshot lately, so a precise system spins for a millisecond only. `start` is the
    /// tick of the first pass.
    pub fn next(
        &mut self,
        pump: &mut EventPump,
        app: &App,
        start: u64,
        state: &mut State,
        commands: &mut Vec<Command>,
    ) {
        let due = app.next_due(sdl3::timer::ticks() - start);
        let early = due
            .saturating_sub(self.slack.ceil() as u64 + 1)
            .min(u64::from(u32::MAX));
        let waited = std::time::Instant::now();
        match pump.wait_event_timeout_ms(early as u32) {
            Some(event) => state.note(event, commands),
            None => {
                // A wait of 0 ms measures nothing.
                if early > 0 {
                    let over = waited.elapsed().as_secs_f64() * 1e3 - early as f64;
                    self.slack = over.clamp(0.0, MAX_SLACK_MS).max(self.slack * 0.99);
                }
                while app.next_due(sdl3::timer::ticks() - start) > 0 {
                    if let Some(event) = pump.poll_event() {
                        state.note(event, commands);
                        break;
                    }
                    std::thread::sleep(std::time::Duration::from_micros(50));
                }
            }
        }
    }
}
